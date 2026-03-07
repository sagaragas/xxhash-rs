//! CLI file-list parity tests.
//!
//! Validates VAL-HASH-006: Both --files-from and --filelist accept listed
//! targets in order, support list input from file and stdin, ignore comment
//! lines beginning with `#`, and accept escaped entries the same way as the
//! reference surface.

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// Returns the path to the Rust CLI binary built by cargo.
fn rust_binary() -> PathBuf {
    env!("CARGO_BIN_EXE_xxhash-rs").into()
}

/// Default path to the external reference checkout.
const DEFAULT_REFERENCE_ROOT: &str = "/Users/ragas/code/rewrites/xxhash-reference";

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

/// Monotonic counter for unique temp dir names.
static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Run a CLI binary with the given args and optional stdin data.
fn run_cli(
    bin: &PathBuf,
    args: &[&str],
    stdin_data: Option<&[u8]>,
    cwd: Option<&PathBuf>,
) -> (String, String, i32) {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

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
fn run_rust(
    args: &[&str],
    stdin_data: Option<&[u8]>,
    cwd: Option<&PathBuf>,
) -> (String, String, i32) {
    run_cli(&rust_binary(), args, stdin_data, cwd)
}

/// Run the reference CLI.
fn run_ref(
    args: &[&str],
    stdin_data: Option<&[u8]>,
    cwd: Option<&PathBuf>,
) -> (String, String, i32) {
    run_cli(
        &reference_binary().expect("reference binary not found"),
        args,
        stdin_data,
        cwd,
    )
}

/// Create a unique temp directory for a single test.
fn unique_temp_dir() -> PathBuf {
    let id = DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("xxhash_filelist_{}_{}", pid, id));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir
}

/// Create a file inside a directory.
fn create_file(dir: &std::path::Path, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(&path, content).expect("failed to write temp file");
    path
}

/// Create a filelist file inside a directory.
fn create_filelist(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).expect("failed to write filelist");
    path
}

// =========================================================================
// --files-from reads listed files in order
// =========================================================================

#[test]
fn cli_filelist_parity_files_from_basic_order() {
    let dir = unique_temp_dir();
    create_file(&dir, "aaa.txt", b"alpha\n");
    create_file(&dir, "bbb.txt", b"beta\n");
    create_file(&dir, "ccc.txt", b"gamma\n");

    let filelist = create_filelist(&dir, "list.txt", "aaa.txt\nbbb.txt\nccc.txt\n");
    let filelist_str = filelist.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&["--files-from", filelist_str], None, Some(&dir));
    let (ref_out, _, _) = run_ref(&["--files-from", filelist_str], None, Some(&dir));

    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "--files-from basic order mismatch");

    // Verify ordering
    let lines: Vec<&str> = rust_out.lines().collect();
    assert_eq!(lines.len(), 3, "Should produce 3 output lines");
    assert!(lines[0].ends_with("aaa.txt"), "First file should be aaa.txt");
    assert!(lines[1].ends_with("bbb.txt"), "Second file should be bbb.txt");
    assert!(lines[2].ends_with("ccc.txt"), "Third file should be ccc.txt");

    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// --filelist is an alias for --files-from
// =========================================================================

