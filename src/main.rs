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
mod cli;
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
use cli::Options;
use config::{
    map_format_from_string, packing_mode_from_string, scale_mode_from_string,
    valid_map_format_strings, valid_packing_mode_strings, valid_scale_mode_strings,
    Default_Atlas_Height, Default_Atlas_Width, Pack_Options,
};
use tpack_cli::{CliError, parse_args as cli_parse_args};


// main parses arguments, validates them, runs the packing pipeline, and
// translates any error into a stderr message and non-zero exit code.
fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args.first().cloned().unwrap_or_else(|| "tpack".to_string());
    process::exit(run(&program, &args[1..]));
}

fn run(program: &str, args: &[String]) -> i32 {
    // Parse the UNIX-style command line against the generic parser; the
    // parse result is an option-name/value map that fill_options folds
    // into the defaults of the tool's own Options struct.
    let arguments = match cli_parse_args(cli::spec(), args) {
        Ok(arguments) => arguments,
        Err(e) => {
            // Each error type is turned into a plain, user-facing message.
            // In particular, raw enum values (flag reasons, OS error codes)
            // are never formatted directly, so the user never sees internal
            // constants.
            match e {
                CliError::Help => {
                    cli::usage(program);
                    return 0;
                }
                CliError::Message(message) => {
                    eprintln!("{program}: {message}");
                    return 2;
                }
            }
        }
    };
    let mut opt = Options::default();
    cli::fill_options(&mut opt, &arguments);

    // Validate values that must be valid C identifiers
    if let Err(message) = cli::validate(&opt) {
        eprintln!("{program}: {message}");
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
