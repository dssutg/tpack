// cli.rs - tpack's command-line surface.
//
// This module owns everything the tool knows about its own flags: the
// Options struct (raw CLI form with defaults), the flag table handed to
// the generic parser in lib/tpack_cli, translation of parsed arguments
// into Options, post-parse validation, and the usage text.
//
// lib/tpack_cli itself is option-agnostic: it only knows the mechanics of
// parsing --name/value tokens for whatever specs it is handed. Everything
// tpack-specific lives here so the parser library stays general.

use tpack_cli::{ArgKind, Arguments, OptionSpec, Spec, Value, ValueKind};

use crate::config::{
    Default_Atlas_Height, Default_Atlas_Width, Default_Genetic_Gens, Default_Genetic_Pop,
    Default_Map_Array_Name, Default_Map_Array_Type, Default_Map_Enum_Prefix, Default_Map_Format,
    Default_Scale_Mode, Default_Seed, Default_Sprite_Padding, Max_Atlas_Height, Max_Atlas_Width,
    Max_Sprite_Padding, Min_Threads, default_thread_count,
};
use crate::util::is_valid_c_identifier;

// Options is the raw CLI form of the user's arguments, mirroring the flags
// one field per option, with every default prefilled. parse_args fills it
// via fill_options; semantic validation happens in validate.
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

impl Default for Options {
    fn default() -> Self {
        Options {
            dir: String::new(),
            atlas_path: String::new(),
            sprite_map: String::new(),
            // width/height default to 0, meaning "not specified", so that
            // auto-size (the default) is used unless the user supplies
            // explicit dimensions.
            atlas_width: 0,
            atlas_height: 0,
            auto_size: true,
            no_auto_size: false,
            spr_root: String::new(),
            sprite_padding: Default_Sprite_Padding,
            threads: default_thread_count(),
            packing_mode: "skyline".to_string(),
            scale_mode: Default_Scale_Mode.to_string(),
            seed: Default_Seed,
            genetic_pop: Default_Genetic_Pop,
            genetic_gens: Default_Genetic_Gens,
            no_sort: false,
            map_format: Default_Map_Format.to_string(),
            map_enum_prefix: Default_Map_Enum_Prefix.to_string(),
            map_array_name: Default_Map_Array_Name.to_string(),
            map_array_type: Default_Map_Array_Type.to_string(),
            no_overwrite: false,
            incremental: false,
        }
    }
}

// ARGUMENTS declares the options the generic parser recognizes. Value
// bounds are enforced by the parser during the scan: --threads must be at
// least Min_Threads; the other numeric options are parsed loose here and
// range-validated later against their own min/max constants.
const ARGUMENTS: &[OptionSpec] = &[
    OptionSpec::new(
        "width",
        ArgKind::Value(ValueKind::Integer { min: None, max: None }),
    ),
    OptionSpec::new(
        "height",
        ArgKind::Value(ValueKind::Integer { min: None, max: None }),
    ),
    OptionSpec::new("auto-size", ArgKind::Flag),
    OptionSpec::new("no-auto-size", ArgKind::Flag),
    OptionSpec::new(
        "spr-root",
        ArgKind::Value(ValueKind::Text),
    ),
    OptionSpec::new(
        "sprite-padding",
        ArgKind::Value(ValueKind::Integer { min: None, max: None }),
    ),
    OptionSpec::new(
        "threads",
        ArgKind::Value(ValueKind::Usize { min: Some(Min_Threads), max: None }),
    ),
    // "algorithm"/"pack-algorithm" are aliases of "packing-mode".
    OptionSpec::new("packing-mode", ArgKind::Value(ValueKind::Text)),
    OptionSpec::new("algorithm", ArgKind::Value(ValueKind::Text)),
    OptionSpec::new("pack-algorithm", ArgKind::Value(ValueKind::Text)),
    OptionSpec::new("seed", ArgKind::Value(ValueKind::U64 { min: None, max: None })),
    OptionSpec::new("genetic-pop", ArgKind::Value(ValueKind::Usize { min: None, max: None })),
    OptionSpec::new("genetic-gens", ArgKind::Value(ValueKind::Usize { min: None, max: None })),
    OptionSpec::new("scale-mode", ArgKind::Value(ValueKind::Text)),
    OptionSpec::new("no-sort", ArgKind::Flag),
    OptionSpec::new("map-format", ArgKind::Value(ValueKind::Text)),
    OptionSpec::new("map-enum-prefix", ArgKind::Value(ValueKind::Text)),
    OptionSpec::new("map-array-name", ArgKind::Value(ValueKind::Text)),
    OptionSpec::new("map-array-type", ArgKind::Value(ValueKind::Text)),
    OptionSpec::new("no-overwrite", ArgKind::Flag),
    OptionSpec::new("incremental", ArgKind::Flag),
];

