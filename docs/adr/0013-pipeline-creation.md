# ADR-0013: Non-blocking pipeline creation

- **Status:** Proposed (needs PoC-5)
- **Date:** 2026-11-13
- **Related:** doc 01 §7, E14, E19; doc 02 §12; ADR-0007, ADR-0010, ADR-0012

## Context
Asynchronous translation is not enough if pipeline creation stays synchronous (Lichii holds
draws until the driver finishes; doc 01 E14). Vulkan offers several partial remedies with
uneven driver support.

## Requirements
No draw waits for a specialized pipeline when Tier 0 is available; pipeline layouts never
created per draw; driver binaries reusable lazily.

## Alternatives
1. Monolithic pipelines created on workers.
2. Graphics pipeline libraries: stage libraries on workers, fast link (non-LTO), LTO later.
3. Shader objects (`VK_EXT_shader_object`) with dynamic state.
4. Pipeline binaries for persistence.

## Decision
Capability-driven combination:
- **V3 hosts:** shader objects *or* GPL, chosen per driver by PoC-5 data; LTO pipelines for
  Tier 2; `VK_KHR_pipeline_binary` for L6 persistence when the host has that flag, sharded
  `VkPipelineCache` otherwise (ADR-0007, D-008).
- **V2:** GPL stage libraries + fast link (Tier 1), LTO (Tier 2), sharded `VkPipelineCache`.
- **V1:** monolithic pipelines on workers; Tier 0 covers the gap.
- One global bindless pipeline layout family; created at device init.
- All creation on compile workers; publication by atomic swap; render threads never call
  `vkCreate*Pipelines` or `vkCreateShadersEXT`.

## Technical justification
Removes the driver from the render-critical path and exploits each driver's best mechanism.

## Advantages
Driver-agnostic guarantee: critical path never compiles.

## Disadvantages
Several creation paths to test.

## Risks
Driver bugs in GPL/shader objects. Mitigation: per-driver override table, conformance tests in CI.

## Validation plan
PoC-5 microbenchmarks per vendor and a ledger check that `vkCreate*` never appears on
render-critical threads.

## Conditions for revisiting
A host API removing pipeline objects universally, or measured fast-link costs > 1 ms.
