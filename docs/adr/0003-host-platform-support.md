# ADR-0003: Host platform support

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** ADR-0005, ADR-0007, ADR-0019

## Context
Platform choice affects JIT backends, memory mapping APIs, graphics APIs and test load.

## Requirements
Cover most players early; keep the matrix small enough to test continuously.

## Alternatives
Tier lists considered for Windows x64, Linux x64 (incl. handheld PCs), Linux/Windows ARM64,
macOS ARM64, Android ARM64.

## Decision
- **Tier 1 (CI + perf gates):** Windows 10/11 x64, Linux x64 (Mesa RADV/ANV/NVK and NVIDIA proprietary).
- **Tier 2 (CI build + smoke tests, after M6):** Linux ARM64, Windows ARM64.
- **Tier 3 (later, separate ADRs):** macOS ARM64 (needs a Metal or translation-layer decision), Android.

## Technical justification
x64 desktops are the largest audience and have the best Vulkan extension coverage
(pipeline libraries, shader objects, mesh shaders). ARM64 hosts enable native-execution
experiments (ADR-0005) but have weaker GPU driver coverage.

## Advantages
Focus; strongest Vulkan features first.

## Disadvantages
Mac and Android users wait.

## Risks
Designing x64-only assumptions into memory or JIT code. Mitigation: AArch64 host target builds
in CI from M2 even before it is supported.

## Validation plan
CI matrix; per-platform frame-ledger runs on reference hosts.

## Conditions for revisiting
Contributor capacity for a platform plus a Vulkan feature-coverage survey for it.
