# ADR-0022: GPU synchronization

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** doc 01 E13, E21; ADR-0008, ADR-0021, ADR-0023

## Context
NVN2 uses semaphores, syncpoints and QMD release slots for intra- and cross-queue sync; one title
produced 5,500–7,000 fences/s and ~10,500 unsatisfied semaphore acquires/s (doc 01).

## Requirements
Correct guest-visible ordering; no `vkQueueWaitIdle`/`vkDeviceWaitIdle` in gameplay; fences cheap.

## Alternatives
1. Host fence per guest release.
2. **Timeline semaphores** as the single host sync primitive; guest semaphores/syncpoints
   modeled as guest-memory values with host-side waiter lists; releases batched.
3. CPU-side emulation of all releases.

## Decision
Option 2. Guest releases that only feed other guest GPU work stay on the GPU timeline; CPU
waiters are woken by a single fencing thread watching timeline values. Empty fences are
coalesced. Global waits are forbidden by lint (`xtask ci` greps for them outside teardown).

## Technical justification
Timeline semaphores express ordering without per-event objects and without CPU round trips.

## Advantages
Cheap sync; no global stalls; clear ledger events.

## Disadvantages
Guest semantics (max-releases, equal-value waits) must be modeled exactly.

## Risks
Missed wake-ups or value regressions (Lichii rows 62, 70). Mitigation: property tests of the
semaphore model; latch waiters on release.

## Validation plan
Unit/property tests; ledger: `FenceWait` time on critical threads; zero global waits.

## Conditions for revisiting
Driver timeline-semaphore performance issues on a tier-1 platform.
