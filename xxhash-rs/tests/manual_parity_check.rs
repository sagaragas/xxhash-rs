//! Manual parity checks comparing the Rust implementation against the
//! external reference binary on sample inputs.
//!
//! These tests invoke the reference binary and compare its output to
//! the Rust implementation for the same inputs, serving as a cross-check
//! beyond the hardcoded test vectors.

#[allow(dead_code)]
mod fixtures;

use fixtures::reference;
use xxhash_rs::xxh32::xxh32;
use xxhash_rs::xxh64::xxh64;

/// Helper: hash data with Rust and reference, assert they match.
fn assert_parity_xxh32(data: &[u8], seed: u32, description: &str) {
    let rust_hash = xxh32(data, seed);
    let rust_hex = format!("{rust_hash:08x}");

    let seed_args: Vec<String>;
    let extra: Vec<&str>;
    if seed != 0 {
        seed_args = vec!["--seed".to_string(), seed.to_string()];
        extra = seed_args.iter().map(|s| s.as_str()).collect();
    } else {
        extra = vec![];
    }

    let result = reference::hash_stdin(data, "-H0", &extra)
        .unwrap_or_else(|e| panic!("Reference invocation failed for {description}: {e}"));

    let ref_hex = result
        .digest
        .expect("Reference should produce a digest");

    assert_eq!(
        rust_hex, ref_hex,
        "XXH32 parity mismatch for {description}: Rust={rust_hex}, Ref={ref_hex}"
    );
}

fn assert_parity_xxh64(data: &[u8], seed: u64, description: &str) {
    let rust_hash = xxh64(data, seed);
    let rust_hex = format!("{rust_hash:016x}");

    let seed_args: Vec<String>;
    let extra: Vec<&str>;
    if seed != 0 {
        seed_args = vec!["--seed".to_string(), seed.to_string()];
        extra = seed_args.iter().map(|s| s.as_str()).collect();
    } else {
        extra = vec![];
    }

    let result = reference::hash_stdin(data, "-H1", &extra)
        .unwrap_or_else(|e| panic!("Reference invocation failed for {description}: {e}"));

    let ref_hex = result
        .digest
        .expect("Reference should produce a digest");

    assert_eq!(
        rust_hex, ref_hex,
        "XXH64 parity mismatch for {description}: Rust={rust_hex}, Ref={ref_hex}"
    );
}

// ============================================================================
// XXH32 parity tests
// ============================================================================

#[test]
fn parity_xxh32_empty() {
    assert_parity_xxh32(b"", 0, "empty input seed=0");
}

#[test]
fn parity_xxh32_short_string() {
    assert_parity_xxh32(b"test input data", 0, "\"test input data\" seed=0");
}

#[test]
fn parity_xxh32_hello_world() {
    assert_parity_xxh32(b"hello world", 0, "\"hello world\" seed=0");
}

#[test]
fn parity_xxh32_seeded() {
    assert_parity_xxh32(b"hello world", 42, "\"hello world\" seed=42");
}

#[test]
fn parity_xxh32_longer_input() {
    let data: Vec<u8> = (0..256).map(|i| (i & 0xFF) as u8).collect();
    assert_parity_xxh32(&data, 0, "256 bytes sequential seed=0");
}

// ============================================================================
// XXH64 parity tests
// ============================================================================

#[test]
fn parity_xxh64_empty() {
    assert_parity_xxh64(b"", 0, "empty input seed=0");
}

#[test]
fn parity_xxh64_short_string() {
    assert_parity_xxh64(b"test input data", 0, "\"test input data\" seed=0");
}

#[test]
fn parity_xxh64_hello_world() {
    assert_parity_xxh64(b"hello world", 0, "\"hello world\" seed=0");
}

#[test]
fn parity_xxh64_seeded() {
    assert_parity_xxh64(b"hello world", 42, "\"hello world\" seed=42");
}

#[test]
fn parity_xxh64_longer_input() {
    let data: Vec<u8> = (0..256).map(|i| (i & 0xFF) as u8).collect();
    assert_parity_xxh64(&data, 0, "256 bytes sequential seed=0");
}
