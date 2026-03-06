//! Streaming chunk parity tests: verify that streaming (chunked) hashing
//! produces the same result as one-shot hashing for all four algorithms
//! across multiple chunk plans.
//!
//! This test file covers VAL-HASH-002 requirements:
//! - Streaming `reset/update/digest` produces the same result as one-shot
//!   hashing for the same bytes across multiple chunking patterns.

mod fixtures;

use fixtures::generate_test_buffer;
use xxhash_rs::xxh3::{xxh3_128, xxh3_64, Xxh3_128State, Xxh3_64State};
use xxhash_rs::xxh32::{xxh32, Xxh32State};
use xxhash_rs::xxh64::{xxh64, Xxh64State};

/// Test lengths that exercise all code paths:
/// - Empty, tiny, small, medium, large, and multi-block inputs
/// - Boundary lengths from VAL-HASH-001 plus additional streaming-relevant sizes
const TEST_LENGTHS: &[usize] = &[
    0, 1, 3, 4, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 128, 129, 240, 241, 256, 300, 512,
    1023, 1024, 1025, 2048, 4096,
];

/// Chunk plans: different ways to split the input into pieces.
/// Each plan is a function that returns a list of chunk sizes for a given
/// total length.
fn chunk_plans(total_len: usize) -> Vec<(&'static str, Vec<usize>)> {
    let mut plans = Vec::new();

    // Plan 1: Single chunk (equivalent to one-shot)
    plans.push(("single", vec![total_len]));

    // Plan 2: One byte at a time
    plans.push(("byte_at_a_time", vec![1; total_len]));

    // Plan 3: Fixed 3-byte chunks
    if total_len > 0 {
        let mut chunks = Vec::new();
        let mut remaining = total_len;
        while remaining > 3 {
            chunks.push(3);
            remaining -= 3;
        }
        if remaining > 0 {
            chunks.push(remaining);
        }
        plans.push(("chunks_of_3", chunks));
    }

    // Plan 4: Fixed 7-byte chunks (non-power-of-2, misaligned)
    if total_len > 0 {
        let mut chunks = Vec::new();
        let mut remaining = total_len;
        while remaining > 7 {
            chunks.push(7);
            remaining -= 7;
        }
        if remaining > 0 {
            chunks.push(remaining);
        }
        plans.push(("chunks_of_7", chunks));
    }

    // Plan 5: Fixed 64-byte chunks (stripe-aligned)
    if total_len > 0 {
        let mut chunks = Vec::new();
        let mut remaining = total_len;
        while remaining > 64 {
            chunks.push(64);
            remaining -= 64;
        }
        if remaining > 0 {
            chunks.push(remaining);
        }
        plans.push(("chunks_of_64", chunks));
    }

    // Plan 6: Half-and-half split
    if total_len > 1 {
        let half = total_len / 2;
        plans.push(("half_split", vec![half, total_len - half]));
    }

    // Plan 7: Alternating small/large chunks
    if total_len > 10 {
        let mut chunks = Vec::new();
        let mut remaining = total_len;
        let mut small = true;
        while remaining > 0 {
            let size = if small {
                1.min(remaining)
            } else {
                100.min(remaining)
            };
            chunks.push(size);
            remaining -= size;
            small = !small;
        }
        plans.push(("alternating_1_100", chunks));
    }

    plans
}

// ============================================================================
// XXH32 streaming chunk parity
// ============================================================================

#[test]
fn streaming_chunk_parity_xxh32_seed0() {
    let seed: u32 = 0;
    for &len in TEST_LENGTHS {
        let data = generate_test_buffer(len);
        let expected = xxh32(&data, seed);

        for (plan_name, chunks) in chunk_plans(len) {
            let mut state = Xxh32State::new(seed);
            let mut offset = 0;
            for chunk_size in &chunks {
                state.update(&data[offset..offset + chunk_size]);
                offset += chunk_size;
            }
            let got = state.digest();
            assert_eq!(
                got, expected,
                "XXH32 seed=0 len={} plan={}: streaming={:#010X} one_shot={:#010X}",
                len, plan_name, got, expected
            );
        }
    }
}

#[test]
fn streaming_chunk_parity_xxh32_seeded() {
    let seed: u32 = 0x9E3779B1;
    for &len in TEST_LENGTHS {
        let data = generate_test_buffer(len);
        let expected = xxh32(&data, seed);

        for (plan_name, chunks) in chunk_plans(len) {
            let mut state = Xxh32State::new(seed);
            let mut offset = 0;
            for chunk_size in &chunks {
                state.update(&data[offset..offset + chunk_size]);
                offset += chunk_size;
            }
            let got = state.digest();
            assert_eq!(
                got, expected,
                "XXH32 seed={:#X} len={} plan={}: streaming={:#010X} one_shot={:#010X}",
                seed, len, plan_name, got, expected
            );
        }
    }
}

