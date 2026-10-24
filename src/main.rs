#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

// main.rs - tpack CLI entry point.
//
// Parses command-line arguments, wires them into the packing pipeline
// (see atlas.rs), and reports results / errors on stderr.
//
// The pack cycle owns all of its data through ordinary Rust values; sprite
// surfaces and atlas pixels are released when they go out of scope.
//
// Exit codes:
//   0 - success (or -h/-help)
//   1 - packing or I/O failure
//   2 - invalid command-line arguments

mod atlas;
mod config;
mod incremental;
mod packer;
mod sprite_map;
mod sprites;
mod util;

use std::env;
use std::process;

use atlas::Error;
use config::{
    heuristic_from_string, map_format_from_string, scale_mode_from_string, valid_heuristic_strings,
    valid_map_format_strings, valid_scale_mode_strings, Default_Atlas_Height, Default_Atlas_Width,
    Default_Heuristic, Default_Map_Array_Name, Default_Map_Array_Type, Default_Map_Enum_Prefix,
    Default_Map_Format, Default_Scale_Mode, Default_Sprite_Padding, Max_Atlas_Height,
    Max_Atlas_Width, Max_Sprite_Padding, Pack_Options,
};
use util::is_valid_c_identifier;

// Options mirrors the CLI: named flags and three positional arguments.
// Field names mirror the flags (UNIX style: --name value).
struct Options {
    dir: String,             // sprites directory
    atlas_path: String,      // output atlas PNG
    sprite_map: String,      // output sprite map
    atlas_width: i32,        // atlas width (0 = unset; overridden by auto-size)
    atlas_height: i32,       // atlas height (0 = unset; overridden by auto-size)
    auto_size: bool,         // fit the smallest power-of-two atlas (default)
    no_auto_size: bool,      // use explicit/default dimensions instead
    spr_root: String,        // project root for spr path hashing
    sprite_padding: i32,     // gap between sprites
    heuristic: String,       // bl (bottom-left) or bf (best-fit)
    scale_mode: String,      // nearest, linear, or pixel-art
    no_sort: bool,           // disable size pre-sorting
    map_format: String,      // auto/c/structured-c/json/ssm/spr
    map_enum_prefix: String, // enum prefix (structured-c)
    map_array_name: String,  // array name (c/structured-c)
    map_array_type: String,  // array element type (structured-c)
    no_overwrite: bool,      // refuse to overwrite outputs
    incremental: bool,       // skip rebuild when outputs are current
}

enum CliError {
    Help,
    Message(String),
}

// main parses arguments, validates them, runs the packing pipeline, and
// translates any error into a stderr message and non-zero exit code.
fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args.first().cloned().unwrap_or_else(|| "tpack".to_string());
    process::exit(run(&program, &args[1..]));
}

