//! Reference binary invocation helpers.
//!
//! Provides utilities to locate, build, and invoke the external xxHash
//! reference binary (`xxhsum`) for parity testing. The reference checkout
//! lives outside the Rust repo at a configurable path and is never vendored
//! into this repository.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Default path to the external reference checkout.
const DEFAULT_REFERENCE_ROOT: &str = "/Users/ragas/code/missions/xxhash-reference";

/// Environment variable that overrides the reference checkout path.
const REFERENCE_ROOT_ENV: &str = "XXHASH_REFERENCE_ROOT";

/// Returns the path to the external reference checkout.
///
/// Checks `XXHASH_REFERENCE_ROOT` env var first, then falls back to the
/// default path. Returns `None` if the directory does not exist.
pub fn reference_root() -> Option<PathBuf> {
    let root = env::var(REFERENCE_ROOT_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_REFERENCE_ROOT));

    if root.is_dir() {
        Some(root)
    } else {
        None
    }
}

/// Returns the path to the reference `xxhsum` binary.
///
/// Returns `None` if the reference root is not found or the binary
/// does not exist.
pub fn reference_binary() -> Option<PathBuf> {
    let root = reference_root()?;
    let bin = root.join("xxhsum");
    if bin.exists() {
        Some(bin)
    } else {
        None
    }
}

/// Builds the reference binary if it does not already exist.
///
/// Runs `make xxhsum` in the reference checkout directory.
/// Returns `Ok(path)` with the binary path on success, or an error message.
pub fn ensure_reference_built() -> Result<PathBuf, String> {
    let root = reference_root().ok_or_else(|| {
        format!(
            "Reference checkout not found. Set {REFERENCE_ROOT_ENV} or ensure \
             {DEFAULT_REFERENCE_ROOT} exists."
        )
    })?;

    let bin = root.join("xxhsum");
    if bin.exists() {
        return Ok(bin);
    }

    let output = Command::new("make")
        .arg("xxhsum")
        .current_dir(&root)
        .output()
        .map_err(|e| format!("Failed to run make: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "make xxhsum failed with status {}:\nstdout: {}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    if bin.exists() {
        Ok(bin)
    } else {
        Err("make xxhsum succeeded but binary not found".to_string())
    }
}

/// Result of invoking the reference binary.
#[derive(Debug, Clone)]
pub struct ReferenceResult {
    /// The full stdout output.
    pub stdout: String,
    /// The full stderr output.
    pub stderr: String,
    /// The process exit code (None if terminated by signal).
    pub exit_code: Option<i32>,
    /// The parsed hash digest from stdout (if available).
    pub digest: Option<String>,
}

/// Invokes the reference binary to hash data from stdin.
///
/// # Arguments
/// * `data` - The bytes to hash via stdin
/// * `algo_flag` - Algorithm flag (e.g., "-H0" for XXH32, "-H1" for XXH64)
/// * `extra_args` - Additional command-line arguments
pub fn hash_stdin(
    data: &[u8],
    algo_flag: &str,
    extra_args: &[&str],
) -> Result<ReferenceResult, String> {
    let bin = reference_binary().ok_or_else(|| {
        "Reference binary not found. Run `make xxhsum` in the reference checkout.".to_string()
    })?;

    let mut cmd = Command::new(&bin);
    cmd.arg(algo_flag);
    for arg in extra_args {
        cmd.arg(arg);
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn reference binary: {e}"))?;

    // Write data to stdin
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(data)
            .map_err(|e| format!("Failed to write to stdin: {e}"))?;
        // Drop stdin to close it
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for reference binary: {e}"))?;

    parse_output(output)
}

/// Invokes the reference binary to hash a file.
///
/// # Arguments
/// * `file_path` - Path to the file to hash
/// * `algo_flag` - Algorithm flag (e.g., "-H0" for XXH32, "-H1" for XXH64)
/// * `extra_args` - Additional command-line arguments
pub fn hash_file(
    file_path: &Path,
    algo_flag: &str,
    extra_args: &[&str],
) -> Result<ReferenceResult, String> {
    let bin = reference_binary().ok_or_else(|| {
        "Reference binary not found. Run `make xxhsum` in the reference checkout.".to_string()
    })?;

    let mut cmd = Command::new(&bin);
    cmd.arg(algo_flag);
    for arg in extra_args {
        cmd.arg(arg);
    }
    cmd.arg(file_path);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run reference binary: {e}"))?;

    parse_output(output)
}

/// Parses the output of the reference binary into a `ReferenceResult`.
fn parse_output(output: Output) -> Result<ReferenceResult, String> {
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code();

    // Parse the digest from the first field of stdout.
    // Reference output format: "<hex_digest>  <filename>\n"
    // or for XXH3: "XXH3_<hex_digest>  <filename>\n"
    let digest = stdout
        .lines()
        .next()
        .and_then(|line| {
            let first_field = line.split_whitespace().next()?;
            // Strip any "XXH3_" prefix if present
            let hex = first_field.strip_prefix("XXH3_").unwrap_or(first_field);
            Some(hex.to_lowercase())
        });

    Ok(ReferenceResult {
        stdout,
        stderr,
        exit_code,
        digest,
    })
}

/// Fixture metadata about the reference binary for reproducibility.
#[derive(Debug, Clone)]
pub struct ReferenceMetadata {
    /// Path to the reference checkout root.
    pub root: PathBuf,
    /// Path to the reference binary.
    pub binary: PathBuf,
    /// Whether the binary exists and is executable.
    pub available: bool,
    /// Git commit hash of the reference checkout (if available).
    pub git_commit: Option<String>,
}

/// Collects metadata about the reference binary for reproducibility tracking.
pub fn collect_metadata() -> ReferenceMetadata {
    let root = reference_root().unwrap_or_else(|| PathBuf::from(DEFAULT_REFERENCE_ROOT));
    let binary = root.join("xxhsum");
    let available = binary.exists();

    let git_commit = if root.join(".git").exists() {
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&root)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
    } else {
        None
    };

    ReferenceMetadata {
        root,
        binary,
        available,
        git_commit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_root_exists() {
        let root = reference_root();
        assert!(
            root.is_some(),
            "Reference checkout should exist at {} or via {}",
            DEFAULT_REFERENCE_ROOT,
            REFERENCE_ROOT_ENV,
        );
    }

    #[test]
    fn reference_binary_exists() {
        let bin = reference_binary();
        assert!(
            bin.is_some(),
            "Reference binary (xxhsum) should exist in the reference checkout"
        );
    }

    #[test]
    fn reference_metadata_available() {
        let meta = collect_metadata();
        assert!(
            meta.available,
            "Reference binary should be available for parity testing"
        );
        assert!(
            meta.git_commit.is_some(),
            "Reference checkout should have a git commit"
        );
    }
}
