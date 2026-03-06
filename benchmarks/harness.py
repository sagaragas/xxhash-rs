#!/usr/bin/env python3
"""
Manifest-driven benchmark harness for xxhash-rs rewrite study.

Executes the canonical comparator matrix across declared scenarios,
retains raw samples and run metadata, enforces completeness and
correctness gates, and provides safe latest-run resolution.

Usage:
    python3 benchmarks/harness.py smoke --run-set local
    python3 benchmarks/harness.py full --run-set local
    python3 benchmarks/harness.py latest
    python3 benchmarks/harness.py inspect --run <run-id>
"""

import argparse
import fcntl
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path


# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

HARNESS_DIR = Path(__file__).resolve().parent
REPO_ROOT = HARNESS_DIR.parent
RUNS_DIR = HARNESS_DIR / "runs"
SCENARIOS_PATH = HARNESS_DIR / "scenarios.json"
COMPARATORS_PATH = HARNESS_DIR / "comparators.json"
POLICY_PATH = HARNESS_DIR / "policy.json"

CANONICAL_COMPARATOR_IDS = ["c_xxhsum", "rust_xxhash_rs", "b3sum", "md5"]


# ---------------------------------------------------------------------------
# Manifest loading and hashing
# ---------------------------------------------------------------------------

def load_json(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


def load_json_safe(path: Path, default=None):
    """Load JSON from *path*, returning *default* on any read/parse failure.

    Tolerates missing files, empty files, partially-written files, and
    corrupt JSON — all of which can occur when parallel smoke runs or
    tests race to update the run index.
    """
    if default is None:
        default = {}
    try:
        with open(path) as f:
            data = json.load(f)
        if not isinstance(data, dict):
            return default
        return data
    except (OSError, json.JSONDecodeError, ValueError):
        return default


def file_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def load_manifests():
    scenarios = load_json(SCENARIOS_PATH)
    comparators = load_json(COMPARATORS_PATH)
    policy = load_json(POLICY_PATH)
    return scenarios, comparators, policy


# ---------------------------------------------------------------------------
# Comparator resolution
# ---------------------------------------------------------------------------

def resolve_comparator(comp_def: dict) -> dict | None:
    """Resolve a comparator definition to an executable path and version."""
    resolve = comp_def["resolve"]
    binary_name = resolve["binary_name"]

    # Try environment variable first
    env_var = resolve.get("binary_path_env")
    if env_var and os.environ.get(env_var):
        env_root = os.environ[env_var]
        candidate = Path(env_root) / binary_name
        if candidate.exists():
            return _probe_binary(candidate, resolve, comp_def)

    # Try default path
    default_path = resolve.get("default_path")
    if default_path and Path(default_path).exists():
        return _probe_binary(Path(default_path), resolve, comp_def)

    # For rust_xxhash_rs, try building and finding in target/release
    build_cmd = resolve.get("build_command")
    if build_cmd:
        result = subprocess.run(
            build_cmd.split(),
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
        )
        if result.returncode == 0:
            candidate = REPO_ROOT / "target" / "release" / binary_name
            if candidate.exists():
                return _probe_binary(candidate, resolve, comp_def)

    # Try PATH lookup
    which = shutil.which(binary_name)
    if which:
        return _probe_binary(Path(which), resolve, comp_def)

    return None


def _probe_binary(binary_path: Path, resolve: dict, comp_def: dict) -> dict:
    """Probe a binary for version info.

    Provenance hardening rules:
    - If version_flag is set and the command succeeds (exit 0), use
      the first non-empty line of stdout (preferred) or stderr.
    - If the version command fails (non-zero exit) or times out, fall
      back to a deterministic path-based provenance string rather than
      storing captured error text.
    - If version_flag is null (e.g. macOS ``md5``), synthesise a
      deterministic provenance string from the resolved binary path
      so the manifest never contains a null version field.
    """
    version_flag = resolve.get("version_flag")
    if version_flag:
        try:
            result = subprocess.run(
                [str(binary_path), version_flag],
                capture_output=True,
                text=True,
                timeout=10,
            )
            if result.returncode == 0:
                raw = (result.stdout.strip() or result.stderr.strip())
                version = raw.split("\n")[0][:200] if raw else None
            else:
                # Command failed — do not capture error text as version.
                version = None
        except (subprocess.TimeoutExpired, OSError):
            version = None
    else:
        version = None

    # Deterministic fallback: use "path:<binary_path>" so the manifest
    # always carries a non-null, non-error provenance string.
    if not version:
        version = f"path:{binary_path}"

    return {
        "id": comp_def["id"],
        "binary_path": str(binary_path),
        "version": version,
        "role": comp_def["role"],
        "parity_oracle": comp_def["parity_oracle"],
    }


def resolve_all_comparators(comparators_manifest: dict) -> dict:
    """Resolve all canonical comparators. Returns {id: resolved_info}."""
    resolved = {}
    for comp_def in comparators_manifest["canonical_comparators"]:
        info = resolve_comparator(comp_def)
        if info:
            resolved[comp_def["id"]] = info
    return resolved


# ---------------------------------------------------------------------------
# Invocation building
# ---------------------------------------------------------------------------

def build_invocation(
    comp_id: str,
    algorithm: str,
    comparators_manifest: dict,
    resolved: dict,
    payload_file: str,
) -> list[str] | None:
    """Build the command-line invocation for a comparator + algorithm + file."""
    comp_def = None
    for c in comparators_manifest["canonical_comparators"]:
        if c["id"] == comp_id:
            comp_def = c
            break
    if not comp_def:
        return None

    info = resolved.get(comp_id)
    if not info:
        return None

    template = comp_def["invocation_template"].get(algorithm, [])
    if not template:
        return None

    binary_path = info["binary_path"]
    cmd = [part.replace("{binary}", binary_path) for part in template]
    cmd.append(payload_file)
    return cmd


# ---------------------------------------------------------------------------
# Payload generation
# ---------------------------------------------------------------------------

def create_payload(size_bytes: int, work_dir: Path) -> Path:
    """Create a deterministic payload file of the specified size."""
    payload_path = work_dir / f"payload_{size_bytes}.bin"
    if payload_path.exists():
        return payload_path

    # Deterministic pseudo-random content using a simple PRNG pattern
    # We use repeating 256-byte blocks for speed
    block = bytes(range(256))
    with open(payload_path, "wb") as f:
        remaining = size_bytes
        while remaining > 0:
            chunk = block[:remaining] if remaining < 256 else block
            f.write(chunk)
            remaining -= len(chunk)
    return payload_path


# ---------------------------------------------------------------------------
# Single measurement
# ---------------------------------------------------------------------------

def measure_single(cmd: list[str], payload_file: str) -> dict:
    """Run a single invocation and capture timing + output."""
    start = time.perf_counter_ns()
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=120,
        )
        elapsed_ns = time.perf_counter_ns() - start
        return {
            "command": cmd,
            "elapsed_ns": elapsed_ns,
            "exit_code": result.returncode,
            "stdout_first_line": (result.stdout.strip().split("\n")[0] if result.stdout else ""),
            "stderr_snippet": result.stderr.strip()[:200] if result.stderr else "",
            "success": result.returncode == 0,
        }
    except subprocess.TimeoutExpired:
        elapsed_ns = time.perf_counter_ns() - start
        return {
            "command": cmd,
            "elapsed_ns": elapsed_ns,
            "exit_code": -1,
            "stdout_first_line": "",
            "stderr_snippet": "TIMEOUT",
            "success": False,
        }
    except OSError as e:
        return {
            "command": cmd,
            "elapsed_ns": 0,
            "exit_code": -1,
            "stdout_first_line": "",
            "stderr_snippet": str(e)[:200],
            "success": False,
        }


