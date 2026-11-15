// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Safe wrapper over `cpp/include/lince-cpu-a64.h`. This module and its generated `bindings`
//! are the only places with `unsafe`, as ADR-0030 prescribes for `[c++]` crates (ARCH-24).
#![allow(
    unsafe_code,
    reason = "calls into the C++ library; see the SAFETY comments"
)]

mod bindings;

use std::ffi::c_void;
use std::num::NonZeroU16;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use bindings::{
    LCA_EXIT_DATA_ABORT, LCA_EXIT_FETCH_ABORT, LCA_EXIT_INTERRUPTED, LCA_EXIT_LIMIT,
    LCA_EXIT_PC_MISALIGNED, LCA_EXIT_SVC, LCA_EXIT_UNDEFINED, LCA_EXIT_UNIMPLEMENTED,
    LCA_INSN_NAME_MAX, LCA_INSN_UNKNOWN, LCA_NULL_ARGUMENT, LCA_OK, LCA_PERM_EXECUTE,
    LCA_PERM_READ, LCA_PERM_WRITE, lca_callbacks, lca_exclusive, lca_exit, lca_insn, lca_memory,
    lca_region, lca_state,
};
use lince_common::GuestVa;
use lince_mem::{AddressSpace, Permission, SpaceView};

use crate::error::CpuError;
use crate::interp::{Exclusive, State};

pub(crate) use bindings::lca_exit as RawExit;
/// Registers in `lca_state::x`; the header's constant always fits in `usize`.
pub(crate) const GPR_COUNT: usize = bindings::LCA_GPR_COUNT as usize;
pub(crate) const EXIT_SVC: u32 = LCA_EXIT_SVC;
pub(crate) const EXIT_LIMIT: u32 = LCA_EXIT_LIMIT;
pub(crate) const EXIT_UNDEFINED: u32 = LCA_EXIT_UNDEFINED;
pub(crate) const EXIT_UNIMPLEMENTED: u32 = LCA_EXIT_UNIMPLEMENTED;
pub(crate) const EXIT_FETCH_ABORT: u32 = LCA_EXIT_FETCH_ABORT;
pub(crate) const EXIT_PC_MISALIGNED: u32 = LCA_EXIT_PC_MISALIGNED;
pub(crate) const EXIT_DATA_ABORT: u32 = LCA_EXIT_DATA_ABORT;
pub(crate) const EXIT_INTERRUPTED: u32 = LCA_EXIT_INTERRUPTED;
/// The `detail` of a data abort on a write.
pub(crate) const ACCESS_WRITE: u32 = LCA_PERM_WRITE;

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

/// What the callbacks of one `lca_run` call reach through `lca_callbacks::context`.
struct Context<'v, 'a> {
    memory: SpaceView<'v>,
    interrupt: Option<&'a AtomicBool>,
}

/// The safe-point poll (`lca_callbacks::interrupted`): nonzero once the caller's flag is set.
unsafe extern "C" fn interrupted(context: *mut c_void) -> i32 {
    // SAFETY: `context` is the `Context` that `run` holds for the whole `lca_run` call; C++
    // calls this only during that call, on the same thread.
    let context = unsafe { &*context.cast::<Context<'_, '_>>() };
    i32::from(
        context
            .interrupt
            .is_some_and(|flag| flag.load(Ordering::Acquire)),
    )
}

/// The slow-path read (`lca_callbacks::read`): `lince-mem` checks that every byte is mapped
/// and readable.
unsafe extern "C" fn slow_read(context: *mut c_void, va: u64, out: *mut u8, size: u64) -> i32 {
    let Ok(size) = usize::try_from(size) else {
        return 1;
    };
    // SAFETY: `context` is the `Context` (and its `SpaceView`) that `run` holds for the whole `lca_run` call; C++
    // calls this only during that call, on the same thread. `out` holds `size` writable bytes
    // that C++ owns and does not otherwise access during the call.
    let (memory, buf) = unsafe {
        (
            &(*context.cast::<Context<'_, '_>>()).memory,
            std::slice::from_raw_parts_mut(out, size),
        )
    };
    i32::from(memory.read_bytes(GuestVa::new(va), buf).is_err())
}

/// The slow-path write (`lca_callbacks::write`): `lince-mem` checks that every byte is mapped
/// and writable before it copies any.
unsafe extern "C" fn slow_write(context: *mut c_void, va: u64, data: *const u8, size: u64) -> i32 {
    let Ok(size) = usize::try_from(size) else {
        return 1;
    };
    // SAFETY: as in `slow_read`; `data` holds `size` readable bytes that C++ does not change
    // during the call.
    let (memory, data) = unsafe {
        (
            &(*context.cast::<Context<'_, '_>>()).memory,
            std::slice::from_raw_parts(data, size),
        )
    };
    i32::from(memory.write_bytes(GuestVa::new(va), data).is_err())
}

