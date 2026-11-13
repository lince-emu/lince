---
name: write-adr
description: Drafts a new Lince Architecture Decision Record (or one that supersedes an existing ADR) using the project template, and updates the ADR index. Use when a design choice is not covered by an Accepted ADR.
argument-hint: "<title> [supersedes ADR-XXXX]"
---

# Write an ADR

Input: `$ARGUMENTS`.

1. Read `docs/adr/README.md` (index + template) and only the ADRs directly related.
2. Pick the next free number. File: `docs/adr/NNNN-kebab-title.md`.
3. Fill every section of the template:
   - **Status:** always `Proposed` (or `Proposed (needs PoC-N)`). Agents never write `Accepted`.
   - **Context:** the problem and the evidence, with citations (doc 01 entries, research notes,
     PoC reports). Mark hypotheses as hypotheses.
   - **Requirements:** charter requirements that constrain the decision.
   - **Alternatives:** at least two real options, including the status quo.
   - **Decision / justification / advantages / disadvantages / risks.**
   - **Validation plan:** measurable, with the PoC or test that proves it.
   - **Conditions for revisiting:** concrete thresholds.
4. If it supersedes an ADR, do **not** edit the old ADR's content; add a line to the index and
   ask the maintainer to change the old status to `Superseded by ADR-NNNN`.
5. Add the ADR to the index table in `docs/adr/README.md`.
6. Keep the ADR to about one page: short context, 2–3 alternatives in a few lines each.
   No architect subagent review unless the maintainer asks for one.
