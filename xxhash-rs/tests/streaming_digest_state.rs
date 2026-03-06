//! Streaming digest state tests: verify non-destructive digest semantics
//! and update-after-digest parity for all four algorithms.
//!
//! This test file covers VAL-HASH-002 requirements:
//! - Repeated `digest()` calls on unchanged state stay stable.
//! - `update(A) -> digest() -> update(B)` matches one-shot hashing on `A+B`.

mod fixtures;

use fixtures::generate_test_buffer;
use xxhash_rs::xxh3::{xxh3_128, xxh3_64, Xxh3_128State, Xxh3_64State};
use xxhash_rs::xxh32::{xxh32, Xxh32State};
use xxhash_rs::xxh64::{xxh64, Xxh64State};

// ============================================================================
// Repeated digest stability tests
// ============================================================================

/// Verify that calling digest() multiple times on the same state returns
/// identical results for XXH32.
#[test]
fn streaming_digest_state_xxh32_repeated_digest() {
    for &len in &[0, 1, 5, 16, 31, 32, 64, 128, 256, 512, 1024] {
        let data = generate_test_buffer(len);
        let mut state = Xxh32State::new(0);
        state.update(&data);

        let d1 = state.digest();
        let d2 = state.digest();
        let d3 = state.digest();

        assert_eq!(d1, d2, "XXH32 len={}: digest instability d1 vs d2", len);
        assert_eq!(d2, d3, "XXH32 len={}: digest instability d2 vs d3", len);

        // Also verify it matches one-shot
        let expected = xxh32(&data, 0);
        assert_eq!(d1, expected, "XXH32 len={}: digest doesn't match one-shot", len);
    }
}

/// Verify that calling digest() multiple times on the same state returns
/// identical results for XXH64.
#[test]
fn streaming_digest_state_xxh64_repeated_digest() {
    for &len in &[0, 1, 5, 16, 31, 32, 64, 128, 256, 512, 1024] {
        let data = generate_test_buffer(len);
        let mut state = Xxh64State::new(0);
        state.update(&data);

        let d1 = state.digest();
        let d2 = state.digest();
        let d3 = state.digest();

        assert_eq!(d1, d2, "XXH64 len={}: digest instability d1 vs d2", len);
        assert_eq!(d2, d3, "XXH64 len={}: digest instability d2 vs d3", len);

        let expected = xxh64(&data, 0);
        assert_eq!(d1, expected, "XXH64 len={}: digest doesn't match one-shot", len);
    }
}

/// Verify that calling digest() multiple times on the same state returns
/// identical results for XXH3_64.
#[test]
fn streaming_digest_state_xxh3_64_repeated_digest() {
    for &len in &[0, 1, 5, 16, 31, 32, 64, 128, 240, 241, 256, 512, 1024] {
        let data = generate_test_buffer(len);
        let mut state = Xxh3_64State::new(0);
        state.update(&data);

        let d1 = state.digest();
        let d2 = state.digest();
        let d3 = state.digest();

        assert_eq!(d1, d2, "XXH3_64 len={}: digest instability d1 vs d2", len);
        assert_eq!(d2, d3, "XXH3_64 len={}: digest instability d2 vs d3", len);

        let expected = xxh3_64(&data, 0);
        assert_eq!(d1, expected, "XXH3_64 len={}: digest doesn't match one-shot", len);
    }
}

/// Verify that calling digest() multiple times on the same state returns
/// identical results for XXH3_128.
#[test]
fn streaming_digest_state_xxh3_128_repeated_digest() {
    for &len in &[0, 1, 5, 16, 31, 32, 64, 128, 240, 241, 256, 512, 1024] {
        let data = generate_test_buffer(len);
        let mut state = Xxh3_128State::new(0);
        state.update(&data);

        let d1 = state.digest();
        let d2 = state.digest();
        let d3 = state.digest();

        assert_eq!(d1, d2, "XXH3_128 len={}: digest instability d1 vs d2", len);
        assert_eq!(d2, d3, "XXH3_128 len={}: digest instability d2 vs d3", len);

        let expected = xxh3_128(&data, 0);
        assert_eq!(d1, expected, "XXH3_128 len={}: digest doesn't match one-shot", len);
    }
}

