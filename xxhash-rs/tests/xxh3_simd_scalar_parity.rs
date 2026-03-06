//! Scalar-vs-optimized parity tests for XXH3 SIMD hot paths.
//!
//! Verifies that the SIMD-optimized `accumulate_stripe` and
//! `scramble_accumulators` produce bit-exact results compared to the
//! scalar reference implementation, and that the full one-shot and
//! streaming large-input paths produce identical digests under the
//! optimized path vs an independent scalar-only oracle that bypasses
//! SIMD dispatch entirely.
//!
//! Expectations for end-to-end tests use `xxh3_*_scalar()` (which calls
//! `accumulate_stripe_scalar` / `scramble_accumulators_scalar` internally),
//! so a tautological "same path verifies itself" gap is impossible.
//!
//! Covers VAL-HASH-007: Platform-optimized XXH3 long-input hashing stays
//! bit-exact on Apple Silicon.

#[allow(dead_code)]
mod fixtures;

use fixtures::generate_test_buffer;
use xxhash_rs::xxh3::{
    accumulate_stripe_scalar, scramble_accumulators_scalar, xxh3_128, xxh3_64, Xxh3_128State,
    Xxh3_64State,
};
use xxhash_rs::xxh3::tests_public::{default_secret, xxh3_64_scalar, xxh3_128_scalar};
use xxhash_rs::xxh3_simd::{accumulate_stripe_dispatch, scramble_accumulators_dispatch};

/// Initial accumulator values from the XXH3 specification.
fn initial_acc() -> [u64; 8] {
    [
        0xC2B2AE3D, // PRIME32_3
        0x9E3779B185EBCA87, // PRIME64_1
        0xC2B2AE3D27D4EB4F, // PRIME64_2
        0x165667B19E3779F9, // PRIME64_3
        0x85EBCA77C2B2AE63, // PRIME64_4
        0x85EBCA77, // PRIME32_2
        0x27D4EB2F165667C5, // PRIME64_5
        0x9E3779B1, // PRIME32_1
    ]
}

// ============================================================================
// Unit-level: accumulate_stripe parity
// ============================================================================

#[test]
fn xxh3_simd_scalar_parity_accumulate_stripe_basic() {
    let secret = default_secret();
    let stripe = generate_test_buffer(64);

    let mut acc_scalar = initial_acc();
    let mut acc_simd = initial_acc();

    accumulate_stripe_scalar(&mut acc_scalar, &stripe, &secret, 0);
    accumulate_stripe_dispatch(&mut acc_simd, &stripe, &secret, 0);

    assert_eq!(
        acc_scalar, acc_simd,
        "accumulate_stripe: SIMD diverged from scalar at offset 0"
    );
}

#[test]
fn xxh3_simd_scalar_parity_accumulate_stripe_all_offsets() {
    let secret = default_secret();
    let stripe = generate_test_buffer(64);

    // The large-input path uses secret offsets 0, 8, 16, ..., 120 (16 stripes per block)
    for stripe_idx in 0..16 {
        let secret_offset = stripe_idx * 8;
        let mut acc_scalar = initial_acc();
        let mut acc_simd = initial_acc();

        accumulate_stripe_scalar(&mut acc_scalar, &stripe, &secret, secret_offset);
        accumulate_stripe_dispatch(&mut acc_simd, &stripe, &secret, secret_offset);

        assert_eq!(
            acc_scalar, acc_simd,
            "accumulate_stripe: SIMD diverged from scalar at secret_offset={}",
            secret_offset
        );
    }
}

#[test]
fn xxh3_simd_scalar_parity_accumulate_stripe_last_stripe_offset() {
    let secret = default_secret();
    let stripe = generate_test_buffer(64);

    // The last stripe uses secret_offset = secret_len - 71 = 192 - 71 = 121
    let secret_offset = 192 - 71;
    let mut acc_scalar = initial_acc();
    let mut acc_simd = initial_acc();

    accumulate_stripe_scalar(&mut acc_scalar, &stripe, &secret, secret_offset);
    accumulate_stripe_dispatch(&mut acc_simd, &stripe, &secret, secret_offset);

    assert_eq!(
        acc_scalar, acc_simd,
        "accumulate_stripe: SIMD diverged from scalar at last-stripe offset"
    );
}

// ============================================================================
// Unit-level: scramble_accumulators parity
// ============================================================================

#[test]
fn xxh3_simd_scalar_parity_scramble_basic() {
    let secret = default_secret();
    let mut acc_scalar = initial_acc();
    let mut acc_simd = initial_acc();

    scramble_accumulators_scalar(&mut acc_scalar, &secret, 192);
    scramble_accumulators_dispatch(&mut acc_simd, &secret, 192);

    assert_eq!(
        acc_scalar, acc_simd,
        "scramble_accumulators: SIMD diverged from scalar"
    );
}

