//! cli_parser is a small, dependency-free command-line argument parser.
//!
//! The library is deliberately *generic*: it contains no knowledge of any
//! particular application's option set. The application supplies a
//! [`Spec`] describing the options it accepts and how many positional
//! arguments it requires; [`parse_args`] tokenizes the command line
//! against that spec and returns an [`Arguments`] value. The application
//! then interprets those values into whatever option struct it uses.
//!
//! # Dialect
//!
//! The parser accepts this command-line grammar:
//!
//! ```text
//! command_line = *( help | option | positional )
//! help         = "-h" | "-help"
//! option       = "--name" [ value ]    // value present iff the option
//!              //                        is declared Value(kind)
//! positional   = any argument that is not a help flag and does not start
//!              //                        with '-' (a lone "-" is positional)
//! ```
//!
//! - Every argument is visited in order and independently, mirroring
//!   classic Unix-style tools: a help flag anywhere on the line is a help
//!   request, even when the rest of the line is garbage.
//! - A [`ArgKind::Flag`] option takes no value. Note the consequence:
//!   `--myflag x` does *not* feed `x` to the flag — `x` becomes a
//!   positional argument.
//! - A value option unconditionally consumes the *next* argument, even
//!   when it looks like an option (`--a --b` gives `--a` the literal
//!   value `--b`), so options and their values never become ambiguous.
//! - Numeric options reject out-of-range values during parsing, so the
//!   error appears while the command line is still being scanned.
//! - A repeated option overwrites the earlier value; the last occurrence
//!   wins.
//! - After scanning, the number of positional arguments is checked
//!   against the spec.
//!
//! # Example
//!
//! Let the application declare two options and three positionals:
//!
//! ```
//! # use cli_parser::{parse_args, CliError, Spec, OptionSpec, ArgKind, ValueKind};
//! const SPEC: &[OptionSpec] = &[
//!     OptionSpec { name: "out-name", kind: ArgKind::Value(ValueKind::Text) },
//!     OptionSpec { name: "jobs", kind: ArgKind::Value(ValueKind::Usize {
//!         min: Some(1), max: None }) },
//!     OptionSpec { name: "verbose", kind: ArgKind::Flag },
//! ];
//! let spec = Spec::new(SPEC, 1, 1, "<input>");
//! let args = ["--verbose", "--jobs", "4", "in.png"].map(String::from);
//! let arguments = parse_args(spec, &args).unwrap();
//! assert!(arguments.flag("verbose"));
//! assert_eq!(arguments.usize_value("jobs"), Some(4));
//! assert_eq!(arguments.positionals, ["in.png"]);
//! ```
//!
//! The crate is generic and dependency-free: nothing in it is specific to
//! any particular tool or project.

/// ValueKind enumerates the kinds of value an option's argument can take.
///
/// The kinds are parsed, not merely tokenized: a numeric kind rejects
/// non-numeric and out-of-range arguments during parsing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    /// Any string; no validation beyond presence.
    Text,
    /// Signed 32-bit integer.
    Integer { min: Option<i32>, max: Option<i32> },
    /// Unsigned 64-bit integer.
    U64 { min: Option<u64>, max: Option<u64> },
    /// Unsigned machine-size (pointer-width) integer, typically a count.
    Usize { min: Option<usize>, max: Option<usize> },
}

/// ArgKind describes the shape of a declared option.
///
/// A [`ArgKind::Flag`] takes no value (`--name` sets it); every other kind
/// consumes the following argument as the option's value and parses it
/// according to the [`ValueKind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    /// Boolean flag: takes no value.
    Flag,
    /// Value option whose argument parses as the given kind.
    Value(ValueKind),
}

/// OptionSpec declares one long option the parser recognizes.
#[derive(Clone, Copy, Debug)]
pub struct OptionSpec<'a> {
    /// The option name as typed by the user, without the leading `--`
    /// (for `--jobs`, `name` is `"jobs"`).
    pub name: &'a str,
    /// The option's shape: flag or value of some kind.
    pub kind: ArgKind,
}

