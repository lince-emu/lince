// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The SVC table and the handlers of the B1 SVCs.
//!
//! Numbers, registers and result codes: docs/research/nro-and-b1-svcs.md S2–S11 (Switch 1
//! knowledge). Where the note has no value, the code says GUESS.

use lince_common::GuestVa;
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission};

use crate::kernel::{Entry, HEAP_BASE, Handler, Kernel, Outcome, Regs};

/// An SVC number: the immediate of the `SVC` instruction.
pub type SvcNumber = u8;

/// A Horizon result code, returned in `W0`.
pub type ResultCode = u32;

const SUCCESS: ResultCode = 0;
const INVALID_SIZE: ResultCode = 0xCA01;
const INVALID_ADDRESS: ResultCode = 0xCC01;
const OUT_OF_RESOURCE: ResultCode = 0xD001;
const INVALID_STATE: ResultCode = 0xD401;
const INVALID_PERMISSION: ResultCode = 0xD801;

const SET_HEAP_SIZE: SvcNumber = 0x01;
const SET_MEMORY_PERMISSION: SvcNumber = 0x02;
const SET_MEMORY_ATTRIBUTE: SvcNumber = 0x03;
const QUERY_MEMORY: SvcNumber = 0x06;
const EXIT_PROCESS: SvcNumber = 0x07;
const BREAK: SvcNumber = 0x26;
const OUTPUT_DEBUG_STRING: SvcNumber = 0x27;
const SET_GPU_MEMORY_ATTRIBUTE: SvcNumber = 0x80;

pub(crate) const BREAK_NAME: &str = "Break";
pub(crate) const GPU_ATTRIBUTE_NAME: &str = "SetGpuMemoryAttribute";
pub(crate) const ATTRIBUTE_NAME: &str = "SetMemoryAttribute";

/// Heap sizes are multiples of 2 MiB and at most 4 GiB (S5).
const HEAP_ALIGN: u64 = 0x20_0000;
const HEAP_MAX: u64 = 0x1_0000_0000;
/// Longest `OutputDebugString` accepted per call.
// GUESS: the note gives no limit; 64 KiB exceeds any B1 message.
const DEBUG_STRING_MAX: u64 = 0x1_0000;
/// Most undrained debug-output bytes kept; past it `OutputDebugString` returns
/// `OUT_OF_RESOURCE`, so a guest cannot grow host memory without bound.
const DEBUG_PENDING_MAX: usize = 0x10_0000;
/// `Break` reason bit that makes the call return (S10).
const BREAK_RETURN_BIT: u64 = 1 << 31;
/// The only memory attribute `SetMemoryAttribute` may change in B1: Uncached (S4, S7).
const ATTRIBUTE_UNCACHED: u64 = 1 << 3;

const MEMORY_INFO_SIZE: usize = 0x28;
const TYPE_FREE: u32 = 0;
const TYPE_CODE: u32 = 0xDC_7E03;
const TYPE_NORMAL: u32 = 0x37E_BD05;
const TYPE_INACCESSIBLE: u32 = 0x10;

pub(crate) static TABLE: [Option<Entry>; 256] = build_table();

const fn build_table() -> [Option<Entry>; 256] {
    let mut table = [None; 256];
    table[SET_HEAP_SIZE as usize] = Some(entry("SetHeapSize", set_heap_size));
    table[SET_MEMORY_PERMISSION as usize] =
        Some(entry("SetMemoryPermission", set_memory_permission));
    table[SET_MEMORY_ATTRIBUTE as usize] = Some(entry(ATTRIBUTE_NAME, set_memory_attribute));
    table[QUERY_MEMORY as usize] = Some(entry("QueryMemory", query_memory));
    table[EXIT_PROCESS as usize] = Some(entry("ExitProcess", exit_process));
    table[BREAK as usize] = Some(entry(BREAK_NAME, break_));
    table[OUTPUT_DEBUG_STRING as usize] = Some(entry("OutputDebugString", output_debug_string));
    // 0x80..=0x8F are reserved for Switch 2 (ADR-0024); only 0x80 has a name so far.
    table[SET_GPU_MEMORY_ATTRIBUTE as usize] =
        Some(entry(GPU_ATTRIBUTE_NAME, set_gpu_memory_attribute));
    table
}

const fn entry(name: &'static str, handler: Handler) -> Entry {
    Entry { name, handler }
}

