#!/usr/bin/env python3
"""
Generate the publication evidence pack for the xxhash-rs rewrite study.

Collects parity outputs, benchmark artifacts, measured revision metadata,
and scenario IDs into stable, machine-readable files under publication/evidence/.

The evidence pack is designed so that the website post can cite pinned
repo-side artifact paths rather than mutable latest links.

Usage:
    python3 publication/generate_evidence.py
"""

import hashlib
import json
import os
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
BENCHMARKS_DIR = REPO_ROOT / "benchmarks"
EVIDENCE_DIR = REPO_ROOT / "publication" / "evidence"
RUNS_DIR = BENCHMARKS_DIR / "runs"
INDEX_PATH = RUNS_DIR / "index.json"


def file_sha256(path: Path) -> str:
    """Compute SHA-256 hex digest of a file."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def get_repo_revision() -> str:
    """Return the current git HEAD revision."""
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        capture_output=True, text=True, cwd=REPO_ROOT,
    )
    return result.stdout.strip()


def get_repo_dirty() -> bool:
    """Return whether the working tree has uncommitted changes."""
    result = subprocess.run(
        ["git", "status", "--porcelain"],
        capture_output=True, text=True, cwd=REPO_ROOT,
    )
    return bool(result.stdout.strip())


def load_json(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


def load_json_safe(path: Path, default=None):
    if default is None:
        default = {}
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, json.JSONDecodeError, ValueError):
        return default


def collect_parity_evidence() -> dict:
    """Run cargo test and collect parity/vector test results."""
    # Run the full parity + vector test suite using the canonical stable
    # workspace test command (services.yaml: --test-threads=3) instead of
    # the known-flaky --test-threads=5 that causes intermittent CLI parity
    # failures due to reference binary path contention under high concurrency.
    result = subprocess.run(
        ["cargo", "test", "--workspace", "--all-targets", "--", "--test-threads=3"],
        capture_output=True, text=True, cwd=REPO_ROOT,
        timeout=300,
    )

    # Parse test results from output
    lines = result.stdout.splitlines()
    test_results = []
    total_passed = 0
    total_failed = 0

    # Categorize tests into parity areas
    parity_categories = {
        "oneshot_reference_parity": "Hash one-shot parity vs C reference binary",
        "streaming_chunk_parity": "Streaming hash parity across chunking patterns",
        "manual_parity": "Manual parity checks (XXH32/XXH64 vs reference)",
        "hash_vectors_boundary": "Boundary-length vector conformance",
        "xxh32_vectors": "XXH32 known-vector validation",
        "xxh64_vectors": "XXH64 known-vector validation",
        "xxh3_64_vectors": "XXH3_64 known-vector validation",
        "xxh3_128_vectors": "XXH3_128 known-vector validation",
        "xxh3_simd_scalar_parity": "SIMD vs scalar parity (Apple Silicon NEON)",
        "xxh3_optimized_path": "Optimized path detection and parity",
        "streaming_digest_state": "Streaming digest state stability",
        "cli_output_format_parity": "CLI output format parity vs reference",
        "cli_input_flow_parity": "CLI input flow parity vs reference",
        "cli_algorithm_selection": "CLI algorithm selection parity",
        "cli_check_success": "CLI check-mode success parity",
        "cli_check_malformed": "CLI check-mode malformed line handling",
        "cli_check_escaped": "CLI check-mode escaped/little-endian parity",
        "cli_check_status": "CLI check-mode --status/--quiet parity",
        "cli_filelist_parity": "CLI file-list parity vs reference",
    }

    category_results = {}
    for line in lines:
        line = line.strip()
        if line.startswith("test ") and " ... " in line:
            parts = line.split(" ... ")
            test_name = parts[0].replace("test ", "")
            status = parts[1].strip()
            if status == "ok":
                total_passed += 1
            elif status == "FAILED":
                total_failed += 1

            # Categorize
            for prefix, desc in parity_categories.items():
                if prefix in test_name:
                    if prefix not in category_results:
                        category_results[prefix] = {
                            "description": desc,
                            "passed": 0,
                            "failed": 0,
                            "tests": [],
                        }
                    category_results[prefix]["tests"].append({
                        "name": test_name,
                        "status": status,
                    })
                    if status == "ok":
                        category_results[prefix]["passed"] += 1
                    else:
                        category_results[prefix]["failed"] += 1
                    break

    return {
        "total_passed": total_passed,
        "total_failed": total_failed,
        "all_passed": total_failed == 0 and result.returncode == 0,
        "exit_code": result.returncode,
        "categories": category_results,
    }


def collect_benchmark_evidence(measured_revision: str) -> dict:
    """Collect benchmark run evidence from the runs index."""
    index = load_json_safe(INDEX_PATH, {"runs": []})
    runs = index.get("runs", [])

    # Find claim-ready runs at the measured revision
    claim_ready_runs = [
        r for r in runs
        if r.get("claim_ready") and r.get("revision") == measured_revision
    ]

    if not claim_ready_runs:
        return {
            "measured_revision": measured_revision,
            "claim_ready_run_count": 0,
            "pinned_run_ids": [],
            "error": "No claim-ready runs found at measured revision",
        }

    # Collect up to 3 representative pinned runs (most recent)
    pinned_runs = sorted(
        claim_ready_runs,
        key=lambda r: r.get("timestamp_utc", ""),
        reverse=True,
    )[:3]

    pinned_run_ids = [r["run_id"] for r in pinned_runs]

    # Collect scenario coverage from the first pinned run
    first_run_dir = RUNS_DIR / pinned_runs[0]["run_id"]
    manifest = load_json_safe(first_run_dir / "manifest.json")
    checksums = load_json_safe(first_run_dir / "checksums.json")

    # Collect per-scenario correctness gate results
    correctness_gate = manifest.get("correctness_gate", {})

    # Collect comparator inventory from the first run
    resolved_comparators = manifest.get("resolved_comparators", {})

    # Collect manifest hashes
    manifest_hashes = manifest.get("manifest_hashes", {})

    # Get scenario IDs from samples
    sample_dir = first_run_dir / "samples"
    scenario_ids = []
    if sample_dir.exists():
        for f in sorted(sample_dir.iterdir()):
            if f.suffix == ".json":
                scenario_ids.append(f.stem)

    return {
        "measured_revision": measured_revision,
        "claim_ready_run_count": len(claim_ready_runs),
        "pinned_run_ids": pinned_run_ids,
        "scenario_ids": scenario_ids,
        "manifest_hashes": manifest_hashes,
        "policy_version": manifest.get("policy_version"),
        "policy_hash": manifest.get("policy_hash"),
        "correctness_gate": correctness_gate,
        "resolved_comparators": {
            cid: {
                "id": c.get("id"),
                "version": c.get("version"),
                "role": c.get("role"),
                "parity_oracle": c.get("parity_oracle"),
            }
            for cid, c in resolved_comparators.items()
        },
        "statistical_method": manifest.get("statistical_method"),
        "environment": {
            "hostname": manifest.get("environment", {}).get("hostname"),
            "platform": manifest.get("environment", {}).get("platform"),
            "machine": manifest.get("environment", {}).get("machine"),
        },
        "artifact_checksums": checksums,
    }


def build_artifact_manifest(
    measured_revision: str,
    parity_evidence: dict,
    benchmark_evidence: dict,
) -> dict:
    """Build the master artifact manifest linking all evidence."""
    pinned_run_ids = benchmark_evidence.get("pinned_run_ids", [])

    # Build stable paths relative to repo root
    artifacts = {
        "parity": {
            "summary": "publication/evidence/parity_summary.json",
            "description": "Machine-readable parity test results across all hash variants and CLI surface",
            "all_passed": parity_evidence.get("all_passed", False),
        },
        "benchmark": {
            "summary": "publication/evidence/benchmark_summary.json",
            "description": "Benchmark evidence with pinned run IDs and correctness gate results",
            "pinned_run_ids": pinned_run_ids,
            "run_snapshot_paths": [
                f"publication/evidence/benchmark_runs/{rid}" for rid in pinned_run_ids
            ],
            "scenarios_manifest": "benchmarks/scenarios.json",
            "comparators_manifest": "benchmarks/comparators.json",
            "policy": "benchmarks/policy.json",
        },
        "traceability": {
            "claim_map_inputs": "publication/evidence/claim_map_inputs.json",
            "description": "Claim-to-evidence mapping inputs for publication traceability",
        },
    }

    # Compute checksums for key manifest files
    manifest_file_checksums = {}
    for rel_path in [
        "benchmarks/scenarios.json",
        "benchmarks/comparators.json",
        "benchmarks/policy.json",
    ]:
        abs_path = REPO_ROOT / rel_path
        if abs_path.exists():
            manifest_file_checksums[rel_path] = file_sha256(abs_path)

    return {
        "schema_version": "1.0.0",
        "description": "Publication evidence pack artifact manifest for xxhash-rs rewrite study",
        "measured_revision": measured_revision,
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        "artifacts": artifacts,
        "manifest_file_checksums": manifest_file_checksums,
    }


def build_claim_map_inputs(
    measured_revision: str,
    parity_evidence: dict,
    benchmark_evidence: dict,
) -> dict:
    """Build structured inputs for the claim/evidence map."""
    pinned_run_ids = benchmark_evidence.get("pinned_run_ids", [])
    scenario_ids = benchmark_evidence.get("scenario_ids", [])

    claims = []

    # Parity/correctness claims
    if parity_evidence.get("all_passed"):
        claims.append({
            "claim_id": "parity-all-variants",
            "claim": "Rust implementation produces bit-exact output for XXH32, XXH64, XXH3_64, and XXH3_128 across all tested input lengths and seeds",
            "evidence_type": "parity_test",
            "evidence_path": "publication/evidence/parity_summary.json",
            "pinned_revision": measured_revision,
        })

    # SIMD parity claim
    simd_cat = parity_evidence.get("categories", {}).get("xxh3_simd_scalar_parity", {})
    if simd_cat and simd_cat.get("failed", 1) == 0 and simd_cat.get("passed", 0) > 0:
        claims.append({
            "claim_id": "simd-parity",
            "claim": "NEON-optimized XXH3 long-input paths produce bit-exact output matching the scalar reference on Apple Silicon",
            "evidence_type": "parity_test",
            "evidence_path": "publication/evidence/parity_summary.json",
            "pinned_revision": measured_revision,
        })

    # CLI parity claim
    cli_categories = [
        k for k in parity_evidence.get("categories", {})
        if k.startswith("cli_")
    ]
    cli_all_passed = all(
        parity_evidence["categories"][k].get("failed", 1) == 0
        for k in cli_categories
    ) if cli_categories else False
    if cli_all_passed:
        claims.append({
            "claim_id": "cli-behavioral-parity",
            "claim": "CLI achieves behavioral parity with the reference xxhsum for the validated output formats, check modes, and input flows",
            "evidence_type": "parity_test",
            "evidence_path": "publication/evidence/parity_summary.json",
            "pinned_revision": measured_revision,
        })

    # Benchmark correctness gate claim
    cg = benchmark_evidence.get("correctness_gate", {})
    if cg.get("all_passed"):
        claims.append({
            "claim_id": "benchmark-correctness-gate",
            "claim": "C and Rust xxHash binaries agree on digest output for all benchmarked scenarios",
            "evidence_type": "benchmark_correctness_gate",
            "evidence_path": "publication/evidence/benchmark_summary.json",
            "pinned_run_ids": pinned_run_ids,
            "pinned_revision": measured_revision,
        })

    # Per-scenario benchmark claims
    for sid in scenario_ids:
        claims.append({
            "claim_id": f"benchmark-{sid}",
            "claim": f"Benchmark throughput data available for scenario {sid}",
            "evidence_type": "benchmark_samples",
            "evidence_paths": [
                f"publication/evidence/benchmark_runs/{rid}/samples/{sid}.json"
                for rid in pinned_run_ids
            ],
            "pinned_run_ids": pinned_run_ids,
            "pinned_revision": measured_revision,
        })

    return {
        "schema_version": "1.0.0",
        "description": "Claim-to-evidence mapping inputs for publication traceability",
        "measured_revision": measured_revision,
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        "claims": claims,
    }


def main():
    EVIDENCE_DIR.mkdir(parents=True, exist_ok=True)

    measured_revision = get_repo_revision()
    print(f"Measured revision: {measured_revision}")

    # 1. Collect parity evidence
    print("Collecting parity evidence...")
    parity_evidence = collect_parity_evidence()
    print(f"  Parity: {parity_evidence['total_passed']} passed, "
          f"{parity_evidence['total_failed']} failed, "
          f"all_passed={parity_evidence['all_passed']}")

    parity_path = EVIDENCE_DIR / "parity_summary.json"
    parity_output = {
        "schema_version": "1.0.0",
        "description": "Machine-readable parity test results for xxhash-rs",
        "measured_revision": measured_revision,
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        **parity_evidence,
    }
    with open(parity_path, "w") as f:
        json.dump(parity_output, f, indent=2)
        f.write("\n")
    print(f"  Written: {parity_path.relative_to(REPO_ROOT)}")

    # 2. Collect benchmark evidence
    print("Collecting benchmark evidence...")
    benchmark_evidence = collect_benchmark_evidence(measured_revision)
    print(f"  Benchmark: {benchmark_evidence.get('claim_ready_run_count', 0)} "
          f"claim-ready runs, pinned IDs: {benchmark_evidence.get('pinned_run_ids', [])}")

    bench_path = EVIDENCE_DIR / "benchmark_summary.json"
    bench_output = {
        "schema_version": "1.0.0",
        "description": "Benchmark evidence with pinned run IDs for xxhash-rs rewrite study",
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        **benchmark_evidence,
    }
    with open(bench_path, "w") as f:
        json.dump(bench_output, f, indent=2)
        f.write("\n")
    print(f"  Written: {bench_path.relative_to(REPO_ROOT)}")

    # 2b. Snapshot pinned benchmark runs into evidence directory
    pinned_ids = benchmark_evidence.get("pinned_run_ids", [])
    runs_snapshot_dir = EVIDENCE_DIR / "benchmark_runs"
    if runs_snapshot_dir.exists():
        shutil.rmtree(runs_snapshot_dir)
    runs_snapshot_dir.mkdir(parents=True, exist_ok=True)
    for rid in pinned_ids:
        src = RUNS_DIR / rid
        dst = runs_snapshot_dir / rid
        if src.exists():
            shutil.copytree(src, dst)
            print(f"  Snapshot: publication/evidence/benchmark_runs/{rid}")
        else:
            print(f"  WARNING: Pinned run directory not found: {rid}")

    # 3. Build claim/evidence map inputs
    print("Building claim/evidence map inputs...")
    claim_inputs = build_claim_map_inputs(
        measured_revision, parity_evidence, benchmark_evidence,
    )
    claim_path = EVIDENCE_DIR / "claim_map_inputs.json"
    with open(claim_path, "w") as f:
        json.dump(claim_inputs, f, indent=2)
        f.write("\n")
    print(f"  Written: {claim_path.relative_to(REPO_ROOT)}")
    print(f"  Claims: {len(claim_inputs['claims'])}")

    # 4. Build artifact manifest
    print("Building artifact manifest...")
    artifact_manifest = build_artifact_manifest(
        measured_revision, parity_evidence, benchmark_evidence,
    )
    manifest_path = EVIDENCE_DIR / "artifact_manifest.json"
    with open(manifest_path, "w") as f:
        json.dump(artifact_manifest, f, indent=2)
        f.write("\n")
    print(f"  Written: {manifest_path.relative_to(REPO_ROOT)}")

    # Summary
    print("\n=== Evidence Pack Summary ===")
    print(f"  Revision:         {measured_revision}")
    print(f"  Parity:           {'PASS' if parity_evidence['all_passed'] else 'FAIL'}")
    print(f"  Benchmark runs:   {benchmark_evidence.get('claim_ready_run_count', 0)} claim-ready")
    print(f"  Pinned run IDs:   {benchmark_evidence.get('pinned_run_ids', [])}")
    print(f"  Claim inputs:     {len(claim_inputs['claims'])} claims mapped")
    print(f"  Output directory:  publication/evidence/")

    if not parity_evidence["all_passed"]:
        print("\nWARNING: Parity tests did not all pass!")
        sys.exit(1)

    if benchmark_evidence.get("claim_ready_run_count", 0) < 3:
        print("\nWARNING: Fewer than 3 claim-ready benchmark runs at measured revision")
        sys.exit(1)

    print("\nEvidence pack generated successfully.")
    return 0


if __name__ == "__main__":
    sys.exit(main() or 0)
