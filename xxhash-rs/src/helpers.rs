//! Shared low-level helpers for xxHash algorithms.
//!
//! Provides little-endian reads, rotation, and byte-order utilities used by
//! XXH32, XXH64, and later XXH3 implementations. These helpers are derived
//! from the published xxHash specification and BSD-licensed reference material.

/// Reads a little-endian `u32` from the given byte slice starting at `offset`.
///
/// # Panics
///
/// Panics if `offset + 4 > bytes.len()`.
#[inline(always)]
pub fn read_le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

/// Reads a little-endian `u64` from the given byte slice starting at `offset`.
///
/// # Panics
///
/// Panics if `offset + 8 > bytes.len()`.
#[inline(always)]
pub fn read_le_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

/// 32-bit left rotation.
#[inline(always)]
pub fn rotl32(value: u32, amount: u32) -> u32 {
    value.rotate_left(amount)
}

/// 64-bit left rotation.
#[inline(always)]
pub fn rotl64(value: u64, amount: u32) -> u64 {
    value.rotate_left(amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_le_u32_basic() {
        let bytes = [0x01, 0x02, 0x03, 0x04];
        assert_eq!(read_le_u32(&bytes, 0), 0x04030201);
    }

    #[test]
    fn read_le_u32_offset() {
        let bytes = [0x00, 0x01, 0x02, 0x03, 0x04];
        assert_eq!(read_le_u32(&bytes, 1), 0x04030201);
    }

    #[test]
    fn read_le_u64_basic() {
        let bytes = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        assert_eq!(read_le_u64(&bytes, 0), 0x0807060504030201);
    }

    #[test]
    fn rotl32_basic() {
        assert_eq!(rotl32(1, 1), 2);
        assert_eq!(rotl32(0x80000000, 1), 1);
    }

    #[test]
    fn rotl64_basic() {
        assert_eq!(rotl64(1, 1), 2);
        assert_eq!(rotl64(0x8000000000000000, 1), 1);
    }
}
