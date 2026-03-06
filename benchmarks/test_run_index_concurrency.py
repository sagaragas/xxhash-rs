#!/usr/bin/env python3
"""
Regression tests for run-index concurrency hardening.

Proves that:
1. Empty or corrupt index.json does not crash _update_run_index or
   resolve_latest_run.
2. Atomic _write_json prevents partial/corrupt reads on concurrent access.
3. Concurrent _update_run_index calls from multiple threads converge to
   valid JSON state.
4. load_json_safe returns the default on missing, empty, partial, and
   corrupt files.
"""

import json
import os
import sys
import tempfile
import threading
import unittest
from pathlib import Path

# Ensure we can import the harness module
sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness  # noqa: E402


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _minimal_manifest(run_id: str = "test-run-001") -> dict:
    """Return a manifest dict with the minimal fields needed by
    _update_run_index."""
    return {
        "run_id": run_id,
        "timestamp_utc": "2026-01-15T12:00:00+00:00",
        "run_type": "smoke",
        "status": "complete",
        "claim_ready": True,
        "scenario_count": 4,
        "environment": {"repo_revision": "abc123"},
        "manifest_hashes": {
            "scenarios": "s_hash",
            "comparators": "c_hash",
            "policy": "p_hash",
        },
    }


# ---------------------------------------------------------------------------
# load_json_safe
# ---------------------------------------------------------------------------

