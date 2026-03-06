//! CLI --check mode: escaped filenames and little-endian verification.
//!
//! Validates:
//! - VAL-CHECK-006: Escaped filenames round-trip through checksum verification.
//! - VAL-CHECK-007: Little-endian checksum files verify with the
//!   reference-compatible rules.

use std::env;
use std::fs;
use std::io::Write;
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
        .join("xxhash_cli_esc_le_tests")
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
// VAL-CHECK-006: Escaped GNU filenames round-trip through --check
// =========================================================================

#[test]
fn cli_check_escaped_and_little_endian_gnu_backslash() {
    let dir = test_dir("gnu_backslash");
    let file = dir.join("back\\slash.txt");
    fs::write(&file, b"hello\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&[path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 0, "escaped backslash check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");

    // Verify the output has the escape prefix and escaped backslash in filename
    assert!(
        rust_out.starts_with('\\'),
        "escaped filename check output should start with backslash prefix: got '{}'",
        rust_out
    );
    assert!(
        rust_out.contains("back\\\\slash.txt: OK"),
        "should display escaped name with doubled backslash: got '{}'",
        rust_out
    );
}

#[test]
fn cli_check_escaped_and_little_endian_gnu_newline() {
    let dir = test_dir("gnu_newline");
    let name = "new\nline.txt";
    let file = dir.join(name);
    fs::write(&file, b"hello\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&[path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 0, "escaped newline check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_escaped_and_little_endian_gnu_cr() {
    let dir = test_dir("gnu_cr");
    let name = "cr\rret.txt";
    let file = dir.join(name);
    fs::write(&file, b"hello\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&[path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 0, "escaped CR check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-006: Escaped BSD filenames round-trip through --check
// =========================================================================

#[test]
fn cli_check_escaped_and_little_endian_bsd_backslash() {
    let dir = test_dir("bsd_backslash");
    let file = dir.join("back\\slash.txt");
    fs::write(&file, b"hello\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--tag", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 0, "escaped BSD backslash check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_escaped_and_little_endian_bsd_newline() {
    let dir = test_dir("bsd_newline");
    let name = "new\nline.txt";
    let file = dir.join(name);
    fs::write(&file, b"hello\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--tag", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 0, "escaped BSD newline check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_escaped_and_little_endian_bsd_cr() {
    let dir = test_dir("bsd_cr");
    let name = "cr\rret.txt";
    let file = dir.join(name);
    fs::write(&file, b"hello\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--tag", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 0, "escaped BSD CR check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-006: Multiple escaped filenames in one checksum file
// =========================================================================

#[test]
fn cli_check_escaped_and_little_endian_mixed_escaped_gnu() {
    let dir = test_dir("mixed_escaped_gnu");
    let file_bs = dir.join("back\\slash.txt");
    let file_nl = dir.join("new\nline.txt");
    let file_normal = dir.join("normal.txt");
    fs::write(&file_bs, b"content1\n").unwrap();
    fs::write(&file_nl, b"content2\n").unwrap();
    fs::write(&file_normal, b"content3\n").unwrap();

    let bs = file_bs.to_str().unwrap();
    let nl = file_nl.to_str().unwrap();
    let norm = file_normal.to_str().unwrap();

    let checksums = generate_checksums_ref(&[bs, nl, norm]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 0, "mixed escaped check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-007: GNU LE checksum files verify with --check --little-endian
// =========================================================================

#[test]
fn cli_check_escaped_and_little_endian_gnu_le_xxh64() {
    let dir = test_dir("gnu_le_xxh64");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--little-endian", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--little-endian", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--little-endian", cf]);

    assert_eq!(rust_code, 0, "GNU LE XXH64 check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_escaped_and_little_endian_gnu_le_xxh32() {
    let dir = test_dir("gnu_le_xxh32");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--little-endian", "-H0", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--little-endian", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--little-endian", cf]);

    assert_eq!(rust_code, 0, "GNU LE XXH32 check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_escaped_and_little_endian_gnu_le_xxh3_64() {
    let dir = test_dir("gnu_le_xxh3_64");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--little-endian", "-H3", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--little-endian", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--little-endian", cf]);

    assert_eq!(rust_code, 0, "GNU LE XXH3_64 check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_escaped_and_little_endian_gnu_le_xxh128() {
    let dir = test_dir("gnu_le_xxh128");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--little-endian", "-H2", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--little-endian", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--little-endian", cf]);

    assert_eq!(rust_code, 0, "GNU LE XXH128 check should succeed");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-007: GNU LE without --little-endian should fail
// =========================================================================

#[test]
fn cli_check_escaped_and_little_endian_gnu_le_without_flag_fails() {
    let dir = test_dir("gnu_le_no_flag");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--little-endian", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(rust_code, 1, "GNU LE without flag should fail");
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-007: BSD LE checksum files verify under --check without extra flag
// =========================================================================

#[test]
fn cli_check_escaped_and_little_endian_bsd_le_xxh64_no_flag() {
    let dir = test_dir("bsd_le_xxh64");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--tag", "--little-endian", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(
        rust_code, 0,
        "BSD LE XXH64 should verify without --little-endian flag"
    );
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_escaped_and_little_endian_bsd_le_xxh32_no_flag() {
    let dir = test_dir("bsd_le_xxh32");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--tag", "--little-endian", "-H0", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(
        rust_code, 0,
        "BSD LE XXH32 should verify without --little-endian flag"
    );
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_escaped_and_little_endian_bsd_le_xxh3_64_no_flag() {
    let dir = test_dir("bsd_le_xxh3_64");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--tag", "--little-endian", "-H3", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(
        rust_code, 0,
        "BSD LE XXH3_64 should verify without --little-endian flag"
    );
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

#[test]
fn cli_check_escaped_and_little_endian_bsd_le_xxh128_no_flag() {
    let dir = test_dir("bsd_le_xxh128");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--tag", "--little-endian", "-H2", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(
        rust_code, 0,
        "BSD LE XXH128 should verify without --little-endian flag"
    );
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-007: BSD LE also verifies under --check --little-endian (explicit)
// =========================================================================

#[test]
fn cli_check_escaped_and_little_endian_bsd_le_explicit_flag() {
    let dir = test_dir("bsd_le_explicit");
    let file = dir.join("test.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let path = file.to_str().unwrap();

    let checksums = generate_checksums_ref(&["--tag", "--little-endian", path]);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &checksums).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", "--little-endian", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", "--little-endian", cf]);

    assert_eq!(
        rust_code, 0,
        "BSD LE with explicit --little-endian should also work"
    );
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}

// =========================================================================
// VAL-CHECK-007: Multiple algorithms in one LE checksum file (BSD format)
// =========================================================================

#[test]
fn cli_check_escaped_and_little_endian_bsd_le_multi_algo() {
    let dir = test_dir("bsd_le_multi_algo");
    let file = dir.join("test.txt");
    fs::write(&file, b"multi algo test\n").unwrap();
    let path = file.to_str().unwrap();

    // Generate BSD LE lines for each algorithm
    let line32 = generate_checksums_ref(&["--tag", "--little-endian", "-H0", path]);
    let line64 = generate_checksums_ref(&["--tag", "--little-endian", "-H1", path]);
    let line3 = generate_checksums_ref(&["--tag", "--little-endian", "-H3", path]);
    let line128 = generate_checksums_ref(&["--tag", "--little-endian", "-H2", path]);

    let content = format!("{}{}{}{}", line32, line64, line3, line128);
    let checksum_file = dir.join("sums.xxh");
    fs::write(&checksum_file, &content).unwrap();
    let cf = checksum_file.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&["--check", cf]);
    let (ref_out, ref_err, ref_code) = run_ref(&["--check", cf]);

    assert_eq!(
        rust_code, 0,
        "BSD LE multi-algo check should succeed without flag"
    );
    assert_eq!(rust_code, ref_code);
    assert_eq!(rust_out, ref_out, "stdout should match reference");
    assert_eq!(rust_err, ref_err, "stderr should match reference");
}
