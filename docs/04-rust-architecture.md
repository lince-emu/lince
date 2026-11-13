# 04 — Complete Rust Architecture

Status: Proposed · Related ADRs: 0001–0008, 0017–0026

## 1. Shape of the system

```text
                     ┌────────────────────────── lince-app (frontend, config) ─────────────────────┐
                     │                                                                              │
┌──────────── Guest CPU ────────────┐   ┌──────── GPU front ────────┐   ┌──────── GPU back ────────┐
│ 6 guest-core threads              │   │ GPFIFO / pushbuffer parse  │   │ engine methods, state,   │
│ A64 interp + baseline JIT         │──▶│ MME64 macro JIT + memo     │──▶│ draw/dispatch build,     │
│ kernel (SVC), HLE IPC services    │   │ register mirrors           │   │ binding resolution       │
└──────────────┬────────────────────┘   └────────────────────────────┘   └───────────┬──────────────┘
               │ guest memory (fastmem)                                              │ draw packets
               ▼                                                                     ▼
       ┌───────────────┐   ┌──────── Compile workers (governed) ────────┐   ┌──── Vulkan record/submit ───┐
       │ lince-mem     │   │ discovery, translation, SPIR-V, pipelines   │   │ command buffers, sync,      │
       │ page tracking │   └──────────────────────┬─────────────────────┘   │ uploads (transfer queue)    │
       └───────────────┘   ┌──────── Cache I/O + writer ────────┐          └──────────────┬──────────────┘
                           │ resolve, validate, append, compact │                         ▼
                           └────────────────────────────────────┘                 present thread
```

## 2. Workspace layout

```text
lince-emu/
├── Cargo.toml                  # [workspace], shared lints, profiles
├── rust-toolchain.toml         # pinned stable toolchain
├── xtask/                      # build orchestration in Rust (no CMake, no shell scripts)
├── crates/
│   ├── lince-common/           # ids, errors, units, small utilities; no unsafe
│   ├── lince-trace/            # frame ledger, Tracy bridge, counters
│   ├── lince-sched/            # thread roles, priority queues, frame governor
│   ├── lince-mem/              # guest address space, fastmem, page tracking   [unsafe]
│   ├── lince-cpu-a64/          # A64 decoder, interpreter, CPU state
│   ├── lince-jit/              # baseline JIT + optimizing tier, code buffers  [unsafe]
│   ├── lince-kernel/           # Horizon-compatible kernel HLE: threads, sync, SVCs
│   ├── lince-hle/              # IPC (CMIF/TIPC) framework, common services
│   ├── lince-hle-nx2/          # Switch 2 specific services (gsv, vi2, syncpt, …)
│   ├── lince-loader/           # NSO/NPDM/ExeFS/RomFS (already-decrypted content only)
│   ├── lince-gpu-cmd/          # channels, GPFIFO, MME64 JIT, semaphores, syncpoints
│   ├── lince-gpu-ampere/       # Ampere class engines (3D, compute/QMD, DMA, 2D)
│   ├── lince-shader-ir/        # SSA IR, dependency model, passes
│   ├── lince-shader-sass-ampere/ # Ampere SASS decoder → IR, micro-op pre-decoder
│   ├── lince-shader-spirv/     # IR → SPIR-V emitter (also builds interpreter shaders)
│   ├── lince-shader-interp/    # Tier-0 micro-op format + interpreter shader generation
│   ├── lince-pipeline/         # keys, tiers, slots, compile scheduling
│   ├── lince-cache/            # content-addressed store, index, writer, compaction
│   ├── lince-texture/          # swizzle (GOB/block-linear), ASTC/BC codecs (GPU + CPU ref)
│   ├── lince-render-vk/        # Vulkan backend: device, memory, resources, sync, present [unsafe]
│   ├── lince-audio/
│   ├── lince-core/             # system composition, lifecycle
│   └── lince-app/              # CLI first; GUI later
└── tools/
    ├── lince-replay/           # GPU command-stream capture replayer (perf + correctness CI)
    ├── lince-frame-analyzer/   # ledger → percentiles, spike attribution reports
    └── lince-shader-lab/       # offline decode/translate/diff of captured programs
```

Switch 1 later adds `lince-cpu-a32`, `lince-gpu-maxwell`, `lince-shader-sass-maxwell`,
`lince-hle-nx1`. Both SASS front-ends target the same `lince-shader-ir` (ADR-0025).

**Dependency direction** is strictly downward: `lince-app → lince-core → subsystems →
lince-common/trace/sched`. GPU crates never depend on CPU crates; they share only
`lince-mem` interfaces.

## 3. Threading model (no general-purpose async runtime in the core)

| Thread | Count | Priority class | Never does |
|--------|-------|----------------|------------|
| Guest core | 6 (Switch 2 application cores) | High | Disk I/O, compile waits |
| GPU front | 1 per process (channels multiplexed) | High | Rasterizer work, I/O |
| GPU back | 1 (future: N recording threads) | High | Driver compiles, I/O |
| Vulkan submit | 1 | High | Compiles |
| Present | 1 | Highest (time-critical) | Anything but present & pacing |
| Audio | 1 | Highest | Allocation in steady state |
| Compile workers | governed pool (0..N) | Below normal | Touch live guest state |
| Cache I/O + writer | 1–2 | Low, low I/O priority | Anything render-critical |
| HLE service hosts | few | Normal | Block guest cores unnecessarily |

