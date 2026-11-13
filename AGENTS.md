# AGENTS.md — Rules for AI coding assistants (and humans)

This file tells any AI assistant working in this repository how to behave. Humans follow the
same rules.

## 1. Read before writing

Before changing code in a subsystem, read:
1. `docs/00-architecture-workshop.md` (requirements and invariants),
2. the rules in `docs/conventions/`: legal (`LEGAL-xx`), architecture (`ARCH-xx`), code
   structure (`STRUCT-xx`), code quality (`QUAL-xx`), development (`DEV-xx`) and the
   canonical patterns (`PAT-xx`),
3. the design doc for the subsystem (`docs/02`–`docs/05`),
4. every ADR referenced by that doc.

If the change is not covered by an **Accepted** ADR, stop and ask the maintainer. Do not
invent architecture in code.

## 2. Non-negotiable invariants

Violating any of these is a bug, regardless of test results. The complete, numbered list is
`docs/conventions/architecture-rules.md`; these are the ones most often at risk:

1. **Never discard** draws, dispatches, resource updates or guest-visible side effects.
2. **No shader or pipeline compilation on render-critical threads** (guest cores, GPU front,
   GPU back, Vulkan submit, present). `vkCreate*Pipelines` / `vkCreateShadersEXT` only on
   compile workers.
3. **No disk I/O on render-critical threads** — including `mmap` page faults on cache files.
4. **No full cache load at startup.** Opening a cache reads a bounded amount of data.
5. **No `vkQueueWaitIdle` / `vkDeviceWaitIdle`** outside teardown.
6. **Translation never reads live guest state** — only the recorded dependency snapshot.
7. **Background work goes through `lince-sched`** priority classes and the governor; never
   spawn ad-hoc threads.
8. **`unsafe` only in crates marked `[unsafe]`** in `docs/04-rust-architecture.md`, each block
   with a `// SAFETY:` comment.
9. **No code copied or translated** from other emulators (yuzu, Eden, Eglantine, Lichii,
   Ryujinx, Dolphin, Cemu, …). Behavior knowledge must cite repository + commit + file in
   `docs/research/`.
10. **No keys, firmware or decryption code**, ever — not even keys or firmware the user
    supplies (ADR-0029) — and no leaked or NDA material (SDKs, headers, documentation) in any
    form (`docs/conventions/legal-rules.md`).
11. **No code reproduced from memory** of other projects; write from behavior descriptions
    (LEGAL-21).
12. **Every source file starts with the SPDX header** (LEGAL-30).
13. **Stay within the size budget:** if a change grows past 2× the planned lines or touches a
    crate not in the plan, stop and report (QUAL-62).

## 3. How to work

- **Small changes.** One concern per pull request; prefer < 400 changed lines.
- **Only what was asked.** No speculative abstractions, no unrequested features, no
  "while I'm here" refactors.
- **Tests with every change.** Unit tests for logic; differential tests for anything with
  multiple execution tiers; fault-injection tests for cache code.
- **Instrument critical paths.** New work on a critical thread must emit frame-ledger events.
- **Performance claims need data.** Attach before/after ledger or benchmark output; never
  write numbers you did not measure.
- **Mark uncertainty.** Unknown hardware behavior is written as `// UNKNOWN:` or
  `// GUESS: <evidence>` and listed in `docs/08-switch2-hardware.md`.

## 4. When to ask instead of deciding

Ask the maintainer when a change:
- needs a new ADR or changes an Accepted one,
- adds a dependency (any non-Rust dependency always needs ADR-0026 review),
- changes a public crate API or a cache format,
- trades correctness for performance in any way.

## 5. Commits, branches and pull requests

- **Commits:** Conventional Commits as defined in `docs/conventions/commits.md`:
  `<type>(<scope>): <description>`, footers `Refs: T-xxx`, `ADR: NNNN`,
  `BREAKING CHANGE:` when applicable. The `commit-msg` hook and the Claude Bash guard reject
  other formats.
- **Branches:** `lince/T-xxx-short-slug` (`docs/conventions/branches-and-prs.md`).
- **PR title:** a valid Conventional Commit header (it becomes the squash commit).
- **PR body:** the template in `docs/conventions/branches-and-prs.md` §3 (the `open-pr` skill
  fills it).
- Cite rule ids (`ARCH-xx`, `DEV-xx`) in reviews and in PRs that need an exception.

## 6. Commands

```text
cargo xtask ci        # fmt, clippy, tests, license and invariant checks
cargo xtask bench     # microbenchmarks
cargo xtask analyze   # frame-ledger report
```
(Available from milestone M2.)
