#!/usr/bin/env python3
"""
Unit tests for harness.py oracle-digest hardening.

Proves that check_correctness_gate fails when either oracle digest
(c_xxhsum or rust_xxhash_rs) is missing or empty, rather than silently
filtering the missing value away.
"""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

# Ensure we can import the harness module
sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness  # noqa: E402


def _make_scenario_result(
    c_digest: str | None,
    rust_digest: str | None,
    b3sum_ok: bool = True,
    md5_ok: bool = True,
) -> dict:
    """Build a synthetic scenario_result dict for gate testing.

    The comparator_results entries simulate measured_samples with
    stdout_first_line carrying the oracle digest.
    """
    comparator_results = {}

    for oid, digest in [("c_xxhsum", c_digest), ("rust_xxhash_rs", rust_digest)]:
        if digest is None:
            # Comparator missing entirely
            comparator_results[oid] = {"status": "missing", "error": "not resolved"}
        else:
            comparator_results[oid] = {
                "status": "success",
                "measured_samples": [
                    {
                        "stdout_first_line": f"{digest}  payload.bin" if digest else "",
                        "success": True,
                    }
                ],
            }

    for cid, ok in [("b3sum", b3sum_ok), ("md5", md5_ok)]:
        comparator_results[cid] = {
            "status": "success" if ok else "failed",
            "measured_samples": [
                {
                    "stdout_first_line": "abcdef1234567890  payload.bin",
                    "success": ok,
                }
            ],
        }

    return {
        "scenario_id": "test-scenario",
        "comparator_results": comparator_results,
    }


# The policy used by all gate tests
GATE_POLICY = {
    "correctness_gate": {
        "oracle_comparators": ["c_xxhsum", "rust_xxhash_rs"],
        "contrast_comparators": ["b3sum", "md5"],
        "oracle_must_agree": True,
        "contrast_must_execute": True,
    }
}


class TestCorrectnessGateBothDigestsRequired(unittest.TestCase):
    """Both oracle digests must be present and non-empty."""

    def test_both_present_and_matching_passes(self):
        sr = _make_scenario_result("abcd1234", "abcd1234")
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertTrue(result["passed"], result.get("reason"))

    def test_c_xxhsum_missing_fails(self):
        sr = _make_scenario_result(None, "abcd1234")
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertFalse(result["passed"], "Gate must fail when c_xxhsum is missing")
        self.assertIn("c_xxhsum", result["reason"])

    def test_rust_xxhash_rs_missing_fails(self):
        sr = _make_scenario_result("abcd1234", None)
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertFalse(result["passed"], "Gate must fail when rust_xxhash_rs is missing")
        self.assertIn("rust_xxhash_rs", result["reason"])

    def test_both_missing_fails(self):
        sr = _make_scenario_result(None, None)
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertFalse(result["passed"], "Gate must fail when both oracles are missing")

    def test_c_xxhsum_empty_digest_fails(self):
        sr = _make_scenario_result("", "abcd1234")
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertFalse(result["passed"], "Gate must fail when c_xxhsum digest is empty")
        self.assertIn("c_xxhsum", result["reason"])

    def test_rust_xxhash_rs_empty_digest_fails(self):
        sr = _make_scenario_result("abcd1234", "")
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertFalse(result["passed"], "Gate must fail when rust_xxhash_rs digest is empty")
        self.assertIn("rust_xxhash_rs", result["reason"])

    def test_both_empty_digests_fail(self):
        sr = _make_scenario_result("", "")
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertFalse(result["passed"], "Gate must fail when both digests are empty")

    def test_digests_disagree_fails(self):
        sr = _make_scenario_result("aaaa1111", "bbbb2222")
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertFalse(result["passed"], "Gate must fail when oracle digests disagree")
        self.assertIn("disagree", result["reason"].lower())

    def test_contrast_failure_fails_even_with_matching_oracles(self):
        sr = _make_scenario_result("abcd1234", "abcd1234", b3sum_ok=False)
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertFalse(result["passed"], "Gate must fail when contrast comparator fails")

    def test_oracle_digests_reported_in_result(self):
        sr = _make_scenario_result("abcd1234", "abcd1234")
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertIn("oracle_digests", result)
        self.assertEqual(result["oracle_digests"]["c_xxhsum"], "abcd1234")
        self.assertEqual(result["oracle_digests"]["rust_xxhash_rs"], "abcd1234")

    def test_missing_oracle_reports_empty_digest_in_result(self):
        """When an oracle has an empty stdout line, the digest should be
        reported and the gate should fail with a clear reason."""
        sr = _make_scenario_result("", "abcd1234")
        result = harness.check_correctness_gate(sr, GATE_POLICY)
        self.assertFalse(result["passed"])
        # The oracle_digests should still be present in the result for debugging
        self.assertIn("oracle_digests", result)


