---
name: next-task
description: Lince development loop. Picks the next Ready task from docs/backlog.md (or the task id given), implements it on a branch with tests, runs the checks, opens a pull request, and asks the maintainer when a real decision is needed. Use when asked to "continue", "work on the backlog", "pick the next task" or to run unattended.
argument-hint: "[T-xxx | loop]"
---

# Next task

Arguments: `$ARGUMENTS` — a task id (`T-010`), `loop` (keep going until nothing is Ready), or
empty (one task).

**Budget rule:** spend tokens on code, not on reading. Read only what the task needs; do the
work in this session; do not spawn subagents unless a step below says so.

## 0. Preconditions
1. `git status --porcelain` must be empty; otherwise stop and report.
2. `git switch main && git pull --ff-only origin main` (skip without a remote).

## 1. Select the task
- Read only the backlog table (`grep -E '^\| T-' docs/backlog.md`), not the whole file.
- Use the id given, or the first `Ready` row whose dependencies are `Done`.
- `Impl` tasks need their ADRs Accepted; `PoC`/`Research`/`Docs` may use Proposed ADRs.
  `Maintainer` tasks are never taken.

## 2. Plan
1. Read the task's detail section (`sed -n '/^### T-xxx/,/^### T-/p' docs/backlog.md`) and the
   **Decision** sections of the ADRs it lists. Open a design doc or convention file only for
   the specific question you have; `AGENTS.md` §2 (already in context) is the checklist.
2. Write a short plan (≤ 10 lines) with the files and an **expected size in lines**. If the
   work later exceeds 2× that size or touches an unplanned crate, stop and ask (QUAL-62).
3. Stop and ask only if the task needs an architecture choice no ADR covers, or could involve
   protected material (LEGAL-52). Small choices: pick the simplest option, note it in the PR.

## 3. Branch and implement
1. `git switch -c lince/T-xxx-short-slug`; set the backlog row to `In progress`.
2. Smallest change that meets the acceptance criteria, with tests. Code first; reports and docs
   only as long as the acceptance criteria require.
3. PoC code goes in `poc/<slug>/`; its report is one page (`new-poc` skill).

## 4. Verify
1. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (or `cargo xtask ci`).
2. `bash .claude/hooks/check-invariants.sh`.
3. Only when the diff touches `crates/` code on a render-critical path, `unsafe`, FFI or the
   cache format: run the `invariant-reviewer` subagent once, fix what it finds. Docs, research
   and PoC-only diffs skip it.

## 5. Publish
1. Commit per `docs/conventions/commits.md` with footers `Refs: T-xxx`, `ADR: NNNN`. The row
   stays `In progress` while the PR is open and becomes `Done` when it merges.
2. `open-pr` skill. Never merge.
3. In `loop` mode: back to step 0.

## 6. When a decision is needed
Ask the maintainer directly with the question tool (2–4 options, recommendation first). Write
`docs/decisions/D-xxx-<slug>.md` only when running unattended or when the decision blocks
other tasks; keep it under 30 lines.

## Final report
Task and status, PR link, check results, open questions. A few lines, no recap of the plan.
