# ADR-0014: Layered content-addressed cache

- **Status:** Accepted (principles and layers, maintainer, 2026-11-13) — storage engine decided separately in ADR-0015
- **Date:** 2026-11-13
- **Related:** doc 02; doc 01 E17–E19; ADR-0015, ADR-0016

## Context
Existing Switch emulators load or rebuild their caches at boot, so startup grows with cache size
(doc 01 §6). The charter requires the opposite: the cache is optional and invisible.

## Requirements
Charter §4 in full: no full load at startup; misses never block; corruption never blocks;
versioning without migration; size-independent startup and gameplay.

## Alternatives
1. Per-title monolithic files loaded at boot (yuzu/Ryujinx lineage).
2. Append-only log read at boot with compaction (Lichii cache v2).
3. **Layered content-addressed store with an on-disk index and lazy resolution off the render
   thread.**

## Decision
Option 3 with the six principles of doc 02 §1 and the layers L0–L8 of doc 02 §2.

## Technical justification
Content addressing removes update/migration logic; an index removes boot-time parsing; lazy
resolution on dedicated threads removes I/O from rendering.

## Advantages
O(1) startup; corruption is a cache miss; emulator and driver updates degrade gracefully.

## Disadvantages
More moving parts than a single file; storage engine to implement or adopt.

## Risks
Index/segment bugs. Mitigation: everything checksummed and re-derivable; fuzzing the reader.

## Validation plan
PoC-6 startup and lookup benchmarks across cache sizes; fault-injection tests (truncation,
bit flips, killed writer).

## Conditions for revisiting
Principles: never (they are charter requirements). Layers: if a layer's hit rate or value does
not justify its storage (PoC data).
