# Versioning and Changelog

## 1. Version numbers

Lince-NX uses [Semantic Versioning 2.0.0](https://semver.org/) for the application and tools.

- **0.y.z** while the architecture is being established (current phase). Any `0.y` bump may
  contain breaking changes; `z` is for fixes only.
- **1.0.0** is not tied to compatibility; it is declared when the core execution and cache
  architecture is stable (all M1–M9 ADRs Accepted and PoC-10 targets met).
- Internal crates are not published and share the workspace version.

## 2. Mapping commits to versions

| Commits since last release | Next version (before 1.0) | Next version (after 1.0) |
|----------------------------|---------------------------|--------------------------|
| Any `!` / `BREAKING CHANGE` | 0.**y+1**.0 | **x+1**.0.0 |
| Any `feat` or `perf` | 0.**y+1**.0 | x.**y+1**.0 |
| Only `fix` | 0.y.**z+1** | x.y.**z+1** |
| Only `docs`, `test`, `refactor`, `build`, `ci`, `chore` | no release needed | no release needed |

## 3. Persistent formats are versioned separately

Formats that live on users' disks carry their own version numbers, independent of SemVer:

| Format | Version lives in | Rule on change |
|--------|------------------|----------------|
| Cache store | `MANIFEST` format field | Old stores discarded and rebuilt lazily; never migrated at boot (doc 02 §8) |
| Shader translation | `translator_version` | Bumped per semantic change; old entries stop matching |
| Frame ledger dump | Ledger header | Analyzer reads the current and previous version |
| Capture/replay | Capture header | Replayer states which versions it supports |
| Configuration | `config_version` key | Migrated on load with a logged message |

A change to any of them is a breaking change in the commit message (see
[commits.md §5](commits.md#5-breaking-changes)).

## 4. Changelog

- `CHANGELOG.md` is **generated** from Conventional Commits on `main`, grouped by type as in
  [commits.md §2](commits.md#2-types). Planned tool: `git-cliff` (Rust), configured in
  `cliff.toml` (task T-004).
- Hand-written release notes may add context on top, never replace the generated list.

## 5. Tags

Releases are tagged `vX.Y.Z` on `main` by the maintainer. Agents never create tags.
