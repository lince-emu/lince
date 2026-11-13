#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Lince contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# PreToolUse(Bash): blocks commands that are dangerous or that break project policy.
# Exit 2 = block; stderr is shown to Claude as the reason.
source "$(dirname "$0")/lib.sh"
input="$(cat)"
cmd="$(json_get "$input" tool_input.command)"
[ -z "$cmd" ] && exit 0

block() { echo "Blocked by .claude/hooks/guard-bash.sh: $1" >&2; exit 2; }

# History rewriting and direct pushes to protected branches.
grep -Eq 'git[[:space:]]+push[^|;&]*(--force|-f([[:space:]]|$)|--force-with-lease)' <<<"$cmd" \
  && block "force pushes are not allowed. Push a new commit instead."
grep -Eq 'git[[:space:]]+push[^|;&]*[[:space:]](main|master)([[:space:]]|$|:)' <<<"$cmd" \
  && block "never push to main. Push a lince/<task> branch and open a pull request."
grep -Eq 'git[[:space:]]+(commit|merge)[^|;&]*--no-verify' <<<"$cmd" \
  && block "--no-verify skips project checks."
grep -Eq 'gh[[:space:]]+pr[[:space:]]+merge' <<<"$cmd" \
  && block "agents never merge pull requests; the maintainer merges."

# Conventional Commits (docs/conventions/commits.md) for messages given inline with -m.
# Messages written through an editor or -F are checked by .githooks/commit-msg.
if grep -Eq '(^|[;&|[:space:]])git[[:space:]]+commit([[:space:]]|$)' <<<"$cmd" \
   && ! grep -Eq -- '--amend[[:space:]]+--no-edit|--fixup|--squash' <<<"$cmd"; then
  msg="$(python3 - "$cmd" 2>/dev/null <<'PY'
import shlex, sys
try:
    args = shlex.split(sys.argv[1])
except ValueError:
    sys.exit(0)
parts, i = [], 0
while i < len(args):
    a = args[i]
    if a in ("-m", "--message") and i + 1 < len(args):
        parts.append(args[i + 1]); i += 2; continue
    if a.startswith("--message="):
        parts.append(a.split("=", 1)[1])
    elif a.startswith("-m") and len(a) > 2:
        parts.append(a[2:])
    i += 1
print("\n\n".join(parts), end="")
PY
)"
  if [ -n "$msg" ] && [ -f "$PROJECT_DIR/.githooks/commit-msg" ]; then
    tmp="$(mktemp)"; printf '%s\n' "$msg" > "$tmp"
    if ! out="$(bash "$PROJECT_DIR/.githooks/commit-msg" "$tmp" 2>&1)"; then
      rm -f "$tmp"; block "$out"
    fi
    rm -f "$tmp"
  fi
fi

# Piping downloads into a shell.
grep -Eq '(curl|wget)[^|]*\|[[:space:]]*(sudo[[:space:]]+)?(ba|z)?sh' <<<"$cmd" \
  && block "piping downloads into a shell is not allowed."

# Legal policy: no keys, firmware or decryption tooling (docs/09-legal-and-content.md).
grep -Eiq '(prod|title|dev)\.keys|hactool|lockpick|nxdumptool|firmware[^[:space:]]*\.(zip|nca)' <<<"$cmd" \
  && block "keys, firmware and decryption tools are outside project policy (docs/09-legal-and-content.md)."

# Async runtimes in emulation crates need an ADR (ADR-0017).
grep -Eq 'cargo[[:space:]]+add[^|;&]*\b(tokio|async-std|smol)\b' <<<"$cmd" \
  && block "general-purpose async runtimes are not allowed in emulation crates (ADR-0017). Ask the maintainer."

exit 0
