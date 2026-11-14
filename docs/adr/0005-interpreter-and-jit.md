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
6. Pluggable backends behind the own IR (own, LLVM, Cranelift). Discarded: see
   [`docs/discarded-ideas/pluggable-jit-backends.md`](../discarded-ideas/pluggable-jit-backends.md).

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

**Pipeline.** Every JIT tier follows one pipeline whose middle is Lince's own:

```text
A64 decoder → LCIR (Lince CPU IR) → own backend → code cache + dispatcher
```

- **LCIR** is the contract between front and back. It is Lince's own IR, designed from the Arm
  specification and Lince's requirements, not modelled on another emulator's IR
  (LEGAL-20/21). It is host-independent, deterministic and serializable as plain data, so
  it can cross the C ABI (ARCH-50) in one call per region and be hashed for the cache.
- **One backend, Lince's own, in C++:** a fast lowering for Tier B and an optimizing one for
  Tier C. No compiler framework (LLVM, Cranelift, …) is used in any tier. A pluggable-backend
  layer was considered and discarded as a speculative abstraction
  ([`docs/discarded-ideas/pluggable-jit-backends.md`](../discarded-ideas/pluggable-jit-backends.md));
  LCIR stays a clean, plain-data contract, so a different backend remains possible later
  without designing for it now.
- **Code cache and dispatcher belong to Lince.** The backend returns position-independent
  code plus an explicit relocation list and the fault metadata ADR-0031 needs; lookup,
  publication (atomic patching of entry points by Rust, ADR-0031), persistence and lazy
  resolution are separate from code generation.

The pipeline is shaped by the three problems the charter targets:
- **Stutter:** regions have a size limit and every pass is linear or near-linear in region
  size, so the worst-case compile time is bounded and known; every compile also has a time
  budget, and a Tier C compile that exceeds it is abandoned, leaving the Tier B code in place.
  Nothing compiles on a guest core in any case.
- **Cache:** a translation is keyed by the hash of the guest code bytes plus the LCIR and
  backend versions, persisted in `lince-cache` and resolved lazily per block (option 5),
  instead of being retranslated at every boot.
- **Performance:** guest registers and flags are explicit LCIR values inside a region, so the
  backend can keep them in host registers across a hot region; memory operations carry their
  size and ordering (plain, acquire, release, exclusive), so ordering is preserved without
  blanket barriers; exits (branch, SVC, exception, interrupt check) are explicit, so guest
  state is precise at every exit.

LCIR v1 is deliberately minimal: only what Tier B needs to lower the B1/B2 instruction set;
optimization-oriented features are added with Tier C.

Tier A interprets the decoded instructions directly and does not use LCIR. Tier B is LCIR
without optimization passes, lowered by the own fast backend; since it runs on workers, the
cost of building LCIR does not reach the guest. LCIR's format is specified in its own design
document before the first JIT tier is implemented. The machine-code emitter inside the own
backend (own assembler or a third-party C++ assembler library) is not decided; a third-party
emitter needs its own ADR (ADR-0030).

Option 4 is an ARM64-host experiment after M6. Option 5 lands with the first JIT tier,
behind the cache's lazy lookup.

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
