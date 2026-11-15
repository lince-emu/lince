// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The service-I/O role thread (ADR-0017 B2 scope): blocking service calls, queued.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};

use crate::{Result, SchedError, SubmitError};

/// Name of the host thread, as shown by debuggers and profilers.
const THREAD_NAME: &str = "lince-service-io";

/// A wake-up the caller provides, such as a kernel event. It may be called from the
/// service-I/O thread and must not block.
pub trait Signal: Send + Sync {
    /// Fires the signal.
    fn signal(&self);
}

/// Work to run on the service-I/O thread. The work stores its own result (for example in the
/// guest thread's reply buffer); `done` fires after the work returns.
pub struct Request {
    name: &'static str,
    done: Arc<dyn Signal>,
    work: Box<dyn FnOnce() + Send>,
}

impl Request {
    /// A request named `name` (for diagnostics) that runs `work`, then fires `done`.
    pub fn new(
        name: &'static str,
        done: Arc<dyn Signal>,
        work: impl FnOnce() + Send + 'static,
    ) -> Self {
        Self {
            name,
            done,
            work: Box::new(work),
        }
    }

    /// The diagnostic name given at creation.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// What [`ServiceIo::shutdown`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shutdown {
    /// Requests completed over the thread's life, including those still queued when shutdown
    /// began: every accepted request is completed.
    pub completed: u64,
}

/// State shared by submitters and the thread.
struct Shared {
    /// Set by a submitter that found the queue full; the thread fires `space` on its next pop.
    waiting_for_space: AtomicBool,
    space: Arc<dyn Signal>,
    submitted: AtomicU64,
    completed: AtomicU64,
}

/// The service-I/O role thread and its bounded queue.
///
/// Dropping it without [`shutdown`](Self::shutdown) still completes every queued request, but
/// does not wait for them.
pub struct ServiceIo {
    tx: SyncSender<Request>,
    shared: Arc<Shared>,
    thread: JoinHandle<()>,
}

impl ServiceIo {
    /// Starts the thread with a queue of `capacity` requests. `space` fires after a
    /// [`SubmitError::Full`] once the queue has room; it may also fire when no one waits.
    pub fn start(capacity: usize, space: Arc<dyn Signal>) -> Result<Self> {
        if capacity == 0 {
            return Err(SchedError::ZeroCapacity);
        }
        let (tx, rx) = mpsc::sync_channel(capacity);
        let shared = Arc::new(Shared {
            waiting_for_space: AtomicBool::new(false),
            space,
            submitted: AtomicU64::new(0),
            completed: AtomicU64::new(0),
        });
        let thread_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || serve(&rx, &thread_shared))
            .map_err(|source| SchedError::Spawn {
                name: THREAD_NAME,
                source,
            })?;
        Ok(Self { tx, shared, thread })
    }

    /// Queues `request` without blocking. On [`SubmitError::Full`] the caller keeps the
    /// request, waits for the `space` signal and submits it again.
    pub fn try_submit(&self, request: Request) -> Result<(), SubmitError> {
        match self.tx.try_send(request) {
            Ok(()) => {
                self.accepted();
                Ok(())
            }
            Err(TrySendError::Disconnected(request)) => Err(SubmitError::ShutDown(request)),
            Err(TrySendError::Full(request)) => {
                self.shared.waiting_for_space.store(true, Ordering::SeqCst);
                // The thread may have popped between the failed send and the store above, and
                // then missed the flag; retry once so that wake-up cannot be lost.
                match self.tx.try_send(request) {
                    Ok(()) => {
                        self.accepted();
                        Ok(())
                    }
                    Err(TrySendError::Full(request)) => Err(SubmitError::Full(request)),
                    Err(TrySendError::Disconnected(request)) => Err(SubmitError::ShutDown(request)),
                }
            }
        }
    }

    fn accepted(&self) {
        self.shared.submitted.fetch_add(1, Ordering::Relaxed);
    }

    /// Stops accepting requests, completes every queued one and joins the thread. Teardown
    /// only: it blocks until the queue is empty.
    pub fn shutdown(self) -> Result<Shutdown> {
        let Self { tx, shared, thread } = self;
        drop(tx);
        let joined = thread.join();
        let completed = shared.completed.load(Ordering::Acquire);
        match joined {
            Ok(()) => Ok(Shutdown { completed }),
            Err(_) => Err(SchedError::Panicked {
                name: THREAD_NAME,
                pending: shared.submitted.load(Ordering::Relaxed) - completed,
            }),
        }
    }
}

impl fmt::Debug for ServiceIo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceIo")
            .field("submitted", &self.shared.submitted.load(Ordering::Relaxed))
            .field("completed", &self.shared.completed.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// The thread body: runs requests in order until every sender is gone and the queue is empty.
fn serve(rx: &Receiver<Request>, shared: &Shared) {
    while let Ok(request) = rx.recv() {
        if shared.waiting_for_space.swap(false, Ordering::SeqCst) {
            shared.space.signal();
        }
        let Request { done, work, .. } = request;
        work();
        shared.completed.fetch_add(1, Ordering::Release);
        done.signal();
    }
}
