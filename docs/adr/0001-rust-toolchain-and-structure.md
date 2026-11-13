# ADR-0001: Rust toolchain and project structure

- **Status:** Accepted (Rust as the language, by charter) · Proposed (toolchain details and structure)
- **Date:** 2026-11-13
- **Related:** doc 04 §2, §5, ADR-0002, ADR-0026

## Context
Rust is the only pre-approved technology. We must choose toolchain policy, edition, crate
structure and `unsafe` discipline so that a large emulator stays fast, modular and reviewable.

## Requirements
Predictable performance; explicit ownership; isolation of `unsafe`; parallel compilation;
crates small enough to test in isolation; no circular dependencies.

## Alternatives
1. Single crate with modules.
2. **Workspace of focused crates** with a strict dependency direction (doc 04 §2).
3. Workspace + nightly toolchain for unstable features (portable SIMD, naked functions).

## Decision
Option 2, stable toolchain pinned in `rust-toolchain.toml` (current stable at bootstrap,
updated deliberately), edition 2024. `#![forbid(unsafe_code)]` everywhere except crates marked
`[unsafe]` in doc 04, which use `deny(unsafe_op_in_unsafe_fn)` and `// SAFETY:` comments.
Workspace-level lints: `clippy::all`, `clippy::pedantic` (selectively allowed),
`missing_docs` for public items in core crates.

## Technical justification
Crate boundaries enforce architecture (a GPU crate cannot reach into CPU internals), speed up
incremental builds, and confine `unsafe` to auditable places. Nightly is not needed for
anything identified so far; JIT and fastmem work on stable with `core::arch` and platform APIs.

## Advantages
Enforced layering; parallel builds; easy targeted testing and fuzzing.

## Disadvantages
More `Cargo.toml` boilerplate; cross-crate inlining requires care (`#[inline]`, LTO).

## Risks
Over-splitting early. Mitigation: create crates only when their first real code lands.

## Validation plan
`cargo deny` for dependency direction and licenses; build-time tracking in CI.

## Conditions for revisiting
A required feature exists only on nightly with a measured benefit > 10% on a critical path.
