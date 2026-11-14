// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Instruction decoding through the C++ decoder.

use std::fmt;
use std::num::NonZeroU16;

use crate::ffi;

/// An instruction form from the description table, such as `ADD_IMM` or `LDP_OFF`.
///
/// Values exist only for words the table decodes. Ids depend on the table version; compare
/// and persist names, not ids.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Insn(NonZeroU16);

impl Insn {
    /// The form's name in the description table.
    pub fn name(self) -> String {
        ffi::insn_name(self.0.get())
    }
}

impl fmt::Debug for Insn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

impl fmt::Display for Insn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

/// The instruction form of the A64 word `word`; `None` when the table has no entry for it
/// (an unallocated, reserved or not yet implemented encoding).
pub fn decode(word: u32) -> Option<Insn> {
    ffi::decode(word).map(Insn)
}