// ============================================================================
// Update-after-digest parity tests
// ============================================================================

/// Test update(A) -> digest() -> update(B) -> digest() == one-shot(A+B) for XXH32.
#[test]
fn streaming_digest_state_xxh32_update_after_digest() {
    let test_cases: &[(usize, usize)] = &[
        (0, 1),
        (1, 1),
        (3, 5),
        (8, 8),
        (15, 1),
        (16, 1),
        (16, 16),
        (31, 33),
        (100, 200),
        (256, 256),
        (500, 524),
    ];

    for &(len_a, len_b) in test_cases {
        let total = len_a + len_b;
        let full_data = generate_test_buffer(total);
        let part_a = &full_data[..len_a];
        let part_b = &full_data[len_a..];

        let mut state = Xxh32State::new(0);
        state.update(part_a);

        // Intermediate digest should match one-shot of A alone
        let digest_a = state.digest();
        let expected_a = xxh32(part_a, 0);
        assert_eq!(
            digest_a, expected_a,
            "XXH32 update-after-digest: intermediate digest mismatch for len_a={}",
            len_a
        );

        // Continue updating with B
        state.update(part_b);

        // Final digest should match one-shot of A+B
        let digest_ab = state.digest();
        let expected_ab = xxh32(&full_data, 0);
        assert_eq!(
            digest_ab, expected_ab,
            "XXH32 update-after-digest: final digest mismatch for len_a={} len_b={}",
            len_a, len_b
        );
    }
}

/// Test update(A) -> digest() -> update(B) -> digest() == one-shot(A+B) for XXH64.
#[test]
fn streaming_digest_state_xxh64_update_after_digest() {
    let test_cases: &[(usize, usize)] = &[
        (0, 1),
        (1, 1),
        (3, 5),
        (8, 8),
        (15, 1),
        (16, 16),
        (31, 1),
        (32, 1),
        (32, 32),
        (100, 200),
        (256, 256),
        (500, 524),
    ];

    for &(len_a, len_b) in test_cases {
        let total = len_a + len_b;
        let full_data = generate_test_buffer(total);
        let part_a = &full_data[..len_a];
        let part_b = &full_data[len_a..];

        let mut state = Xxh64State::new(0);
        state.update(part_a);

        let digest_a = state.digest();
        let expected_a = xxh64(part_a, 0);
        assert_eq!(
            digest_a, expected_a,
            "XXH64 update-after-digest: intermediate digest mismatch for len_a={}",
            len_a
        );

        state.update(part_b);

        let digest_ab = state.digest();
        let expected_ab = xxh64(&full_data, 0);
        assert_eq!(
            digest_ab, expected_ab,
            "XXH64 update-after-digest: final digest mismatch for len_a={} len_b={}",
            len_a, len_b
        );
    }
}

/// Test update(A) -> digest() -> update(B) -> digest() == one-shot(A+B) for XXH3_64.
#[test]
fn streaming_digest_state_xxh3_64_update_after_digest() {
    let test_cases: &[(usize, usize)] = &[
        (0, 1),
        (1, 1),
        (3, 5),
        (8, 8),
        (16, 1),
        (17, 17),
        (64, 64),
        (128, 1),
        (129, 1),
        (240, 1),
        (241, 1),
        (100, 200),
        (256, 256),
        (500, 524),
        (1024, 1024),
    ];

    for &(len_a, len_b) in test_cases {
        let total = len_a + len_b;
        let full_data = generate_test_buffer(total);
        let part_a = &full_data[..len_a];
        let part_b = &full_data[len_a..];

        let mut state = Xxh3_64State::new(0);
        state.update(part_a);

        let digest_a = state.digest();
        let expected_a = xxh3_64(part_a, 0);
        assert_eq!(
            digest_a, expected_a,
            "XXH3_64 update-after-digest: intermediate digest mismatch for len_a={}",
            len_a
        );

        state.update(part_b);

        let digest_ab = state.digest();
        let expected_ab = xxh3_64(&full_data, 0);
        assert_eq!(
            digest_ab, expected_ab,
            "XXH3_64 update-after-digest: final digest mismatch for len_a={} len_b={}",
            len_a, len_b
        );
    }
}

