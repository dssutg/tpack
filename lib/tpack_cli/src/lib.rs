// CLI parsing utilities
#[derive(Clone, Debug)]
pub struct Args {
    pub program: String,
    pub argv: Vec<String>,
}

impl Args {
    pub fn from_env() -> Self {
        let mut a: Vec<String> = std::env::args().collect();
        let program = if !a.is_empty() { a.remove(0) } else { "tpack".to_string() };
        Args { program, argv: a }
    }
}

pub fn is_help(s: &str) -> bool {
    s == "-h" || s == "-help"
}
