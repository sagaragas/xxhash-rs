# Legal and Clean-Room Boundary

## Overview

This project is a **clean-room Rust reimplementation** of the xxHash family of
hash functions. It is informed by the published xxHash specification and
BSD-licensed reference library material, and validated against the upstream C
reference as a behavioral oracle.

## Source Material Boundary

### Permitted sources (BSD-2-Clause)

The Rust hash algorithm implementation in this repository is derived from:

- The **published xxHash specification** describing the algorithm behavior,
  constants, and processing steps.
- The **xxHash reference library** (`xxhash.h` / `xxhash.c`), which is released
  under the BSD-2-Clause license and provides the authoritative algorithm
  definition.

These materials are BSD-licensed and may be freely used to inform a compatible
reimplementation.

### Excluded sources (GPL)

The upstream `xxhsum` CLI tool is released under **GPLv2**. This project does
**not** incorporate, vendor, translate, or derive from any GPL-licensed CLI
source code. Specifically:

- No `xxhsum` CLI source files are present in this repository.
- No GPL-licensed help text, error messages, comments, or implementation logic
  has been copied into this codebase.
- CLI behavioral compatibility was achieved through **black-box observation**
  of the upstream CLI's input/output behavior, not through source-level
  translation.

### External reference checkout

The upstream xxHash C reference source (including the GPL CLI) is maintained in
a **separate checkout outside this repository** for use as a behavioral oracle
during parity testing and benchmarking. It is never vendored into or committed
to this repository.

This checkout is local to the developer's machine and external to this repository.

## Attribution

- **xxHash** was created by Yann Collet.
- The xxHash library and specification are available at
  <https://github.com/Cyan4973/xxHash> under the BSD-2-Clause license.
- The `xxhsum` CLI tool is available under the GPLv2 license.

## This Project's License

This Rust reimplementation is released under the **MIT OR Apache-2.0** dual
license. See `LICENSE-MIT` and `LICENSE-APACHE` for details.
