// fnv32a computes the 32-bit FNV-1a hash of a byte string.
pub fn fnv32a(bytes: &[u8]) -> u32 {
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
    fn test_fnv() {
        assert_eq!(fnv32a(b"hello"), 0x4f9f2cab);
    }
}
