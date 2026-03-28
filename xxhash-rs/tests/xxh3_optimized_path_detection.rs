//! Optimized-path-detection tests for XXH3 SIMD acceleration.
//!
//! Verifies that the SIMD dispatch layer correctly identifies and activates
//! the expected optimized path on the current platform, and that the
//! optimized path is actually being exercised in release builds.
//!
//! Expectations are validated against independent scalar oracles (which
//! bypass SIMD dispatch) and the external reference binary, so a
//! tautological pass — where the same optimized code path computes both
//! the actual and expected values — is no longer possible.
//!
//! Covers the feature requirement:
//! "SIMD-specific tests or diagnostics prove the optimized path is active
//! without weakening correctness."

#[allow(dead_code)]
mod fixtures;

use fixtures::generate_test_buffer;
use xxhash_rs::xxh3::{xxh3_64, xxh3_128};
use xxhash_rs::xxh3::tests_public::{xxh3_64_scalar, xxh3_128_scalar};
use xxhash_rs::xxh3_simd::active_simd_path;

// ============================================================================
// Path identification
// ============================================================================

macro_rules! skip_without_reference {
    () => {
        if fixtures::reference::reference_binary().is_none() {
            eprintln!("Skipped: reference binary not available (set XXHASH_REFERENCE_ROOT)");
            return;
        }
    };
}


#[test]
fn xxh3_optimized_path_detection_reports_valid_path() {
    let path = active_simd_path();
    let valid_paths = ["neon", "sse2", "avx2", "scalar"];
    assert!(
        valid_paths.contains(&path),
        "active_simd_path() returned unexpected value: {:?}",
        path
    );
}

#[test]
fn xxh3_optimized_path_detection_is_neon_on_aarch64() {
    if cfg!(target_arch = "aarch64") {
        assert_eq!(
            active_simd_path(),
            "neon",
            "Expected NEON path on aarch64 (Apple Silicon)"
        );
    }
}

#[test]
fn xxh3_optimized_path_detection_is_simd_on_x86_64() {
    if cfg!(target_arch = "x86_64") {
        let path = active_simd_path();
        assert!(
            path == "sse2" || path == "avx2",
            "Expected SSE2 or AVX2 on x86_64, got: {:?}",
            path
        );
    }
}

#[test]
fn xxh3_optimized_path_detection_not_scalar_on_supported_arch() {
    if cfg!(target_arch = "aarch64") || cfg!(target_arch = "x86_64") {
        assert_ne!(
            active_simd_path(),
            "scalar",
            "Expected an optimized SIMD path on this architecture, but got scalar"
        );
    }
}

// ============================================================================
// Diagnostic output: path activation evidence
// ============================================================================

/// This test prints diagnostic information that proves the optimized path
/// is active. When run with `--nocapture`, the output serves as evidence
/// for VAL-HASH-007.
///
/// Expectations come from the **scalar-only** oracle (`xxh3_*_scalar`),
/// which bypasses SIMD dispatch entirely, so a green result proves the
/// optimized path produces the same digest as the scalar reference.
#[test]
fn xxh3_optimized_path_detection_diagnostic_transcript() {
    let path = active_simd_path();
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "other"
    };

    // Compute hashes via the (potentially SIMD-accelerated) public API.
    let buf = generate_test_buffer(100_000);
    let hash_64 = xxh3_64(&buf, 0);
    let (lo_128, hi_128) = xxh3_128(&buf, 0);

    // Independent scalar oracle — never touches SIMD dispatch.
    let scalar_64 = xxh3_64_scalar(&buf, 0);
    let scalar_128 = xxh3_128_scalar(&buf, 0);

    eprintln!("=== XXH3 SIMD Path Diagnostic ===");
    eprintln!("  Host architecture: {}", arch);
    eprintln!("  Active SIMD path:  {}", path);
    eprintln!("  Test input size:   100,000 bytes");
    eprintln!("  XXH3_64  (opt):    {:016X}", hash_64);
    eprintln!("  XXH3_64  (scalar): {:016X}", scalar_64);
    eprintln!("  XXH3_128 (opt):    {:016X}{:016X}", hi_128, lo_128);
    eprintln!("  XXH3_128 (scalar): {:016X}{:016X}", scalar_128.1, scalar_128.0);
    eprintln!("=================================");

    assert_eq!(
        hash_64, scalar_64,
        "XXH3_64 100KB: optimized path diverged from scalar oracle"
    );
    assert_eq!(
        (lo_128, hi_128),
        scalar_128,
        "XXH3_128 100KB: optimized path diverged from scalar oracle"
    );
}

