# ADR-0027: Observability — frame ledger and tracing

- **Status:** Partially accepted. **B1 scope** (logging) accepted (maintainer, 2026-11-14);
  the ledger stays Proposed until PoC-7's ARM run.
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
reports. Diagnostics are one system, not a set of per-problem traces switched by environment
variables: every subsystem emits typed events into the same rings.

**B1 scope:** the logging part. `lince-trace` records named events (an unknown SVC, a stub)
once per name into a per-thread buffer drained off the critical threads, with no I/O on
render-critical threads (ARCH-03); the records use the ledger's event shape so the ledger
added later reads the same data.

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
