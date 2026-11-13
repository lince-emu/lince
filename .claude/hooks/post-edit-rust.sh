#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Lince contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# PostToolUse(Edit|Write): formats edited Rust files and flags obvious invariant breaks early.
source "$(dirname "$0")/lib.sh"
input="$(cat)"
file="$(json_get "$input" tool_input.file_path)"
case "$file" in *.rs) ;; *) exit 0 ;; esac
[ -f "$file" ] || exit 0

if command -v rustfmt >/dev/null 2>&1; then
  rustfmt --edition 2024 --quiet "$file" 2>/dev/null || true
fi

# Fast single-file invariant check; the full check runs in the Stop hook.
if ! report="$(bash "$(dirname "$0")/check-invariants.sh" --files "$file" 2>&1)"; then
  echo "$report" >&2
  exit 2   # shown to Claude; the edit already happened, so it must fix it next.
fi
exit 0
