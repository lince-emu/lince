#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Lince contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Shared helpers for Lince Claude Code hooks.
# Works with jq when available, falls back to python3, then to a crude sed extraction.

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"

# json_get <json> <dotted.path>  -> prints the string value (empty if missing)
json_get() {
  local json="$1" path="$2"
  if command -v jq >/dev/null 2>&1; then
    jq -r ".${path} // empty" <<<"$json" 2>/dev/null
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c '
import json, sys
data = json.loads(sys.argv[1] or "{}")
for key in sys.argv[2].split("."):
    data = data.get(key) if isinstance(data, dict) else None
print("" if data is None else data if isinstance(data, str) else json.dumps(data))
' "$json" "$path" 2>/dev/null
  else
    local key="${path##*.}"
    sed -n "s/.*\"${key}\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" <<<"$json" | head -n1
  fi
}

# json_escape <text> -> JSON string literal (with quotes)
json_escape() {
  if command -v jq >/dev/null 2>&1; then
    printf '%s' "$1" | jq -Rs .
  elif command -v python3 >/dev/null 2>&1; then
    printf '%s' "$1" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'
  else
    local s="${1//\\/\\\\}"; s="${s//\"/\\\"}"; s="${s//$'\n'/\\n}"; s="${s//$'\t'/ }"
    printf '"%s"' "$s"
  fi
}

# Normalise a path to be relative to the project root.
rel_path() {
  local p="${1//\\//}"
  local root="${PROJECT_DIR//\\//}"
  printf '%s' "${p#"$root"/}"
}