// ============================================================================
// XXH64 streaming chunk parity
// ============================================================================

#[test]
fn streaming_chunk_parity_xxh64_seed0() {
    let seed: u64 = 0;
    for &len in TEST_LENGTHS {
        let data = generate_test_buffer(len);
        let expected = xxh64(&data, seed);

        for (plan_name, chunks) in chunk_plans(len) {
            let mut state = Xxh64State::new(seed);
            let mut offset = 0;
            for chunk_size in &chunks {
                state.update(&data[offset..offset + chunk_size]);
                offset += chunk_size;
            }
            let got = state.digest();
            assert_eq!(
                got, expected,
                "XXH64 seed=0 len={} plan={}: streaming={:#018X} one_shot={:#018X}",
                len, plan_name, got, expected
            );
        }
    }
}

#[test]
fn streaming_chunk_parity_xxh64_seeded() {
    let seed: u64 = 0x000000009E3779B1;
    for &len in TEST_LENGTHS {
        let data = generate_test_buffer(len);
        let expected = xxh64(&data, seed);

        for (plan_name, chunks) in chunk_plans(len) {
            let mut state = Xxh64State::new(seed);
            let mut offset = 0;
            for chunk_size in &chunks {
                state.update(&data[offset..offset + chunk_size]);
                offset += chunk_size;
            }
            let got = state.digest();
            assert_eq!(
                got, expected,
                "XXH64 seed={:#X} len={} plan={}: streaming={:#018X} one_shot={:#018X}",
                seed, len, plan_name, got, expected
            );
        }
    }
}

// ============================================================================
// XXH3_64 streaming chunk parity
// ============================================================================

#[test]
fn streaming_chunk_parity_xxh3_64_seed0() {
    let seed: u64 = 0;
    for &len in TEST_LENGTHS {
        let data = generate_test_buffer(len);
        let expected = xxh3_64(&data, seed);

        for (plan_name, chunks) in chunk_plans(len) {
            let mut state = Xxh3_64State::new(seed);
            let mut offset = 0;
            for chunk_size in &chunks {
                state.update(&data[offset..offset + chunk_size]);
                offset += chunk_size;
            }
            let got = state.digest();
            assert_eq!(
                got, expected,
                "XXH3_64 seed=0 len={} plan={}: streaming={:#018X} one_shot={:#018X}",
                len, plan_name, got, expected
            );
        }
    }
}

#[test]
fn streaming_chunk_parity_xxh3_64_seeded() {
    let seed: u64 = 0x9E3779B185EBCA8D;
    for &len in TEST_LENGTHS {
        let data = generate_test_buffer(len);
        let expected = xxh3_64(&data, seed);

        for (plan_name, chunks) in chunk_plans(len) {
            let mut state = Xxh3_64State::new(seed);
            let mut offset = 0;
            for chunk_size in &chunks {
                state.update(&data[offset..offset + chunk_size]);
                offset += chunk_size;
            }
            let got = state.digest();
            assert_eq!(
                got, expected,
                "XXH3_64 seed={:#X} len={} plan={}: streaming={:#018X} one_shot={:#018X}",
                seed, len, plan_name, got, expected
            );
        }
    }
}

// ============================================================================
// XXH3_128 streaming chunk parity
// ============================================================================

#[test]
fn streaming_chunk_parity_xxh3_128_seed0() {
    let seed: u64 = 0;
    for &len in TEST_LENGTHS {
        let data = generate_test_buffer(len);
        let expected = xxh3_128(&data, seed);

        for (plan_name, chunks) in chunk_plans(len) {
            let mut state = Xxh3_128State::new(seed);
            let mut offset = 0;
            for chunk_size in &chunks {
                state.update(&data[offset..offset + chunk_size]);
                offset += chunk_size;
            }
            let got = state.digest();
            assert_eq!(
                got, expected,
                "XXH3_128 seed=0 len={} plan={}: streaming=({:#018X},{:#018X}) one_shot=({:#018X},{:#018X})",
                len, plan_name, got.0, got.1, expected.0, expected.1
            );
        }
    }
}

#[test]
fn streaming_chunk_parity_xxh3_128_seeded() {
    let seed: u64 = 0x9E3779B185EBCA8D;
    for &len in TEST_LENGTHS {
        let data = generate_test_buffer(len);
        let expected = xxh3_128(&data, seed);

        for (plan_name, chunks) in chunk_plans(len) {
            let mut state = Xxh3_128State::new(seed);
            let mut offset = 0;
            for chunk_size in &chunks {
                state.update(&data[offset..offset + chunk_size]);
                offset += chunk_size;
            }
            let got = state.digest();
            assert_eq!(
                got, expected,
                "XXH3_128 seed={:#X} len={} plan={}: streaming=({:#018X},{:#018X}) one_shot=({:#018X},{:#018X})",
                seed, len, plan_name, got.0, got.1, expected.0, expected.1
            );
        }
    }
}
