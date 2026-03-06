//! Scalar XXH3 one-shot and streaming hashing (64-bit and 128-bit variants).
//!
//! Implements the XXH3 algorithm family as described in the published xxHash
//! specification (v0.2.0) and BSD-licensed reference material. This module
//! provides:
//! - One-shot `xxh3_64()` and `xxh3_128()` functions for complete inputs.
//! - Streaming `Xxh3_64State` and `Xxh3_128State` structs with `reset()`,
//!   `update()`, and `digest()` methods for incremental hashing.
//!
//! The implementation is scalar and ready for later SIMD work.

use crate::helpers::{read_le_u32, read_le_u64};
use crate::xxh3_simd::{accumulate_stripe_dispatch, scramble_accumulators_dispatch};

// ============================================================================
// Prime constants from the published specification.
// These are shared with XXH32/XXH64 but redefined here as u64 for XXH3 use.
// ============================================================================

const PRIME32_1: u64 = 0x9E3779B1;
const PRIME32_2: u64 = 0x85EBCA77;
const PRIME32_3: u64 = 0xC2B2AE3D;

const PRIME64_1: u64 = 0x9E3779B185EBCA87;
const PRIME64_2: u64 = 0xC2B2AE3D27D4EB4F;
const PRIME64_3: u64 = 0x165667B19E3779F9;
const PRIME64_4: u64 = 0x85EBCA77C2B2AE63;
const PRIME64_5: u64 = 0x27D4EB2F165667C5;

const PRIME_MX1: u64 = 0x165667919E3779F9;
const PRIME_MX2: u64 = 0x9FB21C651E98DF25;

// ============================================================================
// Default secret (192 bytes) from the published specification.
// ============================================================================

#[rustfmt::skip]
const DEFAULT_SECRET: [u8; 192] = [
    0xb8, 0xfe, 0x6c, 0x39, 0x23, 0xa4, 0x4b, 0xbe, 0x7c, 0x01, 0x81, 0x2c, 0xf7, 0x21, 0xad, 0x1c,
    0xde, 0xd4, 0x6d, 0xe9, 0x83, 0x90, 0x97, 0xdb, 0x72, 0x40, 0xa4, 0xa4, 0xb7, 0xb3, 0x67, 0x1f,
    0xcb, 0x79, 0xe6, 0x4e, 0xcc, 0xc0, 0xe5, 0x78, 0x82, 0x5a, 0xd0, 0x7d, 0xcc, 0xff, 0x72, 0x21,
    0xb8, 0x08, 0x46, 0x74, 0xf7, 0x43, 0x24, 0x8e, 0xe0, 0x35, 0x90, 0xe6, 0x81, 0x3a, 0x26, 0x4c,
    0x3c, 0x28, 0x52, 0xbb, 0x91, 0xc3, 0x00, 0xcb, 0x88, 0xd0, 0x65, 0x8b, 0x1b, 0x53, 0x2e, 0xa3,
    0x71, 0x64, 0x48, 0x97, 0xa2, 0x0d, 0xf9, 0x4e, 0x38, 0x19, 0xef, 0x46, 0xa9, 0xde, 0xac, 0xd8,
    0xa8, 0xfa, 0x76, 0x3f, 0xe3, 0x9c, 0x34, 0x3f, 0xf9, 0xdc, 0xbb, 0xc7, 0xc7, 0x0b, 0x4f, 0x1d,
    0x8a, 0x51, 0xe0, 0x4b, 0xcd, 0xb4, 0x59, 0x31, 0xc8, 0x9f, 0x7e, 0xc9, 0xd9, 0x78, 0x73, 0x64,
    0xea, 0xc5, 0xac, 0x83, 0x34, 0xd3, 0xeb, 0xc3, 0xc5, 0x81, 0xa0, 0xff, 0xfa, 0x13, 0x63, 0xeb,
    0x17, 0x0d, 0xdd, 0x51, 0xb7, 0xf0, 0xda, 0x49, 0xd3, 0x16, 0x55, 0x26, 0x29, 0xd4, 0x68, 0x9e,
    0x2b, 0x16, 0xbe, 0x58, 0x7d, 0x47, 0xa1, 0xfc, 0x8f, 0xf8, 0xb8, 0xd1, 0x7a, 0xd0, 0x31, 0xce,
    0x45, 0xcb, 0x3a, 0x8f, 0x95, 0x16, 0x04, 0x28, 0xaf, 0xd7, 0xfb, 0xca, 0xbb, 0x4b, 0x40, 0x7e,
];

/// The default secret length in bytes.
const DEFAULT_SECRET_LEN: usize = 192;

// ============================================================================
// Low-level helper functions
// ============================================================================

/// XXH3 avalanche function.
#[inline(always)]
fn avalanche(mut x: u64) -> u64 {
    x ^= x >> 37;
    x = x.wrapping_mul(PRIME_MX1);
    x ^= x >> 32;
    x
}

/// XXH64-style avalanche function (used in small-input paths).
#[inline(always)]
fn avalanche_xxh64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(PRIME64_2);
    x ^= x >> 29;
    x = x.wrapping_mul(PRIME64_3);
    x ^= x >> 32;
    x
}

/// 128-bit multiply: returns (low64, high64) of a * b.
#[inline(always)]
fn mul128(a: u64, b: u64) -> (u64, u64) {
    let full = (a as u128).wrapping_mul(b as u128);
    (full as u64, (full >> 64) as u64)
}

/// Mix step: combines 16 bytes of data with 16 bytes of secret and the seed
/// into a single u64 value.
#[inline(always)]
fn mix_step(data: &[u8], secret_offset: usize, seed: u64) -> u64 {
    let data_lo = read_le_u64(data, 0);
    let data_hi = read_le_u64(data, 8);
    let secret_lo = read_le_u64(&DEFAULT_SECRET, secret_offset);
    let secret_hi = read_le_u64(&DEFAULT_SECRET, secret_offset + 8);
    let (lo, hi) = mul128(
        data_lo ^ secret_lo.wrapping_add(seed),
        data_hi ^ secret_hi.wrapping_sub(seed),
    );
    lo ^ hi
}

