# Engineering Conventions

How Lince-NX is built and how work is recorded. These conventions apply to humans and AI
agents alike and are enforced by tooling where possible.

| Document | Covers | Enforced by |
|----------|--------|-------------|
| [Architecture rules](architecture-rules.md) | `ARCH-xx`: frame delivery, shaders/cache, structure, correctness, decisions | `invariant-reviewer` agent, `.claude/hooks/check-invariants.sh`, review |
| [Development rules](development-rules.md) | `DEV-xx`: workflow, Rust style, errors, concurrency, `unsafe`, tests, performance, logging, dependencies; review checklist | Clippy, `cargo deny`, tests, review |
| [Commit messages](commits.md) | Conventional Commits: types, scopes, footers, breaking changes | `.githooks/commit-msg`, `.claude/hooks/guard-bash.sh`, CI (T-004) |
| [Branches and pull requests](branches-and-prs.md) | Branch names, PR titles and body, size, review, squash merge | `.claude/hooks/guard-bash.sh`, `open-pr` skill |
| [Versioning and changelog](versioning.md) | SemVer, persistent-format versions, generated changelog | Release process (T-004) |

Precedence when documents disagree: Accepted ADRs → architecture rules → development rules →
everything else. A rule changes only through an ADR (architecture) or a PR to this folder
approved by the maintainer (development and process).

## One-time setup (every clone)

```bash
git config core.hooksPath .githooks
```

This activates the repository's `commit-msg` hook, which rejects messages that do not follow
[commits.md](commits.md). On Windows the hook runs in the Git Bash shipped with Git for Windows.

## Language

All code, comments, commit messages, branch names, PR titles and descriptions are written in
**English**, like the rest of the repository.