#[test]
fn xxh3_simd_scalar_parity_scramble_after_accumulate() {
    let secret = default_secret();
    let stripe = generate_test_buffer(64);

    let mut acc_scalar = initial_acc();
    let mut acc_simd = initial_acc();

    // Accumulate a full block (16 stripes) + scramble
    for stripe_idx in 0..16 {
        accumulate_stripe_scalar(&mut acc_scalar, &stripe, &secret, stripe_idx * 8);
        accumulate_stripe_dispatch(&mut acc_simd, &stripe, &secret, stripe_idx * 8);
    }
    scramble_accumulators_scalar(&mut acc_scalar, &secret, 192);
    scramble_accumulators_dispatch(&mut acc_simd, &secret, 192);

    assert_eq!(
        acc_scalar, acc_simd,
        "Full block + scramble: SIMD diverged from scalar"
    );
}

// ============================================================================
// Integration-level: one-shot XXH3_64 large-input parity
// ============================================================================

/// Representative large-input sizes that exercise the long-input path.
const LARGE_SIZES: &[usize] = &[
    241,    // minimum for large path
    256,    // 4 full stripes
    512,    // 8 full stripes
    1024,   // exactly 1 block (16 stripes × 64 bytes)
    1025,   // 1 block + 1 byte
    2048,   // 2 blocks
    4096,   // 4 blocks
    8192,   // 8 blocks
    10000,  // non-power-of-two
    65536,  // 64 KB
    100_000, // 100 KB
];

#[test]
fn xxh3_simd_scalar_parity_xxh3_64_large_inputs_seed0() {
    for &size in LARGE_SIZES {
        let buf = generate_test_buffer(size);
        let opt_hash = xxh3_64(&buf, 0);
        let scalar_hash = xxh3_64_scalar(&buf, 0);

        // Primary: optimized path must match independent scalar oracle.
        assert_eq!(
            opt_hash, scalar_hash,
            "XXH3_64 len={}: optimized diverged from scalar oracle",
            size
        );

        // Cross-check against known reference vectors where available.
        match size {
            241 => assert_eq!(opt_hash, 0xC5A639ECD2030E5E, "XXH3_64 len=241 mismatch"),
            256 => assert_eq!(opt_hash, 0x55DE574AD89D0AC5, "XXH3_64 len=256 mismatch"),
            512 => assert_eq!(opt_hash, 0x617E49599013CB6B, "XXH3_64 len=512 mismatch"),
            _ => {}
        }

        // Streaming must also agree with scalar oracle.
        let mut state = Xxh3_64State::new(0);
        state.update(&buf);
        let streaming_hash = state.digest();
        assert_eq!(
            streaming_hash, scalar_hash,
            "XXH3_64 len={}: streaming diverged from scalar oracle",
            size
        );
    }
}

#[test]
fn xxh3_simd_scalar_parity_xxh3_64_large_inputs_seeded() {
    let seed = 0x9E3779B185EBCA8D_u64;
    for &size in LARGE_SIZES {
        let buf = generate_test_buffer(size);
        let opt_hash = xxh3_64(&buf, seed);
        let scalar_hash = xxh3_64_scalar(&buf, seed);

        // Primary: optimized path must match independent scalar oracle.
        assert_eq!(
            opt_hash, scalar_hash,
            "XXH3_64 seeded len={}: optimized diverged from scalar oracle",
            size
        );

        // Cross-check against known seeded vector.
        if size == 241 {
            assert_eq!(opt_hash, 0xDDA9B0A161D4829A, "XXH3_64 seeded len=241 mismatch");
        }

        // Streaming must also agree with scalar oracle.
        let mut state = Xxh3_64State::new(seed);
        state.update(&buf);
        let streaming_hash = state.digest();
        assert_eq!(
            streaming_hash, scalar_hash,
            "XXH3_64 seeded len={}: streaming diverged from scalar oracle",
            size
        );
    }
}

// ============================================================================
// Integration-level: one-shot XXH3_128 large-input parity
// ============================================================================

#[test]
fn xxh3_simd_scalar_parity_xxh3_128_large_inputs_seed0() {
    for &size in LARGE_SIZES {
        let buf = generate_test_buffer(size);
        let opt = xxh3_128(&buf, 0);
        let scalar = xxh3_128_scalar(&buf, 0);

        // Primary: optimized path must match independent scalar oracle.
        assert_eq!(
            opt, scalar,
            "XXH3_128 len={}: optimized diverged from scalar oracle",
            size
        );

        // Streaming must also agree with scalar oracle.
        let mut state = Xxh3_128State::new(0);
        state.update(&buf);
        let streaming = state.digest();
        assert_eq!(
            streaming, scalar,
            "XXH3_128 len={}: streaming diverged from scalar oracle",
            size
        );
    }
}

