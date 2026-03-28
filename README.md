# xxhash-rs

A clean-room Rust implementation of the [xxHash](https://github.com/Cyan4973/xxHash)
family of hash functions, with a CLI tool for hashing and checksum verification.

## Supported Algorithms

| Algorithm   | Output   | Description                      |
|-------------|----------|----------------------------------|
| **XXH32**   | 32-bit   | Classic xxHash, fast on 32-bit   |
| **XXH64**   | 64-bit   | Classic xxHash, fast on 64-bit   |
| **XXH3_64** | 64-bit   | XXH3 variant, optimized for speed|
| **XXH3_128**| 128-bit  | XXH3 variant, 128-bit output     |

## Project Structure

```
xxhash-rs/          # Library crate: hash algorithm implementations
xxhash-cli/         # CLI binary crate: xxhsum-compatible tool
benchmarks/         # Benchmark harness and artifacts
publication/        # Rewrite study and publication evidence
```

## Rewrite Study

This project is the subject of a rewrite study that documents the design decisions,
correctness validation, benchmark methodology, and performance characteristics of the
Rust reimplementation.

**[Read the full article on ragas.dev](https://ragas.dev/blog/rewriting-xxhash-in-rust)** |
**[Read the repo-side study](publication/REWRITE_STUDY.md)**

The study covers:

- Bit-exact parity validation across all four hash variants (508 tests)
- CLI behavioral parity against the upstream `xxhsum` reference
- NEON-optimized XXH3 paths on Apple Silicon with scalar-parity verification
- End-to-end CLI benchmark results against the C reference, BLAKE3, and MD5
- Clean-room methodology and licensing boundary documentation

The study's claims are backed by machine-readable evidence artifacts under
[`publication/evidence/`](publication/evidence/), each pinned to the measured
revision [`evidence-v1`](https://github.com/sagaragas/xxhash-rs/tree/evidence-v1).

## Clean-Room Implementation

This is a clean-room reimplementation. The hash algorithms are implemented from
the published xxHash specification and BSD-licensed reference library material.
The CLI tool achieves behavioral compatibility through black-box observation of
the upstream `xxhsum` tool, without translating or copying any GPL-licensed
source code.

See [LEGAL.md](LEGAL.md) for the full clean-room boundary and attribution details.

## Building

```sh
cargo build --workspace
```

## Testing

```sh
# Run all self-contained tests (hash vectors, streaming, SIMD parity)
cargo test --workspace
```

CLI parity tests compare output against the upstream C reference binary (`xxhsum`).
These tests skip automatically when the reference is not available. To enable them:

```sh
export XXHASH_REFERENCE_ROOT=/path/to/xxHash   # checkout of github.com/Cyan4973/xxHash
make -C "$XXHASH_REFERENCE_ROOT" xxhsum         # build the reference binary
cargo test --workspace                           # parity tests now run
```

## Verification

The publication evidence can be verified with:

```sh
# Verify the claim/evidence map
python3 publication/claim_map.py --verify

# Run the style gate
python3 publication/style_gate.py

# Check traceability across evidence artifacts
python3 publication/traceability_check.py
```

## License

This project is dual-licensed under MIT and Apache-2.0. See [LICENSE-MIT](LICENSE-MIT)
and [LICENSE-APACHE](LICENSE-APACHE).

The xxHash algorithm and specification are by Yann Collet, available under
BSD-2-Clause at <https://github.com/Cyan4973/xxHash>.
