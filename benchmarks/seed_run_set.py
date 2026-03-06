#!/usr/bin/env python3
"""
Deterministic run-set fixture seeder for isolated benchmark testing.

Creates a self-contained runs directory with a configurable number of
compatible claim-ready runs, suitable for exercising claim-gate and
reconcile validation without relying on ambient mutable benchmark state.

The seeded runs share a single deterministic revision, manifest hashes,
and policy metadata so they form a valid compatible multi-run set.

Usage (as a module):
    from benchmarks.seed_run_set import seed_compatible_run_set
    run_dir = seed_compatible_run_set(tmpdir, num_runs=3)

Usage (as a CLI):
    python3 benchmarks/seed_run_set.py --output /tmp/test-runs --num-runs 3
"""

import argparse
import hashlib
import json
import sys
from pathlib import Path


# Deterministic fixture constants
DEFAULT_REVISION = "deadbeef" * 5  # 40 hex chars
DEFAULT_SCENARIOS_HASH = "s_" + "a1b2c3d4" * 7
DEFAULT_COMPARATORS_HASH = "c_" + "e5f6a7b8" * 7
DEFAULT_POLICY_HASH = "p_" + "11223344" * 7
DEFAULT_POLICY_VERSION = "1.0.0"

CANONICAL_COMPARATOR_IDS = ["c_xxhsum", "rust_xxhash_rs", "b3sum", "md5"]

# Deterministic scenario IDs that match the shape of real benchmark runs
DEFAULT_SCENARIO_IDS = [
    "xxh64-4k",
    "xxh64-1m",
    "xxh3-128-1m",
    "xxh64-16m",
]


