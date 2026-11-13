# ADR-0004: Guest CPU architecture model

- **Status:** Proposed
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
Option 2. One table-driven A64 decoder shared by interpreter and JITs, generated from a single
instruction description table in C++ (ADR-0030). Six emulated application cores, each a guest thread
scheduler context; the Horizon-compatible scheduler (ADR-0024) decides which guest thread runs.
PAC instructions behave as on hardware with keys that make authentication a no-op (identity)
— to be validated against titles; MTE tag operations treat all tags as zero.

## Technical justification
A single decode table removes divergence between execution tiers. Features are bounded by the
real core, not open-ended.

## Advantages
No missing-instruction crashes; coverage is measurable with a tool.

## Disadvantages
More upfront decoder work.

## Risks
Undocumented reliance on PAC failure or MTE faults. Mitigation: trace tool logs any
authentication-failure path the game expects.

## Validation plan
`lince-a64-coverage` tool over executables (counts undecoded encodings); differential tests
interpreter vs. JIT on random instruction sequences; comparison with hardware traces where the
team has legal access.

## Conditions for revisiting
A title depending on PAC/MTE faulting behavior.
