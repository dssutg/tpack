// atlas.odin - Top-level packing orchestration and atlas PNG output.
//
// pack_sprites_to_atlas is the main entry point of the tool: it validates
// options, discovers/loads sprites, packs them into the bin, composites the
// atlas image, and writes both the PNG and the sprite map.

package main

import "core:fmt"
import "core:mem"
import "core:os"
import "core:path/filepath"
import "core:strings"

import img "vendor:sdl2/image"
import sdl "vendor:sdl2"

// Error is the union of all errors the packing pipeline can return.
Error :: union {
	string,                // general descriptive error
	Atlas_Too_Small_Error, // some sprites did not fit in the bin
	Output_Exists_Error,   // --no-overwrite and the output already exists
}

// Output_Exists_Error reports that an output file already exists while
// --no-overwrite was requested.
Output_Exists_Error :: struct {
	path: string, // the existing file that blocked the write
}

// Atlas_Too_Small_Error reports that the requested atlas was too small to
// hold every sprite. It carries the sprite slice so the caller can report
// which sprites failed. The slice's Odin-side memory is arena-owned (released
// by the packing arena's free_all); the SDL surfaces must be released by the
// caller via free_sprite_surfaces.
Atlas_Too_Small_Error :: struct {
	sprites: []Sprite,
}