def _file_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def _make_sample_data(
    scenario_id: str,
    algorithm: str = "XXH64",
    payload_bytes: int = 1048576,
    oracle_digest: str = "abcdef0123456789",
) -> dict:
    """Build a deterministic scenario sample with all comparator results."""
    comparator_results = {}
    coverage_ledger = {}

    for comp_id in CANONICAL_COMPARATOR_IDS:
        if comp_id in ("c_xxhsum", "rust_xxhash_rs"):
            digest_line = f"{oracle_digest}  payload_{payload_bytes}.bin"
        else:
            # Contrast comparators produce different but valid digests
            digest_line = f"{'ff' * 16}  payload_{payload_bytes}.bin"

        # Two measured samples: [1500000, 1600000].  The harness median
        # algorithm picks the element at index len//2 of the sorted list,
        # which for 2 elements is index 1 → 1600000.
        measured_samples = [
            {
                "command": [f"/usr/bin/{comp_id}", f"payload_{payload_bytes}.bin"],
                "elapsed_ns": 1500000 + i * 100000,
                "exit_code": 0,
                "stdout_first_line": digest_line,
                "stderr_snippet": "",
                "success": True,
            }
            for i in range(2)
        ]
        # Compute the same median as reconcile.recompute_median
        elapsed_sorted = sorted(s["elapsed_ns"] for s in measured_samples)
        declared_median = elapsed_sorted[len(elapsed_sorted) // 2]

        comparator_results[comp_id] = {
            "status": "success",
            "invocation": [f"/usr/bin/{comp_id}", f"payload_{payload_bytes}.bin"],
            "warmup_samples": [
                {
                    "command": [f"/usr/bin/{comp_id}", f"payload_{payload_bytes}.bin"],
                    "elapsed_ns": 1000000,
                    "exit_code": 0,
                    "stdout_first_line": digest_line,
                    "stderr_snippet": "",
                    "success": True,
                }
            ],
            "measured_samples": measured_samples,
            "median_ns": declared_median,
            "sample_count": 2,
        }
        coverage_ledger[comp_id] = "success"

    return {
        "scenario_id": scenario_id,
        "algorithm": algorithm,
        "payload_bytes": payload_bytes,
        "payload_checksum": hashlib.sha256(b"deterministic").hexdigest(),
        "warmup_iterations": 1,
        "measured_iterations": 2,
        "comparator_results": comparator_results,
        "coverage_ledger": coverage_ledger,
    }


def _make_manifest(
    run_id: str,
    run_index: int,
    revision: str = DEFAULT_REVISION,
    scenarios_hash: str = DEFAULT_SCENARIOS_HASH,
    comparators_hash: str = DEFAULT_COMPARATORS_HASH,
    policy_hash: str = DEFAULT_POLICY_HASH,
    policy_version: str = DEFAULT_POLICY_VERSION,
    scenario_ids: list[str] | None = None,
    oracle_digest: str = "abcdef0123456789",
) -> dict:
    """Build a deterministic run manifest."""
    if scenario_ids is None:
        scenario_ids = DEFAULT_SCENARIO_IDS

    correctness_results = []
    for sid in scenario_ids:
        correctness_results.append({
            "scenario_id": sid,
            "passed": True,
            "reason": "All oracles agree and contrast comparators succeeded",
            "oracle_digests": {
                "c_xxhsum": oracle_digest,
                "rust_xxhash_rs": oracle_digest,
            },
        })

    # Use deterministic timestamps offset by run_index
    base_ts = f"2026-01-15T12:{run_index:02d}:00+00:00"

    return {
        "run_id": run_id,
        "run_type": "smoke",
        "timestamp_utc": base_ts,
        "status": "complete",
        "claim_ready": True,
        "manifest_hashes": {
            "scenarios": scenarios_hash,
            "comparators": comparators_hash,
            "policy": policy_hash,
        },
        "policy_version": policy_version,
        "policy_hash": policy_hash,
        "environment": {
            "repo_revision": revision,
            "hostname": "test-host",
            "platform": "test-platform",
            "machine": "arm64",
            "python_version": "3.14.0",
            "timestamp_utc": base_ts,
            "repo_dirty": False,
        },
        "resolved_comparators": {
            comp_id: {
                "id": comp_id,
                "binary_path": f"/usr/bin/{comp_id}",
                "version": "1.0.0-test",
                "role": "oracle" if comp_id in ("c_xxhsum", "rust_xxhash_rs") else "contrast",
                "parity_oracle": comp_id in ("c_xxhsum", "rust_xxhash_rs"),
            }
            for comp_id in CANONICAL_COMPARATOR_IDS
        },
        "correctness_gate": {
            "all_passed": True,
            "results": correctness_results,
        },
        "completeness": {
            "complete": True,
            "missing_entries": [],
        },
        "statistical_method": {
            "warmup_policy": "discard",
            "summary_statistic": "median",
            "retain_raw_samples": True,
        },
        "scenario_count": len(scenario_ids),
        "comparator_ids": CANONICAL_COMPARATOR_IDS,
    }


def _write_json(path: Path, data: dict) -> None:
    with open(path, "w") as f:
        json.dump(data, f, indent=2, default=str)
        f.write("\n")


def seed_compatible_run_set(
    output_dir: Path,
    num_runs: int = 3,
    revision: str = DEFAULT_REVISION,
    scenarios_hash: str = DEFAULT_SCENARIOS_HASH,
    comparators_hash: str = DEFAULT_COMPARATORS_HASH,
    policy_hash: str = DEFAULT_POLICY_HASH,
    policy_version: str = DEFAULT_POLICY_VERSION,
    scenario_ids: list[str] | None = None,
    oracle_digest: str = "abcdef0123456789",
) -> Path:
    """Seed a deterministic compatible multi-run set into the given directory.

    Returns the output_dir path for convenience.

    The seeded runs are fully self-contained and exercise:
    - compatible run set selection (all share revision + hashes)
    - minimum-runs policy enforcement
    - correctness gate pass/fail
    - reconciliation (raw samples reconcile to declared medians)
    - artifact checksum integrity
    """
    if scenario_ids is None:
        scenario_ids = DEFAULT_SCENARIO_IDS

    output_dir.mkdir(parents=True, exist_ok=True)

    index_entries = []

    for i in range(num_runs):
        run_id = f"seed-run-{i:03d}"
        run_dir = output_dir / run_id
        run_dir.mkdir(parents=True, exist_ok=True)

        # Create manifest
        manifest = _make_manifest(
            run_id=run_id,
            run_index=i,
            revision=revision,
            scenarios_hash=scenarios_hash,
            comparators_hash=comparators_hash,
            policy_hash=policy_hash,
            policy_version=policy_version,
            scenario_ids=scenario_ids,
            oracle_digest=oracle_digest,
        )
        _write_json(run_dir / "manifest.json", manifest)

        # Create sample files per scenario
        samples_dir = run_dir / "samples"
        samples_dir.mkdir(exist_ok=True)
        for sid in scenario_ids:
            sample = _make_sample_data(
                scenario_id=sid,
                oracle_digest=oracle_digest,
            )
            _write_json(samples_dir / f"{sid}.json", sample)

        # Create checksums.json with actual file hashes
        checksums = {}
        for f in sorted(run_dir.rglob("*.json")):
            rel = f.relative_to(run_dir)
            if str(rel) == "checksums.json":
                continue
            checksums[str(rel)] = _file_sha256(f)
        _write_json(run_dir / "checksums.json", checksums)

        # Index entry
        index_entries.append({
            "run_id": run_id,
            "timestamp_utc": manifest["timestamp_utc"],
            "run_type": manifest["run_type"],
            "status": manifest["status"],
            "claim_ready": manifest["claim_ready"],
            "scenario_count": manifest["scenario_count"],
            "revision": revision,
            "manifest_hashes": manifest["manifest_hashes"],
        })

    # Write index.json
    _write_json(output_dir / "index.json", {"runs": index_entries})

    return output_dir


def seed_policy(
    output_dir: Path,
    policy_version: str = DEFAULT_POLICY_VERSION,
    minimum_runs: int = 3,
) -> Path:
    """Write a deterministic policy.json to the given directory.

    Returns the path to the written policy file.
    """
    policy = {
        "schema_version": "1.0.0",
        "description": "Deterministic test policy for isolated benchmark validation",
        "policy_version": policy_version,
        "correctness_gate": {
            "description": "c_xxhsum and rust_xxhash_rs must agree on digest",
            "oracle_comparators": ["c_xxhsum", "rust_xxhash_rs"],
            "contrast_comparators": ["b3sum", "md5"],
            "oracle_must_agree": True,
            "contrast_must_execute": True,
        },
        "completeness": {
            "description": "All canonical comparators must execute for every scenario",
            "require_full_matrix": True,
            "allow_partial_runs": False,
        },
        "statistical_method": {
            "description": "Median of measured samples after warmup",
            "warmup_policy": "discard",
            "summary_statistic": "median",
            "retain_raw_samples": True,
        },
        "claim_readiness": {
            "description": "Claim readiness with configurable minimum runs",
            "require_correctness_gate": True,
            "require_full_matrix": True,
            "require_artifact_checksums": True,
            "require_matching_revision": True,
            "require_matching_manifests": True,
            "minimum_runs": minimum_runs,
        },
        "latest_resolution": {
            "description": "latest resolves only to the most recent complete, claim-ready run",
            "require_complete": True,
            "require_claim_ready": True,
            "exclude_partial": True,
        },
    }
    policy_path = output_dir / "policy.json"
    _write_json(policy_path, policy)
    return policy_path


def main():
    parser = argparse.ArgumentParser(
        description="Seed a deterministic compatible run set for isolated testing"
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Output directory for the seeded runs",
    )
    parser.add_argument(
        "--num-runs",
        type=int,
        default=3,
        help="Number of compatible runs to seed (default: 3)",
    )
    parser.add_argument(
        "--with-policy",
        action="store_true",
        help="Also write a deterministic policy.json in the output directory",
    )
    args = parser.parse_args()

    output_dir = Path(args.output)
    seed_compatible_run_set(output_dir, num_runs=args.num_runs)
    print(f"Seeded {args.num_runs} compatible runs in {output_dir}")

    if args.with_policy:
        policy_path = seed_policy(output_dir)
        print(f"Policy written to {policy_path}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
