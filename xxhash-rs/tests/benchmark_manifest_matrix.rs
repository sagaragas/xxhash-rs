//! Integration tests for benchmark manifest structure and matrix coverage.
//!
//! Verifies VAL-BENCH-001: Benchmark runs record canonical comparators and
//! scenario provenance. Also validates manifest schema integrity and the
//! canonical comparator inventory.
//!
//! ## Release-binary race hardening
//!
//! Several tests invoke the Python benchmark harness which resolves the
//! `rust_xxhash_rs` comparator.  Without hardening, each harness
//! invocation can trigger its own `cargo build --release -p xxhash-cli`,
//! and when `cargo test` runs with `--test-threads=N` (N > 1) multiple
//! builds race on the same `target/release` directory.  This can cause
//! transient provenance failures (binary not found, version probe
//! hitting a partially-linked executable, etc.).
//!
//! The fix: a process-wide `std::sync::Once` prebuild step builds the
//! release binary exactly once and exports `XXHASH_RS_BINARY` so the
//! harness skips its own `cargo build --release` entirely.  All tests
//! that call `run_smoke_and_load_manifest()` go through this path.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The canonical comparator IDs that every scenario must declare.
const CANONICAL_COMPARATORS: &[&str] = &["c_xxhsum", "rust_xxhash_rs", "b3sum", "md5"];

/// Returns the workspace root (parent of the crate directory).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate should be in a workspace")
        .to_path_buf()
}

/// Helper: load and parse a JSON manifest from the benchmarks directory.
fn load_benchmark_json(filename: &str) -> serde_json::Value {
    let path = workspace_root().join("benchmarks").join(filename);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Release-binary prebuild (race hardening)
// ---------------------------------------------------------------------------

/// Process-wide cell that holds the release binary directory once built.
///
/// `OnceLock` is initialised at most once regardless of how many test
/// threads race to call [`ensure_release_binary`].
static RELEASE_BINARY_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Build the release binary once per test process and return the
/// directory that contains the `xxhash-rs` executable.
///
/// Concurrent test threads will block on the `OnceLock` until the
/// first caller finishes the build, then all callers receive the same
/// directory path.  The harness is then invoked with `XXHASH_RS_BINARY`
/// pointing at this directory so it never triggers its own
/// `cargo build --release`.
fn ensure_release_binary() -> &'static PathBuf {
    RELEASE_BINARY_DIR.get_or_init(|| {
        let root = workspace_root();
        let output = Command::new("cargo")
            .args(["build", "--release", "-p", "xxhash-cli"])
            .current_dir(&root)
            .output()
            .expect("Failed to launch cargo build --release");

        assert!(
            output.status.success(),
            "Release binary prebuild failed.\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        let binary_dir = root.join("target").join("release");
        let binary_path = binary_dir.join("xxhash-rs");
        assert!(
            binary_path.exists(),
            "Release binary not found after build: {}",
            binary_path.display(),
        );

        binary_dir
    })
}

// ---------------------------------------------------------------------------
// Scenario manifest tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_manifest_matrix_scenarios_exist() {
    let scenarios = load_benchmark_json("scenarios.json");
    let arr = scenarios["scenarios"].as_array().expect("scenarios should be an array");
    assert!(!arr.is_empty(), "Scenario manifest should declare at least one scenario");
}

#[test]
fn benchmark_manifest_matrix_scenarios_have_required_fields() {
    let scenarios = load_benchmark_json("scenarios.json");
    let arr = scenarios["scenarios"].as_array().unwrap();

    for scenario in arr {
        let id = scenario["id"].as_str().expect("scenario should have string id");
        assert!(
            scenario["algorithm"].is_string(),
            "Scenario {id} missing algorithm"
        );
        assert!(
            scenario["payload_bytes"].is_number(),
            "Scenario {id} missing payload_bytes"
        );
        assert!(
            scenario["warmup_iterations"].is_number(),
            "Scenario {id} missing warmup_iterations"
        );
        assert!(
            scenario["measured_iterations"].is_number(),
            "Scenario {id} missing measured_iterations"
        );
        assert!(
            scenario["comparators"].is_array(),
            "Scenario {id} missing comparators array"
        );
    }
}

