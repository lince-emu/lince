#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Lince-NX contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# SessionStart hook: injects a short, factual snapshot of the project state.
source "$(dirname "$0")/lib.sh"
cd "$PROJECT_DIR" || exit 0

branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo 'not a git repository')"
dirty="$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
accepted="$(grep -l '^- \*\*Status:\*\* Accepted' docs/adr/0*.md 2>/dev/null | wc -l | tr -d ' ')"
proposed="$(grep -l '^- \*\*Status:\*\* Proposed' docs/adr/0*.md 2>/dev/null | wc -l | tr -d ' ')"
ready="$(grep -E '^\| T-[0-9]+ .*\| Ready \|' docs/backlog.md 2>/dev/null | head -n 5 | cut -d'|' -f2,3 | sed 's/^ */- /')"
in_progress="$(grep -E '^\| T-[0-9]+ .*\| In progress \|' docs/backlog.md 2>/dev/null | cut -d'|' -f2,3 | sed 's/^ */- /')"
waiting="$(grep -E '^\| T-[0-9]+ .*\| Needs decision \|' docs/backlog.md 2>/dev/null | cut -d'|' -f2,3 | sed 's/^ */- /')"
crates="$(ls -d crates/*/ 2>/dev/null | wc -l | tr -d ' ')"

ctx="Lince-NX project snapshot.
Git branch: ${branch}. Uncommitted paths: ${dirty}.
Crates present: ${crates}. ADRs accepted (fully or partially): ${accepted}; proposed: ${proposed}.
Tasks in progress:
${in_progress:-- none}
Tasks waiting for a maintainer decision:
${waiting:-- none}
Next ready tasks in docs/backlog.md:
${ready:-- none}
Project rules are in AGENTS.md; decisions in docs/adr/; the autonomous workflow is the next-task skill."

printf '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":%s}}\n' "$(json_escape "$ctx")"
exit 0
