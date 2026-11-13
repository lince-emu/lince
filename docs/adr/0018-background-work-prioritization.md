# ADR-0018: Frame-aware background work governor

- **Status:** Proposed (needs PoC-8)
- **Date:** 2026-11-13
- **Related:** charter §8; ADR-0012, ADR-0016, ADR-0017

## Context
Background compilation can starve guest or GPU threads (CPU), fight for memory bandwidth, and
add GPU load; "more workers" is not automatically better.

## Requirements
Background work shrinks when frame deadlines are at risk; P0/P1 work keeps progressing;
obsolete speculative work is cancelled.

## Alternatives
1. Fixed worker count (cores − N).
2. OS priorities only.
3. **Governor:** a control loop driving per-class concurrency and token budgets from frame
   signals.

## Decision
Option 3. Inputs every frame: frame-time history vs. target, critical-thread busy ratios,
critical-thread run-queue delay, GPU busy ratio, queue depths, memory pressure, thermal
throttling hints. Outputs: concurrency per class (P0 unlimited up to cores, P1 ≥ 1, P2–P4 may
drop to 0), I/O permission for cache writer/compaction, Tier-0 → Tier-1 urgency boost when Tier-0
GPU cost is high. Speculative jobs carry a generation and are cancelled when stale. OS
priorities are set as a secondary measure.

## Technical justification
Only frame-level signals can tell whether background work is harmful right now.

## Advantages
Adapts to host size, thermal state and game load.

## Disadvantages
Control-loop tuning; possible oscillation.

## Risks
Starving tier-up so Tier 0 stays too long. Mitigation: P1 floor; cost-weighted priority.

## Validation plan
PoC-8: scenarios 9, 14, 15 (doc 05) with fixed-count vs. governed policies; compare P99.9 and
time-on-Tier-0.

## Conditions for revisiting
Governed policy not better than the best fixed count on ≥ 2 host classes.
