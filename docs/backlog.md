# Backlog

The task queue for humans and agents. The `next-task` skill reads this file.

**Statuses:** `Ready` · `In progress` · `Needs decision` · `Blocked` · `Done`
**Types:** `Maintainer` (only the maintainer can do it) · `Research` · `Docs` · `PoC` · `Impl`

Rules:
- `Impl` tasks need all listed ADRs **Accepted**. `PoC`, `Research` and `Docs` tasks may run
  against `Proposed` ADRs because they produce the evidence those ADRs need.
- Keep one row per task in the table; details go in the section below. Agents update the
  `Status` column and append the PR link in the details.

| ID | Title | Type | Status | Depends on | ADRs |
|----|-------|------|--------|------------|------|
| T-001 | Choose the project license | Maintainer | Done | — | 0000 |
| T-002 | Review ADRs ready for acceptance (0001 details, 0002, 0014) | Maintainer | Done | — | 0001, 0002, 0014 |
| T-003 | Bootstrap Cargo workspace, xtask and CI skeleton | Impl | Done | T-001, T-002 | 0001, 0002 |
| T-004 | Commit linting in CI and changelog generation (git-cliff) | Impl | Done | T-003 | 0002 |
| T-005 | Reference crate: `lince-common` written to every convention | Impl | Done | T-003 | 0001 |
| T-006 | Decide whether to require DCO `Signed-off-by` on commits | Maintainer | Needs decision | — | 0000 |
| T-010 | PoC-6: cache store prototype and benchmark harness | PoC | Done | — | 0015, 0016 |
| T-011 | PoC-7: frame ledger prototype | PoC | Done | — | 0027 |
| T-012 | PoC-5: Vulkan pipeline mechanisms harness | PoC | Done | — | 0007, 0013 |
| T-013 | Tier-0 micro-op format specification | Docs | Done | — | 0010 |
| T-014 | Ampere SASS test corpus: team-written assembler and programs (no NVIDIA SDK or GPU) | PoC | Done | — | 0009, 0028 |
| T-015 | Research: inspect oboromi, Pound, Hokkaido, Anka, Orchid, Eden | Research | Done | — | — |
| T-016 | Research: Vulkan feature coverage survey per vendor/driver (GPL, shader objects, pipeline binary, barycentrics, subgroup size control, mesh) | Research | Done | — | 0007 |
| T-019 | Research: system data and applets Switch 2 titles depend on (firmware-free replacements) | Research | Done | — | 0024, 0029 |
| T-017 | PoC-2 step 1: compute-only GPU interpreter harness | PoC | Blocked | T-013, T-014 | 0010 |
| T-018 | PoC-4 step 1: Ampere SASS decoder skeleton + encoding notes | PoC | Blocked | T-014 | 0009 |
| T-020 | PoC-6: XXH3 payload checksum and CRC-32C vs XXH3 timing | PoC | Done | T-010 | 0015 |
| T-021 | PoC-7: loom model of the ledger ring and ARM test run | PoC | Done | T-011 | 0027 |
| T-022 | PoC-5: measure `VK_KHR_pipeline_binary` once `ash` supports it | PoC | Blocked | T-012 | 0013 |
| T-023 | Optional: SASS corpus reference outputs on an NVIDIA RTX 30/40 GPU | Maintainer | Blocked | T-014 | 0028 |
| T-024 | PoC-6: time each engine's reader in another process (`redb` in `SingleWriter` mode) | PoC | Done | T-010 | 0015 |
| T-025 | Record the hybrid Rust + C++ decision (ADR-0030) and the planned D3D12/Metal backends; update docs, rules and hooks | Docs | Done | — | 0006, 0007, 0009, 0030 |
| T-026 | PoC-11: Rust↔C++ boundary (`cc` build, C header bindings, call cost, C++ checks in `xtask ci`) | PoC | Done | T-025 | 0030 |
| T-036 | Study: what Rust and C++ should each own (refines ADR-0030) | Research | Done | T-026 | 0005, 0019, 0030, 0031 |
| T-027 | Review the B1 ADRs for acceptance (0004, 0005 interpreter part, 0019 address-space part, 0024 SVC boundary, 0027 logging part) | Maintainer | Done | — | 0004, 0005, 0019, 0024, 0027 |
| T-028 | Research: NRO format and the SVCs B1 uses (public sources) | Research | Done | — | 0024 |
| T-029 | Test homebrew: build the B1 programs from team-written A64 assembly with LLVM, and an ELF→NRO tool | Impl | Done | T-028 | 0001, 0002 |
| T-030 | `lince-trace`: log facade for named stubs (no ledger yet) | Impl | Done | T-027 | 0027 |
| T-031 | `lince-mem`: flat guest address space with typed access | Impl | Done | T-027 | 0019 |
| T-032 | `lince-loader`: load an NRO into guest memory | Impl | Done | T-028, T-031 | 0024 |
| T-033 | `lince-cpu-a64` `[c++]`: crate, A64 description table and decoder for the B1 instructions | Impl | Done | T-027, T-029 | 0004, 0030, 0031 |
| T-042 | `lince-cpu-a64`: Tier A interpreter for the B1 instructions, with differential tests | Impl | Done | T-033 | 0004, 0005, 0019, 0031 |
| T-043 | `lince-cpu-a64`: interpreter throughput benchmark on the B1 programs | Impl | Done | T-042 | 0005 |
| T-034 | `lince-kernel`: SVC dispatch for memory, exit and `OutputDebugString` | Impl | Done | T-027, T-028 | 0024 |
| T-035 | `lince-app`: `lince run <file>` prints the test NRO's "hello" (B1 exit) | Impl | Done | T-029–T-034, T-042 | 0001 |
| T-040 | `lince-cpu-a64`: full v8.0–v8.5 decode table and `lince-a64-coverage` | Impl | Blocked | T-033 | 0004, 0031 |
| T-037 | Study: host core budget (6 guest cores + role threads + workers on 6–8-core hosts) | Research | Done | — | 0017, 0005 |
| T-038 | Design: LCIR v1 (minimal CPU IR for Tier B) | Docs | Done | T-039 | 0005 |
| T-039 | Research: how other JITs schedule compilation (clean-room, cited) | Research | Done | — | 0005 |
| T-041 | `lince-mem` + `lince-kernel`: SetMemoryPermission on any page range inside a region | Impl | Done | T-034 | 0019, 0024 |
| T-044 | Close two local CI blind spots (clang-tidy header filter on Windows, PR title length) | Impl | Done | — | — |
| T-045 | Break B2 into tasks; record the D-006 answer | Docs | Done | T-035 | 0017, 0024 |
| T-046 | Mark the **B2 scope** in ADR-0004, 0017, 0019 and 0024 | Docs | Done | T-045 | 0004, 0017, 0019, 0024 |
| T-047 | Review the B2 ADRs for acceptance (B2 scope of 0004, 0017, 0019, 0024) | Maintainer | Done | T-046 | 0004, 0017, 0019, 0024 |
| T-048 | Research: the SVCs, IPC wire format (CMIF/TIPC) and service commands B2 uses (public sources) | Research | Done | — | 0024 |
| T-049 | Test homebrew: a multi-threaded B2 program that synchronizes and talks to `sm:`, `set:`, `fs`, `time` and `hid` | Impl | Ready | T-048 | 0001, 0002 |
| T-050 | `lince-cpu-a64`: exclusives, barriers and system registers the B2 programs use | Impl | Blocked | T-047, T-049 | 0004, 0005, 0031 |
| T-051 | `lince-kernel`: guest threads on the six core threads, simple scheduler, TLS | Impl | Blocked | T-047, T-048 | 0004, 0017, 0024 |
| T-052 | `lince-kernel`: handle table, sync objects, events and the wait SVCs | Impl | Blocked | T-051 | 0024 |
| T-053 | `lince-kernel` + `lince-hle`: sessions, CMIF/TIPC framework and `sm:` | Impl | Blocked | T-052 | 0024 |
| T-054 | `lince-hle`: `set:`, `fs` (host directory), `time` and `hid` stubs that log | Impl | Blocked | T-053, T-057 | 0024 |
| T-055 | Coverage tool: list every unimplemented SVC and IPC command a run reaches | Impl | Blocked | T-053 | 0024, 0027 |
| T-056 | `lince run` runs the B2 program to completion (B2 exit) | Impl | Blocked | T-049–T-055 | 0001 |
| T-057 | `lince-sched` (minimal): one service-I/O role thread fed by a bounded queue | Impl | Ready | T-047 | 0017, 0024 |
| T-058 | `cargo xtask ci` builds the test programs in `tests/homebrew/` (assemble and link, no byte comparison) | Impl | In progress | T-049 | 0002 |

