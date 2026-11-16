// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The coverage list (T-055): every unimplemented SVC and IPC command a run reached, read back
//! from the drained records (doc 07 B2 exit criterion).

use std::fmt;

use crate::event::{EventKind, HostIoError};
use crate::trace::{Drained, Trace};

/// A call a run reached that has no implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unimplemented {
    /// An SVC number with no handler.
    Svc(u64),
    /// A command an IPC object has no behavior for.
    Command {
        /// The object's name.
        object: &'static str,
        /// The command id.
        command: u32,
    },
}

impl fmt::Display for Unimplemented {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Svc(number) => write!(f, "SVC {number:#04x}"),
            Self::Command { object, command } => write!(f, "{object} command {command}"),
        }
    }
}

/// A request a run sent that failed on the host (T-065), by object, command and error class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct HostIoFailure {
    /// The object's name.
    pub object: &'static str,
    /// The command id.
    pub command: u16,
    /// Why it failed.
    pub error: HostIoError,
}

impl fmt::Display for HostIoFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let why = match self.error {
            HostIoError::NotFound => "not found",
            HostIoError::PermissionDenied => "permission denied",
            HostIoError::OutsideRoot => "outside the root",
            HostIoError::WrongType => "wrong entry type",
            HostIoError::StorageFull => "storage full",
            HostIoError::GuestMemory => "guest memory",
            HostIoError::Other => "other host error",
            HostIoError::Unknown(raw) => {
                return write!(
                    f,
                    "{} command {}: error class {raw}",
                    self.object, self.command
                );
            }
        };
        write!(f, "{} command {}: {why}", self.object, self.command)
    }
}

impl Trace {
    /// The host I/O failures in `drained`, sorted, each listed once even when several threads
    /// recorded it. Other records are ignored.
    pub fn host_io_failures(&self, drained: &Drained) -> Vec<HostIoFailure> {
        let mut list: Vec<_> = drained
            .records
            .iter()
            .filter_map(|record| self.host_io_failure(&record.event))
            .map(|(object, command, error)| HostIoFailure {
                object,
                command,
                error,
            })
            .collect();
        list.sort_unstable();
        list.dedup();
        list
    }

    /// The unimplemented calls in `drained`, sorted, each listed once even when several
    /// threads recorded it. Other records are ignored.
    pub fn unimplemented(&self, drained: &Drained) -> Vec<Unimplemented> {
        let mut list: Vec<_> = drained
            .records
            .iter()
            .filter_map(|record| {
                if record.event.kind == EventKind::UnknownSvc {
                    return Some(Unimplemented::Svc(record.event.arg));
                }
                self.unknown_command(&record.event)
                    .map(|(object, command)| Unimplemented::Command { object, command })
            })
            .collect();
        list.sort_unstable();
        list.dedup();
        list
    }
}