// pack_sprites_to_atlas is the orchestrator for the whole packing run.
//
// It validates the numeric options, optionally refuses to overwrite existing
// outputs, reads all sprites, packs them with the skyline algorithm, blits
// them into an atlas surface, and finally writes the atlas PNG and the
// sprite map file.
//
// Progress is reported to stderr so stdout stays clean for scripting.
//
// allocator is the memory backing the entire pack cycle (a virtual arena
// owned by the caller). All Odin-side allocations — the sprite slice, image
// paths, request lists, and descriptive error strings — are made from it and
// reclaimed in one shot when the caller does virtual.arena_free_all after
// this returns. SDL surfaces are manually released here (free_sprite_surfaces)
// or by the caller in the Atlas_Too_Small_Error branch, since arenas do not
// track C-library allocations.
pack_sprites_to_atlas :: proc(
	opt:       Pack_Options,
	allocator: mem.Allocator,
) -> (err: Error) {
	// Validate sprite padding
	if opt.sprite_padding < Min_Sprite_Padding ||
	   opt.sprite_padding > Max_Sprite_Padding {
		return fmt.aprintf(
			"invalid sprite padding (min: %d, max: %d)",
			Min_Sprite_Padding,
			Max_Sprite_Padding,
			allocator = allocator,
		)
	}

	if !opt.auto_size {
		// Validate explicit atlas dimensions
		// (skipped when --auto-size is set: width/height are ignored there.)
		if opt.atlas_width < Min_Atlas_Width ||
		   opt.atlas_width > Max_Atlas_Width {
			return fmt.aprintf(
				"invalid atlas width (min: %d, max: %d)",
				Min_Atlas_Width,
				Max_Atlas_Width,
				allocator = allocator,
			)
		}
		if opt.atlas_height < Min_Atlas_Height ||
		   opt.atlas_height > Max_Atlas_Height {
			return fmt.aprintf(
				"invalid atlas height (min: %d, max: %d)",
				Min_Atlas_Height,
				Max_Atlas_Height,
				allocator = allocator,
			)
		}
	}

	// Skip the build when the outputs are already current (incremental)
	//
	// Config (atlas size, padding, map format, ...) is assumed to not change
	// between runs; only source modifications are considered. Both the atlas
	// PNG and the sprite map must exist and be newer than every source sprite,
	// otherwise the atlas is regenerated. This runs before the --no-overwrite
	// check: an up-to-date set of outputs is a success, not an overwrite.
	if opt.incremental {
		current, cerr := outputs_are_current(opt.dir, opt.atlas_path, opt.sprite_map_path)
		if cerr != nil {
			return cerr
		}
		if current {
			fmt.eprintf("%s and %s are up to date\n", opt.atlas_path, opt.sprite_map_path)
			return nil
		}
		fmt.eprintf("sources changed, rebuilding\n")
	}

	// Refuse to clobber existing outputs when requested
	if opt.no_overwrite {
		if os.exists(opt.atlas_path) {
			return Output_Exists_Error { path = opt.atlas_path }
		}
		if os.exists(opt.sprite_map_path) {
			return Output_Exists_Error { path = opt.sprite_map_path }
		}
	}

	// Load all sprites
	sprites, ferr := find_and_read_sprites(opt.dir, ".png", allocator)
	if ferr != nil {
		return ferr
	}
	fmt.eprintf("Found %d sprites\n", len(sprites))

	// Resolve the atlas dimensions (explicit or auto-sized)
	width, height := opt.atlas_width, opt.atlas_height
	if opt.auto_size {
		// Probe increasing square power-of-two sizes until everything fits.
		// Each probe overwrites the previous offsets; the last probe leaves the
		// final layout in the sprites. On failure the last probe used the
		// largest permitted size, so the sprites reflect which ones failed.
		size := Min_Auto_Atlas_Size
		for {
			packed := pack_sprites_skyline(
				sprites,
				size, size,
				opt.sprite_padding,
				opt.heuristic,
				opt.sort,
				allocator,
			)
			if packed == len(sprites) {
				break
			}
			if size >= Max_Atlas_Width {
				// Reached the largest allowed size without fitting everything.
				return Atlas_Too_Small_Error { sprites = sprites }
			}
			size *= 2
		}
		width, height = size, size
	} else {
		packed := pack_sprites_skyline(
			sprites,
			width, height,
			opt.sprite_padding,
			opt.heuristic,
			opt.sort,
			allocator,
		)
		if packed != len(sprites) {
			// Not everything fit. The sprite slice's Odin memory stays arena
			// owned; the caller releases the SDL surfaces and reports failures.
			return Atlas_Too_Small_Error { sprites = sprites }
		}
	}
	defer free_sprite_surfaces(sprites)

	fmt.eprintf("Packed %d sprites into %dx%d atlas\n", len(sprites), width, height)

	// Composite the atlas image
	atlas, cerr := create_atlas_surface(width, height)
	if cerr != nil {
		return cerr
	}
	defer sdl.FreeSurface(atlas)

	for &s in sprites {
		copy_sprite_to_atlas(s.surface, atlas, s.offset.x, s.offset.y)
	}

	// Write outputs
	err = save_texture_atlas(opt.atlas_path, atlas, allocator)
	if err != nil {
		return
	}

	err = save_atlas_sprite_map(
		opt.map_format,
		opt.sprite_map_path,
		opt.map_enum_prefix,
		opt.map_array_name,
		opt.map_array_type,
		sprites,
		opt.spr_root,
		opt.spr_scale_mode,
		allocator,
	)
	if err == nil {
		fmt.eprintf("Saved %s and %s\n", opt.atlas_path, opt.sprite_map_path)
	}
	return
}

// save_texture_atlas encodes the atlas surface to a PNG and writes it to
// atlas_path, creating any missing parent directories.
save_texture_atlas :: proc(
	atlas_path: string,
	atlas:      ^sdl.Surface,
	allocator:  mem.Allocator,
) -> (err: Error) {
	// Create parent directories (ignore "already exists").
	if dir := filepath.dir(atlas_path); dir != "." && len(dir) > 0 {
		merr := os.make_directory_all(dir)
		if merr != nil && merr != os.General_Error.Exist {
			return fmt.aprintf(
				"cannot create directory for atlas: %s",
				os.error_string(merr),
				allocator = allocator,
			)
		}
	}

	cpath := strings.clone_to_cstring(atlas_path, context.temp_allocator)
	if img.SavePNG(atlas, cpath) != 0 {
		return fmt.aprintf(
			"error writing texture atlas file: %s",
			sdl.GetErrorString(),
			allocator = allocator,
		)
	}
	return
}