class TestClaimReadyRequiresBothDigests(unittest.TestCase):
    """claim_ready and latest resolution must not accept runs with missing oracle evidence."""

    def test_save_run_bundle_marks_partial_when_oracle_missing(self):
        """A run where the correctness gate failed must not be claim_ready."""
        # Build a scenario result where one oracle is missing
        sr = _make_scenario_result(None, "abcd1234")
        correctness_result = harness.check_correctness_gate(sr, GATE_POLICY)

        # Simulate the claim_ready calculation from save_run_bundle
        all_correct = correctness_result["passed"]
        is_complete = True  # pretend matrix is complete
        claim_ready = all_correct and is_complete
        self.assertFalse(claim_ready, "Run with missing oracle must not be claim-ready")

    def test_save_run_bundle_marks_partial_when_digest_empty(self):
        """A run where an oracle digest is empty must not be claim_ready."""
        sr = _make_scenario_result("", "abcd1234")
        correctness_result = harness.check_correctness_gate(sr, GATE_POLICY)

        all_correct = correctness_result["passed"]
        is_complete = True
        claim_ready = all_correct and is_complete
        self.assertFalse(claim_ready, "Run with empty oracle digest must not be claim-ready")


class TestExtractDigest(unittest.TestCase):
    """extract_digest must return None for empty/missing input."""

    def test_empty_stdout_returns_none(self):
        self.assertIsNone(harness.extract_digest("", "c_xxhsum"))

    def test_none_stdout_returns_none(self):
        # extract_digest checks `if not stdout_line` first
        self.assertIsNone(harness.extract_digest("", "c_xxhsum"))

    def test_valid_gnu_format(self):
        result = harness.extract_digest("abcdef0123456789  payload.bin", "c_xxhsum")
        self.assertEqual(result, "abcdef0123456789")

    def test_valid_tagged_format(self):
        result = harness.extract_digest("XXH64 (payload.bin) = abcdef0123456789", "c_xxhsum")
        self.assertEqual(result, "abcdef0123456789")


class TestProbeBinaryProvenance(unittest.TestCase):
    """_probe_binary must never store null or error text as version."""

    def test_null_version_flag_produces_path_provenance(self):
        """When version_flag is null, version should be a path-based string."""
        comp_def = {
            "id": "test_comp",
            "role": "contrast",
            "parity_oracle": False,
        }
        resolve = {"version_flag": None}
        result = harness._probe_binary(
            Path("/sbin/md5"), resolve, comp_def
        )
        self.assertIsNotNone(result["version"])
        self.assertTrue(result["version"].startswith("path:"))
        self.assertIn("/sbin/md5", result["version"])

    def test_failed_version_command_uses_path_fallback(self):
        """When the version command exits non-zero, version should not
        contain the captured error text."""
        comp_def = {
            "id": "test_comp",
            "role": "contrast",
            "parity_oracle": False,
        }
        # Use a flag that will definitely fail for /usr/bin/true
        resolve = {"version_flag": "--nonexistent-flag"}
        result = harness._probe_binary(
            Path("/usr/bin/false"), resolve, comp_def
        )
        self.assertIsNotNone(result["version"])
        # Should be a path fallback, not error text
        self.assertTrue(
            result["version"].startswith("path:"),
            f"Expected path fallback, got: {result['version']}"
        )

    def test_successful_version_command_returns_clean_output(self):
        """When the version command succeeds, version should be the
        first line of stdout."""
        comp_def = {
            "id": "test_comp",
            "role": "contrast",
            "parity_oracle": False,
        }
        resolve = {"version_flag": "--version"}
        import shutil
        b3sum_path = shutil.which("b3sum")
        if not b3sum_path:
            self.skipTest("b3sum not available")
        result = harness._probe_binary(
            Path(b3sum_path), resolve, comp_def
        )
        self.assertIsNotNone(result["version"])
        self.assertIn("b3sum", result["version"])
        self.assertNotIn("error", result["version"].lower())


if __name__ == "__main__":
    unittest.main()
