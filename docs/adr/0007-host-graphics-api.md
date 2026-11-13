# ADR-0007: Host graphics API — Vulkan with capability tiers

- **Status:** Proposed (needs PoC-5)
- **Date:** 2026-11-13
- **Related:** doc 01 §7, doc 03, ADR-0013

## Context
The shader strategy needs bindless descriptors, buffer device addresses, subgroup control,
fragment barycentrics, mesh shaders, pipeline libraries or shader objects, and per-pipeline
binaries. None of these is universal.

## Requirements
Windows and Linux first; maximum control over compilation; graceful fallback.

## Alternatives
1. **Vulkan 1.3** via `ash`.
2. D3D12 (Windows only; no direct equivalent of pipeline libraries for all cases).
3. OpenGL (no control over compilation; poor threading).
4. wgpu (portable but hides pipeline creation and lacks required features).
5. Metal (macOS only; later).

## Decision
Vulkan is the **first** host graphics API, not the only one: Direct3D 12 and Metal backends are
planned later behind the same packet boundary (ADR-0006), each with its own ADR and capability
tiers. Vulkan 1.3 baseline with **capability tiers** detected at startup:

| Tier | Requirements (beyond 1.3 core) | Pipeline model |
|------|-------------------------------|----------------|
| V3 | shader objects or GPL with fast link; EDS1/2/3 broad; descriptor indexing/buffer; BDA; subgroup size control; fragment barycentric | Fewest pipelines; full Tier 0 |
| V2 | GPL; EDS1/2; descriptor indexing; BDA | Stage libraries; Tier 0 variants for missing barycentrics |
| V1 | 1.3 core + descriptor indexing + BDA | Monolithic pipelines, `FAIL_ON_PIPELINE_COMPILE_REQUIRED` probing; Tier 0 reduced |

Mesh shaders, cooperative matrix and pipeline binary (`VK_KHR_pipeline_binary`, used for L6
persistence) are separate feature flags (D-008). Below V1: unsupported.

## Technical justification
Vulkan is the only cross-platform API exposing the compilation controls the charter needs.

## Advantages
One backend for Windows and Linux; best features on current desktop drivers.

## Disadvantages
macOS needs another path (Metal backend, later); driver variance requires a test matrix.

## Risks
A vendor's GPL/shader-object implementation is slow or buggy. Mitigation: per-driver
override table populated by PoC-5 measurements, never by folklore.

## Validation plan
PoC-5: per vendor (NVIDIA, AMD proprietary, RADV, Intel/ANV, Intel Windows, NVK), measure
stage-library compile, fast link, LTO link, shader-object creation, pipeline-binary load latency.

## Conditions for revisiting
A tier-V1 majority among users, or a new API capability that removes pipeline creation entirely.