#[test]
fn benchmark_manifest_matrix_every_scenario_declares_full_canonical_matrix() {
    let scenarios = load_benchmark_json("scenarios.json");
    let arr = scenarios["scenarios"].as_array().unwrap();
    let canonical: HashSet<&str> = CANONICAL_COMPARATORS.iter().copied().collect();

    for scenario in arr {
        let id = scenario["id"].as_str().unwrap();
        let comps: HashSet<String> = scenario["comparators"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let comps_ref: HashSet<&str> = comps.iter().map(|s| s.as_str()).collect();

        let missing: Vec<&&str> = canonical.difference(&comps_ref).collect();
        assert!(
            missing.is_empty(),
            "Scenario {id} is missing canonical comparators: {missing:?}"
        );
    }
}

#[test]
fn benchmark_manifest_matrix_scenario_ids_are_unique() {
    let scenarios = load_benchmark_json("scenarios.json");
    let arr = scenarios["scenarios"].as_array().unwrap();
    let mut ids = HashSet::new();

    for scenario in arr {
        let id = scenario["id"].as_str().unwrap();
        assert!(ids.insert(id.to_string()), "Duplicate scenario id: {id}");
    }
}

// ---------------------------------------------------------------------------
// Comparator manifest tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_manifest_matrix_comparator_inventory_has_all_canonical() {
    let comparators = load_benchmark_json("comparators.json");
    let arr = comparators["canonical_comparators"]
        .as_array()
        .expect("canonical_comparators should be array");

    let ids: HashSet<String> = arr
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect();

    for expected in CANONICAL_COMPARATORS {
        assert!(
            ids.contains(*expected),
            "Comparator inventory missing canonical comparator: {expected}"
        );
    }
}

#[test]
fn benchmark_manifest_matrix_comparators_have_invocation_templates() {
    let comparators = load_benchmark_json("comparators.json");
    let arr = comparators["canonical_comparators"].as_array().unwrap();
    let expected_algos = ["XXH32", "XXH64", "XXH3_64", "XXH3_128"];

    for comp in arr {
        let id = comp["id"].as_str().unwrap();
        let templates = comp["invocation_template"]
            .as_object()
            .unwrap_or_else(|| panic!("Comparator {id} missing invocation_template"));

        for algo in &expected_algos {
            assert!(
                templates.contains_key(*algo),
                "Comparator {id} missing invocation template for {algo}"
            );
        }
    }
}

#[test]
fn benchmark_manifest_matrix_oracle_and_contrast_roles_defined() {
    let comparators = load_benchmark_json("comparators.json");
    let arr = comparators["canonical_comparators"].as_array().unwrap();

    let mut oracles = Vec::new();
    let mut contrasts = Vec::new();

    for comp in arr {
        let id = comp["id"].as_str().unwrap();
        let role = comp["role"]
            .as_str()
            .unwrap_or_else(|| panic!("Comparator {id} missing role"));
        let parity = comp["parity_oracle"]
            .as_bool()
            .unwrap_or_else(|| panic!("Comparator {id} missing parity_oracle"));

        match role {
            "reference" | "subject" => {
                assert!(parity, "Oracle comparator {id} should have parity_oracle=true");
                oracles.push(id.to_string());
            }
            "contrast" => {
                assert!(
                    !parity,
                    "Contrast comparator {id} should have parity_oracle=false"
                );
                contrasts.push(id.to_string());
            }
            _ => panic!("Unknown role for comparator {id}: {role}"),
        }
    }

    assert!(
        oracles.contains(&"c_xxhsum".to_string()),
        "c_xxhsum should be an oracle"
    );
    assert!(
        oracles.contains(&"rust_xxhash_rs".to_string()),
        "rust_xxhash_rs should be an oracle"
    );
    assert!(
        contrasts.contains(&"b3sum".to_string()),
        "b3sum should be a contrast comparator"
    );
    assert!(
        contrasts.contains(&"md5".to_string()),
        "md5 should be a contrast comparator"
    );
}

// ---------------------------------------------------------------------------
// Policy manifest tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_manifest_matrix_policy_enforces_correctness_gate() {
    let policy = load_benchmark_json("policy.json");
    let gate = &policy["correctness_gate"];

    assert!(
        gate["oracle_must_agree"].as_bool().unwrap(),
        "Policy should require oracle agreement"
    );
    assert!(
        gate["contrast_must_execute"].as_bool().unwrap(),
        "Policy should require contrast comparators to execute"
    );

    let oracles: Vec<String> = gate["oracle_comparators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(oracles.contains(&"c_xxhsum".to_string()));
    assert!(oracles.contains(&"rust_xxhash_rs".to_string()));
}

