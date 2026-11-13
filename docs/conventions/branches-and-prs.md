# Branches and Pull Requests

## 1. Branches

| Branch | Purpose | Who pushes |
|--------|---------|-----------|
| `main` | Always builds and passes CI. Protected. | Nobody directly; only merged PRs |
| `lince/T-xxx-short-slug` | Work on one backlog task | Humans and agents |
| `lince/decision-D-xxx` | A decision request with no code yet | Agents |
| `lince/fix-short-slug` | Small fix without a backlog task (humans only) | Humans |

Rules:
- Lowercase, hyphen-separated slug, ≤ 40 characters after the task id.
- One task per branch. Rebase on `origin/main` before opening or updating a PR when there are
  conflicts; never force-push a branch someone else is reviewing without telling them.
- Agents never push to `main`, never force-push and never merge (enforced by
  `.claude/hooks/guard-bash.sh` and `.claude/settings.json`).

## 2. Pull request title

The PR title **is** the final commit header after squash merging, so it follows
[commits.md](commits.md) exactly:

```text
feat(cache): add memory-mapped open-addressing index
```

The task id goes in the body (`Refs: T-xxx`), not in the title, to keep headers ≤ 72 characters.

## 3. Pull request body

Use this structure (the `open-pr` skill fills it in):

```markdown
## Task
Refs: T-xxx — <title>

## What and why
<2–5 sentences; link ADRs and design docs.>

## How it was verified
- Tests:
- Clippy:
- Invariant check (.claude/hooks/check-invariants.sh):
- Reviewer (invariant-reviewer):

## Performance evidence
<Required when a render-critical path or a `perf` type is involved; otherwise "n/a".>

## Threads affected
<Which threads run the new code; can any of it block a frame?>

## Breaking changes
<"none", or what breaks and the migration.>

## Open questions for the maintainer
<Or "none".>

## Out of scope / noticed
```

The squash commit body is assembled from "What and why" plus the footers
(`Refs`, `ADR`, `Decision`, `BREAKING CHANGE`, `Co-Authored-By`).

## 4. Size

- Aim for < 400 changed lines excluding tests and generated files.
- Larger changes are split into a stack of PRs, each building and passing tests.

## 5. Review and merge

1. CI green (`cargo xtask ci` once available).
2. `invariant-reviewer` verdict `APPROVE` recorded in the PR (agents) or a human review.
3. Maintainer review. Only the maintainer merges.
4. **Squash merge** with the PR title as header. Delete the branch after merge.
5. The backlog row moves to `Done` in the same PR or in the next backlog update.

## 6. Draft PRs

Open as draft when the task is `Needs decision`, when a PoC has no measurements yet, or when
early feedback is wanted. Drafts are never merged.
