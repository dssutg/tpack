// main.odin - tpack CLI entry point.
//
// Parses command-line arguments, wires them into the packing pipeline
// (see atlas.odin), and reports results / errors on stderr.
//
// The pack cycle is backed by a single virtual arena created here and released
// with virtual.arena_free_all once packing finishes. The arena allocator is
// threaded through every allocating function as an explicit `allocator`
// parameter; SDL surfaces (not arena-managed) are released via
// free_sprite_surfaces.
//
// Exit codes:
//   0 - success (or -h/-help)
//   1 - packing or I/O failure
//   2 - invalid command-line arguments

#+feature dynamic-literals
package main

import "core:flags"
import "core:fmt"
import "core:mem/virtual"
import "core:os"
import "core:strings"

import rp "../dssodinlib/rect_pack"

// Options mirrors the CLI: named flags and three positional arguments.
// Field tags drive the core:flags parser (UNIX style: --name value).
Options :: struct {
	dir:             string `args:"pos=0,required"`,       // sprites directory
	atlas_path:      string `args:"pos=1,required"`,       // output atlas PNG
	sprite_map:      string `args:"pos=2,required"`,       // output sprite map
	atlas_width:     int    `args:"name=width"`,           // atlas width (0 = unset; overridden by auto-size)
	atlas_height:    int    `args:"name=height"`,          // atlas height (0 = unset; overridden by auto-size)
	auto_size:       bool   `args:"name=auto-size"`,       // fit the smallest power-of-two atlas (default)
	no_auto_size:    bool   `args:"name=no-auto-size"`,    // use explicit/default dimensions instead
	spr_root:        string `args:"name=spr-root"`,        // project root for spr path hashing
	sprite_padding:  int    `args:"name=sprite-padding"`,  // gap between sprites
	heuristic:       string `args:"name=heuristic"`,       // bl (bottom-left) or bf (best-fit)
	scale_mode:      string `args:"name=scale-mode"`,      // nearest, linear, or pixel-art
	no_sort:         bool   `args:"name=no-sort"`,         // disable size pre-sorting
	map_format:      string `args:"name=map-format"`,      // auto/c/structured-c/json/ssm/spr
	map_enum_prefix: string `args:"name=map-enum-prefix"`, // enum prefix (structured-c)
	map_array_name:  string `args:"name=map-array-name"`,  // array name (c/structured-c)
	map_array_type:  string `args:"name=map-array-type"`,  // array element type (structured-c)
	no_overwrite:    bool   `args:"name=no-overwrite"`,    // refuse to overwrite outputs
	incremental:     bool   `args:"name=incremental"`,     // skip rebuild when outputs are current
}