fn run(program: &str, args: &[String]) -> i32 {
    // Seed defaults; argument parsing overwrites the ones the user supplied.
    // width/height default to 0, meaning "not specified", so that auto-size
    // (the default) is used unless the user supplies explicit dimensions.
    let mut opt = Options {
        dir: String::new(),
        atlas_path: String::new(),
        sprite_map: String::new(),
        atlas_width: 0,
        atlas_height: 0,
        auto_size: true,
        no_auto_size: false,
        spr_root: String::new(),
        sprite_padding: Default_Sprite_Padding,
        heuristic: Default_Heuristic.to_string(),
        scale_mode: Default_Scale_Mode.to_string(),
        no_sort: false,
        map_format: Default_Map_Format.to_string(),
        map_enum_prefix: Default_Map_Enum_Prefix.to_string(),
        map_array_name: Default_Map_Array_Name.to_string(),
        map_array_type: Default_Map_Array_Type.to_string(),
        no_overwrite: false,
        incremental: false,
    };

    if let Err(e) = parse_args(&mut opt, args) {
        // Each error type is turned into a plain, user-facing message. In
        // particular, raw enum values (flag reasons, OS error codes) are never
        // formatted directly, so the user never sees internal constants.
        match e {
            CliError::Help => {
                usage(program);
                return 0;
            }
            CliError::Message(message) => {
                eprintln!("{program}: {message}");
                return 2;
            }
        }
    }

    // Validate values that must be valid C identifiers
    if !is_valid_c_identifier(&opt.map_array_name) {
        eprintln!(
            "{}: --map-array-name must be a valid C identifier, got {:?}",
            program, opt.map_array_name
        );
        return 2;
    }
    if !is_valid_c_identifier(&opt.map_enum_prefix) {
        eprintln!(
            "{}: --map-enum-prefix must be a valid C identifier, got {:?}",
            program, opt.map_enum_prefix
        );
        return 2;
    }

    // Resolve the sprite map format string to an enum
    let map_format = match map_format_from_string(&opt.map_format) {
        Some(f) => f,
        None => {
            let valid = valid_map_format_strings().join(", ");
            eprintln!("{}: invalid map format. Valid formats: {}", program, valid);
            return 2;
        }
    };

    // Resolve the heuristic name to a rect_pack heuristic
    let heuristic = match heuristic_from_string(&opt.heuristic) {
        Some(h) => h,
        None => {
            let valid = valid_heuristic_strings().join(", ");
            eprintln!("{}: invalid heuristic. Valid: {}", program, valid);
            return 2;
        }
    };

    // Resolve the scale-mode name to a Spr_Scale_Mode
    let scale_mode = match scale_mode_from_string(&opt.scale_mode) {
        Some(m) => m,
        None => {
            let valid = valid_scale_mode_strings().join(", ");
            eprintln!("{}: invalid scale-mode. Valid: {}", program, valid);
            return 2;
        }
    };

    // Run the packer
    //
    // Sizing, packing, and output are handled entirely inside
    // pack_sprites_to_atlas; all memory is released when it returns.
    //
    // Resolve sizing.
    //
    // Auto-size is the default. Supplying either --width or --height (or the
    // explicit --no-auto-size flag) switches to explicit dimensions, and
    // explicit dimensions ALWAYS win - even when --auto-size is also given, so
    // "auto-size but honour my width" is not silently misinterpreted. Any
    // dimension left at 0 falls back to its default.
    let explicit_w = opt.atlas_width != 0;
    let explicit_h = opt.atlas_height != 0;
    let use_auto_size = opt.auto_size && !opt.no_auto_size && !(explicit_w || explicit_h);
    let atlas_width = if opt.atlas_width == 0 {
        Default_Atlas_Width
    } else {
        opt.atlas_width
    };
    let atlas_height = if opt.atlas_height == 0 {
        Default_Atlas_Height
    } else {
        opt.atlas_height
    };

    let packed = Pack_Options {
        dir: opt.dir,
        atlas_path: opt.atlas_path,
        sprite_map_path: opt.sprite_map,
        atlas_width,
        atlas_height,
        auto_size: use_auto_size,
        spr_root: opt.spr_root,
        spr_scale_mode: scale_mode,
        sprite_padding: opt.sprite_padding,
        heuristic,
        sort: !opt.no_sort,
        map_format,
        map_enum_prefix: opt.map_enum_prefix,
        map_array_name: opt.map_array_name,
        map_array_type: opt.map_array_type,
        no_overwrite: opt.no_overwrite,
        incremental: opt.incremental,
    };

    match atlas::pack_sprites_to_atlas(&packed) {
        Ok(()) => 0,
        // Translate the typed error into a human-readable message. The match
        // is exhaustive over Error's members, so every variant is turned into
        // text here rather than ever reaching the user as a raw value.
        Err(err) => {
            match err {
                Error::AtlasTooSmall(sprites) => {
                    eprintln!("{}: failed to pack all sprites", program);
                    for s in &sprites {
                        if !s.packed {
                            eprintln!(
                                "  FAILED {} [{}x{}]",
                                s.image_path, s.surface.width, s.surface.height
                            );
                        }
                    }
                }
                Error::OutputExists(path) => {
                    eprintln!(
                        "{}: output file {} already exists (use without --no-overwrite to replace)",
                        program, path
                    );
                }
                Error::Message(message) => {
                    eprintln!("{}: {}", program, message);
                }
            }
            1
        }
    }
}

