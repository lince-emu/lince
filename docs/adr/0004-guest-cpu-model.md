# ADR-0004: Guest CPU architecture model

- **Status:** Proposed. B1 (doc 07) needs only the part marked **B1 scope** below accepted.
- **Date:** 2026-11-13
- **Related:** doc 08 §1, ADR-0005, ADR-0019

## Context
Switch 2 titles are AArch64 programs for Cortex-A78C and use v8.1–v8.5 features heavily (LSE,
CAS/CASP, LDAPR, PAC, MTE `STG`; doc 01 E1). They request 6 application cores (core mask `0x3f`);
folding cores changed allocation behavior and caused crashes in another project (doc 01 E3).

## Requirements
Decode and execute every instruction shipped titles use; faithful core count and scheduling
semantics; correct memory ordering for atomics and acquire/release loads.

## Alternatives
1. Implement v8.0 and add extensions on demand.
2. **Implement the A78C feature set up front** (v8.2-A baseline + features observed in titles),
   driven by a coverage tool over executables.
3. Model a generic "ARMv8.5 everything" core.

## Decision
Option 2.
- **One decoder for every tier:** a table-driven A64 decoder generated from a single
  instruction description table in C++ (ADR-0030), shared by the interpreter and the JITs, so
  tiers cannot disagree on decoding.
- **v8.0–v8.5 from the first decoder:** LSE, CAS/CASP, LDAPR, PAC and MTE encodings are in the
  table from day one; `lince-a64-coverage` reports any encoding a title uses that the table
  lacks (doc 01 E1). An undecoded encoding is a reported coverage gap, never a guessed one.
- **Six application cores, never folded** (doc 01 E2, E3): each is a guest thread scheduler
  context; the Horizon-compatible scheduler (ADR-0024) decides which guest thread runs.
- **PAC and MTE:** authentication behaves as identity and MTE tag operations treat all tags
  as zero, the approach the Lichii notes describe (doc 01 E1), to be validated against
  titles; a trace event fires on any path that would depend on an authentication failure.
- **Memory ordering:** guest acquire/release and LSE atomics map to host atomics with at least
  the guest's ordering; exclusive pairs (`LDXR`/`STXR`) are emulated with a host
  compare-and-swap on the reserved value (see Risks).
- **Not paced by presentation:** guest cores run on their own host threads and keep the
  guest's time from the emulated clock (counter registers derived from the host monotonic
  clock), never from frames. A guest core blocks only on synchronization the guest itself
  requests (a syncpoint or fence wait, an event, a buffer acquire), never on host
  presentation, the swapchain or host GPU completion; every such wait is bounded and
  recorded in the ledger (ADR-0023). A slow frame on the host can therefore delay what the
  guest sees, but never stops a core that has other work.

**B1 scope:** the decoder table, the six-core model and the ordering rule above; the B1
interpreter implements the instructions the B1 test programs use (backlog T-033).

## Technical justification
A single decode table removes divergence between execution tiers. Features are bounded by the
real core, not open-ended.

Evidence from other projects (doc 01): the Eden/yuzu lineage runs Switch 2 titles on a
`dynarmic` that did not decode the v8.1–v8.5 instructions those titles use by the hundred
thousand, so support was added instruction by instruction as titles crashed (E1); folding the
six cores onto four changed heap selection and caused crashes (E3). Lince starts from the
real core's feature set and measures coverage instead of discovering gaps by crashes.

## Advantages
No missing-instruction crashes; coverage is measurable with a tool.

## Disadvantages
More upfront decoder work.

## Risks
Undocumented reliance on PAC failure or MTE faults. Mitigation: trace tool logs any
authentication-failure path the game expects.

Emulating exclusives with a compare-and-swap on the value misses an intervening write that
restores the same value (ABA), which a hardware monitor would detect. Mitigation: a
differential test of lock-free guest patterns; revisit if a title depends on it.

## Validation plan
`lince-a64-coverage` tool over executables (counts undecoded encodings); differential tests
interpreter vs. JIT on random instruction sequences; comparison with hardware traces where the
team has legal access.

## Conditions for revisiting
A title depending on PAC/MTE faulting behavior.