/// Derive secret from seed for large inputs.
/// When seed != 0 and input > 240, a derived secret is used.
#[inline]
fn derive_secret(seed: u64) -> [u8; 192] {
    let mut derived = [0u8; 192];
    for i in 0..12 {
        let offset = i * 16;
        let lo = read_le_u64(&DEFAULT_SECRET, offset).wrapping_add(seed);
        let hi = read_le_u64(&DEFAULT_SECRET, offset + 8).wrapping_sub(seed);
        derived[offset..offset + 8].copy_from_slice(&lo.to_le_bytes());
        derived[offset + 8..offset + 16].copy_from_slice(&hi.to_le_bytes());
    }
    derived
}

/// Read a little-endian u64 from a secret array.
#[inline(always)]
fn secret_u64(secret: &[u8], offset: usize) -> u64 {
    read_le_u64(secret, offset)
}

/// Read a little-endian u32 from a secret array.
#[inline(always)]
fn secret_u32(secret: &[u8], offset: usize) -> u32 {
    read_le_u32(secret, offset)
}

// ============================================================================
// XXH3_64 one-shot API
// ============================================================================

/// Computes the XXH3 64-bit hash of the given `input` with the specified `seed`.
///
/// This is a one-shot function: pass the complete input as a byte slice.
///
/// # Examples
///
/// ```
/// use xxhash_rs::xxh3::xxh3_64;
///
/// let hash = xxh3_64(b"hello", 0);
/// ```
pub fn xxh3_64(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    match len {
        0 => xxh3_64_empty(seed),
        1..=3 => xxh3_64_1to3(input, seed),
        4..=8 => xxh3_64_4to8(input, seed),
        9..=16 => xxh3_64_9to16(input, seed),
        17..=128 => xxh3_64_17to128(input, seed),
        129..=240 => xxh3_64_129to240(input, seed),
        _ => xxh3_64_large(input, seed),
    }
}

/// Computes the XXH3 128-bit hash of the given `input` with the specified `seed`.
///
/// Returns `(low64, high64)` where `low64` is the lower 64 bits and
/// `high64` is the upper 64 bits of the 128-bit hash.
///
/// # Examples
///
/// ```
/// use xxhash_rs::xxh3::xxh3_128;
///
/// let (lo, hi) = xxh3_128(b"hello", 0);
/// ```
pub fn xxh3_128(input: &[u8], seed: u64) -> (u64, u64) {
    let len = input.len();
    match len {
        0 => xxh3_128_empty(seed),
        1..=3 => xxh3_128_1to3(input, seed),
        4..=8 => xxh3_128_4to8(input, seed),
        9..=16 => xxh3_128_9to16(input, seed),
        17..=128 => xxh3_128_17to128(input, seed),
        129..=240 => xxh3_128_129to240(input, seed),
        _ => xxh3_128_large(input, seed),
    }
}

// ============================================================================
// Small inputs: 0 bytes
// ============================================================================

#[inline]
fn xxh3_64_empty(seed: u64) -> u64 {
    let s0 = secret_u64(&DEFAULT_SECRET, 56);
    let s1 = secret_u64(&DEFAULT_SECRET, 64);
    avalanche_xxh64(seed ^ s0 ^ s1)
}

#[inline]
fn xxh3_128_empty(seed: u64) -> (u64, u64) {
    let s0 = secret_u64(&DEFAULT_SECRET, 64);
    let s1 = secret_u64(&DEFAULT_SECRET, 72);
    let s2 = secret_u64(&DEFAULT_SECRET, 80);
    let s3 = secret_u64(&DEFAULT_SECRET, 88);
    let lo = avalanche_xxh64(seed ^ s0 ^ s1);
    let hi = avalanche_xxh64(seed ^ s2 ^ s3);
    (lo, hi)
}

// ============================================================================
// Small inputs: 1-3 bytes
// ============================================================================

#[inline]
fn xxh3_64_1to3(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    let combined: u32 = (input[len - 1] as u32)
        | ((len as u32) << 8)
        | ((input[0] as u32) << 16)
        | ((input[len >> 1] as u32) << 24);

    let s0 = secret_u32(&DEFAULT_SECRET, 0);
    let s1 = secret_u32(&DEFAULT_SECRET, 4);
    let value = ((s0 ^ s1) as u64).wrapping_add(seed) ^ (combined as u64);
    avalanche_xxh64(value)
}

#[inline]
fn xxh3_128_1to3(input: &[u8], seed: u64) -> (u64, u64) {
    let len = input.len();
    let combined: u32 = (input[len - 1] as u32)
        | ((len as u32) << 8)
        | ((input[0] as u32) << 16)
        | ((input[len >> 1] as u32) << 24);

    let s0 = secret_u32(&DEFAULT_SECRET, 0);
    let s1 = secret_u32(&DEFAULT_SECRET, 4);
    let s2 = secret_u32(&DEFAULT_SECRET, 8);
    let s3 = secret_u32(&DEFAULT_SECRET, 12);

    let low = ((s0 ^ s1) as u64).wrapping_add(seed) ^ (combined as u64);
    // bswap32(combined) <<< 13 is a 32-bit rotate
    let rotated = combined.swap_bytes().rotate_left(13);
    let high = ((s2 ^ s3) as u64).wrapping_sub(seed) ^ (rotated as u64);

    (avalanche_xxh64(low), avalanche_xxh64(high))
}

// ============================================================================
// Small inputs: 4-8 bytes
// ============================================================================

#[inline]
fn xxh3_64_4to8(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    let input_first = read_le_u32(input, 0);
    let input_last = read_le_u32(input, len - 4);

    // modifiedSeed = seed xor ((u64)bswap32((u32)lowerHalf(seed)) << 32)
    let modified_seed = seed ^ (((seed as u32).swap_bytes() as u64) << 32);

    let s0 = secret_u64(&DEFAULT_SECRET, 8);
    let s1 = secret_u64(&DEFAULT_SECRET, 16);
    let combined = (input_last as u64) | ((input_first as u64) << 32);
    let mut value = (s0 ^ s1).wrapping_sub(modified_seed) ^ combined;

    value = value ^ value.rotate_left(49) ^ value.rotate_left(24);
    value = value.wrapping_mul(PRIME_MX2);
    value ^= (value >> 35).wrapping_add(len as u64);
    value = value.wrapping_mul(PRIME_MX2);
    value ^= value >> 28;
    value
}

