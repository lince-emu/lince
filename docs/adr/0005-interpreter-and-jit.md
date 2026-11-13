# ADR-0005: Interpreter and JIT strategy

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** ADR-0004, ADR-0017, ADR-0030

## Context
CPU translation can cause stutter just like shader compilation: a guest thread hitting new code
waits for its translation. The charter forbids avoidable interruptions from JIT work too.

## Requirements
New code must run within microseconds; hot code must reach high quality; optimizing work must
run off guest threads. The CPU subsystem is written in C++ inside `lince-cpu-a64`
(ADR-0030); it is an independent implementation, so existing JITs such as dynarmic are
neither used nor consulted as code (LEGAL-20).

## Alternatives
1. Interpreter only.
2. Single-tier optimizing JIT (Cranelift or LLVM) on the guest thread.
3. **Tiered:** interpreter → baseline block JIT (template emission) → optimizing region JIT
   on background workers.
4. Native execution on ARM64 hosts (NCE-style), with traps for SVCs and special instructions.
5. Persistent translation cache (PPTC-style).

## Decision
Option 3 as the primary design:
- **Tier A** interpreter: reference semantics, debugging, rarely executed code.
- **Tier B** baseline JIT: per-block template emission, target compile cost of a few
  microseconds per block, on the guest thread.
- **Tier C** optimizing JIT over hot regions on governed background workers; published by
  atomic patching of block entry points.

Both tiers are C++ (ADR-0030). The code emitter (an own assembler or a third-party C++
assembler library) is not decided; a third-party emitter needs its own ADR (ADR-0030).
The Rust candidates considered earlier (`dynasm-rs`, `iced-x86`, Cranelift) no longer apply.

Option 4 is an ARM64-host experiment after M6. Option 5 is deferred until frame-ledger data
shows boot-time JIT cost matters (expected small with tier B).

LLVM is rejected for runtime use: compile latency and a large C++ dependency.

## Technical justification
Mirrors the shader tiering: always-available correct execution, background optimization. Tier B
bounds the on-thread cost; Tier C gets quality without blocking.

## Advantages
No JIT stalls by construction; tiers share the decoder.

## Disadvantages
Three execution engines to keep consistent; code patching needs careful synchronization.

## Risks
Tier B quality too low for CPU-heavy titles until Tier C lands; Tier C compile time for big
regions. Mitigation: region size limits; measure.

## Validation plan
Microbenchmarks: block compile latency P99.9; guest MIPS per tier; ledger counts of
`JitCompile` on guest threads per frame (target: P99.9 ≤ 0.2 ms per frame).

## Conditions for revisiting
Tier B compile latency P99.9 > 50 µs per block, or Tier C cannot match a 2× gain over Tier B
on hot loops.
