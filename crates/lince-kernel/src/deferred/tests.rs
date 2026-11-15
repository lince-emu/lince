// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Deferred replies through the kernel's own entry points: `SendSyncRequest` and
//! `deliver_replies`, with a real service-I/O thread.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use lince_common::GuestVa;
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission, SpaceConfig};
use lince_trace::{NameId, Trace};
use parking_lot::Mutex;

use crate::deferred::DeferredReply;
use crate::ipc_svc::{IpcMessage, SessionObject, Sessions, UnknownCommand};
use crate::kernel::{Current, Kernel, Outcome, Regs};
use crate::thread::{ThreadParams, WaitKind};

const CONNECT: u8 = 0x1F;
const SEND: u8 = 0x21;
const CLOSE: u8 = 0x16;
const NAME_AT: u64 = 0x1_0000;
const PATIENCE: Duration = Duration::from_secs(10);

/// Defers every request; its work waits for a byte on `release` and replies with that byte
/// everywhere but at offset 1, which echoes the request's first byte. A request whose first
/// byte has bit 7 set first opens another `slow` session, which the reply keeps unless the
/// byte is `0xFF`. Without a service-I/O thread it replies at once with `0xEE`.
struct Slow {
    name: NameId,
    release: Arc<Mutex<Receiver<u8>>>,
}

impl SessionObject for Slow {
    fn name(&self) -> &'static str {
        "slow"
    }

    fn trace_name(&self) -> NameId {
        self.name
    }

    fn handle_request(
        &mut self,
        sessions: &mut dyn Sessions,
        message: &mut IpcMessage,
    ) -> Result<(), UnknownCommand> {
        let tag = message[0];
        if tag & 0x80 != 0 {
            let other = Box::new(Slow {
                name: self.name,
                release: Arc::clone(&self.release),
            });
            sessions.open_session(other);
        }
        let release = Arc::clone(&self.release);
        let work = Box::new(move || {
            let byte = release.lock().recv().unwrap_or(0);
            let mut reply = [byte; 0x100];
            reply[1] = tag;
            if byte == 0xFF {
                DeferredReply::Release(reply)
            } else {
                DeferredReply::Keep(reply)
            }
        });
        if !sessions.defer("slow", work) {
            message.fill(0xEE);
        }
        Ok(())
    }
}

struct Fixture {
    kernel: Kernel,
    mem: AddressSpace,
    release: Sender<u8>,
    session: u64,
}

/// A kernel with `threads` guest threads, a session to `slow` opened by thread 0 and, when
/// `capacity` is given, a service-I/O thread with that queue size.
fn fixture(threads: usize, capacity: Option<usize>) -> Fixture {
    let mut trace = Trace::new(16);
    let mut kernel = Kernel::new(&mut trace).unwrap();
    let name = trace.name("slow").unwrap();
    let (release, rx) = mpsc::channel();
    let rx = Arc::new(Mutex::new(rx));
    kernel.register_named_port(
        "slow",
        Box::new(move || {
            Box::new(Slow {
                name,
                release: Arc::clone(&rx),
            })
        }),
    );
    if let Some(capacity) = capacity {
        kernel.start_service_io(capacity).unwrap();
    }
    let mem = AddressSpace::new(SpaceConfig::for_host()).unwrap();
    mem.map(
        GuestVa::new(NAME_AT),
        GUEST_PAGE_SIZE,
        Permission::READ | Permission::WRITE,
    )
    .unwrap();
    mem.write_bytes(GuestVa::new(NAME_AT), b"slow\0").unwrap();
    for _ in 0..threads {
        let params = ThreadParams {
            entry: GuestVa::new(0x2_0000),
            arg: 0,
            stack_top: 0,
            priority: 44,
            core: 0,
        };
        kernel.spawn_thread(&mem, params).unwrap();
    }
    let mut f = Fixture {
        kernel,
        mem,
        release,
        session: 0,
    };
    let regs = f.svc(0, CONNECT, [0, NAME_AT]);
    assert_eq!(regs[0], 0);
    f.session = regs[1];
    f
}

impl Fixture {
    /// Runs SVC `number` for `thread` with `X0`, `X1` = `x`; it must resume the thread.
    fn svc(&mut self, thread: usize, number: u8, x: [u64; 2]) -> Regs {
        let mut regs = Regs::default();
        regs[..2].copy_from_slice(&x);
        let outcome = self.run(thread, number, &mut regs);
        assert!(matches!(outcome, Outcome::Resume), "{number:#x} resumed");
        regs
    }

    fn run(&mut self, thread: usize, number: u8, regs: &mut Regs) -> Outcome {
        self.kernel.current = Some(Current {
            index: thread,
            core: 0,
        });
        let outcome = self.kernel.run_svc(&self.mem, number, regs).unwrap();
        self.kernel.current = None;
        outcome
    }

    /// Sends a request whose first byte is `tag` from `thread`; it must be deferred.
    fn send_deferred(&mut self, thread: usize, tag: u8) {
        let tls = self.kernel.threads.thread(thread).tls;
        self.mem.write_bytes(tls, &[tag]).unwrap();
        let mut regs = Regs::default();
        regs[0] = self.session;
        let outcome = self.run(thread, SEND, &mut regs);
        assert!(matches!(outcome, Outcome::Reschedule), "the thread waits");
    }

    fn waits_for_reply(&self, thread: usize) -> bool {
        matches!(
            self.kernel.threads.waiting_for(thread),
            Some(WaitKind::Reply { .. })
        )
    }

    /// Delivers replies until none of `threads` waits any more.
    fn deliver_all(&mut self, threads: &[usize]) {
        let start = Instant::now();
        while threads.iter().any(|&t| self.waits_for_reply(t)) {
            assert!(start.elapsed() < PATIENCE, "the replies never arrived");
            self.kernel.deliver_replies(&self.mem);
            std::thread::yield_now();
        }
    }

