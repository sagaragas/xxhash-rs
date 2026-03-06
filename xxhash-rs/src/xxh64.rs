//! Scalar XXH64 one-shot hashing.
//!
//! Implements the 64-bit xxHash algorithm as described in the published
//! xxHash specification and BSD-licensed reference material. This module
//! provides a one-shot `xxh64()` function suitable for hashing a complete
//! input in a single call.
//!
//! The streaming API is left for a later feature.

use crate::helpers::{read_le_u32, read_le_u64, rotl64};

// ============================================================================
// XXH64 prime constants from the published specification.
// ============================================================================

/// XXH_PRIME64_1: used in the initial accumulator setup and round mixing.
pub const PRIME64_1: u64 = 0x9E3779B185EBCA87;

/// XXH_PRIME64_2: used in the round function multiply step.
pub const PRIME64_2: u64 = 0xC2B2AE3D27D4EB4F;

/// XXH_PRIME64_3: used for remaining 8-byte chunks after the main loop.
pub const PRIME64_3: u64 = 0x165667B19E3779F9;

/// XXH_PRIME64_4: used for remaining 4-byte chunks.
pub const PRIME64_4: u64 = 0x85EBCA77C2B2AE63;

/// XXH_PRIME64_5: added to the hash length and used in the finalization.
pub const PRIME64_5: u64 = 0x27D4EB2F165667C5;

// ============================================================================
// XXH64 algorithm
// ============================================================================

/// Computes the XXH64 hash of the given `input` with the specified `seed`.
///
/// This is a one-shot function: pass the complete input as a byte slice.
///
/// # Examples
///
/// ```
/// use xxhash_rs::xxh64::xxh64;
///
/// let hash = xxh64(b"hello", 0);
/// ```
pub fn xxh64(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    let mut h: u64;

    if len >= 32 {
        // Step 1: Initialize internal accumulators
        let mut v1 = seed.wrapping_add(PRIME64_1).wrapping_add(PRIME64_2);
        let mut v2 = seed.wrapping_add(PRIME64_2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(PRIME64_1);

        // Step 2: Process stripes (32-byte blocks)
        let mut offset = 0;
        let limit = len - 31; // We need at least 32 bytes per stripe
        while offset < limit {
            v1 = round64(v1, read_le_u64(input, offset));
            v2 = round64(v2, read_le_u64(input, offset + 8));
            v3 = round64(v3, read_le_u64(input, offset + 16));
            v4 = round64(v4, read_le_u64(input, offset + 24));
            offset += 32;
        }

        // Step 3: Accumulator convergence
        h = rotl64(v1, 1)
            .wrapping_add(rotl64(v2, 7))
            .wrapping_add(rotl64(v3, 12))
            .wrapping_add(rotl64(v4, 18));

        h = merge_round64(h, v1);
        h = merge_round64(h, v2);
        h = merge_round64(h, v3);
        h = merge_round64(h, v4);

        // Add total length
        h = h.wrapping_add(len as u64);

        // Process remaining bytes
        finalize64(h, input, offset)
    } else {
        // Step 1 (small input): Use simplified initialization
        h = seed.wrapping_add(PRIME64_5);
        h = h.wrapping_add(len as u64);
        finalize64(h, input, 0)
    }
}

/// The XXH64 round function: mixes one lane of input into an accumulator.
#[inline(always)]
fn round64(acc: u64, input: u64) -> u64 {
    let acc = acc.wrapping_add(input.wrapping_mul(PRIME64_2));
    let acc = rotl64(acc, 31);
    acc.wrapping_mul(PRIME64_1)
}

/// The XXH64 merge-round function: used during accumulator convergence.
#[inline(always)]
fn merge_round64(mut acc: u64, val: u64) -> u64 {
    let val = round64(0, val);
    acc ^= val;
    acc = acc.wrapping_mul(PRIME64_1).wrapping_add(PRIME64_4);
    acc
}

/// XXH64 finalization: processes remaining bytes (after stripes) and applies
/// the avalanche function.
#[inline(always)]
fn finalize64(mut h: u64, input: &[u8], mut offset: usize) -> u64 {
    let len = input.len();

    // Process remaining 8-byte chunks
    while offset + 8 <= len {
        let k1 = round64(0, read_le_u64(input, offset));
        h ^= k1;
        h = rotl64(h, 27).wrapping_mul(PRIME64_1).wrapping_add(PRIME64_4);
        offset += 8;
    }

    // Process remaining 4-byte chunk (at most one)
    if offset + 4 <= len {
        h ^= (read_le_u32(input, offset) as u64).wrapping_mul(PRIME64_1);
        h = rotl64(h, 23)
            .wrapping_mul(PRIME64_2)
            .wrapping_add(PRIME64_3);
        offset += 4;
    }

    // Process remaining bytes individually
    while offset < len {
        h ^= (input[offset] as u64).wrapping_mul(PRIME64_5);
        h = rotl64(h, 11).wrapping_mul(PRIME64_1);
        offset += 1;
    }

    // Avalanche
    avalanche64(h)
}

/// XXH64 avalanche function: ensures all input bits have a chance to affect
/// every output bit.
#[inline(always)]
fn avalanche64(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(PRIME64_2);
    h ^= h >> 29;
    h = h.wrapping_mul(PRIME64_3);
    h ^= h >> 32;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xxh64_empty_seed0() {
        assert_eq!(xxh64(&[], 0), 0xEF46DB3751D8E999);
    }

    #[test]
    fn xxh64_empty_seeded() {
        assert_eq!(xxh64(&[], 0x000000009E3779B1), 0xAC75FDA2929B17EF);
    }

    #[test]
    fn xxh64_hello() {
        let h = xxh64(b"hello", 0);
        assert_ne!(h, 0);
    }

    #[test]
    fn round64_basic() {
        // round64(0, 0) = rotl64(0 + 0 * PRIME64_2, 31) * PRIME64_1 = 0
        assert_eq!(round64(0, 0), 0);
        // Non-zero inputs should produce non-zero output
        assert_ne!(round64(1, 0), 0);
        assert_ne!(round64(0, 1), 0);
    }

    #[test]
    fn avalanche64_basic() {
        let a = avalanche64(0);
        assert_eq!(a, 0);

        let b = avalanche64(1);
        assert_ne!(b, 1);
    }
}
