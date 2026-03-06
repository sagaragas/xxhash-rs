//! Clean-room Rust CLI for xxHash hashing.
//!
//! Provides `xxhsum`-compatible algorithm selection, seed handling,
//! file/stdin hashing, and correct exit behavior. This implementation
//! is derived from black-box behavioral observation of the upstream
//! CLI surface, without translating or copying any GPL source code.

use std::env;
use std::fs::File;
use std::io::{self, BufRead, Read, Write};
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

impl Algorithm {
    /// Returns the tag label for this algorithm in BSD-style tagged output.
    fn tag_label(self) -> &'static str {
        match self {
            Algorithm::XXH32 => "XXH32",
            Algorithm::XXH64 => "XXH64",
            Algorithm::XXH3_64 => "XXH3",
            Algorithm::XXH3_128 => "XXH128",
        }
    }

    /// Returns the tag label with `_LE` suffix for tagged little-endian output.
    fn tag_label_le(self) -> &'static str {
        match self {
            Algorithm::XXH32 => "XXH32_LE",
            Algorithm::XXH64 => "XXH64_LE",
            Algorithm::XXH3_64 => "XXH3_LE",
            Algorithm::XXH3_128 => "XXH128_LE",
        }
    }
}

/// Output format mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    /// Default GNU-style: `<hash>  <filename>`
    Gnu,
    /// BSD-style tagged: `<ALGO> (<filename>) = <hash>`
    Tag,
}

/// Parsed CLI arguments.
struct CliArgs {
    /// Selected hash algorithm (default: XXH64).
    algorithm: Algorithm,
    /// Seed value for the hash function.
    seed: u64,
    /// Output mode: GNU (default) or tagged.
    output_mode: OutputMode,
    /// Whether to emit little-endian digests.
    little_endian: bool,
    /// Input targets: file paths or "-" for stdin.
    /// Empty means hash stdin.
    inputs: Vec<String>,
    /// File-list source: read input file paths from a file or stdin.
    /// `Some("-")` means read the list from stdin.
    filelist_source: Option<String>,
}

/// Parse command-line arguments into structured CLI args.
///
/// Supports:
/// - `-H0`, `-H32` → XXH32
/// - `-H1`, `-H64` → XXH64 (default)
/// - `-H2`, `-H128` → XXH3_128
/// - `-H3` → XXH3_64
/// - `--seed <N>` → seed value
/// - `--tag` → BSD-style tagged output
/// - `--little-endian` → little-endian digest output
/// - `--files-from <file>` → read input file paths from a file
/// - `--filelist <file>` → alias for --files-from
/// - Positional args → file paths; `-` forces stdin
fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let mut algorithm = Algorithm::XXH64; // default
    let mut seed: u64 = 0;
    let mut inputs = Vec::new();
    let mut output_mode = OutputMode::Gnu;
    let mut little_endian = false;
    let mut filelist_source: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-H0" | "-H32" => algorithm = Algorithm::XXH32,
            "-H1" | "-H64" => algorithm = Algorithm::XXH64,
            "-H2" | "-H128" => algorithm = Algorithm::XXH3_128,
            "-H3" => algorithm = Algorithm::XXH3_64,
            "--tag" => output_mode = OutputMode::Tag,
            "--little-endian" => little_endian = true,
            "--seed" => {
                i += 1;
                if i >= args.len() {
                    return Err("--seed requires a value".to_string());
                }
                seed = args[i]
                    .parse()
                    .map_err(|e| format!("invalid seed value '{}': {}", args[i], e))?;
            }
            "--files-from" | "--filelist" => {
                i += 1;
                if i >= args.len() {
                    return Err(format!("{} requires a value", arg));
                }
                filelist_source = Some(args[i].clone());
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
        output_mode,
        little_endian,
        inputs,
        filelist_source,
    })
}

/// Buffer size for streaming file reads.
const BUF_SIZE: usize = 64 * 1024;

/// Raw digest bytes from the hash function (before formatting).
enum RawDigest {
    U32(u32),
    U64(u64),
    U128 { lo: u64, hi: u64 },
}

/// Hash a reader using the streaming API and return the raw digest.
fn hash_reader_raw(reader: &mut dyn Read, algorithm: Algorithm, seed: u64) -> io::Result<RawDigest> {
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
            Ok(RawDigest::U32(state.digest()))
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
            Ok(RawDigest::U64(state.digest()))
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
            Ok(RawDigest::U64(state.digest()))
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
            Ok(RawDigest::U128 { lo, hi })
        }
    }
}

/// Swap bytes of a u32 to convert between big-endian and little-endian hex representation.
fn swap32(v: u32) -> u32 {
    v.swap_bytes()
}

/// Swap bytes of a u64 to convert between big-endian and little-endian hex representation.
fn swap64(v: u64) -> u64 {
    v.swap_bytes()
}