## Details

### T-001 — Choose the project license
Owner decision on ADR-0000. Agents must not pick a license.
- **Done 2026-11-13:** the owner chose GPL-3.0-or-later; ADR-0000 is Accepted.

### T-002 — Review ADRs ready for acceptance
Maintainer reviews ADR-0001 (details), ADR-0002 and ADR-0014 and changes their status.
- **Done 2026-11-13:** the maintainer accepted ADR-0001, ADR-0002 and ADR-0014 in full.

### T-003 — Bootstrap Cargo workspace, xtask and CI skeleton
- Workspace `Cargo.toml` with shared lints and profiles (doc 04 §7); `rust-toolchain.toml`
  pinned to current stable; `xtask` with `ci` (fmt, clippy `-D warnings`, test, invariant
  check, license headers).
- Only `lince-common` and `lince-trace` crates created (empty APIs with docs and one test each).
- GitHub Actions: Windows x64 + Linux x64 running `cargo xtask ci`.
- Apply `[workspace.lints]` from `docs/conventions/lints.md`; root `rustfmt.toml`,
  `clippy.toml` and `deny.toml` already exist; `xtask ci` runs `cargo deny check` and verifies
  SPDX headers (LEGAL-30) and `license` fields (LEGAL-31).
- **Acceptance:** clean checkout builds and `cargo xtask ci` passes on both OSes.
- **PR:** https://github.com/lince-emu/lince/pull/11

