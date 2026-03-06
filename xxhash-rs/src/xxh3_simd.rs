//! Platform-optimized SIMD implementations for XXH3 hot paths.
//!
//! This module provides optimized implementations of the XXH3 accumulator
//! operations (`accumulate_stripe` and `scramble_accumulators`) using
//! platform-specific SIMD intrinsics:
//!
//! - **aarch64 (Apple Silicon / NEON):** Uses NEON intrinsics for 128-bit
//!   vector operations on the 8-lane accumulator.
//! - **x86_64 (SSE2):** Uses SSE2 intrinsics for 128-bit vector operations.
//! - **x86_64 (AVX2):** Uses AVX2 intrinsics for 256-bit vector operations
//!   (compile-checked, runtime-selected via `is_x86_feature_detected!`).
//!
//! All SIMD paths produce bit-exact results with the scalar fallback.

/// Prime constant used in scramble operations.
const PRIME32_1: u64 = 0x9E3779B1;

// ============================================================================
// Path detection: which optimized path is active on this build?
// ============================================================================

/// Returns a human-readable string identifying the active XXH3 SIMD path.
///
/// This is used for diagnostics and testing to verify that the expected
/// optimized path is being exercised on a given platform.
pub fn active_simd_path() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    {
        "neon"
    }
    #[cfg(target_arch = "x86_64")]
    {
        // At compile time we know SSE2 is available on x86_64.
        // AVX2 is runtime-detected.
        if is_x86_feature_detected!("avx2") {
            "avx2"
        } else {
            "sse2"
        }
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        "scalar"
    }
}

// ============================================================================
// aarch64 NEON implementation
// ============================================================================

#[cfg(target_arch = "aarch64")]
pub mod neon {
    use core::arch::aarch64::*;

    use super::PRIME32_1;

    /// NEON-optimized accumulate_stripe.
    ///
    /// Processes one 64-byte stripe against 64 bytes of secret, updating
    /// the 8-lane accumulator. Bit-exact with the scalar implementation.
    ///
    /// # Safety
    ///
    /// - `stripe` must contain at least 64 bytes from the given offset.
    /// - `secret` must contain at least `secret_offset + 64` bytes.
    #[inline]
    pub unsafe fn accumulate_stripe_neon(
        acc: &mut [u64; 8],
        stripe: &[u8],
        secret: &[u8],
        secret_offset: usize,
    ) {
        // Process 2 lanes at a time (128-bit NEON registers hold 2×u64)
        for i in 0..4 {
            let lane = i * 2;
            let data_offset = lane * 8;
            let sec_offset = secret_offset + lane * 8;

            // Load 16 bytes of data and secret
            let data_vec = vld1q_u64(stripe.as_ptr().add(data_offset) as *const u64);
            let secret_vec = vld1q_u64(secret.as_ptr().add(sec_offset) as *const u64);

            // value = data XOR secret
            let value = veorq_u64(data_vec, secret_vec);

            // Extract low and high 32-bit parts of each value lane
            // value_lo = value & 0xFFFFFFFF (cast to u32x4, keep even elements)
            let value_lo = vmovn_u64(value); // narrows u64x2 to u32x2
            // value_hi = value >> 32
            let value_hi = vshrn_n_u64(value, 32); // narrows (value >> 32) to u32x2

            // Multiply: (value & 0xFFFFFFFF) * (value >> 32) -> u64
            let product = vmull_u32(value_lo, value_hi);

            // acc[lane ^ 1] += data (swap lanes: 0↔1, 2↔3)
            // For NEON we swap by reversing the two 64-bit elements
            let data_swapped = vcombine_u64(vget_high_u64(data_vec), vget_low_u64(data_vec));
            let acc_vec = vld1q_u64(acc.as_ptr().add(lane));
            let acc_plus_data = vaddq_u64(acc_vec, data_swapped);

            // acc[lane] += product
            let result = vaddq_u64(acc_plus_data, product);

            vst1q_u64(acc.as_mut_ptr().add(lane), result);
        }
    }

