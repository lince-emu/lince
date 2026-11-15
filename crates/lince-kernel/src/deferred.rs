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

use crate::ipc_svc::{INVALID_POINTER, IpcMessage, close_sessions};
use crate::kernel::Kernel;
use crate::svc::SUCCESS;
use crate::thread::WaitKind;

/// Builds the reply of a deferred request on the service-I/O thread: it may block and do host
/// I/O, and returns the whole reply message.
pub type DeferredWork = Box<dyn FnOnce() -> DeferredReply + Send>;

/// The reply a [`DeferredWork`] built, and what becomes of the sessions its object opened
/// while handling the request (such as the file an `OpenFile` hands out).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeferredReply {
    /// A reply that hands those sessions to the guest.
    Keep(IpcMessage),
    /// A reply that names none of them, such as an error: the kernel closes them, so a failed
    /// request leaves no session the guest cannot name.
    Release(IpcMessage),
}

/// A reply built on the service-I/O thread, waiting to be delivered.
struct Completion {
    thread: usize,
    ticket: u64,
    reply: DeferredReply,
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
    /// The sessions each deferred request opened, by ticket, until its reply is delivered.
    pub(crate) opened: Vec<(u64, Vec<u32>)>,
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
            let reply = work();
            inbox.push(Completion {
                thread: index,
                ticket,
                reply,
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
    /// thread ready, then submits pending requests. The sessions a request opened are closed
    /// when the guest never learns them: a [`DeferredReply::Release`], a thread that no longer
    /// waits, or a TLS the reply cannot be written to. Runs under the kernel lock, off the
    /// guest cores (the timer thread). Returns whether a thread was woken.
    pub(crate) fn deliver_replies(&mut self, mem: &AddressSpace) -> bool {
        let mut woke = false;
        for completion in self.deferred.inbox.take() {
            let opened = self.take_opened(completion.ticket);
            let (message, keep) = match completion.reply {
                DeferredReply::Keep(message) => (message, true),
                DeferredReply::Release(message) => (message, false),
            };
            let delivered = self.deliver(mem, completion.thread, completion.ticket, &message);
            woke |= delivered.is_some();
            if !(delivered == Some(true) && keep) {
                close_sessions(self, opened);
            }
        }
        self.submit_pending();
        woke
    }

    /// Writes `message` to the TLS of `thread` and ends its wait, if it still waits for the
    /// reply with `ticket`. `None` when it does not; otherwise whether the write succeeded.
    fn deliver(
        &mut self,
        mem: &AddressSpace,
        thread: usize,
        ticket: u64,
        message: &IpcMessage,
    ) -> Option<bool> {
        let waits = matches!(
            self.threads.waiting_for(thread),
            Some(WaitKind::Reply { ticket: t }) if *t == ticket
        );
        if !waits {
            return None;
        }
        let tls = self.threads.thread(thread).tls;
        let written = mem.write_bytes(tls, message).is_ok();
        let code = if written { SUCCESS } else { INVALID_POINTER };
        self.threads.end_wait(thread, code, None);
        Some(written)
    }

    /// The sessions the request with `ticket` opened; empty when it opened none.
    fn take_opened(&mut self, ticket: u64) -> Vec<u32> {
        let opened = &mut self.deferred.opened;
        match opened.iter().position(|(t, _)| *t == ticket) {
            Some(at) => opened.swap_remove(at).1,
            None => Vec::new(),
        }
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
