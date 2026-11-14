// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Safe wrapper over `cpp/include/lince-cpu-a64.h`. This module and its generated `bindings`
//! are the only places with `unsafe`, as ADR-0030 prescribes for `[c++]` crates (ARCH-24).
#![allow(
    unsafe_code,
    reason = "calls into the C++ library; see the SAFETY comments"
)]

mod bindings;

use std::num::NonZeroU16;

use bindings::{LCA_INSN_NAME_MAX, LCA_INSN_UNKNOWN, lca_insn};

/// Buffer size for `lca_insn_name`; the header's constant always fits in `usize`.
const NAME_MAX: usize = LCA_INSN_NAME_MAX as usize;

// `NonZeroU16` encodes "unknown" as `None` only because the header's unknown id is 0.
const _: () = assert!(LCA_INSN_UNKNOWN == 0);

/// The instruction id of `word`; `None` when the table has no entry for it.
pub(crate) fn decode(word: u32) -> Option<NonZeroU16> {
    // SAFETY: a plain value in, a plain value out; the function has no preconditions.
    NonZeroU16::new(unsafe { bindings::lca_decode(word) })
}

/// The name of instruction id `insn`; empty for an id the table does not have.
pub(crate) fn insn_name(insn: lca_insn) -> String {
    let mut buffer = [0u8; NAME_MAX];
    // SAFETY: `buffer` holds LCA_INSN_NAME_MAX bytes, as the header requires, and lives until
    // the call returns; the function only writes through it.
    let length = unsafe { bindings::lca_insn_name(insn, buffer.as_mut_ptr().cast()) };
    // The header promises a length of at most LCA_INSN_NAME_MAX; clamp rather than trust it.
    let length = usize::try_from(length).map_or(NAME_MAX, |l| l.min(NAME_MAX));
    String::from_utf8_lossy(&buffer[..length]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{decode, insn_name};

    #[test]
    fn insn_name_is_empty_when_the_id_is_unknown_or_out_of_range() {
        assert_eq!(insn_name(0), "");
        assert_eq!(insn_name(u16::MAX), "");
    }

    #[test]
    fn decode_returns_none_when_the_word_is_unknown() {
        assert_eq!(decode(0), None);
    }
}
