//! CLI algorithm selection and seed behavior tests.
//!
//! Validates VAL-HASH-003: The CLI defaults to XXH64; -H0/32, -H1/64,
//! -H2/128, and -H3 select the expected algorithm; explicit seed 0 matches
//! the default path; non-zero seeds produce reference-compatible digests.

use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Returns the path to the Rust CLI binary built by cargo.
fn rust_binary() -> PathBuf {
    // cargo test sets CARGO_BIN_EXE_<name> for binaries in the same package.
    // For integration tests in the same crate, use env var or fall back to
    // cargo-built path.
    env!("CARGO_BIN_EXE_xxhash-rs").into()
}

/// Default path to the external reference checkout.
const DEFAULT_REFERENCE_ROOT: &str = "/Users/ragas/code/rewrites/xxhash-reference";

/// Returns the path to the reference `xxhsum` binary, if available.
fn reference_binary() -> Option<PathBuf> {
    let root = env::var("XXHASH_REFERENCE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_REFERENCE_ROOT));
    let bin = root.join("xxhsum");
    if bin.exists() {
        Some(bin)
    } else {
        None
    }
}

/// Run the Rust CLI with the given args and stdin data, returning (stdout, stderr, exit_code).
fn run_rust_cli(args: &[&str], stdin_data: &[u8]) -> (String, String, i32) {
    let bin = rust_binary();
    let mut cmd = Command::new(&bin);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("failed to spawn Rust CLI");

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(stdin_data).expect("failed to write stdin");
    }

    let output = child.wait_with_output().expect("failed to wait for Rust CLI");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

/// Run the reference CLI with the given args and stdin data, returning (stdout, stderr, exit_code).
fn run_reference_cli(args: &[&str], stdin_data: &[u8]) -> (String, String, i32) {
    let bin = reference_binary().expect("reference binary not found");
    let mut cmd = Command::new(&bin);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("failed to spawn reference CLI");

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(stdin_data).expect("failed to write stdin");
    }

    let output = child
        .wait_with_output()
        .expect("failed to wait for reference CLI");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

/// Extract the digest from a GNU-style output line: "hash  filename\n"
/// Handles the XXH3_ prefix by stripping it before returning.
fn extract_digest(output: &str) -> String {
    let line = output.lines().next().expect("no output line");
    let digest = line.split_whitespace().next().expect("no digest token");
    digest
        .strip_prefix("XXH3_")
        .unwrap_or(digest)
        .to_lowercase()
}

/// Extract the full first output line (for format comparison).
fn first_line(output: &str) -> &str {
    output.lines().next().unwrap_or("")
}

// =========================================================================
// Default algorithm is XXH64
// =========================================================================

#[test]
fn cli_algorithm_selection_default_is_xxh64() {
    let data = b"hello\n";
    let (rust_out, _, rust_code) = run_rust_cli(&[], data);
    let (ref_out, _, ref_code) = run_reference_cli(&["-H1"], data);

    assert_eq!(rust_code, 0, "Rust CLI should succeed");
    assert_eq!(ref_code, 0, "Reference CLI should succeed");

    // Default Rust output should match -H1 (XXH64) of reference
    assert_eq!(
        extract_digest(&rust_out),
        extract_digest(&ref_out),
        "Default should be XXH64: rust={}, ref={}",
        first_line(&rust_out),
        first_line(&ref_out)
    );
}

#[test]
fn cli_algorithm_selection_default_matches_reference_default() {
    let data = b"hello\n";
    let (rust_out, _, _) = run_rust_cli(&[], data);
    let (ref_out, _, _) = run_reference_cli(&[], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "Default output lines should match exactly"
    );
}

// =========================================================================
// -H0 and -H32 select XXH32
// =========================================================================

#[test]
fn cli_algorithm_selection_h0_is_xxh32() {
    let data = b"hello\n";
    let (rust_out, _, rust_code) = run_rust_cli(&["-H0"], data);
    let (ref_out, _, ref_code) = run_reference_cli(&["-H0"], data);

    assert_eq!(rust_code, 0);
    assert_eq!(ref_code, 0);
    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "-H0 output should match reference"
    );
}

