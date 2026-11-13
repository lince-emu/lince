# PoC-11 — Rust↔C++ boundary

- **Status:** Done on Windows x64. The Linux x64 result comes from CI on the pull request (see
  "Linux").
- **Decides:** ADR-0030 details: binding generator, compiler discovery, minimum clang version.
- **Code:** [`poc/cpp-boundary/`](../../poc/cpp-boundary/README.md).

## Hardware and toolchain

| Item | Value |
|------|-------|
| CPU | AMD Ryzen 5 7600 (6 cores, 12 threads) |
| OS | Windows 11 Pro 10.0.26300 |
| Compiler | LLVM 23.1.5 (`clang-cl`, official installer via winget) with the MSVC 14.51 libraries |
| Rust | 1.100.0 |

## Method

- **Build:** `build.rs` compiles `cpp/src/boundary.cpp` with `cc` 1.11.2: C++20, `/GR-
  /EHs-c-` (no RTTI, no exceptions), warnings as errors.
- **Bindings:** generated from the header by bindgen 0.73.2 (feature `regen`), formatted by
  rustfmt and checked in. bindgen also emits compile-time size, alignment and field-offset
  assertions for every boundary struct; CI regenerates and compares the file.
- **Checks in `cargo xtask ci`:** `clang-format --dry-run -Werror`, `clang-tidy` (`.clang-tidy`,
  warnings as errors) and the C++ test driver built with
  `-fsanitize=address,undefined -fno-sanitize-recover=all` and run.
- **Call cost:** `bench` (release), 100 million calls per run, median of 7 runs:
  - a `#[inline(never)]` Rust function;
  - `lb_add` called from Rust;
  - `read_u32` called back from C++ inside one `lb_sum_range` call.
  No cross-language LTO, so every C++ call is a real call.
- **Build time:** debug `cargo build` after `cargo clean`, after touching `boundary.cpp`, and
  with no change.

## Results (Windows x64)

| Measurement | Result |
|-------------|--------|
| Rust non-inlined call | 1.01 ns/call (min 1.01, max 1.07) |
| Rust → C++ (`lb_add`) | 0.82 ns/call (min 0.81, max 0.90) |
| C++ → Rust callback | 1.20 ns/call (min 1.19, max 1.22) |
| Clean debug build (no bindgen) | 2.60 s |
| Incremental, `boundary.cpp` touched | 1.22 s |
| No-op rebuild | 0.09 s |
| Extra build time of the `regen` feature (bindgen + libclang) | 8.89 s |
| clang-format, clang-tidy, ASan + UBSan run | Clean |

## Linux

*(Filled from the pull request's CI run: clang version of `ubuntu-latest`, and whether
`cargo xtask ci` and the PoC-11 step pass.)*

## Comparison with the hypothesis

- **`cc`-built C++20 links on Windows with `clang-cl` from a clean checkout:** met.
- **A boundary call costs about a non-inlined call:** met. Both directions are within
  0.2 ns of a non-inlined Rust call, so the C ABI adds no measurable overhead per call on this
  CPU. The Rust → C++ call measured slightly faster than the Rust baseline; that difference
  comes from code layout, not from the boundary.

## Verdict

**Success on Windows; Linux pending CI.** Proposed ADR-0030 details:

1. **Binding generator:** bindgen, run by a developer tool and checked in, with a CI check
   that it matches. It writes the layout assertions automatically and costs about 9 s of
   build time only when regenerating. Hand-written declarations would avoid that cost but
   need hand-written layout tests for every struct.
2. **Compiler discovery:** `CXX` when set, otherwise the LLVM installer's default path on
   Windows, then `PATH`. This needs no configuration on a machine with the official LLVM
   installer, and it matches what `cc` does by default.
3. **Header details found by the PoC:**
   - `noexcept` must be on the declarations too, so the header uses an `LB_NOEXCEPT` macro;
   - status codes are `static const` values of a fixed-width type, because bindgen does not
     translate macros with casts.
4. **Minimum clang version:** the lowest version that passes CI (Linux `ubuntu-latest`,
   recorded above), until a lower one is tested.