/// Format a raw digest as a hex string, applying little-endian byte swap if requested.
///
/// For XXH3_64 in GNU mode without little-endian, the reference CLI prefixes with `XXH3_`.
/// For XXH3_64 in GNU mode with little-endian, the reference CLI also prefixes with `XXH3_`.
fn format_digest(
    raw: &RawDigest,
    algorithm: Algorithm,
    little_endian: bool,
    output_mode: OutputMode,
) -> String {
    let needs_xxh3_prefix =
        algorithm == Algorithm::XXH3_64 && output_mode == OutputMode::Gnu;

    match raw {
        RawDigest::U32(v) => {
            let val = if little_endian { swap32(*v) } else { *v };
            format!("{val:08x}")
        }
        RawDigest::U64(v) => {
            let val = if little_endian { swap64(*v) } else { *v };
            if needs_xxh3_prefix {
                format!("XXH3_{val:016x}")
            } else {
                format!("{val:016x}")
            }
        }
        RawDigest::U128 { lo, hi } => {
            if little_endian {
                // Little-endian: swap each 64-bit word and reverse the word order
                let slo = swap64(*lo);
                let shi = swap64(*hi);
                format!("{slo:016x}{shi:016x}")
            } else {
                // Big-endian (canonical): hi||lo
                format!("{hi:016x}{lo:016x}")
            }
        }
    }
}

/// Check if a filename needs escaping (contains backslash, newline, or carriage return).
fn needs_escaping(name: &str) -> bool {
    name.contains('\\') || name.contains('\n') || name.contains('\r')
}

/// Escape a filename for output: `\` → `\\`, `\n` → `\n`, `\r` → `\r`.
fn escape_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

/// Format and print a single hash result line.
fn print_hash_line(
    out: &mut dyn Write,
    digest_str: &str,
    filename: &str,
    algorithm: Algorithm,
    little_endian: bool,
    output_mode: OutputMode,
) -> io::Result<()> {
    let escaped = needs_escaping(filename);
    let display_name = if escaped {
        escape_filename(filename)
    } else {
        filename.to_string()
    };

    match output_mode {
        OutputMode::Gnu => {
            // GNU format: `<hash>  <filename>`
            // If filename is escaped, prefix the line with `\`
            if escaped {
                writeln!(out, "\\{}  {}", digest_str, display_name)
            } else {
                writeln!(out, "{}  {}", digest_str, display_name)
            }
        }
        OutputMode::Tag => {
            // BSD tagged format: `<ALGO> (<filename>) = <hash>`
            // If filename is escaped, prefix the line with `\`
            let label = if little_endian {
                algorithm.tag_label_le()
            } else {
                algorithm.tag_label()
            };
            if escaped {
                writeln!(out, "\\{} ({}) = {}", label, display_name, digest_str)
            } else {
                writeln!(out, "{} ({}) = {}", label, display_name, digest_str)
            }
        }
    }
}

/// Hash stdin and print the result.
fn hash_stdin(
    algorithm: Algorithm,
    seed: u64,
    little_endian: bool,
    output_mode: OutputMode,
) -> io::Result<()> {
    let stdin_handle = io::stdin();
    let mut reader = stdin_handle.lock();
    let raw = hash_reader_raw(&mut reader, algorithm, seed)?;
    let digest_str = format_digest(&raw, algorithm, little_endian, output_mode);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    print_hash_line(&mut out, &digest_str, "stdin", algorithm, little_endian, output_mode)?;
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
fn hash_file(
    path: &str,
    algorithm: Algorithm,
    seed: u64,
    little_endian: bool,
    output_mode: OutputMode,
) -> Result<(), io::Error> {
    let mut file = File::open(path)?;
    let raw = hash_reader_raw(&mut file, algorithm, seed)?;
    let digest_str = format_digest(&raw, algorithm, little_endian, output_mode);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    print_hash_line(&mut out, &digest_str, path, algorithm, little_endian, output_mode)?;
    Ok(())
}

/// Read a filelist from a reader, filtering out comment lines (starting with `#`)
/// and empty lines.
fn read_filelist(reader: &mut dyn BufRead) -> io::Result<Vec<String>> {
    let mut paths = Vec::new();
    for line in reader.lines() {
        let line = line?;
        // Skip comment lines and empty lines
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        paths.push(line);
    }
    Ok(paths)
}

/// Load file list from a file or stdin.
fn load_filelist(source: &str) -> Result<Vec<String>, String> {
    if source == "-" {
        let stdin_handle = io::stdin();
        let mut reader = stdin_handle.lock();
        read_filelist(&mut reader).map_err(|e| format!("Error reading file list from stdin: {}", e))
    } else {
        let file = File::open(source)
            .map_err(|e| format!("Error opening file list '{}': {}", source, e))?;
        let mut reader = io::BufReader::new(file);
        read_filelist(&mut reader)
            .map_err(|e| format!("Error reading file list '{}': {}", source, e))
    }
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

    // Determine input sources: either from filelist or from positional args
    let inputs: Vec<String> = if let Some(ref source) = cli.filelist_source {
        match load_filelist(source) {
            Ok(paths) => paths,
            Err(e) => {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
    } else {
        cli.inputs.clone()
    };

    if inputs.is_empty() && cli.filelist_source.is_none() {
        // No files specified and no filelist: hash stdin
        if let Err(e) = hash_stdin(cli.algorithm, cli.seed, cli.little_endian, cli.output_mode) {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    } else {
        for input in &inputs {
            if input == "-" && cli.filelist_source.is_none() {
                // Explicit stdin (only in positional mode, not filelist mode)
                if let Err(e) =
                    hash_stdin(cli.algorithm, cli.seed, cli.little_endian, cli.output_mode)
                {
                    eprintln!("Error: {}", e);
                    had_error = true;
                }
            } else {
                // Named file
                if let Err(e) = hash_file(
                    input,
                    cli.algorithm,
                    cli.seed,
                    cli.little_endian,
                    cli.output_mode,
                ) {
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