impl<'a> OptionSpec<'a> {
    /// OptionSpec::new builds a spec for a single option.
    pub const fn new(name: &'a str, kind: ArgKind) -> Self {
        OptionSpec { name, kind }
    }
}

/// Spec is the full grammar of a command line the parser accepts.
///
/// It declares the option table, the acceptable range of positional
/// arguments, and the wording used by the too-few-positional-arguments
/// error.
#[derive(Clone, Copy, Debug)]
pub struct Spec<'a> {
    /// Options recognized by the parser.
    ///
    /// Names should be unique; if the same name is declared twice the
    /// later declaration wins. Flag names must not collide with `-h` or
    /// `-help` (those are reserved for the help request and are never
    /// visible as options).
    pub options: &'a [OptionSpec<'a>],
    /// Minimum number of positional arguments accepted.
    pub min_positionals: usize,
    /// Maximum number of positional arguments accepted; `usize::MAX` means
    /// unbounded.
    pub max_positionals: usize,
    /// Short description of the positional arguments, interpolated into
    /// the too-few error (for example `"<input> <output>"`). Leave empty
    /// when nothing more need be said.
    pub positional_usage: &'a str,
}

impl<'a> Spec<'a> {
    /// Spec::new builds a spec with the given options, positional-argument
    /// range, and positional description.
    pub const fn new(
        options: &'a [OptionSpec<'a>],
        min_positionals: usize,
        max_positionals: usize,
        positional_usage: &'a str,
    ) -> Self {
        Spec {
            options,
            min_positionals,
            max_positionals,
            positional_usage,
        }
    }
}

/// Error is the union of all parse failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliError {
    // Help is not a failure in practice: it reports a `-h`/`-help` request.
    // The caller chooses what to do (usually print its usage text).
    Help,
    Message(String),
}

/// Value is one parsed option occurrence: the option's last value, or the
/// mere presence of a flag.
///
/// The variant always matches the option's declared [`ArgKind`], which the
/// parser enforces, so the typed accessors on [`Arguments`] can be trusted
/// for any option present in the spec.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A flag was seen (its value carries no data).
    Flag,
    /// A text option's value.
    Text(String),
    /// A [`ValueKind::Integer`] option's value.
    Integer(i32),
    /// A [`ValueKind::U64`] option's value.
    U64(u64),
    /// A [`ValueKind::Usize`] option's value.
    Usize(usize),
}

/// Arguments is the result of a successful [`parse_args`].
///
/// `options` maps each supplied option name to its last parsed value for
/// fast lookup, while `occurrences` records every option occurrence in
/// command-line order — including repeats of the same name. Callers that
/// must honor the order of the command line (e.g. two aliases for the
/// same setting) iterate `occurrences`; one-shot readers use the typed
/// accessors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arguments {
    /// The option values that were supplied, keyed by option name.
    pub options: std::collections::HashMap<String, Value>,
    /// Every supplied option occurrence, in scan order.
    pub occurrences: Vec<(String, Value)>,
    /// The positional arguments, in command-line order.
    pub positionals: Vec<String>,
}

impl Arguments {
    /// value returns the option's parsed value, or None when the option
    /// was not on the command line.
    pub fn value(&self, name: &str) -> Option<&Value> {
        self.options.get(name)
    }

    /// flag reports whether a [`ArgKind::Flag`] option was seen.
    pub fn flag(&self, name: &str) -> bool {
        matches!(self.value(name), Some(Value::Flag))
    }

    /// text returns a text option's value.
    pub fn text(&self, name: &str) -> Option<&str> {
        match self.value(name) {
            Some(Value::Text(v)) => Some(v),
            _ => None,
        }
    }

