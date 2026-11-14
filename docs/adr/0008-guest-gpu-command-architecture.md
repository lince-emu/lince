# ADR-0008: Guest GPU command architecture

- **Status:** Proposed (needs PoC-9)
- **Date:** 2026-11-13
- **Related:** doc 01 E4, E10, E11, E13; ADR-0006, ADR-0017, ADR-0022

## Context
Lichii measured a single GPU thread at ~100% while the host GPU sat at 20–40% in Mario Kart
World: command processing ~335 ms/s, macros ~200 ms/s, draw setup ~345 ms/s for ~55,000 draws/s.
A later front/back split gave 1.46×. NVN2 drives state through MME64 macros (~1 M calls/s, 96%
exact repeats) and synchronizes queues through semaphores (~10,500 unsatisfied acquires/s).

## Requirements
Throughput of ≥ 100,000 draws/s on mid-range hosts (target, to validate); in-order guest
semantics; cross-channel semaphore correctness; no global stalls.

## Alternatives
1. One GPU thread (yuzu lineage), split later.
2. **Pipelined from day one:** front (pushbuffer parse, MME64, register mirrors) → back (engine
   methods, state, packet build) → Vulkan record/submit thread.
3. Per-channel threads with shared state locks.

## Decision
Option 2.
- **Front:** parses GPFIFO and command lists; runs MME64 macros on an interpreter with call
  memoization keyed by parameters and the data-RAM words read; a macro JIT is added only if
  the ledger shows macro time in the frame, reusing the C++ emitters (ADR-0031); keeps
  register mirrors required by macros; handles semaphore acquires by parking the channel and
  running another.
- **Back:** applies method writes in order; tracks dirty state; builds packets (ADR-0006);
  drops same-value state writes early.
- **Record/submit:** turns packets into command buffers.
- Stages connect through bounded SPSC rings carrying method batches (copied data for
  multi-word methods, since guest memory can be reused).
- Explicit sync points (macro DMA reads, unsatisfied semaphores, wait-for-idle) drain only to
  the dependency, never the whole pipeline.

## Technical justification
Measured evidence that the GPU command path, not shader compilation, limits frame time in a
real Switch 2 title once shaders are handled.

## Advantages
Uses 2–3 cores for GPU emulation; isolates macro cost; easier profiling per stage.

## Disadvantages
Register mirrors duplicate state; ordering bugs are subtle.

## Risks
Sync-point frequency erodes overlap. Mitigation: count sync points per second in the ledger
(PoC-9) and design methods to avoid them.

## Validation plan
PoC-9 on recorded command streams: throughput per stage, sync points/s, mirror verification mode
(compare front mirrors to back registers on every macro read in debug builds).

## Conditions for revisiting
Back stage > 90% busy at target load → add parallel packet recording per render pass.
