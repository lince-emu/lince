# ADR-0021: Resource streaming and uploads

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** ADR-0019, ADR-0020, ADR-0022

## Context
Uploads, downloads and evictions caused frame spikes in Lichii (rows 131, 133, 139): buffer
downloads at every fence, evictions with GPU waits.

## Requirements
No synchronous uploads on the render path beyond a small budget; no GPU waits for eviction;
downloads only when the guest actually reads.

## Alternatives
1. Synchronous staging per resource.
2. **Ring-buffer staging + dedicated transfer queue + timeline semaphores**, budgets per frame.
3. Host-visible device memory everywhere (ReBAR) — opportunistic only.

## Decision
Option 2, with ReBAR used opportunistically for small, frequent updates. Eviction is deferred to
timeline values (never waits). Downloads are lazy via read traps (ADR-0019) with an explicit
synchronous path only when the guest reads data the GPU wrote.

## Technical justification
Separates bulk transfers from rendering and turns waits into dependencies.

## Advantages
Upload cost overlaps rendering; eviction is free on the critical path.

## Disadvantages
Memory held longer (deferred destruction); transfer-queue ownership transfers to manage.

## Risks
Guest reads of GPU-written data are inherently synchronous. Mitigation: measure frequency per
title; prefetch read-back when patterns repeat.

## Validation plan
Scenarios 11, 16 (doc 05): upload/eviction time on critical threads (target ≈ 0), staging ring
stalls.

## Conditions for revisiting
Staging ring stalls > 0.1% of frames.