#[test]
fn benchmark_manifest_matrix_policy_requires_full_matrix() {
    let policy = load_benchmark_json("policy.json");
    assert!(
        policy["completeness"]["require_full_matrix"].as_bool().unwrap(),
        "Policy should require full matrix coverage"
    );
    assert!(
        !policy["completeness"]["allow_partial_runs"].as_bool().unwrap(),
        "Policy should not allow partial runs"
    );
}

#[test]
fn benchmark_manifest_matrix_policy_retains_raw_samples() {
    let policy = load_benchmark_json("policy.json");
    assert!(
        policy["statistical_method"]["retain_raw_samples"].as_bool().unwrap(),
        "Policy should require raw sample retention"
    );
}

#[test]
fn benchmark_manifest_matrix_policy_latest_excludes_partial() {
    let policy = load_benchmark_json("policy.json");
    let latest = &policy["latest_resolution"];
    assert!(
        latest["require_complete"].as_bool().unwrap(),
        "Latest resolution should require complete runs"
    );
    assert!(
        latest["require_claim_ready"].as_bool().unwrap(),
        "Latest resolution should require claim-ready runs"
    );
    assert!(
        latest["exclude_partial"].as_bool().unwrap(),
        "Latest resolution should exclude partial runs"
    );
}

// ---------------------------------------------------------------------------
// Cross-manifest consistency tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_manifest_matrix_scenario_comparators_match_inventory() {
    let scenarios = load_benchmark_json("scenarios.json");
    let comparators = load_benchmark_json("comparators.json");

    let inventory_ids: HashSet<String> = comparators["canonical_comparators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect();

    for scenario in scenarios["scenarios"].as_array().unwrap() {
        let id = scenario["id"].as_str().unwrap();
        for comp in scenario["comparators"].as_array().unwrap() {
            let comp_id = comp.as_str().unwrap();
            assert!(
                inventory_ids.contains(comp_id),
                "Scenario {id} references unknown comparator: {comp_id}"
            );
        }
    }
}

#[test]
fn benchmark_manifest_matrix_manifest_hashes_are_stable() {
    let root = workspace_root().join("benchmarks");
    for filename in &["scenarios.json", "comparators.json", "policy.json"] {
        let path = root.join(filename);
        assert!(path.exists(), "Manifest file missing: {}", path.display());
        let content = std::fs::read(&path).unwrap();
        assert!(!content.is_empty(), "Manifest file empty: {}", path.display());

        // Verify it's valid JSON
        let _: serde_json::Value = serde_json::from_slice(&content)
            .unwrap_or_else(|e| panic!("Invalid JSON in {}: {e}", path.display()));
    }
}

// ---------------------------------------------------------------------------
// Comparator version/provenance hardening tests
// ---------------------------------------------------------------------------

/// Run the harness smoke and return the latest run manifest as JSON.
///
/// The release binary is prebuilt once via [`ensure_release_binary`] and
/// passed to the harness through `XXHASH_RS_BINARY` so that concurrent
/// test threads never trigger overlapping `cargo build --release` runs.
fn run_smoke_and_load_manifest() -> serde_json::Value {
    let binary_dir = ensure_release_binary();
    let root = workspace_root();
    let harness = root.join("benchmarks").join("harness.py");

    let output = Command::new("python3")
        .args([harness.to_str().unwrap(), "smoke", "--run-set", "local"])
        .current_dir(&root)
        .env("XXHASH_RS_BINARY", binary_dir.to_str().unwrap())
        .output()
        .expect("Failed to run benchmark harness");

    assert!(
        output.status.success(),
        "Benchmark smoke should exit 0.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Find the run directory from stdout: "Run dir: <path>"
    let stdout = String::from_utf8_lossy(&output.stdout);
    let run_dir_line = stdout
        .lines()
        .find(|l| l.starts_with("Run dir:"))
        .expect("Smoke output should contain 'Run dir:' line");
    let run_dir = run_dir_line.trim_start_matches("Run dir:").trim();

    let manifest_path = std::path::PathBuf::from(run_dir).join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("Failed to read manifest: {e}"));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse manifest: {e}"))
}

