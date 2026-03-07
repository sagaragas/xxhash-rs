# Rewriting xxHash in Rust: A Clean-Room Reimplementation Study

## Overview

This document describes the design, validation, and performance characteristics of
`xxhash-rs`, a clean-room Rust reimplementation of the xxHash family of hash functions
and the `xxhsum` CLI tool. The project covers all four xxHash variants (XXH32, XXH64,
XXH3_64, XXH3_128), provides both one-shot and streaming APIs, and includes a CLI
with behavioral parity against the upstream C reference.

The work follows a correctness-first methodology: algorithm parity is validated
before benchmarking, and benchmark claims are tied to pinned artifacts rather than
cherry-picked runs.

**Measured revision:** [`a7cd8e5`](https://github.com/sagaragas/xxhash-rs/commit/a7cd8e53bd71c7498b75f8d1857c2f7b609315ab)

**Published article:** [Rewriting xxHash in Rust](https://ragas.dev/blog/rewriting-xxhash-in-rust)

---

## Correctness and Parity

### Algorithm parity

The Rust implementation produces bit-exact output matching the C reference for all
four hash variants. Parity is validated at 508 individual test points spanning:

- **Boundary-length vectors:** lengths 0, 1, 3, 4, 8, 9, 16, 17, 128, 129, 240,
  241, and larger long-input cases for each algorithm.
- **Seeded variants:** both default (seed 0) and non-zero seeds produce
  reference-compatible digests.
- **Streaming equivalence:** the `reset/update/digest` streaming API produces the
  same results as one-shot hashing across multiple chunking patterns. Repeated
  `digest()` calls on unchanged state return stable results, and
  `update(A) -> digest() -> update(B)` matches one-shot hashing on the
  concatenation `A || B`.

All 508 parity tests pass at the measured revision.
([evidence: `publication/evidence/parity_summary.json`](publication/evidence/parity_summary.json))

### SIMD parity

On Apple Silicon (AArch64), the release build exercises NEON-optimized XXH3 long-input
paths. These optimized paths produce bit-exact output matching the scalar fallback
for both XXH3_64 and XXH3_128 on representative large inputs.

The SIMD parity test suite covers streaming variants, derived-secret paths,
and both 64-bit and 128-bit output widths with seed-0 and seeded inputs.
([evidence: `publication/evidence/parity_summary.json`](publication/evidence/parity_summary.json),
category `xxh3_simd_scalar_parity`)

### CLI behavioral parity

The CLI achieves behavioral parity with the upstream `xxhsum` reference for the
validated surface, which includes:

- Algorithm selection via `-H0`/`-H32`, `-H1`/`-H64`, `-H2`/`-H128`, `-H3` flags
- Seed support with correct boundary handling
- File and stdin hashing with argument-order preservation
- GNU and BSD/tagged output formats
- Little-endian output via `--little-endian` and `--tag --little-endian`
- Escaped-filename handling for filenames containing backslash, newline, or
  carriage return
- File-list input via `--files-from` and `--filelist`
- Check mode with `--quiet`, `--status`, `--warn`, `--strict`, and
  `--ignore-missing` policies
- Malformed-line handling and aggregate failure summaries
- Little-endian checksum verification for both GNU and BSD formats

Parity is validated through direct output comparison against the reference binary
across 31 algorithm-selection tests, 69 output-format tests, 53 input-flow tests,
and 355 check-mode tests.
([evidence: `publication/evidence/parity_summary.json`](publication/evidence/parity_summary.json))

---

## Benchmark Methodology

### Harness design

Benchmarks measure end-to-end CLI throughput: each comparator is invoked as an
external process that reads a payload file and produces a digest on stdout. This
captures the full cost profile including process startup, I/O, and hashing, rather
than isolating the hash function in a microbenchmark loop.

This design was chosen because the CLI surface is the user-facing boundary of the
project. In-process microbenchmarks would be useful for optimizing internal hot
paths but would not reflect the experience of a user running `xxhash-rs` as a
command-line tool.

### Comparators

Four comparators are included in every benchmark scenario:

| ID               | Role       | Version                         |
|------------------|------------|---------------------------------|
| `c_xxhsum`       | Reference  | xxhsum 0.8.3 (Yann Collet)     |
| `rust_xxhash_rs` | Subject    | xxhash-rs 0.1.0                 |
| `b3sum`          | Contrast   | b3sum 1.8.3                     |
| `md5`            | Contrast   | macOS system `/sbin/md5`        |

`c_xxhsum` and `rust_xxhash_rs` are parity oracles: the harness verifies that they
produce the same digest before accepting timing samples. `b3sum` and `md5` are
contrast comparators that provide throughput context from different hash families
but are not expected to produce matching digests.

### Scenarios

The benchmark suite covers four scenarios from the declared manifest:

| Scenario       | Algorithm  | Payload  |
|----------------|------------|----------|
| `xxh64-4k`     | XXH64      | 4 KiB   |
| `xxh64-1m`     | XXH64      | 1 MiB   |
| `xxh64-16m`    | XXH64      | 16 MiB  |
| `xxh3-128-1m`  | XXH3_128   | 1 MiB   |

Each scenario uses warmup iterations (discarded) followed by measured iterations.
The summary statistic is the median of measured samples, chosen to reduce sensitivity
to system load spikes.

### Correctness gate

Before timing results are accepted, the harness applies a hard correctness gate:
`c_xxhsum` and `rust_xxhash_rs` must agree on the output digest for each scenario.
All four scenarios pass the correctness gate at the measured revision.
([evidence: `publication/evidence/benchmark_summary.json`](publication/evidence/benchmark_summary.json))

### Claim-readiness requirements

A benchmark run is claim-ready only when it has full matrix coverage (all comparators
ran for all scenarios), the correctness gate passed, raw samples are retained, and
artifact checksums are recorded. The evidence pack includes three pinned claim-ready
runs:

- `run-20260307T024049Z-76539`
- `run-20260307T024049Z-76533`
- `run-20260307T024049Z-76247`

([evidence: `publication/evidence/benchmark_summary.json`](publication/evidence/benchmark_summary.json))

---

## Results

### Throughput summary

The following throughput numbers are median values averaged across three pinned
benchmark runs on an Apple Silicon host (arm64, macOS). These are CLI-level
measurements that include process startup overhead.

**XXH64, 16 MiB payload** (`xxh64-16m`):

| Comparator       | Median throughput |
|------------------|-------------------|
| `c_xxhsum`       | ~3,694 MB/s       |
| `rust_xxhash_rs` | ~3,972 MB/s       |
| `b3sum`          | ~3,965 MB/s       |
| `md5`            | ~532 MB/s         |

At this payload size, process startup is a small fraction of the total time, and the
numbers primarily reflect hash throughput. The Rust implementation, C reference, and
BLAKE3 all land in the same range (~3.7–4.0 GB/s), while MD5 trails at ~532 MB/s.
The Rust and C xxHash numbers are close enough that run-to-run variance could change
their relative order.

**XXH3_128, 1 MiB payload** (`xxh3-128-1m`):

| Comparator       | Median throughput |
|------------------|-------------------|
| `c_xxhsum`       | ~448 MB/s         |
| `rust_xxhash_rs` | ~414 MB/s         |
| `b3sum`          | ~333 MB/s         |
| `md5`            | ~272 MB/s         |

For XXH3_128 at 1 MiB, the C reference leads the Rust implementation by about 8%
(~448 vs ~414 MB/s). Both the C and Rust NEON-optimized XXH3 paths are exercised on
this Apple Silicon host.

**XXH64, 1 MiB payload** (`xxh64-1m`):

| Comparator       | Median throughput |
|------------------|-------------------|
| `c_xxhsum`       | ~565 MB/s         |
| `rust_xxhash_rs` | ~472 MB/s         |
| `b3sum`          | ~424 MB/s         |
| `md5`            | ~306 MB/s         |

At 1 MiB, process startup is a larger fraction of measured time. The C reference
leads the Rust implementation by about 16% (~565 vs ~472 MB/s), though some of that
gap reflects startup and I/O variance rather than pure hash throughput differences.

**XXH64, 4 KiB payload** (`xxh64-4k`):

| Comparator       | Median throughput |
|------------------|-------------------|
| `c_xxhsum`       | ~2.2 MB/s         |
| `rust_xxhash_rs` | ~2.0 MB/s         |
| `b3sum`          | ~1.7 MB/s         |
| `md5`            | ~2.4 MB/s         |

At 4 KiB, process startup overwhelms the hash computation entirely. All comparators
converge to a similar throughput floor (~2 MB/s). These numbers say nothing about hash
performance and are included only to illustrate the startup-dominated regime.

### Interpretation

The CLI-level benchmarks show that `xxhash-rs` delivers throughput in the same
range as the C reference across the measured scenarios. On the largest payload
(XXH64 at 16 MiB), the two are comparable. On XXH3_128 at 1 MiB and XXH64 at 1 MiB,
the C reference leads by 8–16%, though process startup, file I/O, and output
formatting contribute fixed overhead that compresses the apparent gap at smaller
payloads.

The throughput numbers above reflect the full CLI invocation path. The hash core
itself is faster than what these CLI numbers suggest, because process startup, file
I/O, and output formatting contribute fixed overhead that compresses the apparent
throughput gap at smaller payload sizes and inflates it at larger ones.

Applications that embed the hash library directly (bypassing CLI overhead) would
see higher throughput from both implementations, with the fixed startup cost
removed from the measurement.

([evidence: raw samples in `publication/evidence/benchmark_runs/`](publication/evidence/benchmark_runs/))

---

## Limitations

This study has several scope boundaries that readers should keep in mind:

1. **Single-platform benchmarks.** All measurements were taken on a single Apple
   Silicon host (arm64, macOS). Performance characteristics on x86_64 or other
   architectures may differ, particularly for SIMD-accelerated XXH3 paths where
   the SSE2/AVX2 code paths have not been benchmarked.

2. **CLI-level measurement.** The benchmarks measure end-to-end CLI throughput
   rather than isolated hash-core performance. Process startup overhead dominates
   at small payload sizes and partially masks hash throughput differences at
   medium sizes.

3. **Smoke-level sample counts.** The pinned runs use 2 measured iterations per
   comparator per scenario (smoke-level). A production-grade benchmark study
   would use higher sample counts to tighten confidence intervals.

4. **Subset of declared scenarios.** The evidence pack covers 4 of the 8 declared
   benchmark scenarios. The remaining scenarios (xxh32-4k, xxh3-64-4k, xxh3-64-1m,
   xxh3-64-16m) are declared in the manifest but not included in the pinned
   smoke-level runs.

5. **Validated CLI surface.** The CLI parity validation covers the flags, output
   formats, check modes, and input flows listed in the Correctness section. Features
   outside the validated surface (for example, `--benchmark` mode from the upstream
   CLI) are not implemented or tested.

6. **No production deployment evidence.** This implementation has not been tested
   in production workloads. The parity and benchmark evidence demonstrates
   correctness and baseline performance but does not constitute a production
   readiness assessment.

---

## Licensing and Attribution

### Clean-room boundary

This is a clean-room reimplementation. The hash algorithms were implemented from
the published xxHash specification and the BSD-2-Clause-licensed reference library
material (`xxhash.h`/`xxhash.c`). The CLI tool achieves behavioral compatibility
through black-box observation of the upstream `xxhsum` tool's input/output behavior,
without translating or copying any GPL-licensed source code.

The distinction matters because the upstream project has two license regimes:

- **BSD-2-Clause:** the xxHash library and specification, which defines the hash
  algorithms. This material is freely usable and informed the Rust hash core.
- **GPLv2:** the `xxhsum` CLI tool. This source was treated as an external
  behavioral oracle only. No `xxhsum` source files, help text, error messages,
  comments, or implementation logic were incorporated into this repository.

The external C reference checkout (used for parity testing and benchmarking) is
maintained outside this repository and is never vendored into or committed here.

### Attribution

- **xxHash** was created by Yann Collet.
- The xxHash library and specification are available at
  <https://github.com/Cyan4973/xxHash> under the BSD-2-Clause license.
- The `xxhsum` CLI tool is available under the GPLv2 license.
- This Rust reimplementation (`xxhash-rs`) is released under the MIT OR Apache-2.0
  dual license. See [LICENSE-MIT](../LICENSE-MIT) and
  [LICENSE-APACHE](../LICENSE-APACHE).

For the full clean-room boundary and attribution details, see [LEGAL.md](../LEGAL.md).

---

## Reproducibility

### Source and artifacts

The measured revision for all evidence in this study is
[`a7cd8e5`](https://github.com/sagaragas/xxhash-rs/commit/a7cd8e53bd71c7498b75f8d1857c2f7b609315ab).

The evidence pack is committed under `publication/evidence/` and includes:

- **Parity summary:** [`publication/evidence/parity_summary.json`](evidence/parity_summary.json)
  — 508 test results across all hash variants and the CLI surface.
- **Benchmark summary:** [`publication/evidence/benchmark_summary.json`](evidence/benchmark_summary.json)
  — correctness gate results, comparator inventory, and run metadata.
- **Pinned benchmark runs:** [`publication/evidence/benchmark_runs/`](evidence/benchmark_runs/)
  — raw timing samples, manifest checksums, and per-run checksums for three
  claim-ready runs.
- **Claim/evidence map:** [`publication/evidence/claim_map_inputs.json`](evidence/claim_map_inputs.json)
  — structured mapping from each material claim to its supporting artifact
  and pinned revision.
- **Artifact manifest:** [`publication/evidence/artifact_manifest.json`](evidence/artifact_manifest.json)
  — master index of all evidence artifacts with file checksums.
- **Clean-checkout provenance:** [`publication/evidence/clean_checkout_provenance.json`](evidence/clean_checkout_provenance.json)
  — provenance artifact with manifest hashes, validation commands, and produced
  run identifiers proving the cited revision can reproduce the linked evidence.

### Reproducing from a clean checkout

To reproduce the core validation from a clean checkout of the measured revision:

```sh
# Clone and checkout the measured revision
git clone https://github.com/sagaragas/xxhash-rs.git
cd xxhash-rs
git checkout a7cd8e53bd71c7498b75f8d1857c2f7b609315ab

# Build the workspace
cargo build --workspace --release

# Run the parity test suite (use --test-threads=3 for stable concurrency)
cargo test --workspace --all-targets -- --test-threads=3

# Verify the claim/evidence map
python3 publication/claim_map.py --verify

# Run the traceability check
python3 publication/traceability_check.py

# Run the style gate
python3 publication/style_gate.py
```

Benchmark reproduction additionally requires the external C reference binary
(`xxhsum 0.8.3`), `b3sum`, and the benchmark harness scripts. See
`benchmarks/scenarios.json` and `benchmarks/comparators.json` for the full
comparator and scenario declarations.

### Traceability

Every material claim in this document links to a pinned evidence artifact. The
claim/evidence map at `publication/evidence/claim_map_inputs.json` provides a
machine-readable index that maps each claim ID to its evidence path and pinned
revision. No claims reference mutable `latest` pointers.

The traceability tooling can be verified with:

```sh
python3 publication/traceability_check.py
```