#[inline]
fn xxh3_128_4to8(input: &[u8], seed: u64) -> (u64, u64) {
    let len = input.len();
    let input_first = read_le_u32(input, 0);
    let input_last = read_le_u32(input, len - 4);

    let modified_seed = seed ^ (((seed as u32).swap_bytes() as u64) << 32);

    let s0 = secret_u64(&DEFAULT_SECRET, 16);
    let s1 = secret_u64(&DEFAULT_SECRET, 24);
    // Note: for 128-bit, combined order is reversed: inputFirst | (inputLast << 32)
    let combined = (input_first as u64) | ((input_last as u64) << 32);
    let value = (s0 ^ s1).wrapping_add(modified_seed) ^ combined;

    let (mut lo, mut hi) = mul128(value, PRIME64_1.wrapping_add((len as u64) << 2));
    hi = hi.wrapping_add(lo << 1);

    lo ^= hi >> 3;
    lo ^= lo >> 35;
    lo = lo.wrapping_mul(PRIME_MX2);
    lo ^= lo >> 28;
    hi = avalanche(hi);
    (lo, hi)
}

// ============================================================================
// Small inputs: 9-16 bytes
// ============================================================================

#[inline]
fn xxh3_64_9to16(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    let input_first = read_le_u64(input, 0);
    let input_last = read_le_u64(input, len - 8);

    let s0 = secret_u64(&DEFAULT_SECRET, 24);
    let s1 = secret_u64(&DEFAULT_SECRET, 32);
    let s2 = secret_u64(&DEFAULT_SECRET, 40);
    let s3 = secret_u64(&DEFAULT_SECRET, 48);

    let low = (s0 ^ s1).wrapping_add(seed) ^ input_first;
    let high = (s2 ^ s3).wrapping_sub(seed) ^ input_last;

    let (mul_lo, mul_hi) = mul128(low, high);
    let value = (len as u64)
        .wrapping_add(low.swap_bytes())
        .wrapping_add(high)
        .wrapping_add(mul_lo ^ mul_hi);
    avalanche(value)
}

#[inline]
fn xxh3_128_9to16(input: &[u8], seed: u64) -> (u64, u64) {
    let len = input.len();
    let input_first = read_le_u64(input, 0);
    let input_last = read_le_u64(input, len - 8);

    let s0 = secret_u64(&DEFAULT_SECRET, 32);
    let s1 = secret_u64(&DEFAULT_SECRET, 40);
    let s2 = secret_u64(&DEFAULT_SECRET, 48);
    let s3 = secret_u64(&DEFAULT_SECRET, 56);

    let val1 = (s0 ^ s1).wrapping_sub(seed) ^ input_first ^ input_last;
    let val2 = (s2 ^ s3).wrapping_add(seed) ^ input_last;

    let (mul_lo, mul_hi) = mul128(val1, PRIME64_1);

    let mut low = mul_lo.wrapping_add(((len as u64).wrapping_sub(1)) << 54);

    // high = mul_hi + (higherHalf(val2) << 32) + lowerHalf(val2) * PRIME32_2
    let val2_hi = val2 >> 32;
    let val2_lo = val2 & 0xFFFFFFFF;
    let mut high = mul_hi
        .wrapping_add(val2_hi << 32)
        .wrapping_add(val2_lo.wrapping_mul(PRIME32_2));

    low ^= high.swap_bytes();

    // 128x64 multiply: {low, high} * PRIME64_2
    let (mul2_lo, mul2_hi) = mul128(low, PRIME64_2);
    low = mul2_lo;
    high = mul2_hi.wrapping_add(high.wrapping_mul(PRIME64_2));

    (avalanche(low), avalanche(high))
}

// ============================================================================
// Medium inputs: 17-128 bytes
// ============================================================================

#[inline]
fn xxh3_64_17to128(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    let mut acc = (len as u64).wrapping_mul(PRIME64_1);

    let num_rounds = ((len - 1) >> 5) + 1;

    // Process rounds from num_rounds-1 down to 0
    for i in (0..num_rounds).rev() {
        let offset_start = i * 16;
        let offset_end = len - i * 16 - 16;
        acc = acc.wrapping_add(mix_step(&input[offset_start..], i * 32, seed));
        acc = acc.wrapping_add(mix_step(&input[offset_end..], i * 32 + 16, seed));
    }

    avalanche(acc)
}

#[inline]
fn xxh3_128_17to128(input: &[u8], seed: u64) -> (u64, u64) {
    let len = input.len();
    let mut acc_lo = (len as u64).wrapping_mul(PRIME64_1);
    let mut acc_hi: u64 = 0;

    let num_rounds = ((len - 1) >> 5) + 1;

    for i in (0..num_rounds).rev() {
        let offset_start = i * 16;
        let offset_end = len - i * 16 - 16;

        // mixTwoChunks: mix data1 (from start) and data2 (from end)
        acc_lo = acc_lo.wrapping_add(mix_step(&input[offset_start..], i * 32, seed));
        acc_hi = acc_hi.wrapping_add(mix_step(&input[offset_end..], i * 32 + 16, seed));

        // acc[0] ^= data2_words[0] + data2_words[1]
        let d2_lo = read_le_u64(input, offset_end);
        let d2_hi = read_le_u64(input, offset_end + 8);
        acc_lo ^= d2_lo.wrapping_add(d2_hi);

        // acc[1] ^= data1_words[0] + data1_words[1]
        let d1_lo = read_le_u64(input, offset_start);
        let d1_hi = read_le_u64(input, offset_start + 8);
        acc_hi ^= d1_lo.wrapping_add(d1_hi);
    }

    // Finalization
    let low = acc_lo.wrapping_add(acc_hi);
    let high = acc_lo
        .wrapping_mul(PRIME64_1)
        .wrapping_add(acc_hi.wrapping_mul(PRIME64_4))
        .wrapping_add(((len as u64).wrapping_sub(seed)).wrapping_mul(PRIME64_2));

    (avalanche(low), 0u64.wrapping_sub(avalanche(high)))
}

// ============================================================================
// Medium inputs: 129-240 bytes
// ============================================================================

