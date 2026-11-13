# 09 — Legal, Clean-Room and Content Boundaries

Status: Proposed · Related ADRs: 0000, 0024, 0026

> This document records engineering policy. It is not legal advice. Have a qualified lawyer
> review it before public release.

The operational, numbered rules derived from this policy are in
[`conventions/legal-rules.md`](conventions/legal-rules.md) (`LEGAL-xx`).

## 1. What Lince-NX will not do

- Ship, download, extract or derive console keys or firmware.
- **Use keys or firmware at all — including files the user supplies.** There is no option,
  import path or documentation for them (ADR-0029).
- Implement decryption of protected console content formats, or any circumvention of
  technical protection measures.
- Ship copyrighted system files (fonts, system applets, shader binaries, game data).
- Publish captured GPU command streams, shader dumps or screenshots that contain game assets.
- Use leaked or NDA material (SDKs, headers, documentation, source code) in any form.

## 2. What it loads

Only content the user has already obtained and decrypted by lawful means, in unencrypted
container layouts (initially: loose ExeFS + RomFS, the layout used by Lichii/Eglantine tests).
The loader must fail with a clear message on encrypted input; it never attempts decryption.

**Consequence to accept:** this narrows the user base compared with projects that decrypt
content using user-provided keys. It is a deliberate legal-risk choice.

## 3. System functionality without firmware

The full plan, item by item, is in [doc 10](10-firmware-independence.md).

All system services are original HLE implementations. Where a game requires system data
(e.g. shared fonts), Lince-NX provides freely licensed substitutes with compatible metrics where
possible, and documents rendering differences.

## 4. Clean-room process

Existing emulators are GPL/MPL/MIT licensed and are useful sources of *knowledge*. To keep
Lince-NX's code original:

1. **Specifications, not code.** Research outputs are written as behavior descriptions
   (protocol tables, register semantics, observed edge cases) in `docs/research/`.
2. **No copying or line-by-line translation** of code from other emulators into Lince-NX.
   Reading other projects for understanding is allowed; transcribing is not.
3. **Attribution of knowledge.** Every research note cites repository, commit and file.
4. **AI assistants follow the same rules** (see `AGENTS.md`): they must not reproduce code from
   other emulators and must cite sources for behavioral claims.
5. If the chosen license is incompatible with a source's license, the stricter separation
   (different people writing spec and implementation) applies for that subsystem.

## 5. Test content

- Shader corpora for decoder tests come from: (a) programs compiled by the team from its own
  CUDA sources for Ampere targets (legal, redistributable SASS for compute); (b) private dumps
  from legally owned games, never committed to the public repository.
- Frame captures used for CI replay stay on private runners.

## 6. Third-party code in the product

Each non-Rust dependency needs an ADR (ADR-0026) covering license compatibility and why a
Rust alternative is not viable.
