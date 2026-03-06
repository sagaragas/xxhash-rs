//! Integration tests for the benchmark claim gate.
//!
//! Verifies VAL-BENCH-004: Published summary numbers reconcile to raw samples with declared
//! units/statistics, and any `latest` run pointer resolves only to the most recent run with
//! full matrix coverage, correctness/reproducibility gates, and a complete successful artifact
//! bundle.
//!
//! Verifies VAL-BENCH-005: Performance claims pass a claim gate across the required run set.
//! Any performance claim used in publication artifacts is derived from a declared required run
//! set whose members share the same measured revision and scenario/manifests, and every required
//! run passes correctness, reproducibility, and claim-gate thresholds rather than relying on a
//! single favorable run.
//!
//! NOTE: Tests that exercise claim-gate and reconcile validation use isolated
//! deterministic run sets seeded via `seed_run_set.py` and the `--run-dir` /
//! `--policy` flags, so they never depend on mutable ambient benchmarks/runs/ state.

use std::collections::HashSet;
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

/// Helper: find the most recent claim-ready run directory from the ambient runs.
fn find_latest_claim_ready_run() -> Option<PathBuf> {
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

/// Ensure at least one smoke run exists in ambient state.
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

/// Seed an isolated deterministic run set into the given directory and return
/// (runs_dir, policy_path) so tests can use --run-dir / --policy flags.
fn seed_isolated_run_set(base_dir: &Path, num_runs: u32) -> (PathBuf, PathBuf) {
    let root = workspace_root();
    let seeder = root.join("benchmarks").join("seed_run_set.py");
    let runs_output = base_dir.join("runs");
    let output = Command::new("python3")
        .args([
            seeder.to_str().unwrap(),
            "--output",
            runs_output.to_str().unwrap(),
            "--num-runs",
            &num_runs.to_string(),
            "--with-policy",
        ])
        .current_dir(&root)
        .output()
        .expect("Failed to run seed_run_set.py");
    assert!(
        output.status.success(),
        "seed_run_set.py failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // seed_run_set.py --with-policy writes policy.json in the --output dir
    let policy_path = runs_output.join("policy.json");
    (runs_output, policy_path)
}

// ---------------------------------------------------------------------------
// Claim-gate policy tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_claim_gate_policy_requires_correctness_for_claims() {
    let policy = load_json(&workspace_root().join("benchmarks").join("policy.json"));
    let claim = &policy["claim_readiness"];

    assert!(
        claim["require_correctness_gate"].as_bool().unwrap(),
        "Claim readiness must require correctness gate"
    );
    assert!(
        claim["require_full_matrix"].as_bool().unwrap(),
        "Claim readiness must require full matrix"
    );
    assert!(
        claim["require_artifact_checksums"].as_bool().unwrap(),
        "Claim readiness must require artifact checksums"
    );
}

#[test]
fn benchmark_claim_gate_policy_requires_run_set_matching() {
    let policy = load_json(&workspace_root().join("benchmarks").join("policy.json"));

    // The claim gate policy must require matching revision and manifests
    let claim = &policy["claim_readiness"];
    assert!(
        claim["require_matching_revision"].as_bool().unwrap(),
        "Claim readiness must require matching revision across run set"
    );
    assert!(
        claim["require_matching_manifests"].as_bool().unwrap(),
        "Claim readiness must require matching manifests across run set"
    );
}

#[test]
fn benchmark_claim_gate_policy_requires_minimum_run_set() {
    let policy = load_json(&workspace_root().join("benchmarks").join("policy.json"));
    let claim = &policy["claim_readiness"];

    let min_runs = claim["minimum_runs"]
        .as_u64()
        .expect("claim_readiness should declare minimum_runs");
    assert!(
        min_runs >= 1,
        "Minimum runs for claim readiness should be at least 1"
    );
}

// ---------------------------------------------------------------------------
// Run-set consistency tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_claim_gate_claim_ready_runs_share_revision() {
    // Use an isolated deterministic run set so this test does not depend on
    // whatever happens to be in benchmarks/runs/ (ambient mutable state).
    let tmp = tempfile::tempdir().expect("create temp dir");
    let (runs_dir, policy_path) = seed_isolated_run_set(tmp.path(), 3);

    let index_path = runs_dir.join("index.json");
    let index = load_json(&index_path);
    let runs = index["runs"].as_array().unwrap();

    let claim_ready_runs: Vec<&serde_json::Value> = runs
        .iter()
        .filter(|r| r["claim_ready"].as_bool() == Some(true))
        .collect();

    assert!(
        claim_ready_runs.len() >= 2,
        "Seeded run set should have at least 2 claim-ready runs"
    );

    // All claim-ready runs should share the same manifest hashes and revision
    let mut revisions = HashSet::new();
    let mut manifest_hash_sets = HashSet::new();

    for run_entry in &claim_ready_runs {
        let run_id = run_entry["run_id"].as_str().unwrap();
        let run_dir = runs_dir.join(run_id);
        let manifest = load_json(&run_dir.join("manifest.json"));

        if let Some(rev) = manifest["environment"]["repo_revision"].as_str() {
            revisions.insert(rev.to_string());
        }

        let hashes = &manifest["manifest_hashes"];
        let hash_key = format!(
            "{}:{}:{}",
            hashes["scenarios"].as_str().unwrap_or(""),
            hashes["comparators"].as_str().unwrap_or(""),
            hashes["policy"].as_str().unwrap_or("")
        );
        manifest_hash_sets.insert(hash_key);
    }

    // Singleton revision and hash sets prove the compatible run set is consistent
    assert_eq!(
        revisions.len(),
        1,
        "All seeded claim-ready runs must share one revision, got: {revisions:?}"
    );
    assert_eq!(
        manifest_hash_sets.len(),
        1,
        "All seeded claim-ready runs must share one manifest hash set"
    );

    // Verify claim_gate.py passes against the isolated run set
    let root = workspace_root();
    let claim_gate = root.join("benchmarks").join("claim_gate.py");
    let output = Command::new("python3")
        .args([
            claim_gate.to_str().unwrap(),
            "--run",
            "latest",
            "--run-dir",
            runs_dir.to_str().unwrap(),
            "--policy",
            policy_path.to_str().unwrap(),
        ])
        .current_dir(&root)
        .output()
        .expect("Failed to run claim_gate.py");

    assert!(
        output.status.success(),
        "claim_gate.py should pass for isolated claim-ready runs.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ---------------------------------------------------------------------------
// Reconciliation tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_claim_gate_summary_reconciles_to_raw_samples() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_claim_ready_run().expect("Should have a claim-ready run");
    let manifest = load_json(&run_dir.join("manifest.json"));
    let samples_dir = run_dir.join("samples");

    // For each scenario, verify the summary median reconciles to raw samples
    let gate_results = manifest["correctness_gate"]["results"]
        .as_array()
        .unwrap();

    for gate_result in gate_results {
        let scenario_id = gate_result["scenario_id"].as_str().unwrap();
        let sample_path = samples_dir.join(format!("{scenario_id}.json"));
        assert!(
            sample_path.exists(),
            "Raw samples for scenario {scenario_id} must exist"
        );

        let sample = load_json(&sample_path);
        let comp_results = sample["comparator_results"].as_object().unwrap();

        for (comp_id, cr) in comp_results {
            if cr["status"].as_str() != Some("success") {
                continue;
            }

            let declared_median = cr["median_ns"].as_f64();
            let measured_samples = cr["measured_samples"]
                .as_array()
                .expect("should have measured_samples");

            // Recompute median from raw samples
            let mut elapsed_values: Vec<i64> = measured_samples
                .iter()
                .filter(|s| s["success"].as_bool() == Some(true))
                .filter_map(|s| s["elapsed_ns"].as_i64())
                .collect();
            elapsed_values.sort();

            if !elapsed_values.is_empty() {
                let recomputed_median = elapsed_values[elapsed_values.len() / 2];
                if let Some(declared) = declared_median {
                    assert_eq!(
                        declared as i64, recomputed_median,
                        "Scenario {scenario_id}/{comp_id}: declared median ({declared}) must match recomputed median ({recomputed_median})"
                    );
                }
            }
        }
    }
}

#[test]
fn benchmark_claim_gate_summary_declares_units_and_statistics() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_claim_ready_run().expect("Should have a claim-ready run");
    let manifest = load_json(&run_dir.join("manifest.json"));

    let method = &manifest["statistical_method"];
    assert!(
        method["summary_statistic"].is_string(),
        "Statistical method must declare summary_statistic"
    );
    assert!(
        method["warmup_policy"].is_string(),
        "Statistical method must declare warmup_policy"
    );
    assert!(
        method["retain_raw_samples"].as_bool() == Some(true),
        "Statistical method must retain raw samples"
    );
}

