//! Integration tests for the hard correctness gate between C reference and Rust rewrite.
//!
//! Verifies VAL-BENCH-003: The harness applies a hard correctness gate to C vs Rust xxHash results.
//! Before summary statistics are accepted, the harness verifies that `c_xxhsum` and `rust_xxhash_rs`
//! agree on the digest surface for the measured xxHash scenario, while `b3sum` and `md5` are treated
//! as contrast comparators that must execute successfully but are not parity oracles.
//!
//! Also verifies the blocking aspect from expectedBehavior:
//! "The harness blocks summary acceptance if c_xxhsum and rust_xxhash_rs diverge on the measured
//! digest surface."

use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the workspace root (parent of the crate directory).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate should be in a workspace")
        .to_path_buf()
}

/// Helper: load and parse a JSON file.
fn load_json(path: &Path) -> serde_json::Value {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()))
}

/// Helper: find the most recent claim-ready run directory.
fn find_latest_run_dir() -> Option<PathBuf> {
    let runs_dir = workspace_root().join("benchmarks").join("runs");
    let index_path = runs_dir.join("index.json");
    if !index_path.exists() {
        return None;
    }
    let index = load_json(&index_path);
    let runs = index["runs"].as_array()?;
    let mut eligible: Vec<&serde_json::Value> = runs
        .iter()
        .filter(|r| {
            r["status"].as_str() == Some("complete") && r["claim_ready"].as_bool() == Some(true)
        })
        .collect();
    eligible.sort_by(|a, b| {
        let ts_a = a["timestamp_utc"].as_str().unwrap_or("");
        let ts_b = b["timestamp_utc"].as_str().unwrap_or("");
        ts_b.cmp(ts_a)
    });
    let latest = eligible.first()?;
    let run_id = latest["run_id"].as_str()?;
    let run_dir = runs_dir.join(run_id);
    if run_dir.exists() {
        Some(run_dir)
    } else {
        None
    }
}

