# Commit Messages — Conventional Commits

Lince follows [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/)
with the project-specific types, scopes and footers defined here.

## 1. Format

```text
<type>(<scope>)<!>: <description>
                                            ← blank line
<body>                                      ← optional
                                            ← blank line
<footers>                                   ← optional
```

| Part | Rule |
|------|------|
| `type` | One of the types in §2. Lowercase. |
| `scope` | One of the scopes in §3. Lowercase. Required, except for `chore`, `build` and `revert` when the change is repository-wide. |
| `!` | Marks a breaking change (§5). Must be paired with a `BREAKING CHANGE:` footer. |
| `description` | Imperative mood ("add", not "added"/"adds"), starts lowercase, no trailing period, header ≤ 72 characters in total. |
| `body` | Explains **why** and any non-obvious **how**. Wrap at 72 columns. Mention threads affected when a render-critical path changes. |
| `footers` | `Token: value` lines (§4). |

## 2. Types

| Type | Use for | Appears in changelog |
|------|---------|----------------------|
| `feat` | New behavior or capability (emulation, tooling, PoC functionality) | Features |
| `fix` | Corrects wrong behavior | Bug fixes |
| `perf` | Improves performance without changing behavior; **requires measurement data** in the body or PR | Performance |
| `refactor` | Code change that neither fixes a bug nor adds a feature | — |
| `test` | Adds or corrects tests only | — |
| `docs` | Documentation, ADRs, research notes, PoC reports, backlog | Documentation (ADRs only) |
| `build` | Cargo manifests, dependencies, toolchain, `xtask` build logic | — |
| `ci` | CI configuration and scripts | — |
| `chore` | Maintenance that fits nothing above (gitignore, editor config) | — |
| `revert` | Reverts a previous commit (§6) | Reverts |

`style` is intentionally not used: formatting is automatic (`cargo fmt`) and never committed
on its own.

## 3. Scopes

Scopes name **what** changed. Use the crate name without the `lince-` prefix, or an area.

### Crates (docs/04-rust-architecture.md §2)

`common` · `trace` · `sched` · `mem` · `cpu-a64` · `kernel` · `hle` · `hle-nx2` ·
`loader` · `gpu-cmd` · `gpu-ampere` · `shader` · `pipeline` · `cache` · `texture` ·
`render-vk` · `audio` · `core` · `app`

### Tools and areas

| Scope | Covers |
|-------|--------|
| `xtask` | Build automation crate |
| `replay`, `frame-analyzer`, `shader-lab` | Tools in `tools/` |
| `poc-<n>` | Proof of concept *n* (`poc/<slug>/` and `docs/poc-reports/poc-<n>-*`) |
| `design` | Numbered design documents `docs/NN-*.md` |
| `adr` | Architecture Decision Records |
| `research` | `docs/research/` |
| `backlog` | `docs/backlog.md`, `docs/decisions/` |
| `conventions` | `docs/conventions/` |
| `agents` | `.claude/`, `AGENTS.md`, `CLAUDE.md` |
| `deps` | Dependency updates (with `build`) |
| `repo` | Repository-wide files (README, LICENSE, gitignore) |

A change touching several crates uses the scope of the crate whose behavior changes, or is
split into several commits. Never use a vague scope such as `misc` or `all`.

## 4. Footers

| Footer | When | Example |
|--------|------|---------|
| `Refs: T-xxx` | Every commit that belongs to a backlog task | `Refs: T-010` |
| `ADR: NNNN[, NNNN]` | Commit implements or changes something governed by an ADR | `ADR: 0015, 0016` |
| `Decision: D-xxx` | Commit acts on a maintainer decision | `Decision: D-002` |
| `BREAKING CHANGE: <text>` | Breaking change (§5) | see below |
| `Reverts: <sha>` | `revert` commits | `Reverts: 1a2b3c4` |
| `Co-Authored-By: <name> <email>` | Work produced with an AI assistant or pair | `Co-Authored-By: Claude <noreply@anthropic.com>` |

Footer order: `BREAKING CHANGE`, `Refs`, `ADR`, `Decision`, `Reverts`, `Co-Authored-By`.

## 5. Breaking changes

A change is breaking when it changes any of:

1. A **public API** of a crate used by another crate or tool.
2. A **persistent format**: cache store format (`MANIFEST`, segment framing, index), frame-ledger
   dump format, capture/replay format, configuration files.
3. **Command-line interface** or configuration keys of `lince-app` or tools.
4. An **Accepted ADR's decision** (the commit that implements a superseding ADR).

Mark it with `!` in the header **and** a `BREAKING CHANGE:` footer describing the impact and
the migration (for cache formats: "old stores are discarded and rebuilt lazily").

## 6. Reverts

```text
revert: feat(cache): add lz4 compression to segment records

This reverts commit 1a2b3c4 because resolve latency P99 regressed by
0.4 ms on HDD hosts (see PR #57).

Reverts: 1a2b3c4
```

## 7. Commit hygiene

- **One logical change per commit.** Formatting, refactors and behavior changes are separate.
- Every commit builds and passes tests (`cargo xtask ci` once available) — bisectability matters
  for an emulator.
- No WIP, "fixup" or "address review" commits on `main`; squash merging (see
  [branches-and-prs.md](branches-and-prs.md)) turns the PR title into the final commit.
- Never commit generated artifacts except those checked in on purpose (built-in SPIR-V
  regenerated by `xtask shaders`).

## 8. Examples

```text
feat(cache): add memory-mapped open-addressing index

Lookups touch one or two cache lines and need no deserialization, so
opening a store reads a bounded amount of data regardless of its size.
Index growth happens on the writer thread with a generation swap.

Refs: T-010
ADR: 0015, 0016
```

```text
fix(gpu-cmd): resume split methods across GPFIFO entries

NVN2 can end an entry inside a method's data and continue it in the
next entry; the parser dropped the remaining words.

Refs: T-042
```

```text
perf(shader-interp): bucket register files by highest register used

Tier-0 fragment draws on the corpus: median GPU time 0.41 ms -> 0.29 ms,
P99 0.92 ms -> 0.63 ms (RTX 3060, 5 runs each, method in PR #61).

Refs: T-055
ADR: 0010
```

```text
feat(cache)!: frame segment records with CRC-32C headers

BREAKING CHANGE: cache store format version 2. Stores written by
format 1 are renamed and deleted in the background; titles start with
an empty cache and rebuild lazily.

Refs: T-071
ADR: 0015
```

```text
docs(adr): propose ADR-0029 for video decoding dependencies

Refs: T-090
```

```text
build(deps): bump ash to 0.38
```

```text
feat(poc-6): add redb comparator to the cache store harness

Refs: T-010
ADR: 0015
```

## 9. Validation

The header is checked against:

```text
^(feat|fix|perf|refactor|test|docs|build|ci|chore|revert)(\([a-z0-9][a-z0-9-]*\))?!?: [a-z0-9].{0,}$
```

plus: total header length ≤ 72, no trailing period, scope present unless the type is `chore`,
`build` or `revert`, a `BREAKING CHANGE:` footer when `!` is used. Merge commits created by
GitHub and `fixup!`/`squash!` commits on feature branches are allowed by the local hook but
never reach `main`.

The same rules run in two places: the `.githooks/commit-msg` hook on every local commit, and
`cargo xtask ci`, which checks every commit from `--base` (default `origin/main`) to `HEAD`
and, in CI, the pull-request title (`--pr-title`). A test keeps the two in agreement.