    fn tls(&self, thread: usize) -> IpcMessage {
        let mut message = [0; 0x100];
        self.mem
            .read_bytes(self.kernel.threads.thread(thread).tls, &mut message)
            .unwrap();
        message
    }
}

#[test]
fn a_deferred_reply_reaches_the_tls_once_its_work_is_done() {
    let mut f = fixture(1, Some(4));
    f.send_deferred(0, 7);
    assert!(f.waits_for_reply(0));
    assert!(!f.kernel.deliver_replies(&f.mem), "the work still waits");
    assert_eq!(f.tls(0)[0], 7, "the TLS holds the request until the reply");

    f.release.send(0x5A).unwrap();
    f.deliver_all(&[0]);

    let reply = f.tls(0);
    assert_eq!((reply[0], reply[1], reply[0xFF]), (0x5A, 7, 0x5A));
    assert_eq!(f.kernel.threads.thread(0).state.x[0], 0, "W0 is success");
    assert!(!f.kernel.deliver_replies(&f.mem), "delivered once only");
}

#[test]
fn a_full_queue_keeps_requests_pending_and_drops_none() {
    let mut f = fixture(3, Some(1));
    for thread in 0..3u8 {
        f.send_deferred(usize::from(thread), thread);
    }
    assert!(
        !f.kernel.deferred.pending.is_empty(),
        "one runs, one is queued, the rest wait in the kernel"
    );
    for thread in 0..3u8 {
        f.release.send(0x10 + thread).unwrap();
    }
    f.deliver_all(&[0, 1, 2]);

    assert!(f.kernel.deferred.pending.is_empty());
    for thread in 0..3u8 {
        assert_eq!(f.tls(usize::from(thread))[1], thread, "its own reply");
    }
}

#[test]
fn a_reply_for_a_thread_that_stopped_waiting_is_not_written() {
    let mut f = fixture(1, Some(4));
    f.send_deferred(0, 7);
    // As when the process stops: the wait ends some other way.
    assert!(f.kernel.threads.end_wait(0, 0xDEAD, None));
    f.release.send(0x5A).unwrap();

    let start = Instant::now();
    while f.kernel.deferred.inbox.done.lock().is_empty() {
        assert!(start.elapsed() < PATIENCE, "the work never finished");
        std::thread::yield_now();
    }
    assert!(!f.kernel.deliver_replies(&f.mem));
    assert_eq!(f.tls(0)[0], 7, "the TLS is left alone");
    assert_eq!(f.kernel.threads.thread(0).state.x[0], 0xDEAD);
}

#[test]
fn closing_the_session_keeps_the_reply_in_flight() {
    let mut f = fixture(2, Some(4));
    f.send_deferred(0, 7);
    let session = f.session;
    assert_eq!(
        f.svc(1, CLOSE, [session, 0])[0],
        0,
        "CloseHandle on the session"
    );
    f.release.send(0x5A).unwrap();
    f.deliver_all(&[0]);
    assert_eq!(f.tls(0)[0], 0x5A);
}

#[test]
fn shutdown_runs_every_pending_request() {
    let mut f = fixture(3, Some(1));
    for thread in 0..3 {
        f.send_deferred(thread, 0);
    }
    for _ in 0..3 {
        f.release.send(1).unwrap();
    }
    let shutdown = f.kernel.shutdown_service_io().unwrap().unwrap();
    assert_eq!(shutdown.completed, 3);
    assert!(f.kernel.shutdown_service_io().is_none());
}

#[test]
fn without_a_service_io_thread_the_object_replies_at_once() {
    let mut f = fixture(1, None);
    let tls = f.kernel.threads.thread(0).tls;
    f.mem.write_bytes(tls, &[7]).unwrap();
    let session = f.session;
    assert_eq!(f.svc(0, SEND, [session, 0])[0], 0);
    assert_eq!(f.tls(0)[0], 0xEE);
}

fn live_sessions(f: &Fixture) -> usize {
    f.kernel.sessions.iter().filter(|s| s.is_some()).count()
}

#[test]
fn a_kept_reply_hands_the_guest_the_sessions_its_request_opened() {
    let mut f = fixture(1, Some(4));
    f.send_deferred(0, 0x80);
    assert_eq!(live_sessions(&f), 2);
    f.release.send(0x5A).unwrap();
    f.deliver_all(&[0]);
    assert_eq!(live_sessions(&f), 2, "the new session stays");
    assert_eq!(f.kernel.deferred.opened, Vec::new());
}

#[test]
fn a_released_reply_closes_the_sessions_its_request_opened() {
    let mut f = fixture(1, Some(4));
    f.send_deferred(0, 0x80);
    f.release.send(0xFF).unwrap();
    f.deliver_all(&[0]);
    assert_eq!(f.tls(0)[0], 0xFF, "the reply is still delivered");
    assert_eq!(
        live_sessions(&f),
        1,
        "only the client's own session is left"
    );
    assert_eq!(f.kernel.deferred.opened, Vec::new());
}

#[test]
fn a_reply_no_thread_waits_for_closes_the_sessions_its_request_opened() {
    let mut f = fixture(1, Some(4));
    f.send_deferred(0, 0x80);
    assert!(f.kernel.threads.end_wait(0, 0xDEAD, None));
    f.release.send(0x5A).unwrap();
    let start = Instant::now();
    while !f.kernel.deferred.opened.is_empty() {
        assert!(start.elapsed() < PATIENCE, "the work never finished");
        f.kernel.deliver_replies(&f.mem);
        std::thread::yield_now();
    }
    assert_eq!(live_sessions(&f), 1);
}
