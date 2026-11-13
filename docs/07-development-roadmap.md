# 07 — Development Roadmap

Status: Proposed · Ordered by architectural dependency and risk, not by visible features.
Durations are deliberately omitted until PoC results exist.

| Milestone | Content | Exit criteria | Risk addressed |
|-----------|---------|---------------|----------------|
| **M0 Research & specification** | This dossier; license decision (ADR-0000); research notes for NVN2/services in `docs/research/` | ADR-0000 accepted; PoC plans approved | Wrong foundations |
| **M1 Risk PoCs (parallel)** | PoC-2, PoC-5, PoC-6, PoC-7, PoC-4 (standalone, no emulator needed) | Reports written; ADR-0007/0009/0010/0013/0015/0016/0027 accepted or revised | Shader & cache viability |
| **M2 Core Rust architecture** | Workspace, xtask, CI, `lince-common`, `lince-trace` (ledger), `lince-sched`, `lince-mem` (fastmem) | CI green on Windows/Linux; Miri/loom on primitives | Structure, unsafe discipline |
| **M3 CPU foundation** | A64 decoder (full A78C set), interpreter, coverage tool, baseline JIT x64 | Coverage tool shows 0 undecoded encodings on test executables; differential tests pass | CPU correctness |
| **M4 System & HLE** | Loader (decrypted ExeFS/RomFS), kernel, IPC, core services, Switch 2 services skeleton | A title reaches its first GPU submission with no auto-stub | Boot path |
| **M5 GPU command processing** | Channels, GPFIFO, MME64 (interp + JIT + memo), Ampere engines, packets, null backend, capture tool; PoC-9 | Captures replay deterministically; throughput target | GPU-thread saturation |
| **M6 Vulkan backend & shader IR** | Device/capability tiers, memory, resources, sync, present; IR + Ampere front-end + SPIR-V (from PoC-4) | First correct frames with Tier 1 only (blocking allowed in this milestone only, flagged) | Integration |
| **M7 Generic shader execution** | Tier 0 integrated (from PoC-2); discovery-time pre-decode | Zero pipeline waits outside no-fallback list | Core charter goal |
| **M8 Specialized compilation & pipelines** | Tier 1/2, pipeline slots, atomic publish, governor (PoC-8) | Time-on-Tier-0 distributions; governed compile | Background interference |
| **M9 Cache** | `lince-cache` production version, layers L0–L8, prefetch, compaction | doc 02 §14 answers demonstrated by tests | Startup & persistence |
| **M10 Resources & memory** | Texture pipeline (GPU ASTC/BC), streaming, eviction, read traps | Scenarios 11, 16 targets | Non-shader stutter |
| **M11 First playable titles** | 1–3 titles fully playable | PoC-1 and PoC-10 pass on them | End-to-end proof |
| **M12 Continuous optimization** | Tier C CPU JIT, Tier 2 tuning, parallel recording, PGO | Regression gates; targets tightened | Performance headroom |
| **M13 Compatibility expansion** | More titles, services, mesh/tensor coverage, video decode (third-party code ADR, ADR-0030) | Compatibility list with ledger reports | Breadth |
| **M14 Switch 1** | AArch32, Maxwell front-end, MME, NVN services (ADR-0025) | Switch 1 titles boot on shared core | Second target |

## Rules

- No milestone ships a feature that violates the charter (draw discard, mandatory precompile,
  full cache load). M6's temporary blocking compile is the only exception, removed in M7.
- Frontend/UI work stays minimal (CLI) until M11.
- Every milestone ends with a ledger-based performance report, even before games are playable
  (replays, synthetic loads).
