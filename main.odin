#+feature dynamic-literals
package main

import "core:c"
import "core:flags"
import "core:fmt"
import "core:os"
import "core:path/filepath"
import "core:slice"
import "core:strings"
import "core:unicode"

import rp "../dssodinlib/rect_pack"
import sdl "vendor:sdl2"
import img "vendor:sdl2/image"

Error :: union {
	string,
	Atlas_Too_Small_Error,
}

Options :: struct {
	dir:            string `args:"pos=0,required"`,
	atlas_path:     string `args:"pos=1,required"`,
	sprite_map:     string `args:"pos=2,required"`,
	atlas_width:    int    `args:"name=width"`,
	atlas_height:   int    `args:"name=height"`,
	sprite_padding: int    `args:"name=sprite-padding"`,
	heuristic:      string `args:"name=heuristic"`,
	no_sort:        bool   `args:"name=no-sort"`,
	map_format:     string `args:"name=map-format"`,
	map_enum_prefix: string `args:"name=map-enum-prefix"`,
	map_array_name:  string `args:"name=map-array-name"`,
	map_array_type:  string `args:"name=map-array-type"`,
}

main :: proc() {
	opt := Options {
		atlas_width      = Default_Atlas_Width,
		atlas_height     = Default_Atlas_Height,
		sprite_padding   = Default_Sprite_Padding,
		heuristic        = Default_Heuristic,
		map_format       = Default_Map_Format,
		map_enum_prefix  = Default_Map_Enum_Prefix,
		map_array_name   = Default_Map_Array_Name,
		map_array_type   = Default_Map_Array_Type,
	}

	args := os.args[1:]
	parse_err := flags.parse(&opt, args, .Unix)
	if parse_err != nil {
		fmt.eprintfln("%s: %v", os.args[0], parse_err)
		os.exit(2)
	}

	map_format: Map_Format
	ok: bool
	map_format, ok = map_format_from_string(opt.map_format)
	if !ok {
		valid, _ := strings.join(valid_map_format_strings(), ", ")
		fmt.eprintfln("%s: invalid map format. Valid formats: %s", os.args[0], valid)
		os.exit(2)
	}

	heuristic: rp.Heuristic
	heuristic, ok = heuristic_from_string(opt.heuristic)
	if !ok {
		fmt.eprintfln("%s: invalid heuristic. Valid: bl, bf", os.args[0])
		os.exit(2)
	}

	err := pack_sprites_to_atlas({
		dir             = opt.dir,
		atlas_path      = opt.atlas_path,
		sprite_map_path = opt.sprite_map,
		atlas_width     = opt.atlas_width,
		atlas_height    = opt.atlas_height,
		sprite_padding  = opt.sprite_padding,
		heuristic       = heuristic,
		sort            = !opt.no_sort,
		map_format      = map_format,
		map_enum_prefix = opt.map_enum_prefix,
		map_array_name  = opt.map_array_name,
		map_array_type  = opt.map_array_type,
	})
	if err != nil {
		if too_small, ok := err.(Atlas_Too_Small_Error); ok {
			fmt.eprintfln("%s: failed to pack all sprites", os.args[0])
			for s in too_small.sprites {
				if !s.packed {
					fmt.eprintfln("  FAILED %s [%dx%d]", s.image_path, s.surface.w, s.surface.h)
				}
			}
		} else {
			#partial switch e in err {
			case string:
				fmt.eprintfln("%s: %s", os.args[0], e)
			}
		}

		os.exit(1)
	}
}

ABGR8888 :: (1 << 28) | (6 << 24) | (7 << 20) | (6 << 16) | (32 << 8) | 4

Map_Format :: enum {
	Auto,
	C,
	Structured_C,
	JSON,
	SSM,
}

string_to_map_format := map[string]Map_Format {
	"auto"         = .Auto,
	"c"            = .C,
	"structured-c" = .Structured_C,
	"json"         = .JSON,
	"ssm"          = .SSM,
}

ext_to_map_format := map[string]Map_Format {
	".json" = .JSON,
	".ssm"  = .SSM,
	".c"    = .C,
}

valid_map_format_strings :: proc() -> []string {
	formats := make([dynamic]string, 0, len(string_to_map_format))
	for format in string_to_map_format {
		append(&formats, format)
	}
	slice.sort(formats[:])
	return formats[:]
}

map_format_from_string :: proc(s: string) -> (format: Map_Format, ok: bool) {
	format, ok = string_to_map_format[s]
	return
}

