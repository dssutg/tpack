// tpack_cli - Command-line argument parsing for tpack.
//
// Owns `Options` (the raw CLI form mirroring the flags), `parse_args`.
// Callers resolve string-valued fields into the typed
// packing pipeline.

#[derive(Debug)]
pub struct Options {
    pub dir: String,             // sprites directory
    pub atlas_path: String,      // output atlas PNG
    pub sprite_map: String,      // output sprite map
    pub atlas_width: i32,        // atlas width (0 = unset; overridden by auto-size)
    pub atlas_height: i32,       // atlas height (0 = unset; overridden by auto-size)
    pub auto_size: bool,         // fit the smallest power-of-two atlas (default)
    pub no_auto_size: bool,      // use explicit/default dimensions instead
    pub spr_root: String,        // project root for spr path hashing
    pub sprite_padding: i32,     // gap between sprites
    pub threads: usize,          // worker threads for decoding/compositing
    pub packing_mode: String,    // skyline/skyline-bf/skyline-fast/genetic/maxrects/guillotine/best
    pub scale_mode: String,      // nearest, linear, or pixel-art
    pub seed: u64,               // genetic algorithm seed
    pub genetic_pop: usize,      // genetic population size
    pub genetic_gens: usize,     // genetic generations
    pub no_sort: bool,           // disable size pre-sorting
    pub map_format: String,      // auto/c/structured-c/json/ssm/spr
    pub map_enum_prefix: String, // enum prefix (structured-c)
    pub map_array_name: String,  // array name (c/structured-c)
    pub map_array_type: String,  // array element type (structured-c)
    pub no_overwrite: bool,      // refuse to overwrite outputs
    pub incremental: bool,       // skip rebuild when outputs are current
}

pub const DEFAULT_SEED: u64 = 123456789;
pub const DEFAULT_GENETIC_POP: usize = 40;
pub const DEFAULT_GENETIC_GENS: usize = 60;

#[derive(Debug)]
pub enum CliError {
    Help,
    Message(String),
}

// parse_args fills opt from the UNIX-style command line. Value flags consume
// the following argument; boolean flags take none. Exactly three positional
// arguments are required. `min_threads` is the smallest valid --threads value.
pub fn parse_args(options: &mut Options, args: &[String], min_threads: usize) -> Result<(), CliError> {
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
            let parse_u64 = |v: String| -> Result<u64, CliError> {
                v.parse::<u64>()
                    .map_err(|_| CliError::Message(format!("invalid value for --{name}: {v}")))
            };
            let parse_usize = |v: String| -> Result<usize, CliError> {
                v.parse::<usize>()
                    .map_err(|_| CliError::Message(format!("invalid value for --{name}: {v}")))
            };

            match name {
                "width" => options.atlas_width = parse_i32(next_value(&mut i)?)?,
                "height" => options.atlas_height = parse_i32(next_value(&mut i)?)?,
                "auto-size" => options.auto_size = true,
                "no-auto-size" => options.no_auto_size = true,
                "spr-root" => options.spr_root = next_value(&mut i)?,
                "sprite-padding" => options.sprite_padding = parse_i32(next_value(&mut i)?)?,
                "threads" => {
                    let value = next_value(&mut i)?;
                    let threads = parse_usize(value)?;
                    if threads < min_threads {
                        return Err(CliError::Message(format!(
                            "--threads must be at least {min_threads}, got {threads}"
                        )));
                    }
                    options.threads = threads;
                }
                "packing-mode" | "algorithm" | "pack-algorithm" => {
                    options.packing_mode = next_value(&mut i)?;
                }
                "seed" => options.seed = parse_u64(next_value(&mut i)?)?,
                "genetic-pop" => options.genetic_pop = parse_usize(next_value(&mut i)?)?,
                "genetic-gens" => options.genetic_gens = parse_usize(next_value(&mut i)?)?,
                "scale-mode" => options.scale_mode = next_value(&mut i)?,
                "no-sort" => options.no_sort = true,
                "map-format" => options.map_format = next_value(&mut i)?,
                "map-enum-prefix" => options.map_enum_prefix = next_value(&mut i)?,
                "map-array-name" => options.map_array_name = next_value(&mut i)?,
                "map-array-type" => options.map_array_type = next_value(&mut i)?,
                "no-overwrite" => options.no_overwrite = true,
                "incremental" => options.incremental = true,
                _ => return Err(CliError::Message(format!("unknown flag: {arg}"))),
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

    options.dir = positional[0].clone();
    options.atlas_path = positional[1].clone();
    options.sprite_map = positional[2].clone();
    Ok(())
}
