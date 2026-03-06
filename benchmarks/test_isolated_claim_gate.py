#!/usr/bin/env python3
"""
Integration tests for isolated benchmark claim-gate validation.

Exercises the full claim-gate and reconcile flows against deterministic
seeded run sets, proving that:

1. claim-gate/reconcile tests do NOT depend on whatever happens to be
   present in benchmarks/runs/ (ambient mutable state).
2. The minimum-runs policy can be exercised from an isolated deterministic
   compatible run set.
3. Benchmark-harness validation remains green without ad-hoc manual
   smoke-run seeding between validator passes.

All tests create their own isolated runs directory via seed_run_set and
monkeypatch claim_gate.RUNS_DIR / reconcile.RUNS_DIR so zero ambient
state is accessed.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

# Ensure module imports work
sys.path.insert(0, str(Path(__file__).resolve().parent))
import claim_gate  # noqa: E402
import reconcile  # noqa: E402
from seed_run_set import (  # noqa: E402
    DEFAULT_COMPARATORS_HASH,
    DEFAULT_POLICY_HASH,
    DEFAULT_POLICY_VERSION,
    DEFAULT_REVISION,
    DEFAULT_SCENARIOS_HASH,
    seed_compatible_run_set,
    seed_policy,
)


class _IsolatedRunSetDir:
    """Context manager that creates a fully isolated runs directory with
    seeded deterministic runs, monkeypatches both claim_gate and reconcile
    module RUNS_DIR and POLICY_PATH, and cleans up afterwards.

    Provides a self-contained environment that never touches ambient state.
    """

    def __init__(
        self,
        num_runs: int = 3,
        minimum_runs: int = 3,
        revision: str = DEFAULT_REVISION,
        scenarios_hash: str = DEFAULT_SCENARIOS_HASH,
        comparators_hash: str = DEFAULT_COMPARATORS_HASH,
        policy_hash: str = DEFAULT_POLICY_HASH,
        policy_version: str = DEFAULT_POLICY_VERSION,
        oracle_digest: str = "abcdef0123456789",
    ):
        self.num_runs = num_runs
        self.minimum_runs = minimum_runs
        self.revision = revision
        self.scenarios_hash = scenarios_hash
        self.comparators_hash = comparators_hash
        self.policy_hash = policy_hash
        self.policy_version = policy_version
        self.oracle_digest = oracle_digest
        self._tmpdir = None
        self._orig_claim_runs = None
        self._orig_claim_policy = None
        self._orig_reconcile_runs = None

    def __enter__(self):
        self._tmpdir = tempfile.mkdtemp(prefix="isolated_claim_gate_")
        base = Path(self._tmpdir)
        runs_dir = base / "runs"

        seed_compatible_run_set(
            runs_dir,
            num_runs=self.num_runs,
            revision=self.revision,
            scenarios_hash=self.scenarios_hash,
            comparators_hash=self.comparators_hash,
            policy_hash=self.policy_hash,
            policy_version=self.policy_version,
            oracle_digest=self.oracle_digest,
        )

        policy_path = seed_policy(
            base,
            policy_version=self.policy_version,
            minimum_runs=self.minimum_runs,
        )

        # Monkeypatch claim_gate
        self._orig_claim_runs = claim_gate.RUNS_DIR
        self._orig_claim_policy = claim_gate.POLICY_PATH
        claim_gate.RUNS_DIR = runs_dir
        claim_gate.POLICY_PATH = policy_path

        # Monkeypatch reconcile
        self._orig_reconcile_runs = reconcile.RUNS_DIR
        reconcile.RUNS_DIR = runs_dir

        return base

    def __exit__(self, *args):
        claim_gate.RUNS_DIR = self._orig_claim_runs
        claim_gate.POLICY_PATH = self._orig_claim_policy
        reconcile.RUNS_DIR = self._orig_reconcile_runs
        shutil.rmtree(self._tmpdir, ignore_errors=True)


# ---------------------------------------------------------------------------
# Tests: Claim gate with isolated deterministic run sets
# ---------------------------------------------------------------------------

class TestIsolatedClaimGateFullFlow(unittest.TestCase):
    """End-to-end claim gate checks against isolated seeded run sets."""

    def test_claim_gate_passes_with_enough_compatible_runs(self):
        """3 seeded runs with minimum_runs=3 policy passes all gates."""
        with _IsolatedRunSetDir(num_runs=3, minimum_runs=3):
            run_id = claim_gate.resolve_run_id("latest")
            self.assertIsNotNone(run_id, "Should resolve a latest run")

            run_dir = claim_gate.RUNS_DIR / run_id
            manifest = claim_gate.load_json(run_dir / "manifest.json")
            policy = claim_gate.load_json(claim_gate.POLICY_PATH)

            # All five gates
            cg_pass, cg_issues = claim_gate.check_correctness_gate(manifest)
            self.assertTrue(cg_pass, f"Correctness gate: {cg_issues}")

            mc_pass, mc_issues = claim_gate.check_matrix_completeness(manifest)
            self.assertTrue(mc_pass, f"Matrix completeness: {mc_issues}")

            ac_pass, ac_issues = claim_gate.check_artifact_checksums(run_dir)
            self.assertTrue(ac_pass, f"Artifact checksums: {ac_issues}")

            rs_pass, rs_issues = claim_gate.check_run_set_consistency(manifest, policy)
            self.assertTrue(rs_pass, f"Run-set consistency: {rs_issues}")

            mr_pass, mr_issues = claim_gate.check_minimum_run_set(manifest, policy)
            self.assertTrue(mr_pass, f"Minimum run set: {mr_issues}")

    def test_claim_gate_fails_below_minimum_runs(self):
        """2 seeded runs with minimum_runs=3 policy fails the minimum-runs check."""
        with _IsolatedRunSetDir(num_runs=2, minimum_runs=3):
            run_id = claim_gate.resolve_run_id("latest")
            self.assertIsNotNone(run_id)

            run_dir = claim_gate.RUNS_DIR / run_id
            manifest = claim_gate.load_json(run_dir / "manifest.json")
            policy = claim_gate.load_json(claim_gate.POLICY_PATH)

            mr_pass, mr_issues = claim_gate.check_minimum_run_set(manifest, policy)
            self.assertFalse(mr_pass, "Should fail: only 2 runs, need 3")
            self.assertIn("2", " ".join(mr_issues))

    def test_claim_gate_uses_only_compatible_runs(self):
        """Seeded runs with different revision are excluded from compatible set."""
        with _IsolatedRunSetDir(num_runs=3, minimum_runs=3) as base:
            runs_dir = base / "runs"

            # Inject a run with a different revision
            alien_dir = runs_dir / "alien-run"
            alien_dir.mkdir(parents=True, exist_ok=True)
            alien_manifest = {
                "run_id": "alien-run",
                "run_type": "smoke",
                "timestamp_utc": "2026-01-15T13:00:00+00:00",
                "status": "complete",
                "claim_ready": True,
                "manifest_hashes": {
                    "scenarios": DEFAULT_SCENARIOS_HASH,
                    "comparators": DEFAULT_COMPARATORS_HASH,
                    "policy": DEFAULT_POLICY_HASH,
                },
                "environment": {"repo_revision": "different_revision_abc"},
                "correctness_gate": {"all_passed": True, "results": []},
                "completeness": {"complete": True, "missing_entries": []},
            }
            with open(alien_dir / "manifest.json", "w") as f:
                json.dump(alien_manifest, f, indent=2)

            # Update index with the alien run
            index = claim_gate.load_json(runs_dir / "index.json")
            index["runs"].append({
                "run_id": "alien-run",
                "timestamp_utc": "2026-01-15T13:00:00+00:00",
                "run_type": "smoke",
                "status": "complete",
                "claim_ready": True,
                "scenario_count": 4,
                "revision": "different_revision_abc",
                "manifest_hashes": {
                    "scenarios": DEFAULT_SCENARIOS_HASH,
                    "comparators": DEFAULT_COMPARATORS_HASH,
                    "policy": DEFAULT_POLICY_HASH,
                },
            })
            with open(runs_dir / "index.json", "w") as f:
                json.dump(index, f, indent=2)

            # The compatible set for the original runs should still be 3
            run_id = "seed-run-000"
            manifest = claim_gate.load_json(runs_dir / run_id / "manifest.json")
            compatible = claim_gate.get_compatible_run_set(manifest)
            compatible_ids = {r["run_id"] for r in compatible}

            self.assertNotIn("alien-run", compatible_ids)
            self.assertEqual(len(compatible), 3)

    def test_latest_resolves_to_most_recent_seeded_run(self):
        """'latest' resolves to the highest-timestamp seeded run."""
        with _IsolatedRunSetDir(num_runs=3):
            run_id = claim_gate.resolve_run_id("latest")
            self.assertIsNotNone(run_id)
            # The last seeded run should have the highest index
            self.assertEqual(run_id, "seed-run-002")

    def test_claim_gate_cli_exit_code_zero_with_full_set(self):
        """The claim_gate.py CLI exits 0 against a full isolated run set."""
        with _IsolatedRunSetDir(num_runs=3, minimum_runs=3) as base:
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve().parent / "claim_gate.py"),
                    "--run", "latest",
                    "--run-dir", str(base / "runs"),
                    "--policy", str(base / "policy.json"),
                ],
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(
                result.returncode, 0,
                f"Expected exit 0, got {result.returncode}\n"
                f"stdout: {result.stdout}\nstderr: {result.stderr}"
            )
            self.assertIn("Claim-ready: YES", result.stdout)

    def test_claim_gate_cli_exit_code_nonzero_below_threshold(self):
        """The claim_gate.py CLI exits 1 when below minimum-runs threshold."""
        with _IsolatedRunSetDir(num_runs=1, minimum_runs=3) as base:
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve().parent / "claim_gate.py"),
                    "--run", "latest",
                    "--run-dir", str(base / "runs"),
                    "--policy", str(base / "policy.json"),
                ],
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(
                result.returncode, 1,
                f"Expected exit 1, got {result.returncode}\n"
                f"stdout: {result.stdout}\nstderr: {result.stderr}"
            )
            self.assertIn("Claim-ready: NO", result.stdout)


# ---------------------------------------------------------------------------
# Tests: Reconcile with isolated deterministic run sets
# ---------------------------------------------------------------------------

class TestIsolatedReconcileFullFlow(unittest.TestCase):
    """End-to-end reconciliation against isolated seeded run sets."""

    def test_reconcile_passes_with_seeded_runs(self):
        """Reconciliation of seeded runs passes: declared medians match raw samples."""
        with _IsolatedRunSetDir(num_runs=3):
            run_id = reconcile.resolve_run_id("latest")
            self.assertIsNotNone(run_id)

            # Run the reconciliation directly
            exit_code = reconcile.reconcile_run(run_id)
            self.assertEqual(exit_code, 0, "Reconciliation should pass for seeded runs")

    def test_reconcile_cli_exit_code_zero(self):
        """The reconcile.py CLI exits 0 against a seeded isolated run set."""
        with _IsolatedRunSetDir(num_runs=3) as base:
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve().parent / "reconcile.py"),
                    "--run", "latest",
                    "--run-dir", str(base / "runs"),
                ],
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(
                result.returncode, 0,
                f"Expected exit 0, got {result.returncode}\n"
                f"stdout: {result.stdout}\nstderr: {result.stderr}"
            )
            self.assertIn("Reconciliation: PASS", result.stdout)


# ---------------------------------------------------------------------------
# Tests: Isolation verification (proves no ambient state dependency)
# ---------------------------------------------------------------------------

class TestNoAmbientStateDependency(unittest.TestCase):
    """Proves that isolated tests never access the real benchmarks/runs/ state."""

    def test_monkeypatched_runs_dir_is_not_ambient(self):
        """Inside _IsolatedRunSetDir, RUNS_DIR points to the temp dir,
        not the real benchmarks/runs/."""
        real_runs_dir = Path(__file__).resolve().parent / "runs"
        with _IsolatedRunSetDir(num_runs=3):
            self.assertNotEqual(
                claim_gate.RUNS_DIR, real_runs_dir,
                "claim_gate.RUNS_DIR must be overridden, not pointing at ambient state"
            )
            self.assertNotEqual(
                reconcile.RUNS_DIR, real_runs_dir,
                "reconcile.RUNS_DIR must be overridden, not pointing at ambient state"
            )

    def test_ambient_state_restored_after_context_exit(self):
        """After _IsolatedRunSetDir exits, the real RUNS_DIR is restored."""
        real_claim_runs = claim_gate.RUNS_DIR
        real_reconcile_runs = reconcile.RUNS_DIR

        with _IsolatedRunSetDir(num_runs=1):
            pass  # context exits

        self.assertEqual(claim_gate.RUNS_DIR, real_claim_runs)
        self.assertEqual(reconcile.RUNS_DIR, real_reconcile_runs)

    def test_compatible_run_set_is_deterministic(self):
        """Two invocations of seed_compatible_run_set with the same params
        produce runs with identical manifest metadata."""
        with _IsolatedRunSetDir(num_runs=3) as base1:
            index1 = claim_gate.load_json(base1 / "runs" / "index.json")

        # Create a second independent isolated set
        with _IsolatedRunSetDir(num_runs=3) as base2:
            index2 = claim_gate.load_json(base2 / "runs" / "index.json")

        # Revisions and hashes must be identical
        for i in range(3):
            self.assertEqual(
                index1["runs"][i]["revision"],
                index2["runs"][i]["revision"],
            )
            self.assertEqual(
                index1["runs"][i]["manifest_hashes"],
                index2["runs"][i]["manifest_hashes"],
            )

    def test_seeded_runs_have_correct_structure(self):
        """Each seeded run has manifest.json, samples/, and checksums.json."""
        with _IsolatedRunSetDir(num_runs=3) as base:
            runs_dir = base / "runs"
            for i in range(3):
                run_dir = runs_dir / f"seed-run-{i:03d}"
                self.assertTrue((run_dir / "manifest.json").exists())
                self.assertTrue((run_dir / "checksums.json").exists())
                self.assertTrue((run_dir / "samples").is_dir())

                # Samples should contain one file per scenario
                sample_files = list((run_dir / "samples").glob("*.json"))
                self.assertEqual(len(sample_files), 4)


# ---------------------------------------------------------------------------
# Tests: Minimum-runs policy from isolated run set
# ---------------------------------------------------------------------------

class TestMinimumRunsPolicyIsolated(unittest.TestCase):
    """Exercises the minimum-runs policy entirely from isolated seeded state."""

    def test_exactly_at_threshold(self):
        """minimum_runs=3 with exactly 3 compatible runs passes."""
        with _IsolatedRunSetDir(num_runs=3, minimum_runs=3):
            run_id = claim_gate.resolve_run_id("latest")
            manifest = claim_gate.load_json(
                claim_gate.RUNS_DIR / run_id / "manifest.json"
            )
            policy = claim_gate.load_json(claim_gate.POLICY_PATH)

            passed, issues = claim_gate.check_minimum_run_set(manifest, policy)
            self.assertTrue(passed, f"Should pass at threshold: {issues}")

    def test_above_threshold(self):
        """minimum_runs=3 with 5 compatible runs passes."""
        with _IsolatedRunSetDir(num_runs=5, minimum_runs=3):
            run_id = claim_gate.resolve_run_id("latest")
            manifest = claim_gate.load_json(
                claim_gate.RUNS_DIR / run_id / "manifest.json"
            )
            policy = claim_gate.load_json(claim_gate.POLICY_PATH)

            passed, issues = claim_gate.check_minimum_run_set(manifest, policy)
            self.assertTrue(passed, f"Should pass above threshold: {issues}")

    def test_below_threshold(self):
        """minimum_runs=3 with only 2 compatible runs fails."""
        with _IsolatedRunSetDir(num_runs=2, minimum_runs=3):
            run_id = claim_gate.resolve_run_id("latest")
            manifest = claim_gate.load_json(
                claim_gate.RUNS_DIR / run_id / "manifest.json"
            )
            policy = claim_gate.load_json(claim_gate.POLICY_PATH)

            passed, issues = claim_gate.check_minimum_run_set(manifest, policy)
            self.assertFalse(passed, "Should fail below threshold")
            self.assertIn("2", " ".join(issues))

    def test_single_run_fails(self):
        """minimum_runs=3 with only 1 run fails."""
        with _IsolatedRunSetDir(num_runs=1, minimum_runs=3):
            run_id = claim_gate.resolve_run_id("latest")
            manifest = claim_gate.load_json(
                claim_gate.RUNS_DIR / run_id / "manifest.json"
            )
            policy = claim_gate.load_json(claim_gate.POLICY_PATH)

            passed, issues = claim_gate.check_minimum_run_set(manifest, policy)
            self.assertFalse(passed, "Should fail with single run")

    def test_minimum_runs_equals_one(self):
        """minimum_runs=1 with 1 run passes."""
        with _IsolatedRunSetDir(num_runs=1, minimum_runs=1):
            run_id = claim_gate.resolve_run_id("latest")
            manifest = claim_gate.load_json(
                claim_gate.RUNS_DIR / run_id / "manifest.json"
            )
            policy = claim_gate.load_json(claim_gate.POLICY_PATH)

            passed, issues = claim_gate.check_minimum_run_set(manifest, policy)
            self.assertTrue(passed, f"Should pass with min=1: {issues}")


if __name__ == "__main__":
    unittest.main()