/// The name of SVC `number`, or `None` if the table has no entry for it.
pub fn svc_name(number: SvcNumber) -> Option<&'static str> {
    TABLE[usize::from(number)].map(|entry| entry.name)
}

fn done(regs: &mut Regs, code: ResultCode) -> Outcome {
    regs[0] = u64::from(code);
    Outcome::Resume
}

/// `X1` = size; out `W0`, `X1` = heap base. The heap is kept as the chunks that grew it, so it
/// can shrink only to a size where a chunk ends.
// GUESS: B1 programs shrink the heap to 0 or not at all; other sizes return `INVALID_STATE`.
fn set_heap_size(kernel: &mut Kernel, mem: &mut AddressSpace, regs: &mut Regs) -> Outcome {
    let size = regs[1];
    if !size.is_multiple_of(HEAP_ALIGN) || size > HEAP_MAX {
        return done(regs, INVALID_SIZE);
    }
    let current = kernel.heap_chunks.last().copied().unwrap_or(0);
    if size > current {
        let grown = mem.map(
            HEAP_BASE.wrapping_add(current),
            size - current,
            Permission::READ | Permission::WRITE,
        );
        if grown.is_err() {
            return done(regs, OUT_OF_RESOURCE);
        }
        kernel.heap_chunks.push(size);
    } else if size < current {
        if size != 0 && !kernel.heap_chunks.contains(&size) {
            return done(regs, INVALID_STATE);
        }
        while let Some(&end) = kernel.heap_chunks.last() {
            if end <= size {
                break;
            }
            let start = kernel.heap_chunks.iter().rev().nth(1).copied().unwrap_or(0);
            if mem
                .unmap(HEAP_BASE.wrapping_add(start), end - start)
                .is_err()
            {
                return done(regs, INVALID_STATE);
            }
            kernel.heap_chunks.pop();
        }
    }
    regs[1] = HEAP_BASE.get();
    done(regs, SUCCESS)
}

fn aligned_range(regs: &Regs) -> std::result::Result<(GuestVa, u64), ResultCode> {
    let (va, size) = (GuestVa::new(regs[0]), regs[1]);
    if !va.is_aligned_to(GUEST_PAGE_SIZE) {
        Err(INVALID_ADDRESS)
    } else if size == 0 || !size.is_multiple_of(GUEST_PAGE_SIZE) {
        Err(INVALID_SIZE)
    } else {
        Ok((va, size))
    }
}

/// `X0` address, `X1` size, `W2` permission: none, read or read-write only (S6). The range may
/// be part of a region or span several; every page in it must be mapped.
fn set_memory_permission(_: &mut Kernel, mem: &mut AddressSpace, regs: &mut Regs) -> Outcome {
    let (va, size) = match aligned_range(regs) {
        Ok(range) => range,
        Err(code) => return done(regs, code),
    };
    let perm = match regs[2] & 0xFFFF_FFFF {
        0 => Permission::NONE,
        1 => Permission::READ,
        3 => Permission::READ | Permission::WRITE,
        _ => return done(regs, INVALID_PERMISSION),
    };
    let code = if mem.protect(va, size, perm).is_ok() {
        SUCCESS
    } else {
        INVALID_STATE
    };
    done(regs, code)
}

/// `X0` address, `X1` size, `W2` mask, `W3` value. A no-op on mapped memory when only the
/// Uncached bit is named (S7). Other bits (`PermissionLocked` on newer firmware, or bits S7 does
/// not allow) return the distinct `Unimplemented` error, logged once by name: their result code
/// is undocumented (research O3), so none is invented.
// GUESS: accepting Uncached as a no-op is enough for B1, which has no device memory.
fn set_memory_attribute(kernel: &mut Kernel, mem: &mut AddressSpace, regs: &mut Regs) -> Outcome {
    let (va, size) = match aligned_range(regs) {
        Ok(range) => range,
        Err(code) => return done(regs, code),
    };
    let (mask, value) = (regs[2] & 0xFFFF_FFFF, regs[3] & 0xFFFF_FFFF);
    if (mask | value) & !ATTRIBUTE_UNCACHED != 0 {
        kernel.log_stub(kernel.attribute_stub);
        return Outcome::Unsupported;
    }
    let mapped = mem.query(va).is_some_and(|span| {
        span.permission.is_some()
            && span.base.get().saturating_add(span.size) >= va.get().saturating_add(size)
    });
    done(regs, if mapped { SUCCESS } else { INVALID_STATE })
}

