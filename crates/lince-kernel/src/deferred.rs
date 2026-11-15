// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Deferred IPC replies (T-063, ADR-0024 B2 scope): a session object that must block or do host
//! I/O hands its work to the service-I/O role thread of `lince-sched` instead of running it on
//! the guest core. The calling guest thread waits ([`WaitKind::Reply`]); when the work is
//! done, its reply lands in the [`Inbox`], the kernel timer thread wakes, and under the kernel
//! lock the reply is copied to the thread's TLS and the thread is made ready again.
//!
//! A request the full queue cannot take waits in the kernel's own pending list and is
//! submitted again when the queue has room: it is never dropped (invariant 1). A reply whose
//! thread no longer waits for it (the process stopped) is not written anywhere; each reply
//! carries a ticket so it can never land in a later wait of the same thread.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use lince_mem::AddressSpace;
use lince_sched::{Request, SchedError, ServiceIo, Shutdown, Signal, SubmitError, Timer};
use parking_lot::Mutex;

use crate::ipc_svc::{INVALID_POINTER, IpcMessage};
use crate::kernel::Kernel;
use crate::svc::SUCCESS;
use crate::thread::WaitKind;

/// Builds the reply of a deferred request on the service-I/O thread: it may block and do host
/// I/O, and returns the whole reply message.
pub type DeferredWork = Box<dyn FnOnce() -> IpcMessage + Send>;

/// A reply built on the service-I/O thread, waiting to be delivered.
struct Completion {
    thread: usize,
    ticket: u64,
    message: IpcMessage,
}

/// Replies the service-I/O thread finished, and the timer that makes the kernel deliver them.
/// Shared between the kernel and the service-I/O thread.
#[derive(Default)]
pub(crate) struct Inbox {
    done: Mutex<Vec<Completion>>,
    /// The timer of the running [`run_cores`](crate::run_cores); `None` between runs, when
    /// the replies wait for the next run.
    waker: Mutex<Option<Arc<Timer>>>,
}

impl Inbox {
    fn push(&self, completion: Completion) {
        self.done.lock().push(completion);
    }

    fn take(&self) -> Vec<Completion> {
        std::mem::take(&mut *self.done.lock())
    }

    pub(crate) fn set_waker(&self, timer: Option<Arc<Timer>>) {
        *self.waker.lock() = timer;
    }
}

/// Fired by the service-I/O thread after each request and when its queue has room again: asks
/// the kernel timer to fire now. Holds two short locks; never waits for the kernel.
impl Signal for Inbox {
    fn signal(&self) {
        if let Some(timer) = self.waker.lock().as_ref() {
            timer.wake_at(Instant::now());
        }
    }
}

/// The kernel's side of deferred replies. Dropping it shuts the service-I/O thread down as
/// [`Kernel::shutdown_service_io`] does, so a kernel dropped without that call still runs
/// every pending request (invariant 1).
#[derive(Default)]
pub(crate) struct Deferred {
    io: Option<ServiceIo>,
    pub(crate) inbox: Arc<Inbox>,
    /// Requests the full queue could not take yet, oldest first.
    pending: VecDeque<Request>,
    next_ticket: u64,
    /// The ticket of the request being handled, once its object deferred it.
    pub(crate) taken: Option<u64>,
}

impl Deferred {
    fn shutdown(&mut self) -> Option<Result<Shutdown, SchedError>> {
        let io = self.io.take()?;
        while let Some(request) = self.pending.pop_front() {
            submit_waiting(&io, request);
        }
        self.inbox.take();
        Some(io.shutdown())
    }
}

impl Drop for Deferred {
    fn drop(&mut self) {
        // A panicked service-I/O thread was already reported by its own panic; there is no
        // caller left to return the error to.
        drop(self.shutdown());
    }
}

impl Kernel {
    /// Starts the service-I/O role thread (ADR-0017) with a queue of `capacity` requests, so
    /// session objects can defer their replies ([`Sessions::defer`](crate::Sessions::defer)).
    /// Call during setup; without it, deferring fails.
    ///
    /// # Errors
    /// The thread could not be created, or `capacity` is 0.
    pub fn start_service_io(&mut self, capacity: usize) -> Result<(), SchedError> {
        let space: Arc<dyn Signal> = Arc::clone(&self.deferred.inbox) as Arc<dyn Signal>;
        self.deferred.io = Some(ServiceIo::start(capacity, space)?);
        Ok(())
    }

    /// Teardown: submits every request still pending, completes them all on the service-I/O
    /// thread and joins it. Their replies are not delivered, since no guest thread runs any
    /// more, but their work (host I/O) is never dropped. Blocks until the queue is empty.
    /// `None` when the thread was never started.
    ///
    /// # Errors
    /// The service-I/O thread panicked.
    pub fn shutdown_service_io(&mut self) -> Option<Result<Shutdown, SchedError>> {
        self.deferred.shutdown()
    }

    /// Queues `work` for the thread `index`, which waits for its reply. `false` when no
    /// service-I/O thread runs or the request already deferred once.
    pub(crate) fn defer_reply(
        &mut self,
        index: usize,
        name: &'static str,
        work: DeferredWork,
    ) -> bool {
        let deferred = &mut self.deferred;
        if deferred.io.is_none() || deferred.taken.is_some() {
            return false;
        }
        let ticket = deferred.next_ticket;
        deferred.next_ticket += 1;
        let inbox = Arc::clone(&deferred.inbox);
        let done: Arc<dyn Signal> = Arc::clone(&deferred.inbox) as Arc<dyn Signal>;
        let request = Request::new(name, done, move || {
            let message = work();
            inbox.push(Completion {
                thread: index,
                ticket,
                message,
            });
        });
        deferred.pending.push_back(request);
        self.submit_pending();
        self.deferred.taken = Some(ticket);
        true
    }

    /// Submits pending requests, oldest first, until the queue is full.
    fn submit_pending(&mut self) {
        let deferred = &mut self.deferred;
        let Some(io) = &deferred.io else {
            return;
        };
        while let Some(request) = deferred.pending.pop_front() {
            if let Err(full) = io.try_submit(request) {
                deferred.pending.push_front(full.into_request());
                return;
            }
        }
    }

    /// Copies each finished reply to the TLS of the thread that waits for it and makes that
    /// thread ready, then submits pending requests. Runs under the kernel lock, off the guest
    /// cores (the timer thread). Returns whether a thread was woken.
    pub(crate) fn deliver_replies(&mut self, mem: &AddressSpace) -> bool {
        let mut woke = false;
        for completion in self.deferred.inbox.take() {
            let waits = matches!(
                self.threads.waiting_for(completion.thread),
                Some(WaitKind::Reply { ticket }) if *ticket == completion.ticket
            );
            if !waits {
                continue;
            }
            let tls = self.threads.thread(completion.thread).tls;
            let code = match mem.write_bytes(tls, &completion.message) {
                Ok(()) => SUCCESS,
                Err(_) => INVALID_POINTER,
            };
            woke |= self.threads.end_wait(completion.thread, code, None);
        }
        self.submit_pending();
        woke
    }
}

/// Teardown only: submits `request`, yielding while the queue is full; the queue drains on its
/// own thread.
fn submit_waiting(io: &ServiceIo, mut request: Request) {
    loop {
        match io.try_submit(request) {
            Ok(()) => return,
            Err(SubmitError::Full(back)) => {
                request = back;
                std::thread::yield_now();
            }
            Err(SubmitError::ShutDown(_)) => {
                unreachable!("invariant: only `shutdown_service_io` shuts the thread down")
            }
        }
    }
}

#[cfg(test)]
mod tests;
