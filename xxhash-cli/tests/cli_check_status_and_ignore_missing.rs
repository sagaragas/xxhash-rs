//! CLI --check mode: --status and --ignore-missing tests.
//!
//! Validates:
//! - VAL-CHECK-003: --quiet and --status follow upstream-compatible semantics
//! - VAL-CHECK-005: --ignore-missing succeeds only when at least one file verified

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Returns the path to the Rust CLI binary built by cargo.
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

/// Run a CLI binary with the given args.
fn run_cli(bin: &PathBuf, args: &[&str]) -> (String, String, i32) {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd.output().expect("failed to run CLI");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

fn run_rust(args: &[&str]) -> (String, String, i32) {
    run_cli(&rust_binary(), args)
}

fn run_ref(args: &[&str]) -> (String, String, i32) {
    run_cli(
        &reference_binary().expect("reference binary not found"),
        args,
    )
}

fn test_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("xxhash_cli_check_tests")
        .join(test_name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create test dir");
    dir
}

fn generate_checksums_ref(args: &[&str]) -> String {
    let (stdout, _, code) = run_ref(args);
    assert_eq!(code, 0, "reference generation should succeed");
    stdout
}

// =========================================================================
// VAL-CHECK-003: --status success (no output)
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_status_success() {
    let dir = test_dir("status_success");
    let file = dir.join("test.txt");
    fs::write(&file, b"status test\n").unwrap();

    let path = file.to_str().unwrap();
    let checksums = generate_checksums_ref(&[path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--status", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--status", cf]);

    assert_eq!(rust_code, 0, "status check should succeed");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference (empty)");
    assert_eq!(rust_err, ref_err, "stderr should match reference (empty)");
    assert!(rust_out.is_empty(), "status success should have no stdout");
    assert!(rust_err.is_empty(), "status success should have no stderr");
}

// =========================================================================
// VAL-CHECK-003: --status mismatch (no output, exit 1)
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_status_mismatch() {
    let dir = test_dir("status_mismatch");
    let file = dir.join("test.txt");
    fs::write(&file, b"original\n").unwrap();

    let path = file.to_str().unwrap();
    let checksums = generate_checksums_ref(&[path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    fs::write(&file, b"changed\n").unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--status", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--status", cf]);

    assert_eq!(rust_code, 1);
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should be empty in status mismatch");
    assert_eq!(rust_err, ref_err, "stderr should be empty in status mismatch");
    assert!(rust_out.is_empty(), "status mode should suppress FAILED line and summary");
    assert!(rust_err.is_empty(), "status mismatch should have no stderr");
}

// =========================================================================
// VAL-CHECK-003: --status with mixed-missing (no output, exit 1)
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_status_mixed_missing() {
    let dir = test_dir("status_mixed_missing");
    let file_a = dir.join("a.txt");
    let file_b = dir.join("b.txt");
    fs::write(&file_a, b"aaa\n").unwrap();
    fs::write(&file_b, b"bbb\n").unwrap();

    let a = file_a.to_str().unwrap();
    let b = file_b.to_str().unwrap();

    let checksums = generate_checksums_ref(&[a, b]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    fs::remove_file(&file_a).unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--status", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--status", cf]);

    assert_eq!(rust_code, 1);
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should be empty in status mixed-missing");
    assert_eq!(rust_err, ref_err, "stderr should be empty in status mixed-missing");
}

// =========================================================================
// VAL-CHECK-003: --status with malformed lines (stderr diagnostic)
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_status_all_malformed() {
    let dir = test_dir("status_malformed");
    let checksum_file = dir.join("bad.xxh");
    fs::write(&checksum_file, "garbage line 1\ngarbage line 2\n").unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--status", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--status", cf]);

    assert_eq!(rust_code, 1, "all malformed should exit 1");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // "no properly formatted" should go to stderr even in --status mode
    assert!(
        rust_err.contains("no properly formatted xxHash checksum lines found"),
        "stderr should contain malformed diagnostic: got stderr='{}'",
        rust_err
    );
}

// =========================================================================
// VAL-CHECK-003: --status with mixed valid/malformed (silent, exit 0)
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_status_mixed_malformed() {
    let dir = test_dir("status_mixed_malformed");
    let file = dir.join("test.txt");
    fs::write(&file, b"test content\n").unwrap();

    let path = file.to_str().unwrap();
    let valid_line = generate_checksums_ref(&[path]);

    let checksum_file = dir.join("sums.xxh");
    let content = format!("{}bad line\n", valid_line);
    fs::write(&checksum_file, &content).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--status", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--status", cf]);

    assert_eq!(rust_code, 0, "mixed valid/malformed should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should be empty");
    assert_eq!(rust_err, ref_err, "stderr should be empty");
}

// =========================================================================
// VAL-CHECK-005: --ignore-missing with mixed present/missing
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_ignore_missing_mixed() {
    let dir = test_dir("ignore_missing_mixed");
    let file_a = dir.join("a.txt");
    let file_b = dir.join("b.txt");
    fs::write(&file_a, b"content_a\n").unwrap();
    fs::write(&file_b, b"content_b\n").unwrap();

    let a = file_a.to_str().unwrap();
    let b = file_b.to_str().unwrap();

    let checksums = generate_checksums_ref(&[a, b]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    // Remove first file
    fs::remove_file(&file_a).unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--ignore-missing", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--ignore-missing", cf]);

    assert_eq!(rust_code, 0, "should succeed when at least one file verified");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // Should show OK for the present file
    assert!(
        rust_out.contains(&format!("{}: OK", b)),
        "should show OK for present file"
    );

    // Should NOT show "Could not open or read" diagnostics
    assert!(
        !rust_out.contains("Could not open or read"),
        "should suppress missing-file diagnostics"
    );
}

