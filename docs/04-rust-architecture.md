# 04 — Complete Rust Architecture

Status: Proposed · Related ADRs: 0001–0008, 0017–0025, 0030

Lince is Rust with two C++ components (ADR-0030). Rust owns the architecture described here;
crates marked `[c++]` contain a C++ library behind a C ABI and expose a safe Rust API.

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
│   ├── lince-cpu-a64/          # A64 decoder, interpreter, JIT tiers, code buffers  [c++]
│   ├── lince-kernel/           # Horizon-compatible kernel HLE: threads, sync, SVCs
│   ├── lince-hle/              # IPC (CMIF/TIPC) framework, common services
│   ├── lince-hle-nx2/          # Switch 2 specific services (gsv, vi2, syncpt, …)
│   ├── lince-loader/           # NSO/NPDM/ExeFS/RomFS (already-decrypted content only)
│   ├── lince-gpu-cmd/          # channels, GPFIFO, MME64 JIT, semaphores, syncpoints
│   ├── lince-gpu-ampere/       # Ampere class engines (3D, compute/QMD, DMA, 2D)
│   ├── lince-shader/           # SSA IR + passes, Ampere SASS front-end, SPIR-V emitter,  [c++]
│   │                           #   Tier-0 micro-op format + interpreter shader generation
│   ├── lince-pipeline/         # keys, tiers, slots, compile scheduling
│   ├── lince-cache/            # content-addressed store, index, writer, compaction
│   ├── lince-texture/          # swizzle (GOB/block-linear), ASTC/BC codecs (GPU + CPU ref)
│   ├── lince-render-vk/        # Vulkan backend: device, memory, resources, sync, present [unsafe]
│   │                           #   (later: lince-render-d3d12, lince-render-metal; ADR-0006)
│   ├── lince-audio/
│   ├── lince-core/             # system composition, lifecycle
│   └── lince-app/              # CLI first; GUI later
└── tools/
    ├── lince-replay/           # GPU command-stream capture replayer (perf + correctness CI)
    ├── lince-frame-analyzer/   # ledger → percentiles, spike attribution reports
    └── lince-shader-lab/       # offline decode/translate/diff of captured programs
```

Switch 1 later adds an A32 front-end inside `lince-cpu-a64`, a Maxwell SASS front-end inside
`lince-shader`, and the crates `lince-gpu-maxwell` and `lince-hle-nx1`. Both SASS front-ends
target the same shader IR (ADR-0025).

**`[c++]` crate layout** (ADR-0030):

```text
crates/lince-shader/
├── build.rs                    # compiles cpp/src with the `cc` crate (clang / clang-cl, C++20)
├── cpp/include/lince-shader.h  # the C ABI: the only contract between the two languages
├── cpp/src/                    # C++ sources; no exceptions, no RTTI, no threads, no I/O
└── src/
    ├── ffi.rs                  # safe wrapper over the C ABI                          [unsafe]
    ├── ffi/bindings.rs         # Rust declarations generated from the header, checked in [unsafe]
    └── lib.rs …                # public Rust API, no `unsafe`; the rest of Lince uses only this
```

C++ receives guest memory access, frame-ledger events and any other service from Rust through
a callback table passed at creation.

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

- `#![forbid(unsafe_code)]` in every crate except those marked `[unsafe]` above. In `[c++]`
  crates, `unsafe` is allowed only in the `src/ffi` module (`src/ffi.rs` and `src/ffi/`).
- Inside `[unsafe]` crates: `#![deny(unsafe_op_in_unsafe_fn)]`, every `unsafe` block carries a
  `// SAFETY:` comment, wrappers expose safe APIs.
- Miri runs on pure-Rust parts of `lince-mem` and `lince-cache`; `loom` models the
  lock-free rings and pipeline-slot publication.
- C++ code is covered by ASan/UBSan runs of its Rust-driven test suite, `clang-tidy` and
  differential tests (Miri and `loom` cannot see it).

## 6. Dependency evaluation (initial)

| Need | Candidates | Recommendation | Reason |
|------|-----------|----------------|--------|
| Vulkan bindings | `ash`, `vulkano`, `wgpu` | **`ash`** | Raw, complete, zero-cost; we need GPL, shader objects, pipeline binaries, descriptor buffers, mesh shaders — `wgpu` hides pipeline creation and lacks most of these. |
| GPU memory allocation | `gpu-allocator`, `vk-mem` (C++ VMA) | `gpu-allocator` + own block policy | Pure Rust; Lichii's churn lessons (doc 01 E12) call for our own large-block policy anyway. |
| SPIR-V emission | own C++ emitter, `SPIRV-Headers` (C) | Own emitter in `lince-shader` (C++, ADR-0030) | Direct control of capabilities/decorations; using `SPIRV-Headers` needs a third-party-code ADR. |
| SPIR-V validation | `spirv-tools` (C++) | CI/debug only, behind a feature | Third-party foreign code allowed off the runtime path (ADR-0030). |
| JIT emission x86-64 | own C++ assembler, third-party C++ assemblers | Undecided | The JIT is C++ (ADR-0030); a third-party emitter needs its own ADR. LLVM rejected (compile latency). |
| JIT emission AArch64 | own C++ assembler, third-party C++ assemblers | Same decision as x86-64 | Also enables a native-execution mode later (ADR-0005). |
| C++ build | `cc`, `cmake` crate | **`cc`** | No CMake install needed; one build system (ADR-0030). |
| Hashing | `xxhash-rust` (XXH3), `blake3` | XXH3-128 keys, BLAKE3 only if caches are ever shared | Speed. |
| Checksums | `crc32fast` / CRC-32C | CRC-32C | Hardware-accelerated. |
| Compression | `lz4_flex`, `zstd` (C) | `lz4_flex` | Pure Rust, fast decode. |
| Embedded DB (comparison only) | `redb`, SQLite (`rusqlite`, C) | PoC-6 baseline: `redb` | Pure Rust comparator for the custom store. |
| Lock-free queues | `crossbeam`, `rtrb`, own | `crossbeam` + own SPSC ring where measured | |
| Locks | `parking_lot`, std | `parking_lot` | Fair, small, fast uncontended path. |
| Profiling | `tracy-client`, `puffin` | **Tracy** + own frame ledger | Tracy gives zones, plots, frame marks, GPU zones. |
| Platform memory | `rustix`, `windows-sys` | Both, directly | `memmap2` is insufficient for placeholder mappings. |
| Windowing/input | `winit`, `sdl3` bindings (C) | `winit` first | Pure Rust; revisit for controllers. |
| Video decode | FFmpeg (C), `dav1d` (C), pure Rust | Undecided | Third-party foreign code: ADR required (ADR-0030). |

## 7. Build and profiles

- Cargo workspace, edition 2024, pinned stable toolchain; C++20 with clang (`clang-cl` on
  Windows), minimum version pinned by PoC-11.
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
