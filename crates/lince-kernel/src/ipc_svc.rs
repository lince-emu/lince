// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! IPC sessions: `ConnectToNamedPort` and `SendSyncRequest` (docs/research/b2-svcs-and-ipc.md
//! V16, V17, Switch 1 knowledge).
//!
//! The server end of every session is host code, a [`SessionObject`] (ADR-0024, research
//! V19): `SendSyncRequest` hands it the calling thread's message buffer, under the kernel lock,
//! and copies the reply back. An object may open more sessions, such as `sm:` handing out a
//! service, through [`Sessions`]; the handle it gets is already in the client's table.

use lince_common::GuestVa;
use lince_mem::AddressSpace;
use lince_trace::NameId;

use crate::event_svc::INVALID_HANDLE;
use crate::handle::Object;
use crate::kernel::{Kernel, Outcome, Regs};
use crate::svc::{OUT_OF_RESOURCE, ResultCode, SUCCESS, done};

/// Size of the IPC message buffer at the start of a thread's TLS block.
pub const IPC_MESSAGE_SIZE: usize = 0x100;

/// A thread's IPC message buffer.
pub type IpcMessage = [u8; IPC_MESSAGE_SIZE];

/// Builds a new session object each time a program connects to a named port. It runs on the
/// guest core under the kernel lock: it may allocate the object but must not block, do I/O or
/// compile.
pub type PortFactory = Box<dyn Fn() -> Box<dyn SessionObject> + Send>;

/// The port name pointer is not readable (V10's invalid-address code).
// GUESS: the note lists no result for ConnectToNamedPort (V16).
const INVALID_POINTER: ResultCode = 0xE601;
/// No port has that name.
// GUESS: the note lists no result for an unknown name (V16); module 1, description 121.
const NOT_FOUND: ResultCode = 0xF201;
/// Longest port name, without its terminating NUL.
// GUESS: names such as `sm:` are short; Horizon's limit is not in the note.
const PORT_NAME_MAX: usize = 11;
/// Most sessions a process may have open at once.
// GUESS: Horizon's limit comes from the process's resource limit, which B2 does not model.
const MAX_SESSIONS: usize = 256;

/// A request the object has no behavior for. The kernel logs it once and stops the guest; it
/// never reports success (ADR-0024).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownCommand {
    /// The command id, or the message type when the message has no command id.
    pub command: u32,
}

/// What a session object may do to the client's process while it handles a request.
pub trait Sessions {
    /// Opens a new session served by `object` and returns the client's handle to it; `None`
    /// when the process is out of handles or sessions.
    fn open_session(&mut self, object: Box<dyn SessionObject>) -> Option<u32>;

    /// Records that the stub `name` ran, the first time only (ADR-0024: stubs log by name).
    fn log_stub(&mut self, name: NameId);
}

/// The server end of a session: host code that answers the requests a program sends.
pub trait SessionObject: Send {
    /// The object's public or descriptive name, for errors.
    fn name(&self) -> &'static str;

    /// The object's name as registered with the trace, for the unknown-command log.
    fn trace_name(&self) -> NameId;

    /// Reads the request in `message` and writes the reply over it.
    fn handle_request(
        &mut self,
        sessions: &mut dyn Sessions,
        message: &mut IpcMessage,
    ) -> Result<(), UnknownCommand>;
}

impl Sessions for Kernel {
    fn open_session(&mut self, object: Box<dyn SessionObject>) -> Option<u32> {
        let running = self.running_session;
        let free = self
            .sessions
            .iter()
            .enumerate()
            .position(|(index, slot)| slot.is_none() && Some(index) != running);
        if free.is_none() && self.sessions.len() >= MAX_SESSIONS {
            return None;
        }
        let index = free.unwrap_or(self.sessions.len());
        let handle = self.handles.open(Object::Session(index))?;
        if index == self.sessions.len() {
            self.sessions.push(Some(object));
        } else {
            self.sessions[index] = Some(object);
        }
        if self.running_session.is_some() {
            self.opened_during_request.push(handle);
        }
        Some(handle)
    }

    fn log_stub(&mut self, name: NameId) {
        Kernel::log_stub(self, name);
    }
}

