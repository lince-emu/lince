# Lince-NX

**Lince-NX** (Recomp Foundry) is a Nintendo Switch 2 emulator written from scratch in Rust,
with Nintendo Switch 1 support planned later in the same project.

Its defining goal:

> **Play immediately and keep playing smoothly** — no shader precompilation, no cache loading
> before the game starts, no warm-up session, and no stutter caused by shader or pipeline
> compilation, even with a completely empty cache.

The project is in the **architecture phase**. There is no emulator code yet, by design: the
riskiest assumptions are being prototyped first.

## How it plans to get there

- **Run first, compile later.** Unseen guest shaders run immediately on a GPU interpreter
  (Tier 0) that needs no per-shader compile; native shaders (Tier 1, Tier 2) are compiled in the
  background and swapped in atomically. Draws are never discarded.
- **Compile when the game uploads, not when it draws.** New shader code is detected when it is
  written to GPU memory, typically during the game's own loading screens.
- **A cache that is never required.** Content-addressed, indexed, lazily resolved off the render
  thread; startup cost does not grow with cache size.
- **Governed background work.** A frame-aware governor throttles compilation and cache
  maintenance when deadlines are at risk.
- **Measured, not claimed.** An always-on frame ledger attributes every frame-time spike to a
  cause.

## Documentation

| Document | Content |
|----------|---------|
| [00 — Architecture workshop](docs/00-architecture-workshop.md) | Charter review: requirements, contradictions, risks |
| [01 — Research report](docs/01-research-report.md) | Eglantine, Lichii, Dolphin, Cemu, yuzu, Ryujinx — with commits and evidence levels |
| [02 — Cache architecture](docs/02-cache-architecture.md) | Highest-priority design |
| [03 — Shader execution](docs/03-shader-execution.md) | Tiered execution, GPU interpreter, discovery-time compile |
| [04 — Rust architecture](docs/04-rust-architecture.md) | Crates, threads, ownership, dependencies |
| [05 — Performance validation](docs/05-performance-validation.md) | Metrics, ledger, spike attribution, targets |
| [06 — PoC roadmap](docs/06-poc-roadmap.md) | Ten prototypes that decide the architecture |
| [07 — Development roadmap](docs/07-development-roadmap.md) | Milestones by dependency and risk |
| [08 — Switch 2 hardware](docs/08-switch2-hardware.md) | Known / inferred / unknown |
| [09 — Legal and content](docs/09-legal-and-content.md) | No keys, no firmware, clean-room rules |
| [ADRs](docs/adr/README.md) | 30 architecture decision records |
| [Conventions](docs/conventions/README.md) | Architecture rules, development rules, Conventional Commits, branches/PRs, versioning |
| [Backlog](docs/backlog.md) | Task queue for humans and agents |
| [Autonomous development](docs/autonomous-development.md) | How Claude Code agents work on the project (`.claude/`) |

## Legal

Lince-NX does not include, download or extract keys, firmware or games, and does not decrypt
protected content. See [docs/09-legal-and-content.md](docs/09-legal-and-content.md).
Nintendo Switch is a trademark of Nintendo. This project is not affiliated with Nintendo.

## License

To be decided — see [ADR-0000](docs/adr/0000-project-license.md).
