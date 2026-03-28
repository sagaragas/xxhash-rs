//! Integration tests for fixture loading and reference parity infrastructure.
//!
//! These tests verify that:
//! - The test buffer generator produces deterministic output
//! - All vector files load correctly with proper metadata
//! - The external reference binary is available and functional
//! - The parity harness can invoke the reference and parse its output
//! - Fixture metadata is recorded for reproducibility

#[allow(dead_code)]
mod fixtures;

use fixtures::reference;
use fixtures::{
    generate_test_buffer, load_all_vectors, load_vectors_for, val_hash_001_lengths, Algorithm,
};

// ============================================================================
// Test buffer generation
// ============================================================================

macro_rules! skip_without_reference {
    () => {
        if fixtures::reference::reference_binary().is_none() {
            eprintln!("Skipped: reference binary not available (set XXHASH_REFERENCE_ROOT)");
            return;
        }
    };
}


#[test]
fn reference_fixture_loading_test_buffer_deterministic() {
    let buf1 = generate_test_buffer(1024);
    let buf2 = generate_test_buffer(1024);
    assert_eq!(buf1, buf2, "Test buffer must be deterministic");
}

#[test]
fn reference_fixture_loading_test_buffer_matches_reference_first_bytes() {
    // The reference buffer starts with byteGen = PRIME32 = 0x9E3779B1 (as u64).
    // byte[0] = (0x000000009E3779B1 >> 56) = 0x00
    // Then byteGen *= PRIME64 (wrapping).
    let buf = generate_test_buffer(4);
    // Verify we get 4 bytes (exact values depend on wrapping arithmetic)
    assert_eq!(buf.len(), 4);
    // First byte: 0x9E3779B1 >> 56 = 0 (since it fits in 32 bits)
    assert_eq!(buf[0], 0x00);
}

#[test]
fn reference_fixture_loading_test_buffer_nonempty_for_large_sizes() {
    for size in [1, 16, 128, 240, 241, 256, 512, 2048] {
        let buf = generate_test_buffer(size);
        assert_eq!(buf.len(), size);
        // For sizes > 0, at least some bytes should be non-zero after the first
        if size > 4 {
            assert!(
                buf.iter().any(|&b| b != 0),
                "Buffer of size {size} should contain non-zero bytes"
            );
        }
    }
}

// ============================================================================
// Vector loading
// ============================================================================

#[test]
fn reference_fixture_loading_all_vectors_load() {
    let (vectors, metadata) = load_all_vectors();

    assert!(
        !vectors.is_empty(),
        "Should load at least some test vectors"
    );
    assert_eq!(
        metadata.vector_count,
        vectors.len(),
        "Metadata count must match actual vector count"
    );
    assert_eq!(
        metadata.algorithms.len(),
        4,
        "Should cover all 4 algorithm variants"
    );
    assert!(
        metadata.max_buffer_len > 0,
        "Max buffer length should be positive"
    );

    println!(
        "Loaded {} vectors from: {}",
        metadata.vector_count, metadata.source
    );
    println!(
        "Algorithms: {:?}",
        metadata
            .algorithms
            .iter()
            .map(|a| a.name())
            .collect::<Vec<_>>()
    );
    println!("Max buffer length: {}", metadata.max_buffer_len);
}

#[test]
fn reference_fixture_loading_per_algorithm_vectors() {
    for algo in [
        Algorithm::XXH32,
        Algorithm::XXH64,
        Algorithm::XXH3_64,
        Algorithm::XXH3_128,
    ] {
        let vectors = load_vectors_for(algo);
        assert!(
            !vectors.is_empty(),
            "{} should have at least one test vector",
            algo.name()
        );

        // All vectors should be tagged with the correct algorithm
        for v in &vectors {
            assert_eq!(
                v.algorithm, algo,
                "Vector for len={} should be tagged as {}",
                v.len,
                algo.name()
            );
        }

        // Should include seed=0 and at least one seeded vector
        let has_zero_seed = vectors.iter().any(|v| v.seed == 0);
        let has_nonzero_seed = vectors.iter().any(|v| v.seed != 0);
        assert!(has_zero_seed, "{} should have seed=0 vectors", algo.name());
        assert!(
            has_nonzero_seed,
            "{} should have non-zero seed vectors",
            algo.name()
        );
    }
}