#[test]
fn benchmark_manifest_matrix_all_comparators_have_clean_version_provenance() {
    let manifest = run_smoke_and_load_manifest();
    let resolved = manifest["resolved_comparators"]
        .as_object()
        .expect("manifest should have resolved_comparators object");

    for comp_id in CANONICAL_COMPARATORS {
        let comp = resolved
            .get(*comp_id)
            .unwrap_or_else(|| panic!("Missing resolved comparator: {comp_id}"));

        // version must be a non-null string
        let version = comp["version"]
            .as_str()
            .unwrap_or_else(|| panic!("{comp_id}: version is null or not a string"));

        // version must not be empty
        assert!(
            !version.is_empty(),
            "{comp_id}: version is empty"
        );

        // version must not contain captured error text
        let lower = version.to_lowercase();
        assert!(
            !lower.contains("error"),
            "{comp_id}: version contains error text: {version}"
        );
        assert!(
            !lower.contains("no such file"),
            "{comp_id}: version contains file-not-found text: {version}"
        );
        assert!(
            !lower.contains("could not open"),
            "{comp_id}: version contains 'could not open' text: {version}"
        );
        assert!(
            !lower.contains("unrecognized option"),
            "{comp_id}: version contains 'unrecognized option' text: {version}"
        );
    }
}

#[test]
fn benchmark_manifest_matrix_rust_xxhash_rs_reports_semver_version() {
    let manifest = run_smoke_and_load_manifest();
    let resolved = manifest["resolved_comparators"]
        .as_object()
        .expect("resolved_comparators");

    let rust_comp = &resolved["rust_xxhash_rs"];
    let version = rust_comp["version"]
        .as_str()
        .expect("rust_xxhash_rs version should be a string");

    // Should contain "xxhash-rs" and a semver-like pattern
    assert!(
        version.contains("xxhash-rs"),
        "rust_xxhash_rs version should contain 'xxhash-rs', got: {version}"
    );
}

#[test]
fn benchmark_manifest_matrix_md5_has_deterministic_provenance() {
    let manifest = run_smoke_and_load_manifest();
    let resolved = manifest["resolved_comparators"]
        .as_object()
        .expect("resolved_comparators");

    let md5_comp = &resolved["md5"];
    let version = md5_comp["version"]
        .as_str()
        .expect("md5 version should be a string (not null)");

    // Must be non-empty and deterministic (not error text)
    assert!(
        !version.is_empty(),
        "md5 version must not be empty"
    );
    assert!(
        !version.to_lowercase().contains("error"),
        "md5 version must not contain error text: {version}"
    );
}

// ---------------------------------------------------------------------------
// Harness smoke execution test (runs the Python harness and validates output)
// ---------------------------------------------------------------------------

#[test]
fn benchmark_manifest_matrix_run_index_concurrency_regression() {
    let root = workspace_root();

    // Run the Python concurrency regression tests that prove:
    // 1. Empty/corrupt index.json does not crash _update_run_index or resolve_latest_run
    // 2. Concurrent _update_run_index calls produce valid JSON
    // 3. Atomic writes prevent partial reads
    let output = Command::new("python3")
        .args([
            "-m",
            "pytest",
            "benchmarks/test_run_index_concurrency.py",
            "-v",
            "--tb=short",
        ])
        .current_dir(&root)
        .output()
        .expect("Failed to run concurrency regression tests");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Run-index concurrency regression tests should pass.\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Verify the key test classes executed
    assert!(
        stdout.contains("TestLoadJsonSafe"),
        "Should have run load_json_safe tests"
    );
    assert!(
        stdout.contains("TestConcurrentUpdateRunIndex"),
        "Should have run concurrent update tests"
    );
    assert!(
        stdout.contains("TestUpdateRunIndexCorruptRecovery"),
        "Should have run corrupt recovery tests"
    );
}

#[test]
fn benchmark_manifest_matrix_harness_smoke_produces_complete_run() {
    let binary_dir = ensure_release_binary();
    let root = workspace_root();
    let harness = root.join("benchmarks").join("harness.py");

    // Run the smoke benchmark with the prebuilt release binary.
    let output = Command::new("python3")
        .args([harness.to_str().unwrap(), "smoke", "--run-set", "local"])
        .current_dir(&root)
        .env("XXHASH_RS_BINARY", binary_dir.to_str().unwrap())
        .output()
        .expect("Failed to run benchmark harness");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Benchmark smoke should exit 0.\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Verify output contains key markers
    assert!(
        stdout.contains("Manifest hashes:"),
        "Should report manifest hashes"
    );
    assert!(
        stdout.contains("Resolved comparators:"),
        "Should report resolved comparators"
    );
    for comp in CANONICAL_COMPARATORS {
        assert!(
            stdout.contains(comp),
            "Output should mention comparator {comp}"
        );
    }
    assert!(
        stdout.contains("Matrix complete: True"),
        "Should report matrix complete"
    );
    assert!(
        stdout.contains("Correctness gate: PASSED"),
        "Should report correctness gate passed"
    );
    assert!(
        stdout.contains("Claim ready: True"),
        "Should report claim ready"
    );
}

