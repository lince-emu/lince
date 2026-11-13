# PoC-11 — Rust↔C++ boundary

Standalone crate for [PoC-11](../../docs/06-poc-roadmap.md) (ADR-0030). Report:
[`docs/poc-reports/poc-11-cpp-boundary.md`](../../docs/poc-reports/poc-11-cpp-boundary.md).

| Path | Content |
|------|---------|
| `cpp/include/boundary.h` | The C header: the whole contract (ARCH-50, ARCH-53) |
| `cpp/src/boundary.cpp` | C++20, no exceptions or RTTI, `noexcept` exports (ARCH-51) |
| `cpp/tests/boundary_test.cpp` | Driver run under ASan and UBSan by `cargo xtask ci` |
| `build.rs` | Builds the C++ with the `cc` crate: `clang-cl` on Windows, `clang++` elsewhere |
| `src/ffi/bindings.rs` | Generated from the header by bindgen; checked in, with layout assertions |
| `src/ffi.rs` | Safe wrapper; the only module with `unsafe` |
| `src/bin/bench.rs` | Call-cost benchmark |

## Running

```bash
cargo test
cargo run --release --bin bench
cargo run --features regen --bin regen_bindings            # after changing the header
cargo run --features regen --bin regen_bindings -- --check # what CI runs
```

Needs LLVM (clang, clang-format, clang-tidy, libclang). Without `CXX`, `build.rs` uses
`C:\Program Files\LLVM\bin\clang-cl.exe` on Windows when present, otherwise the compiler on
`PATH`. The C++ format, lint and sanitizer checks run from the workspace root with
`cargo xtask ci`.