// ---------------------------------------------------------------------------
// Artifact integrity tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_claim_gate_artifact_checksums_are_valid() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_claim_ready_run().expect("Should have a claim-ready run");
    let checksums_path = run_dir.join("checksums.json");
    assert!(
        checksums_path.exists(),
        "Run bundle must have checksums.json"
    );

    let checksums = load_json(&checksums_path);
    let obj = checksums.as_object().unwrap();

    // Verify each declared checksum matches the actual file
    for (rel_path, expected_hash) in obj {
        if rel_path == "checksums.json" {
            // checksums.json can't checksum itself
            continue;
        }
        let file_path = run_dir.join(rel_path);
        assert!(
            file_path.exists(),
            "Checksummed file should exist: {rel_path}"
        );

        let expected = expected_hash.as_str().unwrap();
        let actual = sha256_file(&file_path);
        assert_eq!(
            expected, actual,
            "Checksum mismatch for {rel_path}: expected={expected}, actual={actual}"
        );
    }
}

fn sha256_file(path: &Path) -> String {
    use std::io::Read;
    let mut file = std::fs::File::open(path).unwrap();
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    hasher.finalize_hex()
}

/// A minimal SHA-256 implementation for test-side checksum verification.
/// We don't pull in external crates; instead we shell out to shasum.
struct Sha256 {
    data: Vec<u8>,
}