#[inline]
fn xxh3_64_129to240(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    let mut acc = (len as u64).wrapping_mul(PRIME64_1);
    let num_chunks = len >> 4;

    // Process first 8 chunks
    for i in 0..8 {
        acc = acc.wrapping_add(mix_step(&input[i * 16..], i * 16, seed));
    }
    acc = avalanche(acc);

    // Process remaining full chunks
    for i in 8..num_chunks {
        acc = acc.wrapping_add(mix_step(&input[i * 16..], (i - 8) * 16 + 3, seed));
    }

    // Process last 16 bytes
    acc = acc.wrapping_add(mix_step(&input[len - 16..], 119, seed));

    avalanche(acc)
}

#[inline]
fn xxh3_128_129to240(input: &[u8], seed: u64) -> (u64, u64) {
    let len = input.len();
    let mut acc_lo = (len as u64).wrapping_mul(PRIME64_1);
    let mut acc_hi: u64 = 0;
    let num_chunks = len >> 5; // number of 32-byte chunks

    // Process first 4 pairs of 16-byte chunks (128 bytes)
    for i in 0..4 {
        let base = i * 32;
        // mixTwoChunks
        acc_lo = acc_lo.wrapping_add(mix_step(&input[base..], i * 32, seed));
        acc_hi = acc_hi.wrapping_add(mix_step(&input[base + 16..], i * 32 + 16, seed));

        let d2_lo = read_le_u64(input, base + 16);
        let d2_hi = read_le_u64(input, base + 24);
        acc_lo ^= d2_lo.wrapping_add(d2_hi);

        let d1_lo = read_le_u64(input, base);
        let d1_hi = read_le_u64(input, base + 8);
        acc_hi ^= d1_lo.wrapping_add(d1_hi);
    }
    acc_lo = avalanche(acc_lo);
    acc_hi = avalanche(acc_hi);

    // Process remaining full 32-byte chunks
    for i in 4..num_chunks {
        let base = i * 32;
        let secret_offset = (i - 4) * 32 + 3;
        // mixTwoChunks
        acc_lo = acc_lo.wrapping_add(mix_step(&input[base..], secret_offset, seed));
        acc_hi = acc_hi.wrapping_add(mix_step(&input[base + 16..], secret_offset + 16, seed));

        let d2_lo = read_le_u64(input, base + 16);
        let d2_hi = read_le_u64(input, base + 24);
        acc_lo ^= d2_lo.wrapping_add(d2_hi);

        let d1_lo = read_le_u64(input, base);
        let d1_hi = read_le_u64(input, base + 8);
        acc_hi ^= d1_lo.wrapping_add(d1_hi);
    }

    // Last 32 bytes with negated seed
    // mixTwoChunks(input[len-16..], input[len-32..len-16], 103, -seed)
    let neg_seed = 0u64.wrapping_sub(seed);
    acc_lo = acc_lo.wrapping_add(mix_step(&input[len - 16..], 103, neg_seed));
    acc_hi = acc_hi.wrapping_add(mix_step(&input[len - 32..], 103 + 16, neg_seed));

    let d2_lo = read_le_u64(input, len - 32);
    let d2_hi = read_le_u64(input, len - 24);
    acc_lo ^= d2_lo.wrapping_add(d2_hi);

    let d1_lo = read_le_u64(input, len - 16);
    let d1_hi = read_le_u64(input, len - 8);
    acc_hi ^= d1_lo.wrapping_add(d1_hi);

    // Finalization
    let low = acc_lo.wrapping_add(acc_hi);
    let high = acc_lo
        .wrapping_mul(PRIME64_1)
        .wrapping_add(acc_hi.wrapping_mul(PRIME64_4))
        .wrapping_add(((len as u64).wrapping_sub(seed)).wrapping_mul(PRIME64_2));

    (avalanche(low), 0u64.wrapping_sub(avalanche(high)))
}

// ============================================================================
// Large inputs: > 240 bytes
// ============================================================================

/// Accumulate one stripe of 64 bytes against 64 bytes of secret.
///
/// Scalar reference implementation of accumulate_stripe, kept as the
/// authoritative correctness oracle for parity tests in integration tests.
#[inline]
pub fn accumulate_stripe_scalar(
    acc: &mut [u64; 8],
    stripe: &[u8],
    secret: &[u8],
    secret_offset: usize,
) {
    for i in 0..8 {
        let data_val = read_le_u64(stripe, i * 8);
        let secret_val = read_le_u64(secret, secret_offset + i * 8);
        let value = data_val ^ secret_val;
        acc[i ^ 1] = acc[i ^ 1].wrapping_add(data_val);
        acc[i] = acc[i].wrapping_add((value & 0xFFFFFFFF).wrapping_mul(value >> 32));
    }
}

/// Accumulate one stripe — dispatches to the best SIMD path available.
#[inline]
fn accumulate_stripe(acc: &mut [u64; 8], stripe: &[u8], secret: &[u8], secret_offset: usize) {
    accumulate_stripe_dispatch(acc, stripe, secret, secret_offset);
}

/// Scramble the accumulators using the last 64 bytes of the secret.
///
/// Scalar reference implementation of scramble_accumulators, kept as the
/// authoritative correctness oracle for parity tests in integration tests.
#[inline]
#[allow(clippy::needless_range_loop)]
pub fn scramble_accumulators_scalar(acc: &mut [u64; 8], secret: &[u8], secret_len: usize) {
    let offset = secret_len - 64;
    for i in 0..8 {
        let secret_val = read_le_u64(secret, offset + i * 8);
        acc[i] ^= acc[i] >> 47;
        acc[i] ^= secret_val;
        acc[i] = acc[i].wrapping_mul(PRIME32_1);
    }
}

/// Scramble accumulators — dispatches to the best SIMD path available.
#[inline]
fn scramble_accumulators(acc: &mut [u64; 8], secret: &[u8], secret_len: usize) {
    scramble_accumulators_dispatch(acc, secret, secret_len);
}

/// Final merge: combine all 8 accumulators into a single u64.
#[inline]
fn final_merge(acc: &[u64; 8], init_value: u64, secret: &[u8], secret_offset: usize) -> u64 {
    let mut result = init_value;
    for i in 0..4 {
        let a = acc[i * 2] ^ read_le_u64(secret, secret_offset + i * 16);
        let b = acc[i * 2 + 1] ^ read_le_u64(secret, secret_offset + i * 16 + 8);
        let (lo, hi) = mul128(a, b);
        result = result.wrapping_add(lo ^ hi);
    }
    avalanche(result)
}