// main parses arguments, validates them, runs the packing pipeline, and
// translates any error into a stderr message and non-zero exit code.
main :: proc() {
	// Seed defaults; flags.parse overwrites the ones the user supplied.
	// width/height default to 0, meaning "not specified", so that auto-size
	// (the default) is used unless the user supplies explicit dimensions.
	opt := Options {
		auto_size       = true,
		sprite_padding  = Default_Sprite_Padding,
		heuristic       = Default_Heuristic,
		scale_mode      = Default_Scale_Mode,
		map_format      = Default_Map_Format,
		map_enum_prefix = Default_Map_Enum_Prefix,
		map_array_name  = Default_Map_Array_Name,
		map_array_type  = Default_Map_Array_Type,
	}

	args := os.args[1:]
	parse_err := flags.parse(&opt, args, .Unix)
	if parse_err != nil {
		// Each error type is turned into a plain, user-facing message. In
		// particular, raw enum values (flag reasons, os error codes) are never
		// formatted directly, so the user never sees internal constants.
		switch e in parse_err {
		case flags.Help_Request:
			usage(os.args[0])
			os.exit(0)
		case flags.Parse_Error:
			fmt.eprintfln("%s: %s", os.args[0], e.message)
		case flags.Validation_Error:
			fmt.eprintfln("%s: %s", os.args[0], e.message)
		case flags.Open_File_Error:
			fmt.eprintfln("%s: cannot open %s: %s", os.args[0], e.filename, os.error_string(e.errno))
		}
		os.exit(2)
	}

	// Validate values that must be valid C identifiers
	if !is_valid_c_identifier(opt.map_array_name) {
		fmt.eprintfln(
			"%s: --map-array-name must be a valid C identifier, got %q",
			os.args[0],
			opt.map_array_name,
		)
		os.exit(2)
	}
	if !is_valid_c_identifier(opt.map_enum_prefix) {
		fmt.eprintfln(
			"%s: --map-enum-prefix must be a valid C identifier, got %q",
			os.args[0],
			opt.map_enum_prefix,
		)
		os.exit(2)
	}

	// Resolve the sprite map format string to an enum
	map_format, map_format_ok := map_format_from_string(opt.map_format)
	if !map_format_ok {
		valid := strings.join(valid_map_format_strings(), ", ")
		fmt.eprintfln("%s: invalid map format. Valid formats: %s", os.args[0], valid)
		os.exit(2)
	}

	// Resolve the heuristic name to a rect_pack enum
	heuristic, heuristic_ok := heuristic_from_string(opt.heuristic)
	if !heuristic_ok {
		valid := strings.join(valid_heuristic_strings(), ", ")
		fmt.eprintfln("%s: invalid heuristic. Valid: %s", os.args[0], valid)
		os.exit(2)
	}

	// Resolve the scale-mode name to a Spr_Scale_Mode
	scale_mode, scale_mode_ok := scale_mode_from_string(opt.scale_mode)
	if !scale_mode_ok {
		valid := strings.join(valid_scale_mode_strings(), ", ")
		fmt.eprintfln("%s: invalid scale-mode. Valid: %s", os.args[0], valid)
		os.exit(2)
	}

	// Run the packer
	//
	// A virtual arena backs every allocation the pack cycle makes (the sprite
	// slice, image paths, request lists, and error strings). It is released in
	// one shot on the success path; on the error path the
	// process exits and the OS reclaims everything. SDL surfaces are not
	// arena-managed and are released explicitly (free_sprite_surfaces).
	arena: virtual.Arena
	if aerr := virtual.arena_init_growing(&arena); aerr != nil {
		fmt.eprintfln("%s: failed to allocate backing arena: %v", os.args[0], aerr)
		os.exit(1)
	}
	defer virtual.arena_destroy(&arena)
	allocator := virtual.arena_allocator(&arena)

	// Resolve sizing
	//
	// Auto-size is the default. Supplying either --width or --height (or the
	// explicit --no-auto-size flag) switches to explicit dimensions; any
	// dimension left at 0 falls back to its default.
	explicit_w := opt.atlas_width != 0
	explicit_h := opt.atlas_height != 0
	use_auto_size := opt.auto_size && !opt.no_auto_size && !(explicit_w || explicit_h)
	atlas_width := opt.atlas_width == 0 ? Default_Atlas_Width : opt.atlas_width
	atlas_height := opt.atlas_height == 0 ? Default_Atlas_Height : opt.atlas_height

	err := pack_sprites_to_atlas({
		dir             = opt.dir,
		atlas_path      = opt.atlas_path,
		sprite_map_path = opt.sprite_map,
		atlas_width     = atlas_width,
		atlas_height    = atlas_height,
		auto_size       = use_auto_size,
		spr_root        = opt.spr_root,
		spr_scale_mode  = scale_mode,
		sprite_padding  = opt.sprite_padding,
		heuristic       = heuristic,
		sort            = !opt.no_sort,
		map_format      = map_format,
		map_enum_prefix = opt.map_enum_prefix,
		map_array_name  = opt.map_array_name,
		map_array_type  = opt.map_array_type,
		no_overwrite    = opt.no_overwrite,
		incremental     = opt.incremental,
	}, allocator)
	if err != nil {
		// Translate the typed error union into a human-readable message. The
		// switch is exhaustive over Error's members, so every variant is turned
		// into text here rather than ever reaching the user as a raw value.
		switch err in err {
		case Atlas_Too_Small_Error:
			fmt.eprintfln("%s: failed to pack all sprites", os.args[0])
			for s in err.sprites {
				if !s.packed {
					fmt.eprintfln("  FAILED %s [%dx%d]", s.image_path, s.surface.w, s.surface.h)
				}
			}
			// Release the SDL surfaces the pipeline handed back on error (the
			// Odin-side slice/strings are arena-owned and reclaimed on exit).
			free_sprite_surfaces(err.sprites)
		case Output_Exists_Error:
			fmt.eprintfln(
				"%s: output file %s already exists (use without --no-overwrite to replace)",
				os.args[0],
				err.path,
			)
		case string:
			fmt.eprintfln("%s: %s", os.args[0], err)
		}
		os.exit(1)
	}
}