// spec describes the whole command line: the option table above plus the
// positional argument requirement and how those arguments are described in
// error messages.
pub fn spec() -> Spec<'static> {
    Spec::new(ARGUMENTS, 3, 3, "<sprites_dir> <atlas_path> <sprite_map_path>")
}

// fill_options copies every supplied argument into options, leaving the
// defaults that were not on the command line untouched.
//
// Occurrences are applied in command-line order so that the last one wins
// when several options alias the same field (packing-mode/algorithm/
// pack-algorithm), or when an option is repeated. Every arm pairs an
// option name with the Value kind its spec declares; the wildcard is only
// reachable from a hand-built Arguments whose kinds disagree with the
// spec, which fill simply ignores.
pub fn fill_options(options: &mut Options, arguments: &Arguments) {
    for (name, value) in &arguments.occurrences {
        match (name.as_str(), value) {
            ("width", Value::Integer(v)) => options.atlas_width = *v,
            ("height", Value::Integer(v)) => options.atlas_height = *v,
            ("auto-size", Value::Flag) => options.auto_size = true,
            ("no-auto-size", Value::Flag) => options.no_auto_size = true,
            ("spr-root", Value::Text(v)) => options.spr_root = v.clone(),
            ("sprite-padding", Value::Integer(v)) => options.sprite_padding = *v,
            ("threads", Value::Usize(v)) => options.threads = *v,
            ("packing-mode" | "algorithm" | "pack-algorithm", Value::Text(v)) => {
                options.packing_mode = v.clone();
            }
            ("seed", Value::U64(v)) => options.seed = *v,
            ("genetic-pop", Value::Usize(v)) => options.genetic_pop = *v,
            ("genetic-gens", Value::Usize(v)) => options.genetic_gens = *v,
            ("scale-mode", Value::Text(v)) => options.scale_mode = v.clone(),
            ("no-sort", Value::Flag) => options.no_sort = true,
            ("map-format", Value::Text(v)) => options.map_format = v.clone(),
            ("map-enum-prefix", Value::Text(v)) => options.map_enum_prefix = v.clone(),
            ("map-array-name", Value::Text(v)) => options.map_array_name = v.clone(),
            ("map-array-type", Value::Text(v)) => options.map_array_type = v.clone(),
            ("no-overwrite", Value::Flag) => options.no_overwrite = true,
            ("incremental", Value::Flag) => options.incremental = true,
            _ => {}
        }
    }

    // The spec guarantees exactly these three, in order.
    options.dir = arguments.positionals[0].clone();
    options.atlas_path = arguments.positionals[1].clone();
    options.sprite_map = arguments.positionals[2].clone();
}

// validate checks the option values the parser could not: whether they are
// identifiers, enum members, and so on is application semantics. Returns
// the first problem as a user-facing message.
pub fn validate(options: &Options) -> Result<(), String> {
    if !is_valid_c_identifier(&options.map_array_name) {
        return Err(format!(
            "--map-array-name must be a valid C identifier, got {:?}",
            options.map_array_name
        ));
    }
    if !is_valid_c_identifier(&options.map_enum_prefix) {
        return Err(format!(
            "--map-enum-prefix must be a valid C identifier, got {:?}",
            options.map_enum_prefix
        ));
    }
    Ok(())
}

// usage prints the command-line help to stderr. Called for -h/-help and
// mirrored in the error output for bad usage.
pub fn usage(program: &str) {
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
    eprintln!("  --seed <n>             Seed for genetic algorithm (default: {Default_Seed})");
    eprintln!("  --genetic-pop <n>      Genetic population size (default: {Default_Genetic_Pop})");
    eprintln!("  --genetic-gens <n>     Genetic generations (default: {Default_Genetic_Gens})");
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