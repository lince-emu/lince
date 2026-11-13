# ADR-0012: Specialized shader compilation tiers

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** doc 03 §3, §5, §7; ADR-0009, ADR-0013, ADR-0018

## Context
After Tier 0 runs a program correctly, native code must replace it quickly (Tier 1) and later
optimally (Tier 2), without competing with the frame for resources.

## Requirements
Translation off render-critical threads; priority by measured fallback cost; persisted results;
discovery-time start.

## Alternatives
1. One optimized compile per program.
2. **Two native tiers:** Tier 1 (light passes, program-generic, fast-link) and Tier 2 (full
   optimization, specialized constants, LTO).
3. Profile-guided Tier 2 using runtime data (later extension of 2).

## Decision
Option 2, with 3 as a later extension. Compile requests carry: keys, dependency snapshot,
priority class (P0 no-fallback, P1 currently on Tier 0, P2 predicted/discovered, P3 tier-up,
P4 maintenance), and a deadline hint. Translation never reads live state. Tier-1 creation may
use `VK_PIPELINE_CREATE_DISABLE_OPTIMIZATION_BIT` where PoC-5 shows a net win.

## Technical justification
Splitting speed-to-native from best-native keeps Tier-0 time short without paying full
optimization latency first.

## Advantages
Short Tier-0 exposure; best steady-state performance.

## Disadvantages
Up to three implementations alive per program; more compile work in total.

## Risks
Total compile CPU competes with the game. Mitigation: governor (ADR-0018).

## Validation plan
Measure time-on-Tier-0 per program (P50/P99), compile CPU per minute, and frame impact under
PoC-8 governor policies.

## Conditions for revisiting
Tier 1 turnaround P99 > 500 ms on mid hosts, or Tier 2 adds < 5% over Tier 1 on average.