impl Sha256 {
    fn new() -> Self {
        Self { data: Vec::new() }
    }
    fn update(&mut self, chunk: &[u8]) {
        self.data.extend_from_slice(chunk);
    }
    fn finalize_hex(self) -> String {
        // Use shasum command to compute the hash
        use std::io::Write;
        let mut child = Command::new("shasum")
            .args(["-a", "256"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to spawn shasum");
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&self.data)
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.split_whitespace().next().unwrap_or("").to_string()
    }
}

// ---------------------------------------------------------------------------
// Latest resolution safety after gates are added
// ---------------------------------------------------------------------------

#[test]
fn benchmark_claim_gate_latest_excludes_non_claim_ready_runs() {
    ensure_smoke_run_exists();

    let runs_dir = workspace_root().join("benchmarks").join("runs");
    let index_path = runs_dir.join("index.json");
    let index = load_json(&index_path);
    let runs = index["runs"].as_array().unwrap();

    // Check that the latest resolution picks the correct run
    let root = workspace_root();
    let harness = root.join("benchmarks").join("harness.py");
    let output = Command::new("python3")
        .args([harness.to_str().unwrap(), "latest"])
        .current_dir(&root)
        .output()
        .expect("Failed to run harness latest");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "Latest resolution should succeed.\nstdout: {stdout}"
    );

    // Verify the resolved run is claim-ready in the index
    for run in runs {
        let run_id = run["run_id"].as_str().unwrap();
        if stdout.contains(run_id) {
            assert_eq!(
                run["status"].as_str(),
                Some("complete"),
                "Latest-resolved run should have status=complete"
            );
            assert_eq!(
                run["claim_ready"].as_bool(),
                Some(true),
                "Latest-resolved run should be claim_ready"
            );
        }
    }
}

#[test]
fn benchmark_claim_gate_latest_resolves_most_recent_complete() {
    ensure_smoke_run_exists();

    let runs_dir = workspace_root().join("benchmarks").join("runs");
    let index_path = runs_dir.join("index.json");
    let index = load_json(&index_path);
    let runs = index["runs"].as_array().unwrap();

    // Find the most recent claim-ready run by timestamp
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

    if let Some(expected_latest) = eligible.first() {
        let expected_id = expected_latest["run_id"].as_str().unwrap();

        let root = workspace_root();
        let harness = root.join("benchmarks").join("harness.py");
        let output = Command::new("python3")
            .args([harness.to_str().unwrap(), "latest"])
            .current_dir(&root)
            .output()
            .unwrap();

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(expected_id),
            "Latest should resolve to {expected_id}, got: {stdout}"
        );
    }
}

// ---------------------------------------------------------------------------
// claim_gate.py script tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_claim_gate_script_checks_run_set_integrity() {
    // Use an isolated deterministic run set so this test does not depend on
    // whatever happens to be in benchmarks/runs/ (ambient mutable state).
    let tmp = tempfile::tempdir().expect("create temp dir");
    let (runs_dir, policy_path) = seed_isolated_run_set(tmp.path(), 3);

    let root = workspace_root();
    let claim_gate = root.join("benchmarks").join("claim_gate.py");

    let output = Command::new("python3")
        .args([
            claim_gate.to_str().unwrap(),
            "--run",
            "latest",
            "--run-dir",
            runs_dir.to_str().unwrap(),
            "--policy",
            policy_path.to_str().unwrap(),
        ])
        .current_dir(&root)
        .output()
        .expect("Failed to run claim_gate.py");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "claim_gate.py --run latest should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Verify the output includes key claim-gate checks
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("revision") || combined.contains("Revision"),
        "claim_gate.py should check revision consistency.\nOutput: {combined}"
    );
    assert!(
        combined.contains("manifest") || combined.contains("Manifest"),
        "claim_gate.py should check manifest consistency.\nOutput: {combined}"
    );
    assert!(
        combined.contains("PASS") || combined.contains("pass") || combined.contains("ready"),
        "claim_gate.py should report pass/ready status.\nOutput: {combined}"
    );
}

#[test]
fn benchmark_claim_gate_reconcile_script_validates_all_scenarios() {
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
        "reconcile.py --run latest should succeed.\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Verify the output includes reconciliation results
    assert!(
        stdout.contains("reconcil") || stdout.contains("Reconcil"),
        "reconcile.py should report reconciliation results.\nstdout: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Incomplete/partial run rejection tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_claim_gate_incomplete_runs_are_not_claim_ready() {
    ensure_smoke_run_exists();

    let runs_dir = workspace_root().join("benchmarks").join("runs");
    let index_path = runs_dir.join("index.json");
    let index = load_json(&index_path);
    let runs = index["runs"].as_array().unwrap();

    // Any run marked as partial should NOT be claim-ready
    for run in runs {
        let status = run["status"].as_str().unwrap_or("unknown");
        let claim_ready = run["claim_ready"].as_bool().unwrap_or(false);
        let run_id = run["run_id"].as_str().unwrap_or("unknown");

        if status == "partial" {
            assert!(
                !claim_ready,
                "Partial run {run_id} must not be claim-ready"
            );
        }
    }
}
