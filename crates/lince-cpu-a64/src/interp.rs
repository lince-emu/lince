// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Tier A interpreter (ADR-0005) through the C++ `lca_run`.

use lince_common::GuestVa;
use lince_mem::AddressSpace;

use crate::decode::Insn;
use crate::error::CpuError;
use crate::ffi::{self, RawExit};

/// The architectural state of one guest core.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// General-purpose registers X0 to X30.
    pub x: [u64; 31],
    /// The stack pointer.
    pub sp: u64,
    /// The program counter.
    pub pc: u64,
    /// The N, Z, C and V flags in bits 31 to 28.
    pub nzcv: u32,
}

/// Why [`run`] returned. Except for `Svc` and `Limit`, the instruction at `pc` did not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// An SVC completed; `pc` is already past it.
    Svc {
        /// The SVC number (imm16).
        number: u16,
    },
    /// The instruction limit was reached.
    Limit,
    /// The word at `pc` is unallocated or reserved.
    Undefined {
        /// Its address.
        pc: GuestVa,
    },
    /// The word at `pc` decodes, but the interpreter has no semantics for it yet.
    Unimplemented {
        /// Its address.
        pc: GuestVa,
        /// The decoded form.
        insn: Insn,
    },
    /// `pc` is not in a region with execute permission.
    FetchAbort {
        /// The address fetched.
        pc: GuestVa,
    },
    /// `pc` is not a multiple of 4 (the PC alignment fault of the Arm specification).
    PcMisaligned {
        /// The misaligned address.
        pc: GuestVa,
    },
}

/// The result of one [`run`] call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    /// Why it returned.
    pub exit: Exit,
    /// Instructions completed, the unit of ADR-0004's deterministic clock.
    pub executed: u64,
}

/// Runs the core in `state` over `memory` until an exit or until `limit` instructions
/// completed. Guest memory is accessed inline through `memory`'s host base (ADR-0031).
pub fn run(state: &mut State, memory: &mut AddressSpace, limit: u64) -> Result<Run, CpuError> {
    let raw = ffi::run(state, memory, limit)?;
    Ok(Run {
        exit: exit_from(raw)?,
        executed: raw.executed,
    })
}

fn exit_from(raw: RawExit) -> Result<Exit, CpuError> {
    let unknown = CpuError::UnknownExit {
        reason: raw.reason,
        detail: raw.detail,
    };
    let pc = GuestVa::new(raw.address);
    Ok(match raw.reason {
        ffi::EXIT_SVC => Exit::Svc {
            number: u16::try_from(raw.detail).map_err(|_| unknown)?,
        },
        ffi::EXIT_LIMIT => Exit::Limit,
        ffi::EXIT_UNDEFINED => Exit::Undefined { pc },
        ffi::EXIT_UNIMPLEMENTED => Exit::Unimplemented {
            pc,
            insn: u16::try_from(raw.detail)
                .ok()
                .and_then(Insn::from_id)
                .ok_or(unknown)?,
        },
        ffi::EXIT_FETCH_ABORT => Exit::FetchAbort { pc },
        ffi::EXIT_PC_MISALIGNED => Exit::PcMisaligned { pc },
        _ => return Err(unknown),
    })
}
