// sprite_map.odin - Sprite map output generation (C, JSON, SSM, SPR).
//
// save_atlas_sprite_map writes a description of where each sprite landed in
// the atlas, in one of several formats. Each format is deliberately
// self-contained and deterministic.

package main

import "core:fmt"
import "core:hash"
import "core:mem"
import "core:os"
import "core:path/filepath"
import "core:slice"
import "core:strings"
import "core:unicode"
import "core:unicode/utf8"

import u "vendor:dss/utils"

// Generated-file banner. Every format that emits source code starts with a
// comment warning that the file is generated, so it is not hand-edited.
generated_banner :: "/* DO NOT EDIT; generated with tpack */\n\n"

// resolve_map_format turns a possibly-.Auto Map_Format into a concrete one by
// inspecting the output file extension. It is called up front, before anything
// is written, so an unresolvable extension fails without truncating an existing
// output file or leaving a fresh atlas PNG with no map behind.
resolve_map_format :: proc(
	format: Map_Format,
	sprite_map_path: string,
	allocator: mem.Allocator,
) -> (
	resolved: Map_Format,
	err: Error,
) {
	if format != .Auto {
		return format, nil
	}
	ext := strings.to_lower(filepath.ext(sprite_map_path), context.temp_allocator)
	if f, found := ext_to_map_format[ext]; found {
		return f, nil
	}
	return .Auto, fmt.aprintf(
		"cannot determine sprite map format from extension of %s",
		sprite_map_path,
		allocator = allocator,
	)
}

// save_atlas_sprite_map writes the sprite map to sprite_map_path, creating
// the parent directory if needed. format must already be concrete (the caller
// resolves .Auto via resolve_map_format before any file is opened, so an
// unresolvable extension never truncates an existing output). allocator is
// used for any descriptive error strings; the actual map body is streamed to
// disk, so it does not accumulate in memory.
//
// spr_root and spr_scale_mode are used only by the binary .spr writer: the
// former is the project root that sprite path hashes are made relative to,
// and the latter is recorded in the spr header's scale_mode byte.
save_atlas_sprite_map :: proc(
	format: Map_Format,
	sprite_map_path: string,
	map_enum_prefix: string,
	map_array_name: string,
	map_array_type: string,
	sprites: []Sprite,
	spr_root: string,
	spr_scale_mode: Spr_Scale_Mode,
	allocator: mem.Allocator,
) -> (
	err: Error,
) {
	// Create parent directories (ignore "already exists").
	if dir := filepath.dir(sprite_map_path); dir != "." && len(dir) > 0 {
		merr := os.make_directory_all(dir)
		if merr != nil && merr != os.General_Error.Exist {
			return fmt.aprintf(
				"cannot create directory for sprite map: %s",
				os.error_string(merr),
				allocator = allocator,
			)
		}
	}

	// Open (truncating) the output file: 0644 permissions.
	handle, oerr := os.open(
		sprite_map_path,
		os.O_WRONLY | os.O_CREATE | os.O_TRUNC,
		os.Permissions{.Write_User, .Read_User, .Read_Group, .Read_Other},
	)
	if oerr != nil {
		return fmt.aprintf(
			"error creating sprite map file %s: %s",
			sprite_map_path,
			os.error_string(oerr),
			allocator = allocator,
		)
	}
	defer os.close(handle)

	#partial switch format {
	case .JSON:
		write_json_sprite_map(handle, sprites)
	case .SSM:
		write_ssm_sprite_map(handle, sprites)
	case .C:
		write_c_sprite_map(handle, map_array_name, sprites)
	case .Structured_C:
		write_structured_c_sprite_map(
			handle = handle,
			map_enum_prefix = map_enum_prefix,
			map_array_name = map_array_name,
			map_array_type = map_array_type,
			sprites = sprites,
		)
	case .SPR:
		werr := write_spr_sprite_map(handle, sprites, spr_root, spr_scale_mode, allocator)
		if werr != nil {
			return werr
		}
	case .Auto:
		return fmt.aprintf(
			"cannot determine sprite map format from extension of %s",
			sprite_map_path,
			allocator = allocator,
		)
	case:
		return fmt.aprintf(
			"unknown format of sprite map %s",
			sprite_map_path,
			allocator = allocator,
		)

	}
	return
}

// write_json_sprite_map writes an object keyed by sprite image path:
//
//	{
//	  "path/to/a.png": { "x": 0, "y": 0, "w": 16, "h": 16 },
//	  "path/to/b.png": { "x": 16, "y": 0, "w": 32, "h": 32 }
//	}
//
// Keys are JSON-escaped. Valid, parseable JSON is guaranteed for any input.
write_json_sprite_map :: proc(handle: ^os.File, sprites: []Sprite) {
	os.write_string(handle, "{\n")
	for &s, i in sprites {
		escaped := escape_json_string(s.image_path, context.temp_allocator)
		fmt.fprintf(
			handle,
			"  \"%s\": {{ \"x\": %d, \"y\": %d, \"w\": %d, \"h\": %d }}",
			escaped,
			s.offset.x,
			s.offset.y,
			s.surface.w,
			s.surface.h,
		)
		if i != len(sprites) - 1 {
			os.write_string(handle, ",")
		}
		os.write_string(handle, "\n")
	}
	os.write_string(handle, "}\n")
}

