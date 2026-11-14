// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The kernel state and the SVC dispatch table.

use lince_common::GuestVa;
use lince_mem::AddressSpace;
use lince_trace::{EventKind, NameId, Recorder, ThreadRole, Trace};

use crate::error::{KernelError, Result};
use crate::svc::{self, SvcNumber};

/// Guest registers `X0`–`X7`: SVC arguments on entry, results on return.
pub type Regs = [u64; 8];

/// Base of the guest heap that `SetHeapSize` maps. The kernel picks the base on Horizon; B1
/// fixes it.
// GUESS: B1 programs take the base from `X1` and need no particular address; the caller must
// load the program elsewhere (docs/research/nro-and-b1-svcs.md O6).
pub const HEAP_BASE: GuestVa = GuestVa::new(0x20_0000_0000);

/// What the guest core does after an SVC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// Resume the guest after the `SVC` instruction.
    Continue,
    /// The process exited; stop the guest.
    Exit,
    /// The guest called `Break` without the "return" bit: it aborted; stop the guest.
    Break {
        /// The reason the guest passed in `X0`.
        reason: u64,
    },
}

/// What a handler tells the dispatcher.
pub(crate) enum Outcome {
    /// The SVC ran; resume the guest.
    Resume,
    /// The process exited.
    Exit,
    /// The guest aborted through `Break` with this reason.
    Break(u64),
    /// The SVC is reserved, or was called with arguments this build has no behavior for.
    Unsupported,
}

pub(crate) type Handler = fn(&mut Kernel, &mut AddressSpace, &mut Regs) -> Outcome;

#[derive(Clone, Copy)]
pub(crate) struct Entry {
    pub(crate) name: &'static str,
    pub(crate) handler: Handler,
}

/// The kernel state for one guest process.
pub struct Kernel {
    recorder: Recorder,
    pub(crate) break_stub: NameId,
    pub(crate) gpu_attribute_stub: NameId,
    pub(crate) attribute_stub: NameId,
    pub(crate) heap_chunks: Vec<u64>,
    pub(crate) debug_output: Vec<u8>,
}

impl Kernel {
    /// A kernel whose log records go to `trace`. Call during setup, not on a guest core.
    pub fn new(trace: &mut Trace) -> Result<Self> {
        Ok(Self {
            recorder: trace.recorder(ThreadRole::GuestCore),
            break_stub: trace.name(svc::BREAK_NAME)?,
            gpu_attribute_stub: trace.name(svc::GPU_ATTRIBUTE_NAME)?,
            attribute_stub: trace.name(svc::ATTRIBUTE_NAME)?,
            heap_chunks: Vec::new(),
            debug_output: Vec::new(),
        })
    }

    /// Runs SVC `number` against `mem`, reading arguments from and writing results to `regs`.
    ///
    /// An SVC the table does not know returns [`KernelError::UnknownSvc`]; a reserved one, or one
    /// called with arguments this build cannot handle, returns [`KernelError::Unimplemented`].
    /// Both are recorded once, by number or by name.
    pub fn dispatch(
        &mut self,
        mem: &mut AddressSpace,
        number: SvcNumber,
        regs: &mut Regs,
    ) -> Result<Flow> {
        let Some(entry) = svc::TABLE.get(usize::from(number)).copied().flatten() else {
            self.recorder.once(EventKind::UnknownSvc, u64::from(number));
            return Err(KernelError::UnknownSvc {
                number: u32::from(number),
            });
        };
        match (entry.handler)(self, mem, regs) {
            Outcome::Resume => Ok(Flow::Continue),
            Outcome::Exit => Ok(Flow::Exit),
            Outcome::Break(reason) => Ok(Flow::Break { reason }),
            Outcome::Unsupported => Err(KernelError::Unimplemented {
                number: u32::from(number),
                name: entry.name,
            }),
        }
    }

    /// Takes the bytes the guest wrote with `OutputDebugString` since the last call.
    pub fn take_debug_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.debug_output)
    }

    pub(crate) fn log_stub(&mut self, name: NameId) {
        self.recorder.stub(name);
    }
}
