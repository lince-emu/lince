# Development Rules

How code is written in Lince. Ids (`DEV-xx`) are cited in reviews. Architecture-level rules
are in [architecture-rules.md](architecture-rules.md).

Severity: **MUST** (blocks merge) · **SHOULD** (deviation needs a reason in the PR).

## A. Workflow

| Id | Rule |
|----|------|
| DEV-01 | Every change starts from a backlog task (`docs/backlog.md`) and lives on a `lince/T-xxx-slug` branch ([branches-and-prs.md](branches-and-prs.md)). |
| DEV-02 | Commits follow [commits.md](commits.md). One logical change per commit; every commit builds and passes tests. |
| DEV-03 | Implement only what the task asks. No speculative abstractions, no unrequested features, no drive-by refactors (open a task instead). |
| DEV-04 | Pull requests **SHOULD** stay under 400 changed lines excluding tests and generated files. |
| DEV-05 | Before a PR: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, tests, `.claude/hooks/check-invariants.sh` (or `cargo xtask ci`). |

## B. Rust style

| Id | Rule |
|----|------|
| DEV-10 | Edition 2024, pinned stable toolchain, `rustfmt` defaults. Formatting is never discussed in review. |
| DEV-11 | Naming follows the Rust API Guidelines. Guest concepts keep hardware names in `snake_case` (`gpfifo_entry`, `qmd`), with the term defined in the module docs. |
| DEV-12 | Modules are organized by responsibility, not by type kind (no `types.rs` / `utils.rs` dumping grounds). A file over ~800 lines is a signal to split. |
| DEV-13 | Prefer concrete types and generics on hot paths; `dyn Trait` only at cold boundaries (configuration, tools, plugin-like registries). |
| DEV-14 | Newtypes for every unit and address space: `GuestVa`, `GpuVa`, `HostPtr`, `PageIndex`, `Nanos`. Raw `u64` addresses do not cross module boundaries. |
| DEV-15 | `#[inline]` only with a benchmark or for small cross-crate functions on hot paths; `#[inline(always)]` needs a measured reason. |
| DEV-16 | Constants with hardware meaning are named and documented with their source (`/// Ampere QMD v3 release slot count — research/qmd3.md`). |

## C. Errors and panics

| Id | Rule |
|----|------|
| DEV-20 | Library crates define error enums (`thiserror` allowed); `anyhow` only in `lince-app`, tools and `xtask`. |
| DEV-21 | Guest-caused conditions (bad guest data, unknown commands, invalid GPU state) are **never** host panics: they become guest-visible errors, ledger events, or `Unimplemented` paths. |
| DEV-22 | `unwrap`/`expect` only for proven invariants, with a message stating the invariant. `expect("…")` is preferred to `unwrap()`. Clippy `unwrap_used` is denied in emulation crates. |
| DEV-23 | Errors from the cache and other optional subsystems degrade to a slower path; they never stop emulation (ARCH-17). |

## D. Concurrency and memory

| Id | Rule |
|----|------|
| DEV-30 | Every shared structure documents its owner thread(s), who may read and write it, and the synchronization used. |
| DEV-31 | Every atomic operation with an ordering other than `SeqCst` carries a comment explaining the ordering and its pairing. |
| DEV-32 | Lock-free structures and publication protocols have `loom` tests. |
| DEV-33 | No lock is held across a call into another subsystem, a guest callback, or a wait. |
| DEV-34 | Guest memory is accessed through `lince-mem` types; no `&[u8]` borrowed from guest memory outlives the operation. |
| DEV-35 | Hot-path collections are pre-sized or pooled; `Vec::new()` inside a per-draw or per-instruction loop is a review finding. |

## E. `unsafe`

| Id | Rule |
|----|------|
| DEV-40 | `unsafe` only in crates allowed by ARCH-24. `#![deny(unsafe_op_in_unsafe_fn)]` in those crates. |
| DEV-41 | Each `unsafe` block is as small as possible and has a `// SAFETY:` comment listing the invariants relied upon. |
| DEV-42 | Each `unsafe fn` has a `# Safety` doc section. Safe wrappers are the only public API. |
| DEV-43 | Code with `unsafe` gets Miri coverage where Miri can run it, and fuzz or property tests otherwise. |

## F. Testing

| Id | Rule |
|----|------|
| DEV-50 | Every behavior change has a test that fails without the change. Bug fixes include a regression test. |
| DEV-51 | Anything with two implementations of the same semantics has differential tests (ADR-0028). |
| DEV-52 | Parsers of untrusted bytes (executables, SASS, IPC, cache records, captures) have fuzz targets. |
| DEV-53 | Cache code has fault-injection tests; the only acceptable outcomes are a correct hit or a miss. |
| DEV-54 | Tests are deterministic: seeded RNGs, no wall-clock dependence, no network. |
| DEV-55 | No game content in the repository's tests (doc 09 §5). |

## G. Performance work

| Id | Rule |
|----|------|
| DEV-60 | `perf` commits and PRs include before/after measurements, hardware, runs and variance (`perf-check` skill). |
| DEV-61 | Optimizations that reduce clarity need a benchmark that guards them, so they can be removed if they stop paying off. |
| DEV-62 | Frame-time results are reported as distributions (median, P95, P99, P99.9 when the sample allows), never only as averages. |

## H. Logging, diagnostics and comments

| Id | Rule |
|----|------|
| DEV-70 | Use `tracing`. Hot paths log only through rate-limited, deduplicated macros; nothing logs per draw or per instruction by default. |
| DEV-71 | Debug switches are typed settings, not scattered environment variables. |
| DEV-72 | Comments explain *why*, invariants and hardware facts; they do not restate the code. |
| DEV-73 | `TODO(T-xxx): …` must reference a backlog task. Untracked `TODO`/`FIXME`/`HACK` are review findings. |
| DEV-74 | Public items in library crates have doc comments; module-level docs explain the module's role and threads. |

## I. Dependencies

| Id | Rule |
|----|------|
| DEV-80 | New dependencies need a reason in the PR: what it replaces, license, maintenance status, compile-time cost. Emulation crates prefer few, well-maintained, pure-Rust crates; third-party C/C++ needs an ADR (ADR-0030). |
| DEV-81 | `cargo deny` checks licenses, advisories and duplicates. |
| DEV-82 | Dependency versions are bumped in dedicated `build(deps)` commits. |

## Review checklist (summary)

- [ ] Task and ADR coverage (ARCH-40, DEV-01)
- [ ] Render-critical threads unaffected or justified (ARCH-01–06)
- [ ] No discarded guest work (ARCH-02)
- [ ] Shader/cache rules respected where relevant (ARCH-10–18)
- [ ] Dependency direction and `unsafe` placement (ARCH-20–27, DEV-40–43)
- [ ] Errors and panics (DEV-20–23)
- [ ] Concurrency documented and tested (DEV-30–35)
- [ ] Tests added (DEV-50–55)
- [ ] Performance evidence if applicable (ARCH-42, DEV-60–62)
- [ ] Commit messages and PR format ([commits.md](commits.md), [branches-and-prs.md](branches-and-prs.md))