// write_ssm_sprite_map writes one tab-separated record per sprite:
//
//	S\t<path>\t<x>\t<y>\t<w>\t<h>
//
// The "S" tag marks a sprite record. Field values are escaped so tabs /
// newlines inside a path cannot corrupt the record framing.
write_ssm_sprite_map :: proc(handle: ^os.File, sprites: []Sprite) {
	for s in sprites {
		escaped := escape_ssm_field(s.image_path, context.temp_allocator)
		fmt.fprintf(
			handle,
			"S\t%s\t%d\t%d\t%d\t%d\n",
			escaped,
			s.offset.x,
			s.offset.y,
			s.surface.w,
			s.surface.h,
		)
	}
}

// write_c_sprite_map writes a plain C source file containing a struct and an
// array of {filename, x, y, w, h} entries:
//
//	const struct { const char *filename; int x, y, w, h; } sprites[] = {
//	  { .filename = "...", .x = 0, .y = 0, .w = 16, .h = 16 },
//	};
write_c_sprite_map :: proc(handle: ^os.File, map_array_name: string, sprites: []Sprite) {
	os.write_string(handle, generated_banner)
	os.write_string(handle, "const struct {\n")
	os.write_string(handle, "  const char *filename;\n")
	os.write_string(handle, "  int x, y, w, h;\n")
	fmt.fprintf(handle, "}} %s[] = {{\n", map_array_name)
	for s in sprites {
		escaped := escape_json_string(s.image_path, context.temp_allocator)
		fmt.fprintf(
			handle,
			"  {{ .filename = \"%s\", .x = %d, .y = %d, .w = %d, .h = %d }},\n",
			escaped,
			s.offset.x,
			s.offset.y,
			s.surface.w,
			s.surface.h,
		)
	}
	os.write_string(handle, "};\n")
}

// write_structured_c_sprite_map writes a C file with a generated enum of
// sprite identifiers plus an optionally-instantiated array indexed by those
// enum values:
//
//	enum { SPRITE_Air, SPRITE_Aux1Btn, ... };
//	extern const SDL_Rect sprites[];
//	#ifdef SPRITE_IMPLEMENTATION
//	const SDL_Rect sprites[] = { [SPRITE_Air] = { .x = ..., }, ... };
//	#endif
//
// The array body is guarded by <prefix>_IMPLEMENTATION so callers include the
// header for declarations and define it once to get the definition.
write_structured_c_sprite_map :: proc(
	handle: ^os.File,
	map_enum_prefix: string,
	map_array_name: string,
	map_array_type: string,
	sprites: []Sprite,
) {
	os.write_string(handle, generated_banner)
	os.write_string(handle, "enum {\n")
	for s in sprites {
		fmt.fprintf(handle, "  %s%s,\n", map_enum_prefix, image_path_to_enum_name(s.image_path))
	}
	os.write_string(handle, "};\n\n")
	fmt.fprintf(handle, "extern const %s %s[];\n\n", map_array_type, map_array_name)
	fmt.fprintf(handle, "#ifdef %s_IMPLEMENTATION\n", map_enum_prefix)
	fmt.fprintf(handle, "const %s %s[] = {{\n", map_array_type, map_array_name)
	for s in sprites {
		fmt.fprintf(
			handle,
			" [%s%s] = {{ .x = %d, .y = %d, .w = %d, .h = %d }},\n",
			map_enum_prefix,
			image_path_to_enum_name(s.image_path),
			s.offset.x,
			s.offset.y,
			s.surface.w,
			s.surface.h,
		)
	}
	os.write_string(handle, "};\n")
	fmt.fprintf(handle, "#endif /* %s_IMPLEMENTATION */\n", map_enum_prefix)
}

// image_path_to_enum_name derives a C identifier from an image filename by
// removing the extension, splitting on '-', '_' and '.', and capitalizing each
// word. Examples:
//
//	"aux1_btn.png"       -> "Aux1Btn"
//	"minimap_mask_round" -> "MinimapMaskRound"
//
// The result is built in the temp allocator (no explicit cleanup required) and
// suitable to append after the user's enum prefix.
image_path_to_enum_name :: proc(name: string) -> string {
	// Rebuild the base name one delimiter-separated word at a time.
	sb := strings.builder_make(context.temp_allocator)

	base_name_it := u.remove_filename_extension(filepath.base(name))
	part_idx := 0
	delimiters :: []string{"/", "-", ".", "_", " "}
	for part in strings.split_multi_iterate(&base_name_it, delimiters) {
		if part == "" {
			// Ignore empty parts left behind by multiple delimiters.
			continue
		}

		// Insert underscore only if not the first part.
		defer part_idx += 1
		if part_idx > 0 {
			strings.write_byte(&sb, '_')
		}

		// Capitalize the first rune and keep the rest as-is.
		first, first_len := utf8.decode_rune_in_string(part)
		strings.write_rune(&sb, unicode.to_upper(first))
		strings.write_string(&sb, part[first_len:])
	}

	return strings.to_string(sb)
}

