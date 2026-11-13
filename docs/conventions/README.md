# Engineering Conventions

How Lince is built and how work is recorded. These conventions apply to humans and AI
agents alike and are enforced by tooling where possible.

## Rules

| Document | Ids | Covers | Enforced by |
|----------|-----|--------|-------------|
| [Legal rules](legal-rules.md) | `LEGAL-xx` | Protected content, sources of knowledge, code from other projects, licensing, trademarks, agent limits | Review, `deny.toml`, `.claude/hooks/`, SPDX check |
| [Architecture rules](architecture-rules.md) | `ARCH-xx` | Frame delivery, shaders/cache, structure, correctness, decisions | `invariant-reviewer`, `.claude/hooks/check-invariants.sh`, review |
| [Code structure](code-structure.md) | `STRUCT-xx` | Crate template, modules, file limits, visibility, imports, module docs, where code goes | Clippy (`mod_module_files`, `wildcard_imports`), review |
| [Code quality](code-quality.md) | `QUAL-xx` | Readability, types, ownership, errors, state, change discipline, tests | Clippy thresholds and lints, review |
| [Development rules](development-rules.md) | `DEV-xx` | Workflow, Rust style, errors, concurrency, `unsafe`, testing, performance, logging, dependencies; review checklist | Clippy, `cargo deny`, tests, review |
| [Canonical patterns](patterns.md) | `PAT-xx` | One project-wide solution for errors, newtypes, decoding, jobs, publication, queues, ledger, platform code, HLE handlers, shader instructions, settings | Review |

## Process

| Document | Covers | Enforced by |
|----------|--------|-------------|
| [Commit messages](commits.md) | Conventional Commits: types, scopes, footers, breaking changes | `.githooks/commit-msg`, `.claude/hooks/guard-bash.sh`, CI (T-004) |
| [Branches and pull requests](branches-and-prs.md) | Branch names, PR titles and body, size, review, squash merge | `.claude/hooks/guard-bash.sh`, `open-pr` skill |
| [Versioning and changelog](versioning.md) | SemVer, persistent-format versions, generated changelog | Release process (T-004) |
| [Lint and tooling configuration](lints.md) | `rustfmt.toml`, `clippy.toml`, `deny.toml`, `[workspace.lints]` | The tools themselves |

## Precedence

When documents disagree: **legal rules** → Accepted ADRs → architecture rules → structure,
quality and development rules → patterns → everything else. Legal and architecture rules change
only through an ADR; the others through a PR to this folder approved by the maintainer.

## One-time setup (every clone)

```bash
git config core.hooksPath .githooks
```

This activates the repository's git hooks (on Windows they run in the Git Bash shipped with
Git for Windows):

| Hook | Blocks |
|------|--------|
| `pre-commit` | Key, firmware and game-content files, and code that refers to key or firmware handling (ADR-0029) |
| `commit-msg` | Messages that do not follow [commits.md](commits.md) |

## Language

All code, comments, commit messages, branch names, PR titles and descriptions are written in
**English**, like the rest of the repository.