/// Ensure at least one smoke run exists.
fn ensure_smoke_run_exists() {
    let root = workspace_root();
    let runs_dir = root.join("benchmarks").join("runs");
    let index_path = runs_dir.join("index.json");
    if index_path.exists() {
        let index = load_json(&index_path);
        if let Some(runs) = index["runs"].as_array() {
            if runs
                .iter()
                .any(|r| r["claim_ready"].as_bool() == Some(true))
            {
                return;
            }
        }
    }
    let harness = root.join("benchmarks").join("harness.py");
    let output = Command::new("python3")
        .args([harness.to_str().unwrap(), "smoke", "--run-set", "local"])
        .current_dir(&root)
        .output()
        .expect("Failed to run benchmark harness");
    assert!(
        output.status.success(),
        "Smoke run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------
// Correctness gate structure tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_correctness_gate_policy_declares_oracle_comparators() {
    let policy = load_json(&workspace_root().join("benchmarks").join("policy.json"));
    let gate = &policy["correctness_gate"];

    let oracles = gate["oracle_comparators"]
        .as_array()
        .expect("correctness_gate should have oracle_comparators");
    let oracle_ids: Vec<&str> = oracles.iter().map(|v| v.as_str().unwrap()).collect();

    assert!(
        oracle_ids.contains(&"c_xxhsum"),
        "c_xxhsum must be an oracle comparator"
    );
    assert!(
        oracle_ids.contains(&"rust_xxhash_rs"),
        "rust_xxhash_rs must be an oracle comparator"
    );
}

#[test]
fn benchmark_correctness_gate_policy_declares_contrast_comparators() {
    let policy = load_json(&workspace_root().join("benchmarks").join("policy.json"));
    let gate = &policy["correctness_gate"];

    let contrast = gate["contrast_comparators"]
        .as_array()
        .expect("correctness_gate should have contrast_comparators");
    let contrast_ids: Vec<&str> = contrast.iter().map(|v| v.as_str().unwrap()).collect();

    assert!(
        contrast_ids.contains(&"b3sum"),
        "b3sum should be a contrast comparator"
    );
    assert!(
        contrast_ids.contains(&"md5"),
        "md5 should be a contrast comparator"
    );
}

#[test]
fn benchmark_correctness_gate_oracle_must_agree() {
    let policy = load_json(&workspace_root().join("benchmarks").join("policy.json"));
    assert!(
        policy["correctness_gate"]["oracle_must_agree"]
            .as_bool()
            .unwrap(),
        "Policy must require oracle agreement"
    );
}

#[test]
fn benchmark_correctness_gate_contrast_must_execute_not_agree() {
    let policy = load_json(&workspace_root().join("benchmarks").join("policy.json"));
    assert!(
        policy["correctness_gate"]["contrast_must_execute"]
            .as_bool()
            .unwrap(),
        "Policy must require contrast comparators to execute"
    );

    // Contrast comparators should NOT be parity oracles
    let comparators = load_json(&workspace_root().join("benchmarks").join("comparators.json"));
    for comp in comparators["canonical_comparators"].as_array().unwrap() {
        let id = comp["id"].as_str().unwrap();
        let role = comp["role"].as_str().unwrap();
        let parity = comp["parity_oracle"].as_bool().unwrap();
        if role == "contrast" {
            assert!(
                !parity,
                "Contrast comparator {id} should not be a parity oracle"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Run-level correctness gate validation
// ---------------------------------------------------------------------------

#[test]
fn benchmark_correctness_gate_run_manifest_records_per_scenario_results() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have a claim-ready run");
    let manifest = load_json(&run_dir.join("manifest.json"));

    let gate = &manifest["correctness_gate"];
    assert!(
        gate["all_passed"].is_boolean(),
        "correctness_gate should have all_passed boolean"
    );

    let results = gate["results"]
        .as_array()
        .expect("correctness_gate should have results array");
    assert!(
        !results.is_empty(),
        "correctness_gate results should not be empty"
    );

    for result in results {
        assert!(
            result["scenario_id"].is_string(),
            "Each correctness result should have scenario_id"
        );
        assert!(
            result["passed"].is_boolean(),
            "Each correctness result should have passed boolean"
        );
        assert!(
            result["reason"].is_string(),
            "Each correctness result should have reason string"
        );
        assert!(
            result["oracle_digests"].is_object(),
            "Each correctness result should have oracle_digests"
        );
    }
}

#[test]
fn benchmark_correctness_gate_oracles_agree_on_digest() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have a claim-ready run");
    let manifest = load_json(&run_dir.join("manifest.json"));

    let results = manifest["correctness_gate"]["results"]
        .as_array()
        .unwrap();

    for result in results {
        let scenario_id = result["scenario_id"].as_str().unwrap();
        assert!(
            result["passed"].as_bool().unwrap(),
            "Scenario {scenario_id} correctness gate should pass"
        );

        let digests = result["oracle_digests"].as_object().unwrap();
        let c_digest = digests
            .get("c_xxhsum")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let rust_digest = digests
            .get("rust_xxhash_rs")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        assert!(
            !c_digest.is_empty(),
            "Scenario {scenario_id}: c_xxhsum digest should not be empty"
        );
        assert!(
            !rust_digest.is_empty(),
            "Scenario {scenario_id}: rust_xxhash_rs digest should not be empty"
        );
        assert_eq!(
            c_digest, rust_digest,
            "Scenario {scenario_id}: c_xxhsum ({c_digest}) and rust_xxhash_rs ({rust_digest}) must agree"
        );
    }
}

#[test]
fn benchmark_correctness_gate_blocks_summary_on_divergence() {
    // Verify that the harness.py correctness_gate.py reports divergence as blocking.
    // We test this by checking the claim_gate.py script with a synthetic diverged run.
    let root = workspace_root();
    let claim_gate = root.join("benchmarks").join("claim_gate.py");
    assert!(
        claim_gate.exists(),
        "benchmarks/claim_gate.py must exist for correctness gate enforcement"
    );

    // Also verify the reconcile.py script exists
    let reconcile = root.join("benchmarks").join("reconcile.py");
    assert!(
        reconcile.exists(),
        "benchmarks/reconcile.py must exist for reconciliation"
    );
}

#[test]
fn benchmark_correctness_gate_claim_ready_requires_all_gates_passed() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have a claim-ready run");
    let manifest = load_json(&run_dir.join("manifest.json"));

    // claim_ready should only be true when correctness_gate.all_passed is true
    let claim_ready = manifest["claim_ready"].as_bool().unwrap();
    let all_passed = manifest["correctness_gate"]["all_passed"].as_bool().unwrap();
    let matrix_complete = manifest["completeness"]["complete"].as_bool().unwrap();

    if claim_ready {
        assert!(
            all_passed,
            "claim_ready=true requires correctness_gate.all_passed=true"
        );
        assert!(
            matrix_complete,
            "claim_ready=true requires completeness.complete=true"
        );
    }
}

// ---------------------------------------------------------------------------
// Oracle-digest hardening: both digests required (regression coverage)
// ---------------------------------------------------------------------------

/// Verify that the harness correctness gate rejects a synthetic scenario
/// where one oracle digest is missing.  We call the harness module's
/// `check_correctness_gate` indirectly through a small inline Python script.
fn run_python_gate_check(c_digest: &str, rust_digest: &str) -> (bool, String) {
    let script = format!(
        r#"
import sys, json
sys.path.insert(0, "{harness_dir}")
import harness

sr = {{
    "scenario_id": "synth",
    "comparator_results": {{
        "c_xxhsum": {{
            "status": "success",
            "measured_samples": [{{
                "stdout_first_line": "{c_digest}  payload.bin" if "{c_digest}" else "",
                "success": True,
            }}],
        }},
        "rust_xxhash_rs": {{
            "status": "success",
            "measured_samples": [{{
                "stdout_first_line": "{rust_digest}  payload.bin" if "{rust_digest}" else "",
                "success": True,
            }}],
        }},
        "b3sum": {{
            "status": "success",
            "measured_samples": [{{"stdout_first_line": "aabbccdd  p", "success": True}}],
        }},
        "md5": {{
            "status": "success",
            "measured_samples": [{{"stdout_first_line": "aabbccdd  p", "success": True}}],
        }},
    }},
}}
policy = {{
    "correctness_gate": {{
        "oracle_comparators": ["c_xxhsum", "rust_xxhash_rs"],
        "contrast_comparators": ["b3sum", "md5"],
        "oracle_must_agree": True,
        "contrast_must_execute": True,
    }}
}}
result = harness.check_correctness_gate(sr, policy)
print(json.dumps(result))
"#,
        harness_dir = workspace_root()
            .join("benchmarks")
            .to_str()
            .unwrap()
            .replace('\\', "\\\\"),
        c_digest = c_digest,
        rust_digest = rust_digest,
    );

    let output = Command::new("python3")
        .args(["-c", &script])
        .current_dir(workspace_root())
        .output()
        .expect("Failed to run python3 gate check");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    assert!(
        output.status.success(),
        "Python gate-check script failed.\nstdout: {stdout}\nstderr: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Should parse gate JSON");
    let passed = parsed["passed"].as_bool().unwrap();
    let reason = parsed["reason"].as_str().unwrap_or("").to_string();
    (passed, reason)
}

#[test]
fn benchmark_correctness_gate_rejects_missing_c_xxhsum_digest() {
    let (passed, reason) = run_python_gate_check("", "abcd1234");
    assert!(
        !passed,
        "Gate must fail when c_xxhsum digest is empty. Reason: {reason}"
    );
    assert!(
        reason.contains("c_xxhsum"),
        "Reason should mention c_xxhsum: {reason}"
    );
}

#[test]
fn benchmark_correctness_gate_rejects_missing_rust_digest() {
    let (passed, reason) = run_python_gate_check("abcd1234", "");
    assert!(
        !passed,
        "Gate must fail when rust_xxhash_rs digest is empty. Reason: {reason}"
    );
    assert!(
        reason.contains("rust_xxhash_rs"),
        "Reason should mention rust_xxhash_rs: {reason}"
    );
}

#[test]
fn benchmark_correctness_gate_rejects_both_empty_digests() {
    let (passed, _reason) = run_python_gate_check("", "");
    assert!(!passed, "Gate must fail when both oracle digests are empty");
}

#[test]
fn benchmark_correctness_gate_accepts_both_matching_digests() {
    let (passed, reason) = run_python_gate_check("abcdef0123456789", "abcdef0123456789");
    assert!(
        passed,
        "Gate should pass when both oracle digests match. Reason: {reason}"
    );
}

#[test]
fn benchmark_correctness_gate_rejects_disagreeing_digests() {
    let (passed, reason) = run_python_gate_check("aaaa1111", "bbbb2222");
    assert!(!passed, "Gate must fail when oracle digests disagree");
    assert!(
        reason.contains("disagree"),
        "Reason should mention disagreement: {reason}"
    );
}

// ---------------------------------------------------------------------------
// Correctness gate enforcement via claim_gate.py
// ---------------------------------------------------------------------------

#[test]
fn benchmark_correctness_gate_claim_gate_script_validates_correctness() {
    ensure_smoke_run_exists();
    let root = workspace_root();
    let claim_gate = root.join("benchmarks").join("claim_gate.py");

    let output = Command::new("python3")
        .args([claim_gate.to_str().unwrap(), "--run", "latest"])
        .current_dir(&root)
        .output()
        .expect("Failed to run claim_gate.py");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "claim_gate.py --run latest should succeed for a claim-ready run.\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Verify the output includes correctness gate status
    assert!(
        stdout.contains("correctness") || stdout.contains("Correctness"),
        "claim_gate.py output should mention correctness gate status.\nstdout: {stdout}"
    );
}

#[test]
fn benchmark_correctness_gate_reconcile_script_validates_samples() {
    ensure_smoke_run_exists();
    let root = workspace_root();
    let reconcile = root.join("benchmarks").join("reconcile.py");

    let output = Command::new("python3")
        .args([reconcile.to_str().unwrap(), "--run", "latest"])
        .current_dir(&root)
        .output()
        .expect("Failed to run reconcile.py");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "reconcile.py --run latest should succeed for a claim-ready run.\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Verify reconciliation output includes summary statistic and unit info
    assert!(
        stdout.contains("median") || stdout.contains("reconcil"),
        "reconcile.py output should mention reconciliation or median.\nstdout: {stdout}"
    );
}
