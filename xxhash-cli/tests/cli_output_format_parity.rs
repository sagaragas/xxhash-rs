//! CLI output format parity tests.
//!
//! Validates VAL-HASH-005: Default GNU output, --tag, --little-endian, the
//! combined --tag --little-endian mode, and filenames containing backslash,
//! newline, or carriage return are emitted with reference-compatible textual
//! formatting, including the expected little-endian `_LE` algorithm token or
//! suffix where applicable.

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

/// Create a temporary test file with the given content and return its path.
fn create_temp_file(name: &str, content: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join("xxhash_cli_fmt_tests");
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    let path = dir.join(name);
    fs::write(&path, content).expect("failed to write temp file");
    path
}

// =========================================================================
// Default GNU output matches reference for all algorithms
// =========================================================================

#[test]
fn cli_output_format_parity_gnu_default_xxh64_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&[], Some(data));
    let (ref_out, _, _) = run_ref(&[], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "GNU default XXH64 stdin mismatch");
}

#[test]
fn cli_output_format_parity_gnu_xxh32_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["-H0"], Some(data));
    let (ref_out, _, _) = run_ref(&["-H0"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "GNU XXH32 stdin mismatch");
}

#[test]
fn cli_output_format_parity_gnu_xxh3_64_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["-H3"], Some(data));
    let (ref_out, _, _) = run_ref(&["-H3"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "GNU XXH3_64 stdin mismatch");
}

#[test]
fn cli_output_format_parity_gnu_xxh3_128_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["-H2"], Some(data));
    let (ref_out, _, _) = run_ref(&["-H2"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "GNU XXH3_128 stdin mismatch");
}

#[test]
fn cli_output_format_parity_gnu_file() {
    let file = create_temp_file("gnu_file.txt", b"test content\n");
    let path = file.to_str().unwrap();
    let (rust_out, _, rc) = run_rust(&[path], None);
    let (ref_out, _, _) = run_ref(&[path], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "GNU file output mismatch");
}

// =========================================================================
// --tag (BSD-style) output matches reference for all algorithms
// =========================================================================

