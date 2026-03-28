//! CLI --check mode: malformed-line policies.
//!
//! Validates VAL-CHECK-004:
//! - Default mode tolerates malformed lines when at least one checksum line is
//!   valid and still produces the aggregate malformed-line summary.
//! - `--warn` adds per-line malformed diagnostics to stderr.
//! - `--strict` makes malformed lines fatal (exit 1).
//! - An all-invalid checksum file is a hard failure with the explicit
//!   "no properly formatted xxHash checksum lines found" outcome.

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Returns the path to the Rust CLI binary built by cargo.

macro_rules! skip_without_reference {
    () => {
        if reference_binary().is_none() {
            eprintln!("Skipped: reference binary not available (set XXHASH_REFERENCE_ROOT)");
            return;
        }
    };
}

fn rust_binary() -> PathBuf {
    env!("CARGO_BIN_EXE_xxhash-rs").into()
}

/// Default path to the external reference checkout.
const DEFAULT_REFERENCE_ROOT: &str = "../xxhash-reference";

/// Returns the path to the reference `xxhsum` binary, if available.
fn reference_binary() -> Option<PathBuf> {
    let root = env::var("XXHASH_REFERENCE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_REFERENCE_ROOT));
    let bin = root.join("xxhsum");
    if bin.exists() {
        Some(bin)
    } else {
        None
    }
}

/// Run a CLI binary with the given args and optional stdin data.
fn run_cli(bin: &PathBuf, args: &[&str], stdin_data: Option<&[u8]>) -> (String, String, i32) {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if stdin_data.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }

    let mut child = cmd.spawn().expect("failed to spawn CLI");

    if let Some(data) = stdin_data {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(data).expect("failed to write stdin");
        }
    }

    let output = child.wait_with_output().expect("failed to wait for CLI");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

fn run_rust(args: &[&str]) -> (String, String, i32) {
    run_cli(&rust_binary(), args, None)
}

fn run_ref(args: &[&str]) -> (String, String, i32) {
    run_cli(
        &reference_binary().expect("reference binary not found"),
        args,
        None,
    )
}

/// Create a temporary directory for this test and return its path.
fn test_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("xxhash_cli_malformed_tests")
        .join(test_name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create test dir");
    dir
}

/// Generate a checksum line using the reference CLI.
fn generate_checksums_ref(args: &[&str]) -> String {
    let (stdout, _, code) = run_ref(args);
    assert_eq!(code, 0, "reference generation should succeed");
    stdout
}

// =========================================================================
// VAL-CHECK-004: Default mode — mixed valid/malformed
// =========================================================================

#[test]
fn cli_check_malformed_lines_default_mixed() {
    skip_without_reference!();
    let dir = test_dir("default_mixed");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let valid_line = generate_checksums_ref(&[path]);

    // Build checksum file with garbage lines mixed in
    let content = format!("garbage line 1\n{}another garbage line\n", valid_line);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &content).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(
        rust_code, ref_code,
        "exit codes should match (expected {}): rust_out='{}' rust_err='{}'",
        ref_code, rust_out, rust_err
    );
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // Default mode: exit 0, aggregate malformed summary in stdout, no per-line stderr
    assert_eq!(rust_code, 0, "default mode with valid lines should succeed");
    assert!(
        rust_out.contains("2 lines are improperly formatted"),
        "should show aggregate malformed summary in stdout"
    );
    assert!(rust_err.is_empty(), "default mode should not emit per-line warnings to stderr");
}

#[test]
fn cli_check_malformed_lines_default_single_malformed() {
    skip_without_reference!();
    let dir = test_dir("default_single_malformed");
    let file = dir.join("test.txt");
    fs::write(&file, b"test content\n").unwrap();
    let path = file.to_str().unwrap();

    let valid_line = generate_checksums_ref(&[path]);
    let content = format!("{}garbage\n", valid_line);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &content).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    assert!(
        rust_out.contains("1 line is improperly formatted"),
        "should show singular malformed summary"
    );
}

// =========================================================================
// VAL-CHECK-004: --warn mode — per-line diagnostics to stderr
// =========================================================================

#[test]
fn cli_check_malformed_lines_warn_mixed() {
    skip_without_reference!();
    let dir = test_dir("warn_mixed");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let valid_line = generate_checksums_ref(&[path]);
    let content = format!("garbage line 1\n{}another garbage line\n", valid_line);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &content).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--warn", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--warn", cf]);

    assert_eq!(
        rust_code, ref_code,
        "exit codes should match (expected {}): rust_err='{}'",
        ref_code, rust_err
    );
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // --warn: per-line diagnostics go to stderr
    assert!(
        rust_err.contains(&format!(
            "{}:1: Error: Improperly formatted checksum line.",
            cf
        )),
        "stderr should contain per-line warning for line 1"
    );
    assert!(
        rust_err.contains(&format!(
            "{}:3: Error: Improperly formatted checksum line.",
            cf
        )),
        "stderr should contain per-line warning for line 3"
    );
    // stdout still has OK and aggregate summary
    assert!(rust_out.contains(": OK"), "stdout should still have OK");
    assert!(
        rust_out.contains("2 lines are improperly formatted"),
        "stdout should still have aggregate summary"
    );
}

