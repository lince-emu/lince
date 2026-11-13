# Architecture Decision Records

An ADR records one significant decision: context, alternatives, the choice, and how we will
know if it was wrong. ADRs are immutable once **Accepted**; a change of mind creates a new ADR
that **Supersedes** the old one.

Statuses: `Proposed` · `Accepted` · `Rejected` · `Superseded by ADR-XXXX`.
`Proposed (needs PoC-N)` means the decision cannot responsibly be accepted before that
prototype reports.

## Index

| ID | Title | Status |
|----|-------|--------|
| [0000](0000-project-license.md) | Project license | Proposed — owner decision |
| [0001](0001-rust-toolchain-and-structure.md) | Rust toolchain and project structure | Accepted (Rust) / Proposed (details) |
| [0002](0002-build-system.md) | Build system: Cargo workspace + xtask | Proposed |
| [0003](0003-host-platform-support.md) | Host platform support | Proposed |
| [0004](0004-guest-cpu-model.md) | Guest CPU architecture model | Proposed |
| [0005](0005-interpreter-and-jit.md) | Interpreter and JIT strategy | Proposed |
| [0006](0006-gpu-abstraction.md) | GPU abstraction boundary | Proposed |
| [0007](0007-host-graphics-api.md) | Host graphics API: Vulkan with capability tiers | Proposed (needs PoC-5) |
| [0008](0008-guest-gpu-command-architecture.md) | Guest GPU command architecture | Proposed (needs PoC-9) |
| [0009](0009-shader-ir.md) | Ampere-native shader IR with explicit dependencies | Proposed |
| [0010](0010-shader-execution-fallback.md) | Tier-0 shader execution: GPU interpreter | Proposed (needs PoC-2) |
| [0011](0011-ubershader-strategy.md) | Ubershader strategy: program-generic shaders | Proposed (needs PoC-3) |
| [0012](0012-specialized-shader-compilation.md) | Specialized shader compilation tiers | Proposed |
| [0013](0013-pipeline-creation.md) | Non-blocking pipeline creation | Proposed (needs PoC-5) |
| [0014](0014-cache-architecture.md) | Layered content-addressed cache | Accepted (principles) |
| [0015](0015-cache-persistence-format.md) | Cache persistence format | Proposed (needs PoC-6) |
| [0016](0016-cache-indexing-and-lazy-loading.md) | Cache indexing and lazy loading | Proposed (needs PoC-6) |
| [0017](0017-threading-and-scheduling.md) | Threading and scheduling model | Proposed |
| [0018](0018-background-work-prioritization.md) | Frame-aware background work governor | Proposed (needs PoC-8) |
| [0019](0019-memory-management.md) | Guest and host memory management | Proposed |
| [0020](0020-texture-processing.md) | Texture processing | Proposed |
| [0021](0021-resource-streaming.md) | Resource streaming and uploads | Proposed |
| [0022](0022-gpu-synchronization.md) | GPU synchronization | Proposed |
| [0023](0023-frame-pacing.md) | Frame pacing and presentation | Proposed (needs PoC-10) |
| [0024](0024-hle-architecture.md) | HLE architecture | Proposed |
| [0025](0025-switch1-switch2-boundaries.md) | Switch 1 / Switch 2 component boundaries | Proposed |
| [0026](0026-foreign-code-policy.md) | Foreign (non-Rust) code policy | Proposed |
| [0027](0027-observability.md) | Observability: frame ledger and tracing | Proposed (needs PoC-7) |
| [0028](0028-testing-strategy.md) | Testing strategy | Proposed |
| [0029](0029-no-keys-no-firmware.md) | No console keys and no firmware, including user-provided ones | **Accepted** |

## Template

```markdown
# ADR-XXXX: Title

- **Status:** Proposed
- **Date:** YYYY-MM-DD
- **Deciders:**
- **Related:** ADR-…, doc …, PoC-…

## Context
## Requirements
## Alternatives
## Decision
## Technical justification
## Advantages
## Disadvantages
## Risks
## Validation plan
## Conditions for revisiting
```