    /// NEON-optimized scramble_accumulators.
    ///
    /// Scrambles the 8-lane accumulator using the last 64 bytes of the secret.
    /// Bit-exact with the scalar implementation.
    ///
    /// # Safety
    ///
    /// - `secret` must contain at least `secret_len` bytes.
    /// - `secret_len` must be at least 64.
    #[inline]
    pub unsafe fn scramble_accumulators_neon(
        acc: &mut [u64; 8],
        secret: &[u8],
        secret_len: usize,
    ) {
        let offset = secret_len - 64;
        let prime_vec = vdupq_n_u32(PRIME32_1 as u32);

        for i in 0..4 {
            let lane = i * 2;
            let sec_off = offset + lane * 8;

            let acc_vec = vld1q_u64(acc.as_ptr().add(lane));
            let secret_vec = vld1q_u64(secret.as_ptr().add(sec_off) as *const u64);

            // acc ^= acc >> 47
            let shifted = vshrq_n_u64(acc_vec, 47);
            let xored = veorq_u64(acc_vec, shifted);

            // acc ^= secret
            let xored2 = veorq_u64(xored, secret_vec);

            // acc *= PRIME32_1 (using 32-bit multiply-widen and add)
            // Split into lo/hi 32-bit halves, multiply each by prime, recombine
            let lo32 = vmovn_u64(xored2); // lower 32 bits of each u64
            let hi32 = vshrn_n_u64(xored2, 32); // upper 32 bits

            // lo_product = lo32 * prime (u32 * u32 -> u64)
            let lo_product = vmull_u32(lo32, vget_low_u32(prime_vec));
            // hi_product = hi32 * prime, shifted left by 32
            let hi_product = vmull_u32(hi32, vget_low_u32(prime_vec));
            let hi_shifted = vshlq_n_u64(hi_product, 32);

            let result = vaddq_u64(lo_product, hi_shifted);

            vst1q_u64(acc.as_mut_ptr().add(lane), result);
        }
    }
}

// ============================================================================
// x86_64 SSE2 implementation
// ============================================================================

#[cfg(target_arch = "x86_64")]
pub mod sse2 {
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    use super::PRIME32_1;

    /// SSE2-optimized accumulate_stripe.
    ///
    /// # Safety
    ///
    /// - Requires SSE2 (baseline on x86_64).
    /// - `stripe` must contain at least 64 bytes.
    /// - `secret` must contain at least `secret_offset + 64` bytes.
    #[target_feature(enable = "sse2")]
    #[inline]
    pub unsafe fn accumulate_stripe_sse2(
        acc: &mut [u64; 8],
        stripe: &[u8],
        secret: &[u8],
        secret_offset: usize,
    ) {
        for i in 0..4 {
            let lane = i * 2;
            let data_offset = lane * 8;
            let sec_offset = secret_offset + lane * 8;

            // Load 16 bytes of data and secret
            let data_vec = _mm_loadu_si128(stripe.as_ptr().add(data_offset) as *const __m128i);
            let secret_vec = _mm_loadu_si128(secret.as_ptr().add(sec_offset) as *const __m128i);

            // value = data XOR secret
            let value = _mm_xor_si128(data_vec, secret_vec);

            // Multiply: (value & 0xFFFFFFFF) * (value >> 32) for each 64-bit lane
            // _mm_mul_epu32 multiplies the low 32-bit of each 64-bit lane
            let value_shifted = _mm_srli_epi64(value, 32);
            let product = _mm_mul_epu32(value, value_shifted);

            // Swap data lanes: acc[lane ^ 1] += data
            let data_swapped = _mm_shuffle_epi32(data_vec, 0x4E); // swap 64-bit halves

            let acc_vec = _mm_loadu_si128(acc.as_ptr().add(lane) as *const __m128i);
            let acc_plus_data = _mm_add_epi64(acc_vec, data_swapped);
            let result = _mm_add_epi64(acc_plus_data, product);

            _mm_storeu_si128(acc.as_mut_ptr().add(lane) as *mut __m128i, result);
        }
    }

