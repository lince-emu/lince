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

use bindings::{
    LCA_EXIT_FETCH_ABORT, LCA_EXIT_LIMIT, LCA_EXIT_PC_MISALIGNED, LCA_EXIT_SVC, LCA_EXIT_UNDEFINED,
    LCA_EXIT_UNIMPLEMENTED, LCA_INSN_NAME_MAX, LCA_INSN_UNKNOWN, LCA_NULL_ARGUMENT, LCA_OK,
    LCA_PERM_EXECUTE, LCA_PERM_READ, LCA_PERM_WRITE, lca_exit, lca_insn, lca_memory, lca_region,
    lca_state,
};
use lince_mem::{AddressSpace, Permission};

use crate::error::CpuError;
use crate::interp::State;

pub(crate) use bindings::lca_exit as RawExit;
/// Registers in `lca_state::x`; the header's constant always fits in `usize`.
pub(crate) const GPR_COUNT: usize = bindings::LCA_GPR_COUNT as usize;
pub(crate) const EXIT_SVC: u32 = LCA_EXIT_SVC;
pub(crate) const EXIT_LIMIT: u32 = LCA_EXIT_LIMIT;
pub(crate) const EXIT_UNDEFINED: u32 = LCA_EXIT_UNDEFINED;
pub(crate) const EXIT_UNIMPLEMENTED: u32 = LCA_EXIT_UNIMPLEMENTED;
pub(crate) const EXIT_FETCH_ABORT: u32 = LCA_EXIT_FETCH_ABORT;
pub(crate) const EXIT_PC_MISALIGNED: u32 = LCA_EXIT_PC_MISALIGNED;

// The header's permission bits are lince-mem's (`lca_region::perm` carries `Permission::bits`).
const _: () = assert!(Permission::READ.bits() as u32 == LCA_PERM_READ);
const _: () = assert!(Permission::WRITE.bits() as u32 == LCA_PERM_WRITE);
const _: () = assert!(Permission::EXECUTE.bits() as u32 == LCA_PERM_EXECUTE);

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

/// Most mapped regions one `run` call passes to C++. B1 programs map five or six (segments,
/// bss, stack); the kernel's region count grows with later milestones.
pub(crate) const MAX_REGIONS: usize = 64;

/// Runs `state` over `memory` through `lca_run`; `state` is updated in place.
pub(crate) fn run(
    state: &mut State,
    memory: &mut AddressSpace,
    limit: u64,
) -> Result<lca_exit, CpuError> {
    // A fixed array, not a Vec: `run` is called in a loop on a guest core, which must not
    // allocate in steady state.
    let empty = lca_region {
        base: 0,
        size: 0,
        perm: 0,
        reserved: 0,
    };
    let mut regions = [empty; MAX_REGIONS];
    let mut count = 0;
    for span in memory.regions() {
        let slot = regions
            .get_mut(count)
            .ok_or(CpuError::TooManyRegions { max: MAX_REGIONS })?;
        *slot = lca_region {
            base: span.base.get(),
            size: span.size,
            perm: span.permission.map_or(0, |p| u32::from(p.bits())),
            reserved: 0,
        };
        count += 1;
    }
    let view = lca_memory {
        host_base: memory.host_base().as_ptr(),
        host_size: memory.size(),
        regions: regions.as_ptr(),
        // `count` is at most MAX_REGIONS, which fits in u32.
        region_count: u32::try_from(count).unwrap_or(0),
        reserved: 0,
    };
    let mut raw = lca_state {
        x: state.x,
        sp: state.sp,
        pc: state.pc,
        nzcv: state.nzcv,
        reserved: 0,
    };
    let mut exit = lca_exit {
        reason: 0,
        detail: 0,
        address: 0,
        executed: 0,
    };
    // SAFETY: `raw`, `view`, `regions` and `exit` live until the call returns, and the header
    // only borrows them for the call (ARCH-53). `host_base` is the reservation of `memory`,
    // which this function borrows mutably for the whole call, so nothing else changes its
    // mappings or reads or writes it meanwhile. The first `region_count` entries of `regions`
    // list exactly the mapped regions, each committed at `host_base + base` for as long as it
    // stays mapped (the `AddressSpace::host_base` contract), and C++ accesses no other address;
    // in this version it only reads (instruction fetch).
    let status = unsafe { bindings::lca_run(&raw mut raw, &raw const view, limit, &raw mut exit) };
    match status {
        LCA_OK => {}
        LCA_NULL_ARGUMENT => return Err(CpuError::NullArgument),
        status => return Err(CpuError::Status { status }),
    }
    state.x = raw.x;
    state.sp = raw.sp;
    state.pc = raw.pc;
    state.nzcv = raw.nzcv;
    Ok(exit)
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