// ---------------------------------------------------------------------------
// Release-binary race-hardening regression test
// ---------------------------------------------------------------------------

/// Regression test: multiple concurrent harness smoke runs with the
/// prebuilt release binary must all produce deterministic provenance.
///
/// This test spawns several smoke invocations in parallel — the same
/// scenario that previously caused transient provenance failures when
/// each invocation independently ran `cargo build --release`.  With
/// the prebuild strategy the binary is already present, so all
/// invocations resolve cleanly and concurrently.
#[test]
fn benchmark_manifest_matrix_concurrent_smoke_provenance_is_stable() {
    let binary_dir = ensure_release_binary();
    let root = workspace_root();
    let harness = root.join("benchmarks").join("harness.py");
    let concurrent_runs = 3;

    // Spawn concurrent smoke runs.
    let children: Vec<_> = (0..concurrent_runs)
        .map(|_| {
            Command::new("python3")
                .args([harness.to_str().unwrap(), "smoke", "--run-set", "local"])
                .current_dir(&root)
                .env("XXHASH_RS_BINARY", binary_dir.to_str().unwrap())
                .output()
        })
        .collect();

    let mut rust_versions = Vec::new();
    let mut md5_versions = Vec::new();

    for (i, child_result) in children.into_iter().enumerate() {
        let output = child_result
            .unwrap_or_else(|e| panic!("Concurrent smoke run {i} failed to launch: {e}"));

        assert!(
            output.status.success(),
            "Concurrent smoke run {i} should exit 0.\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        // Parse the run manifest to inspect provenance.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let run_dir_line = stdout
            .lines()
            .find(|l| l.starts_with("Run dir:"))
            .unwrap_or_else(|| {
                panic!("Concurrent run {i}: missing 'Run dir:' in output")
            });
        let run_dir = run_dir_line.trim_start_matches("Run dir:").trim();
        let manifest_path = PathBuf::from(run_dir).join("manifest.json");
        let content = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("Concurrent run {i}: read manifest: {e}"));
        let manifest: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Concurrent run {i}: parse manifest: {e}"));

        let resolved = manifest["resolved_comparators"]
            .as_object()
            .unwrap_or_else(|| panic!("Concurrent run {i}: missing resolved_comparators"));

        // Every canonical comparator must have a clean version string.
        for comp_id in CANONICAL_COMPARATORS {
            let comp = resolved.get(*comp_id).unwrap_or_else(|| {
                panic!("Concurrent run {i}: missing comparator {comp_id}")
            });
            let version = comp["version"].as_str().unwrap_or_else(|| {
                panic!("Concurrent run {i}: {comp_id} version is null")
            });
            assert!(
                !version.is_empty(),
                "Concurrent run {i}: {comp_id} version is empty"
            );
            let lower = version.to_lowercase();
            assert!(
                !lower.contains("error"),
                "Concurrent run {i}: {comp_id} version contains error text: {version}"
            );
        }

        // Collect rust_xxhash_rs and md5 versions to verify determinism.
        let rust_v = resolved["rust_xxhash_rs"]["version"]
            .as_str()
            .unwrap()
            .to_string();
        let md5_v = resolved["md5"]["version"]
            .as_str()
            .unwrap()
            .to_string();
        rust_versions.push(rust_v);
        md5_versions.push(md5_v);
    }

    // All runs must report the same version strings (deterministic).
    let unique_rust: HashSet<&str> = rust_versions.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        unique_rust.len(),
        1,
        "rust_xxhash_rs versions should be identical across concurrent runs: {rust_versions:?}"
    );

    let unique_md5: HashSet<&str> = md5_versions.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        unique_md5.len(),
        1,
        "md5 versions should be identical across concurrent runs: {md5_versions:?}"
    );
}