heuristic_from_string :: proc(s: string) -> (h: rp.Heuristic, ok: bool) {
	switch strings.to_lower(s) {
	case "bl":  return .Skyline_default, true
	case "bf":  return .Skyline_BF_sortHeight, true
	}
	return .Skyline_default, false
}

Min_Atlas_Width          := 8
Default_Atlas_Width      := 2048
Max_Atlas_Width          := 4096
Min_Atlas_Height         := 8
Default_Atlas_Height     := 2048
Max_Atlas_Height         := 4096
Min_Sprite_Padding       := 0
Default_Sprite_Padding   := 0
Max_Sprite_Padding       := 128
Default_Map_Format       := "auto"
Default_Heuristic        := "bl"
Default_Map_Enum_Prefix  := "SPRITE_"
Default_Map_Array_Name   := "sprites"
Default_Map_Array_Type   := "SDL_Rect"

Pack_Options :: struct {
	dir:             string,
	atlas_path:      string,
	sprite_map_path: string,
	atlas_width:     int,
	atlas_height:    int,
	sprite_padding:  int,
	heuristic:       rp.Heuristic,
	sort:            bool,
	map_format:      Map_Format,
	map_enum_prefix: string,
	map_array_name:  string,
	map_array_type:  string,
}

Sprite :: struct {
	offset:     [2]int,
	image_path: string,
	surface:    ^sdl.Surface,
	packed:     bool,
}

Atlas_Too_Small_Error :: struct {
	sprites: []Sprite,
}

pack_sprites_to_atlas :: proc(opt: Pack_Options) -> (err: Error) {
	if opt.atlas_width < Min_Atlas_Width || opt.atlas_width > Max_Atlas_Width {
		return fmt.aprintf("invalid atlas width (min: %d, max: %d)", Min_Atlas_Width, Max_Atlas_Width)
	}
	if opt.atlas_height < Min_Atlas_Height || opt.atlas_height > Max_Atlas_Height {
		return fmt.aprintf("invalid atlas height (min: %d, max: %d)", Min_Atlas_Height, Max_Atlas_Height)
	}
	if opt.sprite_padding < Min_Sprite_Padding || opt.sprite_padding > Max_Sprite_Padding {
		return fmt.aprintf("invalid sprite padding (min: %d, max: %d)", Min_Sprite_Padding, Max_Sprite_Padding)
	}

	sprites, ferr := find_and_read_sprites(opt.dir, ".png")
	if ferr != nil {
		return ferr
	}

	packed := pack_sprites_skyline(sprites, opt.atlas_width, opt.atlas_height, opt.sprite_padding, opt.heuristic, opt.sort)
	if packed != len(sprites) {
		return Atlas_Too_Small_Error { sprites = sprites }
	}
	defer delete_sprites(sprites)

	atlas := sdl.CreateRGBSurfaceWithFormat(0, c.int(opt.atlas_width), c.int(opt.atlas_height), 32, ABGR8888)
	if atlas == nil {
		return fmt.aprintf("failed to create atlas surface: %s", sdl.GetErrorString())
	}
	defer sdl.FreeSurface(atlas)

	for &s in sprites {
		if s.packed {
			copy_sprite_to_atlas(s.surface, atlas, s.offset.x, s.offset.y)
		}
	}

	err = save_texture_atlas(opt.atlas_path, atlas)
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
	)
	return
}

delete_sprites :: proc(sprites: []Sprite) {
	for s in sprites {
		sdl.FreeSurface(s.surface)
	}
	delete(sprites)
}

find_and_read_sprites :: proc(path: string, file_ext: string) -> (sprites: []Sprite, err: Error) {
	filenames, ferr := find_image_files(path, file_ext)
	if ferr != nil {
		return nil, fmt.aprintf("cannot read sprite image files: %v", ferr)
	}
	defer delete(filenames)

	dyn := make([dynamic]Sprite, 0, len(filenames))

	for path in filenames {
		sprite, rerr := read_sprite_from_file(path)
		if rerr != nil {
			delete(dyn)
			return nil, fmt.aprintf("cannot read sprite image file %s: %v", path, rerr)
		}
		append(&dyn, sprite)
	}

	return dyn[:], nil
}

