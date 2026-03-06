//! Integration tests for benchmark run resolution and completeness checking.
//!
//! Verifies VAL-BENCH-002: Successful benchmark runs are complete, reproducible,
//! and claim-ready. Also validates VAL-BENCH-004: latest resolves safely.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the workspace root (parent of the crate directory).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate should be in a workspace")
        .to_path_buf()
}

/// Helper: find the most recent run directory in benchmarks/runs/.
fn find_latest_run_dir() -> Option<PathBuf> {
    let runs_dir = workspace_root().join("benchmarks").join("runs");
    if !runs_dir.exists() {
        return None;
    }

    let index_path = runs_dir.join("index.json");
    if !index_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&index_path).ok()?;
    let index: serde_json::Value = serde_json::from_str(&content).ok()?;
    let runs = index["runs"].as_array()?;

    // Find the latest claim-ready run
    let mut eligible: Vec<&serde_json::Value> = runs
        .iter()
        .filter(|r| {
            r["status"].as_str() == Some("complete")
                && r["claim_ready"].as_bool() == Some(true)
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

/// Ensure at least one smoke run exists before resolution tests.
fn ensure_smoke_run_exists() {
    let root = workspace_root();
    let runs_dir = root.join("benchmarks").join("runs");
    let index_path = runs_dir.join("index.json");

    // If there's already a claim-ready run, skip
    if index_path.exists() {
        let content = std::fs::read_to_string(&index_path).unwrap_or_default();
        if let Ok(index) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(runs) = index["runs"].as_array() {
                if runs.iter().any(|r| r["claim_ready"].as_bool() == Some(true)) {
                    return;
                }
            }
        }
    }

    // Run a smoke benchmark to create a run bundle
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
// Run bundle structure tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_run_resolution_run_bundle_has_manifest() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have at least one run");
    let manifest_path = run_dir.join("manifest.json");
    assert!(
        manifest_path.exists(),
        "Run bundle should contain manifest.json"
    );
}

#[test]
fn benchmark_run_resolution_run_bundle_has_samples() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have at least one run");
    let samples_dir = run_dir.join("samples");
    assert!(samples_dir.exists(), "Run bundle should contain samples/");

    let sample_files: Vec<_> = std::fs::read_dir(&samples_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();
    assert!(
        !sample_files.is_empty(),
        "Run bundle should contain at least one sample file"
    );
}

#[test]
fn benchmark_run_resolution_run_bundle_has_checksums() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have at least one run");
    let checksums_path = run_dir.join("checksums.json");
    assert!(
        checksums_path.exists(),
        "Run bundle should contain checksums.json"
    );

    let content = std::fs::read_to_string(&checksums_path).unwrap();
    let checksums: serde_json::Value = serde_json::from_str(&content).unwrap();
    let obj = checksums.as_object().expect("checksums should be an object");
    assert!(
        !obj.is_empty(),
        "checksums.json should have at least one entry"
    );

    // Verify manifest.json is among the checksummed files
    assert!(
        obj.contains_key("manifest.json"),
        "checksums should include manifest.json"
    );
}

// ---------------------------------------------------------------------------
// Run manifest content tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_run_resolution_manifest_has_required_fields() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have at least one run");
    let manifest_path = run_dir.join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Required top-level fields
    let required = [
        "run_id",
        "run_type",
        "timestamp_utc",
        "status",
        "claim_ready",
        "manifest_hashes",
        "policy_version",
        "policy_hash",
        "environment",
        "resolved_comparators",
        "correctness_gate",
        "completeness",
        "statistical_method",
        "scenario_count",
        "comparator_ids",
    ];
    for field in &required {
        assert!(
            !manifest[field].is_null(),
            "Run manifest missing required field: {field}"
        );
    }
}

