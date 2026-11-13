# Lint and Tooling Configuration

The rules in this folder are enforced by tools wherever possible. Root files:

| File | Tool | Status |
|------|------|--------|
| `rustfmt.toml` | `cargo fmt` | Active |
| `clippy.toml` | `cargo clippy` thresholds and disallowed APIs | Active |
| `deny.toml` | `cargo deny` licenses, advisories, bans | Active |
| `[workspace.lints]` in `Cargo.toml` | rustc + clippy lint levels | Applied in T-003 (snippet below) |

## Workspace lints (to be placed in the root `Cargo.toml` by T-003)

Every crate opts in with `[lints] workspace = true`.

```toml
[workspace.lints.rust]
unsafe_code = "deny"                 # allowed per crate with #![allow(unsafe_code)] — ARCH-24 crates only
unsafe_op_in_unsafe_fn = "deny"      # DEV-40
missing_docs = "warn"                # DEV-74
unreachable_pub = "warn"             # STRUCT-05
rust_2018_idioms = { level = "warn", priority = -1 }
let_underscore_drop = "warn"         # QUAL-41
static_mut_refs = "deny"             # QUAL-51

[workspace.lints.clippy]
# Groups
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }

# Safety and panics
undocumented_unsafe_blocks = "deny"  # DEV-41: every unsafe block has // SAFETY:
missing_safety_doc = "deny"          # DEV-42
unwrap_used = "deny"                 # QUAL-44 (tests allowed via clippy.toml)
todo = "deny"                        # QUAL-43
unimplemented = "deny"               # QUAL-43
dbg_macro = "deny"                   # QUAL-43
print_stdout = "deny"                # QUAL-43 (tools opt out locally)
print_stderr = "deny"

# Structure and readability
wildcard_enum_match_arm = "warn"     # QUAL-22
fn_params_excessive_bools = "deny"   # QUAL-05
struct_excessive_bools = "warn"
excessive_nesting = "deny"           # QUAL-02 (threshold in clippy.toml)
too_many_lines = "deny"              # QUAL-04 (threshold in clippy.toml)
mod_module_files = "deny"            # STRUCT-02: forbids mod.rs
wildcard_imports = "deny"            # STRUCT-06
self_named_module_files = "allow"    # foo.rs + foo/ is the chosen style

# Types and casts
cast_possible_truncation = "deny"    # QUAL-23
cast_sign_loss = "deny"
cast_possible_wrap = "deny"
large_types_passed_by_value = "warn" # QUAL-35
large_stack_arrays = "warn"

# Pedantic lints we deliberately relax
module_name_repetitions = "allow"    # conflicts with re-export style in lib.rs
must_use_candidate = "allow"         # noisy; use #[must_use] deliberately
missing_errors_doc = "allow"         # error enums document themselves (PAT-01)
```

Emulation crates whose arithmetic emulates the guest additionally set, in their `lib.rs`:

```rust
#![warn(clippy::arithmetic_side_effects)] // QUAL-24: guest arithmetic must be explicit
```

## Exceptions

A local `#[allow(clippy::…)]` or `#[expect(clippy::…)]` must carry a `reason`:

```rust
#[expect(clippy::cast_possible_truncation, reason = "guest register is 32-bit; upper bits are zero by decode")]
```

`#[expect]` is preferred to `#[allow]` because it fails when the exception becomes unnecessary.
