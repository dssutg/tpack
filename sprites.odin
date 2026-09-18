// sprites.odin - Sprite image discovery and loading.
//
// Responsible for recursively walking a directory for image files, decoding
// each into an SDL surface, and managing their lifetimes. The Sprite struct
// combines the raw surface with its packing result (offset + packed flag).

package main

import "core:fmt"
import "core:mem"
import "core:os"
import "core:path/filepath"
import "core:slice"
import "core:strings"

import sdl "vendor:sdl2"
import img "vendor:sdl2/image"

import u "vendor:dss/utils"

// Sprite is a single image that will be packed into the atlas.
//
// offset holds the top-left pixel coordinate of the sprite's content within
// the final atlas (post-padding), and packed records whether it was
// successfully placed. A sprite that did not fit keeps offset zeroed and
// packed = false.
Sprite :: struct {
	offset:     [2]int, // top-left position in the atlas
	image_path: string, // filesystem path the sprite was loaded from
	surface:    ^sdl.Surface, // decoded ABGR8888 pixel data
	packed:     bool, // whether the sprite was placed in the atlas
}

// find_and_read_sprites discovers every file with the given extension under
// path (recursively) and decodes each into a Sprite, returning them as a
// single slice.
//
// All Odin-side memory (the backing slice and each sprite's image_path string)
// is allocated from allocator and released in one shot when the caller does
// virtual.arena_free_all on that arena. The SDL surfaces, which are not
// arena-managed, must be released via free_sprite_surfaces. On error the slice
// is nil and a descriptive error is returned.
find_and_read_sprites :: proc(
	path: string,
	file_ext: string,
	allocator: mem.Allocator,
) -> (
	sprites: []Sprite,
	err: Error,
) {
	filenames, ferr := find_image_files(path, file_ext, allocator)
	if ferr != nil {
		err = fmt.aprintf(
			"cannot read sprite image files: %v",
			os.error_string(ferr),
			allocator = allocator,
		)
		return nil, err
	}

	dyn := make([dynamic]Sprite, 0, len(filenames), allocator)
	for file_path in filenames {
		sprite, rerr := read_sprite_from_file(file_path, allocator)
		if rerr != "" {
			// Free the surfaces of sprites already loaded before aborting;
			// the arena reclaims the Odin-side memory such as image_path.
			free_sprite_surfaces(dyn[:])
			err = fmt.aprintf(
				"cannot read sprite image file %s: %s",
				file_path,
				rerr,
				allocator = allocator,
			)
			return nil, err
		}
		append(&dyn, sprite)
	}

	return dyn[:], nil
}

// find_image_files recursively collects the paths of all files under
// dir_path whose name ends (case-insensitively) with extension. The returned
// slice and each path string are allocated in allocator (typically an arena).
find_image_files :: proc(
	dir_path: string,
	extension: string,
	allocator: mem.Allocator,
) -> (
	paths: []string,
	err: os.Error,
) {
	dyn := make([dynamic]string, 0, 100, allocator)
	werr := walk_image_dir(dir_path, "", extension, &dyn, allocator)
	if werr != nil {
		return nil, werr
	}
	return dyn[:], nil
}

// walk_image_dir is the recursive core of find_image_files. It walks the
// directory rooted at root, appending matching file paths (cloned into
// allocator) to out.
//
// subdirectories are visited depth-first and directory entries are sorted by
// name so the resulting list (and therefore the atlas layout) is
// deterministic across runs.
walk_image_dir :: proc(
	root: string,
	rel: string,
	extension: string,
	out: ^[dynamic]string,
	allocator: mem.Allocator,
) -> (
	err: os.Error,
) {
	open_path := root
	if len(rel) > 0 {
		open_path, _ = filepath.join({root, rel}, context.temp_allocator)
	}

	h, oerr := os.open(open_path)
	if oerr != nil {
		return oerr
	}
	defer os.close(h)

	entries, rerr := os.read_dir(h, 0, context.temp_allocator)
	if rerr != nil {
		return rerr
	}

	// Sort entries so traversal order (and sprite order) is deterministic.
	slice.sort_by(entries, proc(a, b: os.File_Info) -> bool {return a.name < b.name})

	for e in entries {
		child_rel, _ := filepath.join(
			{rel, e.name} if len(rel) > 0 else {e.name},
			context.temp_allocator,
		)

		if e.type == .Directory {
			derr := walk_image_dir(root, child_rel, extension, out, allocator)
			if derr != nil {
				return derr
			}
		} else {
			// Match the extension case-insensitively so ".PNG" works too.
			if u.string_has_suffix_fold(e.name, extension) {
				disp, _ := filepath.join({root, child_rel}, allocator)
				append(out, disp)
			}
		}
	}
	return
}

// read_sprite_from_file decodes a single image file into a Sprite.
//
// The raw surface is immediately converted to the tool's canonical ABGR8888
// format so pixel-copying later is a plain 32-bit copy. The image_path string
// is recorded using allocator (arena-owned); the SDL surface is not and must
// be released by the caller via free_sprite_surfaces.
//
// The error is a plain string ("" on success) so the caller never has to
// decode a non-string error variant.
read_sprite_from_file :: proc(
	path: string,
	allocator: mem.Allocator,
) -> (
	sprite: Sprite,
	err: string,
) {
	raw := img.Load(strings.clone_to_cstring(path, context.temp_allocator))
	if raw == nil {
		return {}, fmt.aprintf("cannot load image %s: %s", path, sdl.GetErrorString(), allocator = allocator)
	}
	defer sdl.FreeSurface(raw)

	surface := sdl.ConvertSurfaceFormat(raw, u32(sdl.PixelFormatEnum.ABGR8888), 0)
	if surface == nil {
		return {}, fmt.aprintf("cannot convert image %s: %s", path, sdl.GetErrorString(), allocator = allocator)
	}
	return Sprite{image_path = strings.clone(path, allocator), surface = surface}, ""
}

// free_sprite_surfaces releases the SDL surface of every sprite.
//
// SDL surfaces are C-library allocations and are not tracked by an Odin arena,
// so they must be freed individually. It must be called exactly once on a slice
// of sprites whose surfaces were allocated. The Odin-side memory backing the
// slice (and each image_path) is reclaimed separately by the arena's free_all.
free_sprite_surfaces :: proc(sprites: []Sprite) {
	for s in sprites {
		sdl.FreeSurface(s.surface)
	}
}