// parse_args fills opt from the UNIX-style command line. Value flags consume
// the following argument; boolean flags take none. Exactly three positional
// arguments are required.
fn parse_args(opt: &mut Options, args: &[String]) -> Result<(), CliError> {
    let mut positional: Vec<String> = Vec::with_capacity(3);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-h" || arg == "-help" {
            return Err(CliError::Help);
        }

        if let Some(name) = arg.strip_prefix("--") {
            let next_value = |i: &mut usize| -> Result<String, CliError> {
                *i += 1;
                args.get(*i)
                    .cloned()
                    .ok_or_else(|| CliError::Message(format!("missing value for --{name}")))
            };
            let parse_i32 = |v: String| -> Result<i32, CliError> {
                v.parse::<i32>()
                    .map_err(|_| CliError::Message(format!("invalid value for --{name}: {v}")))
            };

            match name {
                "width" => opt.atlas_width = parse_i32(next_value(&mut i)?)?,
                "height" => opt.atlas_height = parse_i32(next_value(&mut i)?)?,
                "auto-size" => opt.auto_size = true,
                "no-auto-size" => opt.no_auto_size = true,
                "spr-root" => opt.spr_root = next_value(&mut i)?,
                "sprite-padding" => opt.sprite_padding = parse_i32(next_value(&mut i)?)?,
                "heuristic" => opt.heuristic = next_value(&mut i)?,
                "scale-mode" => opt.scale_mode = next_value(&mut i)?,
                "no-sort" => opt.no_sort = true,
                "map-format" => opt.map_format = next_value(&mut i)?,
                "map-enum-prefix" => opt.map_enum_prefix = next_value(&mut i)?,
                "map-array-name" => opt.map_array_name = next_value(&mut i)?,
                "map-array-type" => opt.map_array_type = next_value(&mut i)?,
                "no-overwrite" => opt.no_overwrite = true,
                "incremental" => opt.incremental = true,
                _ => {
                    return Err(CliError::Message(format!("unknown flag: {arg}")));
                }
            }
        } else if arg.starts_with('-') && arg.len() > 1 {
            return Err(CliError::Message(format!("unknown flag: {arg}")));
        } else {
            positional.push(arg.clone());
        }
        i += 1;
    }

    if positional.len() < 3 {
        return Err(CliError::Message(
            "expected 3 positional arguments: <sprites_dir> <atlas_path> <sprite_map_path>"
                .to_string(),
        ));
    }
    if positional.len() > 3 {
        return Err(CliError::Message(format!(
            "unexpected argument: {}",
            positional[3]
        )));
    }

    opt.dir = positional[0].clone();
    opt.atlas_path = positional[1].clone();
    opt.sprite_map = positional[2].clone();
    Ok(())
}

// usage prints the command-line help to stderr. Called for -h/-help and
// mirrored in the error output for bad usage.
fn usage(program: &str) {
    eprintln!("Usage: {program} [options] <sprites_dir> <atlas_path> <sprite_map_path>\n");
    eprintln!("Pack PNG sprites into a texture atlas and generate a sprite map.\n");
    eprintln!("Arguments:");
    eprintln!("  sprites_dir      Directory of PNG sprites (recursive)");
    eprintln!("  atlas_path       Output atlas image path (PNG)");
    eprintln!("  sprite_map_path  Output sprite map path\n");
    eprintln!("Options:");
    eprintln!(
        "  --width <n>            Atlas width (default: {Default_Atlas_Width}, max: {Max_Atlas_Width})"
    );
    eprintln!(
        "  --height <n>           Atlas height (default: {Default_Atlas_Height}, max: {Max_Atlas_Height})"
    );
    eprintln!(
        "  --auto-size            Fit the smallest square power-of-two atlas for all sprites"
    );
    eprintln!("                           (default; used unless --width/--height are given)");
    eprintln!("  --no-auto-size         Use explicit/default dimensions instead of auto-size");
    eprintln!("  --spr-root <dir>       Project root: spr hashes paths relative to this");
    eprintln!(
        "  --sprite-padding <n>   Padding between sprites (default: {Default_Sprite_Padding}, max: {Max_Sprite_Padding})"
    );
    eprintln!(
        "  --heuristic <h>        Packing heuristic: bl (bottom-left) or bf (best-fit) (default: {Default_Heuristic})"
    );
    eprintln!(
        "  --scale-mode <m>       Atlas scale mode: nearest, linear, or pixel-art (default: {Default_Scale_Mode})"
    );
    eprintln!("  --no-sort              Don't sort sprites before packing");
    eprintln!(
        "  --map-format <f>       Sprite map format: auto, c, structured-c, json, ssm, spr (default: {Default_Map_Format})"
    );
    eprintln!(
        "  --map-enum-prefix <p>  Enum prefix for structured-c (default: {Default_Map_Enum_Prefix})"
    );
    eprintln!(
        "  --map-array-name <n>   Array name for C output (default: {Default_Map_Array_Name})"
    );
    eprintln!(
        "  --map-array-type <t>   Array type for structured-c (default: {Default_Map_Array_Type})"
    );
    eprintln!("  --no-overwrite         Don't overwrite existing output files");
    eprintln!("  --incremental          Skip the rebuild if the atlas and sprite map");
    eprintln!("                           are newer than every source sprite");
    eprintln!("  -h, -help              Show this help message");
}
