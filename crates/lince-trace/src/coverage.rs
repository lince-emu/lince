// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The coverage list (T-055): every unimplemented SVC and IPC command a run reached, read back
//! from the drained records (doc 07 B2 exit criterion).

use std::fmt;

use crate::event::EventKind;
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

impl Trace {
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
