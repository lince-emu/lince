# ADR-0010: Tier-0 shader execution — GPU interpreter

- **Status:** Proposed (needs PoC-2) — the most important open decision of the project
- **Date:** 2026-11-13
- **Related:** doc 03 §3–§6; ADR-0011, ADR-0012, ADR-0013

## Context
The charter requires correct execution of unseen shaders without waiting for compilation and
without discarding draws. Host driver compilation of a new program is unavoidable if the
program becomes host shader code. Therefore the first execution must treat the guest program
as *data*.

## Requirements
No per-program host compile on first use; correct results identical to compiled tiers; GPU cost
bounded and measured; works on capability tier V2+.

## Alternatives
1. Wait for compilation (status quo).
2. Skip draws (rejected by charter).
3. Classic ubershaders (not applicable to arbitrary programs; ADR-0011).
4. **GPU interpreter of pre-decoded SASS micro-ops**, global pipelines compiled once per driver.
5. CPU software rasterizer for unseen shaders (infeasible at Switch 2 resolutions).

## Decision
Option 4, provisionally, with the design in doc 03 §4: micro-op streams, per-lane PCs with
min-PC reconvergence, explicit quad derivatives, vertex pulling, barycentric-based
interpolation, bindless resources, BDA global memory, phased barriers for compute,
numeric-class-keyed fragment output variants. Classes Tier 0 cannot handle go on the
**no-fallback list** (doc 03 §6).

## Technical justification
It is the only candidate that satisfies "no compile on first use" and "no discarded work" at the
same time.

## Advantages
Eliminates first-use compile stalls by construction; independent of driver compile speed.

## Disadvantages
High GPU cost while active; a second implementation of all shader semantics; complex.

## Risks
1. Fragment derivatives and helper lanes under divergence.
2. Warp ops on hosts that cannot force subgroup size 32.
3. GPU cost spikes when many new programs appear at once.
4. Interpreter shader itself slow to compile on some drivers (one-time per driver).

## Validation plan
PoC-2 success criteria: bit-exact results vs. Tier 1 on the test corpus for supported classes;
GPU time per draw within an agreed factor of Tier 1 (factor recorded per instruction class);
global pipeline set compiles in the background on first emulator launch per driver.

## Conditions for revisiting
If PoC-2 shows Tier 0 cannot cover fragment shaders correctly on V2/V3 hosts, fall back to
"discovery-time Tier 1 + bounded waits" and update the charter's guarantees honestly.
