---
name: new-poc
description: Scaffolds a Lince proof of concept from docs/06-poc-roadmap.md — standalone crate in poc/<slug>/, measurement harness, and report skeleton in docs/poc-reports/. Use when starting any PoC task.
argument-hint: "<PoC-N> <slug>"
---

# Start a proof of concept

Input: `$ARGUMENTS` (e.g. `PoC-6 cache-store`).

1. Read the PoC's section in `docs/06-poc-roadmap.md` and the ADRs it decides.
2. Create `poc/<slug>/` as a standalone Cargo crate (not a workspace member):
   - `Cargo.toml` with `publish = false`, edition 2024.
   - `src/lib.rs` for the prototype, `benches/` or `src/bin/` for the harness.
   - `README.md` restating objective, hypothesis, alternatives compared and how to run.
3. Build the **measurement harness first**: what will be measured, units, number of runs,
   output as CSV/JSON in `poc/<slug>/results/` (gitignored unless small).
4. Implement each alternative behind the same harness interface so results are comparable.
5. Write correctness tests before performance runs.
6. Create `docs/poc-reports/poc-<n>-<slug>.md` with sections: hardware & drivers, method,
   results, comparison with hypothesis, verdict (success/failure per the roadmap criteria),
   ADR changes proposed. Leave results empty until measured; never fill in expected numbers.
   Keep the report to about one page: a results table and a verdict, not an essay.
7. PoC code may be throwaway quality, but the invariants that the PoC is testing must hold.
