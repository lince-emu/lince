# Code Structure: Folders, Modules and Files

Where code goes and how it is organized. Ids (`STRUCT-xx`) are cited in reviews. The workspace
layout (which crates exist) is in [doc 04 §2](../04-rust-architecture.md#2-workspace-layout).

## 1. Crate template

Every crate follows this shape. Folders that would be empty are not created.

```text
crates/lince-<name>/
├── Cargo.toml              # [lints] workspace = true; no unused dependencies
├── src/
│   ├── lib.rs              # crate docs + public API re-exports + module declarations. No logic.
│   ├── error.rs            # the crate's error enum(s)
│   ├── <concept>.rs        # one module per responsibility (noun: index.rs, segment.rs)
│   ├── <concept>/          # submodules of <concept>.rs when it grows
│   │   ├── <part>.rs
│   │   └── tests.rs        # large unit-test suites for <concept> (optional)
│   ├── platform.rs         # only crates with OS/arch-specific code (PAT-08): safe neutral API
│   └── platform/
│       ├── windows.rs
│       └── unix.rs
├── tests/                  # integration tests: public API only
│   └── <behavior_area>.rs
├── benches/                # criterion benchmarks: <hot_path>.rs
└── fuzz/                   # cargo-fuzz targets for parsers of untrusted bytes
```

Example for the cache:

```text
crates/lince-cache/src/
├── lib.rs          # pub use store::{Store, StoreConfig}; pub use key::*; …
├── error.rs
├── key.rs          # ArtifactKey, layer kinds, hashing
├── record.rs       # segment record framing + validation
├── segment.rs      # append-only segment files
├── index.rs        # mmap hash index
├── index/
│   └── growth.rs   # background resize + generation swap
├── writer.rs       # single writer thread
├── resolver.rs     # cache I/O thread, resolve requests
├── compaction.rs
└── manifest.rs
```

## 2. Rules

| Id | Rule | Sev. |
|----|------|------|
| STRUCT-01 | `lib.rs` contains crate docs, `mod` declarations and `pub use` of the public API only. No functions, no types. | MUST |
| STRUCT-02 | Module files use the `foo.rs` + `foo/` layout. **No `mod.rs` files.** | MUST |
| STRUCT-03 | Modules are named after **responsibilities** (nouns of the domain). Forbidden names: `utils`, `helpers`, `misc`, `common` (inside a crate), `stuff`, `types`, `manager`. | MUST |
| STRUCT-04 | **Files ≤ 500 lines** excluding tests (soft); **800** hard limit → split by responsibility. | MUST |
| STRUCT-05 | **Private by default.** Items are `pub(crate)` unless part of the crate's API; `pub` items must be re-exported from `lib.rs` or be reachable through a re-exported type. | MUST |
| STRUCT-06 | **No glob imports** (`use foo::*`) except `use super::*` in test modules and enum variants inside a single large `match`-heavy function. | MUST |
| STRUCT-07 | Import order: `std`/`core`/`alloc` → external crates → workspace crates (`lince_*`) → `crate::`/`super::`. One `use` group per block, separated by blank lines. | SHOULD |
| STRUCT-08 | **Item order in a file:** module docs → imports → constants → public types → private types → `impl` blocks (next to their type) → free functions → `#[cfg(test)] mod tests`. | SHOULD |
| STRUCT-09 | Unit tests live in `#[cfg(test)] mod tests` at the bottom of the file, or in `<module>/tests.rs` when larger than ~200 lines. Integration tests in `tests/` use only the public API. | MUST |
| STRUCT-10 | Platform-specific code lives only under `platform/` with a safe, platform-neutral API; no `#[cfg(windows)]` scattered through domain code. | MUST |
| STRUCT-11 | One primary type per file when the type has substantial behavior; small closely related types may share a file. | SHOULD |
| STRUCT-12 | **Cyclic module dependencies inside a crate are not allowed**; when two modules need each other, extract the shared part. | MUST |
| STRUCT-13 | File and module names: `snake_case`, singular nouns (`segment.rs`, not `segments.rs`), no crate-name prefix repeated (`lince_cache::cache_index` ✗ → `lince_cache::index`). | MUST |
| STRUCT-14 | Generated code (instruction tables, SPIR-V blobs) goes in `src/generated/` with a header comment naming its generator; never edited by hand. | MUST |

## 3. Module documentation template

Every module starts with a doc comment in this shape (STRUCT-15, MUST for modules with
concurrency or `unsafe`, SHOULD otherwise):

```rust
//! Memory-mapped open-addressing index of a cache store.
//!
//! # Role
//! Maps 128-bit artifact keys to record locations without deserializing anything at open.
//!
//! # Threads
//! Read by the cache I/O thread only (never by render-critical threads — ARCH-01).
//! Written only by the store's writer thread.
//!
//! # Invariants
//! - Load factor ≤ 0.7; growth happens off-thread with a generation swap.
//! - Entries carry a check field; a torn entry reads as a miss.
//!
//! # References
//! docs/02-cache-architecture.md §4.2 · ADR-0016
```

## 4. Where new code goes — decision list

1. Does it emulate guest behavior? → the crate of that guest component.
2. Is it host-API specific (Vulkan)? → `lince-render-vk` only (ARCH-22).
3. Is it OS-specific? → that crate's `platform/` (STRUCT-10).
4. Is it reused by ≥ 3 crates and domain-free? → `lince-common` (only after the rule of three, QUAL-60).
5. Is it a developer tool? → `tools/` or `xtask`, never an emulation crate.
6. None of the above → ask before creating a new crate (requires an ADR update of doc 04 §2).
