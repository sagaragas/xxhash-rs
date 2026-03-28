//! One-shot reference parity conformance entrypoint.
//!
//! This test proves that all four one-shot hash algorithms match reference
//! outputs across the full boundary-length matrix and additional larger inputs.
//! It serves as the conformance surface required by VAL-HASH-001, producing
//! a per-algorithm pass summary.

#[allow(dead_code)]
mod fixtures;

use fixtures::{
    generate_test_buffer, load_all_vectors, load_vectors_for, reference, val_hash_001_lengths,
    Algorithm,
};
use xxhash_rs::xxh3::{xxh3_128, xxh3_64};
use xxhash_rs::xxh32::xxh32;
use xxhash_rs::xxh64::xxh64;

/// Hash using the appropriate Rust implementation and return hex string.
/// Uses lo||hi order for XXH3_128 (matching internal vector format).

macro_rules! skip_without_reference {
    () => {
        if fixtures::reference::reference_binary().is_none() {
            eprintln!("Skipped: reference binary not available (set XXHASH_REFERENCE_ROOT)");
            return;
        }
    };
}

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
            format!("{lo:016x}{hi:016x}")
        }
    }
}

/// Hash and return hex in the canonical (reference CLI) format.
/// For XXH3_128, the reference binary outputs hi||lo (big-endian canonical).
fn hash_with_algo_canonical(algo: Algorithm, data: &[u8], seed: u64) -> String {
    match algo {
        Algorithm::XXH3_128 => {
            let (lo, hi) = xxh3_128(data, seed);
            format!("{hi:016x}{lo:016x}")
        }
        _ => hash_with_algo(algo, data, seed),
    }
}

/// Full conformance entrypoint: verify all four algorithms against all known
/// vectors and the external reference binary, printing a per-algorithm summary.
#[test]
fn oneshot_reference_parity() {
    let (all_vectors, metadata) = load_all_vectors();
    let max_len = metadata.max_buffer_len;
    let buf = generate_test_buffer(max_len);

    println!("=== One-Shot Reference Parity Conformance ===");
    println!(
        "Source: {}, {} vectors across {:?}",
        metadata.source,
        metadata.vector_count,
        metadata
            .algorithms
            .iter()
            .map(|a| a.name())
            .collect::<Vec<_>>()
    );
    println!("Max buffer length: {max_len}");
    println!();

    let mut total_pass = 0;
    let mut total_fail = 0;

    for algo in [
        Algorithm::XXH32,
        Algorithm::XXH64,
        Algorithm::XXH3_64,
        Algorithm::XXH3_128,
    ] {
        let algo_vectors: Vec<_> = all_vectors.iter().filter(|v| v.algorithm == algo).collect();
        let mut pass = 0;
        let mut fail = 0;

        for v in &algo_vectors {
            let input = &buf[..v.len];
            let result_hex = hash_with_algo(algo, input, v.seed);
            if result_hex == v.expected_hex {
                pass += 1;
            } else {
                fail += 1;
                eprintln!(
                    "  FAIL: {} len={} seed={:#x}: got {result_hex}, expected {}",
                    algo.name(),
                    v.len,
                    v.seed,
                    v.expected_hex
                );
            }
        }

        println!(
            "{}: {}/{} vectors PASS{}",
            algo.name(),
            pass,
            algo_vectors.len(),
            if fail > 0 {
                format!(" ({fail} FAILED)")
            } else {
                String::new()
            }
        );

        total_pass += pass;
        total_fail += fail;
    }

    println!();
    println!(
        "Total: {total_pass}/{} vectors PASS, {total_fail} FAILED",
        total_pass + total_fail
    );

    assert_eq!(
        total_fail, 0,
        "All one-shot reference vector checks must pass"
    );
}

