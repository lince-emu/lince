// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The thread SVCs (docs/research/b2-svcs-and-ipc.md V1–V6, Switch 1 knowledge; switchbrew.org
//! SVC page re-read 2026-11-14 for the registers and the special values).
//!
//! The research note lists no result code for a bad handle, priority, core or mask (O4); like
//! `SetMemoryAttribute`, those calls return the distinct `Unimplemented` error instead of an
//! invented code. They are unsupported too without a current thread (the B1 path).

use std::time::{Duration, Instant};

use lince_common::GuestVa;
use lince_mem::AddressSpace;

use crate::kernel::{Kernel, Outcome, Regs};
use crate::svc::{OUT_OF_RESOURCE, SUCCESS, done};
use crate::thread::{CORE_COUNT, CURRENT_THREAD, LOWEST_PRIORITY, Status, ThreadParams};

/// The `CreateThread` core id that means the process's default core.
const DEFAULT_CORE_ID: i32 = -2;
/// The process's default core.
// GUESS: the core of the main thread, which the frontend puts on core 0.
const DEFAULT_CORE: usize = 0;
/// Affinity bits of the cores that exist.
const ALL_CORES: u64 = (1 << CORE_COUNT) - 1;

/// The low 32 bits of a register: its `W` view.
fn w(reg: u64) -> u32 {
    u32::try_from(reg & 0xFFFF_FFFF).unwrap_or(u32::MAX)
}

/// The thread index `handle` names, or `None`.
fn resolve(kernel: &Kernel, handle: u32) -> Option<usize> {
    let current = kernel.current?;
    if handle == CURRENT_THREAD {
        Some(current.index)
    } else {
        kernel.threads.index(handle)
    }
}

fn valid_core(core: i32) -> Option<usize> {
    usize::try_from(core).ok().filter(|&core| core < CORE_COUNT)
}

/// `X1` entry, `X2` argument, `X3` stack top, `W4` priority, `W5` core id; out `W0`, `W1`
/// handle. The thread starts only with `StartThread`.
pub(crate) fn create_thread(kernel: &mut Kernel, mem: &AddressSpace, regs: &mut Regs) -> Outcome {
    let priority = w(regs[4]);
    let core_id = w(regs[5]).cast_signed();
    let core = if core_id == DEFAULT_CORE_ID {
        Some(DEFAULT_CORE)
    } else {
        valid_core(core_id)
    };
    let (Some(_), Some(core)) = (kernel.current, core) else {
        return Outcome::Unsupported;
    };
    if priority > LOWEST_PRIORITY {
        return Outcome::Unsupported;
    }
    let params = ThreadParams {
        entry: GuestVa::new(regs[1]),
        arg: regs[2],
        stack_top: regs[3],
        priority,
        core,
    };
    // GUESS: running out of TLS space is a resource limit; the code is Switch 1's generic one.
    let Some(handle) = kernel.threads.create(mem, params) else {
        return done(regs, OUT_OF_RESOURCE);
    };
    regs[1] = u64::from(handle);
    done(regs, SUCCESS)
}

/// `W0` handle; out `W0`. Only a created, not yet started thread can start.
// GUESS: the page lists no result register; returning success in W0 is harmless either way.
pub(crate) fn start_thread(kernel: &mut Kernel, _: &AddressSpace, regs: &mut Regs) -> Outcome {
    let Some(index) = resolve(kernel, w(regs[0])) else {
        return Outcome::Unsupported;
    };
    if kernel.threads.status(index) != Status::Created {
        return Outcome::Unsupported;
    }
    kernel.threads.make_ready(index);
    done(regs, SUCCESS)
}

/// No arguments, no return: the calling thread ends.
pub(crate) fn exit_thread(kernel: &mut Kernel, _: &AddressSpace, _: &mut Regs) -> Outcome {
    let Some(current) = kernel.current else {
        return Outcome::Unsupported;
    };
    kernel.threads.set_status(current.index, Status::Exited);
    Outcome::Reschedule
}

