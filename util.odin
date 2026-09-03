// util.odin - Small shared helpers used by the output generators.
//
// These are general-purpose utilities with no packer-specific knowledge:
// C-identifier validation, string escaping for JSON/SSM/C output, and a
// cheap existence check for files.

package main

import "core:slice"
import "core:strings"

// is_valid_c_identifier reports whether s is a syntactically valid C
// identifier: an ASCII letter or underscore, followed by letters, digits,
// or underscores. Used to reject dangerous --map-array-name /
// --map-enum-prefix values before emitting C code.
is_valid_c_identifier :: proc(s: string) -> bool {
	if len(s) == 0 || !is_identifier_first(s[0]) {
		return false
	}
  return slice.all_of_proc(transmute([]byte)s, is_identifier_rest)
}

// is_identifier_first reports whether c can start a C identifier.
is_identifier_first :: #force_inline proc(c: u8) -> bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_'
}

// is_identifier_rest reports whether c can appear after the first char.
is_identifier_rest :: #force_inline proc(c: u8) -> bool {
	return is_identifier_first(c) || (c >= '0' && c <= '9')
}

// escape_json_string returns a copy of s with all characters escaped for
// inclusion inside a double-quoted JSON string: quotes, backslashes, control
// characters, and the standard C-like shorthand for \n \r \t.
// Allocated from the provided allocator (defaults to heap).
escape_json_string :: proc(s: string, allocator := context.allocator) -> string {
	b := strings.builder_make(allocator)
	for c in s {
		switch c {
		case '"':  strings.write_string(&b, "\\\"")
		case '\\': strings.write_string(&b, "\\\\")
		case '\n': strings.write_string(&b, "\\n")
		case '\r': strings.write_string(&b, "\\r")
		case '\t': strings.write_string(&b, "\\t")
		case:
			if c < 0x20 {
				// Control char below printable range -> \u00XX
				hex := "0123456789abcdef"
				strings.write_string(&b, `\u00`)
				strings.write_byte(&b, hex[(c >> 4) & 0xF])
				strings.write_byte(&b, hex[c & 0xF])
			} else {
				strings.write_rune(&b, c)
			}
		}
	}
	return strings.to_string(b)
}

// escape_ssm_field returns a copy of s with characters that would break the
// tab-separated SSM record format escaped: tabs, newlines, carriage returns
// (which would inject extra records) as well as backslashes and quotes.
// Allocated from the provided allocator (defaults to heap).
escape_ssm_field :: proc(s: string, allocator := context.allocator) -> string {
	b := strings.builder_make(allocator)
	for c in s {
		switch c {
		case '\t': strings.write_string(&b, "\\t")
		case '\n': strings.write_string(&b, "\\n")
		case '\r': strings.write_string(&b, "\\r")
		case '\\': strings.write_string(&b, "\\\\")
		case '"':  strings.write_string(&b, "\\\"")
		case:
			strings.write_rune(&b, c)
		}
	}
	return strings.to_string(b)
}
