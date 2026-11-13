# ADR-0006: GPU abstraction boundary

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** ADR-0007, ADR-0008, doc 04

## Context
Emulators often build a generic "renderer interface" with OpenGL/Vulkan/D3D backends. That
abstraction tends to hide exactly the controls that matter for stutter (pipeline creation,
compile flags, descriptor models).

## Requirements
Full access to modern Vulkan features; testable guest GPU logic without a host GPU. Vulkan is
the first backend; Direct3D 12 and Metal are planned later (maintainer, 2026-11-13) and must
not require rewriting guest GPU code, the shader IR or the cache format.

## Alternatives
1. Generic multi-API HAL (wgpu-style).
2. **Two-level boundary:** guest GPU emulation produces backend-neutral *draw/dispatch packets*
   (resolved state, resource references, shader keys); a single Vulkan backend consumes them.
3. Guest GPU code calls Vulkan directly.

## Decision
Option 2. The packet format is the only interface between `lince-gpu-*` and the backend crate
(`lince-render-vk` first; later `lince-render-d3d12`, `lince-render-metal`). Packets carry no
host-API types or concepts. No dynamic dispatch per draw on the hot path. How a build that
contains two backends (e.g. Vulkan and Direct3D 12 on Windows) selects one at startup is decided
when the second backend starts.

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
Work on the second backend starts (selection mechanism, packet review against that API).
