#!/usr/bin/env python3
"""
Traceability check for the xxhash-rs publication evidence pack.

Verifies that publication, parity, and benchmark artifacts share one
measured revision lineage (VAL-CROSS-003), that stable artifact paths
exist for all referenced evidence, and that no mutable latest links
are used as publication evidence targets.

Usage:
    python3 publication/traceability_check.py
"""

import hashlib
import json
import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
EVIDENCE_DIR = REPO_ROOT / "publication" / "evidence"
BENCHMARKS_DIR = REPO_ROOT / "benchmarks"


def load_json(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


def file_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def check_evidence_files_exist() -> list:
    """Check that all required evidence files exist."""
    errors = []
    required_files = [
        EVIDENCE_DIR / "artifact_manifest.json",
        EVIDENCE_DIR / "parity_summary.json",
        EVIDENCE_DIR / "benchmark_summary.json",
        EVIDENCE_DIR / "claim_map_inputs.json",
        EVIDENCE_DIR / "clean_checkout_provenance.json",
    ]
    for f in required_files:
        if not f.exists():
            errors.append(f"Missing evidence file: {f.relative_to(REPO_ROOT)}")
    return errors


def check_revision_lineage() -> list:
    """Verify all evidence artifacts share one measured revision."""
    errors = []

    manifest = load_json(EVIDENCE_DIR / "artifact_manifest.json")
    parity = load_json(EVIDENCE_DIR / "parity_summary.json")
    benchmark = load_json(EVIDENCE_DIR / "benchmark_summary.json")
    claim_map = load_json(EVIDENCE_DIR / "claim_map_inputs.json")
    provenance = load_json(EVIDENCE_DIR / "clean_checkout_provenance.json")

    revisions = {
        "artifact_manifest": manifest.get("measured_revision"),
        "parity_summary": parity.get("measured_revision"),
        "benchmark_summary": benchmark.get("measured_revision"),
        "claim_map_inputs": claim_map.get("measured_revision"),
        "clean_checkout_provenance": provenance.get("measured_revision"),
    }

    unique_revisions = set(v for v in revisions.values() if v)

    if len(unique_revisions) == 0:
        errors.append("No measured revisions found in any evidence file")
    elif len(unique_revisions) > 1:
        errors.append(
            f"Revision lineage mismatch across evidence files: "
            f"{json.dumps(revisions, indent=2)}"
        )

    # Check that the revision is a valid git ref (40-char hex hash or tag name)
    for name, rev in revisions.items():
        if not rev:
            continue
        is_hex_hash = len(rev) == 40 and all(c in "0123456789abcdef" for c in rev)
        is_tag_ref = bool(re.match(r'^[a-zA-Z0-9._-]+$', rev)) and len(rev) >= 3
        if not is_hex_hash and not is_tag_ref:
            errors.append(f"{name}: measured_revision is not a valid git ref: {rev}")

    return errors


def check_pinned_benchmark_runs() -> list:
    """Verify that pinned benchmark run snapshots exist and are complete."""
    errors = []

    benchmark = load_json(EVIDENCE_DIR / "benchmark_summary.json")
    pinned_ids = benchmark.get("pinned_run_ids", [])

    if not pinned_ids:
        errors.append("No pinned benchmark run IDs in benchmark_summary.json")
        return errors

    # Check snapshots in evidence directory (committed, stable paths)
    snapshot_dir = EVIDENCE_DIR / "benchmark_runs"

    for rid in pinned_ids:
        # Check no 'latest' in run ID
        if "latest" in rid.lower():
            errors.append(f"Pinned run ID contains mutable 'latest': {rid}")
            continue

        # Require committed snapshot in the evidence directory (no fallback
        # to mutable benchmarks/runs state)
        run_dir = snapshot_dir / rid
        if not run_dir.exists():
            errors.append(
                f"Pinned run snapshot missing from committed evidence: "
                f"publication/evidence/benchmark_runs/{rid}"
            )
            continue

        # Check required run artifacts exist
        manifest_path = run_dir / "manifest.json"
        checksums_path = run_dir / "checksums.json"
        samples_dir = run_dir / "samples"

        if not manifest_path.exists():
            errors.append(f"Missing manifest.json in run {rid}")
        if not checksums_path.exists():
            errors.append(f"Missing checksums.json in run {rid}")
        if not samples_dir.exists() or not any(samples_dir.iterdir()):
            errors.append(f"Missing or empty samples/ in run {rid}")

        # Verify checksum integrity
        if checksums_path.exists():
            checksums = load_json(checksums_path)
            for rel_file, expected_hash in checksums.items():
                file_path = run_dir / rel_file
                if not file_path.exists():
                    errors.append(
                        f"Checksummed file missing in run {rid}: {rel_file}"
                    )
                else:
                    actual = file_sha256(file_path)
                    if actual != expected_hash:
                        errors.append(
                            f"Checksum mismatch in run {rid}/{rel_file}: "
                            f"expected {expected_hash[:16]}..., got {actual[:16]}..."
                        )

    return errors


def check_parity_evidence() -> list:
    """Verify parity evidence is present and passing."""
    errors = []

    parity = load_json(EVIDENCE_DIR / "parity_summary.json")

    if not parity.get("all_passed"):
        errors.append("Parity evidence shows test failures")

    categories = parity.get("categories", {})
    if not categories:
        errors.append("Parity evidence has no test categories")

    # Check that key parity areas are covered
    required_areas = [
        "oneshot_reference_parity",
        "streaming_chunk_parity",
        "xxh3_simd_scalar_parity",
    ]
    for area in required_areas:
        if area not in categories:
            errors.append(f"Missing required parity area: {area}")
        elif categories[area].get("passed", 0) == 0:
            errors.append(f"Parity area {area} has 0 passed tests")

    return errors


def check_manifest_file_integrity() -> list:
    """Verify manifest file checksums are correct."""
    errors = []

    manifest = load_json(EVIDENCE_DIR / "artifact_manifest.json")
    file_checksums = manifest.get("manifest_file_checksums", {})

    for rel_path, expected_hash in file_checksums.items():
        abs_path = REPO_ROOT / rel_path
        if not abs_path.exists():
            errors.append(f"Manifest-referenced file missing: {rel_path}")
        else:
            actual = file_sha256(abs_path)
            if actual != expected_hash:
                errors.append(
                    f"Checksum mismatch for {rel_path}: "
                    f"expected {expected_hash[:16]}..., got {actual[:16]}..."
                )

    return errors


def check_no_latest_references() -> list:
    """Scan evidence files for any mutable 'latest' references."""
    errors = []

    claim_map = load_json(EVIDENCE_DIR / "claim_map_inputs.json")
    for claim in claim_map.get("claims", []):
        cid = claim.get("claim_id", "<unknown>")
        for path in claim.get("evidence_paths", []):
            if "latest" in path.lower():
                errors.append(f"Claim {cid}: mutable 'latest' in evidence_path: {path}")
        ep = claim.get("evidence_path", "")
        if ep and "latest" in ep.lower():
            errors.append(f"Claim {cid}: mutable 'latest' in evidence_path: {ep}")
        for rid in claim.get("pinned_run_ids", []):
            if "latest" in rid.lower():
                errors.append(f"Claim {cid}: mutable 'latest' in pinned_run_id: {rid}")

    return errors


def check_release_traceability() -> list:
    """Verify cross-repo release traceability artifact is present and consistent."""
    errors = []

    release_path = EVIDENCE_DIR / "release_traceability.json"
    if not release_path.exists():
        errors.append("Missing release traceability artifact: publication/evidence/release_traceability.json")
        return errors

    release = load_json(release_path)

    # Verify measured revision matches other evidence
    manifest = load_json(EVIDENCE_DIR / "artifact_manifest.json")
    if release.get("measured_revision") != manifest.get("measured_revision"):
        errors.append(
            f"Release traceability measured_revision ({release.get('measured_revision')}) "
            f"does not match artifact_manifest ({manifest.get('measured_revision')})"
        )

    # Verify bidirectional links are present
    bidir = release.get("bidirectional_links", {})
    w2r = bidir.get("website_to_repo", [])
    r2w = bidir.get("repo_to_website", [])

    if not w2r:
        errors.append("No website-to-repo links recorded in release traceability")
    if not r2w:
        errors.append("No repo-to-website links recorded in release traceability")

    # Verify repo-to-website links reference an actual website URL
    for link in r2w:
        target = link.get("target", "")
        if not target or "ragas.dev" not in target:
            errors.append(f"Repo-to-website link target is missing or invalid: {target}")

    # Verify website post commit is recorded
    website = release.get("website", {})
    if not website.get("website_post_commit"):
        errors.append("Website post commit not recorded in release traceability")

    # Verify clean-checkout reproducibility is documented
    repro = release.get("clean_checkout_reproducibility", {})
    if not repro.get("checkout_command"):
        errors.append("Clean-checkout reproducibility missing checkout command")
    if not repro.get("validation_commands"):
        errors.append("Clean-checkout reproducibility missing validation commands")

    return errors


def check_clean_checkout_provenance() -> list:
    """Verify the clean-checkout provenance artifact is present, consistent, and complete."""
    errors = []

    provenance_path = EVIDENCE_DIR / "clean_checkout_provenance.json"
    if not provenance_path.exists():
        errors.append("Missing clean-checkout provenance artifact: publication/evidence/clean_checkout_provenance.json")
        return errors

    provenance = load_json(provenance_path)

    # Verify measured revision matches other evidence
    manifest = load_json(EVIDENCE_DIR / "artifact_manifest.json")
    if provenance.get("measured_revision") != manifest.get("measured_revision"):
        errors.append(
            f"Provenance measured_revision ({provenance.get('measured_revision')}) "
            f"does not match artifact_manifest ({manifest.get('measured_revision')})"
        )

    # Verify required fields
    if not provenance.get("checkout_command"):
        errors.append("Provenance artifact missing checkout_command")
    if not provenance.get("manifest_hashes"):
        errors.append("Provenance artifact missing manifest_hashes")
    if not provenance.get("validation_commands"):
        errors.append("Provenance artifact missing validation_commands")
    if not provenance.get("produced_run_ids"):
        errors.append("Provenance artifact missing produced_run_ids")
    if not provenance.get("clone_url"):
        errors.append("Provenance artifact missing clone_url")

    # Verify manifest hashes match benchmark summary
    benchmark = load_json(EVIDENCE_DIR / "benchmark_summary.json")
    bench_hashes = benchmark.get("manifest_hashes", {})
    prov_hashes = provenance.get("manifest_hashes", {})
    for key in ["scenarios", "comparators", "policy"]:
        if bench_hashes.get(key) and prov_hashes.get(key):
            if bench_hashes[key] != prov_hashes[key]:
                errors.append(
                    f"Provenance manifest hash for '{key}' does not match benchmark summary"
                )

    # Verify produced run IDs match benchmark pinned IDs
    bench_ids = set(benchmark.get("pinned_run_ids", []))
    prov_ids = set(provenance.get("produced_run_ids", []))
    if bench_ids and prov_ids and bench_ids != prov_ids:
        errors.append(
            f"Provenance produced_run_ids {sorted(prov_ids)} do not match "
            f"benchmark pinned_run_ids {sorted(bench_ids)}"
        )

    # Verify script checksums are present (proves the scripts are committed)
    if not provenance.get("script_checksums"):
        errors.append("Provenance artifact missing script_checksums")

    return errors


def check_bidirectional_repo_links() -> list:
    """Verify that README and REWRITE_STUDY link to the published article."""
    errors = []

    readme_path = REPO_ROOT / "README.md"
    study_path = REPO_ROOT / "publication" / "REWRITE_STUDY.md"

    article_url = "https://ragas.dev/blog/rewriting-xxhash-in-rust"

    if readme_path.exists():
        readme_text = readme_path.read_text()
        if article_url not in readme_text:
            errors.append(f"README.md does not link to published article: {article_url}")
    else:
        errors.append("README.md not found")

    if study_path.exists():
        study_text = study_path.read_text()
        if article_url not in study_text:
            errors.append(f"REWRITE_STUDY.md does not link to published article: {article_url}")
    else:
        errors.append("publication/REWRITE_STUDY.md not found")

    return errors


def check_scenario_ids() -> list:
    """Verify recorded scenario IDs match the scenarios manifest."""
    errors = []

    benchmark = load_json(EVIDENCE_DIR / "benchmark_summary.json")
    recorded_ids = set(benchmark.get("scenario_ids", []))

    scenarios = load_json(BENCHMARKS_DIR / "scenarios.json")
    declared_ids = set(s["id"] for s in scenarios.get("scenarios", []))

    # The benchmark may run a subset (smoke), so recorded IDs should
    # be a subset of declared IDs
    extra = recorded_ids - declared_ids
    if extra:
        errors.append(f"Scenario IDs in evidence not declared in manifest: {extra}")

    if not recorded_ids:
        errors.append("No scenario IDs recorded in benchmark evidence")

    return errors


def main():
    print("=== Publication Traceability Check ===\n")

    all_errors = []

    checks = [
        ("Evidence files exist", check_evidence_files_exist),
        ("Revision lineage consistency", check_revision_lineage),
        ("Pinned benchmark runs", check_pinned_benchmark_runs),
        ("Parity evidence", check_parity_evidence),
        ("Manifest file integrity", check_manifest_file_integrity),
        ("No mutable latest references", check_no_latest_references),
        ("Scenario ID coverage", check_scenario_ids),
        ("Release traceability", check_release_traceability),
        ("Clean-checkout provenance", check_clean_checkout_provenance),
        ("Bidirectional repo/article links", check_bidirectional_repo_links),
    ]

    for name, check_fn in checks:
        try:
            errors = check_fn()
        except FileNotFoundError as e:
            errors = [f"File not found: {e}"]
        except json.JSONDecodeError as e:
            errors = [f"JSON parse error: {e}"]

        if errors:
            print(f"  FAIL: {name}")
            for e in errors:
                print(f"    - {e}")
            all_errors.extend(errors)
        else:
            print(f"  PASS: {name}")

    print()
    if all_errors:
        print(f"FAIL: {len(all_errors)} traceability error(s)")
        return 1
    else:
        print("OK: All traceability checks passed")
        return 0


if __name__ == "__main__":
    sys.exit(main() or 0)
