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

    # Verify per-scenario oracle digest agreement AND non-emptiness
    results = gate.get("results", [])
    for r in results:
        digests = r.get("oracle_digests", {})
        c_digest = digests.get("c_xxhsum", "")
        rust_digest = digests.get("rust_xxhash_rs", "")

        # Both oracle digests must be present and non-empty
        if not c_digest:
            issues.append(
                f"  Scenario {r.get('scenario_id', '?')}: "
                f"c_xxhsum digest is missing or empty"
            )
            return False, issues
        if not rust_digest:
            issues.append(
                f"  Scenario {r.get('scenario_id', '?')}: "
                f"rust_xxhash_rs digest is missing or empty"
            )
            return False, issues

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


def get_compatible_run_set(manifest: dict) -> list[dict]:
    """Return the subset of claim-ready index entries that share the target
    manifest's revision and manifest/policy hashes.

    The compatible run set is the only set that may be counted toward
    claim-readiness thresholds.  Runs with a different revision or
    different scenario/comparator/policy hashes are excluded even if
    they are individually claim-ready.
    """
    index_path = RUNS_DIR / "index.json"
    if not index_path.exists():
        return []

    index = load_json(index_path)

    target_revision = manifest.get("environment", {}).get("repo_revision")
    target_hashes = manifest.get("manifest_hashes", {})

    compatible = []
    for entry in index.get("runs", []):
        # Must be complete and claim-ready
        if entry.get("status") != "complete" or entry.get("claim_ready") is not True:
            continue

        # Check revision match (from index entry or fall back to manifest file)
        entry_revision = entry.get("revision")
        entry_hashes = entry.get("manifest_hashes", {})

        # If index entry lacks extended metadata, try loading from manifest file
        if not entry_revision or not entry_hashes:
            run_dir = RUNS_DIR / entry["run_id"]
            manifest_path = run_dir / "manifest.json"
            if manifest_path.exists():
                run_manifest = load_json(manifest_path)
                if not entry_revision:
                    entry_revision = run_manifest.get("environment", {}).get("repo_revision")
                if not entry_hashes:
                    entry_hashes = run_manifest.get("manifest_hashes", {})

        # Filter: revision must match
        if entry_revision != target_revision:
            continue

        # Filter: all required hashes must match
        required_keys = ["scenarios", "comparators", "policy"]
        hashes_match = all(
            entry_hashes.get(k) == target_hashes.get(k)
            for k in required_keys
        )
        if not hashes_match:
            continue

        # Augment the entry with the resolved metadata for downstream callers
        compatible.append({
            **entry,
            "revision": entry_revision,
            "manifest_hashes": entry_hashes,
        })

    return compatible


def check_run_set_consistency(manifest: dict, policy: dict) -> tuple[bool, list[str]]:
    """Check that the run's revision and manifests are consistent with the
    compatible run set.

    The compatible run set consists of all claim-ready runs that share
    the same revision and manifest/policy hashes as the target run.
    Consistency requires:
    1. The target run records a revision and all required manifest hashes.
    2. The compatible run set is non-empty.
    3. Every member of the compatible run set has exactly one unique
       revision and one unique set of required hashes (singleton sets).
    """
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

    # If any prerequisite metadata is missing, bail early
    if issues:
        return False, issues

    # Build the compatible run set
    compatible = get_compatible_run_set(manifest)

    if not compatible:
        issues.append(
            "Compatible run set is empty: no claim-ready runs share the "
            "same revision and manifest/policy hashes as this run"
        )
        return False, issues

    # Assert singleton revision set across compatible runs
    revisions = {r.get("revision") for r in compatible}
    if len(revisions) != 1:
        issues.append(
            f"Compatible run set has divergent revisions: {revisions}"
        )

    # Assert singleton hash set across compatible runs
    hash_tuples = set()
    for r in compatible:
        h = r.get("manifest_hashes", {})
        hash_tuples.add(tuple(h.get(k, "") for k in required_hashes))
    if len(hash_tuples) != 1:
        issues.append(
            f"Compatible run set has divergent manifest/policy hashes: "
            f"{len(hash_tuples)} distinct hash sets found"
        )

    # Check policy version consistency
    policy_version = manifest.get("policy_version")
    current_policy_version = policy.get("policy_version")
    if policy_version and current_policy_version and policy_version != current_policy_version:
        issues.append(
            f"Policy version mismatch: run={policy_version}, current={current_policy_version}"
        )

    return len(issues) == 0, issues


def check_minimum_run_set(manifest: dict, policy: dict) -> tuple[bool, list[str]]:
    """Check that the minimum number of compatible claim-ready runs exist.

    Only runs in the compatible run set (same revision and manifest/policy
    hashes as the target manifest) are counted.  Heterogeneous global
    runs are excluded.
    """
    issues = []
    claim_policy = policy.get("claim_readiness", {})
    minimum_runs = claim_policy.get("minimum_runs", 1)

    compatible = get_compatible_run_set(manifest)
    compatible_count = len(compatible)

    if compatible_count < minimum_runs:
        issues.append(
            f"Minimum run set: {compatible_count} compatible claim-ready run(s) < "
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
        ("Minimum run set", check_minimum_run_set(manifest, policy)),
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
