# ADR-0030: Hybrid Rust + C++ implementation

- **Status:** Accepted (maintainer, 2026-11-13); interop cost and toolchain details validated by
  PoC-11 (backlog T-026)
- **Date:** 2026-11-13
- **Deciders:** maintainer
- **Supersedes:** ADR-0001 in part (the "Rust as the only language" clause, charter R1);
  ADR-0026 (foreign code policy)
- **Related:** ADR-0002, ADR-0005, ADR-0009, ADR-0017, ADR-0027, doc 04, doc 09, PoC-11

## Context
The charter made Rust the only implementation language (R1, ADR-0001) and allowed foreign code
only as isolated third-party exceptions (ADR-0026). The maintainer decided to write two
subsystems in C++: the guest CPU (A64 decoder, interpreter and JIT tiers) and the shader
toolchain (Ampere SASS decoding, Lince shader IR, SPIR-V emission, Tier-0 micro-op
generation). Both are code-generation-heavy subsystems where C++ has mature low-level
emitters and where the team wants direct control over code layout. Everything else stays Rust.

## Requirements
- Rust keeps ownership of the architecture: threads, scheduling, memory, cache, GPU command
  processing, Vulkan, HLE and composition.
- The Rust/C++ boundary is narrow, stable, auditable and cheap enough for the hot calls
  (JIT callbacks into the kernel and memory, one translation request per shader).
- All invariants (AGENTS.md §2, `ARCH-xx`, `LEGAL-xx`) apply to C++ exactly as to Rust.
- One build command on Windows x64 and Linux x64 (ADR-0002 unchanged).

## Alternatives
1. Rust only (ADR-0001 as written).
2. **Rust core, C++ for the CPU and shader subsystems, C ABI boundary, Cargo-driven build.**
3. C++ core with Rust in the periphery.
4. Per-subsystem language choice, one ADR each.
5. Boundary through the `cxx` crate instead of a C ABI.
6. CMake on top, Rust crates imported through Corrosion.

## Decision
Option 2.

**Where C++ lives.** Two C++ components, each inside one Rust crate that owns the build and
exposes a safe Rust API:

| Crate | Replaces (doc 04 §2) | C++ contents |
|-------|----------------------|--------------|
| `lince-cpu-a64` `[c++]` | `lince-cpu-a64`, `lince-jit` | A64 decoder, interpreter, baseline and optimizing JIT, code buffers |
| `lince-shader` `[c++]` | `lince-shader-ir`, `lince-shader-sass-ampere`, `lince-shader-spirv`, `lince-shader-interp` | shader IR and passes, Ampere SASS front-end, SPIR-V emitter, Tier-0 micro-op generation |

Merging the former crates keeps the C ABI at the domain boundary instead of between C++
components that share types. Later front-ends (A32, Maxwell SASS; ADR-0025) are added inside
these components. No other crate contains C++ without a new ADR.

**Crate layout.** `crates/<crate>/cpp/include/<crate>.h` (the C header: the only contract),
`cpp/src/` (C++ sources), `build.rs` (compiles with the `cc` crate), `src/ffi/bindings.rs`
(Rust declarations generated from the header and checked in; `xtask ci` verifies they match),
`src/ffi.rs` (the safe wrapper), other `src/*.rs` (the crate's public Rust API, no `unsafe`).

**Boundary rules.**
- Only `extern "C"` functions, plain structs of fixed-size integer fields, and opaque handles
  cross the boundary. No C++ types, templates, `std::` types or Rust types cross it.
- Every exported function is `noexcept`. C++ is built without exceptions and RTTI; errors
  cross as status codes. Rust callbacks called from C++ must not unwind.
- Ownership is explicit per function: the side that allocates frees. Large buffers (SPIR-V
  words, code blocks) are written into memory provided by the caller.
- Guest memory, I/O, threads, synchronization with other subsystems and frame-ledger events
  are provided by Rust through a callback table passed at creation. C++ never creates threads,
  never performs file or network I/O, and keeps no mutable global state.
- Translation input is the recorded dependency snapshot passed by value (ARCH-11).

**Language and toolchain.** C++20. Clang on both platforms (`clang-cl` on Windows); the
minimum version is pinned in `xtask` by PoC-11. `clang-format` and `clang-tidy` (warnings as
errors) run in `cargo xtask ci`; ASan and UBSan run on the C++ test suite in CI. C++ is tested
through its Rust API with `cargo test` (unit, differential and fuzz tests, ADR-0028); a C++
test framework needs a separate dependency decision.

**Build.** Cargo + `xtask` remains the only entry point (ADR-0002). C++ is compiled by
`build.rs` with the `cc` crate; no CMake.

**Third-party foreign code** (replaces ADR-0026). Lince's own C++ is governed by this ADR.
Third-party C or C++ code linked into release builds still needs its own ADR, license entry in
`THIRD-PARTY-NOTICES.md` (LEGAL-33), and isolation behind a feature flag. Allowed without an
ADR: system libraries the OS provides (Vulkan loader, OS APIs). Allowed in dev/CI only:
`spirv-tools` validation, RenderDoc integration. Code emitters for the JIT (e.g. assembler
libraries) count as third-party code and need that ADR.

## Technical justification
A C ABI is the most stable and auditable boundary between two languages, needs no extra
runtime support, and makes every crossing visible in one header. Grouping the C++ code into
two components keeps the number of boundaries at two. Building through `cc` keeps one build
system and one CI command. Keeping threads, I/O and memory in Rust preserves the scheduling
and frame-delivery guarantees (ARCH-01, ARCH-04) without duplicating them in C++.

## Advantages
- The CPU and shader subsystems can use mature C++ emitters and low-level control.
- The architecture, ownership model and invariants stay in one language.
- Two narrow, documented boundaries instead of many.

## Disadvantages
- Two languages, two sets of lints and formatters, and a mandatory clang toolchain.
- Miri and `loom` do not cover C++; sanitizers and differential tests must compensate.
- Every boundary call is opaque to the optimizer (no cross-language inlining by default).
- The C ABI is less ergonomic than `cxx`: bindings and handle types are maintained by hand.

## Risks
- **Clean-room pressure.** The closest existing C++ code (dynarmic, yuzu/Eden shader recompiler)
  is easy to consult and to reproduce. LEGAL-20/21 apply without change; reviews check C++
  against them explicitly.
- **Boundary cost on the hot path** (JIT memory and SVC callbacks). Mitigation: fastmem
  accesses are emitted inline by the JIT; only slow paths cross the boundary. PoC-11 measures
  the call cost.
- **Windows build friction** (compiler discovery, path length). Mitigation: PoC-11 runs the
  build from a clean checkout on Windows x64 and Linux x64.
- **Unsafe surface in Rust grows.** Mitigation: `unsafe` stays confined to the `src/ffi`
  module (`src/ffi.rs` and `src/ffi/`) of each `[c++]` crate (ARCH-24).

## Validation plan
PoC-11 (backlog T-026): a minimal `[c++]` crate built with `cc` on Windows x64 and Linux x64
from a clean checkout; header-to-Rust binding generation and its CI check; per-call cost of a
Rust→C++ call and of a C++→Rust callback; clang-format, clang-tidy and sanitizer runs in
`xtask ci`.

## Conditions for revisiting
- Measured boundary cost on a critical path that the design cannot amortize.
- Clean build time on CI above the ADR-0002 limit because of C++.
- A third subsystem proposed for C++.
