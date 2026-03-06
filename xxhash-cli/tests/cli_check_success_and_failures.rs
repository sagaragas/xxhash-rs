//! CLI --check mode: success, mismatch, and unreadable file tests.
//!
//! Validates:
//! - VAL-CHECK-001: Basic --check success ignores comment lines
//! - VAL-CHECK-002: Mismatches and unreadable files fail with expected summaries

use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Returns the path to the Rust CLI binary built by cargo.
fn rust_binary() -> PathBuf {
    env!("CARGO_BIN_EXE_xxhash-rs").into()
}

/// Default path to the external reference checkout.
const DEFAULT_REFERENCE_ROOT: &str = "/Users/ragas/code/missions/xxhash-reference";

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
        .join("xxhash_cli_check_tests")
        .join(test_name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create test dir");
    dir
}

/// Generate a checksum file using the reference CLI.
fn generate_checksums_ref(args: &[&str]) -> String {
    let (stdout, _, code) = run_ref(args);
    assert_eq!(code, 0, "reference generation should succeed");
    stdout
}

// =========================================================================
// VAL-CHECK-001: Basic --check success ignores comment lines
// =========================================================================

#[test]
fn cli_check_success_and_failures_basic_success() {
    let dir = test_dir("basic_success");
    let file_a = dir.join("a.txt");
    let file_b = dir.join("b.txt");
    fs::write(&file_a, b"hello world\n").unwrap();
    fs::write(&file_b, b"another file\n").unwrap();

    let a = file_a.to_str().unwrap();
    let b = file_b.to_str().unwrap();

    // Generate checksum file using the reference CLI
    let checksums = generate_checksums_ref(&[a, b]);
    let checksum_file = dir.join("checksums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    // Verify with Rust CLI
    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);

    assert_eq!(rust_code, 0, "check should succeed, got stderr: {}", rust_err);
    assert!(rust_out.contains(": OK"), "stdout should contain OK lines");
    assert!(rust_err.is_empty(), "stderr should be empty on success");

    // Both files should be OK
    assert!(
        rust_out.contains(&format!("{}: OK", a)),
        "should show a.txt OK"
    );
    assert!(
        rust_out.contains(&format!("{}: OK", b)),
        "should show b.txt OK"
    );
}

#[test]
fn cli_check_success_and_failures_comment_lines_ignored() {
    let dir = test_dir("comment_lines");
    let file_a = dir.join("a.txt");
    let file_b = dir.join("b.txt");
    fs::write(&file_a, b"hello world\n").unwrap();
    fs::write(&file_b, b"another file\n").unwrap();

    let a = file_a.to_str().unwrap();
    let b = file_b.to_str().unwrap();

    // Generate checksum lines
    let line_a = generate_checksums_ref(&[a]);
    let line_b = generate_checksums_ref(&[b]);

    // Build checksum file with comments
    let checksum_content = format!(
        "# This is a comment\n{}# Another comment\n{}",
        line_a, line_b
    );
    let checksum_file = dir.join("checksums.xxh");
    fs::write(&checksum_file, &checksum_content).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);

    assert_eq!(rust_code, 0, "check with comments should succeed");
    assert!(rust_err.is_empty(), "stderr should be empty");
    assert!(
        rust_out.contains(&format!("{}: OK", a)),
        "should verify a.txt"
    );
    assert!(
        rust_out.contains(&format!("{}: OK", b)),
        "should verify b.txt"
    );

    // Comment lines themselves should NOT appear as verification entries
    assert!(
        !rust_out.contains("# This is a comment"),
        "comment lines should not appear as verification entries"
    );
    assert!(
        !rust_out.contains("# Another comment"),
        "comment lines should not appear as verification entries"
    );
}