class TestLoadJsonSafe(unittest.TestCase):
    """load_json_safe must tolerate every flavour of bad input."""

    def test_missing_file_returns_default(self):
        path = Path(tempfile.mktemp(suffix=".json"))
        result = harness.load_json_safe(path, default={"runs": []})
        self.assertEqual(result, {"runs": []})

    def test_empty_file_returns_default(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            f.write("")
            f.flush()
            path = Path(f.name)
        try:
            result = harness.load_json_safe(path, default={"runs": []})
            self.assertEqual(result, {"runs": []})
        finally:
            os.unlink(path)

    def test_corrupt_json_returns_default(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            f.write("{invalid json!!!}")
            f.flush()
            path = Path(f.name)
        try:
            result = harness.load_json_safe(path, default={"runs": []})
            self.assertEqual(result, {"runs": []})
        finally:
            os.unlink(path)

    def test_truncated_json_returns_default(self):
        """Simulates a partial write: valid JSON prefix but truncated."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            f.write('{"runs": [{"run_id": "test"')  # truncated
            f.flush()
            path = Path(f.name)
        try:
            result = harness.load_json_safe(path, default={"runs": []})
            self.assertEqual(result, {"runs": []})
        finally:
            os.unlink(path)

    def test_null_json_returns_default(self):
        """A file containing just 'null' should return the default."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            f.write("null")
            f.flush()
            path = Path(f.name)
        try:
            result = harness.load_json_safe(path, default={"runs": []})
            self.assertEqual(result, {"runs": []})
        finally:
            os.unlink(path)

    def test_json_array_returns_default(self):
        """A file containing a JSON array (not dict) should return default."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            f.write('[1, 2, 3]')
            f.flush()
            path = Path(f.name)
        try:
            result = harness.load_json_safe(path, default={"runs": []})
            self.assertEqual(result, {"runs": []})
        finally:
            os.unlink(path)

    def test_valid_json_returns_data(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({"runs": [{"run_id": "abc"}]}, f)
            f.flush()
            path = Path(f.name)
        try:
            result = harness.load_json_safe(path)
            self.assertEqual(result["runs"][0]["run_id"], "abc")
        finally:
            os.unlink(path)

    def test_default_is_empty_dict_when_not_specified(self):
        path = Path(tempfile.mktemp(suffix=".json"))
        result = harness.load_json_safe(path)
        self.assertEqual(result, {})


# ---------------------------------------------------------------------------
# Atomic _write_json
# ---------------------------------------------------------------------------

class TestAtomicWriteJson(unittest.TestCase):
    """_write_json must produce valid JSON even under concurrent writes."""

    def test_write_produces_valid_json(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "test.json"
            data = {"key": "value", "nested": {"a": 1}}
            harness._write_json(path, data)

            with open(path) as f:
                loaded = json.load(f)
            self.assertEqual(loaded, data)

    def test_write_is_atomic_no_partial_reads(self):
        """Multiple rapid writes should never leave the file in a
        half-written state that json.load cannot parse."""
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "index.json"
            errors = []
            stop = threading.Event()

            def writer():
                for i in range(50):
                    if stop.is_set():
                        break
                    harness._write_json(path, {"runs": [{"i": i}]})

            def reader():
                for _ in range(100):
                    if stop.is_set():
                        break
                    try:
                        if path.exists():
                            with open(path) as f:
                                data = json.load(f)
                            # Verify structure
                            if not isinstance(data, dict):
                                errors.append(f"Not a dict: {type(data)}")
                    except json.JSONDecodeError as e:
                        errors.append(str(e))
                    except OSError:
                        pass  # file might be in-flight

            threads = [
                threading.Thread(target=writer),
                threading.Thread(target=reader),
            ]
            for t in threads:
                t.start()
            for t in threads:
                t.join(timeout=30)
            stop.set()

            self.assertEqual(
                errors, [],
                f"Concurrent reads hit corrupt JSON: {errors[:5]}"
            )


# ---------------------------------------------------------------------------
# _update_run_index with corrupt / empty pre-existing index
# ---------------------------------------------------------------------------

class TestUpdateRunIndexCorruptRecovery(unittest.TestCase):
    """_update_run_index must recover when index.json is empty or corrupt."""

    def setUp(self):
        self._tmpdir = tempfile.mkdtemp(prefix="test_run_index_")
        self._orig_runs_dir = harness.RUNS_DIR
        harness.RUNS_DIR = Path(self._tmpdir)

    def tearDown(self):
        harness.RUNS_DIR = self._orig_runs_dir
        import shutil
        shutil.rmtree(self._tmpdir, ignore_errors=True)

    def test_empty_index_file_does_not_crash(self):
        """If index.json is an empty file, _update_run_index should
        create a fresh index and append the new entry."""
        index_path = harness.RUNS_DIR / "index.json"
        index_path.parent.mkdir(parents=True, exist_ok=True)
        index_path.write_text("")

        manifest = _minimal_manifest()
        harness._update_run_index("run-001", manifest)

        with open(index_path) as f:
            index = json.load(f)
        self.assertIn("runs", index)
        self.assertEqual(len(index["runs"]), 1)
        self.assertEqual(index["runs"][0]["run_id"], "run-001")

    def test_corrupt_index_file_does_not_crash(self):
        """If index.json contains garbage, _update_run_index should
        start a fresh index."""
        index_path = harness.RUNS_DIR / "index.json"
        index_path.parent.mkdir(parents=True, exist_ok=True)
        index_path.write_text("{broken json content")

        manifest = _minimal_manifest()
        harness._update_run_index("run-001", manifest)

        with open(index_path) as f:
            index = json.load(f)
        self.assertIn("runs", index)
        self.assertEqual(len(index["runs"]), 1)

    def test_missing_index_file_creates_new(self):
        """If index.json does not exist, _update_run_index creates it."""
        manifest = _minimal_manifest()
        harness._update_run_index("run-001", manifest)

        index_path = harness.RUNS_DIR / "index.json"
        self.assertTrue(index_path.exists())
        with open(index_path) as f:
            index = json.load(f)
        self.assertEqual(len(index["runs"]), 1)

    def test_index_with_broken_runs_key_recovers(self):
        """If index.json has 'runs' as a non-list, _update_run_index
        recovers and creates a valid index."""
        index_path = harness.RUNS_DIR / "index.json"
        index_path.parent.mkdir(parents=True, exist_ok=True)
        with open(index_path, "w") as f:
            json.dump({"runs": "not_a_list"}, f)

        manifest = _minimal_manifest()
        harness._update_run_index("run-001", manifest)

        with open(index_path) as f:
            index = json.load(f)
        self.assertIsInstance(index["runs"], list)
        self.assertEqual(len(index["runs"]), 1)


# ---------------------------------------------------------------------------
# resolve_latest_run with corrupt / empty index
# ---------------------------------------------------------------------------

class TestResolveLatestRunCorruptRecovery(unittest.TestCase):
    """resolve_latest_run must return None (not crash) on corrupt index."""

    def setUp(self):
        self._tmpdir = tempfile.mkdtemp(prefix="test_resolve_latest_")
        self._orig_runs_dir = harness.RUNS_DIR
        harness.RUNS_DIR = Path(self._tmpdir)

    def tearDown(self):
        harness.RUNS_DIR = self._orig_runs_dir
        import shutil
        shutil.rmtree(self._tmpdir, ignore_errors=True)

    def test_empty_index_returns_none(self):
        index_path = harness.RUNS_DIR / "index.json"
        index_path.write_text("")
        result = harness.resolve_latest_run()
        self.assertIsNone(result)

    def test_corrupt_index_returns_none(self):
        index_path = harness.RUNS_DIR / "index.json"
        index_path.write_text("not valid json at all")
        result = harness.resolve_latest_run()
        self.assertIsNone(result)

    def test_index_with_non_list_runs_returns_none(self):
        index_path = harness.RUNS_DIR / "index.json"
        with open(index_path, "w") as f:
            json.dump({"runs": "string_not_list"}, f)
        result = harness.resolve_latest_run()
        self.assertIsNone(result)


# ---------------------------------------------------------------------------
# Concurrent _update_run_index
# ---------------------------------------------------------------------------

class TestConcurrentUpdateRunIndex(unittest.TestCase):
    """Multiple threads calling _update_run_index should not corrupt the
    index or crash with JSONDecodeError."""

    def setUp(self):
        self._tmpdir = tempfile.mkdtemp(prefix="test_concurrent_index_")
        self._orig_runs_dir = harness.RUNS_DIR
        harness.RUNS_DIR = Path(self._tmpdir)

    def tearDown(self):
        harness.RUNS_DIR = self._orig_runs_dir
        import shutil
        shutil.rmtree(self._tmpdir, ignore_errors=True)

    def test_concurrent_updates_produce_valid_json(self):
        """Parallel _update_run_index calls must leave index.json as
        valid JSON.  We accept that some entries may be lost due to
        last-writer-wins, but the file must never be corrupt."""
        num_threads = 8
        updates_per_thread = 10
        errors = []

        def updater(thread_id):
            for i in range(updates_per_thread):
                run_id = f"run-t{thread_id}-{i:03d}"
                try:
                    manifest = _minimal_manifest(run_id)
                    harness._update_run_index(run_id, manifest)
                except Exception as e:
                    errors.append(f"Thread {thread_id} iteration {i}: {e}")

        threads = [
            threading.Thread(target=updater, args=(tid,))
            for tid in range(num_threads)
        ]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=60)

        # The file must be valid JSON after all threads finish
        index_path = harness.RUNS_DIR / "index.json"
        self.assertTrue(index_path.exists(), "index.json should exist")

        with open(index_path) as f:
            index = json.load(f)

        self.assertIn("runs", index)
        self.assertIsInstance(index["runs"], list)
        # At minimum one entry should be present; under contention some
        # may be lost, but the file must always be valid JSON.
        self.assertGreater(len(index["runs"]), 0)

        # No thread should have crashed
        self.assertEqual(errors, [], f"Errors during concurrent updates: {errors}")

    def test_concurrent_update_and_resolve(self):
        """Concurrent _update_run_index + resolve_latest_run must not
        crash.  resolve_latest_run may return None during contention
        but must never raise."""
        errors = []

        def updater():
            for i in range(20):
                run_id = f"run-{i:03d}"
                try:
                    manifest = _minimal_manifest(run_id)
                    harness._update_run_index(run_id, manifest)
                except Exception as e:
                    errors.append(f"updater {i}: {e}")

        def resolver():
            for _ in range(30):
                try:
                    harness.resolve_latest_run()
                except Exception as e:
                    errors.append(f"resolver: {e}")

        threads = [
            threading.Thread(target=updater),
            threading.Thread(target=resolver),
        ]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=60)

        self.assertEqual(errors, [], f"Errors during concurrent ops: {errors}")


# ---------------------------------------------------------------------------
# Repeated smoke execution keeps index valid
# ---------------------------------------------------------------------------

class TestRepeatedIndexUpdatesKeepStateValid(unittest.TestCase):
    """Simulates repeated smoke runs appending to the same index."""

    def setUp(self):
        self._tmpdir = tempfile.mkdtemp(prefix="test_repeated_smoke_")
        self._orig_runs_dir = harness.RUNS_DIR
        harness.RUNS_DIR = Path(self._tmpdir)

    def tearDown(self):
        harness.RUNS_DIR = self._orig_runs_dir
        import shutil
        shutil.rmtree(self._tmpdir, ignore_errors=True)

    def test_sequential_updates_accumulate(self):
        """N sequential _update_run_index calls produce exactly N entries."""
        n = 10
        for i in range(n):
            manifest = _minimal_manifest(f"run-{i:03d}")
            harness._update_run_index(f"run-{i:03d}", manifest)

        index_path = harness.RUNS_DIR / "index.json"
        with open(index_path) as f:
            index = json.load(f)

        self.assertEqual(len(index["runs"]), n)
        # Verify all entries are present
        ids = {r["run_id"] for r in index["runs"]}
        for i in range(n):
            self.assertIn(f"run-{i:03d}", ids)

    def test_index_is_valid_json_after_each_update(self):
        """After every _update_run_index, the file should parse cleanly."""
        for i in range(20):
            manifest = _minimal_manifest(f"run-{i:03d}")
            harness._update_run_index(f"run-{i:03d}", manifest)

            index_path = harness.RUNS_DIR / "index.json"
            with open(index_path) as f:
                index = json.load(f)
            self.assertIsInstance(index["runs"], list)
            self.assertEqual(len(index["runs"]), i + 1)


if __name__ == "__main__":
    unittest.main()