// usage prints the command-line help to stderr. Called for -h/-help and
// mirrored in the error output for bad usage.
usage :: proc(program: string) {
	fmt.eprintf("Usage: %s [options] <sprites_dir> <atlas_path> <sprite_map_path>\n\n", program)
	fmt.eprintf("Pack PNG sprites into a texture atlas and generate a sprite map.\n\n")
	fmt.eprintf("Arguments:\n")
	fmt.eprintf("  sprites_dir      Directory of PNG sprites (recursive)\n")
	fmt.eprintf("  atlas_path       Output atlas image path (PNG)\n")
	fmt.eprintf("  sprite_map_path  Output sprite map path\n\n")
	fmt.eprintf("Options:\n")
	fmt.eprintf("  --width <n>            Atlas width (default: %d, max: %d)\n", Default_Atlas_Width, Max_Atlas_Width)
	fmt.eprintf("  --height <n>           Atlas height (default: %d, max: %d)\n", Default_Atlas_Height, Max_Atlas_Height)
	fmt.eprintf("  --auto-size            Fit the smallest square power-of-two atlas for all sprites\n")
	fmt.eprintf("                           (default; used unless --width/--height are given)\n")
	fmt.eprintf("  --no-auto-size         Use explicit/default dimensions instead of auto-size\n")
	fmt.eprintf("  --spr-root <dir>       Project root: spr hashes paths relative to this\n")
	fmt.eprintf("  --sprite-padding <n>   Padding between sprites (default: %d, max: %d)\n", Default_Sprite_Padding, Max_Sprite_Padding)
	fmt.eprintf("  --heuristic <h>        Packing heuristic: bl (bottom-left) or bf (best-fit) (default: %s)\n", Default_Heuristic)
	fmt.eprintf("  --scale-mode <m>       Atlas scale mode: nearest, linear, or pixel-art (default: %s)\n", Default_Scale_Mode)
	fmt.eprintf("  --no-sort              Don't sort sprites before packing\n")
	fmt.eprintf("  --map-format <f>       Sprite map format: auto, c, structured-c, json, ssm, spr (default: %s)\n", Default_Map_Format)
	fmt.eprintf("  --map-enum-prefix <p>  Enum prefix for structured-c (default: %s)\n", Default_Map_Enum_Prefix)
	fmt.eprintf("  --map-array-name <n>   Array name for C output (default: %s)\n", Default_Map_Array_Name)
	fmt.eprintf("  --map-array-type <t>   Array type for structured-c (default: %s)\n", Default_Map_Array_Type)
	fmt.eprintf("  --no-overwrite         Don't overwrite existing output files\n")
	fmt.eprintf("  --incremental          Skip the rebuild if the atlas and sprite map\n")
	fmt.eprintf("                           are newer than every source sprite\n")
	fmt.eprintf("  -h, -help              Show this help message\n")
}
