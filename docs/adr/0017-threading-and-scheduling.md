# ADR-0017: Threading and scheduling model

- **Status:** Partially accepted. **B2 scope** accepted (maintainer, 2026-11-14); the rest
  stays Proposed.
- **Date:** 2026-11-13
- **Related:** doc 04 §3; ADR-0005, ADR-0008, ADR-0018

## Context
Emulation has a few latency-critical pipelines (guest cores → GPU front → back → submit →
present; audio) and a lot of throughput work (compiles, cache, decoding). Mixing them in one
pool causes priority inversion and stutter.

## Requirements
Critical threads never wait on background work; bounded queues; measurable wakeup latency; no
general-purpose async runtime in emulation code.

## Alternatives
1. Async runtime (`tokio`) for everything.
2. Work-stealing pool (`rayon`) for everything.
3. **Dedicated role threads + governed worker pools**, explicit queues.

## Decision
Option 3 per doc 04 §3. Guest cores are real host threads (6). Background pools are created by
`lince-sched` with priority classes P0–P4 and a concurrency limit set by the governor.
Critical-path queues are bounded SPSC rings; blocking uses parking, not spinning (short bounded
spins allowed where measured).

**B2 scope:** six host threads, one per guest core, created at process start and joined at
teardown, each parking while the guest threads it hosts are idle; and a minimal `lince-sched`
that owns one service-I/O role thread fed by a bounded queue, for blocking service calls
(ADR-0024 B2 scope; maintainer, 2026-11-14). The other role threads, worker pools, priority
classes and the governor stay out.

## Technical justification
Latency-critical stages need predictable scheduling that async executors and work stealing do
not guarantee.

## Advantages
Clear ownership; easy profiling per role; no hidden executor latency.

## Disadvantages
More threads than cores on small hosts (6 guest + ~6 role threads).

## Risks
Oversubscription on 4–6 core hosts. Mitigation: guest-core threads park when the guest thread
they host is idle; host-core budget table per CPU size (PoC-8).

## Validation plan
Ledger wakeup-latency events; run-queue delay sampling; scenario 14 (doc 05).

## Conditions for revisiting
Measured wakeup latency P99 > 200 µs on critical threads.
