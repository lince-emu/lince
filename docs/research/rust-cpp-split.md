# Study: what each language should own in Lince (Rust and C++)

- **Status:** Study for the maintainer. Its three amendments were accepted as
  [ADR-0031](../adr/0031-rust-cpp-placement.md) (2026-11-13).
- **Source:** ADR-0005, ADR-0019, ADR-0030; doc 04 §2; PoC-11 report
  (`docs/poc-reports/poc-11-cpp-boundary.md`); the architecture rules (ARCH-xx).
- **Evidence level:** MEASURED where PoC-11 measured it; otherwise Lince design analysis. Claims
  about third-party libraries and other projects are **RECALL** (marked), to be verified before
  any of them is used.
- **Applies to Lince:** yes: subsystem placement, the shape of the two boundaries, and three
  gaps in the current crate map.

## 1. The principle behind ADR-0030, made explicit

The split pays off when each language does the kind of work it is best at, and the boundary
falls where the data flow is already coarse:

| | Rust owns | C++ owns |
|---|---|---|
| **Kind of work** | Concurrency, scheduling, lifetimes of shared state, I/O, OS resources, parsing untrusted input, orchestration of the frame | Code generation and compiler-like transforms: decoding, IR passes, instruction selection, emitting machine code or SPIR-V |
| **Why** | Data-race freedom, ownership of shared caches, `Send`/`Sync`, Miri and `loom` checks: the stutter-relevant parts of an emulator are concurrency problems (doc 01 E10, E13, E14) | Mature low-level emitters and bit-twiddling idioms; compiler code is mostly single-threaded and pure (data in, data out), where Rust's concurrency guarantees add little |
| **Shape of calls** | Calls into C++ once per unit of work (a block, a region, a shader) | Calls back into Rust only on slow paths (a memory miss, an SVC, a ledger event) |

PoC-11 measured that one boundary call costs about as much as a non-inlined Rust call (Rust →
C++ 0.82 ns, C++ → Rust callback 1.20 ns, Rust non-inlined 1.01 ns, Windows x64; MEASURED). So
the boundary itself is not the cost to fear. **What would be costly is splitting one hot loop
or one shared data structure across it.** The rest of this study applies that rule.

## 2. Placement per subsystem

| Subsystem | Language | Boundary | Verdict |
|-----------|----------|----------|---------|
| A64 decoder, interpreter, JIT tiers (`lince-cpu-a64`) | C++ | Rust worker calls "compile block/region"; guest thread calls "run until exit"; C++ calls back on SVC, slow memory path, ledger event | **Right** (ADR-0030). Code generation is where C++ pays off |
| Executable memory for JIT code ("code buffers") | **Rust** (`lince-mem`) | C++ receives a writable buffer, Rust makes it executable | **Gap 1**, see §3 |
| Guest memory, fastmem, fault handling (`lince-mem`) | Rust `[unsafe]` | JIT code reads guest memory inline through a base pointer from Rust; faults map back to JIT metadata | **Right**, with **gap 2** (fault → block lookup), see §3 |
| Kernel, scheduler, HLE services, IPC parsing | Rust | — | **Right**: concurrency and untrusted input |
| Loader (NRO/NSO/NPDM/RomFS parsing) | Rust | — | **Right**: untrusted input, fuzzed with `cargo fuzz` |
| GPU command processing, channels, semaphores (`lince-gpu-cmd`) | Rust | — | **Right**: the GPU front/back pipeline is the main concurrency problem (E10) |
| MME64 macro JIT (listed in `lince-gpu-cmd`) | Rust today | — | **Gap 3**, see §3: it is code generation in the Rust half |
| SASS decode, shader IR, passes, SPIR-V, Tier-0 micro-op generation (`lince-shader`) | C++ | Rust compile worker calls "translate snapshot" with an immutable dependency snapshot (ARCH-11) | **Right**: compiler work, one call per shader |
| Pipelines, Vulkan, caches, texture codecs | Rust | — | **Right**: `ash`, lifetimes of GPU objects, cache concurrency |
| Frame ledger | Rust | C++ emits events through the callback table (ARCH-52) | **Right**: one event costs about one callback, PoC-7's ring stays in Rust |
| Tools (shader lab, frame analyzer, coverage) | Rust | They decode through the C++ API, never through a second decoder | **Right**, as long as no decoder is duplicated (§4) |

## 3. Gaps in the current crate map