/// The store-exclusive (`lca_callbacks::compare_exchange`): one host compare-and-swap on the
/// guest bytes (ADR-0004), after `lince-mem` confirms they are mapped writable.
unsafe extern "C" fn compare_exchange(
    context: *mut c_void,
    va: u64,
    size: u64,
    expected: u64,
    desired: u64,
    stored: *mut u32,
) -> i32 {
    // SAFETY: as in `slow_read`. `stored` points to a u32 that C++ owns for the call.
    let (memory, stored) = unsafe { (&(*context.cast::<Context<'_, '_>>()).memory, &mut *stored) };
    let writable = memory.query(GuestVa::new(va)).is_some_and(|span| {
        span.permission
            .is_some_and(|p| p.contains(Permission::WRITE))
            && va.checked_add(size).is_some_and(|end| {
                span.base
                    .get()
                    .checked_add(span.size)
                    .is_some_and(|limit| end <= limit)
            })
    });
    let Ok(offset) = usize::try_from(va) else {
        return 1;
    };
    if !writable || !va.is_multiple_of(size.max(1)) {
        return 1;
    }
    // SAFETY: the bytes [va, va + size) lie in one region mapped writable, committed at
    // `host_base + va` (the `AddressSpace::host_base` contract), and `va` is a multiple of
    // `size`; the reservation is page-aligned (`AddressSpace` reserves it with the host's
    // page allocator), so the pointer is aligned for the atomic type. The view keeps the region
    // mapped during the exchange. Other cores reach these bytes with atomics from Rust and
    // with plain accesses from C++ (ADR-0019 B2 scope).
    let exchanged = unsafe {
        let host = memory.host_base().as_ptr().add(offset);
        match size {
            4 => {
                let (Ok(expected), Ok(desired)) = (u32::try_from(expected), u32::try_from(desired))
                else {
                    return 1;
                };
                AtomicU32::from_ptr(host.cast())
                    .compare_exchange(expected, desired, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            }
            8 => AtomicU64::from_ptr(host.cast())
                .compare_exchange(expected, desired, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            _ => return 1,
        }
    };
    *stored = u32::from(exchanged);
    0
}

/// The C monitor of `exclusive`; size 0 means no reservation.
fn raw_exclusive(exclusive: Option<Exclusive>) -> lca_exclusive {
    exclusive.map_or(
        lca_exclusive {
            address: 0,
            value: 0,
            size: 0,
            reserved: 0,
        },
        |e| lca_exclusive {
            address: e.address.get(),
            value: e.value,
            size: u32::from(e.size),
            reserved: 0,
        },
    )
}

/// The reservation `raw` describes, if any.
fn exclusive_from(raw: lca_exclusive) -> Option<Exclusive> {
    let size = u8::try_from(raw.size).ok().filter(|size| *size != 0)?;
    Some(Exclusive {
        address: GuestVa::new(raw.address),
        value: raw.value,
        size,
    })
}

/// Runs `state` over `memory` through `lca_run`; `state` is updated in place.
pub(crate) fn run(
    state: &mut State,
    memory: &AddressSpace,
    limit: u64,
    interrupt: Option<&AtomicBool>,
) -> Result<lca_exit, CpuError> {
    // Held until `lca_run` returns: no region is mapped, unmapped or reprotected meanwhile.
    let memory = memory.view();
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
    let host_base = memory.host_base().as_ptr();
    let host_size = memory.size();
    // From here until `lca_run` returns, this thread reaches the space only through this
    // pointer (by the slow-path callbacks) and through `host_base`.
    let context = Context { memory, interrupt };
    let context: *const Context<'_, '_> = &raw const context;
    let view = lca_memory {
        host_base,
        host_size,
        regions: regions.as_ptr(),
        // `count` is at most MAX_REGIONS, which fits in u32.
        region_count: u32::try_from(count).unwrap_or(0),
        reserved: 0,
        callbacks: lca_callbacks {
            context: context.cast_mut().cast(),
            read: Some(slow_read),
            write: Some(slow_write),
            compare_exchange: Some(compare_exchange),
            interrupted: Some(interrupted),
        },
    };
    let mut raw = lca_state {
        x: state.x,
        sp: state.sp,
        pc: state.pc,
        nzcv: state.nzcv,
        reserved: 0,
        tpidrro_el0: state.tpidrro_el0,
        exclusive: raw_exclusive(state.exclusive),
    };
    let mut exit = lca_exit {
        reason: 0,
        detail: 0,
        address: 0,
        executed: 0,
    };
    // SAFETY: `raw`, `view`, `regions` and `exit` live until the call returns, and the header
    // only borrows them for the call (ARCH-53). `host_base` is the reservation of `memory`, a
    // view held for the whole call, so no thread changes its mappings meanwhile; other cores
    // may read and write the same guest bytes (ADR-0019 B2 scope). The first `region_count`
    // entries of `regions` list exactly the mapped regions, each committed at
    // `host_base + base` while the view lives (the `SpaceView::host_base` contract), and C++
    // accesses no other address inline: every other data access goes through the callbacks,
    // which take no lock and do not change the mappings. `context` points to the `Context` that
    // holds that view and the borrowed flag; both live until after the call.
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
    state.tpidrro_el0 = raw.tpidrro_el0;
    state.exclusive = exclusive_from(raw.exclusive);
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