#[test]
fn cli_algorithm_selection_h32_is_xxh32() {
    let data = b"hello\n";
    let (rust_out, _, _) = run_rust_cli(&["-H32"], data);
    let (ref_out, _, _) = run_reference_cli(&["-H32"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "-H32 output should match reference"
    );
}

#[test]
fn cli_algorithm_selection_h0_and_h32_agree() {
    let data = b"test data\n";
    let (h0_out, _, _) = run_rust_cli(&["-H0"], data);
    let (h32_out, _, _) = run_rust_cli(&["-H32"], data);

    assert_eq!(
        extract_digest(&h0_out),
        extract_digest(&h32_out),
        "-H0 and -H32 should select the same algorithm"
    );
}

// =========================================================================
// -H1 and -H64 select XXH64
// =========================================================================

#[test]
fn cli_algorithm_selection_h1_is_xxh64() {
    let data = b"hello\n";
    let (rust_out, _, _) = run_rust_cli(&["-H1"], data);
    let (ref_out, _, _) = run_reference_cli(&["-H1"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "-H1 output should match reference"
    );
}

#[test]
fn cli_algorithm_selection_h64_is_xxh64() {
    let data = b"hello\n";
    let (rust_out, _, _) = run_rust_cli(&["-H64"], data);
    let (ref_out, _, _) = run_reference_cli(&["-H64"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "-H64 output should match reference"
    );
}

// =========================================================================
// -H2 and -H128 select XXH3_128
// =========================================================================

#[test]
fn cli_algorithm_selection_h2_is_xxh3_128() {
    let data = b"hello\n";
    let (rust_out, _, _) = run_rust_cli(&["-H2"], data);
    let (ref_out, _, _) = run_reference_cli(&["-H2"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "-H2 output should match reference"
    );
}

#[test]
fn cli_algorithm_selection_h128_is_xxh3_128() {
    let data = b"hello\n";
    let (rust_out, _, _) = run_rust_cli(&["-H128"], data);
    let (ref_out, _, _) = run_reference_cli(&["-H128"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "-H128 output should match reference"
    );
}

// =========================================================================
// -H3 selects XXH3_64
// =========================================================================

#[test]
fn cli_algorithm_selection_h3_is_xxh3_64() {
    let data = b"hello\n";
    let (rust_out, _, _) = run_rust_cli(&["-H3"], data);
    let (ref_out, _, _) = run_reference_cli(&["-H3"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "-H3 output should match reference"
    );
}

#[test]
fn cli_algorithm_selection_h3_has_xxh3_prefix() {
    let data = b"hello\n";
    let (rust_out, _, _) = run_rust_cli(&["-H3"], data);

    let line = first_line(&rust_out);
    assert!(
        line.starts_with("XXH3_"),
        "XXH3_64 output should have XXH3_ prefix, got: {}",
        line
    );
}

// =========================================================================
// Algorithms produce distinct outputs from each other
// =========================================================================

#[test]
fn cli_algorithm_selection_all_algorithms_produce_distinct_digests() {
    let data = b"hello world\n";

    let (h0, _, _) = run_rust_cli(&["-H0"], data);
    let (h1, _, _) = run_rust_cli(&["-H1"], data);
    let (h2, _, _) = run_rust_cli(&["-H2"], data);
    let (h3, _, _) = run_rust_cli(&["-H3"], data);

    let digests: Vec<String> = [&h0, &h1, &h2, &h3]
        .iter()
        .map(|o| extract_digest(o))
        .collect();

    // All digests should be unique
    for i in 0..digests.len() {
        for j in (i + 1)..digests.len() {
            assert_ne!(
                digests[i], digests[j],
                "Algorithms {} and {} should produce different digests",
                i, j
            );
        }
    }
}

// =========================================================================
// Seed behavior: seed 0 matches default, non-zero seeds produce different output
// =========================================================================

#[test]
fn cli_algorithm_selection_seed_0_matches_default() {
    let data = b"hello\n";

    // Default (no --seed)
    let (default_out, _, _) = run_rust_cli(&[], data);
    // Explicit --seed 0
    let (seed0_out, _, _) = run_rust_cli(&["--seed", "0"], data);

    assert_eq!(
        first_line(&default_out),
        first_line(&seed0_out),
        "Explicit seed 0 should match default output"
    );
}

#[test]
fn cli_algorithm_selection_seed_0_matches_reference_default() {
    let data = b"hello\n";

    let (rust_out, _, _) = run_rust_cli(&["--seed", "0"], data);
    let (ref_out, _, _) = run_reference_cli(&["--seed", "0"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "Seed 0 should match reference default"
    );
}

#[test]
fn cli_algorithm_selection_nonzero_seed_xxh64() {
    let data = b"hello\n";

    let (rust_out, _, _) = run_rust_cli(&["--seed", "42"], data);
    let (ref_out, _, _) = run_reference_cli(&["--seed", "42"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "Non-zero seed XXH64 should match reference"
    );

    // Also verify it's different from the default
    let (default_out, _, _) = run_rust_cli(&[], data);
    assert_ne!(
        extract_digest(&rust_out),
        extract_digest(&default_out),
        "Non-zero seed should produce different digest than default"
    );
}

#[test]
fn cli_algorithm_selection_nonzero_seed_xxh32() {
    let data = b"hello\n";

    let (rust_out, _, _) = run_rust_cli(&["-H0", "--seed", "42"], data);
    let (ref_out, _, _) = run_reference_cli(&["-H0", "--seed", "42"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "Non-zero seed XXH32 should match reference"
    );
}

#[test]
fn cli_algorithm_selection_nonzero_seed_xxh3_128() {
    let data = b"hello\n";

    let (rust_out, _, _) = run_rust_cli(&["-H2", "--seed", "42"], data);
    let (ref_out, _, _) = run_reference_cli(&["-H2", "--seed", "42"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "Non-zero seed XXH3_128 should match reference"
    );
}

#[test]
fn cli_algorithm_selection_nonzero_seed_xxh3_64() {
    let data = b"hello\n";

    let (rust_out, _, _) = run_rust_cli(&["-H3", "--seed", "42"], data);
    let (ref_out, _, _) = run_reference_cli(&["-H3", "--seed", "42"], data);

    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "Non-zero seed XXH3_64 should match reference"
    );
}

#[test]
fn cli_algorithm_selection_seed_0_all_algorithms_match_reference() {
    let data = b"test data for all algorithms\n";

    for (flag, name) in &[
        ("-H0", "XXH32"),
        ("-H1", "XXH64"),
        ("-H2", "XXH3_128"),
        ("-H3", "XXH3_64"),
    ] {
        let (rust_out, _, _) = run_rust_cli(&[flag, "--seed", "0"], data);
        let (ref_out, _, _) = run_reference_cli(&[flag, "--seed", "0"], data);

        assert_eq!(
            first_line(&rust_out),
            first_line(&ref_out),
            "{} with seed 0 should match reference: rust={}, ref={}",
            name,
            first_line(&rust_out),
            first_line(&ref_out)
        );

        // Also verify it matches the default (no seed) for that algorithm
        let (default_out, _, _) = run_rust_cli(&[flag], data);
        assert_eq!(
            extract_digest(&rust_out),
            extract_digest(&default_out),
            "{} seed 0 should match default",
            name
        );
    }
}

// =========================================================================
// XXH32 seed overflow behavior
// =========================================================================

#[test]
fn cli_algorithm_selection_xxh32_seed_u32_max_rejected_for_parity() {
    // u32::MAX = 4294967295 is rejected by the reference CLI's conservative
    // decimal parser (seeds >= 4294967290 are rejected). Our CLI must match.
    let data = b"hello\n";
    let (_, rust_err, rust_code) = run_rust_cli(&["-H0", "--seed", "4294967295"], data);
    assert_eq!(
        rust_code, 1,
        "XXH32 with seed u32::MAX should be rejected for reference parity, stderr: {}",
        rust_err
    );
    assert!(
        rust_err.contains("numeric value too large"),
        "Error should contain 'numeric value too large', got: {}",
        rust_err
    );
}

#[test]
fn cli_algorithm_selection_xxh32_seed_overflow_rejected() {
    // u32::MAX + 1 = 4294967296 should be rejected for XXH32
    let data = b"hello\n";
    let (_, rust_err, rust_code) = run_rust_cli(&["-H0", "--seed", "4294967296"], data);
    assert_eq!(
        rust_code, 1,
        "XXH32 with seed u32::MAX+1 should exit 1"
    );
    assert!(
        rust_err.contains("numeric value too large"),
        "Error should contain 'numeric value too large', got: {}",
        rust_err
    );
}

#[test]
fn cli_algorithm_selection_xxh32_seed_large_overflow_rejected() {
    // A very large seed should be rejected for XXH32
    let data = b"hello\n";
    let (_, rust_err, rust_code) =
        run_rust_cli(&["-H0", "--seed", "18446744073709551615"], data);
    assert_eq!(
        rust_code, 1,
        "XXH32 with u64::MAX seed should exit 1"
    );
    assert!(
        rust_err.contains("numeric value too large"),
        "Error should contain 'numeric value too large', got: {}",
        rust_err
    );
}

#[test]
fn cli_algorithm_selection_xxh32_seed_overflow_parity_with_reference() {
    // Both Rust and reference should reject the same out-of-range seed for XXH32
    let data = b"hello\n";

    let (_, rust_err, rust_code) = run_rust_cli(&["-H0", "--seed", "4294967296"], data);
    let (_, ref_err, ref_code) = run_reference_cli(&["-H0", "--seed", "4294967296"], data);

    assert_eq!(
        rust_code, ref_code,
        "Rust and reference should agree on exit code for overflow seed"
    );
    assert_eq!(rust_code, 1, "Both should exit 1");
    assert!(
        rust_err.contains("numeric value too large"),
        "Rust error should match reference pattern, got: {}",
        rust_err
    );
    assert!(
        ref_err.contains("numeric value too large"),
        "Reference error should match pattern, got: {}",
        ref_err
    );
}

#[test]
fn cli_algorithm_selection_xxh64_large_seed_still_accepted() {
    // XXH64 should accept seeds > u32::MAX since it uses a u64 seed
    let data = b"hello\n";
    let (rust_out, _, rust_code) = run_rust_cli(&["-H1", "--seed", "4294967296"], data);
    let (ref_out, _, ref_code) = run_reference_cli(&["-H1", "--seed", "4294967296"], data);
    assert_eq!(rust_code, 0, "XXH64 should accept u64 seed");
    assert_eq!(ref_code, 0, "Reference XXH64 should accept u64 seed");
    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "XXH64 large seed output should match reference"
    );
}

#[test]
fn cli_algorithm_selection_xxh32_boundary_seeds_match_reference() {
    // Valid boundary seeds for XXH32 should produce matching output
    let data = b"hello\n";

    for seed in &["0", "1", "2147483647", "2147483648"] {
        let (rust_out, _, rust_code) = run_rust_cli(&["-H0", "--seed", seed], data);
        let (ref_out, _, ref_code) = run_reference_cli(&["-H0", "--seed", seed], data);

        assert_eq!(
            rust_code, 0,
            "XXH32 with seed {} should succeed",
            seed
        );
        assert_eq!(
            ref_code, 0,
            "Reference with seed {} should succeed",
            seed
        );
        assert_eq!(
            first_line(&rust_out),
            first_line(&ref_out),
            "XXH32 seed {} output should match reference",
            seed
        );
    }
}

// =========================================================================
// XXH32 seed overflow boundary — reference-compatible rejection at 4294967290+
// =========================================================================
// The reference CLI's conservative decimal parser rejects seeds where the
// intermediate value exceeds UINT32_MAX / 10 (= 429496729), meaning decimal
// seeds >= 4294967290 are rejected even though they fit in a u32.  Our CLI
// must reproduce this exact boundary for parity.

#[test]
fn cli_algorithm_selection_xxh32_seed_boundary_4294967289_accepted() {
    // Last accepted seed before the reference overflow boundary.
    let data = b"hello\n";
    let (rust_out, rust_err, rust_code) =
        run_rust_cli(&["-H0", "--seed", "4294967289"], data);
    let (ref_out, _, ref_code) =
        run_reference_cli(&["-H0", "--seed", "4294967289"], data);
    assert_eq!(
        rust_code, 0,
        "Seed 4294967289 should succeed, stderr: {}",
        rust_err
    );
    assert_eq!(ref_code, 0, "Reference should also accept 4294967289");
    assert_eq!(
        first_line(&rust_out),
        first_line(&ref_out),
        "Seed 4294967289 output should match reference"
    );
}

#[test]
fn cli_algorithm_selection_xxh32_seed_boundary_4294967290_rejected() {
    // First rejected seed — reference overflow boundary.
    let data = b"hello\n";
    let (_, rust_err, rust_code) =
        run_rust_cli(&["-H0", "--seed", "4294967290"], data);
    let (_, ref_err, ref_code) =
        run_reference_cli(&["-H0", "--seed", "4294967290"], data);
    assert_eq!(
        rust_code, 1,
        "Seed 4294967290 should be rejected, stderr: {}",
        rust_err
    );
    assert_eq!(ref_code, 1, "Reference should reject 4294967290");
    assert!(
        rust_err.contains("numeric value too large"),
        "Rust error should say 'numeric value too large', got: {}",
        rust_err
    );
    assert!(
        ref_err.contains("numeric value too large"),
        "Reference error should say 'numeric value too large', got: {}",
        ref_err
    );
}

#[test]
fn cli_algorithm_selection_xxh32_seed_boundary_4294967295_rejected() {
    // u32::MAX itself is rejected by the reference's conservative parser.
    let data = b"hello\n";
    let (_, rust_err, rust_code) =
        run_rust_cli(&["-H0", "--seed", "4294967295"], data);
    let (_, ref_err, ref_code) =
        run_reference_cli(&["-H0", "--seed", "4294967295"], data);
    assert_eq!(
        rust_code, 1,
        "Seed u32::MAX should be rejected for parity, stderr: {}",
        rust_err
    );
    assert_eq!(ref_code, 1);
    assert!(rust_err.contains("numeric value too large"));
    assert!(ref_err.contains("numeric value too large"));
}

#[test]
fn cli_algorithm_selection_xxh32_seed_overflow_range_parity() {
    // The full rejected range 4294967290..=4294967295 must be rejected.
    let data = b"hello\n";
    for seed in 4294967290u64..=4294967295 {
        let seed_str = seed.to_string();
        let (_, rust_err, rust_code) =
            run_rust_cli(&["-H0", "--seed", &seed_str], data);
        let (_, _, ref_code) =
            run_reference_cli(&["-H0", "--seed", &seed_str], data);
        assert_eq!(
            rust_code, 1,
            "Seed {} should be rejected",
            seed
        );
        assert_eq!(
            ref_code, 1,
            "Reference should reject seed {}",
            seed
        );
        assert!(
            rust_err.contains("numeric value too large"),
            "Seed {} error should contain 'numeric value too large', got: {}",
            seed,
            rust_err
        );
    }
}

#[test]
fn cli_algorithm_selection_xxh32_seed_accepted_range_parity() {
    // Seeds just below the boundary should succeed and match reference.
    let data = b"hello\n";
    for seed in &["4294967285", "4294967286", "4294967287", "4294967288", "4294967289"] {
        let (rust_out, rust_err, rust_code) =
            run_rust_cli(&["-H0", "--seed", seed], data);
        let (ref_out, _, ref_code) =
            run_reference_cli(&["-H0", "--seed", seed], data);
        assert_eq!(
            rust_code, 0,
            "Seed {} should succeed, stderr: {}",
            seed, rust_err
        );
        assert_eq!(ref_code, 0, "Reference should accept seed {}", seed);
        assert_eq!(
            first_line(&rust_out),
            first_line(&ref_out),
            "Seed {} output should match reference",
            seed
        );
    }
}

// =========================================================================
// Exit codes
// =========================================================================

#[test]
fn cli_algorithm_selection_all_algorithms_exit_0() {
    let data = b"hello\n";

    for flag in &["-H0", "-H1", "-H2", "-H3", "-H32", "-H64", "-H128"] {
        let (_, _, code) = run_rust_cli(&[flag], data);
        assert_eq!(code, 0, "{} should exit 0 on valid input", flag);
    }
}