### T-005 — Reference crate `lince-common`
Write `lince-common` (newtypes such as `GuestVa`/`GpuVa`/`Nanos`, a small error enum, one
platform-neutral helper only if three crates need it) as the model every later crate copies:
crate template (STRUCT), module docs, patterns PAT-01/02/03, tests (QUAL-70–73), lints clean.
- **Acceptance:** maintainer reviews it explicitly as the reference; `docs/conventions/patterns.md`
  links to its real implementations.
- **PR:** https://github.com/lince-emu/lince/pull/26

### T-006 — DCO decision
Decide whether commits must carry `Signed-off-by` (Developer Certificate of Origin, LEGAL-35).
If yes, extend `.githooks/commit-msg` and `docs/conventions/commits.md`.

### T-004 — Commit linting in CI and changelog generation
- `cargo xtask ci` validates every commit in the PR range with the same rules as
  `.githooks/commit-msg` (`docs/conventions/commits.md` §9), and the PR title.
- `cliff.toml` for `git-cliff` grouping by type as in `docs/conventions/versioning.md` §4.
- **Acceptance:** CI fails on a non-conforming commit or PR title; `git cliff` produces a
  grouped `CHANGELOG.md` from the existing history.
- **PR:** https://github.com/lince-emu/lince/pull/25

### T-010 — PoC-6 cache store
Follow `docs/06-poc-roadmap.md` PoC-6 and `docs/02-cache-architecture.md` §4–§9.
- Custom store (segments + mmap index), `redb` comparator, SQLite comparator behind a feature.
- Synthetic workload generator (blob sizes 1 KiB–2 MiB, configurable distribution).
- Fault-injection tests (truncation, bit flips, kill during append).
- **Acceptance:** harness produces open-time vs. size and resolve-latency CSVs; report skeleton
  in `docs/poc-reports/poc-6-cache-store.md`; correctness tests pass. Measurements on the
  maintainer's hardware are a follow-up.
PR: https://github.com/lince-emu/lince/pull/2

### T-011 — PoC-7 frame ledger
Per-thread lock-free ring of fixed-size events, dump format, minimal analyzer computing
percentiles and attributing synthetic injected spikes.
- **Acceptance:** event cost microbenchmark; analyzer test with synthetic spikes identifies
  the injected cause.
PR: https://github.com/lince-emu/lince/pull/3

### T-012 — PoC-5 Vulkan pipeline mechanisms
Standalone `ash` harness that creates pipelines from generated SPIR-V of several sizes using:
monolithic, GPL non-LTO + fast link, GPL LTO, shader objects, sharded `VkPipelineCache`,
`DISABLE_OPTIMIZATION`. Detects support at runtime and skips unsupported paths. Pipeline
binary load is deferred to T-022, because `ash` 0.38 has no `VK_KHR_pipeline_binary` bindings.
- **Acceptance:** runs on any Vulkan 1.3 device, writes per-mechanism latency CSV; report skeleton.
- **PR:** https://github.com/lince-emu/lince/pull/14

### T-013 — Tier-0 micro-op format specification
Write `docs/research/tier0-micro-op-format.md`: encoding, operand descriptors, register-file
buckets, branch target resolution, how predicates/uniform registers are represented, and how
the GPU interpreter loop consumes it (doc 03 §4). Propose as input for ADR-0010; no code.
PR: https://github.com/lince-emu/lince/pull/1

### T-014 — SASS corpus
Decisions [D-005](decisions/D-005-sass-corpus-without-nvidia-gpu.md) and
[D-010](decisions/D-010-cuda-eula-sass-corpus.md). No NVIDIA SDK tool is used (LEGAL-15).
Encoding source: Mesa NAK, per [D-011](decisions/D-011-sass-encoding-source.md).
Standalone crate in `poc/sass-corpus/`:
- a Rust assembler for an initial subset of Ampere (`sm_86`) SASS: integer and logic ops,
  FADD/FMUL/FFMA, moves, predicates, branches, LDG/STG and EXIT;
- every encoding used is recorded in `docs/research/ampere-sass-encoding.md` with its public
  source (LEGAL-11; oboromi's tables only as
  [D-006](decisions/D-006-oboromi-sass-tables-off-limits.md) allows); encodings that no public
  source gives are not guessed into the corpus;
- corpus programs in the assembler's text form, each with deterministic inputs and expected
  outputs computed on the CPU from the program's documented semantics;
- the one-command build that assembles every program and stores its binary and expected
  output, and the T-023 runner, which stops with a clear message where it needs a GPU.