    /// SSE2-optimized scramble_accumulators.
    ///
    /// # Safety
    ///
    /// - Requires SSE2 (baseline on x86_64).
    /// - `secret` must contain at least `secret_len` bytes.
    #[target_feature(enable = "sse2")]
    #[inline]
    pub unsafe fn scramble_accumulators_sse2(
        acc: &mut [u64; 8],
        secret: &[u8],
        secret_len: usize,
    ) {
        let offset = secret_len - 64;
        let prime_vec = _mm_set1_epi32(PRIME32_1 as i32);

        for i in 0..4 {
            let lane = i * 2;
            let sec_off = offset + lane * 8;

            let acc_vec = _mm_loadu_si128(acc.as_ptr().add(lane) as *const __m128i);
            let secret_vec = _mm_loadu_si128(secret.as_ptr().add(sec_off) as *const __m128i);

            // acc ^= acc >> 47
            let shifted = _mm_srli_epi64(acc_vec, 47);
            let xored = _mm_xor_si128(acc_vec, shifted);

            // acc ^= secret
            let xored2 = _mm_xor_si128(xored, secret_vec);

            // acc *= PRIME32_1
            // Split: lo = xored2 & 0xFFFFFFFF, hi = xored2 >> 32
            let hi32 = _mm_srli_epi64(xored2, 32);

            // lo_product = _mm_mul_epu32(xored2, prime) -> multiplies low 32 bits
            let lo_product = _mm_mul_epu32(xored2, prime_vec);
            // hi_product = _mm_mul_epu32(hi32, prime) << 32
            let hi_product = _mm_mul_epu32(hi32, prime_vec);
            let hi_shifted = _mm_slli_epi64(hi_product, 32);

            let result = _mm_add_epi64(lo_product, hi_shifted);

            _mm_storeu_si128(acc.as_mut_ptr().add(lane) as *mut __m128i, result);
        }
    }
}

// ============================================================================
// x86_64 AVX2 implementation
// ============================================================================

#[cfg(target_arch = "x86_64")]
pub mod avx2 {
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    use super::PRIME32_1;

    /// AVX2-optimized accumulate_stripe.
    ///
    /// Processes 4 accumulator lanes at once (256-bit registers).
    ///
    /// # Safety
    ///
    /// - Requires AVX2.
    /// - `stripe` must contain at least 64 bytes.
    /// - `secret` must contain at least `secret_offset + 64` bytes.
    #[target_feature(enable = "avx2")]
    #[inline]
    pub unsafe fn accumulate_stripe_avx2(
        acc: &mut [u64; 8],
        stripe: &[u8],
        secret: &[u8],
        secret_offset: usize,
    ) {
        for i in 0..2 {
            let lane = i * 4;
            let data_offset = lane * 8;
            let sec_offset = secret_offset + lane * 8;

            // Load 32 bytes of data and secret
            let data_vec = _mm256_loadu_si256(stripe.as_ptr().add(data_offset) as *const __m256i);
            let secret_vec =
                _mm256_loadu_si256(secret.as_ptr().add(sec_offset) as *const __m256i);

            // value = data XOR secret
            let value = _mm256_xor_si256(data_vec, secret_vec);

            // Multiply: (value & 0xFFFFFFFF) * (value >> 32)
            let value_shifted = _mm256_srli_epi64(value, 32);
            let product = _mm256_mul_epu32(value, value_shifted);

            // Swap data lanes: each pair of 64-bit values within 128-bit sub-lanes
            let data_swapped = _mm256_shuffle_epi32(data_vec, 0x4E);

            let acc_vec = _mm256_loadu_si256(acc.as_ptr().add(lane) as *const __m256i);
            let acc_plus_data = _mm256_add_epi64(acc_vec, data_swapped);
            let result = _mm256_add_epi64(acc_plus_data, product);

            _mm256_storeu_si256(acc.as_mut_ptr().add(lane) as *mut __m256i, result);
        }
    }

    /// AVX2-optimized scramble_accumulators.
    ///
    /// # Safety
    ///
    /// - Requires AVX2.
    /// - `secret` must contain at least `secret_len` bytes.
    #[target_feature(enable = "avx2")]
    #[inline]
    pub unsafe fn scramble_accumulators_avx2(
        acc: &mut [u64; 8],
        secret: &[u8],
        secret_len: usize,
    ) {
        let offset = secret_len - 64;
        let prime_vec = _mm256_set1_epi32(PRIME32_1 as i32);

        for i in 0..2 {
            let lane = i * 4;
            let sec_off = offset + lane * 8;

            let acc_vec = _mm256_loadu_si256(acc.as_ptr().add(lane) as *const __m256i);
            let secret_vec =
                _mm256_loadu_si256(secret.as_ptr().add(sec_off) as *const __m256i);

            // acc ^= acc >> 47
            let shifted = _mm256_srli_epi64(acc_vec, 47);
            let xored = _mm256_xor_si256(acc_vec, shifted);

            // acc ^= secret
            let xored2 = _mm256_xor_si256(xored, secret_vec);

            // acc *= PRIME32_1
            let hi32 = _mm256_srli_epi64(xored2, 32);
            let lo_product = _mm256_mul_epu32(xored2, prime_vec);
            let hi_product = _mm256_mul_epu32(hi32, prime_vec);
            let hi_shifted = _mm256_slli_epi64(hi_product, 32);

            let result = _mm256_add_epi64(lo_product, hi_shifted);

            _mm256_storeu_si256(acc.as_mut_ptr().add(lane) as *mut __m256i, result);
        }
    }
}

