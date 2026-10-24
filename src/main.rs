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
mod parallel;
mod sprite_map;
mod sprites;
mod util;

use std::env;
use std::process;

use atlas::Error;
use config::{
    default_thread_count, map_format_from_string, packing_mode_from_string,
    scale_mode_from_string, valid_map_format_strings, valid_packing_mode_strings,
    valid_scale_mode_strings, Default_Atlas_Height, Default_Atlas_Width, Default_Map_Array_Name,
    Default_Map_Array_Type, Default_Map_Enum_Prefix, Default_Map_Format, Default_Scale_Mode,
    Default_Sprite_Padding, Max_Atlas_Height, Max_Atlas_Width, Max_Sprite_Padding, Min_Threads,
    Pack_Options,
};
use tpack_cli::{parse_args as cli_parse_args, Options as CliOptions, DEFAULT_GENETIC_GENS, DEFAULT_GENETIC_POP, DEFAULT_SEED};
use util::is_valid_c_identifier;


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
    let mut opt = CliOptions {
        dir: String::new(),
        atlas_path: String::new(),
        sprite_map: String::new(),
        atlas_width: 0,
        atlas_height: 0,
        auto_size: true,
        no_auto_size: false,
        spr_root: String::new(),
        sprite_padding: Default_Sprite_Padding,
        threads: default_thread_count(),
        packing_mode: "skyline".to_string(),
        scale_mode: Default_Scale_Mode.to_string(),
        seed: DEFAULT_SEED,
        genetic_pop: DEFAULT_GENETIC_POP,
        genetic_gens: DEFAULT_GENETIC_GENS,
        no_sort: false,
        map_format: Default_Map_Format.to_string(),
        map_enum_prefix: Default_Map_Enum_Prefix.to_string(),
        map_array_name: Default_Map_Array_Name.to_string(),
        map_array_type: Default_Map_Array_Type.to_string(),
        no_overwrite: false,
        incremental: false,
    };

    if let Err(e) = cli_parse_args(&mut opt, args, Min_Threads) {
        // Each error type is turned into a plain, user-facing message. In
        // particular, raw enum values (flag reasons, OS error codes) are never
        // formatted directly, so the user never sees internal constants.
        match e {
            tpack_cli::CliError::Help => {
                usage(program);
                return 0;
            }
            tpack_cli::CliError::Message(message) => {
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

    let packing_mode = match packing_mode_from_string(&opt.packing_mode) {
        Some(m) => m,
        None => {
            let valid = valid_packing_mode_strings().join(", ");
            eprintln!("{}: invalid packing mode. Valid: {}", program, valid);
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
        sort: !opt.no_sort,
        threads: opt.threads,
        packing_mode,
        seed: opt.seed,
        genetic_pop: opt.genetic_pop,
        genetic_gens: opt.genetic_gens,
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
        "  --threads <n>          Worker threads for decoding/compositing (default: all cores, min: {Min_Threads})"
    );
    eprintln!("  --packing-mode <m>     Packing algorithm: skyline, skyline-bf, skyline-fast, genetic, maxrects, guillotine, best (default: skyline)");
    eprintln!("  --seed <n>             Seed for genetic algorithm (default: 123456789)");
    eprintln!("  --genetic-pop <n>      Genetic population size (default: 40)");
    eprintln!("  --genetic-gens <n>     Genetic generations (default: 60)");
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