    /// integer returns an integer-valued option's parsed 32-bit value.
    pub fn integer(&self, name: &str) -> Option<i32> {
        match self.value(name) {
            Some(Value::Integer(v)) => Some(*v),
            _ => None,
        }
    }

    /// uint64 returns a u64-valued option's parsed value.
    pub fn uint64(&self, name: &str) -> Option<u64> {
        match self.value(name) {
            Some(Value::U64(v)) => Some(*v),
            _ => None,
        }
    }

    /// usize_value returns a usize-valued option's parsed value.
    pub fn usize_value(&self, name: &str) -> Option<usize> {
        match self.value(name) {
            Some(Value::Usize(v)) => Some(*v),
            _ => None,
        }
    }

    /// positional returns the i'th positional argument.
    pub fn positional(&self, i: usize) -> Option<&str> {
        self.positionals.get(i).map(|s| s.as_str())
    }
}

/// parse_args fills arguments from the UNIX-style command line.
///
/// The scanner visits each argument exactly once, so errors surface in
/// command-line order; positional-argument count checks run after the
/// scan (see the crate docs for the full grammar). On success the
/// returned [`Arguments`] maps only the options that were actually
/// supplied; anything the caller does not find there must come from its
/// own defaults.
pub fn parse_args(spec: Spec<'_>, args: &[String]) -> Result<Arguments, CliError> {
    let mut arguments = Arguments::default();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-h" || arg == "-help" {
            // Reserved help request: stop immediately and let the caller
            // decide what to print.
            return Err(CliError::Help);
        }

        if let Some(name) = arg.strip_prefix("--") {
            let option = spec
                .options
                .iter()
                .rev()
                .find(|option| option.name == name);
            let Some(option) = option else {
                // Report the argument as typed, dashes included.
                return Err(CliError::Message(format!("unknown flag: {arg}")));
            };
            match option.kind {
                ArgKind::Flag => {
                    let value = Value::Flag;
                    arguments.options.insert(name.to_string(), value.clone());
                    arguments.occurrences.push((name.to_string(), value));
                }
                ArgKind::Value(kind) => {
                    i += 1;
                    let raw = args
                        .get(i)
                        .ok_or_else(|| CliError::Message(format!("missing value for --{name}")))?;
                    let value = parse_value(&format!("--{name}"), kind, raw)?;
                    arguments.options.insert(name.to_string(), value.clone());
                    arguments.occurrences.push((name.to_string(), value));
                }
            };
        } else if arg.starts_with('-') && arg.len() > 1 {
            // Dash-prefixed but not "--name" and not help: unknown dialect.
            return Err(CliError::Message(format!("unknown flag: {arg}")));
        } else {
            arguments.positionals.push(arg.clone());
        }
        i += 1;
    }

    check_positionals(&arguments.positionals, &spec)?;
    Ok(arguments)
}

// parse_value parses one raw argument for the given value kind. Name is the
// dashed option name, used for error wording.
//
// Range checks happen here — during the scan — rather than after the whole
// line is collected, so a bad value is reported before anything the caller
// might do with later arguments.
fn parse_value(name: &str, kind: ValueKind, raw: &str) -> Result<Value, CliError> {
    let fail = |expected: &str| {
        CliError::Message(format!("invalid value for {name}{expected}: {raw}"))
    };
    let out_of_range = |value: &str, bound: &str, limit: u128| {
        CliError::Message(format!(
            "{name} must be {bound} {limit}, got {value}"
        ))
    };
    match kind {
        ValueKind::Text => Ok(Value::Text(raw.to_string())),
        ValueKind::Integer { min, max } => {
            let value = raw.parse::<i32>().map_err(|_| fail(""))?;
            if let Some(min) = min {
                if value < min {
                    return Err(out_of_range(raw, "at least", min as u128));
                }
            }
            if let Some(max) = max {
                if value > max {
                    return Err(out_of_range(raw, "at most", max as u128));
                }
            }
            Ok(Value::Integer(value))
        }
        ValueKind::U64 { min, max } => {
            let value = raw.parse::<u64>().map_err(|_| fail(""))?;
            if let Some(min) = min {
                if value < min {
                    return Err(out_of_range(raw, "at least", min as u128));
                }
            }
            if let Some(max) = max {
                if value > max {
                    return Err(out_of_range(raw, "at most", max as u128));
                }
            }
            Ok(Value::U64(value))
        }
        ValueKind::Usize { min, max } => {
            let value = raw.parse::<usize>().map_err(|_| fail(""))?;
            if let Some(min) = min {
                if value < min {
                    return Err(out_of_range(raw, "at least", min as u128));
                }
            }
            if let Some(max) = max {
                if value > max {
                    return Err(out_of_range(raw, "at most", max as u128));
                }
            }
            Ok(Value::Usize(value))
        }
    }
}