// ============================================================================
// Dispatching wrappers: select the best available implementation
// ============================================================================

/// Accumulate one 64-byte stripe against the secret, using the best available
/// SIMD path. Bit-exact with the scalar `accumulate_stripe` in `xxh3.rs`.
#[cfg(target_arch = "aarch64")]
#[inline]
pub fn accumulate_stripe_dispatch(
    acc: &mut [u64; 8],
    stripe: &[u8],
    secret: &[u8],
    secret_offset: usize,
) {
    // NEON is always available on aarch64
    unsafe {
        neon::accumulate_stripe_neon(acc, stripe, secret, secret_offset);
    }
}

/// Accumulate one 64-byte stripe against the secret, using the best available
/// SIMD path. Bit-exact with the scalar `accumulate_stripe` in `xxh3.rs`.
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn accumulate_stripe_dispatch(
    acc: &mut [u64; 8],
    stripe: &[u8],
    secret: &[u8],
    secret_offset: usize,
) {
    if is_x86_feature_detected!("avx2") {
        unsafe {
            avx2::accumulate_stripe_avx2(acc, stripe, secret, secret_offset);
        }
    } else {
        // SSE2 is always available on x86_64
        unsafe {
            sse2::accumulate_stripe_sse2(acc, stripe, secret, secret_offset);
        }
    }
}

/// Accumulate one 64-byte stripe against the secret (scalar fallback).
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
#[inline]
pub fn accumulate_stripe_dispatch(
    acc: &mut [u64; 8],
    stripe: &[u8],
    secret: &[u8],
    secret_offset: usize,
) {
    accumulate_stripe_scalar(acc, stripe, secret, secret_offset);
}

/// Scramble the 8-lane accumulator using the last 64 bytes of the secret,
/// using the best available SIMD path. Bit-exact with the scalar
/// `scramble_accumulators` in `xxh3.rs`.
#[cfg(target_arch = "aarch64")]
#[inline]
pub fn scramble_accumulators_dispatch(acc: &mut [u64; 8], secret: &[u8], secret_len: usize) {
    unsafe {
        neon::scramble_accumulators_neon(acc, secret, secret_len);
    }
}

/// Scramble the 8-lane accumulator using the last 64 bytes of the secret,
/// using the best available SIMD path. Bit-exact with the scalar
/// `scramble_accumulators` in `xxh3.rs`.
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn scramble_accumulators_dispatch(acc: &mut [u64; 8], secret: &[u8], secret_len: usize) {
    if is_x86_feature_detected!("avx2") {
        unsafe {
            avx2::scramble_accumulators_avx2(acc, secret, secret_len);
        }
    } else {
        unsafe {
            sse2::scramble_accumulators_sse2(acc, secret, secret_len);
        }
    }
}

/// Scramble the 8-lane accumulator (scalar fallback).
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
#[inline]
pub fn scramble_accumulators_dispatch(acc: &mut [u64; 8], secret: &[u8], secret_len: usize) {
    scramble_accumulators_scalar(acc, secret, secret_len);
}

/// Scalar fallback for accumulate_stripe (used on unsupported platforms).
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
fn accumulate_stripe_scalar(
    acc: &mut [u64; 8],
    stripe: &[u8],
    secret: &[u8],
    secret_offset: usize,
) {
    use crate::helpers::read_le_u64;
    for i in 0..8 {
        let data_val = read_le_u64(stripe, i * 8);
        let secret_val = read_le_u64(secret, secret_offset + i * 8);
        let value = data_val ^ secret_val;
        acc[i ^ 1] = acc[i ^ 1].wrapping_add(data_val);
        acc[i] = acc[i].wrapping_add((value & 0xFFFFFFFF).wrapping_mul(value >> 32));
    }
}

