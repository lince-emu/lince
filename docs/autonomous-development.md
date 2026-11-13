# Autonomous Development with Claude Code

How agents work on Lince-NX, what they may do on their own, and how the maintainer stays in
control.

## 1. The loop

```text
docs/backlog.md ──▶ /next-task ──▶ branch lince/T-xxx ──▶ implement + tests
        ▲                                                     │
        │                         invariant check (hooks) ◀───┤
        │                         invariant-reviewer agent ◀──┘
        │                                   │
  maintainer merges ◀── pull request ◀──────┘
        │
  decision needed? ──▶ docs/decisions/D-xxx.md + draft PR + question to maintainer
```

- Agents **never** merge, push to `main`, force-push, change licenses, accept ADRs, or edit
  the rule files (`AGENTS.md`, `CLAUDE.md`, `.claude/settings.json`, `.claude/hooks/`) without
  approval.
- Every change arrives as a pull request with tests, an invariant-check result and an
  independent review verdict.

## 2. What is in `.claude/`

| Path | Purpose |
|------|---------|
| `settings.json` | Permissions (allow/ask/deny), environment, hooks. Default mode `acceptEdits`. |
| `hooks/session-context.sh` | Injects branch, ADR counts and the next Ready tasks at session start. |
| `hooks/guard-bash.sh` | Blocks force-push, pushes to `main`, `--no-verify`, merges, `curl \| sh`, key/firmware tooling, async runtimes. |
| `hooks/guard-paths.sh` | Asks before editing rule files and Accepted ADRs; denies key files. |
| `hooks/post-edit-rust.sh` | Formats edited `.rs` files and runs the single-file invariant check. |
| `hooks/check-invariants.sh` | Text-level checks for AGENTS.md invariants; runs as the Stop hook. |
| `agents/architect.md` | Plan and ADR review; read-only on code. |
| `agents/implementer.md` | One task in an isolated git worktree. |
| `agents/invariant-reviewer.md` | Independent diff review before every PR. |
| `agents/test-engineer.md` | Differential, property, fuzz, fault-injection and loom tests. |
| `agents/perf-analyst.md` | Frame-time and benchmark analysis. |
| `agents/researcher.md` | Clean-room research notes with citations. |
| `agents/shader-engineer.md` | SASS / IR / SPIR-V / Tier-0 specialist. |
| `skills/next-task` | `/next-task [T-xxx\|loop]` — the autonomous loop. |
| `skills/open-pr` | Push + PR with the project template. |
| `skills/write-adr` | Draft an ADR from the template. |
| `skills/new-poc` | Scaffold a PoC crate and report. |
| `skills/research-note` | Run the researcher on a topic (forked context). |
| `skills/perf-check` | Measure a branch against `main` (forked context). |
| `../.githooks/commit-msg` | Conventional Commits validator (git hook; also called by `guard-bash.sh` for `git commit -m`). |

All rules agents follow are in `docs/conventions/` (architecture `ARCH-xx`, development
`DEV-xx`, commits, branches/PRs, versioning).

## 3. Running it

Interactive (recommended at first):

```text
claude
> /next-task T-010
```

Several tasks in a row:

```text
> /next-task loop
```

Unattended, from a terminal (one task per run; a scheduled job can repeat it):

```text
claude -p "/next-task" --permission-mode acceptEdits
```

Notes:
- The project file sets `defaultMode: acceptEdits`. More permissive modes such as `auto` or
  `bypassPermissions` are **not** honored from a project settings file; if you want them, set
  them in your user settings or pass `--permission-mode` yourself, ideally inside a container
  or VM.
- Personal permission tweaks go in `.claude/settings.local.json` (not committed).
- Hooks need `bash` (on Windows, the Git Bash that Claude Code uses). `jq` or `python3` is
  used when present; the hooks fall back to plain text parsing otherwise.

## 4. Maintainer routine

1. Answer open questions in `docs/decisions/` (or in the draft PRs).
2. Review and merge PRs; change ADR statuses when you accept them.
3. Re-prioritize `docs/backlog.md` (order of `Ready` rows is the order agents take them).
4. Run PoC measurements on real hardware when a PoC task asks for it.

## 5. Limits of the guardrails

The hooks are text searches: they catch obvious violations, not every one. The real
protection is the combination of small PRs, the independent reviewer agent, tests, and the
maintainer's review.