fn xxh3_64_large(input: &[u8], seed: u64) -> u64 {
    let (lo, _hi) = xxh3_128_large(input, seed);
    lo
}

fn xxh3_128_large(input: &[u8], seed: u64) -> (u64, u64) {
    let len = input.len();

    // Determine which secret to use
    let derived;
    let secret: &[u8] = if seed == 0 {
        &DEFAULT_SECRET
    } else {
        derived = derive_secret(seed);
        &derived
    };
    let secret_len = DEFAULT_SECRET_LEN;

    // Initialize accumulators
    let mut acc: [u64; 8] = [
        PRIME32_3, PRIME64_1, PRIME64_2, PRIME64_3, PRIME64_4, PRIME32_2, PRIME64_5, PRIME32_1,
    ];

    let stripes_per_block = (secret_len - 64) / 8; // 16 for 192-byte secret
    let block_size = 64 * stripes_per_block; // 1024 for 192-byte secret

    // Process full blocks (all but the last block's worth of data)
    let nb_blocks = (len - 1) / block_size;
    for block_idx in 0..nb_blocks {
        let block_start = block_idx * block_size;
        // Process all stripes in this block
        for stripe_idx in 0..stripes_per_block {
            let stripe_start = block_start + stripe_idx * 64;
            accumulate_stripe(&mut acc, &input[stripe_start..], secret, stripe_idx * 8);
        }
        // Scramble after each full block
        scramble_accumulators(&mut acc, secret, secret_len);
    }

    // Process the last block
    let last_block_start = nb_blocks * block_size;
    let last_block_len = len - last_block_start;
    let n_full_stripes = (last_block_len - 1) / 64;
    for stripe_idx in 0..n_full_stripes {
        let stripe_start = last_block_start + stripe_idx * 64;
        accumulate_stripe(&mut acc, &input[stripe_start..], secret, stripe_idx * 8);
    }
    // Last stripe is the last 64 bytes of input
    accumulate_stripe(&mut acc, &input[len - 64..], secret, secret_len - 71);

    // Finalization
    let lo = final_merge(
        &acc,
        (len as u64).wrapping_mul(PRIME64_1),
        secret,
        11,
    );
    let hi = final_merge(
        &acc,
        !((len as u64).wrapping_mul(PRIME64_2)),
        secret,
        secret_len - 75,
    );
    (lo, hi)
}

// ============================================================================
// XXH3 streaming state machine (shared core for 64-bit and 128-bit)
// ============================================================================

/// The number of stripes in one block for the default 192-byte secret.
const STRIPES_PER_BLOCK: usize = (DEFAULT_SECRET_LEN - 64) / 8; // 16

/// The stripe length in bytes.
const STRIPE_LEN: usize = 64;

/// Internal buffer size for streaming XXH3.
/// Must hold at least 241 bytes (to cover the mid-size one-shot paths)
/// plus one extra stripe for the last-stripe overlap logic.
/// The reference uses 256 bytes.
const INTERNAL_BUFFER_SIZE: usize = 256;

/// Shared streaming state for XXH3 (used by both 64-bit and 128-bit variants).
///
/// The streaming XXH3 algorithm works as follows:
/// - For inputs ≤240 bytes, all data is buffered and the one-shot paths are
///   used at digest time.
/// - For inputs >240 bytes, the 8-lane accumulator is maintained incrementally
///   as full 64-byte stripes arrive, with scramble operations at block
///   boundaries.
struct Xxh3StreamState {
    /// The 8-lane accumulator.
    acc: [u64; 8],
    /// Internal buffer for partial stripes / small-input accumulation.
    buf: [u8; INTERNAL_BUFFER_SIZE],
    /// Number of valid bytes in `buf`.
    buf_len: usize,
    /// Total length of all data consumed.
    total_len: u64,
    /// Number of stripes processed so far in the current block.
    stripes_so_far: usize,
    /// The seed value.
    seed: u64,
    /// The secret to use (derived from seed if seed != 0).
    secret: [u8; 192],
}

impl Xxh3StreamState {
    fn new(seed: u64) -> Self {
        let secret = if seed == 0 {
            DEFAULT_SECRET
        } else {
            derive_secret(seed)
        };
        Xxh3StreamState {
            acc: [
                PRIME32_3, PRIME64_1, PRIME64_2, PRIME64_3, PRIME64_4, PRIME32_2, PRIME64_5,
                PRIME32_1,
            ],
            buf: [0u8; INTERNAL_BUFFER_SIZE],
            buf_len: 0,
            total_len: 0,
            stripes_so_far: 0,
            seed,
            secret,
        }
    }

    fn reset(&mut self) {
        self.acc = [
            PRIME32_3, PRIME64_1, PRIME64_2, PRIME64_3, PRIME64_4, PRIME32_2, PRIME64_5,
            PRIME32_1,
        ];
        self.buf_len = 0;
        self.total_len = 0;
        self.stripes_so_far = 0;
        self.secret = if self.seed == 0 {
            DEFAULT_SECRET
        } else {
            derive_secret(self.seed)
        };
    }

