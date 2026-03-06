//! Unified boundary-length matrix test for all four one-shot hash algorithms.
//!
//! This test exercises every algorithm at the exact edge lengths specified in
//! VAL-HASH-001: 0, 1, 3, 4, 8, 9, 16, 17, 128, 129, 240, 241, plus at least
//! one long-input case (512). Each length is tested with both seed=0 and a
//! non-zero seed, and the output is compared against known reference vectors.

#[allow(dead_code)]
mod fixtures;

use fixtures::{generate_test_buffer, load_vectors_for, val_hash_001_lengths, Algorithm};
use xxhash_rs::xxh3::{xxh3_128, xxh3_64};
use xxhash_rs::xxh32::xxh32;
use xxhash_rs::xxh64::xxh64;

/// Format helper for XXH3_128: lo || hi as 32-char lowercase hex.
fn format_128(lo: u64, hi: u64) -> String {
    format!("{lo:016x}{hi:016x}")
}

/// Hash using the appropriate Rust implementation and return hex string.
fn hash_with_algo(algo: Algorithm, data: &[u8], seed: u64) -> String {
    match algo {
        Algorithm::XXH32 => {
            let h = xxh32(data, seed as u32);
            format!("{h:08x}")
        }
        Algorithm::XXH64 => {
            let h = xxh64(data, seed);
            format!("{h:016x}")
        }
        Algorithm::XXH3_64 => {
            let h = xxh3_64(data, seed);
            format!("{h:016x}")
        }
        Algorithm::XXH3_128 => {
            let (lo, hi) = xxh3_128(data, seed);
            format_128(lo, hi)
        }
    }
}

/// Core boundary matrix test: for each algorithm and each VAL-HASH-001 length,
/// verify the Rust implementation matches the known reference vector.
#[test]
fn hash_vectors_boundary_matrix() {
    let boundary_lengths = val_hash_001_lengths();

    for algo in [
        Algorithm::XXH32,
        Algorithm::XXH64,
        Algorithm::XXH3_64,
        Algorithm::XXH3_128,
    ] {
        let vectors = load_vectors_for(algo);
        let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
        let buf = generate_test_buffer(max_len);

        let mut tested_count = 0;

        for &len in boundary_lengths {
            // Test seed=0 vectors
            for v in vectors.iter().filter(|v| v.len == len && v.seed == 0) {
                let input = &buf[..v.len];
                let result_hex = hash_with_algo(algo, input, v.seed);
                assert_eq!(
                    result_hex, v.expected_hex,
                    "{} boundary mismatch at len={}, seed=0: got {result_hex}, expected {}",
                    algo.name(),
                    v.len,
                    v.expected_hex
                );
                tested_count += 1;
            }

            // Test seeded vectors at this length
            for v in vectors.iter().filter(|v| v.len == len && v.seed != 0) {
                let input = &buf[..v.len];
                let result_hex = hash_with_algo(algo, input, v.seed);
                assert_eq!(
                    result_hex, v.expected_hex,
                    "{} boundary mismatch at len={}, seed={:#x}: got {result_hex}, expected {}",
                    algo.name(),
                    v.len,
                    v.seed,
                    v.expected_hex
                );
                tested_count += 1;
            }
        }

        // Ensure we actually tested some vectors for each algorithm
        assert!(
            tested_count >= boundary_lengths.len(),
            "{}: expected at least {} boundary vectors tested, got {}",
            algo.name(),
            boundary_lengths.len(),
            tested_count,
        );
    }
}

/// Verify that every algorithm has vectors covering all required boundary lengths.
#[test]
fn hash_vectors_boundary_matrix_coverage() {
    let boundary_lengths = val_hash_001_lengths();

    for algo in [
        Algorithm::XXH32,
        Algorithm::XXH64,
        Algorithm::XXH3_64,
        Algorithm::XXH3_128,
    ] {
        let vectors = load_vectors_for(algo);
        let available_lengths: std::collections::HashSet<usize> =
            vectors.iter().map(|v| v.len).collect();

        for &len in boundary_lengths {
            assert!(
                available_lengths.contains(&len),
                "{} is missing a vector for boundary length {} (required by VAL-HASH-001)",
                algo.name(),
                len
            );
        }
    }
}

/// Verify that all four algorithms produce correct output for all available
/// vectors (not just boundary lengths).
#[test]
fn hash_vectors_boundary_matrix_full_vectors() {
    for algo in [
        Algorithm::XXH32,
        Algorithm::XXH64,
        Algorithm::XXH3_64,
        Algorithm::XXH3_128,
    ] {
        let vectors = load_vectors_for(algo);
        let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
        let buf = generate_test_buffer(max_len);

        for v in &vectors {
            let input = &buf[..v.len];
            let result_hex = hash_with_algo(algo, input, v.seed);
            assert_eq!(
                result_hex, v.expected_hex,
                "{} full-vector mismatch at len={}, seed={:#x}: got {result_hex}, expected {}",
                algo.name(),
                v.len,
                v.seed,
                v.expected_hex
            );
        }
    }
}
