---
name: research-note
description: Produces a clean-room research note in docs/research/ about hardware, NVN2, Switch 2 services, Vulkan behavior or another emulator's design, with pinned-commit citations and evidence levels. Use for research tasks or before implementing behavior learned from elsewhere.
argument-hint: "<topic>"
context: fork
agent: researcher
---

Research topic: $ARGUMENTS

Follow the rules in your agent definition and `docs/09-legal-and-content.md`:
behavior descriptions only, no code from other projects, every claim with source + commit +
evidence level, guesses marked as guesses.

Write `docs/research/<kebab-topic>.md` with:
1. Question
2. Short answer (3–6 lines)
3. Findings table: source (repo@commit:file or URL) · observation · evidence level · applies to Lince?
4. Open questions
5. Affected ADRs / PoCs / backlog tasks

Return the file path and the short answer.