/// `X1` pointer to a NUL-terminated name; out `W0`, `W1` session handle (V16).
pub(crate) fn connect_to_named_port(
    kernel: &mut Kernel,
    mem: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    let mut name = [0u8; PORT_NAME_MAX + 1];
    let base = GuestVa::new(regs[1]);
    let mut len = None;
    // Byte by byte, so a name at the end of a mapped page is still readable.
    for (i, byte) in name.iter_mut().enumerate() {
        let at = base.wrapping_add(i as u64);
        if mem.read_bytes(at, std::slice::from_mut(byte)).is_err() {
            return done(regs, INVALID_POINTER);
        }
        if *byte == 0 {
            len = Some(i);
            break;
        }
    }
    let Some(len) = len else {
        return done(regs, INVALID_POINTER);
    };
    let Some(port) = kernel
        .named_ports
        .iter()
        .position(|(port, _)| port.as_bytes() == &name[..len])
    else {
        return done(regs, NOT_FOUND);
    };
    let object = (kernel.named_ports[port].1)();
    let Some(handle) = kernel.open_session(object) else {
        return done(regs, OUT_OF_RESOURCE);
    };
    regs[1] = u64::from(handle);
    done(regs, SUCCESS)
}

/// `W0` session handle; out `W0` (V17). The request and the reply are the calling thread's
/// TLS message buffer.
pub(crate) fn send_sync_request(
    kernel: &mut Kernel,
    mem: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    let Some(current) = kernel.current else {
        return Outcome::Unsupported;
    };
    let handle = u32::try_from(regs[0] & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    let Some(Object::Session(session)) = kernel.handles.get(handle) else {
        return done(regs, INVALID_HANDLE);
    };
    let tls = kernel.threads.thread(current.index).tls;
    let mut message = [0u8; IPC_MESSAGE_SIZE];
    if mem.read_bytes(tls, &mut message).is_err() {
        return done(regs, INVALID_POINTER);
    }
    // The object leaves its slot while it runs, so it can open sessions through the kernel.
    let Some(mut object) = kernel.sessions[session].take() else {
        return done(regs, INVALID_HANDLE);
    };
    kernel.running_session = Some(session);
    let handled = object.handle_request(kernel, &mut message);
    kernel.running_session = None;
    let (name, trace_name) = (object.name(), object.trace_name());
    kernel.sessions[session] = Some(object);
    if let Err(unknown) = handled {
        close_opened(kernel);
        kernel.log_unknown_command(trace_name, unknown.command);
        return Outcome::UnknownCommand {
            object: name,
            command: unknown.command,
        };
    }
    // The TLS block was just read, so only a concurrent unmap fails here.
    if mem.write_bytes(tls, &message).is_err() {
        close_opened(kernel);
        return done(regs, INVALID_POINTER);
    }
    kernel.opened_during_request.clear();
    done(regs, SUCCESS)
}

/// Closes the sessions the object opened during a request whose reply the guest never got,
/// so no handle or session slot is left behind that the guest cannot name.
fn close_opened(kernel: &mut Kernel) {
    let mut opened = std::mem::take(&mut kernel.opened_during_request);
    for handle in opened.drain(..) {
        if let Some(Object::Session(index)) = kernel.handles.close(handle) {
            kernel.sessions[index] = None;
        }
    }
    kernel.opened_during_request = opened;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use lince_common::GuestVa;
    use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission, SpaceConfig};
    use lince_trace::{NameId, Trace};

    use super::{IpcMessage, SessionObject, Sessions, UnknownCommand};
    use crate::kernel::{Current, Kernel, Outcome, Regs};
    use crate::thread::{TLS_BASE, ThreadParams};
    use crate::{KernelError, SvcNumber};

    const CONNECT: SvcNumber = 0x1F;
    const SEND: SvcNumber = 0x21;
    const CLOSE: SvcNumber = 0x16;
    const NAME_AT: u64 = 0x1_0000;

    /// Echoes command 1 with every byte increased by its step; command 2 opens an echo session
    /// whose step is 10 more and puts its handle in the first word; anything else is unknown.
    struct Echo(NameId, u8);

    impl SessionObject for Echo {
        fn name(&self) -> &'static str {
            "echo"
        }

        fn trace_name(&self) -> NameId {
            self.0
        }

        fn handle_request(
            &mut self,
            sessions: &mut dyn Sessions,
            message: &mut IpcMessage,
        ) -> Result<(), UnknownCommand> {
            match message[0] {
                1 => message.iter_mut().for_each(|b| *b = b.wrapping_add(self.1)),
                2 => {
                    let handle = sessions
                        .open_session(Box::new(Echo(self.0, self.1 + 10)))
                        .unwrap_or(0);
                    message[..4].copy_from_slice(&handle.to_le_bytes());
                }
                other => {
                    return Err(UnknownCommand {
                        command: u32::from(other),
                    });
                }
            }
            Ok(())
        }
    }

    /// Opens an echo session, then fails: with `unmap`, its reply cannot be written back
    /// because it unmaps the caller's TLS; without, it reports an unknown command.
    struct Opener {
        name: NameId,
        mem: Arc<AddressSpace>,
        unmap: bool,
    }

    impl SessionObject for Opener {
        fn name(&self) -> &'static str {
            "opener"
        }

        fn trace_name(&self) -> NameId {
            self.name
        }

        fn handle_request(
            &mut self,
            sessions: &mut dyn Sessions,
            _: &mut IpcMessage,
        ) -> Result<(), UnknownCommand> {
            sessions.open_session(Box::new(Echo(self.name, 1)));
            if !self.unmap {
                return Err(UnknownCommand { command: 0 });
            }
            self.mem.unmap(TLS_BASE, GUEST_PAGE_SIZE).unwrap();
            Ok(())
        }
    }

    struct Fixture {
        trace: Trace,
        kernel: Kernel,
        mem: Arc<AddressSpace>,
    }

    impl Fixture {
        fn new() -> Self {
            let mut trace = Trace::new(16);
            let mut kernel = Kernel::new(&mut trace).unwrap();
            let echo = trace.name("echo").unwrap();
            kernel.register_named_port("echo", Box::new(move || Box::new(Echo(echo, 1))));
            let mem = Arc::new(AddressSpace::new(SpaceConfig::for_host()).unwrap());
            mem.map(
                GuestVa::new(NAME_AT),
                GUEST_PAGE_SIZE,
                Permission::READ | Permission::WRITE,
            )
            .unwrap();
            let params = ThreadParams {
                entry: GuestVa::new(0x2_0000),
                arg: 0,
                stack_top: 0,
                priority: 44,
                core: 0,
            };
            kernel.spawn_thread(&mem, params).unwrap();
            kernel.current = Some(Current { index: 0, core: 0 });
            Self { trace, kernel, mem }
        }

        fn svc(&mut self, number: SvcNumber, x0: u64, x1: u64) -> Result<Regs, KernelError> {
            let mut regs = Regs::default();
            regs[0] = x0;
            regs[1] = x1;
            self.kernel
                .run_svc(&self.mem, number, &mut regs)
                .map(|outcome| {
                    assert!(matches!(outcome, Outcome::Resume));
                    regs
                })
        }

        fn connect(&mut self, name: &[u8]) -> Regs {
            self.mem.write_bytes(GuestVa::new(NAME_AT), name).unwrap();
            self.svc(CONNECT, 0, NAME_AT).unwrap()
        }

        fn message(&self) -> IpcMessage {
            let mut message = [0; 0x100];
            self.mem.read_bytes(TLS_BASE, &mut message).unwrap();
            message
        }
    }

    #[test]
    fn a_request_reaches_the_object_and_its_reply_reaches_the_tls() {
        let mut f = Fixture::new();
        let regs = f.connect(b"echo\0");
        assert_eq!(regs[0], 0);
        let mut request = [7u8; 0x100];
        request[0] = 1;
        f.mem.write_bytes(TLS_BASE, &request).unwrap();
        assert_eq!(f.svc(SEND, regs[1], 0).unwrap()[0], 0);
        let reply = f.message();
        assert_eq!(reply[0], 2);
        assert!(reply[1..].iter().all(|&b| b == 8));
    }

    #[test]
    fn an_object_opens_sessions_the_client_can_use() {
        let mut f = Fixture::new();
        let first = f.connect(b"echo\0")[1];
        f.mem.write_bytes(TLS_BASE, &[2]).unwrap();
        f.svc(SEND, first, 0).unwrap();
        let second = u64::from(u32::from_le_bytes(f.message()[..4].try_into().unwrap()));
        assert_ne!(second, first);
        f.mem.write_bytes(TLS_BASE, &[1]).unwrap();
        assert_eq!(f.svc(SEND, second, 0).unwrap()[0], 0);
        assert_eq!(f.message()[0], 12, "the new object answers, not the opener");
        f.mem.write_bytes(TLS_BASE, &[1]).unwrap();
        f.svc(SEND, first, 0).unwrap();
        assert_eq!(
            f.message()[0],
            2,
            "the opener still answers its own session"
        );
        assert_eq!(f.svc(CLOSE, first, 0).unwrap()[0], 0);
        f.mem.write_bytes(TLS_BASE, &[1]).unwrap();
        assert_eq!(
            f.svc(SEND, second, 0).unwrap()[0],
            0,
            "closing one keeps the other"
        );
        assert_eq!(f.message()[0], 12);
    }

    fn live_sessions(f: &Fixture) -> usize {
        f.kernel.sessions.iter().filter(|s| s.is_some()).count()
    }

    #[test]
    fn sessions_opened_by_a_request_that_fails_are_closed_again() {
        for unmap in [false, true] {
            let mut f = Fixture::new();
            let (name, mem) = (f.trace.name("opener").unwrap(), Arc::clone(&f.mem));
            f.kernel.register_named_port(
                "opener",
                Box::new(move || {
                    Box::new(Opener {
                        name,
                        mem: Arc::clone(&mem),
                        unmap,
                    })
                }),
            );
            let session = f.connect(b"opener\0")[1];
            let result = f.svc(SEND, session, 0);
            if unmap {
                assert_eq!(result.unwrap()[0], 0xE601, "the reply could not be written");
            } else {
                assert!(matches!(result, Err(KernelError::UnknownCommand { .. })));
            }
            assert_eq!(
                live_sessions(&f),
                1,
                "only the opener's own session is left"
            );
            assert_eq!(f.kernel.opened_during_request, [0u32; 0]);
        }
    }

    #[test]
    fn an_unknown_command_is_logged_once_and_stops_the_guest() {
        let mut f = Fixture::new();
        let session = f.connect(b"echo\0")[1];
        f.mem.write_bytes(TLS_BASE, &[9]).unwrap();
        for _ in 0..2 {
            assert_eq!(
                f.svc(SEND, session, 0),
                Err(KernelError::UnknownCommand {
                    object: "echo",
                    command: 9
                })
            );
        }
        let drained = f.trace.drain();
        let logged: Vec<_> = drained
            .records
            .iter()
            .filter_map(|r| f.trace.unknown_command(&r.event))
            .collect();
        assert_eq!(logged, [("echo", 9)]);
    }

    #[test]
    fn in_coverage_mode_unknown_calls_return_an_error_and_are_all_listed() {
        let mut f = Fixture::new();
        f.kernel.set_keep_going(true);
        let not_implemented = u64::from(crate::NOT_IMPLEMENTED);
        assert_eq!(f.svc(0x7F, 0, 0).unwrap()[0], not_implemented);
        let session = f.connect(b"echo ")[1];
        f.mem.write_bytes(TLS_BASE, &[9]).unwrap();
        assert_eq!(f.svc(SEND, session, 0).unwrap()[0], not_implemented);
        f.mem.write_bytes(TLS_BASE, &[1]).unwrap();
        assert_eq!(
            f.svc(SEND, session, 0).unwrap()[0],
            0,
            "the session still works"
        );
        let drained = f.trace.drain();
        let list: Vec<_> = f
            .trace
            .unimplemented(&drained)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(list, ["SVC 0x7f", "echo command 9"]);
    }

    #[test]
    fn unknown_names_bad_handles_and_closed_sessions_return_errors() {
        let mut f = Fixture::new();
        assert_eq!(f.connect(b"nope\0")[0], 0xF201);
        assert_eq!(
            f.connect(b"echo_long_name")[0],
            0xE601,
            "no NUL in 12 bytes"
        );
        assert_eq!(f.svc(CONNECT, 0, 0).unwrap()[0], 0xE601, "unmapped name");
        assert_eq!(f.svc(SEND, 0x1234, 0).unwrap()[0], 0xE401);
        let session = f.connect(b"echo\0")[1];
        assert_eq!(f.svc(CLOSE, session, 0).unwrap()[0], 0);
        assert_eq!(f.svc(SEND, session, 0).unwrap()[0], 0xE401);
        assert!(
            f.kernel.sessions.iter().all(Option::is_none),
            "object dropped"
        );
    }

    #[test]
    fn send_sync_request_without_a_current_thread_is_unimplemented() {
        let mut f = Fixture::new();
        let session = f.connect(b"echo\0")[1];
        f.kernel.current = None;
        assert!(matches!(
            f.svc(SEND, session, 0),
            Err(KernelError::Unimplemented { .. })
        ));
    }
}
