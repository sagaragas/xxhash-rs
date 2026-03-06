//! Scalar XXH64 one-shot and streaming hashing.
//!
//! Implements the 64-bit xxHash algorithm as described in the published
//! xxHash specification and BSD-licensed reference material. This module
//! provides:
//! - A one-shot `xxh64()` function for hashing a complete input in a single call.
//! - A streaming `Xxh64State` struct with `reset()`, `update()`, and `digest()`
//!   methods for incremental hashing.

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

// ============================================================================
// XXH64 streaming state machine
// ============================================================================

/// Streaming XXH64 state.
///
/// Supports incremental hashing via `update()` and non-destructive `digest()`.
/// Calling `digest()` does not consume or alter the state; subsequent `update()`
/// calls continue accumulating data and `digest()` can be called again.
///
/// # Examples
///
/// ```
/// use xxhash_rs::xxh64::Xxh64State;
///
/// let mut state = Xxh64State::new(0);
/// state.update(b"hel");
/// state.update(b"lo");
/// let hash = state.digest();
/// ```
pub struct Xxh64State {
    /// Current total length of data consumed.
    total_len: u64,
    /// Whether we have consumed at least 32 bytes (activates large-input path).
    large: bool,
    /// The four accumulator lanes, initialized from the seed.
    v: [u64; 4],
    /// Internal buffer for partial stripes (up to 32 bytes).
    buf: [u8; 32],
    /// Number of valid bytes in `buf`.
    buf_len: usize,
    /// The seed, stored for reset.
    seed: u64,
}

impl Xxh64State {
    /// Creates a new streaming XXH64 state with the given seed.
    pub fn new(seed: u64) -> Self {
        let mut s = Xxh64State {
            total_len: 0,
            large: false,
            v: [0; 4],
            buf: [0u8; 32],
            buf_len: 0,
            seed,
        };
        s.reset_internal();
        s
    }

    /// Resets the state to its initial condition (as if freshly created with
    /// the same seed).
    pub fn reset(&mut self) {
        self.reset_internal();
    }

    /// Resets the state to its initial condition with a new seed.
    pub fn reset_with_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.reset_internal();
    }

    fn reset_internal(&mut self) {
        self.total_len = 0;
        self.large = false;
        self.v[0] = self.seed.wrapping_add(PRIME64_1).wrapping_add(PRIME64_2);
        self.v[1] = self.seed.wrapping_add(PRIME64_2);
        self.v[2] = self.seed;
        self.v[3] = self.seed.wrapping_sub(PRIME64_1);
        self.buf_len = 0;
    }

    /// Feeds more data into the hash state. Can be called any number of times,
    /// including after `digest()`.
    pub fn update(&mut self, input: &[u8]) {
        let len = input.len();
        self.total_len += len as u64;

        let mut offset = 0;

        // If we have buffered bytes, try to fill the buffer to 32
        if self.buf_len > 0 {
            let fill = (32 - self.buf_len).min(len);
            self.buf[self.buf_len..self.buf_len + fill].copy_from_slice(&input[..fill]);
            self.buf_len += fill;
            offset += fill;

            if self.buf_len == 32 {
                // Process the full 32-byte buffer
                self.large = true;
                let buf = self.buf;
                self.v[0] = round64(self.v[0], read_le_u64(&buf, 0));
                self.v[1] = round64(self.v[1], read_le_u64(&buf, 8));
                self.v[2] = round64(self.v[2], read_le_u64(&buf, 16));
                self.v[3] = round64(self.v[3], read_le_u64(&buf, 24));
                self.buf_len = 0;
            }
        }

        // Process full 32-byte stripes from input
        let remaining = len - offset;
        if remaining >= 32 {
            self.large = true;
            let limit = offset + remaining - 31;
            while offset < limit {
                self.v[0] = round64(self.v[0], read_le_u64(input, offset));
                self.v[1] = round64(self.v[1], read_le_u64(input, offset + 8));
                self.v[2] = round64(self.v[2], read_le_u64(input, offset + 16));
                self.v[3] = round64(self.v[3], read_le_u64(input, offset + 24));
                offset += 32;
            }
        }

        // Buffer any remaining bytes
        if offset < len {
            let leftover = len - offset;
            self.buf[..leftover].copy_from_slice(&input[offset..]);
            self.buf_len = leftover;
        }
    }

    /// Computes and returns the current hash digest. This is **non-destructive**:
    /// the internal state is not modified, so `digest()` can be called multiple
    /// times and `update()` can continue afterwards.
    pub fn digest(&self) -> u64 {
        let mut h: u64 = if self.large {
            let h = rotl64(self.v[0], 1)
                .wrapping_add(rotl64(self.v[1], 7))
                .wrapping_add(rotl64(self.v[2], 12))
                .wrapping_add(rotl64(self.v[3], 18));

            let h = merge_round64(h, self.v[0]);
            let h = merge_round64(h, self.v[1]);
            let h = merge_round64(h, self.v[2]);
            merge_round64(h, self.v[3])
        } else {
            self.seed.wrapping_add(PRIME64_5)
        };

        h = h.wrapping_add(self.total_len);

        // Finalize with buffered bytes
        finalize64(h, &self.buf[..self.buf_len], 0)
    }
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
