# ADR-0005: Interpreter and JIT strategy

- **Status:** Proposed. B1 (doc 07) needs only the part marked **B1 scope** below accepted.
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
Option 3 as the primary design, with one rule that removes JIT stalls by construction:
**a guest core never compiles.**
- **Tier A** interpreter: reference semantics, the oracle for differential tests, and what
  runs new code immediately. It is fast enough for the short time until Tier B publishes
  (bounded and measured, see Validation).
- **Tier B** baseline JIT: per-block template emission on governed background workers
  (`lince-sched`, ADR-0017), requested the first time a block runs; published by atomic
  patching of the block's entry point. The guest keeps interpreting until then.
- **Tier C** optimizing JIT over hot regions on the same workers at lower priority; published
  the same way.
- **Persistent translations** (option 5, the role PPTC plays in Ryujinx): Tier B/C code is
  stored per executable hash in `lince-cache` and resolved lazily per block through the
  cache index, never loaded in full at boot (AGENTS.md invariant 4). Added after B1, when the
  ledger shows first-run JIT cost.

**B1 scope:** Tier A only, as the reference implementation and test oracle.

Both tiers are C++ (ADR-0030). The code emitter (an own assembler or a third-party C++
assembler library) is not decided; a third-party emitter needs its own ADR (ADR-0030).
The Rust candidates considered earlier (`dynasm-rs`, `iced-x86`, Cranelift) no longer apply.

**Own IR, no compiler framework.** No tier depends on LLVM, Cranelift or any other compiler
framework. Tier C optimizes over Lince's own CPU IR (LCIR), designed from the Arm
specification and Lince's requirements, not modelled on another emulator's IR (LEGAL-20/21).
LCIR exists to fix the three problems the charter targets:
- **Stutter:** every pass is linear or near-linear in region size and regions have a size
  limit, so the worst-case compile time of a region is bounded and known, not left to a
  general-purpose optimizer.
- **Cache:** LCIR is host-independent and deterministic, and emitted code is
  position-independent with an explicit relocation list. A translation is keyed by the hash
  of the guest code bytes plus the LCIR and emitter versions, so it can be persisted and
  resolved lazily per block (option 5) instead of being retranslated at every boot.
- **Performance:** guest registers and flags are explicit values inside a region, so they
  stay in host registers across a hot region; memory operations carry their size and
  ordering (plain, acquire, release, exclusive), so ordering is preserved without blanket
  barriers; exits (branch, SVC, exception, interrupt check) are explicit, so guest state is
  precise at every exit.

Tier B emits directly from the decoded instructions (no IR), so its cost per block stays
minimal; LCIR is only for Tier C. LCIR's format is specified in its own design document
before Tier C is implemented.

Option 4 is an ARM64-host experiment after M6. Option 5 is deferred until frame-ledger data
shows boot-time JIT cost matters (expected small with tier B).

LLVM and Cranelift are rejected for every tier: compile latency that Lince cannot bound, a
large dependency, and no control over the cacheable code format above.

## Technical justification
Mirrors the shader tiering: always-available correct execution, background optimization. The
earlier draft compiled Tier B on the guest thread at "a few µs per block"; a level load that
reaches thousands of new blocks would still spend milliseconds of one frame compiling, which is
the first-run stutter this design exists to remove. Interpreting until a worker publishes keeps
the guest core free of compile work, the same rule AGENTS.md invariant 2 sets for shaders.

Evidence from other projects (doc 01): the Eden/yuzu lineage uses `dynarmic` plus native
execution (NCE) on ARM64 hosts; Ryujinx persists translations (PPTC, R3), and its boot cost
grows with the cache like every other project in the comparison (§6). How `dynarmic` and
ARMeilleure schedule compilation is not yet documented with sources (RECALL only); a research
note should confirm it before the JIT tiers are implemented.

## Advantages
No JIT stalls by construction; tiers share the decoder.

## Disadvantages
Three execution engines to keep consistent; code patching needs careful synchronization.

## Risks
Tier B quality too low for CPU-heavy titles until Tier C lands; Tier C compile time for big
regions. Mitigation: region size limits; measure.

## Validation plan
Microbenchmarks: block compile latency P99.9 on workers; guest MIPS per tier; ledger time a
block spends in Tier A before Tier B publishes (target to be set from the first measurement,
not assumed); ledger `JitCompile` events on guest threads must be zero.

## Conditions for revisiting
Time spent in Tier A before publication shows up as frame-time spikes in the ledger, Tier B
compile latency P99.9 > 50 µs per block, or Tier C cannot match a 2× gain over Tier B on hot
loops.