- **Acceptance:** the corpus builds from a clean checkout with one command; every program has
  its binary and its CPU-computed expected output stored; each encoding cites its source; the
  T-023 runner runs to the point where it needs a GPU.
- **PR:** https://github.com/lince-emu/lince/pull/23

### T-023 — SASS corpus reference outputs (optional)
Run the T-014 runner once on an NVIDIA GPU of compute capability 8.6–8.9 (GeForce RTX 30 or
40 series; `sm_86` code runs only on compute capability 8.6 and later 8.x devices), and
commit the outputs. The runner loads the team-assembled programs through the NVIDIA driver;
no SDK tool is used (LEGAL-15). Anyone with such a GPU can run it. Nothing depends on this task: PoC-2
and PoC-4 use the doc 06 matching rule, and these outputs only tighten it.

### T-015 — Research: other Switch 2 projects
Use the `research-note` skill. Focus: language, CPU backend, shader approach, cache design,
any Rust crates worth evaluating. Pin commits.

### T-016 — Research: Vulkan feature coverage survey
Use public sources (gpuinfo database, Mesa release notes, vendor driver notes). Output a table
per vendor/driver for the features in ADR-0007's tiers; mark data dates.
PR: https://github.com/lince-emu/lince/pull/10

### T-019 — Research: firmware-free system data and applets
Use the `research-note` skill. From public documentation and the service calls observed by
other projects (never from firmware files or leaked material), list the system data archives
and applets Switch 2 titles use, and for each propose an original implementation, a freely
licensed substitute, or a documented "unavailable" behavior. Extend
`docs/10-firmware-independence.md` §3.
- **Done:** [lince-emu/lince#9](https://github.com/lince-emu/lince/pull/9); open decision [D-009](decisions/D-009-shared-font-substitutes.md).

### T-017 / T-018
Start after their dependencies; see PoC-2 and PoC-4 in `docs/06-poc-roadmap.md`.
T-017 is paused until after B6 (`docs/07-development-roadmap.md`, "What pauses").
T-018: [D-006](decisions/D-006-oboromi-sass-tables-off-limits.md) is answered (option A, LEGAL-16);
T-018 returns in B5.

### T-020 — PoC-6 XXH3 checksum
Decision [D-001](decisions/D-001-cache-hash-xxh3.md). Add `xxhash-rust` (`xxh3` feature) to
`poc/cache-store`, use XXH3-64 for the payload checksum as doc 02 §4.1 specifies, and time
CRC-32C vs XXH3-64 on 1 KiB–2 MiB payloads in the harness.
- **Acceptance:** harness writes a checksum-cost CSV; fault-injection tests still pass; report
  updated (simplification 1 removed or measured).
- **PR:** https://github.com/lince-emu/lince/pull/17

### T-021 — PoC-7 memory-model checks
Decision [D-004](decisions/D-004-ledger-memory-model-checks.md). Add `loom` as a
dev-dependency of `poc/frame-ledger` and model one producer and one concurrent snapshot;
run the test suite on an ARM host. Until an ARM CI runner exists (T-003 sets up x64 runners
only), use any ARM host available to the maintainer or a contributor.
- **Acceptance:** the loom test passes, and it fails when either of these fences in
  `poc/frame-ledger/src/ring.rs` is removed (shown in the PR): the release fence in
  `Ring::push` and the acquire fence in `Ring::snapshot`. If loom cannot detect the removal
  of one of them, the PR and the PoC-7 report document that loom limitation instead. The ARM
  run result, with the host it ran on, is recorded in the PoC-7 report.
- **PR:** https://github.com/lince-emu/lince/pull/19

### T-022 — PoC-5 pipeline binaries
Blocked until an `ash` release includes `VK_KHR_pipeline_binary` (0.38 predates it). Then add
pipeline binary load to the PoC-5 harness and the report. Rationale: ADR-0013 relies on
pipeline binaries for L6 persistence, and PoC-5's hypothesis that their loads are
sub-millisecond is untested until they are measured.
- **Acceptance:** the PoC-5 harness times pipeline binary load next to the other mechanisms,
  skips it where the extension is unsupported, and the PoC-5 report records the results.

### T-024 — PoC-6 second-process readers
Decision [D-002](decisions/D-002-cache-engine-second-process.md). Let
`harness resolve --writer on` run the reader in a separate process while the writer appends,
for every engine: the custom store, `redb` opened with `ConcurrencyMode::SingleWriter`
(writer and reader in that mode) and SQLite in WAL mode. Record whether each engine serves
that reader, which ADR-0015's "Conditions for revisiting" requires, so the "within 20%"
comparison uses the same reader setup for all engines.
- **Acceptance:** `harness resolve --writer on` times a reader in another process for each
  engine; the PoC-6 report states, per result row, the reader setup and the `redb` mode, and
  whether each engine served the second-process reader.
- **PR:** https://github.com/lince-emu/lince/pull/21

### T-025 — Hybrid Rust + C++ decision
The maintainer decided on 2026-11-13 that the guest CPU and shader subsystems are written in
C++ behind a C ABI, with Rust owning the rest. Record it as ADR-0030 (superseding ADR-0026 and
the language clause of ADR-0001) and update the charter, doc 04, ADR-0004/0005, the
`ARCH`/`LEGAL`/`DEV` rules, `AGENTS.md`, the agents and `check-invariants.sh`. Also record
that Vulkan is the first of several backends (Direct3D 12 and Metal later): ADR-0006/0007/0009,
backend in the doc 02 cache keys, ARCH-28.
- **Acceptance:** no document still states that Lince is Rust only; the invariant hook flags
  C++ outside `[c++]` crates, exceptions/RTTI, threads and I/O in C++, and missing SPDX headers
  in C++ files.

### T-026 — PoC-11 Rust↔C++ boundary
Follow `docs/06-poc-roadmap.md` PoC-11 and ADR-0030. Standalone crate in `poc/cpp-boundary/`.
- C++20 static library built by `build.rs` with the `cc` crate (`clang-cl` on Windows, `clang`
  on Linux); one C header; Rust declarations generated from it and checked in, with a CI check
  that they match; layout tests for every boundary struct.
- Microbenchmarks: Rust→C++ call and C++→Rust callback vs. a non-inlined Rust call.
- `xtask ci` gains `clang-format`, `clang-tidy` (warnings as errors), ASan/UBSan runs of the
  C++ tests, and the SPDX check (LEGAL-30) for `.cpp`, `.hpp` and `.h` files.
- New dependencies (`cc`, and `bindgen` if chosen) are listed with license and reason
  (DEV-80).
- **Acceptance:** `cargo xtask ci` passes from a clean checkout on Windows x64 and Linux x64;
  report in `docs/poc-reports/poc-11-cpp-boundary.md` with the measured call costs, build
  times, chosen binding generator and minimum clang version.
- **PR:** https://github.com/lince-emu/lince/pull/27

### T-036 — Study: Rust/C++ split
Asked by the maintainer: where each language pays off, given ADR-0030 and PoC-11's measured
boundary cost. Output `docs/research/rust-cpp-split.md`, with three proposed amendments to
ADR-0030 / doc 04 (executable code memory in `lince-mem`, a fault-lookup function in the CPU
header, no separate Rust emitter for the MME64 JIT).
- **Accepted** as ADR-0031 (maintainer, 2026-11-13).
- **PR:** https://github.com/lince-emu/lince/pull/31

### T-027 — B1 ADR review (maintainer)
The boot-first roadmap (doc 07, B1) needs these accepted before its `Impl` tasks start:
ADR-0004 (guest CPU model), ADR-0005 (the interpreter part), ADR-0019 (the guest address-space
part), ADR-0024 (the SVC boundary) and ADR-0027 (the logging part: B1 logs every stub by name;
the frame ledger stays with PoC-7 and its ARM run). Since PR #30 each of these ADRs marks the
part B1 needs as **B1 scope**; accepting means recording that the B1 scope is accepted while
the rest stays Proposed. ADR-0031 (Rust/C++ placement) is already Accepted and applies to
T-031 and T-033.
- **Acceptance:** each ADR's status line states what is accepted; T-030 to T-034 become Ready.
- **Done 2026-11-14:** the maintainer accepted the B1 scope of ADR-0004, 0005, 0019, 0024 and
  0027; the rest of each stays Proposed.

### T-028 — Research: NRO format and B1 SVCs
Use the `research-note` skill. From public sources only (switchbrew and similar, LEGAL-11;
never SDK material, LEGAL-10): the NRO header and segment layout, how a homebrew program
receives its entry arguments, and the numbers, arguments and results of the SVCs B1 uses
(memory query/set, exit, `OutputDebugString`), plus what public sources say about the Switch 2
SVC `0x80` (doc 01 E2), which ADR-0024's SVC table must reserve. Note what is Switch 1
knowledge and what is known to hold on Switch 2 (doc 08).
- **Acceptance:** `docs/research/nro-and-b1-svcs.md` with per-field sources and evidence
  levels; UNKNOWN items listed in doc 08.

### T-029 — Test homebrew programs
The B1 programs are written by the team, never taken from homebrew projects (LEGAL-20/21):
- A64 assembly in `tests/homebrew/` (hello via `OutputDebugString`, then exit; plus one
  program per instruction group the interpreter implements);
- built with LLVM (`clang --target=aarch64-none-elf`, `ld.lld`), already required by ADR-0030;
- a team-written ELF→NRO tool in `tools/` (format from T-028), driven by an `xtask` command,
  with the generated NROs checked in so tests do not need LLVM.
- **Acceptance:** one command rebuilds every NRO bit-for-bit; the NRO header fields are
  checked by a test against T-028's note.

### T-030 — `lince-trace` log facade
Only what B1 needs: a log call that records a named stub or event once per name (no flood),
usable from every crate, with no I/O on render-critical threads (ARCH-03: records go to a
buffer drained elsewhere). Records use the frame ledger's event shape (ADR-0027 B1 scope), so
the ledger promoted from PoC-7 later reads the same data instead of a second log format.
- **Acceptance:** follows the `lince-common` reference layout; tests cover once-per-name and
  draining; no new dependency without approval.

### T-031 — `lince-mem` flat guest address space
The ADR-0019 B1 scope: a 39-bit guest address space of mapped regions with permissions,
typed reads and writes by `GuestVa` (QUAL-20), by software lookup. The layout already follows
ADR-0019's rule for later native execution: one contiguous host reservation whose base and
guest-page-to-host-page mapping are explicit parameters, with no 4 KiB host-page assumption.
The interpreter (T-033) gets the base pointer and reads guest memory inline (ADR-0031).
JIT code memory and W^X (ADR-0031) come with the first JIT tier, not in B1.
- **Acceptance:** tests for mapped, unmapped and permission-faulting accesses; a test with a
  16 KiB host-page configuration; access through `GuestVa` only.

### T-032 — `lince-loader` NRO
Parse the NRO header and map its segments into `lince-mem` with their permissions; decode raw
fields once at the boundary (PAT-03).
- **Acceptance:** loads the T-029 NROs; a corrupt or truncated file is an error, never a panic;
  fuzz target for the header parser (ADR-0028).

### T-033 — `lince-cpu-a64` crate and decoder
The first `[c++]` crate (ADR-0030 layout validated by PoC-11). The decoder is generated from
one instruction description table (ADR-0004), the only A64 decoder in Lince: the interpreter
now and the JIT tiers later share it, and Rust tools decode through the C API (ADR-0031). An
encoding the table lacks is reported as unknown, never guessed. Split from the original T-033
into T-033 (decoder), T-042 (interpreter) and T-043 (benchmark), maintainer decision
2026-11-14. Table format (same decision): `cpp/src/insns.inc`, one X-macro entry per
instruction form with bit-pattern strings, turned into mask/value lines at compile time; a
static check rejects malformed or overlapping patterns.
- **Acceptance:** every distinct instruction of the B1 programs decodes to its form, and
  reserved field values that a bit pattern can exclude decode as unknown (tests through the
  Rust API); the checked-in bindings match the header; `cargo xtask ci` C++ checks pass.

### T-042 — `lince-cpu-a64` Tier A interpreter
The interpreter for the forms T-033 decodes (ADR-0005 Tier A, the reference semantics).
Guest memory (maintainer decision 2026-11-14): `lince-mem` gives the host base pointer and a
snapshot of the mapped regions (base, size, permission); the interpreter accesses memory
inline when an address falls in a region with the needed permission, and through the Rust
callback table otherwise (the slow path, ADR-0031). Tests run with ADR-0004's deterministic
clock. Instruction semantics come from Arm's public specification (LEGAL-11).
- **Acceptance:** differential tests of every implemented instruction against expected values
  derived from the specification (and against the host CPU on ARM hosts where available); the
  B1 programs run to their exit SVC and print "ok"; `cargo xtask ci` C++ checks pass.
- **PR:** https://github.com/lince-emu/lince/pull/42, https://github.com/lince-emu/lince/pull/45

### T-043 — Interpreter throughput benchmark
A benchmark records interpreter throughput (ns per guest instruction per instruction group)
on the B1 programs, the baseline for how long code may stay in Tier A before Tier B publishes
(ADR-0005). No new dependency without approval.
- **Acceptance:** `cargo xtask bench` (or the crate's bench target) prints the per-group
  numbers; the measured baseline is recorded with the machine it ran on.
- **Baseline** (`cargo bench -p lince-cpu-a64 --bench interp`, 2026-11-14, AMD Ryzen 5 7600,
  32 GB, Windows 11, rustc 1.100.0; median ns per guest instruction, SVC exits included):
  exit (hello, 6 insns) 37 · integer (alu, 34) 17 · branch (branch, 74) 13 · load/store
  (memory, 41) 22. Runs are short, so the min–max spread is wide (10–205 ns).
- **PR:** https://github.com/lince-emu/lince/pull/46

### T-044 — Close two local CI blind spots
Found on #42: `.clang-tidy`'s `HeaderFilterRegex` matches only `/` separators, so on Windows
the `[c++]` headers were never linted and a magic number reached CI on Linux; and the `open-pr`
skill validates the title with the commit hook (72 characters) while CI limits PR titles to 66
(`cargo xtask ci --pr-title`).
- **Acceptance:** clang-tidy on Windows reports a header finding the Linux run reports; `open-pr`
  checks the title with the same rule CI applies before pushing.

### T-034 — `lince-kernel` SVC dispatch
Dispatch the SVCs from T-028 (memory, exit, `OutputDebugString`) through a table indexed by
SVC number with the Switch 2 SVC `0x80` reserved (ADR-0024 B1 scope). An unknown SVC returns a
distinct error, never success or a panic, and is logged once by name and number through
`lince-trace`. Names come from public documentation or are descriptive (LEGAL-12).
- **Acceptance:** each SVC tested through the dispatch API; unknown SVCs logged once.

### T-035 — `lince run` (B1 exit)
`lince-app` CLI: `lince run <file.nro>` loads the file, runs it on the interpreter and prints
`OutputDebugString` text on the host terminal.
- **Acceptance:** the B1 exit criterion of doc 07, demonstrated: the T-029 hello NRO prints
  "hello"; an integration test runs it in CI on Windows and Linux.
- **PR:** https://github.com/lince-emu/lince/pull/47

### T-037 — Study: host core budget
ADR-0005 moves all JIT work to `lince-sched` workers, which then compete with the six guest
core threads, the GPU front and back, Vulkan submit, present and audio. On 6–8-core hosts
that is more runnable threads than cores. Study how many threads each role needs, how
priorities (ADR-0017 P0–P4) and the governor share the cores, what happens when the host has
fewer cores than guest cores, and what the ledger must record to see it.
- **Acceptance:** `docs/research/host-core-budget.md` with a proposed policy per host core
  count and the ledger events to validate it; proposed ADR-0017 changes listed, not applied.

### T-038 — LCIR v1 design
Specify the minimal LCIR that Tier B needs for the B1/B2 instruction set (ADR-0005): values,
guest-state access, memory operations with size and ordering, explicit exits, the serialized
form that crosses the C ABI and is hashed for the cache. Written from the Arm specification
and Lince's requirements, after T-039 so known pitfalls are listed without borrowing any
design (LEGAL-20/21).
- **Acceptance:** `docs/design/lcir-v1.md` (or a doc 03-style chapter) the maintainer
  approves before the first JIT tier is implemented.
- **PR:** https://github.com/lince-emu/lince/pull/48

### T-039 — Research: how other JITs schedule compilation
Use the `research-note` skill. Behavior only, cited by repository, commit and file: when
`dynarmic` (Eden lineage) and ARMeilleure (Ryujinx) compile guest code (on the guest thread
or in the background), how they handle first execution, and how persistent caches are
loaded. No code is copied or paraphrased (LEGAL-20/21); the note replaces the RECALL in
ADR-0005.
- **Acceptance:** `docs/research/cpu-jit-scheduling.md` with evidence levels; ADR-0005's
  RECALL sentence updated to cite it.

### T-040 — Full A64 decode table and coverage tool
Populate the decoder's description table (T-033) to the v8.0–v8.5 feature set ADR-0004
requires (LSE, CAS/CASP, LDAPR, PAC, MTE, …), from Arm's public specification, and build
`lince-a64-coverage`, a Rust tool that decodes executables through the C API (ADR-0031) and
lists every encoding the table lacks. Decoding an encoding is not implementing it: the
interpreter only gains what the next phase's programs need.
- **Acceptance:** differential decode tests per instruction class; the coverage tool runs on
  the T-029 programs in CI and reports zero unknown encodings; it can be pointed at a
  user-supplied executable without any of its content entering the repository (LEGAL-50).

### T-041 — SetMemoryPermission on part of a region
`AddressSpace::protect` only changes a whole mapped region, so SetMemoryPermission (T-034)
returns `0xD401` for a page range inside a region, or one that spans the heap's chunks, although
S6 asks only for page alignment (docs/research/nro-and-b1-svcs.md). Let `protect`
split and merge regions for any page-aligned range that is fully mapped, keeping the
region map free of overlaps, and drop the whole-region limit from the kernel. Needed before
the first program that changes permissions on part of a mapping; the B1 programs do not.
- **Acceptance:** `lince-mem` tests for protecting the head, middle and tail of a region and a
  range across adjacent regions, with `query` showing the resulting regions; kernel tests for
  SetMemoryPermission on a sub-range and across heap chunks; an unmapped page in the range
  still returns `0xD401` and changes nothing.

### T-045 — Break B2 into tasks
Turn doc 07's B2 phase (Kernel and IPC) into backlog tasks shaped like B1's, and record the
maintainer's answer to D-006 (option A) in the decision and in `legal-rules.md`.
- **Acceptance:** B2 rows and details below; D-006 answered; LEGAL-16 lists the off-limits
  sources.

### T-046 — Mark the B2 scope in the ADRs
As PR #30 did for B1, add a **B2 scope** line to each ADR B2 needs, naming the part to accept:
ADR-0004 (exclusive monitor and memory ordering across cores), ADR-0017 (guest core threads
only; pools and the governor stay out), ADR-0019 (guest memory shared by several core
threads) and ADR-0024 (handles, sync objects, IPC sessions, CMIF/TIPC and the service
framework). No decision changes; a scope that needs a new decision is listed for T-047.
- **Acceptance:** each ADR states its B2 scope; open questions listed in the PR.

### T-047 — B2 ADR review (maintainer)
Accept the B2 scope marked by T-046, or send it back with questions.
- **Acceptance:** each ADR's status line states what is accepted; T-050 to T-055 can start.
- **Done 2026-11-14:** the maintainer accepted the B2 scope of ADR-0004, 0017, 0019 and 0024,
  and left CMIF domains out of B2 (one object per session).

### T-048 — Research: B2 SVCs, IPC and service commands
Use the `research-note` skill. From public sources only (LEGAL-10/11): the thread, TLS,
synchronization, event, handle and IPC SVCs (numbers, arguments, results, error codes); the
CMIF and TIPC message layouts (header, handle descriptor, buffers, domains); how a program
reaches `sm:` and the commands B2's program calls on `sm:`, `set:`, `fs`, `time` and `hid`.
Mark what is Switch 1 knowledge and what is known on Switch 2 (doc 08).
- **Acceptance:** `docs/research/b2-svcs-and-ipc.md` with per-field sources and evidence
  levels; UNKNOWN items listed in doc 08.
- **PR:** https://github.com/lince-emu/lince/pull/52

### T-049 — B2 test program
Extend the T-029 programs: one NRO starts threads on several cores, synchronizes them
(mutex, condition variable, event), reads its TLS, and opens sessions to `sm:`, `set:`,
`fs`, `time` and `hid`, printing each result through `OutputDebugString`. Team-written
assembly only; the expected output is checked in.
- **Acceptance:** the program builds in CI; its expected output is documented per step.

### T-050 — Interpreter: what B2's program needs
Add to the Tier A interpreter the instructions T-049's program uses that B1 lacks: exclusive
loads/stores with a per-core exclusive monitor, barriers, and the system registers for TLS
and the counter. Differential tests per instruction, as in T-042.
- **Acceptance:** each new instruction tested; two cores contending on an exclusive pair
  never both succeed.

### T-051 — Guest threads
One host thread per guest core (ADR-0017), a simple per-core scheduler with guest priorities,
and the thread SVCs from T-048 (create, start, exit, sleep, priority, core mask) with TLS.
No threads beyond the six core threads (invariant 7).
- **Acceptance:** kernel tests run threads on several cores; a thread pinned to a core never
  runs elsewhere; teardown joins every core thread.

### T-052 — Handles, sync objects and events
A per-process handle table, mutexes and condition variables (address arbitration), events,
and the wait SVCs with timeouts and cancellation. A guest core blocks only on guest
synchronization (ADR-0004).
- **Acceptance:** tests for wake order, timeout and cancellation; closing a handle twice
  returns the documented error.

### T-053 — IPC sessions, CMIF/TIPC and `sm:`
Kernel sessions and the IPC SVCs in `lince-kernel`; the CMIF/TIPC message framework and the
`sm:` service in `lince-hle` (doc 04). Unknown commands return a distinct error and are
logged by name, never silently succeed (ADR-0024).
- **Acceptance:** round-trip tests for each message layout from T-048; an unknown command is
  logged once and returns an error.

### T-054 — First service stubs
`set:`, `fs` (a host directory the user chooses), `time` and `hid` with the commands B2's
program calls; every stub logs by name. No game content enters the repository (LEGAL-50).
`fs` reads the host directory on the T-057 service-I/O thread, never on a guest core thread.
- **Acceptance:** each command tested through the IPC framework; `fs` cannot reach outside
  its host directory.

### T-055 — SVC and IPC coverage tool
A run records every SVC and IPC command it reaches that has no implementation, and prints the
list at exit (doc 07 B2 exit criterion), through `lince-trace`.
- **Acceptance:** a test program calling an unimplemented SVC and command shows both in the
  list; a clean run prints an empty list.

### T-056 — `lince run` (B2 exit)
`lince run` runs T-049's program on several core threads to completion.
- **Acceptance:** the B2 exit criterion of doc 07, demonstrated: the program's output matches
  its expected output; an integration test runs it in CI on Windows and Linux.

### T-057 — Minimal `lince-sched`
The maintainer chose, for B2, a minimal `lince-sched` that owns one service-I/O role thread
(ADR-0017 and ADR-0024 B2 scope, PR #50). Blocking service calls are queued to it through a
bounded queue; a full queue makes the caller wait on a kernel event, never drops the request
(invariant 1). The thread is created by `lince-sched` and joined at teardown; pools, priority
classes and the governor come later.
- **Acceptance:** tests for submission, completion through an event, a full queue, and
  teardown that completes or reports every request still pending.

### T-058 — Build the test programs in CI
Found on #54: CI checks the checked-in NROs but never assembles `tests/homebrew/*.s`, so a
source that no longer builds goes unnoticed. `cargo xtask ci` builds every program without
comparing bytes (LLVM versions differ between hosts); the Linux job installs `lld`.
- **Acceptance:** `cargo xtask ci` fails when a test program does not assemble or link, on
  Windows and Linux; `cargo xtask homebrew [--check]` behaves as before.