    fn reset_with_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.reset();
    }

    fn update(&mut self, input: &[u8]) {
        let len = input.len();
        self.total_len += len as u64;

        if len == 0 {
            return;
        }

        let mut offset = 0;

        // If we have buffered bytes, try to fill the buffer
        if self.buf_len > 0 {
            let fill = (INTERNAL_BUFFER_SIZE - self.buf_len).min(len);
            self.buf[self.buf_len..self.buf_len + fill].copy_from_slice(&input[..fill]);
            self.buf_len += fill;
            offset += fill;

            if self.buf_len == INTERNAL_BUFFER_SIZE {
                // Buffer is full, consume all stripes from it.
                // We have 256 bytes = 4 stripes of 64 bytes.
                let n_stripes = (INTERNAL_BUFFER_SIZE - STRIPE_LEN) / STRIPE_LEN; // 3 stripes
                // Process stripes, but the last 64 bytes are kept as overlap.
                self.consume_stripes(
                    &self.buf.clone(),
                    0,
                    n_stripes,
                );
                // Move the last stripe (64 bytes) to the beginning of the buffer
                self.buf.copy_within(INTERNAL_BUFFER_SIZE - STRIPE_LEN.., 0);
                self.buf_len = STRIPE_LEN;
            }
        }

        // Process full blocks directly from input
        let remaining = len - offset;
        if remaining > 0 {
            if self.buf_len == 0 && remaining > INTERNAL_BUFFER_SIZE {
                // Process directly from input without buffering
                let mut pos = offset;
                let end = offset + remaining;

                while end - pos > INTERNAL_BUFFER_SIZE {
                    let n_stripes = (INTERNAL_BUFFER_SIZE - STRIPE_LEN) / STRIPE_LEN;
                    self.consume_stripes(input, pos, n_stripes);
                    pos += n_stripes * STRIPE_LEN;
                }

                // Buffer the remainder
                let leftover = end - pos;
                self.buf[..leftover].copy_from_slice(&input[pos..end]);
                self.buf_len = leftover;
            } else {
                // Copy remaining into buffer
                let leftover = remaining;
                if self.buf_len + leftover <= INTERNAL_BUFFER_SIZE {
                    self.buf[self.buf_len..self.buf_len + leftover]
                        .copy_from_slice(&input[offset..offset + leftover]);
                    self.buf_len += leftover;
                } else {
                    // Need to consume some stripes first
                    // Fill buffer, consume, repeat
                    let mut pos = offset;
                    let end = offset + leftover;
                    while end - pos + self.buf_len > INTERNAL_BUFFER_SIZE {
                        let fill = (INTERNAL_BUFFER_SIZE - self.buf_len).min(end - pos);
                        self.buf[self.buf_len..self.buf_len + fill]
                            .copy_from_slice(&input[pos..pos + fill]);
                        self.buf_len += fill;
                        pos += fill;

                        if self.buf_len == INTERNAL_BUFFER_SIZE {
                            let n_stripes = (INTERNAL_BUFFER_SIZE - STRIPE_LEN) / STRIPE_LEN;
                            self.consume_stripes(&self.buf.clone(), 0, n_stripes);
                            self.buf.copy_within(INTERNAL_BUFFER_SIZE - STRIPE_LEN.., 0);
                            self.buf_len = STRIPE_LEN;
                        }
                    }
                    let final_leftover = end - pos;
                    if final_leftover > 0 {
                        self.buf[self.buf_len..self.buf_len + final_leftover]
                            .copy_from_slice(&input[pos..end]);
                        self.buf_len += final_leftover;
                    }
                }
            }
        }
    }

    /// Process `n_stripes` stripes from `data` starting at `data_offset`.
    fn consume_stripes(&mut self, data: &[u8], data_offset: usize, n_stripes: usize) {
        for i in 0..n_stripes {
            let stripe_start = data_offset + i * STRIPE_LEN;
            let secret_offset = self.stripes_so_far * 8;
            accumulate_stripe(
                &mut self.acc,
                &data[stripe_start..],
                &self.secret,
                secret_offset,
            );
            self.stripes_so_far += 1;
            if self.stripes_so_far == STRIPES_PER_BLOCK {
                scramble_accumulators(&mut self.acc, &self.secret, DEFAULT_SECRET_LEN);
                self.stripes_so_far = 0;
            }
        }
    }

    /// Finalize and return the 64-bit digest. Non-destructive.
    fn digest_64(&self) -> u64 {
        let total_len = self.total_len as usize;

        // Small inputs: use the one-shot path on buffered data
        if total_len <= 240 {
            let data = &self.buf[..self.buf_len];
            return xxh3_64(data, self.seed);
        }

        // Large inputs: finalize the accumulator
        self.digest_large_64()
    }

    /// Finalize and return the 128-bit digest. Non-destructive.
    fn digest_128(&self) -> (u64, u64) {
        let total_len = self.total_len as usize;

        // Small inputs: use the one-shot path on buffered data
        if total_len <= 240 {
            let data = &self.buf[..self.buf_len];
            return xxh3_128(data, self.seed);
        }

        // Large inputs: finalize the accumulator
        self.digest_large_128()
    }

    fn digest_large_64(&self) -> u64 {
        let total_len = self.total_len as usize;

        // Clone accumulators so we don't mutate self
        let mut acc = self.acc;
        let secret = &self.secret;
        let secret_len = DEFAULT_SECRET_LEN;

        // Process remaining stripes in the buffer
        let buf = &self.buf[..self.buf_len];
        let n_remaining_stripes = (self.buf_len - 1) / STRIPE_LEN;
        let mut stripes_so_far = self.stripes_so_far;

        for i in 0..n_remaining_stripes {
            let stripe_start = i * STRIPE_LEN;
            let secret_offset = stripes_so_far * 8;
            accumulate_stripe(&mut acc, &buf[stripe_start..], secret, secret_offset);
            stripes_so_far += 1;
            if stripes_so_far == STRIPES_PER_BLOCK {
                scramble_accumulators(&mut acc, secret, secret_len);
                stripes_so_far = 0;
            }
        }

        // Last stripe: the last 64 bytes of the buffer
        let last_stripe_start = self.buf_len - STRIPE_LEN;
        accumulate_stripe(
            &mut acc,
            &buf[last_stripe_start..],
            secret,
            secret_len - 71,
        );

        // Final merge
        final_merge(
            &acc,
            (total_len as u64).wrapping_mul(PRIME64_1),
            secret,
            11,
        )
    }

    fn digest_large_128(&self) -> (u64, u64) {
        let total_len = self.total_len as usize;

        // Clone accumulators so we don't mutate self
        let mut acc = self.acc;
        let secret = &self.secret;
        let secret_len = DEFAULT_SECRET_LEN;

        // Process remaining stripes in the buffer
        let buf = &self.buf[..self.buf_len];
        let n_remaining_stripes = (self.buf_len - 1) / STRIPE_LEN;
        let mut stripes_so_far = self.stripes_so_far;

        for i in 0..n_remaining_stripes {
            let stripe_start = i * STRIPE_LEN;
            let secret_offset = stripes_so_far * 8;
            accumulate_stripe(&mut acc, &buf[stripe_start..], secret, secret_offset);
            stripes_so_far += 1;
            if stripes_so_far == STRIPES_PER_BLOCK {
                scramble_accumulators(&mut acc, secret, secret_len);
                stripes_so_far = 0;
            }
        }

        // Last stripe: the last 64 bytes of the buffer
        let last_stripe_start = self.buf_len - STRIPE_LEN;
        accumulate_stripe(
            &mut acc,
            &buf[last_stripe_start..],
            secret,
            secret_len - 71,
        );

        // Final merge (same as one-shot large)
        let lo = final_merge(
            &acc,
            (total_len as u64).wrapping_mul(PRIME64_1),
            secret,
            11,
        );
        let hi = final_merge(
            &acc,
            !((total_len as u64).wrapping_mul(PRIME64_2)),
            secret,
            secret_len - 75,
        );
        (lo, hi)
    }
}

