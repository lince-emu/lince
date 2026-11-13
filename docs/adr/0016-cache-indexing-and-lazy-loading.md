# ADR-0016: Cache indexing and lazy loading

- **Status:** Proposed (needs PoC-6)
- **Date:** 2026-11-13
- **Related:** doc 02 §4.2, §5, §6, §13; ADR-0014, ADR-0015

## Context
A memory-mapped index avoids reading the cache at boot, but mmap page faults are disk I/O in
disguise and must not happen on render-critical threads.

## Requirements
Bounded startup; no faults or I/O on render-critical threads; useful prefetch for warm caches.

## Alternatives
1. Read the full index into memory at boot.
2. **mmap the index; access it only from the cache I/O thread; render threads use in-memory
   tables and enqueue resolves.**
3. Per-key files located by hash path.

## Decision
Option 2 plus a bounded **boot manifest** (≤ 4,096 keys, ≤ 64 KiB) that schedules P2 prefetch
after boot, and scene-cluster prefetch during play (doc 02 §13).

## Technical justification
Keeps startup O(1) and confines unpredictable latency to a thread nobody waits on.

## Advantages
Warm caches still help (prefetch) without being required.

## Disadvantages
First use of a cached artifact costs one resolve round-trip (Tier 0 covers it).

## Risks
Prefetch I/O competing with game file streaming. Mitigation: low I/O priority; governor control.

## Validation plan
Ledger: zero `CacheResolve` events on critical threads; resolve latency distribution; prefetch
hit ratio.

## Conditions for revisiting
Resolve latency P99 > one frame interval on SSD hosts.