// .spr binary sprite bundle

// Spr_Magic is the 4-byte "SPRS" magic that begins every .spr file.
Spr_Magic :: [4]byte{'S', 'P', 'R', 'S'}

// Spr_Version is the current .spr format version.
Spr_Version :: u32(1)

// spr_header mirrors the .spr header laid out in docs/spr-format-spec.md.
// The little-endian field types fix the byte layout; Odin's struct packing
// aligns sprite_count to offset 12, leaving 3 implicit padding bytes after
// scale_mode.
spr_header :: struct {
	magic:        [4]byte,
	version:      u32le,
	scale_mode:   Spr_Scale_Mode,
	sprite_count: u32le,
}

// spr_crop holds one 8-byte little-endian content rectangle (no padding).
spr_crop :: struct {
	x, y, w, h: u16le,
}

// spr_entry pairs a sprite's FNV-1a path hash with its crop, for sorting.
spr_entry :: struct {
	hash: u32le,
	crop: spr_crop,
}

// spr_rel_path returns the path string to hash for a sprite. When spr_root is
// non-empty and is a prefix of image_path, the leading root is stripped so the
// hash is project-root-relative (e.g. "assets/units/christina/0001.png").
// The returned substring points into image_path (no allocation).
spr_rel_path :: proc(image_path, spr_root: string) -> string {
	if spr_root != "" && strings.has_prefix(image_path, spr_root) {
		rel := image_path[len(spr_root):]
		if len(rel) > 0 && (rel[0] == '/' || rel[0] == '\\') {
			rel = rel[1:]
		}
		return rel
	}
	return image_path
}

// write_spr_sprite_map writes a binary .spr sprite bundle. The file is the
// in-memory layout of spr_header, followed by a u32le hash per sprite and a
// spr_crop per sprite (see docs/spr-format-spec.md); the little-endian field
// types define the byte layout, so the whole bundle is staged in one buffer
// and written with a single os.write.
//
// Hashes cover the sprite's path relative to spr_root (or the full path when
// no root is given), so the hash is project-wide unique and implies the full
// relative filename. They are emitted in ascending order for binary search.
write_spr_sprite_map :: proc(
	handle: ^os.File,
	sprites: []Sprite,
	spr_root: string,
	scale_mode: Spr_Scale_Mode,
	allocator: mem.Allocator,
) -> Error {
	count := len(sprites)

	// Pair each sprite's hash with its content crop (offset already excludes
	// padding and surface w/h are the unpadded dimensions).
	entries := make([]spr_entry, count, allocator)
	for s, i in sprites {
		rel := spr_rel_path(s.image_path, spr_root)
		entries[i] = spr_entry {
			hash = u32le(hash.fnv32a(transmute([]byte)rel)),
			crop = spr_crop {
				x = u16le(u16(clamp(s.offset.x, 0, 0xFFFF))),
				y = u16le(u16(clamp(s.offset.y, 0, 0xFFFF))),
				w = u16le(u16(clamp(int(s.surface.w), 0, 0xFFFF))),
				h = u16le(u16(clamp(int(s.surface.h), 0, 0xFFFF))),
			},
		}
	}

	// Sort ascending so lookups can use binary search at runtime.
	slice.sort_by(entries, proc(a, b: spr_entry) -> bool {
		return a.hash < b.hash
	})

	// Stage the whole file in memory: header, then hashes, then crops. All
	// Odin-side memory comes from the packing arena and is reclaimed by its
	// free_all, so nothing is deleted here.
	data_len := size_of(spr_header) + count * (size_of(u32le) + size_of(spr_crop))
	data := make([]byte, data_len, allocator)

	// Write the header directly into the buffer, field by field. make() already
	// zeroed every byte, and per-field writes never touch the struct's implicit
	// padding (bytes 9..11, after scale_mode), so it stays zero - no hardcoded
	// byte offsets, and the .spr output is byte-deterministic across runs.
	//
	// (Copying a stack-allocated header struct here instead would carry its
	// uninitialized padding bytes into the file.)
	header := cast(^spr_header)raw_data(data)
	header.magic = Spr_Magic
	header.version = u32le(Spr_Version)
	header.scale_mode = scale_mode
	header.sprite_count = u32le(u32(count))

	hashes := (cast([^]u32le)raw_data(data[size_of(spr_header):]))[:count]
	crops := (cast([^]spr_crop)raw_data(
			data[size_of(spr_header) + count * size_of(u32le):],
		))[:count]
	for e, i in entries {
		hashes[i] = e.hash
		crops[i] = e.crop
	}

	if n, werr := os.write(handle, data); werr != nil {
		return fmt.aprintf(
			"error writing sprite map file: %s",
			os.error_string(werr),
			allocator = allocator,
		)
	} else if n != len(data) {
		return fmt.aprintf(
			"error writing sprite map file: short write (%d of %d bytes)",
			n,
			len(data),
			allocator = allocator,
		)
	}
	return nil
}
