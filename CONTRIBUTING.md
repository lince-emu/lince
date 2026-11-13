# Contributing to Lince

Thank you for your interest. Lince is in its architecture phase; the most valuable
contributions right now are research notes, prototype measurements and design review.

## Before you start

1. Read `AGENTS.md` — its invariants apply to every contributor, human or AI.
2. Read the conventions in [`docs/conventions/`](docs/conventions/README.md):
   - [Architecture rules](docs/conventions/architecture-rules.md) (`ARCH-xx`)
   - [Development rules](docs/conventions/development-rules.md) (`DEV-xx`)
   - [Commit messages](docs/conventions/commits.md) — Conventional Commits
   - [Branches and pull requests](docs/conventions/branches-and-prs.md)
   - [Versioning and changelog](docs/conventions/versioning.md)
3. Read `docs/09-legal-and-content.md`. In short: no keys, firmware, game content, or code copied
   from other emulators. Research notes cite repository, commit and file.
4. Activate the commit hook once per clone:

   ```bash
   git config core.hooksPath .githooks
   ```

## How work flows

1. Pick or propose a task in `docs/backlog.md`.
2. Branch `lince/T-xxx-short-slug`.
3. Architectural changes start as an ADR (`docs/adr/README.md` has the template).
4. Prototypes go in `poc/<name>/` and end with a report in `docs/poc-reports/`.
5. Open a PR whose title is a Conventional Commit header; fill in the PR template.
6. Performance claims include measurement data and method.
