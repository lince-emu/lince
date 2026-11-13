# CLAUDE.md

@AGENTS.md

Project-specific notes for Claude Code:

- Architecture lives in `docs/`; decisions in `docs/adr/`. Treat Accepted ADRs as binding.
- When a task needs an architectural choice not covered by an Accepted ADR, propose an ADR
  draft and ask before writing implementation code.
- Prefer the smallest change that satisfies the task, with tests.
- Work comes from `docs/backlog.md`; run `/next-task` to take the next one. The full workflow,
  subagents, skills and hooks are described in `docs/autonomous-development.md`.
- Open questions for the maintainer go in `docs/decisions/` (template there) and in the PR.
- Rules: `docs/conventions/` — legal (`LEGAL-xx`), architecture (`ARCH-xx`), structure
  (`STRUCT-xx`), quality (`QUAL-xx`), development (`DEV-xx`), patterns (`PAT-xx`), Conventional
  Commits, branches/PRs, versioning, lints. Legal rules take precedence over everything.
- When writing code, follow `docs/conventions/patterns.md` for recurring problems instead of
  inventing a new approach.
