// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Guest threads and the simple per-core scheduler (ADR-0017 and ADR-0024 B2 scope).
//!
//! Every thread belongs to one core, its ideal core, and runs only there: there is no
//! migration. A core runs the ready thread with the lowest priority value (0 is the highest
//! priority, research V4); threads of equal priority take turns in the order they became
//! ready.

use std::time::Instant;

use lince_common::GuestVa;
use lince_cpu_a64::State;
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission};

/// Guest cores (ADR-0004: six application cores, never folded).
pub const CORE_COUNT: usize = 6;

/// The lowest priority a thread may have; lower values run first (research V4).
pub const LOWEST_PRIORITY: u32 = 0x3F;

/// Base of the region that holds every thread's TLS block.
// GUESS: B2 programs only read TPIDRRO_EL0 and the IPC buffer at its start; any free address
// below the heap works.
pub const TLS_BASE: GuestVa = GuestVa::new(0x1F_0000_0000);

/// Bytes of one thread's TLS block; the IPC message buffer is its first 0x100 bytes.
// GUESS: Switch 1 programs use 0x200-byte blocks; the size is not in docs/research.
pub const TLS_SIZE: u64 = 0x200;

/// Most threads a process may create over its life, so a guest cannot grow host memory
/// without bound.
// GUESS: Horizon's limit comes from the process's resource limit, which B2 does not model.
// Known B2 limitation: exited threads keep their entry and TLS block (their handles stay
// valid until T-052's CloseHandle), so the limit counts creations, not live threads.
const MAX_THREADS: usize = 256;

/// The handle value of the first thread; later threads count up from it.
// GUESS: handle values are opaque to the guest. T-052 replaces this with the handle table.
const FIRST_HANDLE: u32 = 0x1_0000;

/// The pseudo-handle naming the calling thread.
// GUESS: the Switch 1 value; not in docs/research.
pub const CURRENT_THREAD: u32 = 0xFFFF_8000;

/// What creating a thread needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadParams {
    /// The first instruction.
    pub entry: GuestVa,
    /// The value in `X0` on entry.
    pub arg: u64,
    /// The initial stack pointer.
    pub stack_top: u64,
    /// The priority, 0 to [`LOWEST_PRIORITY`].
    pub priority: u32,
    /// The ideal core, below [`CORE_COUNT`].
    pub core: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    /// Created, not started.
    Created,
    /// Waiting for its core; `seq` orders threads of equal priority.
    Ready { seq: u64 },
    /// On its core; the core holds its state.
    Running,
    /// Ready again at `until`.
    Sleeping { until: Instant },
    /// Exited; it never runs again.
    Exited,
}

pub(crate) struct Thread {
    pub(crate) status: Status,
    pub(crate) state: State,
    pub(crate) priority: u32,
    pub(crate) core: usize,
    pub(crate) mask: u64,
}

/// Every thread of the process, indexed by handle.
#[derive(Default)]
pub(crate) struct Threads {
    list: Vec<Thread>,
    next_seq: u64,
    /// Set when a thread became ready or changed status, core or priority, so parked cores
    /// must look again.
    changed: bool,
}

impl Threads {
    pub(crate) fn index(&self, handle: u32) -> Option<usize> {
        let index = usize::try_from(handle.checked_sub(FIRST_HANDLE)?).ok()?;
        (index < self.list.len()).then_some(index)
    }

    pub(crate) fn thread(&self, index: usize) -> &Thread {
        &self.list[index]
    }

    pub(crate) fn thread_mut(&mut self, index: usize) -> &mut Thread {
        self.changed = true;
        &mut self.list[index]
    }

    pub(crate) fn status(&self, index: usize) -> Status {
        self.list[index].status
    }

    pub(crate) fn set_status(&mut self, index: usize, status: Status) {
        self.changed = true;
        self.list[index].status = status;
    }

    /// Whether a parked core may have work since the last call.
    pub(crate) fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Creates a thread in the `Created` state, with its TLS block mapped and zeroed, and
    /// returns its handle; `None` past the thread limit or if the TLS block cannot be mapped.
    pub(crate) fn create(&mut self, mem: &AddressSpace, params: ThreadParams) -> Option<u32> {
        if self.list.len() >= MAX_THREADS {
            return None;
        }
        let index = u64::try_from(self.list.len()).ok()?;
        let tls = TLS_BASE.wrapping_add(index.checked_mul(TLS_SIZE)?);
        if tls.is_aligned_to(GUEST_PAGE_SIZE) {
            mem.map(tls, GUEST_PAGE_SIZE, Permission::READ | Permission::WRITE)
                .ok()?;
        }
        let handle = FIRST_HANDLE.checked_add(u32::try_from(index).ok()?)?;
        let mut state = State {
            pc: params.entry.get(),
            sp: params.stack_top,
            tpidrro_el0: tls.get(),
            ..State::default()
        };
        state.x[0] = params.arg;
        self.list.push(Thread {
            status: Status::Created,
            state,
            priority: params.priority,
            core: params.core,
            mask: 1 << params.core,
        });
        Some(handle)
    }

    /// Puts thread `index` at the back of its core's ready queue.
    pub(crate) fn make_ready(&mut self, index: usize) {
        self.list[index].status = Status::Ready { seq: self.next_seq };
        self.next_seq += 1;
        self.changed = true;
    }

    /// Wakes the sleepers of `core` whose time has come, then takes the next thread for
    /// `core` out of the ready queue, marks it running and returns its index and state.
    pub(crate) fn pick(&mut self, core: usize, now: Instant) -> Option<(usize, State)> {
        for index in 0..self.list.len() {
            let thread = &self.list[index];
            if thread.core == core
                && matches!(thread.status, Status::Sleeping { until } if until <= now)
            {
                self.make_ready(index);
            }
        }
        let index = self
            .list
            .iter()
            .enumerate()
            .filter(|(_, thread)| thread.core == core)
            .filter_map(|(index, thread)| match thread.status {
                Status::Ready { seq } => Some((thread.priority, seq, index)),
                Status::Created | Status::Running | Status::Sleeping { .. } | Status::Exited => {
                    None
                }
            })
            .min()?
            .2;
        let thread = &mut self.list[index];
        thread.status = Status::Running;
        Some((index, std::mem::take(&mut thread.state)))
    }

    /// The earliest time a sleeper of `core` wakes.
    pub(crate) fn next_wake(&self, core: usize) -> Option<Instant> {
        self.list
            .iter()
            .filter(|thread| thread.core == core)
            .filter_map(|thread| match thread.status {
                Status::Sleeping { until } => Some(until),
                Status::Created | Status::Ready { .. } | Status::Running | Status::Exited => None,
            })
            .min()
    }

    /// Gives back the state of running thread `index` after it left its core. A thread whose
    /// SVC did not change its status goes to the back of the ready queue.
    pub(crate) fn put_back(&mut self, index: usize, state: State) {
        self.list[index].state = state;
        if self.list[index].status == Status::Running {
            self.make_ready(index);
        }
    }

    /// Whether every started thread has exited.
    pub(crate) fn all_exited(&self) -> bool {
        self.list
            .iter()
            .all(|thread| matches!(thread.status, Status::Created | Status::Exited))
    }
}
