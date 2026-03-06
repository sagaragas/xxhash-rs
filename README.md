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
tests/              # Integration and parity tests
benchmarks/         # Benchmark harness and artifacts
publication/        # Rewrite study and publication materials
docs/               # Additional documentation
```

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
cargo test --workspace
```

## License

This project is dual-licensed under MIT and Apache-2.0. See [LICENSE-MIT](LICENSE-MIT)
and [LICENSE-APACHE](LICENSE-APACHE).

The xxHash algorithm and specification are by Yann Collet, available under
BSD-2-Clause at <https://github.com/Cyan4973/xxHash>.
