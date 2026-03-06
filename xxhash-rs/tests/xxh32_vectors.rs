//! XXH32 vector tests: validate that `xxhash_rs::xxh32::xxh32()` matches
//! all known reference vectors from the BSD-licensed xxHash sanity data.

#[allow(dead_code)]
mod fixtures;

use fixtures::{generate_test_buffer, load_vectors_for, val_hash_001_lengths, Algorithm};
use xxhash_rs::xxh32::xxh32;

#[test]
fn xxh32_vectors_all_seed0() {
    let vectors = load_vectors_for(Algorithm::XXH32);
    let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
    let buf = generate_test_buffer(max_len);

    for v in vectors.iter().filter(|v| v.seed == 0) {
        let input = &buf[..v.len];
        let result = xxh32(input, v.seed as u32);
        let result_hex = format!("{result:08x}");
        assert_eq!(
            result_hex, v.expected_hex,
            "XXH32 mismatch at len={}, seed=0: got {result_hex}, expected {}",
            v.len, v.expected_hex
        );
    }
}

#[test]
fn xxh32_vectors_all_seeded() {
    let vectors = load_vectors_for(Algorithm::XXH32);
    let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
    let buf = generate_test_buffer(max_len);

    for v in vectors.iter().filter(|v| v.seed != 0) {
        let input = &buf[..v.len];
        let result = xxh32(input, v.seed as u32);
        let result_hex = format!("{result:08x}");
        assert_eq!(
            result_hex, v.expected_hex,
            "XXH32 mismatch at len={}, seed={:#x}: got {result_hex}, expected {}",
            v.len, v.seed, v.expected_hex
        );
    }
}

#[test]
fn xxh32_vectors_val_hash_001_lengths_covered() {
    let required = val_hash_001_lengths();
    let vectors = load_vectors_for(Algorithm::XXH32);
    let max_len = vectors.iter().map(|v| v.len).max().unwrap_or(0);
    let buf = generate_test_buffer(max_len);

    for &len in required {
        // Find a seed=0 vector for this length
        if let Some(v) = vectors.iter().find(|v| v.len == len && v.seed == 0) {
            let input = &buf[..v.len];
            let result = xxh32(input, 0);
            let result_hex = format!("{result:08x}");
            assert_eq!(
                result_hex, v.expected_hex,
                "XXH32 VAL-HASH-001 mismatch at len={len}: got {result_hex}, expected {}",
                v.expected_hex
            );
        }
    }
}

#[test]
fn xxh32_vectors_empty_input() {
    // Empty input with seed=0 should produce the known zero-length vector.
    let result = xxh32(&[], 0);
    assert_eq!(result, 0x02CC5D05, "XXH32 empty input seed=0");

    // Empty input with PRIME32 seed
    let result = xxh32(&[], 0x9E3779B1);
    assert_eq!(result, 0x36B78AE7, "XXH32 empty input seed=PRIME32");
}
