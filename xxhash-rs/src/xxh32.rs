//! Scalar XXH32 one-shot and streaming hashing.
//!
//! Implements the 32-bit xxHash algorithm as described in the published
//! xxHash specification and BSD-licensed reference material. This module
//! provides:
//! - A one-shot `xxh32()` function for hashing a complete input in a single call.
//! - A streaming `Xxh32State` struct with `reset()`, `update()`, and `digest()`
//!   methods for incremental hashing.

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

// ============================================================================
// XXH32 streaming state machine
// ============================================================================

/// Streaming XXH32 state.
///
/// Supports incremental hashing via `update()` and non-destructive `digest()`.
/// Calling `digest()` does not consume or alter the state; subsequent `update()`
/// calls continue accumulating data and `digest()` can be called again.
///
/// # Examples
///
/// ```
/// use xxhash_rs::xxh32::Xxh32State;
///
/// let mut state = Xxh32State::new(0);
/// state.update(b"hel");
/// state.update(b"lo");
/// let hash = state.digest();
/// ```
pub struct Xxh32State {
    /// Current total length of data consumed.
    total_len: u64,
    /// Whether we have consumed at least 16 bytes (activates large-input path).
    large: bool,
    /// The four accumulator lanes, initialized from the seed.
    v: [u32; 4],
    /// Internal buffer for partial stripes (up to 16 bytes).
    buf: [u8; 16],
    /// Number of valid bytes in `buf`.
    buf_len: usize,
    /// The seed, stored for reset.
    seed: u32,
}

impl Xxh32State {
    /// Creates a new streaming XXH32 state with the given seed.
    pub fn new(seed: u32) -> Self {
        let mut s = Xxh32State {
            total_len: 0,
            large: false,
            v: [0; 4],
            buf: [0u8; 16],
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
    pub fn reset_with_seed(&mut self, seed: u32) {
        self.seed = seed;
        self.reset_internal();
    }

    fn reset_internal(&mut self) {
        self.total_len = 0;
        self.large = false;
        self.v[0] = self.seed.wrapping_add(PRIME32_1).wrapping_add(PRIME32_2);
        self.v[1] = self.seed.wrapping_add(PRIME32_2);
        self.v[2] = self.seed;
        self.v[3] = self.seed.wrapping_sub(PRIME32_1);
        self.buf_len = 0;
    }

    /// Feeds more data into the hash state. Can be called any number of times,
    /// including after `digest()`.
    pub fn update(&mut self, input: &[u8]) {
        let len = input.len();
        self.total_len += len as u64;

        let mut offset = 0;

        // If we have buffered bytes, try to fill the buffer to 16
        if self.buf_len > 0 {
            let fill = (16 - self.buf_len).min(len);
            self.buf[self.buf_len..self.buf_len + fill].copy_from_slice(&input[..fill]);
            self.buf_len += fill;
            offset += fill;

            if self.buf_len == 16 {
                // Process the full 16-byte buffer
                self.large = true;
                let buf = self.buf;
                self.v[0] = round32(self.v[0], read_le_u32(&buf, 0));
                self.v[1] = round32(self.v[1], read_le_u32(&buf, 4));
                self.v[2] = round32(self.v[2], read_le_u32(&buf, 8));
                self.v[3] = round32(self.v[3], read_le_u32(&buf, 12));
                self.buf_len = 0;
            }
        }

        // Process full 16-byte stripes from input
        let remaining = len - offset;
        if remaining >= 16 {
            self.large = true;
            let limit = offset + remaining - 15;
            while offset < limit {
                self.v[0] = round32(self.v[0], read_le_u32(input, offset));
                self.v[1] = round32(self.v[1], read_le_u32(input, offset + 4));
                self.v[2] = round32(self.v[2], read_le_u32(input, offset + 8));
                self.v[3] = round32(self.v[3], read_le_u32(input, offset + 12));
                offset += 16;
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
    pub fn digest(&self) -> u32 {
        let total_len = self.total_len as u32;

        let mut h: u32 = if self.large {
            rotl32(self.v[0], 1)
                .wrapping_add(rotl32(self.v[1], 7))
                .wrapping_add(rotl32(self.v[2], 12))
                .wrapping_add(rotl32(self.v[3], 18))
        } else {
            self.seed.wrapping_add(PRIME32_5)
        };

        h = h.wrapping_add(total_len);

        // Finalize with buffered bytes
        finalize32(h, &self.buf[..self.buf_len], 0)
    }
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