#[test]
fn benchmark_run_resolution_manifest_records_manifest_hashes() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have at least one run");
    let manifest_path = run_dir.join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();

    let hashes = &manifest["manifest_hashes"];
    assert!(hashes["scenarios"].is_string(), "Should record scenarios hash");
    assert!(
        hashes["comparators"].is_string(),
        "Should record comparators hash"
    );
    assert!(hashes["policy"].is_string(), "Should record policy hash");

    // Hashes should be SHA-256 (64 hex chars)
    for key in &["scenarios", "comparators", "policy"] {
        let hash = hashes[key].as_str().unwrap();
        assert_eq!(
            hash.len(),
            64,
            "Manifest hash for {key} should be SHA-256 (64 chars)"
        );
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Manifest hash for {key} should be hex"
        );
    }
}

#[test]
fn benchmark_run_resolution_manifest_records_environment() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have at least one run");
    let manifest_path = run_dir.join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();

    let env = &manifest["environment"];
    assert!(env["platform"].is_string(), "Should record platform");
    assert!(
        env["machine"].is_string(),
        "Should record machine architecture"
    );
    assert!(env["timestamp_utc"].is_string(), "Should record timestamp");
    assert!(
        env["repo_revision"].is_string(),
        "Should record repo revision"
    );
}

#[test]
fn benchmark_run_resolution_manifest_records_resolved_comparators() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have at least one run");
    let manifest_path = run_dir.join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();

    let resolved = manifest["resolved_comparators"]
        .as_object()
        .expect("resolved_comparators should be an object");

    for comp_id in &["c_xxhsum", "rust_xxhash_rs", "b3sum", "md5"] {
        assert!(
            resolved.contains_key(*comp_id),
            "Resolved comparators should include {comp_id}"
        );
        let comp = &resolved[*comp_id];
        assert!(
            comp["binary_path"].is_string(),
            "{comp_id} should have binary_path"
        );
        assert!(comp["id"].is_string(), "{comp_id} should have id");
        assert!(comp["role"].is_string(), "{comp_id} should have role");
    }
}

#[test]
fn benchmark_run_resolution_manifest_records_correctness_gate() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have at least one run");
    let manifest_path = run_dir.join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();

    let gate = &manifest["correctness_gate"];
    assert!(
        gate["all_passed"].is_boolean(),
        "Should record correctness gate result"
    );
    assert!(
        gate["results"].is_array(),
        "Should record per-scenario correctness results"
    );
}

#[test]
fn benchmark_run_resolution_manifest_records_statistical_method() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have at least one run");
    let manifest_path = run_dir.join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();

    let method = &manifest["statistical_method"];
    assert!(
        method["summary_statistic"].is_string(),
        "Should record summary statistic"
    );
    assert!(
        method["retain_raw_samples"].as_bool() == Some(true),
        "Should record raw sample retention policy"
    );
}