/// Scalar fallback for scramble_accumulators (used on unsupported platforms).
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
fn scramble_accumulators_scalar(acc: &mut [u64; 8], secret: &[u8], secret_len: usize) {
    use crate::helpers::read_le_u64;
    let offset = secret_len - 64;
    for i in 0..8 {
        let secret_val = read_le_u64(secret, offset + i * 8);
        acc[i] ^= acc[i] >> 47;
        acc[i] ^= secret_val;
        acc[i] = acc[i].wrapping_mul(PRIME32_1);
    }
}

// ============================================================================
// Tests: verify SIMD paths produce identical results to scalar
// ============================================================================

#[cfg(test)]
#[allow(clippy::needless_range_loop)]
mod tests {
    use super::*;
    use crate::helpers::read_le_u64;

    /// Scalar reference implementation of accumulate_stripe for test comparison.
    fn accumulate_stripe_scalar_ref(
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

    /// Scalar reference implementation of scramble_accumulators for test comparison.
    fn scramble_accumulators_scalar_ref(acc: &mut [u64; 8], secret: &[u8], secret_len: usize) {
        let offset = secret_len - 64;
        for i in 0..8 {
            let secret_val = read_le_u64(secret, offset + i * 8);
            acc[i] ^= acc[i] >> 47;
            acc[i] ^= secret_val;
            acc[i] = acc[i].wrapping_mul(PRIME32_1);
        }
    }

    fn make_test_stripe() -> [u8; 64] {
        let mut stripe = [0u8; 64];
        for (i, b) in stripe.iter_mut().enumerate() {
            *b = ((i as u64).wrapping_mul(0x9E3779B1) >> 24) as u8;
        }
        stripe
    }

    fn make_test_secret() -> [u8; 192] {
        // Use the default secret from the XXH3 specification
        crate::xxh3::tests_public::default_secret()
    }

    fn make_test_acc() -> [u64; 8] {
        [
            0x9E3779B185EBCA87,
            0xC2B2AE3D27D4EB4F,
            0x165667B19E3779F9,
            0x85EBCA77C2B2AE63,
            0x27D4EB2F165667C5,
            0x9E3779B185EBCA87,
            0xC2B2AE3D27D4EB4F,
            0x165667B19E3779F9,
        ]
    }

    #[test]
    fn dispatch_accumulate_matches_scalar() {
        let stripe = make_test_stripe();
        let secret = make_test_secret();

        let mut acc_scalar = make_test_acc();
        let mut acc_dispatch = make_test_acc();

        accumulate_stripe_scalar_ref(&mut acc_scalar, &stripe, &secret, 0);
        accumulate_stripe_dispatch(&mut acc_dispatch, &stripe, &secret, 0);

        assert_eq!(
            acc_scalar, acc_dispatch,
            "SIMD dispatch accumulate_stripe diverged from scalar"
        );
    }

    #[test]
    fn dispatch_scramble_matches_scalar() {
        let secret = make_test_secret();

        let mut acc_scalar = make_test_acc();
        let mut acc_dispatch = make_test_acc();

        scramble_accumulators_scalar_ref(&mut acc_scalar, &secret, 192);
        scramble_accumulators_dispatch(&mut acc_dispatch, &secret, 192);

        assert_eq!(
            acc_scalar, acc_dispatch,
            "SIMD dispatch scramble_accumulators diverged from scalar"
        );
    }

    #[test]
    fn dispatch_accumulate_all_secret_offsets() {
        let stripe = make_test_stripe();
        let secret = make_test_secret();

        // Test all valid secret offsets for 16 stripes per block
        for offset_idx in 0..16 {
            let secret_offset = offset_idx * 8;
            let mut acc_scalar = make_test_acc();
            let mut acc_dispatch = make_test_acc();

            accumulate_stripe_scalar_ref(&mut acc_scalar, &stripe, &secret, secret_offset);
            accumulate_stripe_dispatch(&mut acc_dispatch, &stripe, &secret, secret_offset);

            assert_eq!(
                acc_scalar, acc_dispatch,
                "SIMD diverged from scalar at secret_offset={}",
                secret_offset
            );
        }
    }

    #[test]
    fn active_path_is_not_scalar_on_supported_arch() {
        let path = active_simd_path();
        #[cfg(target_arch = "aarch64")]
        assert_eq!(path, "neon", "Expected NEON on aarch64");
        #[cfg(target_arch = "x86_64")]
        assert!(
            path == "sse2" || path == "avx2",
            "Expected SSE2 or AVX2 on x86_64, got: {}",
            path
        );
        // On other architectures, scalar is expected
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        assert_eq!(path, "scalar");
    }
}
