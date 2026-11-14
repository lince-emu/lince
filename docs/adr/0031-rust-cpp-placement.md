# ADR-0031: Rust/C++ placement refinements

- **Status:** Accepted (maintainer, 2026-11-13, in chat)
- **Date:** 2026-11-13
- **Deciders:** maintainer
- **Amends:** ADR-0030 (placement details only; its decision is unchanged)
- **Related:** ADR-0005, ADR-0019, ADR-0021, doc 04 §2, `docs/research/rust-cpp-split.md`,
  PoC-11

## Context
ADR-0030 puts the guest CPU and shader subsystems in C++ behind a C ABI and keeps threads,
I/O, memory and synchronization in Rust (ARCH-52). PoC-11 measured the boundary at about one
non-inlined call in either direction (Rust → C++ 0.82 ns, C++ → Rust 1.20 ns, Windows x64).
The study `docs/research/rust-cpp-split.md` checked every crate in doc 04 against that split
and found three places where the crate map conflicts with ADR-0030's own rules or would
duplicate work across the languages.

## Requirements
Keep the ADR-0030 rules (ARCH-50–54) true for every crate; never split a hot loop or a shared
mutable structure across the boundary; one machine-code emitter per host ISA, not one per
language.

## Alternatives
1. Leave doc 04 as is and resolve each conflict when the crate is written.
2. **Record the placement now**, so the first `[c++]` crate (`lince-cpu-a64`, B1) and
   `lince-mem` are designed against it.

## Decision
Option 2.

1. **Executable code memory belongs to `lince-mem` (Rust, `[unsafe]`).** It reserves code
   memory, maps it writable, makes it executable, flushes the instruction cache on hosts that
   need it, and enforces W^X. C++ JITs emit only into writable buffers they are given and
   never call the OS (ARCH-52). Publication of compiled code (atomic patching of entry points,
   ADR-0005) is done by Rust, which owns the threads that race.
2. **The CPU header exposes a fault lookup.** `lince-cpu-a64`'s C header provides one function
   that maps a faulting host address to its block, guest PC and access kind, so the fastmem
   fault handler in `lince-mem` (ADR-0019) can resolve a fault without knowing JIT internals.
   That function is async-signal-safe: no allocation, no locks, read-only access to metadata
   that Rust publishes before running the code it describes.
3. **No second machine-code emitter in Rust.** The MME64 macros in `lince-gpu-cmd` are
   interpreted with memoization first (doc 01 E11). If the ledger later shows macro execution
   costs frame time, a JIT reuses the C++ emitters through a shared, lower-level `[c++]`
   code-generation crate below both the CPU and GPU crates (ARCH-21 still holds); creating
   that crate needs its own ADR, as ADR-0030 requires for a third C++ component.

Two further rules from the study apply to both `[c++]` crates:
- The interpreter, like the JIT, accesses guest memory inline through the base pointer Rust
  provides; the per-access callback is the slow path only.
- One decoder per guest ISA, in C++; Rust tools decode through the C API, never with a second
  decoder.

## Technical justification
Each point follows from ADR-0030's rules rather than changing them: OS calls stay in Rust,
the boundary carries coarse calls and slow-path callbacks, and code generation stays in C++.
PoC-11 shows the boundary is cheap for that call shape; what would be expensive is a hot loop
or shared structure split across it, which these rules prevent.

## Advantages
- One W^X and code-memory policy for every JIT.
- The fault handler and the JIT stay decoupled through one documented function.
- No duplicated x86-64/AArch64 encoders.

## Disadvantages
- `lince-mem` grows a code-memory API before the first JIT exists.
- The MME64 path starts without a JIT.

## Risks
- The fault lookup must stay async-signal-safe as the JIT evolves. Mitigation: the header
  states the constraint; a test calls it from a signal/VEH handler under ASan.
- Memoization alone may not keep macro execution off the frame budget. Mitigation: the ledger
  measures it; the shared code-generation crate is the planned next step.

## Validation plan
The B1 interpreter (T-033) uses inline guest memory access; the first JIT tier uses
`lince-mem` code buffers and the fault lookup; ledger counters for MME64 execution time per
frame.

## Conditions for revisiting
A JIT needs an OS facility per compiled block that a Rust-provided buffer cannot serve, or
the fault lookup cannot be made async-signal-safe.
