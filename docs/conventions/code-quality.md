# Code Quality Rules

Small, concrete rules that keep code readable, safe and easy to change. Ids (`QUAL-xx`) are cited
in reviews. Where a lint enforces a rule, it is named in the last column (configuration in
[lints.md](lints.md)).

Severity: **MUST** (blocks merge) · **SHOULD** (deviation needs a one-line reason in the PR or a
comment at the site).

## A. Readability

| Id | Rule | Sev. | Enforced by |
|----|------|------|-------------|
| QUAL-01 | **Guard clauses first.** Handle errors and edge cases with early `return`/`?`/`continue`; keep the main path unindented. | SHOULD | review |
| QUAL-02 | **Nesting ≤ 3 levels** inside a function body. Extract a function or use `let … else`. | MUST | `clippy::excessive_nesting` |
| QUAL-03 | **One level of abstraction per function.** A function either orchestrates steps or does one step, not both. | SHOULD | review |
| QUAL-04 | **Functions ≤ 60 lines** (hard limit 80), **≤ 5 parameters**. More parameters → a parameter struct. | MUST | `clippy::too_many_lines`, `too_many_arguments` |
| QUAL-05 | **No boolean parameters** that change behavior (`draw(true)`). Use an enum (`draw(Mode::Indexed)`). One bool flag is tolerated in private helpers. | MUST | `clippy::fn_params_excessive_bools` |
| QUAL-06 | **No magic numbers.** Every literal with domain meaning is a named `const` with a doc comment citing its source. `0`, `1` and obvious sizes are fine. | MUST | review |
| QUAL-07 | **Names reveal intent.** Functions are verbs (`resolve_pipeline`), types are nouns (`PipelineSlot`), booleans read as questions (`is_ready`, `has_depth`). No negated names (`is_not_ready`). | MUST | review |
| QUAL-08 | **No abbreviations** except the glossary below and hardware terms used by the guest documentation. | SHOULD | review |
| QUAL-09 | **Command–query separation.** A function either returns information or changes state; `get_*`/`is_*` never mutate. | SHOULD | review |
| QUAL-10 | **No commented-out code** and no dead code on `main`. Git remembers. | MUST | review, `dead_code` |

Glossary of allowed abbreviations: `addr`, `buf`, `cfg`, `ctx`, `dst`, `src`, `idx`, `len`,
`ptr`, `reg`, `fn`, `id`, `io`, `ir`, `jit`, `hle`, `ipc`, `gpu`, `cpu`, `va`, `pa`, `qmd`,
`mme`, `sph`, `spirv`, `vk`, `tex`, `ubo`, `ssbo`.

## B. Types make invalid states unrepresentable

| Id | Rule | Sev. | Enforced by |
|----|------|------|-------------|
| QUAL-20 | **Newtypes for units and address spaces** (`GuestVa`, `GpuVa`, `Nanos`, `PageIndex`). Raw `u64` never crosses a module boundary as an address. | MUST | review |
| QUAL-21 | **Enums over flags and integer codes.** No `Option<bool>`, no `u8` "mode" fields; decode raw guest values into enums at the boundary. | MUST | review |
| QUAL-22 | **Exhaustive `match` on project enums** — no `_ =>` arm on our own enums, so adding a variant forces every site to be reviewed. Wildcards are fine on foreign or `#[non_exhaustive]` enums. | MUST | `clippy::wildcard_enum_match_arm` |
| QUAL-23 | **No lossy `as` casts.** Use `From`, `TryFrom`, or a named helper that documents truncation (`fn low_u32(x: u64) -> u32`). `as` is allowed for pointer ↔ integer in `[unsafe]` crates and for widening in hot loops with a comment. | MUST | `clippy::cast_possible_truncation`, `cast_sign_loss`, `cast_possible_wrap` |
| QUAL-24 | **Explicit guest arithmetic.** Arithmetic that emulates guest behavior uses `wrapping_*`, `overflowing_*` or `checked_*` explicitly — never plain `+` that panics in debug and wraps in release. | MUST | `clippy::arithmetic_side_effects` (emulation crates) |
| QUAL-25 | **Constructors validate.** A type with invariants has a constructor that enforces them (`fn new(…) -> Result<Self, …>`), private fields, and no way to build an invalid value. | MUST | review |
| QUAL-26 | **No stringly-typed data.** Strings are for humans (logs, UI, paths). Keys, kinds and states are types. | MUST | review |

## C. Ownership and performance hygiene