/// Reference binary parity: for each algorithm, hash the canonical test buffer
/// at several boundary lengths using both the Rust implementation and the
/// external reference binary, and verify they agree.
#[test]
fn oneshot_reference_parity_vs_reference_binary() {
    skip_without_reference!();
    let boundary_lengths = val_hash_001_lengths();
    let max_len = *boundary_lengths.iter().max().unwrap_or(&512);
    let buf = generate_test_buffer(max_len);

    // Ensure reference binary is available
    let ref_bin = reference::reference_binary();
    assert!(
        ref_bin.is_some(),
        "Reference binary must be available for parity testing"
    );

    println!("=== Reference Binary Parity ===");

    let mut total_pass = 0;
    let mut total_fail = 0;

    for algo in [
        Algorithm::XXH32,
        Algorithm::XXH64,
        Algorithm::XXH3_64,
        Algorithm::XXH3_128,
    ] {
        let mut pass = 0;
        let mut fail = 0;

        for &len in boundary_lengths {
            let input = &buf[..len];
            // Use canonical format for comparison against reference binary
            let rust_hex = hash_with_algo_canonical(algo, input, 0);

            // Hash with reference binary
            let result = reference::hash_stdin(input, algo.reference_flag(), &[]);
            match result {
                Ok(ref_result) => {
                    if let Some(ref ref_digest) = ref_result.digest {
                        if rust_hex == *ref_digest {
                            pass += 1;
                        } else {
                            fail += 1;
                            eprintln!(
                                "  FAIL: {} len={}: Rust={rust_hex}, Ref={ref_digest}",
                                algo.name(),
                                len
                            );
                        }
                    } else {
                        fail += 1;
                        eprintln!(
                            "  FAIL: {} len={}: no digest from reference (stdout: {})",
                            algo.name(),
                            len,
                            ref_result.stdout.trim()
                        );
                    }
                }
                Err(e) => {
                    fail += 1;
                    eprintln!(
                        "  FAIL: {} len={}: reference invocation failed: {e}",
                        algo.name(),
                        len
                    );
                }
            }
        }

        println!(
            "{}: {}/{} boundary lengths PASS vs reference binary{}",
            algo.name(),
            pass,
            boundary_lengths.len(),
            if fail > 0 {
                format!(" ({fail} FAILED)")
            } else {
                String::new()
            }
        );

        total_pass += pass;
        total_fail += fail;
    }

    println!();
    let total = total_pass + total_fail;
    println!("Total: {total_pass}/{total} boundary parity checks PASS, {total_fail} FAILED");

    assert_eq!(
        total_fail, 0,
        "All reference binary parity checks must pass"
    );
}

/// Seeded reference binary parity: verify that non-zero seeds produce
/// correct results against the reference binary.
#[test]
fn oneshot_reference_parity_seeded() {
    skip_without_reference!();
    let buf = generate_test_buffer(128);

    // Test a subset of lengths with non-zero seeds
    let test_cases: &[(Algorithm, usize, u64)] = &[
        (Algorithm::XXH32, 0, 0x9E3779B1),
        (Algorithm::XXH32, 16, 0x9E3779B1),
        (Algorithm::XXH32, 128, 0x9E3779B1),
        (Algorithm::XXH64, 0, 0x9E3779B1),
        (Algorithm::XXH64, 16, 0x9E3779B1),
        (Algorithm::XXH64, 128, 0x9E3779B1),
        (Algorithm::XXH3_64, 0, 0x9E3779B185EBCA8D),
        (Algorithm::XXH3_64, 16, 0x9E3779B185EBCA8D),
        (Algorithm::XXH3_64, 128, 0x9E3779B185EBCA8D),
    ];

    let ref_bin = reference::reference_binary();
    assert!(
        ref_bin.is_some(),
        "Reference binary must be available for seeded parity testing"
    );

    let mut pass = 0;
    let mut fail = 0;

    for &(algo, len, seed) in test_cases {
        let input = &buf[..len];
        let rust_hex = hash_with_algo_canonical(algo, input, seed);

        let seed_str = seed.to_string();
        let result =
            reference::hash_stdin(input, algo.reference_flag(), &["--seed", &seed_str]);

        match result {
            Ok(ref_result) => {
                if let Some(ref ref_digest) = ref_result.digest {
                    if rust_hex == *ref_digest {
                        pass += 1;
                    } else {
                        fail += 1;
                        eprintln!(
                            "  FAIL: {} len={} seed={:#x}: Rust={rust_hex}, Ref={ref_digest}",
                            algo.name(),
                            len,
                            seed
                        );
                    }
                } else {
                    fail += 1;
                    eprintln!(
                        "  FAIL: {} len={} seed={:#x}: no digest from reference",
                        algo.name(),
                        len,
                        seed
                    );
                }
            }
            Err(e) => {
                fail += 1;
                eprintln!(
                    "  FAIL: {} len={} seed={:#x}: reference error: {e}",
                    algo.name(),
                    len,
                    seed
                );
            }
        }
    }

    println!("Seeded parity: {pass}/{} PASS", pass + fail);
    assert_eq!(fail, 0, "All seeded reference parity checks must pass");
}

/// Verify that the full set of known vectors all have boundary lengths covered.
#[test]
fn oneshot_reference_parity_boundary_coverage() {
    let boundary_lengths = val_hash_001_lengths();

    for algo in [
        Algorithm::XXH32,
        Algorithm::XXH64,
        Algorithm::XXH3_64,
        Algorithm::XXH3_128,
    ] {
        let vectors = load_vectors_for(algo);
        let available: std::collections::HashSet<usize> = vectors.iter().map(|v| v.len).collect();

        for &len in boundary_lengths {
            assert!(
                available.contains(&len),
                "{}: missing boundary vector for len={len}",
                algo.name()
            );
        }
    }
}
