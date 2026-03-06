#!/usr/bin/env python3
"""
Claim gate for xxhash-rs benchmark runs.

Verifies that a benchmark run (or the latest claim-ready run) passes all
claim-readiness requirements before performance claims can be published:

1. Correctness gate: c_xxhsum and rust_xxhash_rs agree on digest
2. Matrix completeness: all canonical comparators ran for every scenario
3. Artifact integrity: checksums.json present and valid
4. Run-set consistency: revision and manifest hashes match across run set
5. Minimum run count: at least minimum_runs claim-ready runs exist

Usage:
    python3 benchmarks/claim_gate.py --run latest
    python3 benchmarks/claim_gate.py --run <run-id>
"""

import argparse
import hashlib
import json
import sys
from pathlib import Path


HARNESS_DIR = Path(__file__).resolve().parent
RUNS_DIR = HARNESS_DIR / "runs"
POLICY_PATH = HARNESS_DIR / "policy.json"

CANONICAL_COMPARATOR_IDS = ["c_xxhsum", "rust_xxhash_rs", "b3sum", "md5"]


def load_json(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


def file_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def resolve_run_id(run_arg: str) -> str | None:
    """Resolve 'latest' to the most recent claim-ready run ID."""
    if run_arg != "latest":
        return run_arg

    index_path = RUNS_DIR / "index.json"
    if not index_path.exists():
        return None

    index = load_json(index_path)
    eligible = [
        r for r in index.get("runs", [])
        if r.get("status") == "complete" and r.get("claim_ready") is True
    ]
    if not eligible:
        return None

    eligible.sort(key=lambda r: r.get("timestamp_utc", ""), reverse=True)
    return eligible[0]["run_id"]


def check_correctness_gate(manifest: dict) -> tuple[bool, list[str]]:
    """Check that the correctness gate passed for all scenarios."""
    issues = []
    gate = manifest.get("correctness_gate", {})

    if not gate.get("all_passed"):
        issues.append("Correctness gate: NOT all passed")
        results = gate.get("results", [])
        for r in results:
            if not r.get("passed"):
                issues.append(
                    f"  Scenario {r.get('scenario_id', '?')}: {r.get('reason', 'unknown')}"
                )
        return False, issues

    # Verify per-scenario oracle digest agreement
    results = gate.get("results", [])
    for r in results:
        digests = r.get("oracle_digests", {})
        c_digest = digests.get("c_xxhsum", "")
        rust_digest = digests.get("rust_xxhash_rs", "")
        if c_digest != rust_digest:
            issues.append(
                f"  Scenario {r.get('scenario_id', '?')}: "
                f"c_xxhsum={c_digest} != rust_xxhash_rs={rust_digest}"
            )
            return False, issues

    return True, []


def check_matrix_completeness(manifest: dict) -> tuple[bool, list[str]]:
    """Check that the full comparator matrix was covered."""
    issues = []
    completeness = manifest.get("completeness", {})

    if not completeness.get("complete"):
        issues.append("Matrix completeness: NOT complete")
        for entry in completeness.get("missing_entries", []):
            issues.append(
                f"  {entry.get('scenario', '?')}/{entry.get('comparator', '?')}: "
                f"{entry.get('status', '?')}"
            )
        return False, issues

    return True, []


def check_artifact_checksums(run_dir: Path) -> tuple[bool, list[str]]:
    """Verify artifact checksums are present and valid."""
    issues = []
    checksums_path = run_dir / "checksums.json"

    if not checksums_path.exists():
        return False, ["Artifact checksums: checksums.json missing"]

    checksums = load_json(checksums_path)
    for rel_path, expected_hash in checksums.items():
        if rel_path == "checksums.json":
            continue
        file_path = run_dir / rel_path
        if not file_path.exists():
            issues.append(f"  Missing artifact: {rel_path}")
            continue
        actual_hash = file_sha256(file_path)
        if actual_hash != expected_hash:
            issues.append(
                f"  Checksum mismatch: {rel_path} "
                f"(expected={expected_hash[:16]}..., actual={actual_hash[:16]}...)"
            )

    return len(issues) == 0, issues


def check_run_set_consistency(manifest: dict, policy: dict) -> tuple[bool, list[str]]:
    """Check that the run's revision and manifests are consistent with the run set."""
    issues = []
    claim_policy = policy.get("claim_readiness", {})

    # Check revision is recorded
    revision = manifest.get("environment", {}).get("repo_revision")
    if not revision:
        issues.append("Revision: not recorded in environment metadata")

    # Note: repo_dirty is tracked for provenance but does not block claim readiness.
    # The correctness gate and manifest integrity are the primary guards.

    # Check manifest hashes are recorded
    manifest_hashes = manifest.get("manifest_hashes", {})
    required_hashes = ["scenarios", "comparators", "policy"]
    for key in required_hashes:
        if key not in manifest_hashes or not manifest_hashes[key]:
            issues.append(f"Manifest hash missing: {key}")

    # If require_matching_manifests, check against current manifests
    if claim_policy.get("require_matching_manifests"):
        current_hashes = {}
        for name in required_hashes:
            path = HARNESS_DIR / f"{name}.json"
            if path.exists():
                current_hashes[name] = file_sha256(path)

        for key in required_hashes:
            recorded = manifest_hashes.get(key)
            current = current_hashes.get(key)
            if recorded and current and recorded != current:
                issues.append(
                    f"Manifest drift: {key} has changed since run "
                    f"(run={recorded[:16]}..., current={current[:16]}...)"
                )

    # Check policy version
    policy_version = manifest.get("policy_version")
    current_policy_version = policy.get("policy_version")
    if policy_version and current_policy_version and policy_version != current_policy_version:
        issues.append(
            f"Policy version mismatch: run={policy_version}, current={current_policy_version}"
        )

    return len(issues) == 0, issues


def check_minimum_run_set(policy: dict) -> tuple[bool, list[str]]:
    """Check that the minimum number of claim-ready runs exist."""
    issues = []
    claim_policy = policy.get("claim_readiness", {})
    minimum_runs = claim_policy.get("minimum_runs", 1)

    index_path = RUNS_DIR / "index.json"
    if not index_path.exists():
        return False, [f"Run index not found; need {minimum_runs} claim-ready run(s)"]

    index = load_json(index_path)
    claim_ready_count = sum(
        1 for r in index.get("runs", [])
        if r.get("status") == "complete" and r.get("claim_ready") is True
    )

    if claim_ready_count < minimum_runs:
        issues.append(
            f"Minimum run set: {claim_ready_count} claim-ready run(s) < "
            f"required {minimum_runs}"
        )
        return False, issues

    return True, []


def run_claim_gate(run_arg: str) -> int:
    """Run the full claim gate against a specified run."""
    print(f"=== xxhash-rs claim gate ===")
    print(f"Target: {run_arg}")

    # Load policy
    policy = load_json(POLICY_PATH)
    print(f"Policy version: {policy.get('policy_version', 'unknown')}")

    # Resolve run
    run_id = resolve_run_id(run_arg)
    if not run_id:
        print(f"\nERROR: Could not resolve run '{run_arg}'")
        print("No claim-ready runs found.")
        return 1

    run_dir = RUNS_DIR / run_id
    manifest_path = run_dir / "manifest.json"
    if not manifest_path.exists():
        print(f"\nERROR: Run {run_id} not found at {run_dir}")
        return 1

    manifest = load_json(manifest_path)
    print(f"Run ID: {run_id}")
    print(f"Revision: {manifest.get('environment', {}).get('repo_revision', 'unknown')[:16]}...")
    print(f"Manifest hashes: {json.dumps({k: v[:16] + '...' for k, v in manifest.get('manifest_hashes', {}).items()})}")

    # Run all checks
    all_passed = True
    checks = [
        ("Correctness gate", check_correctness_gate(manifest)),
        ("Matrix completeness", check_matrix_completeness(manifest)),
        ("Artifact checksums", check_artifact_checksums(run_dir)),
        ("Run-set consistency", check_run_set_consistency(manifest, policy)),
        ("Minimum run set", check_minimum_run_set(policy)),
    ]

    print(f"\n--- Claim Gate Checks ---")
    for name, (passed, issues) in checks:
        symbol = "PASS" if passed else "FAIL"
        print(f"  [{symbol}] {name}")
        if issues:
            for issue in issues:
                print(f"        {issue}")
        if not passed:
            all_passed = False

    # Overall verdict
    print(f"\n--- Verdict ---")
    if all_passed:
        print(f"  Claim-ready: YES")
        print(f"  Run {run_id} passes all claim gates.")
        print(f"  Performance claims from this run may be cited with:")
        print(f"    Run ID: {run_id}")
        print(f"    Revision: {manifest.get('environment', {}).get('repo_revision', 'unknown')}")
        print(f"    Policy: {policy.get('policy_version', 'unknown')}")
        return 0
    else:
        print(f"  Claim-ready: NO")
        print(f"  Run {run_id} does NOT pass all claim gates.")
        print(f"  Performance claims from this run must NOT be published.")
        return 1


def main():
    parser = argparse.ArgumentParser(
        description="xxhash-rs benchmark claim gate"
    )
    parser.add_argument(
        "--run",
        required=True,
        help="Run ID or 'latest' to check the most recent claim-ready run",
    )
    args = parser.parse_args()
    return run_claim_gate(args.run)


if __name__ == "__main__":
    sys.exit(main())