#[test]
fn cli_check_success_and_failures_parity_with_reference() {
    let dir = test_dir("parity_success");
    let file = dir.join("test.txt");
    fs::write(&file, b"parity test content\n").unwrap();

    let path = file.to_str().unwrap();
    let checksums = generate_checksums_ref(&[path]);
    let checksum_file = dir.join("checksums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-001: Tagged (BSD) format checksums verify
// =========================================================================

#[test]
fn cli_check_success_and_failures_tagged_format() {
    let dir = test_dir("tagged_format");
    let file = dir.join("tag.txt");
    fs::write(&file, b"tagged test\n").unwrap();

    let path = file.to_str().unwrap();
    let checksums = generate_checksums_ref(&["--tag", path]);
    let checksum_file = dir.join("checksums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_success_and_failures_tagged_xxh32() {
    let dir = test_dir("tagged_xxh32");
    let file = dir.join("f.txt");
    fs::write(&file, b"xxh32 tagged\n").unwrap();

    let path = file.to_str().unwrap();
    let checksums = generate_checksums_ref(&["--tag", "-H0", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, _, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, _, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out);
}

#[test]
fn cli_check_success_and_failures_tagged_xxh3() {
    let dir = test_dir("tagged_xxh3");
    let file = dir.join("f.txt");
    fs::write(&file, b"xxh3 tagged\n").unwrap();

    let path = file.to_str().unwrap();
    let checksums = generate_checksums_ref(&["--tag", "-H3", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, _, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, _, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out);
}

#[test]
fn cli_check_success_and_failures_tagged_xxh128() {
    let dir = test_dir("tagged_xxh128");
    let file = dir.join("f.txt");
    fs::write(&file, b"xxh128 tagged\n").unwrap();

    let path = file.to_str().unwrap();
    let checksums = generate_checksums_ref(&["--tag", "-H2", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, _, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, _, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out);
}

// =========================================================================
// VAL-CHECK-002: Mismatches fail with expected summaries
// =========================================================================

#[test]
fn cli_check_success_and_failures_single_mismatch() {
    let dir = test_dir("single_mismatch");
    let file = dir.join("test.txt");
    fs::write(&file, b"original content\n").unwrap();

    let path = file.to_str().unwrap();
    let checksums = generate_checksums_ref(&[path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    // Modify the file to create a mismatch
    fs::write(&file, b"changed content\n").unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 1, "mismatch should exit 1");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    assert!(
        rust_out.contains(&format!("{}: FAILED", path)),
        "should contain FAILED line"
    );
    assert!(
        rust_out.contains("1 computed checksum did NOT match"),
        "should contain singular mismatch summary"
    );
}

#[test]
fn cli_check_success_and_failures_multiple_mismatches() {
    let dir = test_dir("multi_mismatch");
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

    // Modify a and c
    fs::write(&file_a, b"changed_a\n").unwrap();
    fs::write(&file_c, b"changed_c\n").unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 1);
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // b should still be OK
    assert!(rust_out.contains(&format!("{}: OK", b)));
    // a and c should be FAILED
    assert!(rust_out.contains(&format!("{}: FAILED", a)));
    assert!(rust_out.contains(&format!("{}: FAILED", c)));
    assert!(rust_out.contains("2 computed checksums did NOT match"));
}

#[test]
fn cli_check_success_and_failures_mismatch_preserves_ok() {
    let dir = test_dir("mismatch_preserves_ok");
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

    // Modify only first file
    fs::write(&file_a, b"changed\n").unwrap();

    let (rust_out, _, rust_code) = run_rust(&["--check", cf]);

    assert_eq!(rust_code, 1);
    assert!(
        rust_out.contains(&format!("{}: FAILED", a)),
        "mismatch file should show FAILED"
    );
    assert!(
        rust_out.contains(&format!("{}: OK", b)),
        "matching file should still show OK"
    );
}

// =========================================================================
// VAL-CHECK-002: Unreadable files fail with expected summaries
// =========================================================================

#[test]
fn cli_check_success_and_failures_missing_file() {
    let dir = test_dir("missing_file");
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

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 1, "should exit 1 for missing file");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // Should mention Could not open or read
    assert!(
        rust_out.contains("Could not open or read"),
        "should contain open-read error"
    );
    assert!(
        rust_out.contains("No such file or directory"),
        "should contain OS error"
    );
    // b should still be OK
    assert!(
        rust_out.contains(&format!("{}: OK", b)),
        "good file should still show OK"
    );
    assert!(
        rust_out.contains("1 listed file could not be read"),
        "should contain file-read summary"
    );
}

#[test]
fn cli_check_success_and_failures_unreadable_file() {
    let dir = test_dir("unreadable_file");
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

    // Make first file unreadable
    fs::set_permissions(&file_a, fs::Permissions::from_mode(0o000)).unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    // Restore permissions for cleanup
    fs::set_permissions(&file_a, fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(rust_code, 1, "should exit 1 for unreadable file");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    assert!(
        rust_out.contains("Could not open or read"),
        "should contain open-read error"
    );
    assert!(
        rust_out.contains("Permission denied"),
        "should contain permission error"
    );
    assert!(
        rust_out.contains(&format!("{}: OK", b)),
        "good file should still show OK"
    );
}

#[test]
fn cli_check_success_and_failures_multiple_unreadable() {
    let dir = test_dir("multi_unreadable");
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

    // Remove both files
    fs::remove_file(&file_a).unwrap();
    fs::remove_file(&file_b).unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 1);
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    assert!(
        rust_out.contains("2 listed files could not be read"),
        "should use plural for multiple unreadable files"
    );
}

#[test]
fn cli_check_success_and_failures_mixed_mismatch_and_missing() {
    let dir = test_dir("mixed_mismatch_missing");
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

    // a: OK, b: missing, c: mismatch
    fs::remove_file(&file_b).unwrap();
    fs::write(&file_c, b"changed\n").unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 1);
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-002: --quiet suppresses only OK lines
// =========================================================================

#[test]
fn cli_check_success_and_failures_quiet_success() {
    let dir = test_dir("quiet_success");
    let file = dir.join("test.txt");
    fs::write(&file, b"quiet test\n").unwrap();

    let path = file.to_str().unwrap();
    let checksums = generate_checksums_ref(&[path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--quiet", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--quiet", cf]);

    assert_eq!(rust_code, 0, "quiet check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should be empty in quiet success mode");
    assert_eq!(rust_err, ref_err);
    assert!(rust_out.is_empty(), "quiet success should produce no stdout");
}

#[test]
fn cli_check_success_and_failures_quiet_mismatch() {
    let dir = test_dir("quiet_mismatch");
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

    // Mismatch first file
    fs::write(&file_a, b"changed\n").unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--quiet", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--quiet", cf]);

    assert_eq!(rust_code, 1);
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // FAILED should still appear, OK should not
    assert!(
        rust_out.contains(&format!("{}: FAILED", a)),
        "FAILED should appear in quiet mode"
    );
    assert!(
        !rust_out.contains(": OK"),
        "OK should be suppressed in quiet mode"
    );
    assert!(
        rust_out.contains("1 computed checksum did NOT match"),
        "summary should still appear"
    );
}

#[test]
fn cli_check_success_and_failures_quiet_missing_and_mismatch() {
    let dir = test_dir("quiet_mixed");
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

    // a: OK (suppressed by quiet), b: missing, c: mismatch
    fs::remove_file(&file_b).unwrap();
    fs::write(&file_c, b"changed\n").unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--quiet", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--quiet", cf]);

    assert_eq!(rust_code, 1);
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// Stdin-default: --check with no FILE reads stdin
// =========================================================================

fn run_rust_stdin(args: &[&str], stdin_data: &[u8]) -> (String, String, i32) {
    run_cli(&rust_binary(), args, Some(stdin_data))
}

fn run_ref_stdin(args: &[&str], stdin_data: &[u8]) -> (String, String, i32) {
    run_cli(
        &reference_binary().expect("reference binary not found"),
        args,
        Some(stdin_data),
    )
}

#[test]
fn cli_check_success_and_failures_stdin_default_empty() {
    // --check with no FILE and empty stdin → "stdin: no properly formatted..."
    let (rust_out, rust_err, rust_code) = run_rust_stdin(&["--check"], b"");
    let (ref_out, ref_err, ref_code) = run_ref_stdin(&["--check"], b"");

    assert_eq!(rust_code, 1, "empty stdin check should exit 1");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
    assert!(
        rust_err.contains("stdin: no properly formatted xxHash checksum lines found"),
        "stderr should contain stdin diagnostic: got stderr='{}'",
        rust_err
    );
}

#[test]
fn cli_check_success_and_failures_stdin_default_garbage() {
    // --check with no FILE and garbage stdin → "stdin: no properly formatted..."
    let (rust_out, rust_err, rust_code) = run_rust_stdin(&["--check"], b"garbage line\n");
    let (ref_out, ref_err, ref_code) = run_ref_stdin(&["--check"], b"garbage line\n");

    assert_eq!(rust_code, 1, "garbage stdin check should exit 1");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_success_and_failures_stdin_default_valid() {
    // --check with no FILE and valid checksum on stdin → verifies successfully
    let dir = test_dir("stdin_valid");
    let file = dir.join("test.txt");
    fs::write(&file, b"stdin check test\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&[path]);
    let checksum_bytes = checksums.as_bytes();

    let (rust_out, rust_err, rust_code) = run_rust_stdin(&["--check"], checksum_bytes);
    let (ref_out, ref_err, ref_code) = run_ref_stdin(&["--check"], checksum_bytes);

    assert_eq!(rust_code, 0, "valid stdin check should succeed");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
    assert!(
        rust_out.contains(": OK"),
        "should show OK for verified file"
    );
}

// =========================================================================
// Checksum-file read failure diagnostics
// =========================================================================

#[test]
fn cli_check_success_and_failures_nonexistent_checksum_file() {
    // --check on a non-existent file surfaces an explicit diagnostic
    let path = "/tmp/xxhash_nonexistent_checksum_file_ZZZZ.xxh";
    // Make sure it doesn't exist
    let _ = fs::remove_file(path);

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", path]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", path]);

    assert_eq!(rust_code, 1, "non-existent checksum file should exit 1");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
    assert!(
        rust_err.contains("Could not open"),
        "stderr should contain open-error diagnostic"
    );
    assert!(
        rust_err.contains("No such file or directory"),
        "stderr should contain OS error description"
    );
}

#[test]
fn cli_check_success_and_failures_unreadable_checksum_file() {
    // --check on an unreadable checksum file surfaces a permission diagnostic
    let dir = test_dir("unreadable_checksum");
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, "some content\n").unwrap();
    fs::set_permissions(&checksum_file, fs::Permissions::from_mode(0o000)).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    // Restore permissions for cleanup
    fs::set_permissions(&checksum_file, fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(rust_code, 1, "unreadable checksum file should exit 1");
    assert_eq!(rust_code, ref_code, "exit codes should match");
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
    assert!(
        rust_err.contains("Permission denied"),
        "stderr should contain permission error"
    );
}
