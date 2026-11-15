// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Guest threads and the simple per-core scheduler (ADR-0017 and ADR-0024 B2 scope).
//!
//! Every thread belongs to one core, its ideal core, and runs only there: there is no
//! migration (ADR-0032 §5). A core runs the runnable thread with the lowest priority value
//! (0 is the highest priority, research V4); among equal priorities, the one at the front of
//! the queue. A thread that is preempted keeps its place; one that becomes ready or yields
//! goes to the back. Only each core's sliced priority rotates, once per quantum (ADR-0032 §1);
//! every other thread runs until it blocks, yields or a better thread becomes runnable.

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
// Known B2 limitation: exited threads keep their entry and TLS block, so the limit counts
// creations, not live threads.
const MAX_THREADS: usize = 256;

/// The priority each core time slices, if any (ADR-0032 §1): 59 on cores 0 to 4 and 63 on
/// core 5.
// GUESS: the Switch 1 rule for its last core extended to six cores; not measured on NX2
// (doc 08 §4).
pub const DEFAULT_SLICED_PRIORITIES: [Option<u32>; CORE_COUNT] =
    [Some(59), Some(59), Some(59), Some(59), Some(59), Some(63)];

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
    /// Waiting for its core.
    Ready,
    /// On `core`, which holds its state.
    Running { core: usize },
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
    /// Its place in the queue of its priority on its core: lower runs first.
    seq: u64,
}

/// Every thread of the process, indexed by handle.
pub(crate) struct Threads {
    list: Vec<Thread>,
    next_seq: u64,
    /// Set when a thread became ready or changed status, core or priority, so the kernel must
    /// reselect.
    changed: bool,
    sliced: [Option<u32>; CORE_COUNT],
}

impl Default for Threads {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            next_seq: 0,
            changed: false,
            sliced: DEFAULT_SLICED_PRIORITIES,
        }
    }
}

impl Threads {
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

    pub(crate) fn set_sliced(&mut self, sliced: [Option<u32>; CORE_COUNT]) {
        self.sliced = sliced;
    }

    /// The reselection pass (ADR-0032 §4): `None` if nothing changed since the last pass,
    /// otherwise, per core, whether it runs a thread other than the one it should run.
    pub(crate) fn switch_requests(&mut self) -> Option<[bool; CORE_COUNT]> {
        std::mem::take(&mut self.changed)
            .then(|| std::array::from_fn(|core| self.needs_switch(core)))
    }

    /// Creates a thread in the `Created` state, with its TLS block mapped and zeroed, and
    /// returns its index; `None` past the thread limit or if the TLS block cannot be mapped.
    pub(crate) fn create(&mut self, mem: &AddressSpace, params: ThreadParams) -> Option<usize> {
        if self.list.len() >= MAX_THREADS {
            return None;
        }
        let index = u64::try_from(self.list.len()).ok()?;
        let tls = TLS_BASE.wrapping_add(index.checked_mul(TLS_SIZE)?);
        if tls.is_aligned_to(GUEST_PAGE_SIZE) {
            mem.map(tls, GUEST_PAGE_SIZE, Permission::READ | Permission::WRITE)
                .ok()?;
        }
        Some(self.insert(tls, params))
    }

    /// Adds a thread in the `Created` state whose TLS block at `tls` is already mapped, and
    /// returns its index.
    pub(crate) fn insert(&mut self, tls: GuestVa, params: ThreadParams) -> usize {
        let index = self.list.len();
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
            seq: 0,
        });
        index
    }

    /// Puts thread `index` at the back of its priority's queue, ready.
    pub(crate) fn make_ready(&mut self, index: usize) {
        let thread = &mut self.list[index];
        thread.status = Status::Ready;
        thread.seq = self.next_seq;
        self.next_seq += 1;
        self.changed = true;
    }

    /// Makes ready every sleeper whose time has come.
    pub(crate) fn wake_due(&mut self, now: Instant) {
        for index in 0..self.list.len() {
            if matches!(self.list[index].status, Status::Sleeping { until } if until <= now) {
                self.make_ready(index);
            }
        }
    }

    /// Sends the front thread of `core`'s sliced priority, running or not, to the back of its
    /// queue (one quantum ended).
    pub(crate) fn rotate(&mut self, core: usize) {
        let Some(priority) = self.sliced[core] else {
            return;
        };
        let front = self
            .runnable(core)
            .filter(|(_, thread)| thread.priority == priority)
            .min_by_key(|(index, thread)| (thread.seq, *index))
            .map(|(index, _)| index);
        if let Some(index) = front {
            self.list[index].seq = self.next_seq;
            self.next_seq += 1;
            self.changed = true;
        }
    }

    /// The ready and running threads whose core is `core`.
    fn runnable(&self, core: usize) -> impl Iterator<Item = (usize, &Thread)> {
        self.list.iter().enumerate().filter(move |(_, thread)| {
            thread.core == core && matches!(thread.status, Status::Ready | Status::Running { .. })
        })
    }

    /// The thread `core` should run: the best runnable thread whose core is `core`. It may
    /// still be running on another core, which gives it back at its next safe point.
    fn select(&self, core: usize) -> Option<usize> {
        self.runnable(core)
            .min_by_key(|(index, thread)| (thread.priority, thread.seq, *index))
            .map(|(index, _)| index)
    }

    /// The thread running on `core`.
    fn running_on(&self, core: usize) -> Option<usize> {
        self.list
            .iter()
            .position(|thread| thread.status == Status::Running { core })
    }

    /// Whether `core` runs a thread other than the one it should run.
    pub(crate) fn needs_switch(&self, core: usize) -> bool {
        self.running_on(core)
            .is_some_and(|running| self.select(core) != Some(running))
    }

    /// Takes the thread `core` should run, if it is ready: marks it running on `core` and
    /// returns its index and state.
    pub(crate) fn take(&mut self, core: usize) -> Option<(usize, State)> {
        let index = self.select(core)?;
        let thread = &mut self.list[index];
        if thread.status != Status::Ready {
            return None;
        }
        thread.status = Status::Running { core };
        Some((index, std::mem::take(&mut thread.state)))
    }

    /// The earliest time a sleeper of `core` wakes.
    pub(crate) fn next_wake(&self, core: usize) -> Option<Instant> {
        self.list
            .iter()
            .filter(|thread| thread.core == core)
            .filter_map(|thread| match thread.status {
                Status::Sleeping { until } => Some(until),
                Status::Created | Status::Ready | Status::Running { .. } | Status::Exited => None,
            })
            .min()
    }

    /// The earliest time any sleeper wakes.
    pub(crate) fn next_wake_any(&self) -> Option<Instant> {
        (0..CORE_COUNT)
            .filter_map(|core| self.next_wake(core))
            .min()
    }

    /// Gives back the state of thread `index` after it left its core. A thread whose SVC did
    /// not change its status is ready again and keeps its place in the queue.
    pub(crate) fn put_back(&mut self, index: usize, state: State) {
        let thread = &mut self.list[index];
        thread.state = state;
        if matches!(thread.status, Status::Running { .. }) {
            thread.status = Status::Ready;
            self.changed = true;
        }
    }

    /// Whether every started thread has exited.
    pub(crate) fn all_exited(&self) -> bool {
        self.list
            .iter()
            .all(|thread| matches!(thread.status, Status::Created | Status::Exited))
    }
}
