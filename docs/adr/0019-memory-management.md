# ADR-0019: Guest and host memory management

- **Status:** Proposed
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

## Technical justification
Fastmem is the established high-performance approach; large blocks avoid allocation churn seen
in Lichii (row 114).

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
