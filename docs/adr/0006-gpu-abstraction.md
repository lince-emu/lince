# ADR-0006: GPU abstraction boundary

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** ADR-0007, ADR-0008, doc 04

## Context
Emulators often build a generic "renderer interface" with OpenGL/Vulkan/D3D backends. That
abstraction tends to hide exactly the controls that matter for stutter (pipeline creation,
compile flags, descriptor models).

## Requirements
Full access to modern Vulkan features; testable guest GPU logic without a host GPU; room for a
second backend later without rewriting guest GPU code.

## Alternatives
1. Generic multi-API HAL (wgpu-style).
2. **Two-level boundary:** guest GPU emulation produces backend-neutral *draw/dispatch packets*
   (resolved state, resource references, shader keys); a single Vulkan backend consumes them.
3. Guest GPU code calls Vulkan directly.

## Decision
Option 2. The packet format is the only interface between `lince-gpu-*` and `lince-render-vk`.
No trait objects on the hot path; the backend is a concrete type selected at compile time.

## Technical justification
Keeps the guest side testable (packets can be recorded and asserted) and lets the backend use
every Vulkan feature directly. A future Metal/D3D12 backend implements the same packet consumer.

## Advantages
Clear seam for capture/replay tools; no lowest-common-denominator API.

## Disadvantages
A second backend means a second full consumer implementation.

## Risks
Packet format bloat copying too much state. Mitigation: packets reference state blocks by id
and dirty masks.

## Validation plan
Replay tool consumes recorded packets; packet build time per draw tracked (target ≤ 1.5 µs,
vs. ~6.4 µs draw setup measured by Lichii, doc 01 E10).

## Conditions for revisiting
A second backend becomes a funded goal.