| Id | Rule | Sev. | Enforced by |
|----|------|------|-------------|
| QUAL-30 | **Borrow before clone.** A `.clone()` on a non-`Copy` value in a hot path needs a comment. | SHOULD | `clippy::redundant_clone`, review |
| QUAL-31 | **Accept the most general borrowed form**: `&[T]` not `&Vec<T>`, `&str` not `&String`, `impl AsRef<Path>` for paths. | MUST | `clippy::ptr_arg` |
| QUAL-32 | **Pre-size collections** whose size is known (`with_capacity`); no `collect()` followed by a single iteration. | SHOULD | `clippy::needless_collect` |
| QUAL-33 | **Iterators over index loops** unless the index itself is needed or the loop is a measured hot spot. | SHOULD | `clippy::needless_range_loop` |
| QUAL-34 | **No `Box<dyn Trait>` / `Arc<dyn Trait>` in per-draw, per-instruction or per-event code.** Use generics or enums. | MUST | review |
| QUAL-35 | **Large values by reference.** Types over 128 bytes are passed by reference; no large arrays on the stack in recursive or deep call chains. | SHOULD | `clippy::large_types_passed_by_value`, `large_stack_arrays` |

## D. Errors and control flow

| Id | Rule | Sev. | Enforced by |
|----|------|------|-------------|
| QUAL-40 | **Propagate with `?`.** No `match` that only re-wraps an error; implement `From` instead. | SHOULD | `clippy::question_mark` |
| QUAL-41 | **Never swallow errors.** `let _ = fallible();` or `.ok()` that discards an error needs a comment explaining why it is safe. | MUST | `let_underscore_must_use`, review |
| QUAL-42 | **Error messages:** lowercase, no trailing period, state what failed and with which value (`"segment 12 header crc mismatch"`). | SHOULD | review |
| QUAL-43 | **No `todo!()`, `unimplemented!()`, `dbg!()`, `println!()`** in committed library code. Use `Unimplemented` paths (ARCH-13), `tracing`, or a backlog task. | MUST | `clippy::todo`, `unimplemented`, `dbg_macro`, `print_stdout` |
| QUAL-44 | **`unwrap`/`expect` policy** per DEV-22: `expect("invariant: …")` only for proven invariants; tests may use `unwrap`. | MUST | `clippy::unwrap_used` |

## E. State and side effects

| Id | Rule | Sev. | Enforced by |
|----|------|------|-------------|
| QUAL-50 | **Prefer pure functions.** Decode, translate and compute functions take inputs and return outputs; side effects live at the edges. | SHOULD | review |
| QUAL-51 | **No global mutable state.** No `static mut`, no `lazy_static`; `OnceLock` only for truly process-wide immutable data (e.g. CPU feature detection). | MUST | `static_mut_refs`, review |
| QUAL-52 | **Narrow `&mut` scopes.** Borrow mutably for the shortest span; split structs so independent parts can be borrowed independently. | SHOULD | review |
| QUAL-53 | **Configuration is passed in**, not read from the environment or globals deep inside subsystems. | MUST | review |

## F. Change discipline (the rules against "too much code")

| Id | Rule | Sev. | Enforced by |
|----|------|------|-------------|
| QUAL-60 | **Rule of three.** Do not abstract until the third real use; duplication is cheaper than the wrong abstraction. | SHOULD | review |
| QUAL-61 | **No speculative generality.** No traits with a single implementation (except test seams), no unused parameters, no "for later" options. | MUST | review |
| QUAL-62 | **Size budget.** The plan states an expected size. If the change grows past **2× the planned lines**, or touches a crate not in the plan, stop and report before continuing. | MUST | `next-task`, review |
| QUAL-63 | **Touch only what the task needs.** Cleanups outside the touched functions go in a separate task. | MUST | review |
| QUAL-64 | **Delete freely.** Removing unused code, options or indirection is always welcome in its own commit (`refactor`). | — | — |

## G. Tests that help

| Id | Rule | Sev. | Enforced by |
|----|------|------|-------------|
| QUAL-70 | **One behavior per test**, named `<behavior>_when_<condition>` (`resolve_returns_miss_when_checksum_mismatches`). | SHOULD | review |
| QUAL-71 | **Arrange–Act–Assert** layout, separated by blank lines; helpers for arrange, never for assert logic. | SHOULD | review |
| QUAL-72 | **No sleeps or timing assumptions** in tests; use explicit synchronization or simulated clocks. | MUST | review |
| QUAL-73 | **Assert on behavior, not internals.** Tests use the public API of the module under test. | SHOULD | review |
