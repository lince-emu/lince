// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The kernel state and the SVC dispatch table.

use std::time::Duration;

use lince_common::GuestVa;
use lince_mem::AddressSpace;
use lince_trace::{EventKind, NameId, Recorder, ThreadRole, Trace};

use crate::deferred::Deferred;
use crate::error::{KernelError, Result};
use crate::handle::{Handles, Object};
use crate::ipc_svc::{PortFactory, SessionObject};
use crate::svc::{self, NOT_IMPLEMENTED, SvcNumber};
use crate::thread::{CORE_COUNT, LOWEST_PRIORITY, ThreadParams, Threads};

/// Guest registers `X0`–`X7`: SVC arguments on entry, results on return.
pub type Regs = [u64; 8];

/// Base of the guest heap that `SetHeapSize` maps. The kernel picks the base on Horizon; B1
/// fixes it.
// GUESS: B1 programs take the base from `X1` and need no particular address; the caller must
// load the program elsewhere (docs/research/nro-and-b1-svcs.md O6).
pub const HEAP_BASE: GuestVa = GuestVa::new(0x20_0000_0000);

/// How often [`run_cores`](crate::run_cores) rotates the sliced priorities, by default.
// GUESS: the Switch 1 quantum (ADR-0032 §3, Ryujinx note); not measured on NX2.
pub const DEFAULT_QUANTUM: Duration = Duration::from_millis(10);

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
    /// A session object received a command it has no behavior for.
    UnknownCommand { object: &'static str, command: u32 },
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
    pub(crate) handles: Handles,
    /// Whether each event is signaled, by event index; events are never freed (B2).
    pub(crate) events: Vec<bool>,
    /// Session objects by session index; `None` once the session's handle is closed.
    pub(crate) sessions: Vec<Option<Box<dyn SessionObject>>>,
    /// The session whose object is handling a request; its slot is empty while the object runs
    /// but stays reserved, so a session the object opens never lands in it.
    pub(crate) running_session: Option<usize>,
    /// Handles of the sessions the running object opened during its request; closed again if
    /// the request fails, since the guest never learns them. Kept between requests so its
    /// capacity is reused.
    pub(crate) opened_during_request: Vec<u32>,
    /// Named ports a program connects to with `ConnectToNamedPort`.
    pub(crate) named_ports: Vec<(&'static str, PortFactory)>,
    /// Set by a core thread while it dispatches; `None` on the single-thread B1 path, where
    /// the thread SVCs are unsupported.
    pub(crate) current: Option<Current>,
    /// How often [`run_cores`](crate::run_cores) rotates the sliced priorities.
    pub(crate) quantum: Duration,
    /// One recorder per guest core thread, for the scheduler's ledger events (ADR-0027 B2
    /// scope); [`run_cores`](crate::run_cores) lends core `n` the `n`th.
    pub(crate) core_ledgers: Vec<Recorder>,
    /// Coverage mode (T-055): see [`Kernel::set_keep_going`].
    keep_going: bool,
    /// Deferred replies and the service-I/O thread that builds them (T-063).
    pub(crate) deferred: Deferred,
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
            handles: Handles::default(),
            events: Vec::new(),
            sessions: Vec::new(),
            running_session: None,
            opened_during_request: Vec::new(),
            named_ports: Vec::new(),
            current: None,
            quantum: DEFAULT_QUANTUM,
            core_ledgers: (0..CORE_COUNT)
                .map(|_| trace.recorder(ThreadRole::GuestCore))
                .collect(),
            keep_going: false,
            deferred: Deferred::default(),
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
            Outcome::Resume
            | Outcome::Reschedule
            | Outcome::Unsupported
            | Outcome::UnknownCommand { .. } => Flow::Continue,
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
            if self.keep_going {
                regs[0] = u64::from(NOT_IMPLEMENTED);
                return Ok(Outcome::Resume);
            }
            return Err(KernelError::UnknownSvc {
                number: u32::from(number),
            });
        };
        match (entry.handler)(self, mem, regs) {
            Outcome::Unsupported => Err(KernelError::Unimplemented {
                number: u32::from(number),
                name: entry.name,
            }),
            Outcome::UnknownCommand { .. } if self.keep_going => {
                regs[0] = u64::from(NOT_IMPLEMENTED);
                Ok(Outcome::Resume)
            }
            Outcome::UnknownCommand { object, command } => {
                Err(KernelError::UnknownCommand { object, command })
            }
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
        if !self.handles.has_room() {
            return Err(bad);
        }
        let index = self.threads.create(mem, params).ok_or(bad)?;
        let handle = self.handles.open(Object::Thread(index)).ok_or(bad)?;
        self.threads.make_ready(index);
        Ok(handle)
    }

    /// Replaces the priority each core time slices (default
    /// [`DEFAULT_SLICED_PRIORITIES`](crate::DEFAULT_SLICED_PRIORITIES)); `None` slices nothing
    /// on that core. Call before [`run_cores`](crate::run_cores).
    pub fn set_sliced_priorities(&mut self, sliced: [Option<u32>; CORE_COUNT]) {
        self.threads.set_sliced(sliced);
    }

    /// Replaces how often [`run_cores`](crate::run_cores) ends a quantum (default
    /// [`DEFAULT_QUANTUM`]). Call before it.
    pub fn set_quantum(&mut self, quantum: Duration) {
        self.quantum = quantum;
    }

    /// Coverage mode (T-055): when on, an unknown SVC or IPC command is still recorded once,
    /// but instead of stopping the guest it returns [`NOT_IMPLEMENTED`] in `W0` and the guest
    /// continues, so one run reaches every unimplemented call. Off by default. Call before the
    /// guest runs.
    pub fn set_keep_going(&mut self, on: bool) {
        self.keep_going = on;
    }

    /// Makes `name` a port programs can connect to; each connection gets a new object from
    /// `factory`. Call before [`run_cores`](crate::run_cores).
    pub fn register_named_port(&mut self, name: &'static str, factory: PortFactory) {
        self.named_ports.push((name, factory));
    }

    /// Takes the bytes the guest wrote with `OutputDebugString` since the last call.
    pub fn take_debug_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.debug_output)
    }

    pub(crate) fn log_stub(&mut self, name: NameId) {
        self.recorder.stub(name);
    }

    pub(crate) fn log_unknown_command(&mut self, name: NameId, command: u32) {
        self.recorder.unknown_command(name, command);
    }
}