**Gap 1: executable memory.** Doc 04 lists "code buffers" inside `lince-cpu-a64` (C++), but
allocating executable memory and changing page protections are OS calls, which ARCH-52 keeps
out of C++. Proposal: `lince-mem` owns code memory (reserve, map writable, flip to executable,
flush the instruction cache on ARM64 hosts, W^X), and C++ emits into the writable view it is
given. That also keeps the W^X policy in one place for both JITs (gap 3).

**Gap 2: fault handling meets JIT metadata.** Fastmem faults are caught in Rust (signal/VEH,
ADR-0019), but only the C++ JIT knows which guest instruction a faulting host address belongs
to. Proposal: the C header exposes a lookup ("host PC → block, guest PC, access kind") that the
Rust fault handler calls; it must be async-signal-safe (no allocation, no locks), which the
header states for that one function.

**Gap 3: the MME64 JIT is code generation in the Rust half.** Doc 04 places an MME64 JIT in
`lince-gpu-cmd` (Rust), while ADR-0030 puts the machine-code emitters in C++. Building a
second emitter in Rust would duplicate the x86-64 and AArch64 encoders and their tests.
Options, for an ADR:
1. **A shared, lower-level `[c++]` code-generation crate** (emitters only, no CPU or GPU
   knowledge) used by `lince-cpu-a64` and by the MME JIT. ARCH-21 still holds, because both
   depend on a crate below them, not on each other. This adds a third C++ component, which is
   ADR-0030's own "conditions for revisiting" case, so it needs an ADR.
2. Keep the MME JIT in Rust with a Rust emitter (two emitters to maintain).
3. Interpret MME macros with memoization only, no JIT (doc 01 E11 measured 97 % replay hits
   in one title), and revisit after measuring.

The study's recommendation: **option 3 first** (no JIT until the ledger shows macro execution
in the frame), **option 1 if a JIT is needed**.

## 4. Rules that keep both halves at their best

1. **One decoder per guest ISA, in C++.** Rust tools (coverage, shader lab) call it through the
   C API; nothing is decoded twice. The decode table is data generated from one description
   file, so a future Rust consumer could read the same file instead of the code.
2. **Coarse calls into C++, slow-path calls out of it.** Translation, compilation and "run
   until exit" are single calls; per-instruction or per-access calls across the boundary are
   design errors except on slow paths.
3. **Interpreter memory access is inline too.** The interpreter, like the JIT, reads guest
   memory through the base pointer Rust provides; the per-access callback is the slow path
   only. One callback per load would make the interpreter the one hot loop that crosses the
   boundary, which is exactly what Tier A must not cost under ADR-0005's "the guest never
   compiles" rule.
4. **No shared mutable structures across the boundary.** C++ works on caller-provided buffers
   and snapshots passed by value; publication of compiled code (atomic patching of entry
   points) is done by Rust, which owns the threads that race.
5. **Testing compensates for what Rust's tools cannot see.** Miri and `loom` do not cover C++
   (ADR-0030); every `[c++]` crate gets differential tests through its Rust API, ASan/UBSan
   runs (in `xtask ci` since PoC-11), and fuzzing of the C++ decoders with coverage
   instrumentation enabled in the `cc` build (to be set up with the first fuzz target).

## 5. Choices this enables later (not decided here)

- **Emitters:** C++ assembler libraries for x86-64 and AArch64 exist (RECALL, for example
  asmjit, xbyak, oaknut, vixl); each needs its licence verified and its own ADR (ADR-0030).
  The alternative is an own emitter for the subset the JIT emits.
- **Tier C optimizer:** C++ keeps LLVM (ORC) available as a candidate for the background Tier C,
  to compare with an own optimizer in a PoC (ADR-0005). Cranelift does not fit, being Rust,
  unless ADR-0030 is revisited.
- **Cross-language LTO:** clang and rustc can optimize across the boundary when their LLVM
  versions match (RECALL). With coarse calls (rule 2) the benefit is small; not worth the
  toolchain coupling now.

## 6. Verdict

The decision to put the JIT and the compiler-like subsystems in C++ fits the work they do, and
PoC-11 showed the boundary is cheap enough for the call shape the design uses. To get the
benefit of both languages, three amendments are proposed for ADR-0030 / doc 04:

1. Executable code memory belongs to `lince-mem` (Rust); C++ emits into buffers it is given.
2. The C++ CPU header exposes an async-signal-safe "host PC → guest location" lookup for the
   fastmem fault handler.
3. The MME64 JIT does not get its own emitter in Rust: interpret and memoize first; if a JIT
   is needed, use a shared lower-level `[c++]` code-generation crate (new ADR).
