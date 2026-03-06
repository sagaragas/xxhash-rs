#!/usr/bin/env python3
"""
Reconciliation script for xxhash-rs benchmark runs.

Verifies that published summary numbers (e.g. median timings) reconcile exactly
to the retained raw samples, with declared units and statistical methods.

Usage:
    python3 benchmarks/reconcile.py --run latest
    python3 benchmarks/reconcile.py --run <run-id>
"""

import argparse
import json
import sys
from pathlib import Path


HARNESS_DIR = Path(__file__).resolve().parent
RUNS_DIR = HARNESS_DIR / "runs"
POLICY_PATH = HARNESS_DIR / "policy.json"


def set_runs_dir(path: Path) -> None:
    """Override the runs directory (used for isolated testing)."""
    global RUNS_DIR
    RUNS_DIR = path


def load_json(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


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


def recompute_median(samples: list[dict]) -> int | None:
    """Recompute median elapsed_ns from raw measured samples."""
    elapsed_values = [
        s["elapsed_ns"] for s in samples
        if s.get("success") is True and "elapsed_ns" in s
    ]
    if not elapsed_values:
        return None
    elapsed_values.sort()
    return elapsed_values[len(elapsed_values) // 2]


def reconcile_run(run_arg: str) -> int:
    """Reconcile a benchmark run's summaries against raw samples."""
    print(f"=== xxhash-rs benchmark reconciliation ===")
    print(f"Target: {run_arg}")

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
    samples_dir = run_dir / "samples"

    print(f"Run ID: {run_id}")
    print(f"Run type: {manifest.get('run_type', 'unknown')}")
    print(f"Scenarios: {manifest.get('scenario_count', '?')}")

    # Verify statistical method is declared
    stat_method = manifest.get("statistical_method", {})
    summary_stat = stat_method.get("summary_statistic", "unknown")
    warmup_policy = stat_method.get("warmup_policy", "unknown")
    retain_raw = stat_method.get("retain_raw_samples", False)

    print(f"\nDeclared statistical method:")
    print(f"  Summary statistic: {summary_stat}")
    print(f"  Warmup policy: {warmup_policy}")
    print(f"  Retain raw samples: {retain_raw}")

    if not retain_raw:
        print(f"\nERROR: Raw sample retention is disabled; cannot reconcile.")
        return 1

    if summary_stat != "median":
        print(f"\nWARNING: Expected 'median' summary statistic, got '{summary_stat}'")

    # Check samples directory exists
    if not samples_dir.exists():
        print(f"\nERROR: Samples directory not found: {samples_dir}")
        return 1

    # Reconcile each scenario
    all_passed = True
    scenario_count = 0
    comparator_checks = 0
    mismatches = []

    print(f"\n--- Reconciliation Results ---")
    print(f"  Unit: nanoseconds (elapsed_ns)")
    print(f"  Statistic: {summary_stat}")
    print()

    sample_files = sorted(samples_dir.glob("*.json"))
    for sample_path in sample_files:
        sample = load_json(sample_path)
        scenario_id = sample.get("scenario_id", sample_path.stem)
        algorithm = sample.get("algorithm", "?")
        payload_bytes = sample.get("payload_bytes", "?")
        scenario_count += 1

        print(f"  Scenario: {scenario_id} ({algorithm}, {payload_bytes} bytes)")

        comp_results = sample.get("comparator_results", {})
        for comp_id, cr in comp_results.items():
            if cr.get("status") != "success":
                print(f"    {comp_id}: SKIPPED (status={cr.get('status', '?')})")
                continue

            comparator_checks += 1
            declared_median = cr.get("median_ns")
            measured_samples = cr.get("measured_samples", [])

            # Recompute median from raw samples
            recomputed = recompute_median(measured_samples)

            if declared_median is None:
                print(f"    {comp_id}: WARNING - no declared median_ns")
                mismatches.append(f"{scenario_id}/{comp_id}: no declared median")
                all_passed = False
                continue

            if recomputed is None:
                print(f"    {comp_id}: WARNING - no successful samples to recompute")
                mismatches.append(f"{scenario_id}/{comp_id}: no successful samples")
                all_passed = False
                continue

            declared_int = int(declared_median)
            match = declared_int == recomputed
            symbol = "✓" if match else "✗"
            print(
                f"    {symbol} {comp_id}: declared={declared_int} ns, "
                f"recomputed={recomputed} ns, "
                f"samples={len(measured_samples)}"
            )

            if not match:
                mismatches.append(
                    f"{scenario_id}/{comp_id}: declared={declared_int} != recomputed={recomputed}"
                )
                all_passed = False

    # Summary
    print(f"\n--- Summary ---")
    print(f"  Scenarios reconciled: {scenario_count}")
    print(f"  Comparator checks: {comparator_checks}")
    print(f"  Reconciliation: {'PASS' if all_passed else 'FAIL'}")

    if mismatches:
        print(f"  Mismatches:")
        for m in mismatches:
            print(f"    - {m}")

    return 0 if all_passed else 1


def main():
    parser = argparse.ArgumentParser(
        description="xxhash-rs benchmark reconciliation"
    )
    parser.add_argument(
        "--run",
        required=True,
        help="Run ID or 'latest' to reconcile the most recent claim-ready run",
    )
    parser.add_argument(
        "--run-dir",
        default=None,
        help="Override runs directory (for isolated/deterministic testing)",
    )
    args = parser.parse_args()

    if args.run_dir:
        set_runs_dir(Path(args.run_dir))

    return reconcile_run(args.run)


if __name__ == "__main__":
    sys.exit(main())
