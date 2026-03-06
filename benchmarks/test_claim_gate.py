#!/usr/bin/env python3
"""
Unit tests for claim_gate.py run-set hardening.

Proves that:
1. check_run_set_consistency fails when runs in the compatible set have
   divergent revisions or required manifest/policy hashes.
2. check_minimum_run_set counts only the compatible run set (same revision +
   manifest/policy hashes) instead of heterogeneous global runs.
3. Regression coverage includes explicit singleton assertions for revision
   and required-hash sets in claim-ready consistency tests.
"""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

# Ensure we can import the claim_gate module
sys.path.insert(0, str(Path(__file__).resolve().parent))
import claim_gate  # noqa: E402


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_manifest(
    run_id: str = "run-test-001",
    revision: str = "abc123",
    scenarios_hash: str = "s_hash_a",
    comparators_hash: str = "c_hash_a",
    policy_hash: str = "p_hash_a",
    policy_version: str = "1.0.0",
    claim_ready: bool = True,
    all_correct: bool = True,
) -> dict:
    """Build a synthetic run manifest."""
    return {
        "run_id": run_id,
        "run_type": "smoke",
        "timestamp_utc": "2026-03-06T21:00:00+00:00",
        "status": "complete" if claim_ready else "partial",
        "claim_ready": claim_ready,
        "manifest_hashes": {
            "scenarios": scenarios_hash,
            "comparators": comparators_hash,
            "policy": policy_hash,
        },
        "policy_version": policy_version,
        "policy_hash": policy_hash,
        "environment": {
            "repo_revision": revision,
        },
        "correctness_gate": {
            "all_passed": all_correct,
            "results": [
                {
                    "scenario_id": "test-scenario",
                    "passed": all_correct,
                    "reason": "oracles agree" if all_correct else "disagreed",
                    "oracle_digests": {
                        "c_xxhsum": "abcd1234",
                        "rust_xxhash_rs": "abcd1234",
                    },
                }
            ],
        },
        "completeness": {
            "complete": True,
            "missing_entries": [],
        },
    }


def _make_index_entry(
    run_id: str,
    timestamp: str = "2026-03-06T21:00:00+00:00",
    claim_ready: bool = True,
    revision: str = "abc123",
    manifest_hashes: dict | None = None,
) -> dict:
    """Build a synthetic run index entry with extended metadata."""
    entry = {
        "run_id": run_id,
        "timestamp_utc": timestamp,
        "run_type": "smoke",
        "status": "complete" if claim_ready else "partial",
        "claim_ready": claim_ready,
        "scenario_count": 4,
    }
    if revision:
        entry["revision"] = revision
    if manifest_hashes:
        entry["manifest_hashes"] = manifest_hashes
    return entry


STANDARD_HASHES = {
    "scenarios": "s_hash_a",
    "comparators": "c_hash_a",
    "policy": "p_hash_a",
}

MULTI_RUN_POLICY = {
    "policy_version": "1.0.0",
    "claim_readiness": {
        "require_correctness_gate": True,
        "require_full_matrix": True,
        "require_artifact_checksums": True,
        "require_matching_revision": True,
        "require_matching_manifests": True,
        "minimum_runs": 3,
    },
}


class _TempRunsDir:
    """Context manager that sets up a temporary runs directory with index.json
    and manifest.json files for each run, and monkeypatches claim_gate.RUNS_DIR."""

    def __init__(self, runs: list[dict], manifests: dict[str, dict] | None = None):
        """
        Args:
            runs: list of index entries for index.json
            manifests: {run_id: manifest_dict} for any runs that need manifest.json
        """
        self.runs = runs
        self.manifests = manifests or {}
        self._tmpdir = None
        self._original_runs_dir = None

    def __enter__(self):
        self._tmpdir = tempfile.mkdtemp(prefix="claim_gate_test_")
        runs_path = Path(self._tmpdir)

        # Write index
        index = {"runs": self.runs}
        with open(runs_path / "index.json", "w") as f:
            json.dump(index, f, indent=2)

        # Write manifest files
        for run_id, manifest in self.manifests.items():
            run_dir = runs_path / run_id
            run_dir.mkdir(parents=True, exist_ok=True)
            with open(run_dir / "manifest.json", "w") as f:
                json.dump(manifest, f, indent=2)
            # Also write a dummy checksums.json
            with open(run_dir / "checksums.json", "w") as f:
                json.dump({"manifest.json": "dummy"}, f)

        # Monkeypatch
        self._original_runs_dir = claim_gate.RUNS_DIR
        claim_gate.RUNS_DIR = runs_path
        return runs_path

    def __exit__(self, *args):
        claim_gate.RUNS_DIR = self._original_runs_dir
        import shutil
        shutil.rmtree(self._tmpdir, ignore_errors=True)