#[test]
fn cli_filelist_parity_filelist_alias() {
    let dir = unique_temp_dir();
    create_file(&dir, "aaa.txt", b"alpha\n");
    create_file(&dir, "bbb.txt", b"beta\n");

    let filelist = create_filelist(&dir, "list.txt", "aaa.txt\nbbb.txt\n");
    let filelist_str = filelist.to_str().unwrap();

    let (rust_from, _, rc1) = run_rust(&["--files-from", filelist_str], None, Some(&dir));
    let (rust_list, _, rc2) = run_rust(&["--filelist", filelist_str], None, Some(&dir));

    assert_eq!(rc1, 0);
    assert_eq!(rc2, 0);
    assert_eq!(
        rust_from, rust_list,
        "--files-from and --filelist should produce identical output"
    );

    // Also match reference
    let (ref_out, _, _) = run_ref(&["--files-from", filelist_str], None, Some(&dir));
    assert_eq!(rust_from, ref_out, "--files-from should match reference");

    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// Comment lines (starting with #) are ignored
// =========================================================================

#[test]
fn cli_filelist_parity_comments_ignored() {
    let dir = unique_temp_dir();
    create_file(&dir, "file1.txt", b"one\n");
    create_file(&dir, "file2.txt", b"two\n");

    let filelist = create_filelist(
        &dir,
        "commented.txt",
        "# This is a comment\nfile1.txt\n# Another comment\nfile2.txt\n",
    );
    let filelist_str = filelist.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&["--files-from", filelist_str], None, Some(&dir));
    let (ref_out, _, _) = run_ref(&["--files-from", filelist_str], None, Some(&dir));

    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Comment handling mismatch");

    let lines: Vec<&str> = rust_out.lines().collect();
    assert_eq!(lines.len(), 2, "Comments should be skipped, leaving 2 files");

    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// File list input from stdin (--files-from -)
// =========================================================================

#[test]
fn cli_filelist_parity_files_from_stdin() {
    let dir = unique_temp_dir();
    create_file(&dir, "stdin_a.txt", b"data a\n");
    create_file(&dir, "stdin_b.txt", b"data b\n");

    let list_data = b"stdin_a.txt\nstdin_b.txt\n";

    let (rust_out, _, rc) = run_rust(&["--files-from", "-"], Some(list_data), Some(&dir));
    let (ref_out, _, _) = run_ref(&["--files-from", "-"], Some(list_data), Some(&dir));

    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "--files-from stdin mismatch");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cli_filelist_parity_filelist_stdin() {
    let dir = unique_temp_dir();
    create_file(&dir, "stdin_a.txt", b"data a\n");
    create_file(&dir, "stdin_b.txt", b"data b\n");

    let list_data = b"stdin_a.txt\nstdin_b.txt\n";

    let (rust_out, _, rc) = run_rust(&["--filelist", "-"], Some(list_data), Some(&dir));
    let (ref_out, _, _) = run_ref(&["--filelist", "-"], Some(list_data), Some(&dir));

    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "--filelist stdin mismatch");

    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// File list with comments from stdin
// =========================================================================

#[test]
fn cli_filelist_parity_stdin_with_comments() {
    let dir = unique_temp_dir();
    create_file(&dir, "f1.txt", b"one\n");
    create_file(&dir, "f2.txt", b"two\n");

    let list_data = b"# comment\nf1.txt\n# another\nf2.txt\n";

    let (rust_out, _, rc) = run_rust(&["--files-from", "-"], Some(list_data), Some(&dir));
    let (ref_out, _, _) = run_ref(&["--files-from", "-"], Some(list_data), Some(&dir));

    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Stdin with comments mismatch");

    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// File list preserves ordering
// =========================================================================

#[test]
fn cli_filelist_parity_ordering_preserved() {
    let dir = unique_temp_dir();
    create_file(&dir, "c.txt", b"cc\n");
    create_file(&dir, "a.txt", b"aa\n");
    create_file(&dir, "b.txt", b"bb\n");

    // List in reverse alphabetical order
    let filelist = create_filelist(&dir, "ordered.txt", "c.txt\na.txt\nb.txt\n");
    let filelist_str = filelist.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&["--files-from", filelist_str], None, Some(&dir));
    let (ref_out, _, _) = run_ref(&["--files-from", filelist_str], None, Some(&dir));

    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Order preservation mismatch");

    let lines: Vec<&str> = rust_out.lines().collect();
    assert!(lines[0].ends_with("c.txt"), "First should be c.txt");
    assert!(lines[1].ends_with("a.txt"), "Second should be a.txt");
    assert!(lines[2].ends_with("b.txt"), "Third should be b.txt");

    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// File list with escaped entries (backslash in filenames)
// =========================================================================

#[test]
fn cli_filelist_parity_escaped_entries() {
    let dir = unique_temp_dir();
    // Create a file with a backslash in its name
    create_file(&dir, "back\\slash.txt", b"hello\n");

    // The filelist contains the literal filename (with single backslash)
    let filelist = create_filelist(&dir, "escaped.txt", "back\\slash.txt\n");
    let filelist_str = filelist.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&["--files-from", filelist_str], None, Some(&dir));
    let (ref_out, _, _) = run_ref(&["--files-from", filelist_str], None, Some(&dir));

    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "Escaped filelist entry mismatch");
    // The output should have the escape prefix since the filename has a backslash
    assert!(
        rust_out.starts_with('\\'),
        "Escaped filename should have \\ prefix"
    );

    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// File list with --tag output
// =========================================================================

#[test]
fn cli_filelist_parity_with_tag_output() {
    let dir = unique_temp_dir();
    create_file(&dir, "t1.txt", b"tag1\n");
    create_file(&dir, "t2.txt", b"tag2\n");

    let filelist = create_filelist(&dir, "taglist.txt", "t1.txt\nt2.txt\n");
    let filelist_str = filelist.to_str().unwrap();

    let (rust_out, _, rc) =
        run_rust(&["--tag", "--files-from", filelist_str], None, Some(&dir));
    let (ref_out, _, _) =
        run_ref(&["--tag", "--files-from", filelist_str], None, Some(&dir));

    assert_eq!(rc, 0);
    assert_eq!(rust_out, ref_out, "--files-from with --tag mismatch");

    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// File list with various algorithms
// =========================================================================

#[test]
fn cli_filelist_parity_all_algorithms() {
    let dir = unique_temp_dir();
    create_file(&dir, "algo.txt", b"algorithm test\n");

    let filelist = create_filelist(&dir, "algolist.txt", "algo.txt\n");
    let filelist_str = filelist.to_str().unwrap();

    for flag in &["-H0", "-H1", "-H2", "-H3"] {
        let (rust_out, _, rc) =
            run_rust(&[flag, "--files-from", filelist_str], None, Some(&dir));
        let (ref_out, _, _) =
            run_ref(&[flag, "--files-from", filelist_str], None, Some(&dir));

        assert_eq!(rc, 0);
        assert_eq!(
            rust_out, ref_out,
            "--files-from with {} mismatch",
            flag
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// Empty lines in filelist are skipped
// =========================================================================

#[test]
fn cli_filelist_parity_empty_lines_skipped() {
    let dir = unique_temp_dir();
    create_file(&dir, "x.txt", b"data\n");

    let filelist = create_filelist(&dir, "empty_lines.txt", "\n\nx.txt\n\n");
    let filelist_str = filelist.to_str().unwrap();

    let (rust_out, _, rc) = run_rust(&["--files-from", filelist_str], None, Some(&dir));
    assert_eq!(rc, 0);

    let lines: Vec<&str> = rust_out.lines().collect();
    assert_eq!(lines.len(), 1, "Empty lines should be skipped");
    assert!(lines[0].ends_with("x.txt"));

    let _ = fs::remove_dir_all(&dir);
}
