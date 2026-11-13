# ADR-0020: Texture processing

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** doc 01 E12; ADR-0021

## Context
Switch 2 titles use ASTC heavily (including 8192×8192×68 virtual textures) and block-linear
(GOB) layouts with a Switch 2 swizzle (Lichii rows 69, 94, 99–107). Desktop GPUs mostly lack
ASTC; decoding to RGBA8 multiplies memory (≈ 23 GB in one case).

## Requirements
New textures ready the frame they are used where possible; VRAM bounded; no CPU decode threads
saturating cores; correct swizzles.

## Alternatives
1. CPU ASTC decode to RGBA8.
2. **GPU ASTC decode + GPU BC re-encode** (BC7 quality / BC1–BC3 fast), native ASTC where the
   host supports it.
3. Keep ASTC compressed and decode in shaders (cost on every sample).

## Decision
Option 2 as default, native ASTC when supported. De-swizzle on the GPU in the same pass. Huge
images use sparse residency or trimmed mip chains with an explicit budget; never silent
"unbacked" data without a ledger event. A CPU decoder exists as a reference for tests.

## Technical justification
Matches the fixes Lichii converged on, designed in from the start rather than retrofitted.

## Advantages
Low CPU cost; ~4–8× less VRAM than RGBA8.

## Disadvantages
BC re-encode quality loss vs. ASTC (must be documented); GPU time per upload.

## Risks
Real-time BC7 encode cost. Mitigation: BC1/BC3 for opaque/alpha fast path, BC7 when the
governor has GPU headroom (re-encode later).

## Validation plan
Scenario 11 (doc 05): upload latency, VRAM, GPU time per MB; image diff vs. reference decoder.

## Conditions for revisiting
GPU encode time > 1 ms per frame at P99 during streaming.