# ---------------------------------------------------------------------------
# Tests: check_run_set_consistency (cross-run validation)
# ---------------------------------------------------------------------------

class TestRunSetConsistencyCrossRun(unittest.TestCase):
    """check_run_set_consistency must enforce that ALL runs in the compatible
    set share the same revision and manifest/policy hashes."""

    def test_consistent_runs_pass(self):
        """Three runs with identical revision and hashes pass consistency."""
        manifest = _make_manifest(revision="aaa111", **{
            k: v for k, v in [
                ("scenarios_hash", "s1"), ("comparators_hash", "c1"), ("policy_hash", "p1")
            ]
        })
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", {"scenarios": "s1", "comparators": "c1", "policy": "p1"}),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111", {"scenarios": "s1", "comparators": "c1", "policy": "p1"}),
            _make_index_entry("r3", "2026-03-06T03:00:00Z", True, "aaa111", {"scenarios": "s1", "comparators": "c1", "policy": "p1"}),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s1", "c1", "p1"),
            "r3": _make_manifest("r3", "aaa111", "s1", "c1", "p1"),
        }
        with _TempRunsDir(runs, manifests):
            passed, issues = claim_gate.check_run_set_consistency(manifest, MULTI_RUN_POLICY)
            self.assertTrue(passed, f"Expected pass, got issues: {issues}")

    def test_divergent_revision_fails(self):
        """Runs with different revisions must fail consistency."""
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", STANDARD_HASHES),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "bbb222", STANDARD_HASHES),
            _make_index_entry("r3", "2026-03-06T03:00:00Z", True, "aaa111", STANDARD_HASHES),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111"),
            "r2": _make_manifest("r2", "bbb222"),
            "r3": _make_manifest("r3", "aaa111"),
        }
        with _TempRunsDir(runs, manifests):
            passed, issues = claim_gate.check_run_set_consistency(manifest, MULTI_RUN_POLICY)
            self.assertFalse(passed, "Must fail when revisions diverge")
            joined = " ".join(issues)
            self.assertIn("revision", joined.lower())

    def test_divergent_policy_hash_excluded_from_compatible_set(self):
        """Runs with different policy hashes are excluded from the compatible
        set, so the minimum-run threshold fails."""
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111",
                              {"scenarios": "s1", "comparators": "c1", "policy": "p1"}),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111",
                              {"scenarios": "s1", "comparators": "c1", "policy": "p_different"}),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s1", "c1", "p_different"),
        }
        with _TempRunsDir(runs, manifests):
            compatible = claim_gate.get_compatible_run_set(manifest)
            # Only r1 matches; r2 has a different policy hash
            self.assertEqual(len(compatible), 1)
            self.assertEqual(compatible[0]["run_id"], "r1")
            # With only 1 compatible run, minimum_runs=3 fails
            passed, issues = claim_gate.check_minimum_run_set(manifest, MULTI_RUN_POLICY)
            self.assertFalse(passed, "Must fail: only 1 compatible run with matching policy hash")

    def test_divergent_scenario_hash_excluded_from_compatible_set(self):
        """Runs with different scenario hashes are excluded from the compatible
        set, so the minimum-run threshold fails."""
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111",
                              {"scenarios": "s1", "comparators": "c1", "policy": "p1"}),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111",
                              {"scenarios": "s_different", "comparators": "c1", "policy": "p1"}),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s_different", "c1", "p1"),
        }
        with _TempRunsDir(runs, manifests):
            compatible = claim_gate.get_compatible_run_set(manifest)
            # Only r1 matches
            self.assertEqual(len(compatible), 1)
            self.assertEqual(compatible[0]["run_id"], "r1")
            # With only 1 compatible run, minimum_runs=3 fails
            passed, issues = claim_gate.check_minimum_run_set(manifest, MULTI_RUN_POLICY)
            self.assertFalse(passed, "Must fail: only 1 compatible run with matching scenario hash")


    def test_singleton_revision_set_asserted(self):
        """The compatible run set must have exactly one unique revision."""
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111",
                              {"scenarios": "s1", "comparators": "c1", "policy": "p1"}),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111",
                              {"scenarios": "s1", "comparators": "c1", "policy": "p1"}),
            _make_index_entry("r3", "2026-03-06T03:00:00Z", True, "aaa111",
                              {"scenarios": "s1", "comparators": "c1", "policy": "p1"}),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s1", "c1", "p1"),
            "r3": _make_manifest("r3", "aaa111", "s1", "c1", "p1"),
        }
        with _TempRunsDir(runs, manifests):
            passed, issues = claim_gate.check_run_set_consistency(manifest, MULTI_RUN_POLICY)
            self.assertTrue(passed, f"Expected pass: {issues}")
            # The revision set across compatible runs must be exactly {aaa111}
            # This is implicitly verified by the pass, but let's also verify
            # by checking the compatible run set loader directly
            compatible = claim_gate.get_compatible_run_set(manifest)
            revisions = {r.get("revision") for r in compatible}
            self.assertEqual(len(revisions), 1, f"Expected singleton revision set, got {revisions}")
            self.assertIn("aaa111", revisions)

    def test_singleton_hash_sets_asserted(self):
        """The compatible run set must have exactly one unique set of hashes."""
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        hashes = {"scenarios": "s1", "comparators": "c1", "policy": "p1"}
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", hashes),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111", hashes),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s1", "c1", "p1"),
        }
        with _TempRunsDir(runs, manifests):
            compatible = claim_gate.get_compatible_run_set(manifest)
            hash_sets = {
                frozenset(r.get("manifest_hashes", {}).items())
                for r in compatible
            }
            self.assertEqual(len(hash_sets), 1, f"Expected singleton hash set, got {hash_sets}")


