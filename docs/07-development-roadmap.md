# 07 — Development Roadmap

Status: Proposed · Ordered by architectural dependency and risk, not by visible features.
Durations are deliberately omitted until PoC results exist.

## Boot-first track (current priority)

The milestones below stay the architecture plan, but work is now ordered by the shortest path
to the first commercial title on screen. Charter work that does not block booting (Tier 0,
production cache, governor, streaming) moves after B5. Every phase ends with something that
runs, not with a report.

Commercial content is always a **decrypted dump made by the user from a game they own**
(ADR-0029). Lince never decrypts, never ships keys or firmware, and no game content enters the
repository or CI. Test programs are written by the team: Rust `aarch64-unknown-none` binaries
packed as NRO by `xtask`, so no third-party homebrew toolchain is needed.

| Phase | Builds | Exit criterion (demonstrated, not reported) | ADRs to accept first |
|-------|--------|---------------------------------------------|----------------------|
| **B0 Base** | T-003 workspace + CI, `lince-common`, `lince-trace` | `cargo xtask ci` green | 0001, 0002 (done) |
| **B1 Hello homebrew** | `lince-loader` (NRO), `lince-mem` flat guest address space, `lince-cpu-a64` decoder + interpreter for the instructions the test programs use, `lince-kernel` SVC dispatch (memory, exit, `OutputDebugString`), `lince-app` CLI `lince run <file>` | Our NRO prints "hello" on the host terminal; differential tests of every implemented instruction | 0004, 0005 (interpreter part), 0019 (address space part), 0024 (SVC boundary) |
| **B2 Kernel and IPC** | Threads (one host thread per guest core, simple scheduler), handles, sync objects, events, TLS; CMIF/TIPC framework; `sm:`, `set:`, `fs` (host directory), `time`, `hid` stubs that log | Multi-threaded test NRO talks to services over IPC; coverage tool lists every unimplemented SVC/command | 0017 (core threads only), 0024 |
| **B3 Commercial loader** | NPDM, NSO segments (decompress), rtld/subsdk chain, RomFS from a user-supplied extracted directory, `am`/`applet` lifecycle, `nvdrv` and `vi` skeletons, Switch 2 services skeleton in `lince-hle-nx2` | A user's dumped title runs its own code until its first GPU submission, with every stub logged by name (M4 exit) | 0025, 0029 (done) |
| **B4 GPU commands** | GPFIFO, channels, syncpoints/semaphores, Ampere 3D/compute/DMA methods recorded to a null backend; capture tool | Title runs past its first `present` without hanging; capture replays deterministically | 0006, 0008, 0022 (minimum) |
| **B5 First frame** | Vulkan device + present, render targets, buffers, simple textures (swizzle, BC/ASTC on CPU), shader decode SASS → IR → SPIR-V with **blocking compile allowed** (flagged, M6 rule) | The title's first screen (logo or menu) is visible in a window | 0007, 0009, 0020 (CPU path) |
| **B6 Input and progress** | Keyboard/gamepad → `hid`, audio to a null sink, the services the title asks for next, fixes driven by the coverage tool | The title reaches gameplay and accepts input | — |

After B6 the plan resumes at M7 (Tier 0, no pipeline waits), M8, M9 and M10, then the M11
"playable with no stutter" exit criteria.

### What pauses

PoC and research tasks not on this path pause until they block a phase: T-020, T-021, T-022,
T-024 and further cache/ledger PoCs. T-014 (SASS corpus) and T-018 (decoder) return in B5;
T-017 (Tier-0 harness) returns after B6. T-026 (Rust↔C++ boundary) is needed in B1, because
`lince-cpu-a64` is a `[c++]` crate (ADR-0030).

### Main risks

- **Switch 2 public knowledge is thin** (NVN2 driver protocol, Switch 2 services, Ampere SASS
  encodings). B3–B5 can stall on undocumented behavior; mitigation is a coverage tool that
  names every missing piece, and research notes per gap (`docs/08-switch2-hardware.md`).
- **Firmware-free system data** (fonts, shared data, applets) may block menus in B5/B6; see
  the research in `docs/research/system-data-and-applets.md`.
- **Interpreter speed**: a title may be too slow to reach its first frame in reasonable time
  before the JIT (M3). Measure in B3; pull the baseline JIT forward if needed.

## Architecture milestones

| Milestone | Content | Exit criteria | Risk addressed |
|-----------|---------|---------------|----------------|
| **M0 Research & specification** | This dossier; license decision (ADR-0000); research notes for NVN2/services in `docs/research/` | ADR-0000 accepted; PoC plans approved | Wrong foundations |
| **M1 Risk PoCs (parallel)** | PoC-2, PoC-5, PoC-6, PoC-7, PoC-4 (standalone, no emulator needed) | Reports written; ADR-0007/0009/0010/0013/0015/0016/0027 accepted or revised | Shader & cache viability |
| **M2 Core Rust architecture** | Workspace, xtask, CI, `lince-common`, `lince-trace` (ledger), `lince-sched`, `lince-mem` (fastmem) | CI green on Windows/Linux; Miri/loom on primitives | Structure, unsafe discipline |
| **M3 CPU foundation** | A64 decoder (full A78C set), interpreter, coverage tool, baseline JIT x64 | Coverage tool shows 0 undecoded encodings on test executables; differential tests pass | CPU correctness |
| **M4 System & HLE** | Loader (decrypted ExeFS/RomFS), kernel, IPC, core services, Switch 2 services skeleton | A title reaches its first GPU submission with no auto-stub | Boot path |
| **M5 GPU command processing** | Channels, GPFIFO, MME64 (interpreter + memoization; JIT only if measured, ADR-0031), Ampere engines, packets, null backend, capture tool; PoC-9 | Captures replay deterministically; throughput target | GPU-thread saturation |
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