/// `X0` = pointer to a 0x28-byte `MemoryInfo`, `X2` = address; out `W0`, `W1` page info.
// GUESS: page info is 0 and a bad output pointer returns `INVALID_ADDRESS`; neither is
// documented (research O3).
fn query_memory(_: &mut Kernel, mem: &mut AddressSpace, regs: &mut Regs) -> Outcome {
    let out = GuestVa::new(regs[0]);
    // GUESS: an address past the space is one Inaccessible region from the end of the space to
    // the top of the 64-bit range; S3 names the type, the extent is not documented.
    let (base, size, kind, bits) = match mem.query(GuestVa::new(regs[2])) {
        None => (
            mem.size(),
            0u64.wrapping_sub(mem.size()),
            TYPE_INACCESSIBLE,
            0,
        ),
        Some(span) => {
            let (kind, bits) = span_type(span.permission);
            (span.base.get(), span.size, kind, bits)
        }
    };
    let mut info = [0u8; MEMORY_INFO_SIZE];
    info[0x00..0x08].copy_from_slice(&base.to_le_bytes());
    info[0x08..0x10].copy_from_slice(&size.to_le_bytes());
    info[0x10..0x14].copy_from_slice(&kind.to_le_bytes());
    info[0x18..0x1C].copy_from_slice(&bits.to_le_bytes());
    let code = if mem.write_bytes(out, &info).is_ok() {
        SUCCESS
    } else {
        INVALID_ADDRESS
    };
    regs[1] = 0;
    done(regs, code)
}

/// The `MemoryInfo` type and permission bits of a span.
// GUESS: executable regions are Code and other mapped regions Normal; the full type table is
// unchecked (research O4).
fn span_type(permission: Option<Permission>) -> (u32, u32) {
    match permission {
        None => (TYPE_FREE, 0),
        Some(perm) => {
            let bits = u32::from(perm.contains(Permission::READ))
                | u32::from(perm.contains(Permission::WRITE)) << 1
                | u32::from(perm.contains(Permission::EXECUTE)) << 2;
            let kind = if bits & 4 != 0 {
                TYPE_CODE
            } else {
                TYPE_NORMAL
            };
            (kind, bits)
        }
    }
}

fn exit_process(_: &mut Kernel, _: &mut AddressSpace, _: &mut Regs) -> Outcome {
    Outcome::Exit
}

/// `X0` reason, `X1`, `X2` info (S10). A named stub, logged once. With reason bit 31 set the
/// call returns; otherwise the guest aborted and stops.
// GUESS: without bit 31 Horizon ends the process or enters a debugger (S10 says only that bit 31
// makes the call return); stopping the guest keeps an abort visible.
fn break_(kernel: &mut Kernel, _: &mut AddressSpace, regs: &mut Regs) -> Outcome {
    kernel.log_stub(kernel.break_stub);
    let reason = regs[0];
    if reason & BREAK_RETURN_BIT == 0 {
        return Outcome::Break(reason);
    }
    done(regs, SUCCESS)
}

/// `X0` = pointer, `X1` = byte length; out `W0`. The bytes are kept for the host to print; the
/// host drains them with [`Kernel::take_debug_output`].
fn output_debug_string(kernel: &mut Kernel, mem: &mut AddressSpace, regs: &mut Regs) -> Outcome {
    let (ptr, len) = (GuestVa::new(regs[0]), regs[1]);
    if len > DEBUG_STRING_MAX {
        return done(regs, INVALID_SIZE);
    }
    let Ok(len) = usize::try_from(len) else {
        return done(regs, INVALID_SIZE);
    };
    let start = kernel.debug_output.len();
    if start + len > DEBUG_PENDING_MAX {
        return done(regs, OUT_OF_RESOURCE);
    }
    kernel.debug_output.resize(start + len, 0);
    let (_, tail) = kernel.debug_output.split_at_mut(start);
    if mem.read_bytes(ptr, tail).is_err() {
        kernel.debug_output.truncate(start);
        return done(regs, INVALID_ADDRESS);
    }
    done(regs, SUCCESS)
}

/// Reserved for Switch 2 (research S11): named, but its behavior is UNKNOWN.
fn set_gpu_memory_attribute(kernel: &mut Kernel, _: &mut AddressSpace, _: &mut Regs) -> Outcome {
    kernel.log_stub(kernel.gpu_attribute_stub);
    Outcome::Unsupported
}
