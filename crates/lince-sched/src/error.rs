// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error enums of `lince-sched` (PAT-01).

use std::fmt;
use std::io;

use thiserror::Error;

use crate::Request;

/// Errors from creating or tearing down a role thread.
#[derive(Debug, Error)]
pub enum SchedError {
    /// The queue capacity was zero; a bounded queue needs room for one request.
    #[error("queue capacity must be at least 1")]
    ZeroCapacity,
    /// The host refused to create the thread.
    #[error("cannot create thread {name}")]
    Spawn {
        /// Name of the role thread.
        name: &'static str,
        /// Error from the host.
        #[source]
        source: io::Error,
    },
    /// The role thread panicked; the requests it had not finished are listed in the message.
    #[error("thread {name} panicked with {pending} request(s) not completed")]
    Panicked {
        /// Name of the role thread.
        name: &'static str,
        /// Requests submitted but never completed.
        pending: u64,
    },
}

/// Why [`ServiceIo::try_submit`](crate::ServiceIo::try_submit) did not queue a request. The
/// request is always handed back: nothing is dropped (invariant 1).
#[derive(Error)]
pub enum SubmitError {
    /// The queue is full. The caller waits until the "space available" signal fires, then
    /// submits again.
    #[error("service-I/O queue is full")]
    Full(Request),
    /// The thread is shutting down and accepts no new requests.
    #[error("service-I/O thread is shut down")]
    ShutDown(Request),
}

impl SubmitError {
    /// The request that was not queued.
    pub fn into_request(self) -> Request {
        match self {
            Self::Full(request) | Self::ShutDown(request) => request,
        }
    }
}

impl fmt::Debug for SubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, request) = match self {
            Self::Full(request) => ("Full", request),
            Self::ShutDown(request) => ("ShutDown", request),
        };
        f.debug_tuple(kind).field(&request.name()).finish()
    }
}

/// `Result` with [`SchedError`] as the default error type.
pub type Result<T, E = SchedError> = std::result::Result<T, E>;