/// Test update(A) -> digest() -> update(B) -> digest() == one-shot(A+B) for XXH3_128.
#[test]
fn streaming_digest_state_xxh3_128_update_after_digest() {
    let test_cases: &[(usize, usize)] = &[
        (0, 1),
        (1, 1),
        (3, 5),
        (8, 8),
        (16, 1),
        (17, 17),
        (64, 64),
        (128, 1),
        (129, 1),
        (240, 1),
        (241, 1),
        (100, 200),
        (256, 256),
        (500, 524),
        (1024, 1024),
    ];

    for &(len_a, len_b) in test_cases {
        let total = len_a + len_b;
        let full_data = generate_test_buffer(total);
        let part_a = &full_data[..len_a];
        let part_b = &full_data[len_a..];

        let mut state = Xxh3_128State::new(0);
        state.update(part_a);

        let digest_a = state.digest();
        let expected_a = xxh3_128(part_a, 0);
        assert_eq!(
            digest_a, expected_a,
            "XXH3_128 update-after-digest: intermediate digest mismatch for len_a={}",
            len_a
        );

        state.update(part_b);

        let digest_ab = state.digest();
        let expected_ab = xxh3_128(&full_data, 0);
        assert_eq!(
            digest_ab, expected_ab,
            "XXH3_128 update-after-digest: final digest mismatch for len_a={} len_b={}",
            len_a, len_b
        );
    }
}

// ============================================================================
// Reset tests
// ============================================================================

/// Test that reset() restores the state to its initial condition for XXH32.
#[test]
fn streaming_digest_state_xxh32_reset() {
    let data = generate_test_buffer(100);
    let mut state = Xxh32State::new(42);

    state.update(&data);
    let _ = state.digest();

    state.reset();
    state.update(&data);
    let after_reset = state.digest();

    let expected = xxh32(&data, 42);
    assert_eq!(after_reset, expected, "XXH32: reset should restore initial state");
}

/// Test that reset() restores the state to its initial condition for XXH64.
#[test]
fn streaming_digest_state_xxh64_reset() {
    let data = generate_test_buffer(100);
    let mut state = Xxh64State::new(42);

    state.update(&data);
    let _ = state.digest();

    state.reset();
    state.update(&data);
    let after_reset = state.digest();

    let expected = xxh64(&data, 42);
    assert_eq!(after_reset, expected, "XXH64: reset should restore initial state");
}

/// Test that reset() restores the state to its initial condition for XXH3_64.
#[test]
fn streaming_digest_state_xxh3_64_reset() {
    let data = generate_test_buffer(300);
    let mut state = Xxh3_64State::new(42);

    state.update(&data);
    let _ = state.digest();

    state.reset();
    state.update(&data);
    let after_reset = state.digest();

    let expected = xxh3_64(&data, 42);
    assert_eq!(after_reset, expected, "XXH3_64: reset should restore initial state");
}

/// Test that reset() restores the state to its initial condition for XXH3_128.
#[test]
fn streaming_digest_state_xxh3_128_reset() {
    let data = generate_test_buffer(300);
    let mut state = Xxh3_128State::new(42);

    state.update(&data);
    let _ = state.digest();

    state.reset();
    state.update(&data);
    let after_reset = state.digest();

    let expected = xxh3_128(&data, 42);
    assert_eq!(after_reset, expected, "XXH3_128: reset should restore initial state");
}

// ============================================================================
// Reset with new seed tests
// ============================================================================

/// Test that reset_with_seed() changes the seed for XXH32.
#[test]
fn streaming_digest_state_xxh32_reset_with_seed() {
    let data = generate_test_buffer(100);
    let mut state = Xxh32State::new(0);

    state.update(&data);
    let _ = state.digest();

    state.reset_with_seed(0x12345678);
    state.update(&data);
    let after_reset = state.digest();

    let expected = xxh32(&data, 0x12345678);
    assert_eq!(after_reset, expected, "XXH32: reset_with_seed should use new seed");
}