#[test]
fn xxh3_simd_scalar_parity_xxh3_128_large_inputs_seeded() {
    let seed = 0x9E3779B185EBCA8D_u64;
    for &size in LARGE_SIZES {
        let buf = generate_test_buffer(size);
        let opt = xxh3_128(&buf, seed);
        let scalar = xxh3_128_scalar(&buf, seed);

        // Primary: optimized path must match independent scalar oracle.
        assert_eq!(
            opt, scalar,
            "XXH3_128 seeded len={}: optimized diverged from scalar oracle",
            size
        );

        // Streaming must also agree with scalar oracle.
        let mut state = Xxh3_128State::new(seed);
        state.update(&buf);
        let streaming = state.digest();
        assert_eq!(
            streaming, scalar,
            "XXH3_128 seeded len={}: streaming diverged from scalar oracle",
            size
        );
    }
}

// ============================================================================
// Multi-block accumulator chain parity
// ============================================================================

#[test]
fn xxh3_simd_scalar_parity_multi_block_accumulator_chain() {
    let secret = default_secret();
    let buf = generate_test_buffer(4096);

    let mut acc_scalar = initial_acc();
    let mut acc_simd = initial_acc();

    let stripes_per_block = 16;
    let stripe_len = 64;

    // Process 4 full blocks (each = 16 stripes × 64 bytes = 1024 bytes)
    for block in 0..4 {
        for stripe_idx in 0..stripes_per_block {
            let stripe_start = block * stripes_per_block * stripe_len + stripe_idx * stripe_len;
            accumulate_stripe_scalar(
                &mut acc_scalar,
                &buf[stripe_start..],
                &secret,
                stripe_idx * 8,
            );
            accumulate_stripe_dispatch(
                &mut acc_simd,
                &buf[stripe_start..],
                &secret,
                stripe_idx * 8,
            );
        }
        scramble_accumulators_scalar(&mut acc_scalar, &secret, 192);
        scramble_accumulators_dispatch(&mut acc_simd, &secret, 192);

        assert_eq!(
            acc_scalar, acc_simd,
            "Multi-block chain diverged after block {}",
            block
        );
    }
}

// ============================================================================
// Derived-secret parity (seed != 0)
// ============================================================================

#[test]
fn xxh3_simd_scalar_parity_derived_secret_large() {
    // When seed != 0, the large path derives a new secret from the seed.
    // The SIMD path must handle the derived secret identically to the
    // scalar oracle.
    let seed = 42_u64;
    for &size in &[1024, 2048, 4096, 65536] {
        let buf = generate_test_buffer(size);
        let opt_64 = xxh3_64(&buf, seed);
        let opt_128 = xxh3_128(&buf, seed);
        let scalar_64 = xxh3_64_scalar(&buf, seed);
        let scalar_128 = xxh3_128_scalar(&buf, seed);

        // Primary: optimized must match scalar oracle.
        assert_eq!(
            opt_64, scalar_64,
            "Derived-secret XXH3_64 len={}: optimized diverged from scalar oracle",
            size
        );
        assert_eq!(
            opt_128, scalar_128,
            "Derived-secret XXH3_128 len={}: optimized diverged from scalar oracle",
            size
        );

        // Streaming must also agree with scalar oracle.
        let mut state_64 = Xxh3_64State::new(seed);
        state_64.update(&buf);
        assert_eq!(
            state_64.digest(),
            scalar_64,
            "Derived-secret XXH3_64 streaming len={}: diverged from scalar oracle",
            size
        );

        let mut state_128 = Xxh3_128State::new(seed);
        state_128.update(&buf);
        assert_eq!(
            state_128.digest(),
            scalar_128,
            "Derived-secret XXH3_128 streaming len={}: diverged from scalar oracle",
            size
        );
    }
}

// ============================================================================
// Chunked streaming parity for large inputs
// ============================================================================

#[test]
fn xxh3_simd_scalar_parity_streaming_various_chunk_sizes() {
    let buf = generate_test_buffer(10000);

    // Expectations come from the scalar oracle, not the optimized one-shot
    // API, so the streaming path is validated against an independent source.
    let scalar_64 = xxh3_64_scalar(&buf, 0);
    let scalar_128 = xxh3_128_scalar(&buf, 0);

    let chunk_sizes = [1, 7, 13, 63, 64, 65, 127, 128, 129, 255, 256, 1000, 2048];

    for &chunk_size in &chunk_sizes {
        let mut state_64 = Xxh3_64State::new(0);
        let mut state_128 = Xxh3_128State::new(0);

        for chunk in buf.chunks(chunk_size) {
            state_64.update(chunk);
            state_128.update(chunk);
        }

        assert_eq!(
            state_64.digest(),
            scalar_64,
            "XXH3_64 streaming chunk_size={}: diverged from scalar oracle",
            chunk_size
        );
        assert_eq!(
            state_128.digest(),
            scalar_128,
            "XXH3_128 streaming chunk_size={}: diverged from scalar oracle",
            chunk_size
        );
    }
}
