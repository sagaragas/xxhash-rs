//! CLI input flow parity tests.
//!
//! Validates VAL-HASH-004: Named files preserve argument order; piping data
//! with no filenames hashes stdin; explicit `-` forces stdin; readable inputs
//! still produce output even if another input is unreadable; and the command
//! exits non-zero when any requested input fails.

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
fn run_cli(
    bin: &PathBuf,
    args: &[&str],
    stdin_data: Option<&[u8]>,
) -> (String, String, i32) {
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

/// Run the Rust CLI.
fn run_rust(args: &[&str], stdin_data: Option<&[u8]>) -> (String, String, i32) {
    run_cli(&rust_binary(), args, stdin_data)
}

/// Run the reference CLI.
fn run_ref(args: &[&str], stdin_data: Option<&[u8]>) -> (String, String, i32) {
    run_cli(
        &reference_binary().expect("reference binary not found"),
        args,
        stdin_data,
    )
}

/// Extract all digest lines from output (format: "hash  filename").
fn extract_output_lines(output: &str) -> Vec<&str> {
    output.lines().collect()
}

/// Extract just the filename portion from an output line.
fn extract_filename(line: &str) -> &str {
    // Format is "hash  filename" with two spaces
    if let Some(pos) = line.find("  ") {
        &line[pos + 2..]
    } else {
        line
    }
}

/// Create a temporary test file with the given content and return its path.
fn create_temp_file(name: &str, content: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join("xxhash_cli_tests");
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    let path = dir.join(name);
    fs::write(&path, content).expect("failed to write temp file");
    path
}

/// Clean up temp files.
#[allow(dead_code)]
fn cleanup_temp_dir() {
    let dir = std::env::temp_dir().join("xxhash_cli_tests");
    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// Named files preserve argument order
// =========================================================================

#[test]
fn cli_input_flow_parity_file_order_preserved() {
    skip_without_reference!();
    let file_a = create_temp_file("order_a.txt", b"content A\n");
    let file_b = create_temp_file("order_b.txt", b"content B\n");
    let file_c = create_temp_file("order_c.txt", b"content C\n");

    let a = file_a.to_str().unwrap();
    let b = file_b.to_str().unwrap();
    let c = file_c.to_str().unwrap();

    // Hash in order B, A, C
    let (rust_out, _, rust_code) = run_rust(&[b, a, c], None);

    assert_eq!(rust_code, 0, "Should exit 0 for valid files");

    let lines = extract_output_lines(&rust_out);
    assert_eq!(lines.len(), 3, "Should produce 3 output lines");

    // Verify order is preserved
    assert_eq!(extract_filename(lines[0]), b, "First line should be file B");
    assert_eq!(
        extract_filename(lines[1]),
        a,
        "Second line should be file A"
    );
    assert_eq!(extract_filename(lines[2]), c, "Third line should be file C");

    // Compare with reference
    let (ref_out, _, _) = run_ref(&[b, a, c], None);
    assert_eq!(
        rust_out, ref_out,
        "File order output should match reference"
    );
}

#[test]
fn cli_input_flow_parity_single_file_matches_reference() {
    skip_without_reference!();
    let file = create_temp_file("single.txt", b"single file content\n");
    let path = file.to_str().unwrap();

    let (rust_out, _, rust_code) = run_rust(&[path], None);
    let (ref_out, _, ref_code) = run_ref(&[path], None);

    assert_eq!(rust_code, 0);
    assert_eq!(ref_code, 0);
    assert_eq!(rust_out, ref_out, "Single file output should match");
}

// =========================================================================
// No-file mode hashes stdin
// =========================================================================

#[test]
fn cli_input_flow_parity_no_files_reads_stdin() {
    skip_without_reference!();
    let data = b"stdin test data\n";

    let (rust_out, _, rust_code) = run_rust(&[], Some(data));
    let (ref_out, _, ref_code) = run_ref(&[], Some(data));

    assert_eq!(rust_code, 0);
    assert_eq!(ref_code, 0);
    assert_eq!(
        rust_out, ref_out,
        "No-file mode should hash stdin and match reference"
    );

    // Verify the filename field shows "stdin"
    let line = rust_out.lines().next().expect("should have output");
    assert_eq!(
        extract_filename(line),
        "stdin",
        "Filename should be 'stdin'"
    );
}

#[test]
fn cli_input_flow_parity_empty_stdin() {
    skip_without_reference!();
    let data = b"";

    let (rust_out, _, rust_code) = run_rust(&[], Some(data));
    let (ref_out, _, ref_code) = run_ref(&[], Some(data));

    assert_eq!(rust_code, 0);
    assert_eq!(ref_code, 0);
    assert_eq!(
        rust_out, ref_out,
        "Empty stdin should produce matching output"
    );
}

// =========================================================================
// Explicit `-` forces stdin
// =========================================================================

#[test]
fn cli_input_flow_parity_explicit_dash_reads_stdin() {
    skip_without_reference!();
    let data = b"explicit stdin\n";

    let (rust_out, _, rust_code) = run_rust(&["-"], Some(data));
    let (ref_out, _, ref_code) = run_ref(&["-"], Some(data));

    assert_eq!(rust_code, 0);
    assert_eq!(ref_code, 0);
    assert_eq!(
        rust_out, ref_out,
        "Explicit - should read stdin and match reference"
    );
}

#[test]
fn cli_input_flow_parity_dash_shows_stdin_as_name() {
    let data = b"test\n";
    let (rust_out, _, _) = run_rust(&["-"], Some(data));

    let line = rust_out.lines().next().expect("should have output");
    assert_eq!(
        extract_filename(line),
        "stdin",
        "Explicit - should show 'stdin' as filename"
    );
}

// =========================================================================
// Readable inputs still produce output when other inputs fail
// =========================================================================

#[test]
fn cli_input_flow_parity_good_then_bad_then_good() {
    skip_without_reference!();
    let good1 = create_temp_file("good1.txt", b"good one\n");
    let good2 = create_temp_file("good2.txt", b"good two\n");
    let bad = "/tmp/xxhash_cli_tests/nonexistent_file_12345.txt";

    let g1 = good1.to_str().unwrap();
    let g2 = good2.to_str().unwrap();

    let (rust_out, rust_err, rust_code) = run_rust(&[g1, bad, g2], None);
    let (ref_out, _ref_err, ref_code) = run_ref(&[g1, bad, g2], None);

    // Exit code should be non-zero
    assert_eq!(
        rust_code, 1,
        "Should exit 1 when any input fails (got {})",
        rust_code
    );
    assert_eq!(ref_code, 1, "Reference should also exit 1");

    // Stdout should contain the two good files in order
    let rust_lines = extract_output_lines(&rust_out);
    let ref_lines = extract_output_lines(&ref_out);

    assert_eq!(
        rust_lines.len(),
        2,
        "Should have 2 stdout lines (good files)"
    );
    assert_eq!(ref_lines.len(), 2, "Reference should have 2 stdout lines");

    assert_eq!(
        extract_filename(rust_lines[0]),
        g1,
        "First good file should be first in output"
    );
    assert_eq!(
        extract_filename(rust_lines[1]),
        g2,
        "Second good file should be second in output"
    );

    // Stdout content should match reference
    assert_eq!(
        rust_out, ref_out,
        "Stdout should match reference for good files"
    );

    // Stderr should mention the bad file
    assert!(
        rust_err.contains(bad),
        "Stderr should mention the bad file path"
    );
    assert!(
        rust_err.contains("Could not open"),
        "Stderr should contain 'Could not open'"
    );
}

#[test]
fn cli_input_flow_parity_all_bad_files() {
    let bad1 = "/tmp/xxhash_cli_tests/no_such_1.txt";
    let bad2 = "/tmp/xxhash_cli_tests/no_such_2.txt";

    let (rust_out, rust_err, rust_code) = run_rust(&[bad1, bad2], None);

    assert_eq!(rust_code, 1, "Should exit 1 when all inputs fail");
    assert!(
        rust_out.is_empty(),
        "No stdout when all files fail: '{}'",
        rust_out
    );
    assert!(!rust_err.is_empty(), "Stderr should contain error messages");
    assert!(
        rust_err.contains(bad1),
        "Stderr should mention first bad file"
    );
    assert!(
        rust_err.contains(bad2),
        "Stderr should mention second bad file"
    );
}

#[test]
fn cli_input_flow_parity_single_bad_file() {
    skip_without_reference!();
    let bad = "/tmp/xxhash_cli_tests/totally_missing.txt";

    let (rust_out, rust_err, rust_code) = run_rust(&[bad], None);
    let (ref_out, ref_err, ref_code) = run_ref(&[bad], None);

    assert_eq!(rust_code, 1);
    assert_eq!(ref_code, 1);
    assert!(rust_out.is_empty(), "No stdout for missing file");
    assert!(ref_out.is_empty(), "Reference: no stdout for missing file");

    // Both should report the error
    assert!(
        rust_err.contains("Could not open"),
        "Rust stderr: {}",
        rust_err
    );
    assert!(
        ref_err.contains("Could not open"),
        "Ref stderr: {}",
        ref_err
    );
}

// =========================================================================
// File hashing matches reference for various algorithms
// =========================================================================

#[test]
fn cli_input_flow_parity_file_hash_all_algorithms() {
    skip_without_reference!();
    let file = create_temp_file("algo_test.txt", b"algorithm test content\n");
    let path = file.to_str().unwrap();

    for flag in &["-H0", "-H1", "-H2", "-H3"] {
        let (rust_out, _, rust_code) = run_rust(&[flag, path], None);
        let (ref_out, _, ref_code) = run_ref(&[flag, path], None);

        assert_eq!(rust_code, 0, "{} should exit 0", flag);
        assert_eq!(ref_code, 0, "Reference {} should exit 0", flag);
        assert_eq!(
            rust_out, ref_out,
            "File hash with {} should match reference",
            flag
        );
    }
}

#[test]
fn cli_input_flow_parity_file_hash_with_seed() {
    skip_without_reference!();
    let file = create_temp_file("seed_test.txt", b"seeded file content\n");
    let path = file.to_str().unwrap();

    let (rust_out, _, _) = run_rust(&["--seed", "123", path], None);
    let (ref_out, _, _) = run_ref(&["--seed", "123", path], None);

    assert_eq!(
        rust_out, ref_out,
        "Seeded file hash should match reference"
    );
}

// =========================================================================
// Multiple files with different algorithms
// =========================================================================

#[test]
fn cli_input_flow_parity_multiple_files_xxh32() {
    skip_without_reference!();
    let f1 = create_temp_file("multi1.txt", b"file one\n");
    let f2 = create_temp_file("multi2.txt", b"file two\n");

    let p1 = f1.to_str().unwrap();
    let p2 = f2.to_str().unwrap();

    let (rust_out, _, rust_code) = run_rust(&["-H0", p1, p2], None);
    let (ref_out, _, ref_code) = run_ref(&["-H0", p1, p2], None);

    assert_eq!(rust_code, 0);
    assert_eq!(ref_code, 0);
    assert_eq!(
        rust_out, ref_out,
        "Multiple files with -H0 should match reference"
    );
}

// =========================================================================
// Stdin with algorithms and seeds
// =========================================================================

#[test]
fn cli_input_flow_parity_stdin_xxh3_64() {
    skip_without_reference!();
    let data = b"xxh3 stdin test\n";
    let (rust_out, _, _) = run_rust(&["-H3"], Some(data));
    let (ref_out, _, _) = run_ref(&["-H3"], Some(data));

    assert_eq!(
        rust_out, ref_out,
        "XXH3_64 stdin should match reference"
    );
}

#[test]
fn cli_input_flow_parity_stdin_xxh3_128_seeded() {
    skip_without_reference!();
    let data = b"seeded 128 stdin\n";
    let (rust_out, _, _) = run_rust(&["-H2", "--seed", "99"], Some(data));
    let (ref_out, _, _) = run_ref(&["-H2", "--seed", "99"], Some(data));

    assert_eq!(
        rust_out, ref_out,
        "XXH3_128 seeded stdin should match reference"
    );
}

// =========================================================================
// Edge case: large file
// =========================================================================

#[test]
fn cli_input_flow_parity_large_file() {
    skip_without_reference!();
    // Create a file larger than the typical read buffer (64KB)
    let data: Vec<u8> = (0..100_000u32)
        .flat_map(|i| format!("line {}\n", i).into_bytes())
        .collect();

    let file = create_temp_file("large.txt", &data);
    let path = file.to_str().unwrap();

    let (rust_out, _, rust_code) = run_rust(&[path], None);
    let (ref_out, _, ref_code) = run_ref(&[path], None);

    assert_eq!(rust_code, 0);
    assert_eq!(ref_code, 0);
    assert_eq!(
        rust_out, ref_out,
        "Large file hash should match reference"
    );
}

// =========================================================================
// Edge case: file with no trailing newline
// =========================================================================

#[test]
fn cli_input_flow_parity_no_trailing_newline() {
    skip_without_reference!();
    let file = create_temp_file("no_newline.txt", b"no newline here");
    let path = file.to_str().unwrap();

    let (rust_out, _, _) = run_rust(&[path], None);
    let (ref_out, _, _) = run_ref(&[path], None);

    assert_eq!(
        rust_out, ref_out,
        "File without trailing newline should match reference"
    );
}