// check_positionals validates the number of positional arguments against the
// spec. Too many is reported as the first argument past the limit.
fn check_positionals(positionals: &[String], spec: &Spec<'_>) -> Result<(), CliError> {
    if positionals.len() < spec.min_positionals {
        if spec.min_positionals == spec.max_positionals {
            return Err(CliError::Message(format!(
                "expected {} positional arguments: {}",
                spec.min_positionals, spec.positional_usage
            )));
        }
        return Err(CliError::Message(format!(
            "expected at least {} positional arguments: {}",
            spec.min_positionals, spec.positional_usage
        )));
    }
    if positionals.len() > spec.max_positionals {
        return Err(CliError::Message(format!(
            "unexpected argument: {}",
            positionals[spec.max_positionals]
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // specs() builds a spec exercising every value kind plus one flag,
    // requiring exactly `positionals` positional arguments.
    fn spec(positionals: usize) -> Spec<'static> {
        const OPTIONS: &[OptionSpec] = &[
            OptionSpec::new("text-opt", ArgKind::Value(ValueKind::Text)),
            OptionSpec::new(
                "int-opt",
                ArgKind::Value(ValueKind::Integer { min: Some(2), max: Some(10) }),
            ),
            OptionSpec::new("u64-opt", ArgKind::Value(ValueKind::U64 { min: None, max: None })),
            OptionSpec::new(
                "bounded-u64",
                ArgKind::Value(ValueKind::U64 { min: Some(1), max: Some(5) }),
            ),
            OptionSpec::new("usize-opt", ArgKind::Value(ValueKind::Usize { min: None, max: None })),
            OptionSpec::new("bounded", ArgKind::Value(ValueKind::Usize { min: Some(1), max: Some(3) })),
            OptionSpec::new("plain-int", ArgKind::Value(ValueKind::Integer { min: None, max: None })),
            OptionSpec::new("myflag", ArgKind::Flag),
        ];
        Spec::new(OPTIONS, positionals, positionals, "<in> <out>")
    }

    fn parse(positionals: usize, args: &[&str]) -> Result<Arguments, CliError> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_args(spec(positionals), &args)
    }

    #[test]
    fn parses_options_and_positionals() {
        let args = parse_args(
            spec(3),
            &[
                "in".to_string(),
                "out".to_string(),
                "extra".to_string(),
                "--myflag".to_string(),
                "--text-opt".to_string(),
                "hello world".to_string(),
                "--int-opt".to_string(),
                "5".to_string(),
                "--u64-opt".to_string(),
                "18446744073709551615".to_string(),
                "--usize-opt".to_string(),
                "7".to_string(),
            ],
        )
        .unwrap();
        assert!(args.flag("myflag"));
        assert_eq!(args.text("text-opt"), Some("hello world"));
        assert_eq!(args.integer("int-opt"), Some(5));
        assert_eq!(args.uint64("u64-opt"), Some(u64::MAX));
        assert_eq!(args.usize_value("usize-opt"), Some(7));
        assert_eq!(args.positional(0), Some("in"));
        assert_eq!(args.positional(1), Some("out"));
        assert_eq!(args.positional(2), Some("extra"));
        assert_eq!(args.positional(3), None);
        assert_eq!(args.value("myflag"), Some(&Value::Flag));

        // Absent options read as None/absent, not as their zero values,
        // and typed accessors answer None for options of the WRONG kind.
        assert!(!args.flag("plain-int"));
        assert_eq!(args.text("plain-int"), None);
        assert_eq!(args.integer("plain-int"), None);
        assert_eq!(args.uint64("text-opt"), None, "text-opt is Text kind");
        assert_eq!(args.usize_value("text-opt"), None);
        assert_eq!(args.integer("text-opt"), None);
        assert_eq!(args.text("myflag"), None);
        assert_eq!(args.integer("nothing"), None);
        assert_eq!(args.uint64("nothing"), None);
        assert_eq!(args.usize_value("nothing"), None);
        assert_eq!(args.value("nothing"), None);
        assert!(!args.flag("bounded"));
        // a bounded u64 parses inside its range
        let u = parse(0, &["--bounded-u64", "3"]).unwrap();
        assert_eq!(u.uint64("bounded-u64"), Some(3));
    }

    #[test]
    fn last_occurrence_of_an_option_wins() {
        let args = parse(0, &["--int-opt", "3", "--int-opt", "4", "--int-opt", "5"]).unwrap();
        assert_eq!(args.integer("int-opt"), Some(5));

        let args = parse(0, &["--myflag", "--myflag", "--myflag"]).unwrap();
        assert!(args.flag("myflag"));

        // occurrences record every option in scan order, repeats included,
        // so order-sensitive callers (e.g. two aliases of one setting) can
        // replay the command line exactly.
        let args = parse(
            0,
            &["--myflag", "--int-opt", "3", "--u64-opt", "5", "--int-opt", "4"],
        )
        .unwrap();
        assert_eq!(
            args.occurrences,
            vec![
                ("myflag".to_string(), Value::Flag),
                ("int-opt".to_string(), Value::Integer(3)),
                ("u64-opt".to_string(), Value::U64(5)),
                ("int-opt".to_string(), Value::Integer(4)),
            ]
        );
        assert_eq!(args.integer("int-opt"), Some(4));
    }

    #[test]
    fn help_is_reported_as_requested() {
        for help in ["-h", "-help"] {
            let err = parse(0, &[help]).unwrap_err();
            assert_eq!(err, CliError::Help);
            // Anywhere on the line, even after other arguments.
            let err = parse(3, &["a", "b", "c", "--int-opt", "5", help]).unwrap_err();
            assert_eq!(err, CliError::Help);
        }

        // A help token AFTER a value option is not help: the value option
        // consumes the next argument blindly, so it reads as an invalid
        // value for that option.
        assert_eq!(
            parse(0, &["--int-opt", "-h"]).unwrap_err(),
            CliError::Message("invalid value for --int-opt: -h".to_string())
        );
    }

    #[test]
    fn unknown_flags_are_rejected() {
        // Long flags report as typed.
        assert_eq!(
            parse(0, &["--nope"]).unwrap_err(),
            CliError::Message("unknown flag: --nope".to_string())
        );
        // Single-dash flags are not part of this dialect (help is checked
        // before this branch, so -h/-help never land here).
        assert_eq!(
            parse(0, &["-x"]).unwrap_err(),
            CliError::Message("unknown flag: -x".to_string())
        );
        // A bare "--" is not a separator: it is an empty option name.
        assert_eq!(
            parse(0, &["--"]).unwrap_err(),
            CliError::Message("unknown flag: --".to_string())
        );
    }

    #[test]
    fn value_options_consume_the_next_argument_blindly() {
        // The next token is taken as the value even when it looks like an
        // option, and it even fails numeric validation as that raw text.
        assert_eq!(
            parse(0, &["--int-opt", "--myflag"]).unwrap_err(),
            CliError::Message("invalid value for --int-opt: --myflag".to_string())
        );

        // A flag does NOT consume: the following token becomes a positional.
        let args = parse(1, &["--myflag", "positional"]).unwrap();
        assert!(args.flag("myflag"));
        assert_eq!(args.positional(0), Some("positional"));

        // A missing value for the LAST argument is reported per option.
        assert_eq!(
            parse(0, &["--int-opt"]).unwrap_err(),
            CliError::Message("missing value for --int-opt".to_string())
        );
        assert_eq!(
            parse(0, &["--text-opt"]).unwrap_err(),
            CliError::Message("missing value for --text-opt".to_string())
        );
        assert_eq!(
            parse(0, &["--usize-opt"]).unwrap_err(),
            CliError::Message("missing value for --usize-opt".to_string())
        );
        assert_eq!(
            parse(0, &["--u64-opt"]).unwrap_err(),
            CliError::Message("missing value for --u64-opt".to_string())
        );
    }

    #[test]
    fn invalid_values_name_the_option_and_the_offending_text() {
        assert_eq!(
            parse(0, &["--int-opt", "abc"]).unwrap_err(),
            CliError::Message("invalid value for --int-opt: abc".to_string())
        );
        assert_eq!(
            parse(0, &["--u64-opt", "-1"]).unwrap_err(),
            CliError::Message("invalid value for --u64-opt: -1".to_string())
        );
        assert_eq!(
            parse(0, &["--usize-opt", "x"]).unwrap_err(),
            CliError::Message("invalid value for --usize-opt: x".to_string())
        );
        assert_eq!(
            parse(0, &["--plain-int", ""]).unwrap_err(),
            CliError::Message("invalid value for --plain-int: ".to_string())
        );
        // A text option accepts anything, including empty values and
        // values that look like numbers or options.
        let args = parse(0, &["--text-opt", ""]).unwrap();
        assert_eq!(args.text("text-opt"), Some(""));
    }

    #[test]
    fn numeric_ranges_are_enforced_at_parse_time() {
        // Below an explicit minimum.
        assert_eq!(
            parse(0, &["--bounded", "0"]).unwrap_err(),
            CliError::Message("--bounded must be at least 1, got 0".to_string())
        );
        // Above an explicit maximum.
        assert_eq!(
            parse(0, &["--bounded", "4"]).unwrap_err(),
            CliError::Message("--bounded must be at most 3, got 4".to_string())
        );
        // The boundary values themselves parse.
        for good in ["1", "3"] {
            let args = parse(0, &["--bounded", good]).unwrap();
            assert_eq!(args.usize_value("bounded"), Some(good.parse().unwrap()));
        }
        // Options without ranges accept the full type range, including
        // negatives for signed kinds.
        let args = parse(0, &["--plain-int", "-2147483648"]).unwrap();
        assert_eq!(args.integer("plain-int"), Some(i32::MIN));
        // Same minimum wording for integer-sized options.
        assert_eq!(
            parse(0, &["--int-opt", "1"]).unwrap_err(),
            CliError::Message("--int-opt must be at least 2, got 1".to_string())
        );
        assert_eq!(
            parse(0, &["--int-opt", "11"]).unwrap_err(),
            CliError::Message("--int-opt must be at most 10, got 11".to_string())
        );
        // u64 kinds enforce their own bounds with the same wording.
        assert_eq!(
            parse(0, &["--bounded-u64", "0"]).unwrap_err(),
            CliError::Message("--bounded-u64 must be at least 1, got 0".to_string())
        );
        assert_eq!(
            parse(0, &["--bounded-u64", "6"]).unwrap_err(),
            CliError::Message("--bounded-u64 must be at most 5, got 6".to_string())
        );
    }

    #[test]
    fn positional_counts_are_checked_against_the_spec() {
        // Exactly at the required count: fine (checked in other tests).
        // Too few names the usage description.
        assert_eq!(
            parse(3, &[]).unwrap_err(),
            CliError::Message(
                "expected 3 positional arguments: <in> <out>".to_string()
            )
        );
        // One short when several are required.
        assert_eq!(
            parse(3, &["a", "b"]).unwrap_err(),
            CliError::Message(
                "expected 3 positional arguments: <in> <out>".to_string()
            )
        );
        // Beyond the maximum reports the first argument past the limit.
        assert_eq!(
            parse(3, &["a", "b", "c", "extra"]).unwrap_err(),
            CliError::Message("unexpected argument: extra".to_string())
        );
        // Extra arguments before later parse errors: positional count wins
        // only when the scan never fails.
        assert_eq!(
            parse(1, &["a", "b", "--nope"]).unwrap_err(),
            CliError::Message("unknown flag: --nope".to_string())
        );
    }

    #[test]
    fn variable_positional_counts_work() {
        // min < max: too few reads differently than fixed counts.
        let options = [
            OptionSpec::new("myflag", ArgKind::Flag),
            OptionSpec::new(
                "int-opt",
                ArgKind::Value(ValueKind::Integer { min: None, max: None }),
            ),
        ];
        let spec = Spec::new(&options, 1, 2, "<in> [<out>]");
        assert_eq!(
            parse_args(spec, &[]).unwrap_err(),
            CliError::Message(
                "expected at least 1 positional arguments: <in> [<out>]".to_string()
            )
        );
        let args =
            parse_args(spec, &["a".to_string(), "b".to_string()]).unwrap();
        assert_eq!(args.positionals.len(), 2);

        // max == usize::MAX means unbounded.
        let none: [OptionSpec; 0] = [];
        let unbounded = Spec::new(&none, 1, usize::MAX, "<anything...>");
        let many: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        parse_args(unbounded, &many).unwrap();
    }

    #[test]
    fn dash_positional_and_empty_lines() {
        // A lone dash is a positional argument (it is how file-reading
        // tools conventionally mean stdin).
        let args = parse(1, &["-"]).unwrap();
        assert_eq!(args.positional(0), Some("-"));
        // An empty command line with zero required positionals parses to a
        // completely empty Arguments.
        let args = parse(0, &[]).unwrap();
        assert_eq!(args, Arguments::default());
    }

    #[test]
    fn later_spec_declarations_win_over_earlier_ones() {
        let options = [
            OptionSpec::new("dup", ArgKind::Flag),
            OptionSpec::new("dup", ArgKind::Value(ValueKind::Text)),
        ];
        let spec = Spec::new(&options, 0, 0, "");
        // The last declaration wins: a value is expected for "dup".
        assert_eq!(
            parse_args(spec, &["--dup".to_string()]).unwrap_err(),
            CliError::Message("missing value for --dup".to_string())
        );
        let args = parse_args(spec, &["--dup".to_string(), "x".to_string()]).unwrap();
        assert_eq!(args.text("dup"), Some("x"));
        assert!(!args.flag("dup"));
    }

    #[test]
    fn spec_helper_constructs_fully() {
        let spec = spec(2);
        assert_eq!(spec.min_positionals, 2);
        assert_eq!(spec.max_positionals, 2);
        assert_eq!(spec.positional_usage, "<in> <out>");
        assert_eq!(spec.options.len(), 8);
        let via_new =
            Spec::new(spec.options, spec.min_positionals, spec.max_positionals, spec.positional_usage);
        assert_eq!(format!("{spec:?}"), format!("{via_new:?}"));
    }

    #[test]
    fn debug_output_is_derivable() {
        let args = parse(2, &["a", "b"]).unwrap();
        assert!(format!("{args:?}").contains("positionals"));
        assert!(format!("{:?}", CliError::Help).contains("Help"));
        assert!(format!("{:?}", Value::Integer(1)).contains("Integer"));
    }
}