#[test]
fn cli_check_malformed_lines_warn_all_invalid() {
    skip_without_reference!();
    let dir = test_dir("warn_all_invalid");
    let checksum_file = dir.join("bad.xxh");
    fs::write(&checksum_file, "garbage 1\ngarbage 2\ngarbage 3\n").unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--warn", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--warn", cf]);

    assert_eq!(rust_code, 1, "all-invalid should exit 1");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // Per-line diagnostics + "no properly formatted" all go to stderr
    assert!(
        rust_err.contains(&format!(
            "{}:1: Error: Improperly formatted checksum line.",
            cf
        )),
        "stderr should contain per-line warning for line 1"
    );
    assert!(
        rust_err.contains(&format!(
            "{}:2: Error: Improperly formatted checksum line.",
            cf
        )),
        "stderr should contain per-line warning for line 2"
    );
    assert!(
        rust_err.contains(&format!(
            "{}:3: Error: Improperly formatted checksum line.",
            cf
        )),
        "stderr should contain per-line warning for line 3"
    );
    assert!(
        rust_err.contains("no properly formatted xxHash checksum lines found"),
        "stderr should contain no-valid-lines diagnostic"
    );
}

// =========================================================================
// VAL-CHECK-004: --strict mode — malformed lines are fatal
// =========================================================================

#[test]
fn cli_check_malformed_lines_strict_mixed() {
    skip_without_reference!();
    let dir = test_dir("strict_mixed");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let valid_line = generate_checksums_ref(&[path]);
    let content = format!("garbage line 1\n{}another garbage line\n", valid_line);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &content).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--strict", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--strict", cf]);

    assert_eq!(
        rust_code, 1,
        "strict mode with malformed lines should exit 1"
    );
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // stdout should still show OK for valid and aggregate summary
    assert!(rust_out.contains(": OK"), "valid entries should still verify");
    assert!(
        rust_out.contains("2 lines are improperly formatted"),
        "aggregate summary should still appear"
    );
    // No per-line diagnostics in stderr for --strict alone (only with --warn)
    assert!(
        rust_err.is_empty(),
        "strict without --warn should not emit per-line stderr diagnostics"
    );
}

#[test]
fn cli_check_malformed_lines_strict_no_malformed_succeeds() {
    skip_without_reference!();
    let dir = test_dir("strict_clean");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let valid_line = generate_checksums_ref(&[path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &valid_line).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--strict", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--strict", cf]);

    assert_eq!(rust_code, 0, "strict with no malformed should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_malformed_lines_strict_all_invalid() {
    skip_without_reference!();
    let dir = test_dir("strict_all_invalid");
    let checksum_file = dir.join("bad.xxh");
    fs::write(&checksum_file, "garbage 1\ngarbage 2\n").unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--strict", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--strict", cf]);

    assert_eq!(rust_code, 1, "all-invalid should exit 1");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    assert!(
        rust_err.contains("no properly formatted xxHash checksum lines found"),
        "stderr should contain no-valid-lines diagnostic"
    );
}

// =========================================================================
// VAL-CHECK-004: All-invalid with default mode
// =========================================================================

#[test]
fn cli_check_malformed_lines_default_all_invalid() {
    skip_without_reference!();
    let dir = test_dir("default_all_invalid");
    let checksum_file = dir.join("bad.xxh");
    fs::write(&checksum_file, "garbage 1\ngarbage 2\ngarbage 3\n").unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 1, "all-invalid should exit 1");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    assert!(
        rust_err.contains("no properly formatted xxHash checksum lines found"),
        "stderr should contain no-valid-lines diagnostic"
    );
    assert!(
        rust_out.is_empty(),
        "stdout should be empty for all-invalid"
    );
}

// =========================================================================
// VAL-CHECK-004: Empty checksum file
// =========================================================================

#[test]
fn cli_check_malformed_lines_empty_file() {
    skip_without_reference!();
    let dir = test_dir("empty_file");
    let checksum_file = dir.join("empty.xxh");
    fs::write(&checksum_file, "").unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 1, "empty file should exit 1");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    assert!(
        rust_err.contains("no properly formatted xxHash checksum lines found"),
        "stderr should contain no-valid-lines diagnostic"
    );
}

// =========================================================================
// VAL-CHECK-004: --warn + --strict combined
// =========================================================================

#[test]
fn cli_check_malformed_lines_warn_strict_combined() {
    skip_without_reference!();
    let dir = test_dir("warn_strict");
    let file = dir.join("test.txt");
    fs::write(&file, b"content\n").unwrap();
    let path = file.to_str().unwrap();

    let valid_line = generate_checksums_ref(&[path]);
    let content = format!("garbage\n{}", valid_line);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &content).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--warn", "--strict", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--warn", "--strict", cf]);

    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-004: --status with malformed lines (already tested elsewhere
// but verify parity for aggregate malformed summary suppression)
// =========================================================================

#[test]
fn cli_check_malformed_lines_status_mixed_malformed() {
    skip_without_reference!();
    let dir = test_dir("status_mixed_malformed");
    let file = dir.join("test.txt");
    fs::write(&file, b"test\n").unwrap();
    let path = file.to_str().unwrap();

    let valid_line = generate_checksums_ref(&[path]);
    let content = format!("garbage\n{}", valid_line);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &content).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--status", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--status", cf]);

    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}
