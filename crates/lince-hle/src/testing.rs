// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Test doubles shared by the service tests.

use lince_kernel::{DeferredWork, SessionObject, Sessions};
use lince_trace::NameId;

/// Keeps the objects it opens and the stub names logged; each handle is 0xFF plus the number
/// of objects kept. When `full`, it opens no session.
#[derive(Default)]
pub(crate) struct FakeSessions {
    pub(crate) objects: Vec<Box<dyn SessionObject>>,
    pub(crate) stubs: Vec<NameId>,
    pub(crate) full: bool,
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

    fn defer(&mut self, _: &'static str, _: DeferredWork) -> bool {
        false
    }
}
