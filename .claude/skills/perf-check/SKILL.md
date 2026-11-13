---
name: perf-check
description: Measures and reports the performance impact of the current change on Lince-NX critical paths (benchmarks, ledger summaries) following docs/05-performance-validation.md. Use before opening a PR that touches a render-critical thread or when a performance claim is made.
argument-hint: "[benchmark filter]"
context: fork
agent: perf-analyst
---

Measure the performance impact of the current branch versus `main`.

1. Identify which critical paths the diff touches (`git diff $(git merge-base HEAD origin/main)`).
2. Run the relevant benchmarks on `main` and on the branch (filter: `$ARGUMENTS`), same
   machine, ≥ 3 runs each. Use `cargo xtask bench` when available, otherwise `cargo bench`.
3. If replays or ledger dumps exist for the affected subsystem, analyze them too.
4. Report: median and P99 per benchmark, deltas with variance, whether each delta is inside
   noise, and a verdict (improvement / neutral / regression).
5. Write the summary in a form ready to paste into the PR's "Performance evidence" section.

Never estimate numbers that were not measured.
