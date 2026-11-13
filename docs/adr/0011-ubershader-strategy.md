# ADR-0011: Ubershader strategy — program-generic shaders, not configuration ubershaders

- **Status:** Proposed (needs PoC-3)
- **Date:** 2026-11-13
- **Related:** doc 01 §2 (Dolphin), doc 03 §2, §4.2; ADR-0010, ADR-0012

## Context
Dolphin's ubershaders interpret a bounded fixed-function configuration (TEV). Switch 2 programs
are arbitrary code, so a Dolphin-style ubershader is not transferable. The underlying idea —
make state a runtime input instead of a compile-time constant — still applies.

## Requirements
Minimize the number of host compiles per guest program; avoid recompiling when only guest
state changes.

## Alternatives
1. Classic generated ubershaders covering many guest programs (infeasible for arbitrary code).
2. **Program-generic Tier-1 shaders:** one host shader per guest program; dependencies
   (texture types, formats, cbuf-derived constants) read at runtime from parameter blocks.
3. Fully specialized shaders per (program, state) (yuzu-style environments).

## Decision
Option 2 for Tier 1; option 3 only in Tier 2, where the extra compiles run in the background
and pay off in speed. Classic ubershaders are rejected except for emulator-internal
fixed-function paths (blits, clears, format conversions).

## Technical justification
Most variant explosion comes from state dependencies, not from code; moving them to runtime cuts
compile count roughly to the number of distinct programs (hypothesis, PoC-3).

## Advantages
Fewer compiles; cache hits across state changes.

## Disadvantages
Runtime branching on parameters costs some GPU time in Tier 1.

## Risks
Some dependencies are structural (indirect branch tables from cbufs) and cannot be runtime
values. Mitigation: those remain variant-key dependencies.

## Validation plan
PoC-3: on recorded sessions, count host compiles per guest program with options 2 vs. 3, and
Tier-1 GPU overhead per draw.

## Conditions for revisiting
Tier-1 overhead > 25% vs. specialized on representative shaders.