/// `X0` nanoseconds, no result. 0, -1 and -2 are the three kinds of yield; with no migration
/// they all send the thread to the back of its priority's queue (ADR-0032 §5 documents the
/// deviation for -1 and -2).
// GUESS: other negative values are treated as a yield too.
pub(crate) fn sleep_thread(kernel: &mut Kernel, _: &AddressSpace, regs: &mut Regs) -> Outcome {
    let Some(current) = kernel.current else {
        return Outcome::Unsupported;
    };
    if let Ok(ns @ 1..) = u64::try_from(regs[0].cast_signed()) {
        // UNKNOWN: a sleep past the host clock's range; it is not slept as a shorter one.
        let Some(until) = Instant::now().checked_add(Duration::from_nanos(ns)) else {
            return Outcome::Unsupported;
        };
        kernel
            .threads
            .set_status(current.index, Status::Sleeping { until });
    } else {
        // The core still holds the state; it puts it back under this same lock hold.
        kernel.threads.make_ready(current.index);
    }
    Outcome::Reschedule
}

/// `W1` handle; out `W0`, `W1` priority.
pub(crate) fn get_thread_priority(
    kernel: &mut Kernel,
    _: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    let Some(index) = resolve(kernel, w(regs[1])) else {
        return Outcome::Unsupported;
    };
    regs[1] = u64::from(kernel.threads.thread(index).priority);
    done(regs, SUCCESS)
}

/// `W0` handle, `W1` priority (0 to 0x3F); out `W0`. The kernel reselects after the SVC, so
/// the new priority applies at the affected core's next safe point.
pub(crate) fn set_thread_priority(
    kernel: &mut Kernel,
    _: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    let (Some(index), priority) = (resolve(kernel, w(regs[0])), w(regs[1])) else {
        return Outcome::Unsupported;
    };
    if priority > LOWEST_PRIORITY {
        return Outcome::Unsupported;
    }
    kernel.threads.thread_mut(index).priority = priority;
    done(regs, SUCCESS)
}

/// `W2` handle; out `W0`, `W1` ideal core, `X2` affinity mask.
// GUESS: the page names the outputs CoreMask0 and CoreMask1 only; reading them as the ideal
// core and the mask matches what `SetThreadCoreMask` takes.
pub(crate) fn get_thread_core_mask(
    kernel: &mut Kernel,
    _: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    let Some(index) = resolve(kernel, w(regs[2])) else {
        return Outcome::Unsupported;
    };
    let thread = kernel.threads.thread(index);
    regs[1] = thread.core as u64;
    regs[2] = thread.mask;
    done(regs, SUCCESS)
}

/// `W0` handle, `W1` ideal core, `X2` affinity mask; out `W0`. A running thread leaves its
/// old core at that core's next safe point and runs on the new one only after (ADR-0032 §4);
/// a thread that moves itself leaves at once.
// UNKNOWN: the special core ids (research O3); only an explicit core inside the mask is
// supported.
pub(crate) fn set_thread_core_mask(
    kernel: &mut Kernel,
    _: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    let (Some(index), Some(core), mask) = (
        resolve(kernel, w(regs[0])),
        valid_core(w(regs[1]).cast_signed()),
        regs[2],
    ) else {
        return Outcome::Unsupported;
    };
    if mask & !ALL_CORES != 0 || mask & (1 << core) == 0 {
        return Outcome::Unsupported;
    }
    let thread = kernel.threads.thread_mut(index);
    let moved = thread.core != core;
    thread.core = core;
    thread.mask = mask;
    regs[0] = u64::from(SUCCESS);
    let is_self = kernel.current.is_some_and(|current| current.index == index);
    if moved && is_self {
        Outcome::Reschedule
    } else {
        Outcome::Resume
    }
}

/// No arguments; out `W0` the core the calling thread runs on.
pub(crate) fn get_current_processor_number(
    kernel: &mut Kernel,
    _: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    let Some(current) = kernel.current else {
        return Outcome::Unsupported;
    };
    regs[0] = current.core as u64;
    Outcome::Resume
}