# ---------------------------------------------------------------------------
# Tests: check_minimum_run_set (compatible set counting)
# ---------------------------------------------------------------------------

class TestMinimumRunSetCompatibleCounting(unittest.TestCase):
    """check_minimum_run_set must count only the compatible run set for the
    target claim instead of heterogeneous global runs."""

    def test_enough_compatible_runs_pass(self):
        """3 compatible runs satisfy minimum_runs=3."""
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        hashes = {"scenarios": "s1", "comparators": "c1", "policy": "p1"}
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", hashes),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111", hashes),
            _make_index_entry("r3", "2026-03-06T03:00:00Z", True, "aaa111", hashes),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s1", "c1", "p1"),
            "r3": _make_manifest("r3", "aaa111", "s1", "c1", "p1"),
        }
        with _TempRunsDir(runs, manifests):
            passed, issues = claim_gate.check_minimum_run_set(manifest, MULTI_RUN_POLICY)
            self.assertTrue(passed, f"Expected pass: {issues}")

    def test_heterogeneous_runs_not_counted(self):
        """5 total claim-ready runs, but only 2 share the target revision.
        minimum_runs=3 should fail because the compatible set has only 2."""
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        hashes_a = {"scenarios": "s1", "comparators": "c1", "policy": "p1"}
        hashes_b = {"scenarios": "s_other", "comparators": "c1", "policy": "p1"}
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", hashes_a),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111", hashes_a),
            _make_index_entry("r3", "2026-03-06T03:00:00Z", True, "bbb222", hashes_b),
            _make_index_entry("r4", "2026-03-06T04:00:00Z", True, "bbb222", hashes_b),
            _make_index_entry("r5", "2026-03-06T05:00:00Z", True, "bbb222", hashes_b),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s1", "c1", "p1"),
            "r3": _make_manifest("r3", "bbb222", "s_other", "c1", "p1"),
            "r4": _make_manifest("r4", "bbb222", "s_other", "c1", "p1"),
            "r5": _make_manifest("r5", "bbb222", "s_other", "c1", "p1"),
        }
        with _TempRunsDir(runs, manifests):
            passed, issues = claim_gate.check_minimum_run_set(manifest, MULTI_RUN_POLICY)
            self.assertFalse(passed, "Must fail: only 2 compatible runs, need 3")
            joined = " ".join(issues)
            self.assertIn("2", joined)  # should report 2 compatible runs

    def test_exactly_at_threshold_passes(self):
        """Exactly minimum_runs compatible runs passes."""
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        hashes = {"scenarios": "s1", "comparators": "c1", "policy": "p1"}
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", hashes),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111", hashes),
            _make_index_entry("r3", "2026-03-06T03:00:00Z", True, "aaa111", hashes),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s1", "c1", "p1"),
            "r3": _make_manifest("r3", "aaa111", "s1", "c1", "p1"),
        }
        with _TempRunsDir(runs, manifests):
            passed, issues = claim_gate.check_minimum_run_set(manifest, MULTI_RUN_POLICY)
            self.assertTrue(passed, f"Expected pass: {issues}")

    def test_below_threshold_fails(self):
        """2 compatible runs when minimum_runs=3 fails."""
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        hashes = {"scenarios": "s1", "comparators": "c1", "policy": "p1"}
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", hashes),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111", hashes),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s1", "c1", "p1"),
        }
        with _TempRunsDir(runs, manifests):
            passed, issues = claim_gate.check_minimum_run_set(manifest, MULTI_RUN_POLICY)
            self.assertFalse(passed, "Must fail: 2 compatible runs < 3 required")

    def test_non_claim_ready_runs_excluded(self):
        """Partial/non-claim-ready runs with matching revision are not counted."""
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        hashes = {"scenarios": "s1", "comparators": "c1", "policy": "p1"}
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", hashes),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111", hashes),
            _make_index_entry("r3", "2026-03-06T03:00:00Z", False, "aaa111", hashes),  # partial
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s1", "c1", "p1"),
            "r3": _make_manifest("r3", "aaa111", "s1", "c1", "p1", claim_ready=False),
        }
        with _TempRunsDir(runs, manifests):
            passed, issues = claim_gate.check_minimum_run_set(manifest, MULTI_RUN_POLICY)
            self.assertFalse(passed, "Must fail: only 2 claim-ready compatible runs")