/// Test that reset_with_seed() changes the seed for XXH64.
#[test]
fn streaming_digest_state_xxh64_reset_with_seed() {
    let data = generate_test_buffer(100);
    let mut state = Xxh64State::new(0);

    state.update(&data);
    let _ = state.digest();

    state.reset_with_seed(0x123456789ABCDEF0);
    state.update(&data);
    let after_reset = state.digest();

    let expected = xxh64(&data, 0x123456789ABCDEF0);
    assert_eq!(after_reset, expected, "XXH64: reset_with_seed should use new seed");
}

/// Test that reset_with_seed() changes the seed for XXH3_64.
#[test]
fn streaming_digest_state_xxh3_64_reset_with_seed() {
    let data = generate_test_buffer(300);
    let mut state = Xxh3_64State::new(0);

    state.update(&data);
    let _ = state.digest();

    state.reset_with_seed(0x9E3779B185EBCA8D);
    state.update(&data);
    let after_reset = state.digest();

    let expected = xxh3_64(&data, 0x9E3779B185EBCA8D);
    assert_eq!(after_reset, expected, "XXH3_64: reset_with_seed should use new seed");
}

/// Test that reset_with_seed() changes the seed for XXH3_128.
#[test]
fn streaming_digest_state_xxh3_128_reset_with_seed() {
    let data = generate_test_buffer(300);
    let mut state = Xxh3_128State::new(0);

    state.update(&data);
    let _ = state.digest();

    state.reset_with_seed(0x9E3779B185EBCA8D);
    state.update(&data);
    let after_reset = state.digest();

    let expected = xxh3_128(&data, 0x9E3779B185EBCA8D);
    assert_eq!(after_reset, expected, "XXH3_128: reset_with_seed should use new seed");
}

// ============================================================================
// Empty update tests
// ============================================================================

/// Verify that empty updates don't affect the state for any algorithm.
#[test]
fn streaming_digest_state_empty_updates() {
    let data = generate_test_buffer(100);

    // XXH32
    {
        let mut state = Xxh32State::new(0);
        state.update(b"");
        state.update(&data);
        state.update(b"");
        let got = state.digest();
        let expected = xxh32(&data, 0);
        assert_eq!(got, expected, "XXH32: empty updates shouldn't affect result");
    }

    // XXH64
    {
        let mut state = Xxh64State::new(0);
        state.update(b"");
        state.update(&data);
        state.update(b"");
        let got = state.digest();
        let expected = xxh64(&data, 0);
        assert_eq!(got, expected, "XXH64: empty updates shouldn't affect result");
    }

    // XXH3_64
    {
        let mut state = Xxh3_64State::new(0);
        state.update(b"");
        state.update(&data);
        state.update(b"");
        let got = state.digest();
        let expected = xxh3_64(&data, 0);
        assert_eq!(got, expected, "XXH3_64: empty updates shouldn't affect result");
    }

    // XXH3_128
    {
        let mut state = Xxh3_128State::new(0);
        state.update(b"");
        state.update(&data);
        state.update(b"");
        let got = state.digest();
        let expected = xxh3_128(&data, 0);
        assert_eq!(got, expected, "XXH3_128: empty updates shouldn't affect result");
    }
}

/// Verify that digest on a freshly created state matches one-shot on empty input.
#[test]
fn streaming_digest_state_fresh_digest() {
    // XXH32
    {
        let state = Xxh32State::new(0);
        assert_eq!(state.digest(), xxh32(&[], 0), "XXH32: fresh digest != one-shot empty");
    }

    // XXH64
    {
        let state = Xxh64State::new(0);
        assert_eq!(state.digest(), xxh64(&[], 0), "XXH64: fresh digest != one-shot empty");
    }

    // XXH3_64
    {
        let state = Xxh3_64State::new(0);
        assert_eq!(
            state.digest(),
            xxh3_64(&[], 0),
            "XXH3_64: fresh digest != one-shot empty"
        );
    }

    // XXH3_128
    {
        let state = Xxh3_128State::new(0);
        assert_eq!(
            state.digest(),
            xxh3_128(&[], 0),
            "XXH3_128: fresh digest != one-shot empty"
        );
    }
}
