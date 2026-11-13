# Architecture Rules

Binding rules derived from the charter, the design docs and the ADRs. Each rule has an id
(`ARCH-xx`) so reviews, PRs and agents can cite it. A rule changes only through an ADR.

Severity: **MUST** (a violation blocks merge) · **SHOULD** (deviation needs a written reason in
the PR).

## A. Frame delivery (the primary objective)

| Id | Rule | Source |
|----|------|--------|
| ARCH-01 | Render-critical threads — guest cores, GPU front, GPU back, Vulkan submit, present, audio — **MUST NOT** compile shaders, create pipelines, perform disk I/O (including `mmap` page faults on cache files), or wait on background work. | Charter §3, §7; doc 04 §3 |
| ARCH-02 | Guest-visible work **MUST NOT** be discarded or approximated to hide missing work. Approximations exist only as opt-in, labelled modes. | Charter §3.3; ADR-0010 |
| ARCH-03 | Every new operation that can run on a render-critical thread **MUST** emit frame-ledger events and state its worst-case cost in the PR. | ADR-0027 |
| ARCH-04 | Background work **MUST** go through `lince-sched` priority classes (P0–P4) and the governor. No ad-hoc threads, no unbounded queues. | ADR-0017, ADR-0018 |
| ARCH-05 | `vkDeviceWaitIdle` / `vkQueueWaitIdle` **MUST NOT** run outside teardown. Waits are expressed as timeline dependencies. | ADR-0022 |
| ARCH-06 | Steady-state frames **SHOULD** perform zero heap allocations on render-critical threads; pools and arenas are reset per frame. | doc 04 §4 |

## B. Shaders, pipelines and cache

| Id | Rule | Source |
|----|------|--------|
| ARCH-10 | Every guest program **MUST** have a correct execution path on first use (Tier 0) unless its class is on the documented no-fallback list. | doc 03 §3, §6 |
| ARCH-11 | Shader translation **MUST** read only the recorded dependency snapshot, never live guest or GPU state. | ADR-0009 |
| ARCH-12 | Ampere SASS **MUST** be decoded natively into Lince IR; lowering through another guest ISA is forbidden. | ADR-0009 |
| ARCH-13 | Unsupported instructions **MUST** become explicit `Unimplemented` IR ops; whole programs are never silently rejected. | ADR-0009 |
| ARCH-14 | Tier 0, Tier 1 and Tier 2 **MUST** produce bit-identical results; arithmetic rules (contraction, NaN, denormals, special functions) are shared. | doc 03 §8 |
| ARCH-15 | Pipeline publication **MUST** be a single atomic swap; readers never block on publication. | ADR-0013 |
| ARCH-16 | Opening a cache **MUST** read a bounded amount of data independent of cache size. No full load, validation or rebuild at startup. | ADR-0014, ADR-0016 |
| ARCH-17 | Stored artifacts **MUST** be validated on read; any failure is a cache miss, never an error that stops the game. | doc 02 §9 |
| ARCH-18 | Cache keys **MUST** be content-addressed and versioned; there is no in-place migration of stored artifacts. | doc 02 §3, §8 |

## C. Structure and dependencies

| Id | Rule | Source |
|----|------|--------|
| ARCH-20 | Crate dependencies **MUST** point downward: `app → core → subsystems → common/trace/sched`. No cycles, no upward imports. | doc 04 §2 |
| ARCH-21 | GPU crates **MUST NOT** depend on CPU crates and vice versa; they share only `lince-mem` interfaces. | doc 04 §2 |
| ARCH-22 | The guest GPU side communicates with the host backend **only** through draw/dispatch packets. No Vulkan types outside `lince-render-vk`. | ADR-0006 |
| ARCH-23 | Platform-specific code (`cfg(windows)`, `cfg(unix)`, architecture intrinsics) **MUST** live in dedicated `platform` modules behind a safe API. | ADR-0003 |
| ARCH-24 | `unsafe` **MUST** be confined to crates listed as `[unsafe]` in doc 04 and to the `src/ffi` module (`src/ffi.rs`, `src/ffi/`) of `[c++]` crates, each block with a `// SAFETY:` comment. | ADR-0001, ADR-0030 |
| ARCH-25 | Lince's own C++ **MUST** live only in the `[c++]` crates of doc 04. Third-party non-Rust code in release builds **MUST** be approved by its own ADR and isolated behind a crate boundary and a feature flag. | ADR-0030 |
| ARCH-26 | General-purpose async runtimes and work-stealing pools **MUST NOT** be used in emulation crates (tools only). | ADR-0017 |
| ARCH-27 | Switch 2–specific behavior **MUST** live in Switch 2 crates or modules; shared crates contain only demonstrated commonality. | ADR-0025 |

## D. C++ components

| Id | Rule | Source |
|----|------|--------|
| ARCH-50 | The Rust/C++ boundary **MUST** be the crate's C header: `extern "C"` functions, plain structs of fixed-size fields and opaque handles only. No C++ or Rust types cross it. | ADR-0030 |
| ARCH-51 | C++ **MUST** be built without exceptions and RTTI; every exported function is `noexcept` and reports errors as status codes. Rust callbacks called from C++ **MUST NOT** unwind. | ADR-0030 |
| ARCH-52 | C++ **MUST NOT** create threads, perform file or network I/O, or keep mutable global state. Memory access, I/O, synchronization and ledger events come from Rust through the callback table. | ADR-0030, ADR-0017 |
| ARCH-53 | Ownership of every buffer crossing the boundary **MUST** be stated in the header; the side that allocates frees. | ADR-0030 |
| ARCH-54 | C++ code **MUST** pass `clang-format`, `clang-tidy` (warnings as errors) and its test suite under ASan and UBSan in `cargo xtask ci`. | ADR-0030 |

All other rules (frame delivery, shaders, structure, legal) apply to C++ exactly as to Rust.

## E. Correctness and knowledge

| Id | Rule | Source |
|----|------|--------|
| ARCH-30 | Unknown hardware or protocol behavior **MUST** be marked (`// UNKNOWN:` or `// GUESS: <evidence>`) and listed in doc 08 or a research note. | Charter §11 |
| ARCH-31 | Unknown HLE commands **MUST** return a distinct error and a ledger event; blanket auto-stubbing is debug-only. | ADR-0024 |
| ARCH-32 | Guest core count and layout **MUST** match the hardware (6 application cores for Switch 2). | ADR-0004 |
| ARCH-33 | Code **MUST NOT** be copied or translated from other emulators; behavior knowledge cites repository, commit and file. | doc 09 §4 |
| ARCH-34 | Keys, firmware and decryption of protected content are out of scope permanently — including keys or firmware supplied by the user. No module may read them. | doc 09 §1, ADR-0029 |

## F. Decisions

| Id | Rule | Source |
|----|------|--------|
| ARCH-40 | Implementation work **MUST** be covered by an Accepted ADR. PoCs, research and docs may proceed against Proposed ADRs. | AGENTS.md §1 |
| ARCH-41 | Accepted ADRs are immutable; changes happen through a superseding ADR. | docs/adr/README.md |
| ARCH-42 | Performance claims **MUST** include measured data and method; numbers are never estimated in docs or commits. | doc 05 |
