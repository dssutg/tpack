// util.rs - Small shared helpers used by the output generators.
//
// These are general-purpose utilities with no packer-specific knowledge:
// C-identifier validation, string escaping for JSON/SSM/C output, and a
// cheap existence check for files.

// is_valid_c_identifier reports whether s is a syntactically valid C
// identifier: an ASCII letter or underscore, followed by letters, digits,
// or underscores. Used to reject dangerous --map-array-name /
// --map-enum-prefix values before emitting C code.
pub fn is_valid_c_identifier(s: &str) -> bool {
    let mut bytes = s.bytes();
    match bytes.next() {
        Some(first) if is_identifier_first(first) => bytes.all(is_identifier_rest),
        _ => false,
    }
}

// is_identifier_first reports whether c can start a C identifier.
#[inline(always)]
fn is_identifier_first(c: u8) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_uppercase() || c == b'_'
}

// is_identifier_rest reports whether c can appear after the first char.
#[inline(always)]
fn is_identifier_rest(c: u8) -> bool {
    is_identifier_first(c) || c.is_ascii_digit()
}

// escape_json_string returns a copy of s with all characters escaped for
// inclusion inside a double-quoted JSON string: quotes, backslashes, control
// characters, and the standard C-like shorthand for \n \r \t.
// Returned as an owned String.
pub fn escape_json_string(s: &str) -> String {
    let mut b = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => b.push_str("\\\""),
            '\\' => b.push_str("\\\\"),
            '\n' => b.push_str("\\n"),
            '\r' => b.push_str("\\r"),
            '\t' => b.push_str("\\t"),
            c => {
                if (c as u32) < 0x20 {
                    // Control char below printable range -> \u00XX
                    let hex = "0123456789abcdef";
                    let b0 = hex.as_bytes()[((c as u32 >> 4) & 0xF) as usize] as char;
                    let b1 = hex.as_bytes()[(c as u32 & 0xF) as usize] as char;
                    b.push_str("\\u00");
                    b.push(b0);
                    b.push(b1);
                } else {
                    b.push(c);
                }
            }
        }
    }
    b
}

// escape_ssm_field returns a copy of s with characters that would break the
// tab-separated SSM record format escaped: tabs, newlines, carriage returns
// (which would inject extra records) as well as backslashes and quotes.
// Returned as an owned String.
pub fn escape_ssm_field(s: &str) -> String {
    let mut b = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\t' => b.push_str("\\t"),
            '\n' => b.push_str("\\n"),
            '\r' => b.push_str("\\r"),
            '\\' => b.push_str("\\\\"),
            '"' => b.push_str("\\\""),
            c => b.push(c),
        }
    }
    b
}
