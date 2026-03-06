//! XXH3_64 vector tests: validate that `xxhash_rs::xxh3::xxh3_64()` matches
//! all known reference vectors from the BSD-licensed xxHash sanity data.

#[allow(dead_code)]
mod fixtures;

use fixtures::{generate_test_buffer, load_vectors_for, val_hash_001_lengths, Algorithm};
use xxhash_rs::xxh3::xxh3_64;

#[test]
fn xxh3_64_vectors_all_seed0() {
    let vectors = load_vectors_for(Algorithm::XXH3_64);
    let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
    let buf = generate_test_buffer(max_len);

    for v in vectors.iter().filter(|v| v.seed == 0) {
        let input = &buf[..v.len];
        let result = xxh3_64(input, 0);
        let result_hex = format!("{result:016x}");
        assert_eq!(
            result_hex, v.expected_hex,
            "XXH3_64 mismatch at len={}, seed=0: got {result_hex}, expected {}",
            v.len, v.expected_hex
        );
    }
}

#[test]
fn xxh3_64_vectors_all_seeded() {
    let vectors = load_vectors_for(Algorithm::XXH3_64);
    let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
    let buf = generate_test_buffer(max_len);

    for v in vectors.iter().filter(|v| v.seed != 0) {
        let input = &buf[..v.len];
        let result = xxh3_64(input, v.seed);
        let result_hex = format!("{result:016x}");
        assert_eq!(
            result_hex, v.expected_hex,
            "XXH3_64 mismatch at len={}, seed={:#x}: got {result_hex}, expected {}",
            v.len, v.seed, v.expected_hex
        );
    }
}

#[test]
fn xxh3_64_vectors_val_hash_001_lengths_covered() {
    let required = val_hash_001_lengths();
    let vectors = load_vectors_for(Algorithm::XXH3_64);
    let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
    let buf = generate_test_buffer(max_len);

    for &len in required {
        if let Some(v) = vectors.iter().find(|v| v.len == len && v.seed == 0) {
            let input = &buf[..v.len];
            let result = xxh3_64(input, 0);
            let result_hex = format!("{result:016x}");
            assert_eq!(
                result_hex, v.expected_hex,
                "XXH3_64 VAL-HASH-001 mismatch at len={len}: got {result_hex}, expected {}",
                v.expected_hex
            );
        }
    }
}

#[test]
fn xxh3_64_vectors_empty_input() {
    let result = xxh3_64(&[], 0);
    assert_eq!(result, 0x2D06800538D394C2, "XXH3_64 empty input seed=0");

    let result = xxh3_64(&[], 0x9E3779B185EBCA8D);
    assert_eq!(
        result, 0xA8A6B918B2F0364A,
        "XXH3_64 empty input seed=PRIME64_1"
    );
}