// ============================================================================
// Public streaming types: Xxh3_64State and Xxh3_128State
// ============================================================================

/// Streaming XXH3 64-bit state.
///
/// Supports incremental hashing via `update()` and non-destructive `digest()`.
/// Calling `digest()` does not consume or alter the state; subsequent `update()`
/// calls continue accumulating data and `digest()` can be called again.
///
/// # Examples
///
/// ```
/// use xxhash_rs::xxh3::Xxh3_64State;
///
/// let mut state = Xxh3_64State::new(0);
/// state.update(b"hel");
/// state.update(b"lo");
/// let hash = state.digest();
/// ```
#[allow(non_camel_case_types)]
pub struct Xxh3_64State {
    inner: Xxh3StreamState,
}

#[allow(non_camel_case_types)]
impl Xxh3_64State {
    /// Creates a new streaming XXH3 64-bit state with the given seed.
    pub fn new(seed: u64) -> Self {
        Xxh3_64State {
            inner: Xxh3StreamState::new(seed),
        }
    }

    /// Resets the state to its initial condition.
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Resets the state to its initial condition with a new seed.
    pub fn reset_with_seed(&mut self, seed: u64) {
        self.inner.reset_with_seed(seed);
    }

    /// Feeds more data into the hash state.
    pub fn update(&mut self, input: &[u8]) {
        self.inner.update(input);
    }

    /// Computes and returns the current 64-bit hash digest. Non-destructive.
    pub fn digest(&self) -> u64 {
        self.inner.digest_64()
    }
}

/// Streaming XXH3 128-bit state.
///
/// Supports incremental hashing via `update()` and non-destructive `digest()`.
/// Calling `digest()` does not consume or alter the state; subsequent `update()`
/// calls continue accumulating data and `digest()` can be called again.
///
/// # Examples
///
/// ```
/// use xxhash_rs::xxh3::Xxh3_128State;
///
/// let mut state = Xxh3_128State::new(0);
/// state.update(b"hel");
/// state.update(b"lo");
/// let (lo, hi) = state.digest();
/// ```
#[allow(non_camel_case_types)]
pub struct Xxh3_128State {
    inner: Xxh3StreamState,
}

#[allow(non_camel_case_types)]
impl Xxh3_128State {
    /// Creates a new streaming XXH3 128-bit state with the given seed.
    pub fn new(seed: u64) -> Self {
        Xxh3_128State {
            inner: Xxh3StreamState::new(seed),
        }
    }

    /// Resets the state to its initial condition.
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Resets the state to its initial condition with a new seed.
    pub fn reset_with_seed(&mut self, seed: u64) {
        self.inner.reset_with_seed(seed);
    }

    /// Feeds more data into the hash state.
    pub fn update(&mut self, input: &[u8]) {
        self.inner.update(input);
    }

    /// Computes and returns the current 128-bit hash digest as `(low64, high64)`.
    /// Non-destructive.
    pub fn digest(&self) -> (u64, u64) {
        self.inner.digest_128()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xxh3_64_empty_seed0() {
        assert_eq!(xxh3_64(&[], 0), 0x2D06800538D394C2);
    }

    #[test]
    fn xxh3_64_empty_seeded() {
        assert_eq!(xxh3_64(&[], 0x9E3779B185EBCA8D), 0xA8A6B918B2F0364A);
    }

    #[test]
    fn xxh3_128_empty_seed0() {
        let (lo, hi) = xxh3_128(&[], 0);
        assert_eq!(lo, 0x6001C324468D497F);
        assert_eq!(hi, 0x99AA06D3014798D8);
    }

    #[test]
    fn xxh3_64_1byte_seed0() {
        // From vectors: len=1, seed=0 -> 0xC44BDFF4074EECDB
        // Use the canonical test buffer
        let buf = crate::xxh3::tests::test_buffer(1);
        assert_eq!(xxh3_64(&buf, 0), 0xC44BDFF4074EECDB);
    }

    #[test]
    fn xxh3_128_1byte_seed0() {
        let buf = test_buffer(1);
        let (lo, hi) = xxh3_128(&buf, 0);
        assert_eq!(lo, 0xC44BDFF4074EECDB);
        assert_eq!(hi, 0xA6CD5E9392000F6A);
    }

    /// Generate the canonical test buffer (same as fixtures::generate_test_buffer).
    fn test_buffer(len: usize) -> Vec<u8> {
        const P32: u64 = 2_654_435_761;
        const P64: u64 = 11_400_714_785_074_694_797;
        let mut buffer = Vec::with_capacity(len);
        let mut byte_gen: u64 = P32;
        for _ in 0..len {
            buffer.push((byte_gen >> 56) as u8);
            byte_gen = byte_gen.wrapping_mul(P64);
        }
        buffer
    }

    #[test]
    fn xxh3_64_4byte_seed0() {
        let buf = test_buffer(4);
        assert_eq!(xxh3_64(&buf, 0), 0xE5DC74BC51848A51);
    }

    #[test]
    fn xxh3_64_9byte_seed0() {
        let buf = test_buffer(9);
        assert_eq!(xxh3_64(&buf, 0), 0x14D5001C15DD3F2B);
    }

    #[test]
    fn xxh3_64_16byte_seed0() {
        let buf = test_buffer(16);
        assert_eq!(xxh3_64(&buf, 0), 0x981B17D36C7498C9);
    }

    #[test]
    fn xxh3_64_17byte_seed0() {
        let buf = test_buffer(17);
        assert_eq!(xxh3_64(&buf, 0), 0x796F5ACD3A60F862);
    }

    #[test]
    fn xxh3_64_128byte_seed0() {
        let buf = test_buffer(128);
        assert_eq!(xxh3_64(&buf, 0), 0xFCFF24126754D861);
    }

    #[test]
    fn xxh3_64_129byte_seed0() {
        let buf = test_buffer(129);
        assert_eq!(xxh3_64(&buf, 0), 0x98F1B0A679A2CA29);
    }

    #[test]
    fn xxh3_64_240byte_seed0() {
        let buf = test_buffer(240);
        assert_eq!(xxh3_64(&buf, 0), 0x81C3C2B67F568CCF);
    }

    #[test]
    fn xxh3_64_241byte_seed0() {
        let buf = test_buffer(241);
        assert_eq!(xxh3_64(&buf, 0), 0xC5A639ECD2030E5E);
    }

    #[test]
    fn xxh3_64_256byte_seed0() {
        let buf = test_buffer(256);
        assert_eq!(xxh3_64(&buf, 0), 0x55DE574AD89D0AC5);
    }

    #[test]
    fn xxh3_64_512byte_seed0() {
        let buf = test_buffer(512);
        assert_eq!(xxh3_64(&buf, 0), 0x617E49599013CB6B);
    }

    #[test]
    fn xxh3_64_seeded_short() {
        let buf = test_buffer(1);
        assert_eq!(xxh3_64(&buf, 0x9E3779B185EBCA8D), 0x032BE332DD766EF8);
    }

    #[test]
    fn xxh3_64_seeded_medium() {
        let buf = test_buffer(17);
        assert_eq!(xxh3_64(&buf, 0x9E3779B185EBCA8D), 0xF3EC5067F4306DB3);
    }

    #[test]
    fn xxh3_64_seeded_large() {
        let buf = test_buffer(241);
        assert_eq!(xxh3_64(&buf, 0x9E3779B185EBCA8D), 0xDDA9B0A161D4829A);
    }
}

/// Test-support module: exposes internal constants and scalar-only oracles
/// needed by SIMD parity tests.  Not part of the public API surface.
#[doc(hidden)]
pub mod tests_public {
    use super::{
        accumulate_stripe_scalar, derive_secret, final_merge, scramble_accumulators_scalar,
        xxh3_128, xxh3_64, DEFAULT_SECRET, DEFAULT_SECRET_LEN, PRIME32_1, PRIME32_2, PRIME32_3,
        PRIME64_1, PRIME64_2, PRIME64_3, PRIME64_4, PRIME64_5,
    };

