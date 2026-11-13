---
name: next-task
description: Autonomous Lince-NX development loop. Picks the next Ready task from docs/backlog.md (or the task id given), implements it on a branch with tests, gets an independent invariant review, opens a pull request, and stops to ask the maintainer whenever a decision is needed. Use when asked to "continue", "work on the backlog", "pick the next task" or to run unattended.
argument-hint: "[T-xxx | loop]"
---

# Next task

Arguments: `$ARGUMENTS` — a task id (`T-010`), `loop` (keep going until nothing is Ready), or
empty (one task).

## 0. Preconditions
1. `git status --porcelain` must be empty. If not, stop and report what is uncommitted.
2. `git fetch origin` then `git switch main && git pull --ff-only origin main` (skip if there is
   no remote yet; say so in the report).
3. Read `AGENTS.md`, `docs/conventions/README.md` and `docs/backlog.md`.

## 1. Select the task
- If an id was given, use it. Otherwise take the **first** row with status `Ready` whose
  `Depends on` tasks are all `Done`.
- Eligibility check:
  - `Impl` tasks need every listed ADR **Accepted**. If one is not, set the task to
    `Needs decision` (step 6) and pick another.
  - `PoC`, `Research` and `Docs` tasks may proceed with `Proposed` ADRs: they exist to produce
    the evidence those ADRs wait for.
  - Tasks marked `Maintainer` are never taken by an agent.
- If nothing is eligible, stop and report the blocked tasks and what each needs.

## 2. Plan
1. Read the task's detail section, its ADRs and design docs.
2. If any design choice is not settled by those documents, consult the `architect` subagent.
   If it answers "not covered", go to step 6.
3. Write a 5–15 line plan: files (placed per `docs/conventions/code-structure.md`), public
   APIs, canonical patterns used (`patterns.md`), tests, threads involved, risks, and an
   **expected size in lines**. During implementation, if the change grows past 2× that size
   or touches a crate not in the plan, stop and go to step 6 (QUAL-62).
4. If the task could need protected or leaked material in any way, stop and go to step 6
   (LEGAL-52).

## 3. Branch and implement
1. `git switch -c lince/T-xxx-short-slug`.
2. Change the task row to `In progress` in `docs/backlog.md`.
3. Implement the smallest change that meets the acceptance criteria. Use domain subagents when
   relevant (`shader-engineer`, `test-engineer`, `researcher`, `perf-analyst`). For tasks that
   are independent of each other in `loop` mode, the `implementer` subagent can work in its own
   worktree.
4. PoC code goes in `poc/<slug>/` as a standalone crate and ends with a report in
   `docs/poc-reports/` (use the `new-poc` skill to scaffold it).

## 4. Verify
1. Formatting, clippy with `-D warnings`, tests (`cargo xtask ci` once it exists; otherwise
   the cargo commands in the crate or PoC directory).
2. `bash .claude/hooks/check-invariants.sh`.
3. Delegate to the `invariant-reviewer` subagent. Fix every finding and re-run it, at most
   3 rounds. If it still requests changes, stop and go to step 6 with its findings.

## 5. Publish
1. Set the task row to `In review` and add the PR link after creation.
2. Commit following `docs/conventions/commits.md` (Conventional Commits):
   `<type>(<scope>): <description>`, a body that explains why, and footers
   `Refs: T-xxx`, `ADR: NNNN`, `BREAKING CHANGE:` when applicable. Split unrelated changes
   into separate commits.
3. Use the `open-pr` skill. Never merge; the maintainer merges.
4. If `loop`: return to `main` and go back to step 0.

## 6. When a decision is needed
1. Create `docs/decisions/D-xxx-<slug>.md` from `docs/decisions/TEMPLATE.md`: the question,
   options with trade-offs, your recommendation, and what is blocked.
2. Set the task to `Needs decision` with a link to that file. Commit on the task branch (or on
   a `lince/decision-D-xxx` branch if no code exists yet) and open a draft PR so the
   maintainer sees it.
3. If the maintainer is present, ask with the question tool (2–4 options, recommendation first).
   If running unattended, put the question at the top of the final report.
4. In `loop` mode, continue with the next independent task.

## Final report (always)
- Task(s) handled and their new status, branch and PR links.
- Verification results (tests, clippy, invariant check, reviewer verdict).
- Decisions requested, with file links.
- Anything noticed but deliberately left out of scope.
