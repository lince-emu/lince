# ADR-0019: Guest and host memory management

- **Status:** Partially accepted. **B1 scope** accepted (maintainer, 2026-11-14); the rest
  stays Proposed.
- **Date:** 2026-11-13
- **Related:** doc 01 E13, doc 04 §4; ADR-0021, ADR-0022

## Context
Guest code needs fast loads/stores; the GPU emulation needs to know which guest pages the CPU
or GPU wrote; GPU VA equals CPU VA for registered memory on Switch 2 (doc 08 §2).

## Requirements
Single-instruction guest memory access in JIT code; page-granular write tracking; GPU-side
translation for Tier 0/1/2 shaders; no frequent host allocations in steady state.

## Alternatives
1. Software page-table lookup on every access.
2. **Fastmem:** reserve a host region covering the 39-bit guest space; map backing via shared
   memory objects (Linux `memfd` + `mmap`; Windows placeholder APIs `VirtualAlloc2` /
   `MapViewOfFile3`); faults handled by signal/VEH.
3. Mixed: fastmem with software fallback for special regions.

## Decision
Option 3. Page tracking uses host page protections for CPU-write detection of GPU-cached
ranges, and read traps for GPU-written data not yet downloaded (the lazy read-back approach of
doc 01, Lichii row 150). A GPU-resident page table maps guest GPU VAs to host buffer addresses
for shaders. Host GPU memory comes from large blocks (≥ 64 MiB) with our own sub-allocation.

Write tracking keeps its state per page in atomics, not behind a shared lock, so CPU writes,
GPU-cache invalidation and read-back never serialize on one mutex (doc 01 E13).

**B1 scope:** the guest address space only: a 39-bit space (doc 01 E2) of mapped regions with
permissions, accessed through typed `GuestVa` reads and writes (QUAL-20) by software lookup.
Fastmem, write tracking and the GPU page table come with the JIT and GPU phases.

**B2 scope:** the B1 address space shared by the six guest core threads: typed accesses from
any core, guest atomics and exclusives applied with host atomics at the ordering ADR-0004
requires, and region changes (map, unmap, protect) that never leave another core reading a
half-updated region map. The region map sits behind a reader–writer lock: accesses look a
region up under the read side, and map, unmap and protect take the write side. Page contents
are read and written without that lock, through atomics where the guest asks for ordering
(maintainer, 2026-11-14). Fastmem replaces the lookup in the JIT phase.

**Keep native execution possible.** The guest address-space layout must not rule out native
execution on ARM64 hosts later (ADR-0005 option 4): the guest space is reserved as one
contiguous host range whose base address and guest-page-to-host-page mapping are explicit
parameters, and nothing assumes host pages are 4 KiB (some ARM64 hosts use 16 KiB, against
the guest's 4 KiB; the mismatch is handled by the software fallback of option 3).

## Technical justification
Fastmem is the established high-performance approach; large blocks avoid allocation churn seen
in Lichii (row 114). Lichii also measured lock contention on cache mutexes, 5,500–7,000 fences
per second and 250–330 MB/s of read-back at fences (doc 01 E13); per-page atomic state and
lazy read-back are aimed at those costs.

## Advantages
Fast guest access; precise invalidation; low allocation rate.

## Disadvantages
Platform-specific `unsafe` code; fault handling must be signal-safe.

## Risks
Windows placeholder API edge cases; 16 KiB-page hosts (some ARM64) vs. 4 KiB guest pages.
Mitigation: software fallback path for mismatched page sizes.

## Validation plan
Microbenchmarks per access pattern; fault-rate counters in the ledger; stress tests with
concurrent CPU/GPU writes.

## Conditions for revisiting
Fault-handling overhead visible in frame ledger (> 2% of guest-core time).