#[test]
fn reference_fixture_loading_val_hash_001_coverage() {
    let required_lengths = val_hash_001_lengths();

    for algo in [
        Algorithm::XXH32,
        Algorithm::XXH64,
        Algorithm::XXH3_64,
        Algorithm::XXH3_128,
    ] {
        let vectors = load_vectors_for(algo);
        let available_lengths: std::collections::HashSet<usize> =
            vectors.iter().map(|v| v.len).collect();

        for &len in required_lengths {
            assert!(
                available_lengths.contains(&len),
                "{} is missing a vector for length {len} (required by VAL-HASH-001)",
                algo.name()
            );
        }
    }
}

#[test]
fn reference_fixture_loading_hex_format_consistency() {
    let (vectors, _) = load_all_vectors();

    for v in &vectors {
        // Hex strings should only contain lowercase hex digits
        assert!(
            v.expected_hex.chars().all(|c| c.is_ascii_hexdigit()),
            "Vector for {}[len={}] has non-hex chars in digest: {}",
            v.algorithm.name(),
            v.len,
            v.expected_hex,
        );

        // Verify expected hex length per algorithm
        let expected_hex_len = match v.algorithm {
            Algorithm::XXH32 => 8,
            Algorithm::XXH64 | Algorithm::XXH3_64 => 16,
            Algorithm::XXH3_128 => 32,
        };
        assert_eq!(
            v.expected_hex.len(),
            expected_hex_len,
            "Vector for {}[len={}] has wrong hex length: expected {expected_hex_len}, got {}",
            v.algorithm.name(),
            v.len,
            v.expected_hex.len(),
        );
    }
}

// ============================================================================
// Reference binary availability and invocation
// ============================================================================

#[test]
fn reference_fixture_loading_reference_available() {
    skip_without_reference!();
    let root = reference::reference_root();
    assert!(
        root.is_some(),
        "External reference checkout should be available"
    );

    let bin = reference::reference_binary();
    assert!(bin.is_some(), "Reference xxhsum binary should be available");
}

#[test]
fn reference_fixture_loading_reference_metadata() {
    skip_without_reference!();
    let meta = reference::collect_metadata();
    assert!(meta.available, "Reference binary should be available");
    assert!(
        meta.git_commit.is_some(),
        "Reference checkout should have a git commit"
    );

    println!("Reference root: {:?}", meta.root);
    println!("Reference binary: {:?}", meta.binary);
    println!("Git commit: {:?}", meta.git_commit);
}

#[test]
fn reference_fixture_loading_reference_stdin_hash() {
    skip_without_reference!();
    // Hash an empty input with XXH64 (default)
    let result = reference::hash_stdin(b"", "-H1", &[]).expect("Should invoke reference binary");

    assert_eq!(result.exit_code, Some(0), "Reference should exit 0");
    assert!(
        result.digest.is_some(),
        "Should parse a digest from reference output"
    );

    // Empty input with seed=0 for XXH64 should produce the known vector
    let digest = result.digest.unwrap();
    assert_eq!(
        digest, "ef46db3751d8e999",
        "Empty input XXH64 should match known vector"
    );
}

#[test]
fn reference_fixture_loading_reference_all_algorithms() {
    skip_without_reference!();
    let test_data = b"hello world";

    for algo in [
        Algorithm::XXH32,
        Algorithm::XXH64,
        Algorithm::XXH3_64,
        Algorithm::XXH3_128,
    ] {
        let result = reference::hash_stdin(test_data, algo.reference_flag(), &[])
            .unwrap_or_else(|e| panic!("Failed to hash with {}: {e}", algo.name()));

        assert_eq!(
            result.exit_code,
            Some(0),
            "{} should exit 0",
            algo.name()
        );
        assert!(
            result.digest.is_some(),
            "{} should produce a digest",
            algo.name()
        );

        println!(
            "{}: {}",
            algo.name(),
            result.digest.as_deref().unwrap_or("NONE")
        );
    }
}

#[test]
fn reference_fixture_loading_reference_with_seed() {
    skip_without_reference!();
    // Hash empty input with seed=0x9E3779B1 for XXH32
    let result = reference::hash_stdin(b"", "-H0", &["--seed", "2654435761"])
        .expect("Should invoke reference binary with seed");

    assert_eq!(result.exit_code, Some(0));
    let digest = result.digest.expect("Should parse digest");
    assert_eq!(
        digest, "36b78ae7",
        "Empty input XXH32 with PRIME32 seed should match known vector"
    );
}

