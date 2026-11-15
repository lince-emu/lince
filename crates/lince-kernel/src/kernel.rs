// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The kernel state and the SVC dispatch table.

use lince_common::GuestVa;
use lince_mem::AddressSpace;
use lince_trace::{EventKind, NameId, Recorder, ThreadRole, Trace};

use crate::error::{KernelError, Result};
use crate::svc::{self, SvcNumber};
use crate::thread::{CORE_COUNT, LOWEST_PRIORITY, ThreadParams, Threads};

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
    /// The SVC changed the calling thread's status or core; its core must pick again.
    Reschedule,
    /// The SVC is reserved, or was called with arguments this build has no behavior for.
    Unsupported,
}

/// The thread that made the SVC being dispatched, and its core.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Current {
    pub(crate) index: usize,
    pub(crate) core: usize,
}

pub(crate) type Handler = fn(&mut Kernel, &AddressSpace, &mut Regs) -> Outcome;

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
    pub(crate) threads: Threads,
    /// Set by a core thread while it dispatches; `None` on the single-thread B1 path, where
    /// the thread SVCs are unsupported.
    pub(crate) current: Option<Current>,
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
            threads: Threads::default(),
            current: None,
        })
    }

    /// Runs SVC `number` against `mem`, reading arguments from and writing results to `regs`.
    ///
    /// An SVC the table does not know returns [`KernelError::UnknownSvc`]; a reserved one, or one
    /// called with arguments this build cannot handle, returns [`KernelError::Unimplemented`].
    /// Both are recorded once, by number or by name.
    pub fn dispatch(
        &mut self,
        mem: &AddressSpace,
        number: SvcNumber,
        regs: &mut Regs,
    ) -> Result<Flow> {
        Ok(match self.run_svc(mem, number, regs)? {
            Outcome::Exit => Flow::Exit,
            Outcome::Break(reason) => Flow::Break { reason },
            // `run_svc` turns `Unsupported` into an error. Only a thread SVC reschedules, and
            // without a current thread those are unsupported.
            Outcome::Resume | Outcome::Reschedule | Outcome::Unsupported => Flow::Continue,
        })
    }

    /// Runs SVC `number` and returns what the handler decided; `Unsupported` comes back as
    /// [`KernelError::Unimplemented`].
    pub(crate) fn run_svc(
        &mut self,
        mem: &AddressSpace,
        number: SvcNumber,
        regs: &mut Regs,
    ) -> Result<Outcome> {
        let Some(entry) = svc::TABLE.get(usize::from(number)).copied().flatten() else {
            self.recorder.once(EventKind::UnknownSvc, u64::from(number));
            return Err(KernelError::UnknownSvc {
                number: u32::from(number),
            });
        };
        match (entry.handler)(self, mem, regs) {
            Outcome::Unsupported => Err(KernelError::Unimplemented {
                number: u32::from(number),
                name: entry.name,
            }),
            outcome @ (Outcome::Resume
            | Outcome::Exit
            | Outcome::Break(_)
            | Outcome::Reschedule) => Ok(outcome),
        }
    }

    /// Creates and starts a thread from the host, such as the main thread, and returns its
    /// handle. Call before [`run_cores`](crate::run_cores).
    pub fn spawn_thread(&mut self, mem: &AddressSpace, params: ThreadParams) -> Result<u32> {
        let bad = KernelError::BadThread {
            priority: params.priority,
            core: params.core,
        };
        if params.priority > LOWEST_PRIORITY || params.core >= CORE_COUNT {
            return Err(bad);
        }
        let handle = self.threads.create(mem, params).ok_or(bad)?;
        let index = self.threads.index(handle).ok_or(bad)?;
        self.threads.make_ready(index);
        Ok(handle)
    }

    /// Takes the bytes the guest wrote with `OutputDebugString` since the last call.
    pub fn take_debug_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.debug_output)
    }

    pub(crate) fn log_stub(&mut self, name: NameId) {
        self.recorder.stub(name);
    }
}
