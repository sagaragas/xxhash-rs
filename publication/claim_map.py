#!/usr/bin/env python3
"""
Claim/evidence map for the xxhash-rs rewrite study.

Validates that every material claim in the publication has a corresponding
evidence artifact with a pinned revision or run ID, and that no claims
reference mutable latest pointers.

Usage:
    python3 publication/claim_map.py --prepare-inputs
    python3 publication/claim_map.py --verify
"""

import argparse
import json
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
EVIDENCE_DIR = REPO_ROOT / "publication" / "evidence"
CLAIM_MAP_PATH = EVIDENCE_DIR / "claim_map_inputs.json"
ARTIFACT_MANIFEST_PATH = EVIDENCE_DIR / "artifact_manifest.json"


def load_json(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


def prepare_inputs():
    """Generate or refresh the claim/evidence map inputs by running the evidence generator."""
    import subprocess

    gen_script = REPO_ROOT / "publication" / "generate_evidence.py"
    result = subprocess.run(
        [sys.executable, str(gen_script)],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )

    if result.returncode != 0:
        print("ERROR: Evidence generation failed:")
        print(result.stdout)
        print(result.stderr)
        return 1

    print(result.stdout)

    # Now verify the generated inputs
    return verify()


def verify():
    """Verify that the claim/evidence map is complete and well-formed."""
    errors = []

    # 1. Check that claim_map_inputs.json exists and is valid
    if not CLAIM_MAP_PATH.exists():
        print(f"FAIL: {CLAIM_MAP_PATH.relative_to(REPO_ROOT)} does not exist")
        print("  Run: python3 publication/claim_map.py --prepare-inputs")
        return 1

    claim_data = load_json(CLAIM_MAP_PATH)

    # 2. Check schema version
    if claim_data.get("schema_version") != "1.0.0":
        errors.append("Missing or unexpected schema_version in claim_map_inputs.json")

    # 3. Check measured revision is present and non-empty
    revision = claim_data.get("measured_revision", "")
    if not revision or len(revision) < 7:
        errors.append("Missing or invalid measured_revision")

    # 4. Check claims array
    claims = claim_data.get("claims", [])
    if not claims:
        errors.append("No claims found in claim_map_inputs.json")

    # 5. Validate each claim
    for claim in claims:
        claim_id = claim.get("claim_id", "<unknown>")

        # Must have pinned revision
        if not claim.get("pinned_revision"):
            errors.append(f"Claim {claim_id}: missing pinned_revision")

        # Must have evidence path(s)
        evidence_path = claim.get("evidence_path")
        evidence_paths = claim.get("evidence_paths", [])
        if not evidence_path and not evidence_paths:
            errors.append(f"Claim {claim_id}: no evidence_path or evidence_paths")

        # Evidence paths must not contain 'latest'
        all_paths = [evidence_path] if evidence_path else []
        all_paths.extend(evidence_paths)
        for p in all_paths:
            if p and "latest" in p.lower():
                errors.append(f"Claim {claim_id}: evidence path contains mutable 'latest': {p}")

        # Pinned run IDs (if present) must not be 'latest'
        for rid in claim.get("pinned_run_ids", []):
            if "latest" in rid.lower():
                errors.append(f"Claim {claim_id}: pinned_run_id contains 'latest': {rid}")

    # 6. Check artifact manifest exists
    if not ARTIFACT_MANIFEST_PATH.exists():
        errors.append(f"{ARTIFACT_MANIFEST_PATH.relative_to(REPO_ROOT)} does not exist")
    else:
        manifest = load_json(ARTIFACT_MANIFEST_PATH)
        manifest_rev = manifest.get("measured_revision", "")
        if manifest_rev != revision:
            errors.append(
                f"Revision mismatch: claim_map says {revision}, "
                f"artifact_manifest says {manifest_rev}"
            )

    # 7. Check parity and benchmark summaries exist
    parity_path = EVIDENCE_DIR / "parity_summary.json"
    bench_path = EVIDENCE_DIR / "benchmark_summary.json"

    if not parity_path.exists():
        errors.append("publication/evidence/parity_summary.json does not exist")
    else:
        parity = load_json(parity_path)
        if not parity.get("all_passed"):
            errors.append("Parity evidence shows failures")
        if parity.get("measured_revision") != revision:
            errors.append("Parity summary revision does not match claim map revision")

    if not bench_path.exists():
        errors.append("publication/evidence/benchmark_summary.json does not exist")
    else:
        bench = load_json(bench_path)
        if bench.get("measured_revision") != revision:
            errors.append("Benchmark summary revision does not match claim map revision")
        if bench.get("claim_ready_run_count", 0) < 3:
            errors.append(
                f"Fewer than 3 claim-ready runs: {bench.get('claim_ready_run_count', 0)}"
            )

    # Report
    if errors:
        print(f"FAIL: {len(errors)} claim-map verification error(s):")
        for e in errors:
            print(f"  - {e}")
        return 1

    print(f"OK: Claim/evidence map verified ({len(claims)} claims, "
          f"revision {revision[:12]})")
    return 0


def main():
    parser = argparse.ArgumentParser(
        description="Claim/evidence map for xxhash-rs publication",
    )
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument(
        "--prepare-inputs",
        action="store_true",
        help="Generate evidence and build claim/evidence map inputs",
    )
    group.add_argument(
        "--verify",
        action="store_true",
        help="Verify existing claim/evidence map completeness",
    )

    args = parser.parse_args()

    if args.prepare_inputs:
        return prepare_inputs()
    elif args.verify:
        return verify()


if __name__ == "__main__":
    sys.exit(main() or 0)