#[test]
fn cli_output_format_parity_tag_xxh64_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--tag"], Some(data));
    let (ref_out, _, _) = run_ref(&["--tag"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Tag XXH64 stdin mismatch");
}

#[test]
fn cli_output_format_parity_tag_xxh32_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--tag", "-H0"], Some(data));
    let (ref_out, _, _) = run_ref(&["--tag", "-H0"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Tag XXH32 stdin mismatch");
}

#[test]
fn cli_output_format_parity_tag_xxh3_64_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--tag", "-H3"], Some(data));
    let (ref_out, _, _) = run_ref(&["--tag", "-H3"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Tag XXH3_64 stdin mismatch");
}

#[test]
fn cli_output_format_parity_tag_xxh3_128_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--tag", "-H2"], Some(data));
    let (ref_out, _, _) = run_ref(&["--tag", "-H2"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Tag XXH3_128 stdin mismatch");
}

#[test]
fn cli_output_format_parity_tag_file() {
    let file = create_temp_file("tag_file.txt", b"tagged output\n");
    let path = file.to_str().unwrap();
    let (rust_out, _, rc) = run_rust(&["--tag", path], None);
    let (ref_out, _, _) = run_ref(&["--tag", path], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Tag file output mismatch");
}

// =========================================================================
// --little-endian output matches reference for all algorithms
// =========================================================================

#[test]
fn cli_output_format_parity_le_xxh64_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--little-endian"], Some(data));
    let (ref_out, _, _) = run_ref(&["--little-endian"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "LE XXH64 stdin mismatch");
}

#[test]
fn cli_output_format_parity_le_xxh32_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--little-endian", "-H0"], Some(data));
    let (ref_out, _, _) = run_ref(&["--little-endian", "-H0"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "LE XXH32 stdin mismatch");
}

#[test]
fn cli_output_format_parity_le_xxh3_64_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--little-endian", "-H3"], Some(data));
    let (ref_out, _, _) = run_ref(&["--little-endian", "-H3"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "LE XXH3_64 stdin mismatch");
}

#[test]
fn cli_output_format_parity_le_xxh3_128_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--little-endian", "-H2"], Some(data));
    let (ref_out, _, _) = run_ref(&["--little-endian", "-H2"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "LE XXH3_128 stdin mismatch");
}

#[test]
fn cli_output_format_parity_le_file() {
    let file = create_temp_file("le_file.txt", b"little endian test\n");
    let path = file.to_str().unwrap();
    let (rust_out, _, rc) = run_rust(&["--little-endian", path], None);
    let (ref_out, _, _) = run_ref(&["--little-endian", path], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "LE file output mismatch");
}

// =========================================================================
// --tag --little-endian combined output matches reference for all algorithms
// =========================================================================

#[test]
fn cli_output_format_parity_tag_le_xxh64_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--tag", "--little-endian"], Some(data));
    let (ref_out, _, _) = run_ref(&["--tag", "--little-endian"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Tag+LE XXH64 stdin mismatch");
}

#[test]
fn cli_output_format_parity_tag_le_xxh32_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--tag", "--little-endian", "-H0"], Some(data));
    let (ref_out, _, _) = run_ref(&["--tag", "--little-endian", "-H0"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Tag+LE XXH32 stdin mismatch");
}

#[test]
fn cli_output_format_parity_tag_le_xxh3_64_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--tag", "--little-endian", "-H3"], Some(data));
    let (ref_out, _, _) = run_ref(&["--tag", "--little-endian", "-H3"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Tag+LE XXH3_64 stdin mismatch");
}

#[test]
fn cli_output_format_parity_tag_le_xxh3_128_stdin() {
    let data = b"hello\n";
    let (rust_out, _, rc) = run_rust(&["--tag", "--little-endian", "-H2"], Some(data));
    let (ref_out, _, _) = run_ref(&["--tag", "--little-endian", "-H2"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Tag+LE XXH3_128 stdin mismatch");
}

#[test]
fn cli_output_format_parity_tag_le_file() {
    let file = create_temp_file("tagle_file.txt", b"tagged little endian\n");
    let path = file.to_str().unwrap();
    let (rust_out, _, rc) = run_rust(&["--tag", "--little-endian", path], None);
    let (ref_out, _, _) = run_ref(&["--tag", "--little-endian", path], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Tag+LE file output mismatch");
}

// =========================================================================
// Escaped filenames: backslash
// =========================================================================

#[test]
fn cli_output_format_parity_escaped_backslash_gnu() {
    let file = create_temp_file("back\\slash.txt", b"hello\n");
    let path = file.to_str().unwrap();
    let (rust_out, _, rc) = run_rust(&[path], None);
    let (ref_out, _, _) = run_ref(&[path], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Escaped backslash GNU mismatch");
    // Verify the escape prefix is present
    assert!(
        rust_out.starts_with('\\'),
        "Escaped filename line should start with backslash prefix"
    );
}

#[test]
fn cli_output_format_parity_escaped_backslash_tag() {
    let file = create_temp_file("back\\slash.txt", b"hello\n");
    let path = file.to_str().unwrap();
    let (rust_out, _, rc) = run_rust(&["--tag", path], None);
    let (ref_out, _, _) = run_ref(&["--tag", path], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Escaped backslash Tag mismatch");
}

#[test]
fn cli_output_format_parity_escaped_backslash_le() {
    let file = create_temp_file("back\\slash.txt", b"hello\n");
    let path = file.to_str().unwrap();
    let (rust_out, _, rc) = run_rust(&["--little-endian", path], None);
    let (ref_out, _, _) = run_ref(&["--little-endian", path], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Escaped backslash LE mismatch");
}

#[test]
fn cli_output_format_parity_escaped_backslash_tag_le() {
    let file = create_temp_file("back\\slash.txt", b"hello\n");
    let path = file.to_str().unwrap();
    let (rust_out, _, rc) = run_rust(&["--tag", "--little-endian", path], None);
    let (ref_out, _, _) = run_ref(&["--tag", "--little-endian", path], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Escaped backslash Tag+LE mismatch");
}

// =========================================================================
// Escaped filenames: newline
// =========================================================================

#[test]
fn cli_output_format_parity_escaped_newline_gnu() {
    let dir = std::env::temp_dir().join("xxhash_cli_fmt_tests");
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    let name = "new\nline.txt";
    let path = dir.join(name);
    fs::write(&path, b"hello\n").expect("failed to write");
    let path_str = path.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&[path_str], None);
    let (ref_out, _, _) = run_ref(&[path_str], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Escaped newline GNU mismatch");
    assert!(
        rust_out.starts_with('\\'),
        "Escaped filename line should start with backslash prefix"
    );
}

#[test]
fn cli_output_format_parity_escaped_newline_tag() {
    let dir = std::env::temp_dir().join("xxhash_cli_fmt_tests");
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    let name = "new\nline.txt";
    let path = dir.join(name);
    fs::write(&path, b"hello\n").expect("failed to write");
    let path_str = path.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&["--tag", path_str], None);
    let (ref_out, _, _) = run_ref(&["--tag", path_str], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Escaped newline Tag mismatch");
}

// =========================================================================
// Escaped filenames: carriage return
// =========================================================================

#[test]
fn cli_output_format_parity_escaped_cr_gnu() {
    let dir = std::env::temp_dir().join("xxhash_cli_fmt_tests");
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    let name = "cr\rname.txt";
    let path = dir.join(name);
    fs::write(&path, b"hello\n").expect("failed to write");
    let path_str = path.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&[path_str], None);
    let (ref_out, _, _) = run_ref(&[path_str], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Escaped CR GNU mismatch");
    assert!(
        rust_out.starts_with('\\'),
        "Escaped filename line should start with backslash prefix"
    );
}

#[test]
fn cli_output_format_parity_escaped_cr_tag() {
    let dir = std::env::temp_dir().join("xxhash_cli_fmt_tests");
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    let name = "cr\rname.txt";
    let path = dir.join(name);
    fs::write(&path, b"hello\n").expect("failed to write");
    let path_str = path.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&["--tag", path_str], None);
    let (ref_out, _, _) = run_ref(&["--tag", path_str], None);
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Escaped CR Tag mismatch");
}

// =========================================================================
// Combined format + algorithm matrix with a file
// =========================================================================

#[test]
fn cli_output_format_parity_all_formats_all_algos_file() {
    let file = create_temp_file("matrix.txt", b"matrix test data\n");
    let path = file.to_str().unwrap();

    let algo_flags: &[&str] = &["-H0", "-H1", "-H2", "-H3"];
    let format_combos: &[&[&str]] = &[
        &[],                          // GNU default
        &["--tag"],                   // Tag
        &["--little-endian"],         // LE
        &["--tag", "--little-endian"], // Tag+LE
    ];

    for algo in algo_flags {
        for fmt in format_combos {
            let mut rust_args: Vec<&str> = fmt.to_vec();
            rust_args.push(algo);
            rust_args.push(path);

            let mut ref_args: Vec<&str> = fmt.to_vec();
            ref_args.push(algo);
            ref_args.push(path);

            let (rust_out, _, rc) = run_rust(&rust_args, None);
            let (ref_out, _, _) = run_ref(&ref_args, None);
            assert_eq!(rc, 0);
            assert_eq!(
                rust_out, ref_out,
                "Mismatch for algo={} fmt={:?}",
                algo, fmt
            );
        }
    }
}

// =========================================================================
// Seeded output in various formats
// =========================================================================

#[test]
fn cli_output_format_parity_seeded_tag() {
    let data = b"seeded tag test\n";
    let (rust_out, _, rc) = run_rust(&["--tag", "--seed", "42"], Some(data));
    let (ref_out, _, _) = run_ref(&["--tag", "--seed", "42"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Seeded tag output mismatch");
}

#[test]
fn cli_output_format_parity_seeded_le() {
    let data = b"seeded le test\n";
    let (rust_out, _, rc) = run_rust(&["--little-endian", "--seed", "42"], Some(data));
    let (ref_out, _, _) = run_ref(&["--little-endian", "--seed", "42"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Seeded LE output mismatch");
}

#[test]
fn cli_output_format_parity_seeded_tag_le() {
    let data = b"seeded tag le test\n";
    let (rust_out, _, rc) =
        run_rust(&["--tag", "--little-endian", "--seed", "42"], Some(data));
    let (ref_out, _, _) =
        run_ref(&["--tag", "--little-endian", "--seed", "42"], Some(data));
    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Seeded Tag+LE output mismatch");
}

// =========================================================================
// Normal (non-escaped) filenames should NOT have the backslash prefix
// =========================================================================

#[test]
fn cli_output_format_parity_normal_filename_no_escape_prefix() {
    let file = create_temp_file("normal.txt", b"normal\n");
    let path = file.to_str().unwrap();

    let (rust_out, _, _) = run_rust(&[path], None);
    assert!(
        !rust_out.starts_with('\\'),
        "Normal filename should not have escape prefix, got: {}",
        rust_out
    );

    let (rust_out, _, _) = run_rust(&["--tag", path], None);
    assert!(
        !rust_out.starts_with('\\'),
        "Normal filename tag output should not have escape prefix, got: {}",
        rust_out
    );
}

// =========================================================================
// Filenames containing ` = ` — GNU and tagged output must match reference
// =========================================================================

#[test]
fn cli_output_format_parity_filename_with_equals_gnu() {
    let file = create_temp_file("key = value.txt", b"test data\n");
    let path = file.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&[path], None);
    let (ref_out, _, _) = run_ref(&[path], None);
    assert_eq!(rc, 0);
    assert_eq!(
        rust_out, ref_out,
        "GNU output for filename with ' = ' should match reference"
    );
}

#[test]
fn cli_output_format_parity_filename_with_equals_tag() {
    let file = create_temp_file("key = value.txt", b"test data\n");
    let path = file.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&["--tag", path], None);
    let (ref_out, _, _) = run_ref(&["--tag", path], None);
    assert_eq!(rc, 0);
    assert_eq!(
        rust_out, ref_out,
        "Tag output for filename with ' = ' should match reference"
    );
}

#[test]
fn cli_output_format_parity_filename_with_equals_all_algos() {
    let file = create_temp_file("key = value.txt", b"test data\n");
    let path = file.to_str().unwrap();

    for algo in &["-H0", "-H1", "-H2", "-H3"] {
        // GNU format
        let (rust_out, _, rc) = run_rust(&[algo, path], None);
        let (ref_out, _, _) = run_ref(&[algo, path], None);
        assert_eq!(rc, 0);
        assert_eq!(
            rust_out, ref_out,
            "GNU output for {} with ' = ' filename should match reference",
            algo
        );

        // Tagged format
        let (rust_out, _, rc) = run_rust(&["--tag", algo, path], None);
        let (ref_out, _, _) = run_ref(&["--tag", algo, path], None);
        assert_eq!(rc, 0);
        assert_eq!(
            rust_out, ref_out,
            "Tag output for {} with ' = ' filename should match reference",
            algo
        );
    }
}

// =========================================================================
// Filenames containing `) = ` — GNU and tagged output must match reference
// =========================================================================

#[test]
fn cli_output_format_parity_filename_with_paren_eq_gnu() {
    let file = create_temp_file("data) = deadbeef", b"test data\n");
    let path = file.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&[path], None);
    let (ref_out, _, _) = run_ref(&[path], None);
    assert_eq!(rc, 0);
    assert_eq!(
        rust_out, ref_out,
        "GNU output for filename with ') = <hex>' should match reference"
    );
}

#[test]
fn cli_output_format_parity_filename_with_paren_eq_tag() {
    let file = create_temp_file("data) = deadbeef", b"test data\n");
    let path = file.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&["--tag", path], None);
    let (ref_out, _, _) = run_ref(&["--tag", path], None);
    assert_eq!(rc, 0);
    assert_eq!(
        rust_out, ref_out,
        "Tag output for filename with ') = <hex>' should match reference"
    );
}

#[test]
fn cli_output_format_parity_filename_with_paren_eq_all_algos() {
    let file = create_temp_file("data) = deadbeef", b"test data\n");
    let path = file.to_str().unwrap();

    for algo in &["-H0", "-H1", "-H2", "-H3"] {
        // GNU format
        let (rust_out, _, rc) = run_rust(&[algo, path], None);
        let (ref_out, _, _) = run_ref(&[algo, path], None);
        assert_eq!(rc, 0);
        assert_eq!(
            rust_out, ref_out,
            "GNU output for {} with ') = <hex>' filename should match reference",
            algo
        );

        // Tagged format
        let (rust_out, _, rc) = run_rust(&["--tag", algo, path], None);
        let (ref_out, _, _) = run_ref(&["--tag", algo, path], None);
        assert_eq!(rc, 0);
        assert_eq!(
            rust_out, ref_out,
            "Tag output for {} with ') = <hex>' filename should match reference",
            algo
        );
    }
}

#[test]
fn cli_output_format_parity_filename_with_paren_eq_nonhex() {
    // Filename with `) = ` but non-hex suffix — still must match reference.
    let file = create_temp_file("result) = pass.log", b"test data\n");
    let path = file.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&[path], None);
    let (ref_out, _, _) = run_ref(&[path], None);
    assert_eq!(rc, 0);
    assert_eq!(
        rust_out, ref_out,
        "GNU output for filename with ') = <non-hex>' should match reference"
    );

    let (rust_out, _, rc) = run_rust(&["--tag", path], None);
    let (ref_out, _, _) = run_ref(&["--tag", path], None);
    assert_eq!(rc, 0);
    assert_eq!(
        rust_out, ref_out,
        "Tag output for filename with ') = <non-hex>' should match reference"
    );
}