find_image_files :: proc(dir_path, extension: string) -> (paths: []string, err: os.Error) {
	dyn := make([dynamic]string, 0, 100)
	if werr := walk_image_dir(dir_path, "", extension, &dyn); werr != nil {
		delete(dyn)
		return nil, werr
	}
	return dyn[:], nil
}

walk_image_dir :: proc(root, rel, extension: string, out: ^[dynamic]string) -> (err: os.Error) {
	open_path := root
	if len(rel) > 0 {
		open_path = strings.concatenate({root, "/", rel}, context.temp_allocator)
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

	sort_to := entries[:]
	slice.sort_by(sort_to, less_file_info)

	for e in sort_to {
		child_rel := strings.concatenate(
			{rel, "/", e.name} if len(rel) > 0 else {e.name},
			context.temp_allocator,
		)

		if e.type == .Directory {
			if derr := walk_image_dir(root, child_rel, extension, out); derr != nil {
				return derr
			}
		} else {
			if strings.has_suffix(strings.to_lower(e.name, context.temp_allocator), extension) {
				disp := strings.concatenate({root, "/", child_rel}, context.temp_allocator)
				append(out, strings.clone(disp))
			}
		}
	}
	return
}

less_file_info :: proc(a, b: os.File_Info) -> bool {
	return a.name < b.name
}

read_sprite_from_file :: proc(path: string) -> (sprite: Sprite, err: Error) {
	raw := img.Load(strings.clone_to_cstring(path, context.temp_allocator))
	if raw == nil {
		return {}, fmt.aprintf("cannot load image %s: %s", path, sdl.GetErrorString())
	}
	defer sdl.FreeSurface(raw)

	surface := sdl.ConvertSurfaceFormat(raw, ABGR8888, 0)
	if surface == nil {
		return {}, fmt.aprintf("cannot convert image %s: %s", path, sdl.GetErrorString())
	}
	return Sprite {
		image_path = strings.clone(path),
		surface    = surface,
	}, nil
}

copy_sprite_to_atlas :: proc(src, dst: ^sdl.Surface, ox, oy: int) {
	src_pixels := ([^]u8)(src.pixels)
	dst_pixels := ([^]u8)(dst.pixels)
	src_pitch := int(src.pitch)
	dst_pitch := int(dst.pitch)
	w := int(src.w)
	h := int(src.h)

	for y in 0 ..< h {
		for x in 0 ..< w {
			si := y * src_pitch + x * 4
			di := (oy + y) * dst_pitch + (ox + x) * 4
			a := src_pixels[si + 3]
			if a == 0 {
				dst_pixels[di + 0] = 0
				dst_pixels[di + 1] = 0
				dst_pixels[di + 2] = 0
				dst_pixels[di + 3] = 0
			} else {
				dst_pixels[di + 0] = src_pixels[si + 0]
				dst_pixels[di + 1] = src_pixels[si + 1]
				dst_pixels[di + 2] = src_pixels[si + 2]
				dst_pixels[di + 3] = a
			}
		}
	}
}

pack_sprites_skyline :: proc(sprites: []Sprite, atlas_width, atlas_height, padding: int, heuristic: rp.Heuristic, should_sort: bool) -> (packed: int) {
	packer: rp.Packer
	rp.init(&packer, atlas_width, atlas_height, len(sprites), heuristic)
	defer rp.destroy(&packer)

	requests := make([]rp.Request, len(sprites))
	defer delete(requests)
	for s, i in sprites {
		requests[i] = rp.Request {
			w = int(s.surface.w) + padding * 2,
			h = int(s.surface.h) + padding * 2,
		}
	}

	results, _ := rp.pack(&packer, requests, padding = 0, should_sort = should_sort)
	defer delete(results)

	for r, i in results {
		s := &sprites[i]
		if r.was_packed {
			s.offset = [2]int{r.x, r.y}
			s.packed = true
			packed += 1
		}
	}
	return
}

save_texture_atlas :: proc(atlas_path: string, atlas: ^sdl.Surface) -> (err: Error) {
	if dir := filepath.dir(atlas_path); dir != "." && len(dir) > 0 {
		os.make_directory_all(dir)
	}

	cpath := strings.clone_to_cstring(atlas_path, context.temp_allocator)
	if img.SavePNG(atlas, cpath) != 0 {
		return fmt.aprintf("error writing texture atlas file: %s", sdl.GetErrorString())
	}
	return
}

image_path_to_enum_name :: proc(name: string) -> string {
	base := filepath.base(name)
	name_without_ext := base[:len(base) - len(filepath.ext(base))]

	clean, _ := strings.replace_all(name_without_ext, "-", " ")
	clean, _ = strings.replace_all(clean, "_", " ")
	clean, _ = strings.replace_all(clean, ".", " ")
	words := strings.fields(clean, context.temp_allocator)

	builder := strings.builder_make(context.temp_allocator)
	for word in words {
		if len(word) > 0 {
			strings.write_rune(&builder, unicode.to_upper(rune(word[0])))
			for i := 1; i < len(word); i += 1 {
				strings.write_rune(&builder, unicode.to_lower(rune(word[i])))
			}
		}
	}
	return strings.to_string(builder)
}

save_atlas_sprite_map :: proc(
	format: Map_Format,
	sprite_map_path: string,
	map_enum_prefix: string,
	map_array_name: string,
	map_array_type: string,
	sprites: []Sprite,
) -> (err: Error) {
	if dir := filepath.dir(sprite_map_path); dir != "." && len(dir) > 0 {
		os.make_directory_all(dir)
	}

	resolved_format := format
	if resolved_format == .Auto {
		ext := strings.to_lower(filepath.ext(sprite_map_path))
		if f, found := ext_to_map_format[ext]; found {
			resolved_format = f
		}
	}

	handle, oerr := os.open(sprite_map_path, os.O_WRONLY | os.O_CREATE | os.O_TRUNC, os.Permissions{.Write_User, .Read_User, .Read_Group, .Read_Other})
	if oerr != nil {
		return fmt.aprintf("error creating sprite map file %s: %v", sprite_map_path, oerr)
	}
	defer os.close(handle)

	switch resolved_format {
	case .JSON:
		os.write_string(handle, "{\n")
		for i in 0 ..< len(sprites) {
			s := &sprites[i]
			fmt.fprintf(handle, "  %q: {{ \"x\": %d, \"y\": %d, \"w\": %d, \"h\": %d }}",
				s.image_path, s.offset.x, s.offset.y, s.surface.w, s.surface.h)
			if i != len(sprites) - 1 {
				os.write_string(handle, ",")
			}
			os.write_string(handle, "\n")
		}
		os.write_string(handle, "}\n")

	case .SSM:
		for s in sprites {
			fmt.fprintf(handle, "S\t%q\t%d\t%d\t%d\t%d\n",
				s.image_path, s.offset.x, s.offset.y, s.surface.w, s.surface.h)
		}

	case .C:
		os.write_string(handle, "/* DO NOT EDIT; generated with tpack */\n\n")
		os.write_string(handle, "const struct {\n")
		os.write_string(handle, "  const char *filename;\n")
		os.write_string(handle, "  int x, y, w, h;\n")
		fmt.fprintf(handle, "}} %s[] = {{\n", map_array_name)
		for s in sprites {
			fmt.fprintf(handle, "  {{ .filename = %q, .x = %d, .y = %d, .w = %d, .h = %d }},\n",
				s.image_path, s.offset.x, s.offset.y, s.surface.w, s.surface.h)
		}
		os.write_string(handle, "};\n")

	case .Structured_C:
		os.write_string(handle, "/* DO NOT EDIT; generated with tpack */\n\n")
		os.write_string(handle, "enum {\n")
		for s in sprites {
			fmt.fprintf(handle, "  %s%s,\n", map_enum_prefix, image_path_to_enum_name(s.image_path))
		}
		os.write_string(handle, "};\n\n")
		fmt.fprintf(handle, "extern const %s %s[];\n\n", map_array_type, map_array_name)
		fmt.fprintf(handle, "#ifdef %s_IMPLEMENTATION\n", map_enum_prefix)
		fmt.fprintf(handle, "const %s %s[] = {{\n", map_array_type, map_array_name)
		for s in sprites {
			fmt.fprintf(handle, " [%s%s] = {{ .x = %d, .y = %d, .w = %d, .h = %d }},\n",
				map_enum_prefix, image_path_to_enum_name(s.image_path),
				s.offset.x, s.offset.y, s.surface.w, s.surface.h)
		}
		os.write_string(handle, "};\n")
		fmt.fprintf(handle, "#endif /* %s_IMPLEMENTATION */\n", map_enum_prefix)

	case .Auto:
		return fmt.aprintf("cannot determine sprite map format from extension of %s", sprite_map_path)

	case:
		return fmt.aprintf("unknown format of sprite map %s", sprite_map_path)
	}
	return
}
