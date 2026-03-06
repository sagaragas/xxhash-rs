//! XXH3_128 vector tests: validate that `xxhash_rs::xxh3::xxh3_128()` matches
//! all known reference vectors from the BSD-licensed xxHash sanity data.

#[allow(dead_code)]
mod fixtures;

use fixtures::{generate_test_buffer, load_vectors_for, val_hash_001_lengths, Algorithm};
use xxhash_rs::xxh3::xxh3_128;

/// Format a 128-bit hash as a 32-char lowercase hex string (lo || hi).
fn format_128(lo: u64, hi: u64) -> String {
    format!("{lo:016x}{hi:016x}")
}

#[test]
fn xxh3_128_vectors_all_seed0() {
    let vectors = load_vectors_for(Algorithm::XXH3_128);
    let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
    let buf = generate_test_buffer(max_len);

    for v in vectors.iter().filter(|v| v.seed == 0) {
        let input = &buf[..v.len];
        let (lo, hi) = xxh3_128(input, 0);
        let result_hex = format_128(lo, hi);
        assert_eq!(
            result_hex, v.expected_hex,
            "XXH3_128 mismatch at len={}, seed=0: got {result_hex}, expected {}",
            v.len, v.expected_hex
        );
    }
}

#[test]
fn xxh3_128_vectors_all_seeded() {
    let vectors = load_vectors_for(Algorithm::XXH3_128);
    let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
    let buf = generate_test_buffer(max_len);

    for v in vectors.iter().filter(|v| v.seed != 0) {
        let input = &buf[..v.len];
        let (lo, hi) = xxh3_128(input, v.seed);
        let result_hex = format_128(lo, hi);
        assert_eq!(
            result_hex, v.expected_hex,
            "XXH3_128 mismatch at len={}, seed={:#x}: got {result_hex}, expected {}",
            v.len, v.seed, v.expected_hex
        );
    }
}

#[test]
fn xxh3_128_vectors_val_hash_001_lengths_covered() {
    let required = val_hash_001_lengths();
    let vectors = load_vectors_for(Algorithm::XXH3_128);
    let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
    let buf = generate_test_buffer(max_len);

    for &len in required {
        let v = vectors
            .iter()
            .find(|v| v.len == len && v.seed == 0)
            .unwrap_or_else(|| {
                panic!("XXH3_128 VAL-HASH-001: missing required seed=0 vector for len={len}")
            });
        let input = &buf[..v.len];
        let (lo, hi) = xxh3_128(input, 0);
        let result_hex = format_128(lo, hi);
        assert_eq!(
            result_hex, v.expected_hex,
            "XXH3_128 VAL-HASH-001 mismatch at len={len}: got {result_hex}, expected {}",
            v.expected_hex
        );
    }
}

#[test]
fn xxh3_128_vectors_empty_input() {
    let (lo, hi) = xxh3_128(&[], 0);
    assert_eq!(lo, 0x6001C324468D497F, "XXH3_128 empty low seed=0");
    assert_eq!(hi, 0x99AA06D3014798D8, "XXH3_128 empty high seed=0");

    let (lo, hi) = xxh3_128(&[], 0x9E3779B185EBCA8D);
    assert_eq!(
        lo, 0xA986DFC5D7605BFE,
        "XXH3_128 empty low seed=PRIME64_1"
    );
    assert_eq!(
        hi, 0x00FEAA732A3CE25E,
        "XXH3_128 empty high seed=PRIME64_1"
    );
}

/// Verify that the low 64 bits of XXH3_128 matches XXH3_64 for small inputs.
/// The spec states this is true for the 1-3 byte range.
#[test]
fn xxh3_128_low_matches_64_for_1to3() {
    let buf = generate_test_buffer(3);
    for len in 1..=3 {
        let input = &buf[..len];
        let result_64 = xxhash_rs::xxh3::xxh3_64(input, 0);
        let (lo_128, _) = xxh3_128(input, 0);
        assert_eq!(
            result_64, lo_128,
            "XXH3_64 should equal low half of XXH3_128 for len={len}"
        );
    }
}
