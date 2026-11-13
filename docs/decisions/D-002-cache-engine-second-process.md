# D-002: Must a cache engine serve a read-only second process to qualify under ADR-0015?

- **Raised by:** Claude Code (T-010 review) · **Date:** 2026-11-13
- **Blocks:** T-024; the ADR-0015 verdict after PoC-6 measurements
- **Status:** Open (awaiting maintainer reconfirmation of option A). The first version of this
  record, answered on 2026-11-13, rested on a false premise about `redb`; the premise was
  corrected after review on 2026-11-13.

## Question
Doc 02 §5 step 1 and §10 require a second emulator instance to open the store read-only. The
PoC-6 correctness list also includes a "concurrent reader process". ADR-0015's original
revisit clause compared only performance ("within 20% on every PoC-6 metric").

PoC-6 pins `redb` 4.5.0 (`poc/cache-store/Cargo.lock`). That version has
`Builder::set_concurrency_mode`. In the default mode, `ConcurrencyMode::ExclusiveWriter`, a
writer excludes every other process. In `ConcurrencyMode::SingleWriter`, one process writes
and other processes may open the database read-only and follow its commits; readers must be
opened in the writer's mode (`redb-4.5.0/src/db.rs`: the `ConcurrencyMode` enum and the
documentation of `Builder::open_read_only`). The PoC-6 harness opens `redb` in the default
mode, so its `redb` reader shares the writer's `Database` in the same process.

So `redb` can meet the doc 02 requirement, but PoC-6 has not measured it in the mode that
does. No engine's reader has been timed in another process: the custom store's and SQLite's
readers also run in the harness process. Two things need deciding: whether serving a
read-only second process belongs in the ADR-0015 adoption condition, and how PoC-6 measures
the engines against it.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended) Add "serves a read-only second process" to the ADR-0015 adoption condition, and time every engine's reader in another process, with `redb` in `SingleWriter` mode | The comparison uses the configuration Lince would need, alike for every engine. The cost of `SingleWriter` is measured, not assumed | Text change plus harness work | Yes |
| B Keep the condition performance-only and measure `redb` in the default `ExclusiveWriter` mode | No harness work. The numbers describe a configuration doc 02 does not allow: a second instance could not open the store | None | Yes |
| C Add the condition, but drop `redb` from PoC-6 | Less work. Loses a pure-Rust reference that can meet the requirement | Small | Yes |

## Recommendation
Option A. The requirement exists in doc 02, so the adoption rule should include it. `redb`
can meet it in `SingleWriter` mode, so that is the mode worth measuring, and it still tells
us how far the custom store is from a mature engine. The "within 20%" comparison is only fair
if every engine's reader runs in the same setup.

## Consequences
- ADR-0015 lists read-only access from a second process in its Requirements, and its
  "Conditions for revisiting" requires it, citing doc 02 §5 and §10. ADR-0015 stays Proposed.
- The PoC-6 harness must run the `redb` comparator in `SingleWriter` mode, with its reader in
  another process, before the ADR-0015 verdict. The custom store's and SQLite's readers run
  in another process too, so the comparison is like for like. This is task T-024.
- The PoC-6 report no longer claims that `redb` cannot be opened by a second process.
- Until the maintainer reconfirms, the ADR-0015 amendment and T-024 are provisional. Choosing
  option B reverts the ADR-0015 Requirements and revisit-clause changes and drops T-024;
  choosing option C keeps them and removes `redb` from T-024.
