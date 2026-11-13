#!/usr/bin/env bash
# Static checks for the invariants in AGENTS.md §2 that can be detected by text search.
# Usage:
#   check-invariants.sh                 # files changed vs. origin/main (or HEAD) + untracked
#   check-invariants.sh --files a b ... # only these files
#   check-invariants.sh --stop-hook     # as Stop hook: reads stdin, avoids loops
# Exit 0 = clean, 2 = violations (reported on stderr).
# These checks are a safety net, not a proof. Reviews (invariant-reviewer agent) still apply.
source "$(dirname "$0")/lib.sh"
cd "$PROJECT_DIR" || exit 0

mode="changed"; files=()
case "${1:-}" in
  --files) shift; mode="files"; files=("$@") ;;
  --stop-hook)
    mode="changed"
    input="$(cat 2>/dev/null || true)"
    # Do not keep Claude in a loop: if this Stop was already caused by a hook, let it stop.
    [ "$(json_get "$input" stop_hook_active)" = "true" ] && exit 0 ;;
esac

if [ "$mode" = "changed" ]; then
  [ -d crates ] || exit 0
  base="$(git merge-base HEAD origin/main 2>/dev/null || git rev-parse HEAD 2>/dev/null || true)"
  mapfile -t files < <( { [ -n "$base" ] && git diff --name-only "$base" -- ; git ls-files --others --exclude-standard; } 2>/dev/null | sort -u )
fi

# Crates allowed to contain `unsafe` (docs/04-rust-architecture.md §2, ADR-0001, ADR-0026).
UNSAFE_OK='^crates/(lince-mem|lince-jit|lince-render-vk|lince-ffi-[a-z0-9-]+)/'
# Crates on render-critical threads: no blocking file I/O (AGENTS.md invariant 3).
CRITICAL='^crates/(lince-gpu-cmd|lince-gpu-ampere|lince-pipeline)/src/'

violations=()
add() { violations+=("$1"); }

for f in "${files[@]}"; do
  f="$(rel_path "$f")"
  [ -f "$f" ] || continue
  # LEGAL-30: SPDX header in every Rust and shell source file of the project.
  case "$f" in
    crates/*.rs|tools/*.rs|xtask/*.rs|poc/*.rs|crates/*.sh|tools/*.sh|xtask/*.sh)
      if ! head -n 5 "$f" | grep -q 'SPDX-License-Identifier: GPL-3.0-or-later'; then
        add "$f: missing SPDX header (LEGAL-30): '// SPDX-License-Identifier: GPL-3.0-or-later'."
      fi ;;
  esac
  # STRUCT-02 / STRUCT-03: module layout and names.
  case "$f" in
    crates/*/mod.rs|crates/*/src/*/mod.rs|crates/*/src/mod.rs)
      add "$f: mod.rs files are not used; use foo.rs + foo/ (STRUCT-02)." ;;
    crates/*/src/*utils.rs|crates/*/src/*helpers.rs|crates/*/src/*misc.rs|crates/*/src/*/utils.rs|crates/*/src/*/helpers.rs|crates/*/src/*/misc.rs)
      add "$f: module names utils/helpers/misc are not allowed; name it after its responsibility (STRUCT-03)." ;;
  esac

  case "$f" in
    *.rs)
      # ARCH-24: unsafe_code may only be allowed in the designated crates.
      if grep -nE 'allow\(unsafe_code\)' "$f" >/dev/null && [[ "$f" == crates/* ]] && ! [[ "$f" =~ $UNSAFE_OK ]]; then
        add "$f: allow(unsafe_code) outside the crates allowed by docs/04-rust-architecture.md (ARCH-24)."
      fi
      # Invariant 5: no global GPU waits outside teardown code.
      if grep -nE '\b(device_wait_idle|queue_wait_idle|vkDeviceWaitIdle|vkQueueWaitIdle)\b' "$f" \
           | grep -v 'INVARIANT-ALLOW: teardown' >/dev/null; then
        add "$f: global GPU wait (device/queue wait idle). Only teardown code may do this, marked '// INVARIANT-ALLOW: teardown'."
      fi
      # Invariant 8: unsafe only in designated crates, always with a SAFETY comment.
      if grep -nE '\bunsafe[[:space:]]*(\{|fn|impl)' "$f" >/dev/null; then
        if ! [[ "$f" =~ $UNSAFE_OK ]] && [[ "$f" == crates/* ]]; then
          add "$f: 'unsafe' outside the crates allowed by docs/04-rust-architecture.md."
        elif ! grep -q 'SAFETY:' "$f"; then
          add "$f: 'unsafe' block without a '// SAFETY:' comment."
        fi
      fi
      # Invariant 2: pipeline/shader creation only in compile workers.
      if grep -nE '\b(create_graphics_pipelines|create_compute_pipelines|create_shaders_ext|vkCreateGraphicsPipelines|vkCreateShadersEXT)\b' "$f" >/dev/null \
         && ! grep -q 'INVARIANT-ALLOW: compile-worker' "$f"; then
        add "$f: pipeline/shader creation call. Only compile-worker code may do this, marked '// INVARIANT-ALLOW: compile-worker'."
      fi
      # Invariant 3: no blocking file I/O in render-critical crates.
      if [[ "$f" =~ $CRITICAL ]] && grep -nE '\b(std::fs::|File::open|File::create|read_to_end|OpenOptions)\b' "$f" >/dev/null; then
        add "$f: file I/O in a render-critical crate. Route it through lince-cache's I/O thread."
      fi
      ;;
    crates/*/Cargo.toml)
      if grep -nE '^[[:space:]]*(tokio|async-std|smol|rayon)[[:space:]]*=' "$f" >/dev/null; then
        add "$f: async runtime or rayon in an emulation crate (ADR-0017). Tools only."
      fi
      ;;
    *.keys)
      add "$f: key files must never be in the repository." ;;
  esac
done

if [ "${#violations[@]}" -gt 0 ]; then
  {
    echo "Lince invariant check failed (AGENTS.md §2):"
    for v in "${violations[@]}"; do echo "  - $v"; done
    echo "Fix these before finishing, or ask the maintainer if an exception is justified."
  } >&2
  exit 2
fi
exit 0
