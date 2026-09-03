// config.odin - Compile-time constants, option structs, and enums.
//
// This file holds the shared types and constants used across the tool:
// default dimensions and names, the output-sprite-map format enum, and the
// lookup tables that map user-facing strings / file extensions to formats.

#+feature dynamic-literals
package main

import "core:strings"

import rp "../dssodinlib/rect_pack"
import u "../dssodinlib/utils"

// Map_Format enumerates the supported sprite-map output formats.
Map_Format :: enum {
	Auto,         // infer from the output file extension
	C,            // plain C: struct + array of {filename, x, y, w, h}
	Structured_C, // C with a generated enum keyed by sprite name
	JSON,         // JSON object keyed by image path
	SSM,          // tab-separated Simple Sprite Map ("S\tpath\tx\ty\tw\th")
	SPR,          // binary sprite bundle keyed by FNV-1a path hash
}

// Spr_Scale_Mode is the GPU texture filtering mode recorded in the .spr
// header's scale_mode byte (Texture_Scale_Mode in the spec).
Spr_Scale_Mode :: enum u8 {
	Nearest   = 0,
	Linear    = 1,
	Pixel_Art = 2,
}

// string_to_map_format maps user-facing CLI values to Map_Format.
// Kept as a package-level immutable map (assignment disallowed outside init).
string_to_map_format := map[string]Map_Format {
	"auto"         = .Auto,
	"c"            = .C,
	"structured-c" = .Structured_C,
	"json"         = .JSON,
	"ssm"          = .SSM,
	"spr"          = .SPR,
}

// ext_to_map_format maps output-file extensions to a Map_Format. Used for
// the Auto format to infer the format from the output path.
ext_to_map_format := map[string]Map_Format {
	".json" = .JSON,
	".ssm"  = .SSM,
	".c"    = .C,
	".spr"  = .SPR,
}

heuristic_from_string_map := map[string]rp.Heuristic {
	"bl" = .Skyline_default,
	"bf" = .Skyline_BF_sortHeight,
}

scale_mode_from_string_map := map[string]Spr_Scale_Mode {
	"nearest"    = .Nearest,
	"linear"     = .Linear,
	"pixel-art"  = .Pixel_Art,
}

// Atlas / sprite size limits and defaults.
Min_Atlas_Width        :: 8   // narrowest supported atlas
Default_Atlas_Width    :: 2048
Max_Atlas_Width        :: 4096 // widest supported atlas
Min_Atlas_Height       :: 8
Default_Atlas_Height   :: 2048
Max_Atlas_Height       :: 4096
Min_Auto_Atlas_Size    :: 64 // smallest side --auto-size tries first
Min_Sprite_Padding     :: 0
Default_Sprite_Padding :: 0   // gap pixels added around each sprite
Max_Sprite_Padding     :: 128

// Output-generation defaults.
Default_Map_Format      :: "auto"
Default_Heuristic       :: "bl"
Default_Scale_Mode      :: "linear"
Default_Map_Enum_Prefix :: "SPRITE_" // prefix for structured-c enum members
Default_Map_Array_Name  :: "sprites" // array name in C / structured-c output
Default_Map_Array_Type  :: "SDL_Rect" // element type in structured-c output

// Pack_Options carries the fully-resolved set of options from the CLI into
// the packing pipeline. It is the internal, typed form of the user's CLI
// arguments (see Options in main.odin for the raw CLI struct).
Pack_Options :: struct {
	dir:             string, // directory to scan recursively for sprites
	atlas_path:      string, // output atlas image path (.png)
	sprite_map_path: string, // output sprite map path
	atlas_width:     int,    // atlas width in pixels (ignored when auto_size is set)
	atlas_height:    int,    // atlas height in pixels (ignored when auto_size is set)
	auto_size:       bool,   // compute the smallest power-of-two atlas that fits all sprites
	spr_root:        string, // project root: spr hashes paths relative to this ("" = use the path as discovered)
	spr_scale_mode:  Spr_Scale_Mode, // filtering mode written to the spr header
	sprite_padding:  int,    // padding between sprites
	heuristic:       rp.Heuristic, // packing heuristic (bl / bf)
	sort:            bool,   // sort sprites largest-first before packing
	map_format:      Map_Format, // sprite map output format
	map_enum_prefix: string, // enum prefix for structured-c
	map_array_name:  string, // array name for c / structured-c
	map_array_type:  string, // element type for structured-c
	no_overwrite:    bool,   // refuse to overwrite existing outputs
	incremental:     bool,   // skip the build when the outputs are newer than all sources
}

// valid_map_format_strings returns the sorted list of accepted CLI strings
// for the --map-format flag, for use in error / help messages.
// Allocated from the temp allocator; no explicit cleanup required.
valid_map_format_strings :: proc() -> []string {
	return u.get_sorted_map_keys(string_to_map_format, context.temp_allocator)
}

// map_format_from_string resolves a CLI format string to a Map_Format.
// Returns ok=false when the string is unknown.
map_format_from_string :: proc(s: string) -> (format: Map_Format, ok: bool) {
	return string_to_map_format[s]
}

// heuristic_from_string resolves a CLI heuristic name to a rect_pack
// Heuristic. Returns ok=false when the name is unknown.
heuristic_from_string :: proc(s: string) -> (h: rp.Heuristic, ok: bool) {
	return heuristic_from_string_map[strings.to_lower(s, context.temp_allocator)]
}

// valid_heuristic_strings returns the sorted list of accepted CLI strings
// for the --heuristic flag, for use in error / help messages.
// Allocated from the temp allocator; no explicit cleanup required.
valid_heuristic_strings :: proc() -> []string {
	return u.get_sorted_map_keys(heuristic_from_string_map, context.temp_allocator)
}

// scale_mode_from_string resolves a CLI scale-mode string to a
// Spr_Scale_Mode. Returns ok=false when the string is unknown.
scale_mode_from_string :: proc(s: string) -> (m: Spr_Scale_Mode, ok: bool) {
	return scale_mode_from_string_map[strings.to_lower(s, context.temp_allocator)]
}

// valid_scale_mode_strings returns the sorted list of accepted CLI strings
// for the --scale-mode flag, for use in error / help messages.
// Allocated from the temp allocator; no explicit cleanup required.
valid_scale_mode_strings :: proc() -> []string {
	return u.get_sorted_map_keys(scale_mode_from_string_map, context.temp_allocator)
}