# ---------------------------------------------------------------------------
# Tests: policy minimum_runs validation
# ---------------------------------------------------------------------------

class TestPolicyMinimumRuns(unittest.TestCase):
    """Policy must require minimum_runs > 1 for claim readiness."""

    def test_policy_minimum_runs_above_one(self):
        """The production policy must require more than 1 run."""
        policy = claim_gate.load_json(claim_gate.POLICY_PATH)
        minimum = policy.get("claim_readiness", {}).get("minimum_runs", 1)
        self.assertGreater(
            minimum, 1,
            f"Policy minimum_runs must be > 1 for multi-run hardening, got {minimum}"
        )


# ---------------------------------------------------------------------------
# Tests: get_compatible_run_set
# ---------------------------------------------------------------------------

class TestGetCompatibleRunSet(unittest.TestCase):
    """get_compatible_run_set must return only runs matching the target
    run's revision and manifest/policy hashes."""

    def test_filters_by_revision(self):
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        hashes = {"scenarios": "s1", "comparators": "c1", "policy": "p1"}
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", hashes),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "bbb222", hashes),
            _make_index_entry("r3", "2026-03-06T03:00:00Z", True, "aaa111", hashes),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "bbb222", "s1", "c1", "p1"),
            "r3": _make_manifest("r3", "aaa111", "s1", "c1", "p1"),
        }
        with _TempRunsDir(runs, manifests):
            compatible = claim_gate.get_compatible_run_set(manifest)
            ids = {r["run_id"] for r in compatible}
            self.assertEqual(ids, {"r1", "r3"})

    def test_filters_by_manifest_hashes(self):
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111",
                              {"scenarios": "s1", "comparators": "c1", "policy": "p1"}),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", True, "aaa111",
                              {"scenarios": "s_different", "comparators": "c1", "policy": "p1"}),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s_different", "c1", "p1"),
        }
        with _TempRunsDir(runs, manifests):
            compatible = claim_gate.get_compatible_run_set(manifest)
            ids = {r["run_id"] for r in compatible}
            self.assertEqual(ids, {"r1"})

    def test_excludes_non_claim_ready(self):
        manifest = _make_manifest(revision="aaa111", scenarios_hash="s1",
                                  comparators_hash="c1", policy_hash="p1")
        hashes = {"scenarios": "s1", "comparators": "c1", "policy": "p1"}
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", hashes),
            _make_index_entry("r2", "2026-03-06T02:00:00Z", False, "aaa111", hashes),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111", "s1", "c1", "p1"),
            "r2": _make_manifest("r2", "aaa111", "s1", "c1", "p1", claim_ready=False),
        }
        with _TempRunsDir(runs, manifests):
            compatible = claim_gate.get_compatible_run_set(manifest)
            ids = {r["run_id"] for r in compatible}
            self.assertEqual(ids, {"r1"})

    def test_empty_when_no_matching_runs(self):
        manifest = _make_manifest(revision="zzz999", scenarios_hash="s_unique",
                                  comparators_hash="c1", policy_hash="p1")
        runs = [
            _make_index_entry("r1", "2026-03-06T01:00:00Z", True, "aaa111", STANDARD_HASHES),
        ]
        manifests = {
            "r1": _make_manifest("r1", "aaa111"),
        }
        with _TempRunsDir(runs, manifests):
            compatible = claim_gate.get_compatible_run_set(manifest)
            self.assertEqual(len(compatible), 0)


if __name__ == "__main__":
    unittest.main()
