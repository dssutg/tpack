//! tpack_hash provides stable, small hash functions for tpack's data
//! formats (currently the FNV-1a family).
//!
//! The functions here are pure byte-string hashers with no dependencies;
//! their outputs are part of the `spr` sprite-bundle file format, so the
//! algorithms and constants are frozen.

/// fnv32a computes the 32-bit FNV-1a hash of a byte string.
///
/// FNV-1a mixes every byte in with an XOR before the multiply, so the
/// output is deterministic across processes and platforms: the 32-bit
/// offset basis 2166136261 (`0x811c9dc5`) and prime 16777619
/// (`0x01000193`) are frozen constants from the original FNV-1a
/// definition. Hashes of the empty string equal the offset basis.
///
/// Used to key sprite entries in the binary `spr` output by their
/// project-relative image paths, where the hash must match whatever a
/// game runtime computes on its own data files.
///
/// ```
/// # use tpack_hash::fnv32a;
/// assert_eq!(fnv32a(b"hello"), 0x4f9f2cab);
/// assert_eq!(fnv32a(b""), 2166136261);
/// ```
pub fn fnv32a(bytes: &[u8]) -> u32 {
    // Offset basis and prime are frozen FNV-1a constants; do not change
    // them, hashed values appear in released spr files.
    let mut hash = 2_166_136_261u32;
    for &b in bytes {
        hash ^= b as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::fnv32a;

    #[test]
    fn known_vector() {
        // Known FNV-1a vectors from the FNV reference page.
        assert_eq!(fnv32a(b"hello"), 0x4f9f2cab);
        assert_eq!(fnv32a(b"a"), 0xe40c292c);
    }

    #[test]
    fn empty_input_is_the_offset_basis() {
        // With nothing to hash, the state stays at the offset basis.
        assert_eq!(fnv32a(b""), 2166136261);
        assert_eq!(fnv32a(b""), 0x811c9dc5);
    }

    #[test]
    fn every_byte_is_folded_in() {
        // Two single-byte strings must differ, and order matters: hashing
        // is over the whole sequence, not a multiset of bytes.
        assert_ne!(fnv32a(b"a"), fnv32a(b"b"));
        assert_ne!(fnv32a(b"ab"), fnv32a(b"ba"));
        // Byte values 0 and 255 both participate (0 in particular — a
        // hash that zeroed bytes out would collide here with "").
        assert_ne!(fnv32a(&[0u8]), fnv32a(b""));
        assert_ne!(fnv32a(&[255u8]), fnv32a(b""));
        // Prefixes of the empty string... a non-empty prefix differs from
        // its longer form, which checks the per-byte loop actually runs
        // for every byte.
        assert_ne!(fnv32a(b"test"), fnv32a(b"tes"));
    }
}