# ---------------------------------------------------------------------------
# Run a scenario
# ---------------------------------------------------------------------------

def run_scenario(
    scenario: dict,
    comparators_manifest: dict,
    resolved: dict,
    work_dir: Path,
    is_smoke: bool = False,
) -> dict:
    """Execute a single scenario across all its comparators."""
    scenario_id = scenario["id"]
    algorithm = scenario["algorithm"]
    payload_bytes = scenario["payload_bytes"]
    warmup_iters = 1 if is_smoke else scenario["warmup_iterations"]
    measured_iters = 2 if is_smoke else scenario["measured_iterations"]

    payload_file = create_payload(payload_bytes, work_dir)
    payload_checksum = file_sha256(payload_file)

    comparator_results = {}
    coverage_ledger = {}

    for comp_id in scenario["comparators"]:
        cmd = build_invocation(
            comp_id, algorithm, comparators_manifest, resolved, str(payload_file)
        )
        if not cmd:
            coverage_ledger[comp_id] = "missing"
            comparator_results[comp_id] = {
                "status": "missing",
                "error": f"Comparator {comp_id} not resolved",
            }
            continue

        # Warmup
        warmup_samples = []
        for _ in range(warmup_iters):
            sample = measure_single(cmd, str(payload_file))
            warmup_samples.append(sample)

        # Measured
        measured_samples = []
        all_success = True
        for _ in range(measured_iters):
            sample = measure_single(cmd, str(payload_file))
            measured_samples.append(sample)
            if not sample["success"]:
                all_success = False

        elapsed_values = [s["elapsed_ns"] for s in measured_samples if s["success"]]
        if elapsed_values:
            elapsed_values.sort()
            median_ns = elapsed_values[len(elapsed_values) // 2]
        else:
            median_ns = None

        status = "success" if all_success else "failed"
        coverage_ledger[comp_id] = status

        comparator_results[comp_id] = {
            "status": status,
            "invocation": cmd,
            "warmup_samples": warmup_samples,
            "measured_samples": measured_samples,
            "median_ns": median_ns,
            "sample_count": len(measured_samples),
        }

    return {
        "scenario_id": scenario_id,
        "algorithm": algorithm,
        "payload_bytes": payload_bytes,
        "payload_checksum": payload_checksum,
        "warmup_iterations": warmup_iters,
        "measured_iterations": measured_iters,
        "comparator_results": comparator_results,
        "coverage_ledger": coverage_ledger,
    }


# ---------------------------------------------------------------------------
# Correctness gate (c_xxhsum vs rust_xxhash_rs parity)
# ---------------------------------------------------------------------------

def extract_digest(stdout_line: str, comp_id: str) -> str | None:
    """Extract the hex digest from the first output line of a comparator."""
    if not stdout_line:
        return None

    # b3sum and md5 have their own formats; we only need the first hex field
    if comp_id in ("b3sum", "md5"):
        parts = stdout_line.split()
        return parts[0].lower() if parts else None

    # For xxhash tools: handle GNU and tagged format
    line = stdout_line.lstrip("\\")

    # Tagged: ALGO (file) = hexdigest
    if ") = " in line:
        hex_part = line.rsplit(") = ", 1)[-1].strip()
        if hex_part and all(c in "0123456789abcdefABCDEF" for c in hex_part):
            return hex_part.lower()

    # GNU: hex  filename  OR  XXH3_hex  filename
    first = line.split()[0] if line.split() else ""
    if first.startswith("XXH3_"):
        first = first[5:]
    return first.lower() if first else None


def check_correctness_gate(scenario_result: dict, policy: dict) -> dict:
    """Check that oracle comparators agree on digest.

    Both oracle digests must be present and non-empty.  A missing or
    empty digest is a hard failure — the gate never silently filters
    absent values away.
    """
    gate = policy["correctness_gate"]
    oracle_ids = gate["oracle_comparators"]
    contrast_ids = gate["contrast_comparators"]

    oracle_digests = {}
    for oid in oracle_ids:
        cr = scenario_result["comparator_results"].get(oid)
        if not cr or cr["status"] != "success":
            return {
                "passed": False,
                "reason": f"Oracle comparator {oid} did not succeed",
                "oracle_digests": {},
            }
        # Use the first measured sample's stdout for digest extraction
        samples = cr.get("measured_samples", [])
        if not samples:
            return {
                "passed": False,
                "reason": f"Oracle comparator {oid} has no measured samples",
                "oracle_digests": {},
            }
        digest = extract_digest(samples[0].get("stdout_first_line", ""), oid)
        oracle_digests[oid] = digest

    # --- Hard requirement: every oracle digest must be present and non-empty ---
    for oid in oracle_ids:
        if not oracle_digests.get(oid):
            return {
                "passed": False,
                "reason": (
                    f"Oracle comparator {oid} produced a missing or empty digest"
                ),
                "oracle_digests": oracle_digests,
            }

    # Check oracle agreement (all digests are guaranteed non-empty here)
    unique_digests = set(oracle_digests.values())
    if len(unique_digests) != 1:
        return {
            "passed": False,
            "reason": f"Oracle comparators disagree: {oracle_digests}",
            "oracle_digests": oracle_digests,
        }

    # Check contrast comparators executed successfully
    for cid in contrast_ids:
        cr = scenario_result["comparator_results"].get(cid)
        if not cr or cr["status"] != "success":
            return {
                "passed": False,
                "reason": f"Contrast comparator {cid} did not execute successfully",
                "oracle_digests": oracle_digests,
            }

    return {
        "passed": True,
        "reason": "All oracles agree and contrast comparators succeeded",
        "oracle_digests": oracle_digests,
    }


# ---------------------------------------------------------------------------
# Run completeness check
# ---------------------------------------------------------------------------

def check_matrix_completeness(run_results: list[dict]) -> dict:
    """Check that every scenario covered all canonical comparators."""
    missing = []
    for sr in run_results:
        ledger = sr.get("coverage_ledger", {})
        for comp_id in CANONICAL_COMPARATOR_IDS:
            if ledger.get(comp_id) not in ("success",):
                missing.append({
                    "scenario": sr["scenario_id"],
                    "comparator": comp_id,
                    "status": ledger.get(comp_id, "absent"),
                })
    return {
        "complete": len(missing) == 0,
        "missing_entries": missing,
    }


# ---------------------------------------------------------------------------
# Environment metadata
# ---------------------------------------------------------------------------

def collect_environment_metadata() -> dict:
    """Collect host and build environment metadata."""
    meta = {
        "hostname": platform.node(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python_version": platform.python_version(),
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
    }

    # Git revision of the working repo
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
        )
        if result.returncode == 0:
            meta["repo_revision"] = result.stdout.strip()
    except OSError:
        pass

    # Git dirty check
    try:
        result = subprocess.run(
            ["git", "status", "--porcelain"],
            capture_output=True,
            text=True,
            cwd=REPO_ROOT,
        )
        if result.returncode == 0:
            meta["repo_dirty"] = bool(result.stdout.strip())
    except OSError:
        pass

    return meta


# ---------------------------------------------------------------------------
# Run bundle persistence
# ---------------------------------------------------------------------------

def generate_run_id() -> str:
    """Generate a unique run ID based on timestamp."""
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    pid = os.getpid()
    return f"run-{ts}-{pid}"


def save_run_bundle(
    run_id: str,
    run_type: str,
    scenario_results: list[dict],
    correctness_results: list[dict],
    completeness: dict,
    resolved_comparators: dict,
    environment: dict,
    manifest_hashes: dict,
    policy: dict,
) -> Path:
    """Persist a complete run bundle to the runs directory."""
    run_dir = RUNS_DIR / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    # Determine claim-readiness
    all_correct = all(cr["passed"] for cr in correctness_results)
    is_complete = completeness["complete"]
    claim_ready = all_correct and is_complete

    # Run manifest (top-level metadata)
    run_manifest = {
        "run_id": run_id,
        "run_type": run_type,
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "status": "complete" if claim_ready else "partial",
        "claim_ready": claim_ready,
        "manifest_hashes": manifest_hashes,
        "policy_version": policy.get("policy_version", "unknown"),
        "policy_hash": manifest_hashes.get("policy"),
        "environment": environment,
        "resolved_comparators": {
            k: {kk: vv for kk, vv in v.items() if kk != "binary_path"}
            | {"binary_path": v["binary_path"]}
            for k, v in resolved_comparators.items()
        },
        "correctness_gate": {
            "all_passed": all_correct,
            "results": correctness_results,
        },
        "completeness": completeness,
        "statistical_method": policy.get("statistical_method", {}),
        "scenario_count": len(scenario_results),
        "comparator_ids": CANONICAL_COMPARATOR_IDS,
    }
    _write_json(run_dir / "manifest.json", run_manifest)

    # Raw samples per scenario
    samples_dir = run_dir / "samples"
    samples_dir.mkdir(exist_ok=True)
    for sr in scenario_results:
        _write_json(samples_dir / f"{sr['scenario_id']}.json", sr)

    # Artifact checksums
    checksums = {}
    for f in sorted(run_dir.rglob("*.json")):
        rel = f.relative_to(run_dir)
        checksums[str(rel)] = file_sha256(f)
    _write_json(run_dir / "checksums.json", checksums)

    # Update run index
    _update_run_index(run_id, run_manifest)

    return run_dir


def _write_json(path: Path, data: dict):
    """Atomically write *data* as JSON to *path*.

    Writes to a temporary file in the same directory and then uses
    ``os.replace`` (atomic on POSIX) to move it into place.  This
    prevents partial/corrupt reads when another process opens the file
    concurrently.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(
        dir=str(path.parent), suffix=".tmp", prefix=".harness_"
    )
    try:
        with os.fdopen(fd, "w") as f:
            json.dump(data, f, indent=2, default=str)
            f.write("\n")
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, str(path))
    except BaseException:
        # Best-effort cleanup on failure
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


# ---------------------------------------------------------------------------
# Run index and latest resolution
# ---------------------------------------------------------------------------

def _update_run_index(run_id: str, manifest: dict):
    """Append this run to the run index under an exclusive file lock.

    The read-modify-write cycle is serialised with ``fcntl.flock`` so
    concurrent writers (parallel smoke runs, tests, subprocesses) never
    lose each other's entries.  The lock file is a separate ``.lock``
    file next to ``index.json`` so readers using ``load_json_safe`` or
    ``_write_json`` (atomic via temp-file + ``os.replace``) are not
    blocked.

    Tolerates empty/corrupt/missing index files on read via
    ``load_json_safe``.
    """
    index_path = RUNS_DIR / "index.json"
    lock_path = RUNS_DIR / "index.json.lock"
    index_path.parent.mkdir(parents=True, exist_ok=True)

    entry = {
        "run_id": run_id,
        "timestamp_utc": manifest["timestamp_utc"],
        "run_type": manifest["run_type"],
        "status": manifest["status"],
        "claim_ready": manifest["claim_ready"],
        "scenario_count": manifest["scenario_count"],
        "revision": manifest.get("environment", {}).get("repo_revision", ""),
        "manifest_hashes": manifest.get("manifest_hashes", {}),
    }

    lock_fd = os.open(str(lock_path), os.O_CREAT | os.O_RDWR)
    try:
        fcntl.flock(lock_fd, fcntl.LOCK_EX)

        # --- critical section: read, modify, write ---
        index = load_json_safe(index_path, default={"runs": []})
        if "runs" not in index or not isinstance(index.get("runs"), list):
            index = {"runs": []}

        index["runs"].append(entry)
        _write_json(index_path, index)
        # --- end critical section ---
    finally:
        fcntl.flock(lock_fd, fcntl.LOCK_UN)
        os.close(lock_fd)


def resolve_latest_run() -> dict | None:
    """Resolve 'latest' to the most recent complete, claim-ready run.

    Tolerates empty or corrupt ``index.json`` by falling back to an
    empty run list, so a partial write from a concurrent process does
    not crash the reader.
    """
    index_path = RUNS_DIR / "index.json"
    if not index_path.exists():
        return None

    index = load_json_safe(index_path, default={"runs": []})
    runs = index.get("runs", [])
    if not isinstance(runs, list):
        return None

    # Filter to complete, claim-ready runs and sort by timestamp desc
    eligible = [
        r for r in runs
        if r.get("status") == "complete" and r.get("claim_ready") is True
    ]
    if not eligible:
        return None

    # Sort by timestamp descending
    eligible.sort(key=lambda r: r.get("timestamp_utc", ""), reverse=True)
    latest = eligible[0]

    # Verify the run directory still exists and has a manifest
    run_dir = RUNS_DIR / latest["run_id"]
    manifest_path = run_dir / "manifest.json"
    if not manifest_path.exists():
        return None

    return {
        "run_id": latest["run_id"],
        "run_dir": str(run_dir),
        "timestamp_utc": latest["timestamp_utc"],
        "manifest": load_json(manifest_path),
    }


# ---------------------------------------------------------------------------
# Main run orchestration
# ---------------------------------------------------------------------------

def run_benchmark(run_type: str = "smoke", run_set: str = "local"):
    """Execute the full benchmark run."""
    print(f"=== xxhash-rs benchmark harness ({run_type}) ===")

    # Load manifests
    scenarios_manifest, comparators_manifest, policy = load_manifests()

    # Compute manifest hashes
    manifest_hashes = {
        "scenarios": file_sha256(SCENARIOS_PATH),
        "comparators": file_sha256(COMPARATORS_PATH),
        "policy": file_sha256(POLICY_PATH),
    }
    print(f"Manifest hashes: scenarios={manifest_hashes['scenarios'][:12]}... "
          f"comparators={manifest_hashes['comparators'][:12]}... "
          f"policy={manifest_hashes['policy'][:12]}...")

    # Resolve comparators
    resolved = resolve_all_comparators(comparators_manifest)
    print(f"\nResolved comparators: {list(resolved.keys())}")
    for comp_id, info in resolved.items():
        print(f"  {comp_id}: {info['binary_path']} "
              f"(version: {(info.get('version') or 'n/a')[:60]})")

    # Check all canonical comparators are available
    missing_comps = [c for c in CANONICAL_COMPARATOR_IDS if c not in resolved]
    if missing_comps:
        print(f"\nERROR: Missing canonical comparators: {missing_comps}")
        print("Cannot proceed without full comparator coverage.")
        return 1

    # Collect environment metadata
    environment = collect_environment_metadata()
    print(f"\nEnvironment: {environment.get('platform', 'unknown')}")
    print(f"Repo revision: {environment.get('repo_revision', 'unknown')[:12]}...")

    # Select scenarios for smoke vs full
    all_scenarios = scenarios_manifest["scenarios"]
    if run_type == "smoke":
        # Use a representative subset for smoke: first 3 + last 1
        if len(all_scenarios) <= 4:
            scenarios = all_scenarios
        else:
            scenarios = all_scenarios[:3] + [all_scenarios[-1]]
    else:
        scenarios = all_scenarios

    is_smoke = run_type == "smoke"
    print(f"\nRunning {len(scenarios)} scenarios ({run_type} mode)...")

    # Create work directory for payloads
    with tempfile.TemporaryDirectory(prefix="xxhash_bench_") as work_dir:
        work_path = Path(work_dir)

        # Execute scenarios
        scenario_results = []
        correctness_results = []
        for i, scenario in enumerate(scenarios):
            print(f"\n  [{i+1}/{len(scenarios)}] {scenario['id']} "
                  f"({scenario['algorithm']}, {scenario['payload_bytes']} bytes)")
            result = run_scenario(
                scenario, comparators_manifest, resolved, work_path, is_smoke
            )
            scenario_results.append(result)

            # Print coverage ledger
            ledger = result["coverage_ledger"]
            for comp_id, status in ledger.items():
                symbol = "✓" if status == "success" else "✗"
                timing = ""
                cr = result["comparator_results"].get(comp_id, {})
                if cr.get("median_ns"):
                    ms = cr["median_ns"] / 1_000_000
                    timing = f" ({ms:.2f} ms)"
                print(f"    {symbol} {comp_id}: {status}{timing}")

            # Correctness gate
            cg = check_correctness_gate(result, policy)
            correctness_results.append({
                "scenario_id": scenario["id"],
                **cg,
            })
            gate_sym = "✓" if cg["passed"] else "✗"
            print(f"    {gate_sym} correctness gate: {cg['reason'][:80]}")

    # Matrix completeness
    completeness = check_matrix_completeness(scenario_results)

    # Generate run ID and save
    run_id = generate_run_id()
    run_dir = save_run_bundle(
        run_id=run_id,
        run_type=run_type,
        scenario_results=scenario_results,
        correctness_results=correctness_results,
        completeness=completeness,
        resolved_comparators=resolved,
        environment=environment,
        manifest_hashes=manifest_hashes,
        policy=policy,
    )

    # Summary
    all_correct = all(cr["passed"] for cr in correctness_results)
    print(f"\n=== Run Summary ===")
    print(f"Run ID: {run_id}")
    print(f"Run dir: {run_dir}")
    print(f"Scenarios: {len(scenario_results)}")
    print(f"Matrix complete: {completeness['complete']}")
    print(f"Correctness gate: {'PASSED' if all_correct else 'FAILED'}")
    print(f"Claim ready: {completeness['complete'] and all_correct}")

    if not completeness["complete"]:
        print(f"\nMissing coverage entries:")
        for entry in completeness["missing_entries"]:
            print(f"  {entry['scenario']}/{entry['comparator']}: {entry['status']}")

    return 0 if (completeness["complete"] and all_correct) else 1


def cmd_latest():
    """Print the latest claim-ready run."""
    latest = resolve_latest_run()
    if not latest:
        print("No claim-ready runs found.")
        print("Run 'python3 benchmarks/harness.py smoke --run-set local' first.")
        return 1

    print(f"Latest claim-ready run: {latest['run_id']}")
    print(f"Timestamp: {latest['timestamp_utc']}")
    print(f"Run dir: {latest['run_dir']}")
    manifest = latest["manifest"]
    print(f"Scenarios: {manifest.get('scenario_count', '?')}")
    print(f"Comparators: {', '.join(manifest.get('comparator_ids', []))}")
    print(f"Correctness gate: {'PASSED' if manifest.get('correctness_gate', {}).get('all_passed') else 'FAILED'}")
    return 0


def cmd_inspect(run_id: str):
    """Inspect a specific run bundle."""
    run_dir = RUNS_DIR / run_id
    manifest_path = run_dir / "manifest.json"
    if not manifest_path.exists():
        print(f"Run {run_id} not found at {run_dir}")
        return 1

    manifest = load_json(manifest_path)
    print(json.dumps(manifest, indent=2, default=str))
    return 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="xxhash-rs benchmark harness",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    # smoke / full
    for mode in ("smoke", "full"):
        p = sub.add_parser(mode, help=f"Run {mode} benchmarks")
        p.add_argument("--run-set", default="local", help="Run set label")

    # latest
    sub.add_parser("latest", help="Show latest claim-ready run")

    # inspect
    p_inspect = sub.add_parser("inspect", help="Inspect a run bundle")
    p_inspect.add_argument("--run", required=True, help="Run ID")

    args = parser.parse_args()

    if args.command in ("smoke", "full"):
        return run_benchmark(run_type=args.command, run_set=args.run_set)
    elif args.command == "latest":
        return cmd_latest()
    elif args.command == "inspect":
        return cmd_inspect(args.run)
    return 1


if __name__ == "__main__":
    sys.exit(main())