// =========================================================================
// VAL-CHECK-005: --ignore-missing all missing → "no file was verified"
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_ignore_missing_all_missing() {
    let dir = test_dir("ignore_missing_all");
    let file_a = dir.join("a.txt");
    let file_b = dir.join("b.txt");
    fs::write(&file_a, b"content_a\n").unwrap();
    fs::write(&file_b, b"content_b\n").unwrap();

    let a = file_a.to_str().unwrap();
    let b = file_b.to_str().unwrap();

    let checksums = generate_checksums_ref(&[a, b]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    // Remove all files
    fs::remove_file(&file_a).unwrap();
    fs::remove_file(&file_b).unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--ignore-missing", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--ignore-missing", cf]);

    assert_eq!(rust_code, 1, "all-missing should exit 1");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // Should contain "no file was verified"
    assert!(
        rust_out.contains("no file was verified"),
        "should output 'no file was verified'"
    );
}

// =========================================================================
// VAL-CHECK-005: --ignore-missing with mismatch still fails
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_ignore_missing_with_mismatch() {
    let dir = test_dir("ignore_missing_mismatch");
    let file_a = dir.join("a.txt");
    let file_b = dir.join("b.txt");
    let file_c = dir.join("c.txt");
    fs::write(&file_a, b"aaa\n").unwrap();
    fs::write(&file_b, b"bbb\n").unwrap();
    fs::write(&file_c, b"ccc\n").unwrap();

    let a = file_a.to_str().unwrap();
    let b = file_b.to_str().unwrap();
    let c = file_c.to_str().unwrap();

    let checksums = generate_checksums_ref(&[a, b, c]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    // a: missing, b: OK, c: mismatch
    fs::remove_file(&file_a).unwrap();
    fs::write(&file_c, b"changed\n").unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--ignore-missing", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--ignore-missing", cf]);

    assert_eq!(rust_code, 1, "mismatch should still fail even with --ignore-missing");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-003: --status combined with --ignore-missing
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_status_with_ignore_missing_success() {
    let dir = test_dir("status_ignore_missing_success");
    let file_a = dir.join("a.txt");
    let file_b = dir.join("b.txt");
    fs::write(&file_a, b"aaa\n").unwrap();
    fs::write(&file_b, b"bbb\n").unwrap();

    let a = file_a.to_str().unwrap();
    let b = file_b.to_str().unwrap();

    let checksums = generate_checksums_ref(&[a, b]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    // Remove one file
    fs::remove_file(&file_a).unwrap();

    let (rust_out, rust_err, rust_code) =
        run_rust(&["--check", "--status", "--ignore-missing", cf]);
    let (ref_out, ref_err, ref_code) =
        run_ref(&["--check", "--status", "--ignore-missing", cf]);

    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// Additional parity edge cases
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_status_zero_verified() {
    // All missing (not malformed), with --status → no output, exit 1
    let dir = test_dir("status_zero_verified");
    let file = dir.join("test.txt");
    fs::write(&file, b"content\n").unwrap();

    let path = file.to_str().unwrap();
    let checksums = generate_checksums_ref(&[path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    fs::remove_file(&file).unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--status", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--status", cf]);

    assert_eq!(rust_code, 1);
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out);
    assert_eq!(rust_err, ref_err);
}

// =========================================================================
// Combined --status --ignore-missing all-missing → zero-verified diagnostic
// =========================================================================

#[test]
fn cli_check_status_and_ignore_missing_combined_all_missing_emits_diagnostic() {
    // --check --status --ignore-missing with all referenced files missing
    // must still emit the "no file was verified" diagnostic to stdout,
    // matching the reference CLI behavior for the zero-verified edge case.
    let dir = test_dir("combined_status_ignore_all_missing");
    let file_a = dir.join("a.txt");
    let file_b = dir.join("b.txt");
    fs::write(&file_a, b"content_a\n").unwrap();
    fs::write(&file_b, b"content_b\n").unwrap();

    let a = file_a.to_str().unwrap();
    let b = file_b.to_str().unwrap();

    let checksums = generate_checksums_ref(&[a, b]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    // Remove all referenced files
    fs::remove_file(&file_a).unwrap();
    fs::remove_file(&file_b).unwrap();

    let (rust_out, rust_err, rust_code) =
        run_rust(&["--check", "--status", "--ignore-missing", cf]);
    let (ref_out, ref_err, ref_code) =
        run_ref(&["--check", "--status", "--ignore-missing", cf]);

    assert_eq!(rust_code, 1, "all-missing should exit 1");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // The zero-verified diagnostic MUST appear even with --status
    assert!(
        rust_out.contains("no file was verified"),
        "should emit 'no file was verified' even with --status: got stdout='{}'",
        rust_out
    );
}