// ============================================================================
// Tagged output parsing
// ============================================================================

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_format() {
    // GNU format: "e4c191d091bd8853  stdin"
    let digest = reference::parse_digest_from_line("e4c191d091bd8853  stdin");
    assert_eq!(
        digest.as_deref(),
        Some("e4c191d091bd8853"),
        "Should extract digest from GNU format"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_xxh3_gnu_format() {
    // XXH3 GNU format: "XXH3_99fc819aaba2462a  stdin"
    let digest = reference::parse_digest_from_line("XXH3_99fc819aaba2462a  stdin");
    assert_eq!(
        digest.as_deref(),
        Some("99fc819aaba2462a"),
        "Should strip XXH3_ prefix and extract digest"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_bsd_format() {
    // BSD tagged format: "XXH64 (stdin) = e4c191d091bd8853"
    let digest = reference::parse_digest_from_line("XXH64 (stdin) = e4c191d091bd8853");
    assert_eq!(
        digest.as_deref(),
        Some("e4c191d091bd8853"),
        "Should extract digest from BSD tagged format"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_bsd_xxh32() {
    // BSD tagged format for XXH32: "XXH32 (stdin) = 946b5bf9"
    let digest = reference::parse_digest_from_line("XXH32 (stdin) = 946b5bf9");
    assert_eq!(
        digest.as_deref(),
        Some("946b5bf9"),
        "Should extract XXH32 digest from BSD tagged format"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_bsd_xxh128() {
    // BSD tagged format for XXH128: "XXH128 (stdin) = 6bba86c7e069f56d5a10b435f1c8e49c"
    let digest =
        reference::parse_digest_from_line("XXH128 (stdin) = 6bba86c7e069f56d5a10b435f1c8e49c");
    assert_eq!(
        digest.as_deref(),
        Some("6bba86c7e069f56d5a10b435f1c8e49c"),
        "Should extract XXH128 digest from BSD tagged format"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_bsd_xxh3() {
    // BSD tagged format for XXH3: "XXH3 (stdin) = 99fc819aaba2462a"
    let digest = reference::parse_digest_from_line("XXH3 (stdin) = 99fc819aaba2462a");
    assert_eq!(
        digest.as_deref(),
        Some("99fc819aaba2462a"),
        "Should extract XXH3 digest from BSD tagged format"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_bsd_le() {
    // BSD tagged LE format: "XXH64_LE (stdin) = 5388bd91d091c1e4"
    let digest = reference::parse_digest_from_line("XXH64_LE (stdin) = 5388bd91d091c1e4");
    assert_eq!(
        digest.as_deref(),
        Some("5388bd91d091c1e4"),
        "Should extract LE digest from BSD tagged format"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_escaped_gnu() {
    // Escaped GNU format: "\e4c191d091bd8853  back\\slash.txt"
    let digest = reference::parse_digest_from_line("\\e4c191d091bd8853  back\\\\slash.txt");
    assert_eq!(
        digest.as_deref(),
        Some("e4c191d091bd8853"),
        "Should handle escaped GNU format"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_escaped_bsd() {
    // Escaped BSD format: "\XXH64 (back\\slash.txt) = e4c191d091bd8853"
    let digest =
        reference::parse_digest_from_line("\\XXH64 (back\\\\slash.txt) = e4c191d091bd8853");
    assert_eq!(
        digest.as_deref(),
        Some("e4c191d091bd8853"),
        "Should handle escaped BSD tagged format"
    );
}

// ============================================================================
// GNU filenames containing ` = ` — parser must not confuse them with tagged format
// ============================================================================

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_filename_with_equals() {
    // GNU format with filename containing " = ":
    // "26186c7d853ea72d  key = value.txt"
    // The parser must extract "26186c7d853ea72d", not "value.txt"
    let digest =
        reference::parse_digest_from_line("26186c7d853ea72d  key = value.txt");
    assert_eq!(
        digest.as_deref(),
        Some("26186c7d853ea72d"),
        "Should extract digest from GNU line even when filename contains ' = '"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_filename_with_multiple_equals() {
    // GNU format with filename containing multiple " = ":
    // "709a6d99  a = b = c.txt"
    let digest =
        reference::parse_digest_from_line("709a6d99  a = b = c.txt");
    assert_eq!(
        digest.as_deref(),
        Some("709a6d99"),
        "Should extract 8-char XXH32 digest from GNU line with multiple ' = '"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_xxh3_filename_with_equals() {
    // XXH3 GNU format with filename containing " = ":
    // "XXH3_74298474e8c89b3a  key = value.txt"
    let digest =
        reference::parse_digest_from_line("XXH3_74298474e8c89b3a  key = value.txt");
    assert_eq!(
        digest.as_deref(),
        Some("74298474e8c89b3a"),
        "Should extract XXH3 digest from GNU line with ' = ' in filename"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_tagged_still_works_with_equals_in_filename() {
    // Tagged format where filename itself contains " = ":
    // "XXH64 (key = value.txt) = 26186c7d853ea72d"
    // The parser must extract "26186c7d853ea72d" from after the LAST " = "
    let digest = reference::parse_digest_from_line(
        "XXH64 (key = value.txt) = 26186c7d853ea72d",
    );
    assert_eq!(
        digest.as_deref(),
        Some("26186c7d853ea72d"),
        "Should extract digest from tagged line even when filename contains ' = '"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_128bit_filename_with_equals() {
    // XXH128 GNU format with filename containing " = ":
    // "8d7ada9ae0ad378ccb2d0fa0a59fbfe4  key = value.txt"
    let digest = reference::parse_digest_from_line(
        "8d7ada9ae0ad378ccb2d0fa0a59fbfe4  key = value.txt",
    );
    assert_eq!(
        digest.as_deref(),
        Some("8d7ada9ae0ad378ccb2d0fa0a59fbfe4"),
        "Should extract 32-char XXH128 digest from GNU line with ' = ' in filename"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_reference_integration() {
    skip_without_reference!();
    // Integration test: invoke the reference binary in tagged mode and verify
    // the parser extracts the correct digest.
    let result_gnu = reference::hash_stdin(b"hello\n", "-H1", &[])
        .expect("Reference GNU invocation failed");
    let result_tag = reference::hash_stdin(b"hello\n", "-H1", &["--tag"])
        .expect("Reference tagged invocation failed");

    let digest_gnu = result_gnu.digest.expect("GNU digest should be parsed");
    let digest_tag = result_tag.digest.expect("Tagged digest should be parsed");

    assert_eq!(
        digest_gnu, digest_tag,
        "GNU and tagged formats should yield the same parsed digest"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_reference_all_algos() {
    skip_without_reference!();
    // Verify the parser works for all algorithms in tagged mode via reference.
    for algo in [
        Algorithm::XXH32,
        Algorithm::XXH64,
        Algorithm::XXH3_64,
        Algorithm::XXH3_128,
    ] {
        let result_gnu = reference::hash_stdin(b"test\n", algo.reference_flag(), &[])
            .unwrap_or_else(|e| panic!("{}: GNU invocation failed: {e}", algo.name()));
        let result_tag = reference::hash_stdin(b"test\n", algo.reference_flag(), &["--tag"])
            .unwrap_or_else(|e| panic!("{}: tagged invocation failed: {e}", algo.name()));

        let digest_gnu = result_gnu
            .digest
            .unwrap_or_else(|| panic!("{}: GNU digest should be parsed", algo.name()));
        let digest_tag = result_tag
            .digest
            .unwrap_or_else(|| panic!("{}: tagged digest should be parsed", algo.name()));

        assert_eq!(
            digest_gnu, digest_tag,
            "{}: GNU and tagged digests should match",
            algo.name()
        );
    }
}

// ============================================================================
// GNU filenames containing ` = ` with hex-like trailing segments
// ============================================================================
// Regression: when the trailing filename segment after the last ` = ` in a
// GNU-format line is entirely hex-like (e.g. `file = deadbeef`), the parser
// must still recognise the line as GNU and extract the leading digest token,
// not the trailing hex-like filename portion.

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_hex_like_suffix_after_equals() {
    // GNU XXH64 line whose filename ends with a hex-like segment:
    // "e4c191d091bd8853  file = deadbeef"
    // The parser must extract "e4c191d091bd8853", not "deadbeef".
    let digest =
        reference::parse_digest_from_line("e4c191d091bd8853  file = deadbeef");
    assert_eq!(
        digest.as_deref(),
        Some("e4c191d091bd8853"),
        "Should extract digest from GNU line even when trailing filename after ' = ' is hex-like"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_8char_hex_suffix_after_equals() {
    // GNU XXH64 line whose filename has an 8-char hex-like suffix (XXH32-length):
    // "a1b2c3d4e5f6a7b8  config = abcd1234"
    let digest =
        reference::parse_digest_from_line("a1b2c3d4e5f6a7b8  config = abcd1234");
    assert_eq!(
        digest.as_deref(),
        Some("a1b2c3d4e5f6a7b8"),
        "Should extract 16-char digest from GNU line when filename suffix is 8-char hex"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_16char_hex_suffix_after_equals() {
    // GNU XXH64 line whose filename has a 16-char hex-like suffix:
    // "a1b2c3d4e5f6a7b8  result = a1b2c3d4e5f6a7b8"
    // Both the digest and the trailing text are 16 hex chars — parser must pick the leading one.
    let digest =
        reference::parse_digest_from_line("a1b2c3d4e5f6a7b8  result = a1b2c3d4e5f6a7b8");
    assert_eq!(
        digest.as_deref(),
        Some("a1b2c3d4e5f6a7b8"),
        "Should extract leading digest even when trailing filename is also 16-char hex"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_32char_hex_suffix_after_equals() {
    // GNU XXH128 line whose filename has a 32-char hex suffix (mimics another XXH128 digest):
    // "8d7ada9ae0ad378ccb2d0fa0a59fbfe4  data = 0123456789abcdef0123456789abcdef"
    let digest = reference::parse_digest_from_line(
        "8d7ada9ae0ad378ccb2d0fa0a59fbfe4  data = 0123456789abcdef0123456789abcdef",
    );
    assert_eq!(
        digest.as_deref(),
        Some("8d7ada9ae0ad378ccb2d0fa0a59fbfe4"),
        "Should extract 32-char digest from GNU line when trailing filename is also 32-char hex"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_xxh32_gnu_hex_suffix_after_equals() {
    // GNU XXH32 line whose filename has a hex-like suffix:
    // "709a6d99  hash = beef"
    let digest =
        reference::parse_digest_from_line("709a6d99  hash = beef");
    assert_eq!(
        digest.as_deref(),
        Some("709a6d99"),
        "Should extract 8-char XXH32 digest from GNU line with hex-like suffix"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_xxh3_gnu_hex_suffix_after_equals() {
    // XXH3 GNU format with hex-like filename suffix:
    // "XXH3_74298474e8c89b3a  key = cafebabe"
    let digest =
        reference::parse_digest_from_line("XXH3_74298474e8c89b3a  key = cafebabe");
    assert_eq!(
        digest.as_deref(),
        Some("74298474e8c89b3a"),
        "Should extract XXH3 digest from GNU line with hex-like suffix"
    );
}

// ============================================================================
// GNU filenames containing `) = ` with hex-like trailing segments
// ============================================================================
// Regression: when the trailing filename segment after `) = ` in a
// GNU-format line is entirely hex-like, the parser must not confuse it
// with a BSD-tagged line.  Only lines starting with a recognised tagged
// algorithm prefix (e.g. "XXH64 (") should be treated as tagged format.

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_paren_eq_hex_suffix() {
    // GNU XXH64 line whose filename contains `) = <hex>`:
    // "e4c191d091bd8853  foo) = deadbeef"
    // Parser must extract "e4c191d091bd8853", NOT "deadbeef".
    let digest =
        reference::parse_digest_from_line("e4c191d091bd8853  foo) = deadbeef");
    assert_eq!(
        digest.as_deref(),
        Some("e4c191d091bd8853"),
        "Should extract digest from GNU line even when filename contains ') = <hex>'"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_paren_eq_16char_hex() {
    // GNU XXH64 line whose filename contains `) = <16-char hex>`:
    // "a1b2c3d4e5f6a7b8  result) = 1111222233334444"
    // The trailing hex differs from the leading digest to prevent coincidental pass.
    let digest =
        reference::parse_digest_from_line("a1b2c3d4e5f6a7b8  result) = 1111222233334444");
    assert_eq!(
        digest.as_deref(),
        Some("a1b2c3d4e5f6a7b8"),
        "Should extract leading digest from GNU line with ') = <16-char hex>' in filename"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_paren_eq_8char_hex() {
    // GNU XXH32 line whose filename contains `) = <8-char hex>`:
    // "709a6d99  data) = abcd1234"
    let digest =
        reference::parse_digest_from_line("709a6d99  data) = abcd1234");
    assert_eq!(
        digest.as_deref(),
        Some("709a6d99"),
        "Should extract 8-char XXH32 digest from GNU line with ') = <hex>' in filename"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_gnu_paren_eq_32char_hex() {
    // GNU XXH128 line whose filename contains `) = <32-char hex>`:
    // "8d7ada9ae0ad378ccb2d0fa0a59fbfe4  data) = 0123456789abcdef0123456789abcdef"
    let digest = reference::parse_digest_from_line(
        "8d7ada9ae0ad378ccb2d0fa0a59fbfe4  data) = 0123456789abcdef0123456789abcdef",
    );
    assert_eq!(
        digest.as_deref(),
        Some("8d7ada9ae0ad378ccb2d0fa0a59fbfe4"),
        "Should extract 32-char digest from GNU line with ') = <32-char hex>' in filename"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_xxh3_gnu_paren_eq_hex() {
    // XXH3 GNU format with `) = <hex>` in filename:
    // "XXH3_74298474e8c89b3a  key) = cafebabe"
    let digest =
        reference::parse_digest_from_line("XXH3_74298474e8c89b3a  key) = cafebabe");
    assert_eq!(
        digest.as_deref(),
        Some("74298474e8c89b3a"),
        "Should extract XXH3 digest from GNU line with ') = <hex>' in filename"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_tagged_with_paren_eq_filename() {
    // True tagged line where the filename itself contains `) = <hex>`:
    // "XXH64 (data) = deadbeef) = 8c6c22be5ab13e44"
    // The parser should use the LAST `) = ` as the structural separator.
    let digest = reference::parse_digest_from_line(
        "XXH64 (data) = deadbeef) = 8c6c22be5ab13e44",
    );
    assert_eq!(
        digest.as_deref(),
        Some("8c6c22be5ab13e44"),
        "Should extract digest from tagged line when filename contains ') = <hex>'"
    );
}

#[test]
fn reference_fixture_loading_parse_tagged_output_tagged_with_eq_and_paren_eq() {
    // True tagged line where the filename contains both ` = ` and `) = `:
    // "XXH32 (key = val) = cafe) = abcd1234"
    let digest = reference::parse_digest_from_line(
        "XXH32 (key = val) = cafe) = abcd1234",
    );
    assert_eq!(
        digest.as_deref(),
        Some("abcd1234"),
        "Should extract digest from tagged line with complex filename"
    );
}

// ============================================================================
// Parity harness smoke test
// ============================================================================

#[test]
fn reference_fixture_loading_parity_harness_smoke() {
    skip_without_reference!();
    // Verify we can generate a test buffer, hash it with the reference,
    // and get a consistent result.
    let buf = generate_test_buffer(16);

    // Hash with XXH64, seed=0 via reference
    let result = reference::hash_stdin(&buf, "-H1", &[]).expect("Reference invocation failed");
    assert_eq!(result.exit_code, Some(0));

    let digest = result.digest.expect("Should get a digest");
    assert_eq!(digest.len(), 16, "XXH64 digest should be 16 hex chars");

    // Verify determinism: hash the same buffer again
    let result2 = reference::hash_stdin(&buf, "-H1", &[]).expect("Second invocation failed");
    let digest2 = result2.digest.expect("Should get a digest");
    assert_eq!(digest, digest2, "Same input should produce same digest");

    println!("Parity harness smoke: XXH64 of 16-byte test buffer = {digest}");
}

#[test]
fn reference_fixture_loading_parity_test_buffer_against_reference() {
    skip_without_reference!();
    // Cross-check: hash the canonical test buffer at a known length with the
    // reference binary and verify it matches the expected vector.

    // XXH32, len=16, seed=0 -> expected 0x93BA3759
    let buf = generate_test_buffer(16);
    let result = reference::hash_stdin(&buf, "-H0", &[]).expect("Reference invocation failed");
    assert_eq!(result.exit_code, Some(0));
    let digest = result.digest.expect("Should get a digest");
    assert_eq!(
        digest, "93ba3759",
        "Reference XXH32 of 16-byte test buffer should match known vector"
    );

    // XXH64, len=0, seed=0 -> expected 0xEF46DB3751D8E999
    let result = reference::hash_stdin(b"", "-H1", &[]).expect("Reference invocation failed");
    let digest = result.digest.expect("Should get a digest");
    assert_eq!(
        digest, "ef46db3751d8e999",
        "Reference XXH64 of empty input should match known vector"
    );
}
