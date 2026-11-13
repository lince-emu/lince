# Legal Rules

Operational rules that keep Lince-NX legally clean. Ids (`LEGAL-xx`) are cited in reviews. The
policy behind them is in [doc 09](../09-legal-and-content.md).

> This is engineering policy, not legal advice. Have a qualified lawyer review it before the
> first public release, and whenever a rule here is in doubt.

All `LEGAL` rules are **MUST**. When unsure whether something is allowed, stop and ask the
maintainer — do not decide alone.

## A. Protected content and circumvention

| Id | Rule |
|----|------|
| LEGAL-01 | No console keys, firmware, system titles, games or game assets in the repository, issues, PRs, CI artifacts, logs attached to issues, or test fixtures. |
| LEGAL-02 | No code, scripts or documentation that extract keys, decrypt protected formats, or circumvent any technical protection measure — including "helper" scripts and instructions for users. |
| LEGAL-03 | The loader accepts only content that is already decrypted and fails with a neutral message on encrypted input. It never points users to where or how to obtain keys or decryption tools. |
| LEGAL-04 | No links to piracy sites, ROM/ISO downloads, key dumps or firmware mirrors anywhere in the project. |

## B. Sources of knowledge

| Id | Rule |
|----|------|
| LEGAL-10 | **No leaked or confidential material**: no NintendoSDK or NVN/NVN2 SDK files, headers, documentation or symbol lists obtained from leaks; no NDA documents; no leaked source code. Knowledge derived from such material must not enter the project, even paraphrased. |
| LEGAL-11 | Allowed sources: public documentation (e.g. Khronos, Arm, switchbrew), public research, the project's own reverse engineering of legally owned hardware and software, and public open-source emulators under the rules below. Every claim cites its source (doc 09 §4). |
| LEGAL-12 | Names of services, commands and structures come from public documentation when one exists; otherwise the project chooses descriptive names. Never names copied from leaked SDK material. |
| LEGAL-13 | Reverse-engineering notes describe behavior (formats, protocols, semantics); they never reproduce disassembly listings or substantial portions of proprietary code. |
| LEGAL-14 | Vendor tools (e.g. the NVIDIA CUDA toolkit, `nvdisasm`) are used only as their licenses permit, and only on programs the team wrote and compiled itself. |

## C. Code from other projects

| Id | Rule |
|----|------|
| LEGAL-20 | **Lince-NX code is original.** No copying, line-by-line translation (e.g. C++ → Rust) or close paraphrase of code from other emulators (yuzu, Eden, Eglantine, Lichii, Ryujinx, Dolphin, Cemu, …), even where licenses would be compatible. Algorithms and behavior may be learned from them and cited. |
| LEGAL-21 | **AI assistants must not reproduce code from memory** of other projects. If generated code resembles a known project's implementation, rewrite it from the behavior description or discard it. |
| LEGAL-22 | Any exception to LEGAL-20 (e.g. a public-domain table, a permissively licensed algorithm) requires an ADR, the original license notice kept intact, and an entry in `THIRD-PARTY-NOTICES.md`. |
| LEGAL-23 | Code snippets from Q&A sites or blogs are treated as third-party code (LEGAL-22), not as free to paste. |

## D. Licensing of Lince-NX and its dependencies

| Id | Rule |
|----|------|
| LEGAL-30 | Every source file starts with an SPDX header (see below). Files without one are not merged. |
| LEGAL-31 | Every crate's `Cargo.toml` declares `license = "GPL-3.0-or-later"` (per the repository `LICENSE`; ADR-0000). |
| LEGAL-32 | Dependencies must have licenses in the `deny.toml` allow-list, which is limited to GPL-3.0-compatible licenses. Adding a license to the list requires maintainer approval. |
| LEGAL-33 | Non-Rust code in release builds follows ADR-0026 and records its license in `THIRD-PARTY-NOTICES.md`. |
| LEGAL-34 | Fonts, icons, images and sounds shipped with Lince-NX are original or under a license that allows redistribution with GPL software; the license is recorded next to the asset. |
| LEGAL-35 | Contributors agree that their contributions are their own work and licensed under the project license. Whether to require a DCO `Signed-off-by` footer is a maintainer decision (backlog T-006). |

SPDX header for Rust and shell files:

```rust
// SPDX-FileCopyrightText: 2026 Lince-NX contributors
// SPDX-License-Identifier: GPL-3.0-or-later
```

Markdown documentation is covered by the repository license and does not need a header.

## E. Trademarks and public presentation

| Id | Rule |
|----|------|
| LEGAL-40 | "Nintendo", "Nintendo Switch" and game titles are used only descriptively ("an emulator for Nintendo Switch 2 software"). No Nintendo logos, artwork, fonts or trade dress in the project's name, icons, UI or website. |
| LEGAL-41 | Every public page and release carries the non-affiliation notice (README "Legal" section). |
| LEGAL-42 | Screenshots and videos used in documentation or bug reports stay private unless the maintainer has confirmed their use is appropriate; prefer synthetic test scenes. |
| LEGAL-43 | Compatibility reports name games but never include game data, saves, or links to downloads. |

## F. Agent-specific rules

| Id | Rule |
|----|------|
| LEGAL-50 | Agents never read, open, list or search for key, firmware, save or game-content files on the maintainer's machine, even when they are near the project folder. |
| LEGAL-51 | Agents never download games, firmware, keys, SDKs or leaked material, and refuse tasks that require them. |
| LEGAL-52 | When a task appears to need protected material, the agent stops and records a decision request instead of looking for a workaround. |
