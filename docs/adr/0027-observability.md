# ADR-0027: Observability — frame ledger and tracing

- **Status:** Proposed (needs PoC-7)
- **Date:** 2026-11-13
- **Related:** doc 05 §3–§4

## Context
Charter §13.2 requires attributing every frame-time spike. Other projects built ad-hoc traces
per problem (Lichii `nx2_stutter_trace`, `nx2_perf`, many environment variables).

## Requirements
Always-on in release; < 1% frame overhead; correlates CPU, GPU, compile, cache, I/O and
presentation events with frame deadlines; offline analysis.

## Alternatives
1. Logging only.
2. Tracy only.
3. **Own frame ledger (always on) + Tracy (opt-in) + GPU timestamps.**

## Decision
Option 3. Ledger events per doc 05 §3 in per-thread lock-free rings, dumped on demand or on
spike; Tracy zones for deep dives; `lince-frame-analyzer` produces percentile and attribution
reports.

## Technical justification
Tracy is excellent for investigation but not for always-on field data; the ledger is.

## Advantages
User bug reports can include a ledger; regressions classified automatically.

## Disadvantages
Instrumentation discipline required in every subsystem.

## Risks
Overhead creep. Mitigation: PoC-7 budget; CI microbench of event cost.

## Validation plan
PoC-7: event cost, ring sizing, analyzer accuracy on synthetic spikes.

## Conditions for revisiting
Overhead > 1% of frame time.
