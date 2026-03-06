//! Clean-room Rust CLI for xxHash hashing.
//!
//! Provides `xxhsum`-compatible algorithm selection, seed handling,
//! file/stdin hashing, and correct exit behavior. This implementation
//! is derived from black-box behavioral observation of the upstream
//! CLI surface, without translating or copying any GPL source code.

use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process;

use xxhash_rs::xxh3::{Xxh3_128State, Xxh3_64State};
use xxhash_rs::xxh32::Xxh32State;
use xxhash_rs::xxh64::Xxh64State;

/// Hash algorithm selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Algorithm {
    XXH32,
    XXH64,
    XXH3_64,
    XXH3_128,
}

/// Parsed CLI arguments.
struct CliArgs {
    /// Selected hash algorithm (default: XXH64).
    algorithm: Algorithm,
    /// Seed value for the hash function.
    seed: u64,
    /// Input targets: file paths or "-" for stdin.
    /// Empty means hash stdin.
    inputs: Vec<String>,
}

/// Parse command-line arguments into structured CLI args.
///
/// Supports:
/// - `-H0`, `-H32` → XXH32
/// - `-H1`, `-H64` → XXH64 (default)
/// - `-H2`, `-H128` → XXH3_128
/// - `-H3` → XXH3_64
/// - `--seed <N>` → seed value
/// - Positional args → file paths; `-` forces stdin
fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let mut algorithm = Algorithm::XXH64; // default
    let mut seed: u64 = 0;
    let mut inputs = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-H0" | "-H32" => algorithm = Algorithm::XXH32,
            "-H1" | "-H64" => algorithm = Algorithm::XXH64,
            "-H2" | "-H128" => algorithm = Algorithm::XXH3_128,
            "-H3" => algorithm = Algorithm::XXH3_64,
            "--seed" => {
                i += 1;
                if i >= args.len() {
                    return Err("--seed requires a value".to_string());
                }
                seed = args[i]
                    .parse()
                    .map_err(|e| format!("invalid seed value '{}': {}", args[i], e))?;
            }
            _ => {
                // Positional argument: file path or "-" for stdin
                inputs.push(arg.clone());
            }
        }
        i += 1;
    }

    Ok(CliArgs {
        algorithm,
        seed,
        inputs,
    })
}

/// Buffer size for streaming file reads.
const BUF_SIZE: usize = 64 * 1024;

/// Hash a reader using the streaming API and return the formatted digest string.
fn hash_reader(reader: &mut dyn Read, algorithm: Algorithm, seed: u64) -> io::Result<String> {
    let mut buf = vec![0u8; BUF_SIZE];

    match algorithm {
        Algorithm::XXH32 => {
            let mut state = Xxh32State::new(seed as u32);
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                state.update(&buf[..n]);
            }
            let h = state.digest();
            Ok(format!("{h:08x}"))
        }
        Algorithm::XXH64 => {
            let mut state = Xxh64State::new(seed);
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                state.update(&buf[..n]);
            }
            let h = state.digest();
            Ok(format!("{h:016x}"))
        }
        Algorithm::XXH3_64 => {
            let mut state = Xxh3_64State::new(seed);
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                state.update(&buf[..n]);
            }
            let h = state.digest();
            Ok(format!("XXH3_{h:016x}"))
        }
        Algorithm::XXH3_128 => {
            let mut state = Xxh3_128State::new(seed);
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                state.update(&buf[..n]);
            }
            let (lo, hi) = state.digest();
            // Canonical output is hi||lo (big-endian word order)
            Ok(format!("{hi:016x}{lo:016x}"))
        }
    }
}

/// Hash stdin and print the result.
fn hash_stdin(algorithm: Algorithm, seed: u64) -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let digest = hash_reader(&mut reader, algorithm, seed)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "{}  stdin", digest)?;
    Ok(())
}

/// Format an I/O error as the OS-native description (matching strerror),
/// without Rust's "(os error N)" suffix.
fn os_error_description(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::NotFound => "No such file or directory".to_string(),
        io::ErrorKind::PermissionDenied => "Permission denied".to_string(),
        io::ErrorKind::AlreadyExists => "File exists".to_string(),
        io::ErrorKind::InvalidInput => "Invalid argument".to_string(),
        _ => format!("{}", e),
    }
}

/// Hash a file and print the result.
fn hash_file(path: &str, algorithm: Algorithm, seed: u64) -> Result<(), io::Error> {
    let mut file = File::open(path)?;
    let digest = hash_reader(&mut file, algorithm, seed)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "{}  {}", digest, path)?;
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let mut had_error = false;

    if cli.inputs.is_empty() {
        // No files specified: hash stdin
        if let Err(e) = hash_stdin(cli.algorithm, cli.seed) {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    } else {
        for input in &cli.inputs {
            if input == "-" {
                // Explicit stdin
                if let Err(e) = hash_stdin(cli.algorithm, cli.seed) {
                    eprintln!("Error: {}", e);
                    had_error = true;
                }
            } else {
                // Named file
                if let Err(e) = hash_file(input, cli.algorithm, cli.seed) {
                    eprintln!("Error: unable to open input");
                    eprintln!(
                        "Error: Could not open '{}': {}. ",
                        input,
                        os_error_description(&e)
                    );
                    had_error = true;
                }
            }
        }
    }

    if had_error {
        process::exit(1);
    }
}
