# ADR-0026: Foreign (non-Rust) code policy

- **Status:** Superseded by [ADR-0030](0030-hybrid-rust-cpp.md)
- **Date:** 2026-11-13
- **Related:** charter §2.1; ADR-0001, ADR-0015, ADR-0000

## Context
The charter forbids additional languages in the core without compelling reason, but some needs
(Vulkan loader, video codecs, validation tools) have no viable Rust implementation.

## Requirements
Core emulation stays Rust; exceptions are explicit, isolated and license-compatible.

## Alternatives
1. Rust only, no exceptions (blocks video decoding, SPIR-V validation).
2. **Exceptions by ADR**, isolated behind crate boundaries and features.
3. Free use of C/C++ libraries.

## Decision
Option 2. Categories:
- **Allowed without ADR:** system libraries the OS provides (Vulkan loader, OS APIs).
- **Allowed in dev/CI only:** `spirv-tools` validation, RenderDoc integration.
- **Requires ADR:** any C/C++ linked into release builds (candidates: FFmpeg or libvpx/dav1d for
  video decoding).
Each exception lives in a dedicated `lince-ffi-*` crate with a safe API and a feature flag.

## Technical justification
Keeps memory safety and build simplicity where it matters while acknowledging real gaps.

## Advantages
Honest, auditable dependency surface.

## Disadvantages
Some features wait for an ADR.

## Risks
FFmpeg licensing and build complexity. Mitigation: evaluate pure-Rust VP9/H.264 progress first;
dynamic loading of system FFmpeg as an option.

## Validation plan
`cargo deny` + a CI check that only `lince-ffi-*` crates have `links`/build-script C compilation.

## Conditions for revisiting
Mature Rust replacements appear.
