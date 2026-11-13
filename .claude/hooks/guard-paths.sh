#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Lince-NX contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# PreToolUse(Edit|Write): protects files that agents must not change on their own.
source "$(dirname "$0")/lib.sh"
input="$(cat)"
file="$(json_get "$input" tool_input.file_path)"
[ -z "$file" ] && exit 0
rel="$(rel_path "$file")"

deny() {
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":%s}}\n' "$(json_escape "$1")"
  exit 0
}
ask() {
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":%s}}\n' "$(json_escape "$1")"
  exit 0
}

case "$rel" in
  *.keys|*/prod.keys|*/title.keys)
    deny "Key files are outside project policy (docs/09-legal-and-content.md)." ;;
  AGENTS.md|CLAUDE.md|.claude/settings.json|.claude/hooks/*)
    ask "This file defines the rules agents follow. The maintainer must approve changes to it." ;;
  docs/00-architecture-workshop.md)
    ask "The charter review changes project requirements. The maintainer must approve." ;;
  docs/adr/0*.md)
    # Accepted ADRs are immutable: a change of mind is a new ADR that supersedes the old one.
    if [ -f "$file" ] && grep -q '^- \*\*Status:\*\* Accepted' "$file"; then
      ask "ADR ${rel} is Accepted and immutable. Write a new ADR that supersedes it, unless the maintainer explicitly asked for this edit."
    fi ;;
esac
exit 0