- Communication between pipeline stages uses bounded SPSC/MPSC rings with explicit
  backpressure; blocking waits use futex-style parking, never spin indefinitely.
- `rayon` and `tokio` are allowed only in tools and offline jobs, not in emulation threads.
- Thread priorities and affinities are applied through a small platform layer
  (Windows: `SetThreadPriority`/MMCSS for present & audio; Linux: `nice`/`SCHED_*` where
  permitted). Treated as hints, measured in PoC-8.

## 4. Memory ownership and lifetimes

- **Guest memory** is one reserved host virtual region (39-bit guest space) backed by shared
  memory objects so pages can be mapped at multiple host addresses (fastmem + GPU import).
  Rust code accesses it only through `lince-mem` types that encode the invariant "this slice
  may change under you" (no `&[u8]` to guest memory across await points or thread boundaries).
- **GPU resources** follow a frame-timeline ownership model: a resource is released into a
  deferred-destruction queue tagged with the timeline value of its last use; it is destroyed
  only when the GPU passes that value. No `vkDeviceWaitIdle` in gameplay.
- **Compile artifacts** are immutable `Arc` values; publication via atomic swap
  (`arc-swap`-style) into pipeline slots.

## 5. `unsafe` policy

- `#![forbid(unsafe_code)]` in every crate except those marked `[unsafe]` above.
- Inside `[unsafe]` crates: `#![deny(unsafe_op_in_unsafe_fn)]`, every `unsafe` block carries a
  `// SAFETY:` comment, wrappers expose safe APIs.
- Miri runs on pure-Rust parts of `lince-mem` and `lince-cache`; `loom` models the
  lock-free rings and pipeline-slot publication.

## 6. Dependency evaluation (initial)

| Need | Candidates | Recommendation | Reason |
|------|-----------|----------------|--------|
| Vulkan bindings | `ash`, `vulkano`, `wgpu` | **`ash`** | Raw, complete, zero-cost; we need GPL, shader objects, pipeline binaries, descriptor buffers, mesh shaders — `wgpu` hides pipeline creation and lacks most of these. |
| GPU memory allocation | `gpu-allocator`, `vk-mem` (C++ VMA) | `gpu-allocator` + own block policy | Pure Rust; Lichii's churn lessons (doc 01 E12) call for our own large-block policy anyway. |
| SPIR-V emission | `rspirv`, own emitter, `naga` | **`rspirv`** (data representation) + own builder layer | `naga` is an IR translator with its own semantics; we need direct control of capabilities/decorations. |
| SPIR-V validation | `spirv-tools` (C++) | CI/debug only, behind a feature | Foreign code allowed off the runtime path (ADR-0026). |
| JIT emission x86-64 | `dynasm-rs`, `iced-x86` (code assembler), Cranelift | Baseline: `dynasm-rs` or `iced-x86`; optimizing: **Cranelift** | Fast compile for baseline; Cranelift is pure Rust with good codegen. LLVM rejected (compile latency, C++). |
| JIT emission AArch64 | `dynasm-rs`, Cranelift | Same split | Also enables a native-execution mode later (ADR-0005). |
| Hashing | `xxhash-rust` (XXH3), `blake3` | XXH3-128 keys, BLAKE3 only if caches are ever shared | Speed. |
| Checksums | `crc32fast` / CRC-32C | CRC-32C | Hardware-accelerated. |
| Compression | `lz4_flex`, `zstd` (C) | `lz4_flex` | Pure Rust, fast decode. |
| Embedded DB (comparison only) | `redb`, SQLite (`rusqlite`, C) | PoC-6 baseline: `redb` | Pure Rust comparator for the custom store. |
| Lock-free queues | `crossbeam`, `rtrb`, own | `crossbeam` + own SPSC ring where measured | |
| Locks | `parking_lot`, std | `parking_lot` | Fair, small, fast uncontended path. |
| Profiling | `tracy-client`, `puffin` | **Tracy** + own frame ledger | Tracy gives zones, plots, frame marks, GPU zones. |
| Platform memory | `rustix`, `windows-sys` | Both, directly | `memmap2` is insufficient for placeholder mappings. |
| Windowing/input | `winit`, `sdl3` bindings (C) | `winit` first | Pure Rust; revisit for controllers. |
| Video decode | FFmpeg (C), `dav1d` (C), pure Rust | Undecided | ADR-0026 exception required. |

## 7. Build and profiles

- Cargo workspace, edition 2024, pinned stable toolchain.
- Profiles: `dev` (opt-level 1 for emulation crates, debug assertions on), `release`
  (thin LTO, `codegen-units = 16`), `profiling` (release + debug symbols + frame pointers),
  `dist` (fat LTO, `codegen-units = 1`, PGO later).
- `xtask` commands: `xtask ci`, `xtask shaders` (regenerate built-in SPIR-V), `xtask bench`,
  `xtask replay <capture>`, `xtask analyze <ledger>`.

## 8. Diagnostics

- Structured logging via `tracing` with per-module filters; hot paths log through
  rate-limited, deduplicated macros (lesson from Lichii row 90: 1.7 M log lines in 13 min).
- Frame ledger (doc 05 §3) always on in release builds; Tracy behind a feature flag.
- GPU debugging: Vulkan validation in debug builds, RenderDoc/Nsight friendly labels.
