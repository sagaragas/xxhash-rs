//! # xxhash-rs
//!
//! Clean-room Rust implementation of the xxHash family of hash functions.
//!
//! Supported algorithms:
//! - **XXH32** – 32-bit hash
//! - **XXH64** – 64-bit hash
//! - **XXH3_64** – XXH3 64-bit variant
//! - **XXH3_128** – XXH3 128-bit variant
//!
//! This implementation is derived from the published xxHash specification and
//! BSD-licensed reference library material. It does not incorporate any
//! GPL-licensed CLI source code.

/// Shared low-level helpers for byte-order reads, rotation, and avalanche.
pub mod helpers;

/// XXH32 algorithm implementation.
pub mod xxh32;

/// XXH64 algorithm implementation.
pub mod xxh64;

/// XXH3 algorithm family (64-bit and 128-bit variants).
pub mod xxh3;

/// Platform-optimized SIMD implementations for XXH3 hot paths.
pub mod xxh3_simd;
