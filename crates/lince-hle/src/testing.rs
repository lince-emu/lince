// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Test doubles shared by the service tests.

use lince_kernel::{DeferredReply, DeferredWork, SessionObject, Sessions};
use lince_trace::NameId;

/// Keeps the objects it opens and the stub names logged; each handle is 0xFF plus the number
/// of objects kept. When `full`, it opens no session. With `run_deferred`, a deferred work
/// runs at once on the test thread, standing in for the service-I/O thread, and its reply is
/// kept in `deferred`; without, deferring fails.
#[derive(Default)]
pub(crate) struct FakeSessions {
    pub(crate) objects: Vec<Box<dyn SessionObject>>,
    pub(crate) stubs: Vec<NameId>,
    pub(crate) full: bool,
    pub(crate) run_deferred: bool,
    pub(crate) deferred: Vec<DeferredReply>,
}

impl Sessions for FakeSessions {
    fn open_session(&mut self, object: Box<dyn SessionObject>) -> Option<u32> {
        if self.full {
            return None;
        }
        self.objects.push(object);
        u32::try_from(0xFF + self.objects.len()).ok()
    }

    fn log_stub(&mut self, name: NameId) {
        self.stubs.push(name);
    }

    fn defer(&mut self, _: &'static str, work: DeferredWork) -> bool {
        if self.run_deferred {
            self.deferred.push(work());
        }
        self.run_deferred
    }
}
