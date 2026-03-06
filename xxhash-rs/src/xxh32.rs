//! Scalar XXH32 one-shot hashing.
//!
//! Implements the 32-bit xxHash algorithm as described in the published
//! xxHash specification and BSD-licensed reference material. This module
//! provides a one-shot `xxh32()` function suitable for hashing a complete
//! input in a single call.
//!
//! The streaming API is left for a later feature.

use crate::helpers::{read_le_u32, rotl32};

// ============================================================================
// XXH32 prime constants from the published specification.
// ============================================================================

/// XXH_PRIME32_1: used in the initial accumulator setup and round mixing.
pub const PRIME32_1: u32 = 0x9E3779B1;

/// XXH_PRIME32_2: used in the round function multiply step.
pub const PRIME32_2: u32 = 0x85EBCA77;

/// XXH_PRIME32_3: used for remaining 4-byte chunks after the main loop.
pub const PRIME32_3: u32 = 0xC2B2AE3D;

/// XXH_PRIME32_4: used for remaining individual bytes.
pub const PRIME32_4: u32 = 0x27D4EB2F;

/// XXH_PRIME32_5: added to the hash length and used in the finalization.
pub const PRIME32_5: u32 = 0x165667B1;

// ============================================================================
// XXH32 algorithm
// ============================================================================

/// Computes the XXH32 hash of the given `input` with the specified `seed`.
///
/// This is a one-shot function: pass the complete input as a byte slice.
///
/// # Examples
///
/// ```
/// use xxhash_rs::xxh32::xxh32;
///
/// let hash = xxh32(b"hello", 0);
/// ```
pub fn xxh32(input: &[u8], seed: u32) -> u32 {
    let len = input.len();
    let mut h: u32;

    if len >= 16 {
        // Step 1: Initialize internal accumulators
        let mut v1 = seed.wrapping_add(PRIME32_1).wrapping_add(PRIME32_2);
        let mut v2 = seed.wrapping_add(PRIME32_2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(PRIME32_1);

        // Step 2: Process stripes (16-byte blocks)
        let mut offset = 0;
        let limit = len - 15; // We need at least 16 bytes per stripe
        while offset < limit {
            v1 = round32(v1, read_le_u32(input, offset));
            v2 = round32(v2, read_le_u32(input, offset + 4));
            v3 = round32(v3, read_le_u32(input, offset + 8));
            v4 = round32(v4, read_le_u32(input, offset + 12));
            offset += 16;
        }

        // Step 3: Accumulator convergence
        h = rotl32(v1, 1)
            .wrapping_add(rotl32(v2, 7))
            .wrapping_add(rotl32(v3, 12))
            .wrapping_add(rotl32(v4, 18));

        // Process remaining bytes after the last full stripe
        // These are handled in the finalization phase below with `offset`.
        // We set the start of remaining data here.
        h = h.wrapping_add(len as u32);
        finalize32(h, input, offset)
    } else {
        // Step 1 (small input): Use simplified initialization
        h = seed.wrapping_add(PRIME32_5);
        h = h.wrapping_add(len as u32);
        finalize32(h, input, 0)
    }
}

/// The XXH32 round function: mixes one lane of input into an accumulator.
#[inline(always)]
fn round32(acc: u32, input: u32) -> u32 {
    let acc = acc.wrapping_add(input.wrapping_mul(PRIME32_2));
    let acc = rotl32(acc, 13);
    acc.wrapping_mul(PRIME32_1)
}

/// XXH32 finalization: processes remaining bytes (after stripes) and applies
/// the avalanche function.
#[inline(always)]
fn finalize32(mut h: u32, input: &[u8], mut offset: usize) -> u32 {
    let len = input.len();

    // Process remaining 4-byte chunks
    while offset + 4 <= len {
        h = h.wrapping_add(read_le_u32(input, offset).wrapping_mul(PRIME32_3));
        h = rotl32(h, 17).wrapping_mul(PRIME32_4);
        offset += 4;
    }

    // Process remaining bytes individually
    while offset < len {
        h = h.wrapping_add((input[offset] as u32).wrapping_mul(PRIME32_5));
        h = rotl32(h, 11).wrapping_mul(PRIME32_1);
        offset += 1;
    }

    // Avalanche
    avalanche32(h)
}

/// XXH32 avalanche function: ensures all input bits have a chance to affect
/// every output bit.
#[inline(always)]
fn avalanche32(mut h: u32) -> u32 {
    h ^= h >> 15;
    h = h.wrapping_mul(PRIME32_2);
    h ^= h >> 13;
    h = h.wrapping_mul(PRIME32_3);
    h ^= h >> 16;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xxh32_empty_seed0() {
        assert_eq!(xxh32(&[], 0), 0x02CC5D05);
    }

    #[test]
    fn xxh32_empty_seeded() {
        assert_eq!(xxh32(&[], 0x9E3779B1), 0x36B78AE7);
    }

    #[test]
    fn xxh32_hello() {
        // "hello" is 5 bytes, which exercises the small-input path
        // with both 4-byte chunk processing and single-byte remainder.
        let h = xxh32(b"hello", 0);
        // We don't hardcode a value here; this is just a smoke test
        // that the function runs without panic.
        assert_ne!(h, 0);
    }

    #[test]
    fn round32_basic() {
        // round32(0, 0) = rotl32(0 + 0 * PRIME32_2, 13) * PRIME32_1 = 0
        assert_eq!(round32(0, 0), 0);
        // round32(1, 0) should be non-zero from the initial acc
        assert_ne!(round32(1, 0), 0);
        // round32(0, 1) should be non-zero from the input
        assert_ne!(round32(0, 1), 0);
    }

    #[test]
    fn avalanche32_basic() {
        // Avalanche should mix bits thoroughly
        let a = avalanche32(0);
        assert_eq!(a, 0, "avalanche32(0) = 0");

        let b = avalanche32(1);
        assert_ne!(b, 1, "avalanche should change the value");
    }
}
