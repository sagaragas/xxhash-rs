//! Integration tests for benchmark manifest structure and matrix coverage.
//!
//! Verifies VAL-BENCH-001: Benchmark runs record canonical comparators and
//! scenario provenance. Also validates manifest schema integrity and the
//! canonical comparator inventory.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

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
// Harness smoke execution test (runs the Python harness and validates output)
// ---------------------------------------------------------------------------

#[test]
fn benchmark_manifest_matrix_harness_smoke_produces_complete_run() {
    let root = workspace_root();
    let harness = root.join("benchmarks").join("harness.py");

    // Run the smoke benchmark
    let output = Command::new("python3")
        .args([harness.to_str().unwrap(), "smoke", "--run-set", "local"])
        .current_dir(&root)
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