    /// Returns a copy of the default XXH3 secret for use in SIMD parity tests.
    pub fn default_secret() -> [u8; 192] {
        DEFAULT_SECRET
    }

    /// Scalar-only XXH3-128 large-input computation.
    ///
    /// Duplicates the `xxh3_128_large` algorithm but forces the scalar
    /// `accumulate_stripe` and `scramble_accumulators` functions, giving
    /// tests an independent oracle that never touches SIMD dispatch.
    ///
    /// **Must only be called with `input.len() > 240`.**
    pub fn xxh3_128_large_scalar(input: &[u8], seed: u64) -> (u64, u64) {
        let len = input.len();
        debug_assert!(len > 240);

        let derived;
        let secret: &[u8] = if seed == 0 {
            &DEFAULT_SECRET
        } else {
            derived = derive_secret(seed);
            &derived
        };
        let secret_len = DEFAULT_SECRET_LEN;

        let mut acc: [u64; 8] = [
            PRIME32_3, PRIME64_1, PRIME64_2, PRIME64_3, PRIME64_4, PRIME32_2, PRIME64_5, PRIME32_1,
        ];

        let stripes_per_block = (secret_len - 64) / 8;
        let block_size = 64 * stripes_per_block;

        let nb_blocks = (len - 1) / block_size;
        for block_idx in 0..nb_blocks {
            let block_start = block_idx * block_size;
            for stripe_idx in 0..stripes_per_block {
                let stripe_start = block_start + stripe_idx * 64;
                accumulate_stripe_scalar(&mut acc, &input[stripe_start..], secret, stripe_idx * 8);
            }
            scramble_accumulators_scalar(&mut acc, secret, secret_len);
        }

        let last_block_start = nb_blocks * block_size;
        let last_block_len = len - last_block_start;
        let n_full_stripes = (last_block_len - 1) / 64;
        for stripe_idx in 0..n_full_stripes {
            let stripe_start = last_block_start + stripe_idx * 64;
            accumulate_stripe_scalar(&mut acc, &input[stripe_start..], secret, stripe_idx * 8);
        }
        accumulate_stripe_scalar(&mut acc, &input[len - 64..], secret, secret_len - 71);

        let lo = final_merge(&acc, (len as u64).wrapping_mul(PRIME64_1), secret, 11);
        let hi = final_merge(
            &acc,
            !((len as u64).wrapping_mul(PRIME64_2)),
            secret,
            secret_len - 75,
        );
        (lo, hi)
    }

    /// Scalar-only XXH3-64 large-input computation.
    ///
    /// Returns the low 64 bits of `xxh3_128_large_scalar`.
    /// **Must only be called with `input.len() > 240`.**
    pub fn xxh3_64_large_scalar(input: &[u8], seed: u64) -> u64 {
        let (lo, _hi) = xxh3_128_large_scalar(input, seed);
        lo
    }

    /// Scalar-only XXH3-64 wrapper that delegates to the large-input scalar
    /// oracle for inputs > 240 bytes and falls through to the standard
    /// (non-dispatched) API for shorter inputs.
    pub fn xxh3_64_scalar(input: &[u8], seed: u64) -> u64 {
        if input.len() > 240 {
            xxh3_64_large_scalar(input, seed)
        } else {
            xxh3_64(input, seed)
        }
    }

    /// Scalar-only XXH3-128 wrapper that delegates to the large-input scalar
    /// oracle for inputs > 240 bytes and falls through to the standard
    /// (non-dispatched) API for shorter inputs.
    pub fn xxh3_128_scalar(input: &[u8], seed: u64) -> (u64, u64) {
        if input.len() > 240 {
            xxh3_128_large_scalar(input, seed)
        } else {
            xxh3_128(input, seed)
        }
    }
}