// ============================================================================
// Large-input correctness under optimized path
// ============================================================================

#[test]
fn xxh3_optimized_path_detection_known_vectors_large_inputs() {
    // Known vectors for large inputs (from xxh3 unit tests).
    // Each assertion first checks against the hardcoded vector, then
    // cross-checks the scalar oracle agrees — so the optimized path
    // is validated against two independent sources.
    let sizes_and_expected: &[(usize, u64)] = &[
        (241, 0xC5A639ECD2030E5E),
        (256, 0x55DE574AD89D0AC5),
        (512, 0x617E49599013CB6B),
    ];
    for &(size, expected) in sizes_and_expected {
        let buf = generate_test_buffer(size);
        let opt = xxh3_64(&buf, 0);
        let scalar = xxh3_64_scalar(&buf, 0);
        assert_eq!(
            opt, expected,
            "XXH3_64 len={}: optimized path vs hardcoded vector",
            size
        );
        assert_eq!(
            scalar, expected,
            "XXH3_64 len={}: scalar oracle vs hardcoded vector",
            size
        );
    }
}

#[test]
fn xxh3_optimized_path_detection_seeded_large_inputs() {
    let seed = 0x9E3779B185EBCA8D_u64;
    let buf = generate_test_buffer(241);
    let opt = xxh3_64(&buf, seed);
    let scalar = xxh3_64_scalar(&buf, seed);
    assert_eq!(
        opt, 0xDDA9B0A161D4829A,
        "XXH3_64 seeded len=241: optimized vs hardcoded vector"
    );
    assert_eq!(
        scalar, 0xDDA9B0A161D4829A,
        "XXH3_64 seeded len=241: scalar oracle vs hardcoded vector"
    );
}

// ============================================================================
// Reference binary parity on large input
// ============================================================================

/// Reference-binary parity for large inputs.
///
/// Spawn/status failures are now **explicit**: if the reference binary
/// cannot be spawned or returns non-zero, the test fails with a
/// diagnostic message instead of silently passing.
#[test]
fn xxh3_optimized_path_detection_reference_parity_large() {
    skip_without_reference!();
    use fixtures::reference;

    let ref_bin = match reference::reference_binary() {
        Some(bin) => bin,
        None => {
            eprintln!("Skipping reference parity check: reference binary not available");
            return;
        }
    };

    let sizes = [1024, 4096, 65536];
    for &size in &sizes {
        let buf = generate_test_buffer(size);

        // Write test buffer to a temp file
        let tmp_dir = std::env::temp_dir();
        let tmp_file = tmp_dir.join(format!("xxh3_simd_parity_{}.bin", size));
        std::fs::write(&tmp_file, &buf).expect("failed to write temp file");

        // Hash with reference binary (XXH3_64 = -H3)
        let output = std::process::Command::new(&ref_bin)
            .args(["-H3", tmp_file.to_str().unwrap()])
            .output()
            .unwrap_or_else(|e| {
                panic!(
                    "Failed to spawn reference binary {:?} for len={}: {}",
                    ref_bin, size, e
                )
            });

        assert!(
            output.status.success(),
            "Reference binary returned non-zero for len={}: status={}, stderr={}",
            size,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_token = stdout
            .split_whitespace()
            .next()
            .unwrap_or_else(|| panic!("Empty stdout from reference binary for len={}", size));
        let ref_hash_hex = first_token.strip_prefix("XXH3_").unwrap_or(first_token);

        // Compare optimized path against reference CLI.
        let opt_hash = xxh3_64(&buf, 0);
        let opt_hex = format!("{:016x}", opt_hash);
        assert_eq!(
            ref_hash_hex, opt_hex,
            "Reference parity mismatch (optimized) at len={}: ref={} opt={}",
            size, ref_hash_hex, opt_hex
        );

        // Also cross-check the scalar oracle against the reference.
        let scalar_hash = xxh3_64_scalar(&buf, 0);
        let scalar_hex = format!("{:016x}", scalar_hash);
        assert_eq!(
            ref_hash_hex, scalar_hex,
            "Reference parity mismatch (scalar) at len={}: ref={} scalar={}",
            size, ref_hash_hex, scalar_hex
        );

        let _ = std::fs::remove_file(&tmp_file);
    }
}