// ---------------------------------------------------------------------------
// Raw sample content tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_run_resolution_raw_samples_have_invocations_and_timing() {
    ensure_smoke_run_exists();
    let run_dir = find_latest_run_dir().expect("Should have at least one run");
    let samples_dir = run_dir.join("samples");

    let sample_files: Vec<_> = std::fs::read_dir(&samples_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();

    for entry in &sample_files {
        let path = entry.path();
        let content = std::fs::read_to_string(&path).unwrap();
        let sample: serde_json::Value = serde_json::from_str(&content).unwrap();

        let scenario_id = sample["scenario_id"]
            .as_str()
            .unwrap_or_else(|| panic!("{}: missing scenario_id", path.display()));

        // Check required fields
        assert!(
            sample["algorithm"].is_string(),
            "{scenario_id}: missing algorithm"
        );
        assert!(
            sample["payload_bytes"].is_number(),
            "{scenario_id}: missing payload_bytes"
        );
        assert!(
            sample["payload_checksum"].is_string(),
            "{scenario_id}: missing payload_checksum"
        );
        assert!(
            sample["coverage_ledger"].is_object(),
            "{scenario_id}: missing coverage_ledger"
        );

        // Check each comparator has invocation and samples
        let results = sample["comparator_results"]
            .as_object()
            .unwrap_or_else(|| panic!("{scenario_id}: missing comparator_results"));

        for (comp_id, cr) in results {
            let status = cr["status"].as_str().unwrap_or("unknown");
            if status == "success" {
                assert!(
                    cr["invocation"].is_array(),
                    "{scenario_id}/{comp_id}: missing invocation"
                );
                let invocation = cr["invocation"].as_array().unwrap();
                assert!(
                    !invocation.is_empty(),
                    "{scenario_id}/{comp_id}: invocation should not be empty"
                );

                assert!(
                    cr["measured_samples"].is_array(),
                    "{scenario_id}/{comp_id}: missing measured_samples"
                );
                let samples = cr["measured_samples"].as_array().unwrap();
                assert!(
                    !samples.is_empty(),
                    "{scenario_id}/{comp_id}: should have at least one measured sample"
                );

                // Each sample should have timing
                for (i, s) in samples.iter().enumerate() {
                    assert!(
                        s["elapsed_ns"].is_number(),
                        "{scenario_id}/{comp_id}/sample[{i}]: missing elapsed_ns"
                    );
                    assert!(
                        s["command"].is_array(),
                        "{scenario_id}/{comp_id}/sample[{i}]: missing command"
                    );
                }

                assert!(
                    cr["warmup_samples"].is_array(),
                    "{scenario_id}/{comp_id}: missing warmup_samples"
                );
                assert!(
                    cr["median_ns"].is_number(),
                    "{scenario_id}/{comp_id}: missing median_ns"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Latest-run resolution tests
// ---------------------------------------------------------------------------

#[test]
fn benchmark_run_resolution_latest_resolves_to_claim_ready() {
    ensure_smoke_run_exists();

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
        "Latest resolution should succeed.\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Latest claim-ready run:"),
        "Should identify a latest claim-ready run"
    );
    assert!(
        stdout.contains("Correctness gate: PASSED"),
        "Latest run should have passed correctness gate"
    );
}

#[test]
fn benchmark_run_resolution_index_exists_and_has_runs() {
    ensure_smoke_run_exists();
    let runs_dir = workspace_root().join("benchmarks").join("runs");
    let index_path = runs_dir.join("index.json");
    assert!(index_path.exists(), "Run index should exist");

    let content = std::fs::read_to_string(&index_path).unwrap();
    let index: serde_json::Value = serde_json::from_str(&content).unwrap();
    let runs = index["runs"].as_array().expect("index should have runs array");
    assert!(!runs.is_empty(), "Index should have at least one run");

    // Each run entry should have required fields
    for run in runs {
        assert!(run["run_id"].is_string(), "Run entry missing run_id");
        assert!(
            run["timestamp_utc"].is_string(),
            "Run entry missing timestamp_utc"
        );
        assert!(run["status"].is_string(), "Run entry missing status");
        assert!(
            run["claim_ready"].is_boolean(),
            "Run entry missing claim_ready"
        );
    }
}

#[test]
fn benchmark_run_resolution_partial_runs_excluded_from_latest() {
    ensure_smoke_run_exists();

    let runs_dir = workspace_root().join("benchmarks").join("runs");
    let index_path = runs_dir.join("index.json");
    let content = std::fs::read_to_string(&index_path).unwrap();
    let index: serde_json::Value = serde_json::from_str(&content).unwrap();
    let runs = index["runs"].as_array().unwrap();

    // Find which run the latest command resolves to
    let root = workspace_root();
    let harness = root.join("benchmarks").join("harness.py");
    let output = Command::new("python3")
        .args([harness.to_str().unwrap(), "latest"])
        .current_dir(&root)
        .output()
        .expect("Failed to run harness latest");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the resolved run is claim-ready in the index
    for run in runs {
        let run_id = run["run_id"].as_str().unwrap();
        if stdout.contains(run_id) {
            assert_eq!(
                run["status"].as_str(),
                Some("complete"),
                "Latest-resolved run {run_id} should have status=complete"
            );
            assert_eq!(
                run["claim_ready"].as_bool(),
                Some(true),
                "Latest-resolved run {run_id} should be claim_ready"
            );
        }
    }
}
