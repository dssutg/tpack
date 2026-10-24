// config.rs - Compile-time constants, option structs, and enums.
//
// This file holds the shared types and constants used across the tool:
// default dimensions and names, the output-sprite-map format enum, and the
// lookup tables that map user-facing strings / file extensions to formats.

#![allow(non_upper_case_globals)]
#![allow(clippy::upper_case_acronyms)]

use std::thread;

use rect_pack::PackingMode;

// Map_Format enumerates the supported sprite-map output formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Map_Format {
    Auto,         // infer from the output file extension
    C,            // plain C: struct + array of {filename, x, y, w, h}
    Structured_C, // C with a generated enum keyed by sprite name
    JSON,         // JSON object keyed by image path
    SSM,          // tab-separated Simple Sprite Map ("S\tpath\tx\ty\tw\th")
    SPR,          // binary sprite bundle keyed by FNV-1a path hash
}

// Spr_Scale_Mode is the GPU texture filtering mode recorded in the .spr
// header's scale_mode byte (Texture_Scale_Mode in the spec).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spr_Scale_Mode {
    Nearest = 0,
    Linear = 1,
    Pixel_Art = 2,
}

// Atlas / sprite size limits and defaults.
pub const Min_Atlas_Width: i32 = 8; // narrowest supported atlas
pub const Default_Atlas_Width: i32 = 2048;
pub const Max_Atlas_Width: i32 = 4096; // widest supported atlas
pub const Min_Atlas_Height: i32 = 8;
pub const Default_Atlas_Height: i32 = 2048;
pub const Max_Atlas_Height: i32 = 4096;
pub const Min_Auto_Atlas_Size: i32 = 64; // smallest side --auto-size tries first
pub const Min_Sprite_Padding: i32 = 0;
pub const Default_Sprite_Padding: i32 = 0; // gap pixels added around each sprite
pub const Max_Sprite_Padding: i32 = 128;

// Threading: --threads must be at least one; the default is every core the
// OS reports as available for this process.
pub const Min_Threads: usize = 1;

// Output-generation defaults.
pub const Default_Map_Format: &str = "auto";
pub const Default_Scale_Mode: &str = "linear";
pub const Default_Map_Enum_Prefix: &str = "SPRITE_"; // prefix for structured-c enum members
pub const Default_Map_Array_Name: &str = "sprites"; // array name in C / structured-c output
pub const Default_Map_Array_Type: &str = "SDL_Rect"; // element type in structured-c output

// Pack_Options carries the fully-resolved set of options from the CLI into
// the packing pipeline. It is the internal, typed form of the user's CLI
// arguments (see Options in main.rs for the raw CLI struct).
pub struct Pack_Options {
    pub dir: String,                    // directory to scan recursively for sprites
    pub atlas_path: String,             // output atlas image path (.png)
    pub sprite_map_path: String,        // output sprite map path
    pub atlas_width: i32,               // atlas width in pixels (ignored when auto_size is set)
    pub atlas_height: i32,              // atlas height in pixels (ignored when auto_size is set)
    pub auto_size: bool, // compute the smallest power-of-two atlas that fits all sprites
    pub spr_root: String, // project root: spr hashes paths relative to this ("" = use the path as discovered)
    pub spr_scale_mode: Spr_Scale_Mode, // filtering mode written to the spr header
    pub sprite_padding: i32, // padding between sprites
    pub sort: bool,       // sort sprites largest-first before packing
    pub threads: usize,   // worker threads for decoding and compositing
    pub packing_mode: PackingMode, // packing strategy
    pub seed: u64,        // seed for genetic algorithm
    pub genetic_pop: usize,
    pub genetic_gens: usize,
    pub map_format: Map_Format, // sprite map output format
    pub map_enum_prefix: String, // enum prefix for structured-c
    pub map_array_name: String, // array name for c / structured-c
    pub map_array_type: String, // element type for structured-c
    pub no_overwrite: bool, // refuse to overwrite existing outputs
    pub incremental: bool, // skip the build when the outputs are newer than all sources
}

// string_to_map_format maps user-facing CLI values to Map_Format.
pub fn map_format_from_string(s: &str) -> Option<Map_Format> {
    match s {
        "auto" => Some(Map_Format::Auto),
        "c" => Some(Map_Format::C),
        "structured-c" => Some(Map_Format::Structured_C),
        "json" => Some(Map_Format::JSON),
        "ssm" => Some(Map_Format::SSM),
        "spr" => Some(Map_Format::SPR),
        _ => None,
    }
}

// ext_to_map_format maps output-file extensions to a Map_Format. Used for
// the Auto format to infer the format from the output path.
pub fn ext_to_map_format(ext: &str) -> Option<Map_Format> {
    match ext {
        ".json" => Some(Map_Format::JSON),
        ".ssm" => Some(Map_Format::SSM),
        ".c" => Some(Map_Format::C),
        ".spr" => Some(Map_Format::SPR),
        _ => None,
    }
}

// scale_mode_from_string resolves a CLI scale-mode string to a
// Spr_Scale_Mode. Returns None when the string is unknown.
pub fn scale_mode_from_string(s: &str) -> Option<Spr_Scale_Mode> {
    match s.to_lowercase().as_str() {
        "nearest" => Some(Spr_Scale_Mode::Nearest),
        "linear" => Some(Spr_Scale_Mode::Linear),
        "pixel-art" => Some(Spr_Scale_Mode::Pixel_Art),
        _ => None,
    }
}

// packing_mode_from_string resolves CLI string to PackingMode.
pub fn packing_mode_from_string(s: &str) -> Option<PackingMode> {
    match s.to_lowercase().as_str() {
        "skyline" | "skyline-bl" | "bl" | "bottom-left" => Some(PackingMode::Skyline),
        "skyline-bf" | "bf" | "best-fit" => Some(PackingMode::SkylineBF),
        "skyline-fast" | "fast" => Some(PackingMode::SkylineFast),
        "guillotine" => Some(PackingMode::Guillotine),
        "maxrects" | "max-rects" => Some(PackingMode::MaxRects),
        "genetic" => Some(PackingMode::Genetic),
        "best" => Some(PackingMode::Best),
        _ => None,
    }
}

// valid_packing_mode_strings returns valid values for help/errors.
pub fn valid_packing_mode_strings() -> Vec<&'static str> {
    vec!["best", "genetic", "guillotine", "maxrects", "skyline", "skyline-bf", "skyline-fast"]
}

// default_thread_count returns the number of worker threads used when
// --threads is not supplied: all cores available to this process, but never
// fewer than one.
pub fn default_thread_count() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(Min_Threads)
        .max(Min_Threads)
}

// valid_map_format_strings returns the sorted list of accepted CLI strings
// for the --map-format flag, for use in error / help messages.
pub fn valid_map_format_strings() -> Vec<&'static str> {
    let mut keys = vec!["auto", "c", "structured-c", "json", "ssm", "spr"];
    keys.sort();
    keys
}

// valid_scale_mode_strings returns the sorted list of accepted CLI strings
// for the --scale-mode flag, for use in error / help messages.
pub fn valid_scale_mode_strings() -> Vec<&'static str> {
    let mut keys = vec!["nearest", "linear", "pixel-art"];
    keys.sort();
    keys
}
