// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `sm:`, the service manager (docs/research/b2-svcs-and-ipc.md S1): the named port a program
//! connects to first, then asks for a session to each service by name. It speaks TIPC.
//!
//! B2 implements `RegisterClient` (0) and `GetServiceHandle` (1). The services come from an
//! explicit [`ServiceRegistry`] the host fills before the guest runs (ADR-0024); programs
//! cannot register services. Any other command is an [`UnknownCommand`].

use std::sync::Arc;

use lince_kernel::{IpcMessage, Kernel, SessionObject, Sessions, UnknownCommand};
use lince_trace::{NameId, Trace, TraceError};

use crate::hipc::{Message, Parts};
use crate::tipc;

/// The name of the port and of the object in logs.
pub const PORT_NAME: &str = "sm:";

/// Builds the object that serves a new session to a service. It runs on the guest core under
/// the kernel lock: it may allocate the object but must not block, do I/O or compile.
pub type ServiceFactory = Box<dyn Fn() -> Box<dyn SessionObject> + Send + Sync>;

const SUCCESS: u32 = 0;
const REGISTER_CLIENT: u16 = 0;
const GET_SERVICE_HANDLE: u16 = 1;
/// `GetServiceHandle` for a name no service has.
// GUESS: research O6 found no `sm:` result codes; module 21, description 7.
const NOT_REGISTERED: u32 = 0xE15;
/// The client is out of handles or sessions.
// GUESS: the kernel's out-of-resource code; `sm:` codes are not in the note (O6).
const OUT_OF_SESSIONS: u32 = 0xD001;

/// A service name as `sm:` takes it: up to 8 bytes, zero-padded, as a little-endian `u64`
/// (S1, section 3.4). `None` for an empty name, one longer than 8 bytes or one with a NUL.
pub fn service_name(name: &str) -> Option<u64> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 8 || bytes.contains(&0) {
        return None;
    }
    let mut packed = [0u8; 8];
    packed[..bytes.len()].copy_from_slice(bytes);
    Some(u64::from_le_bytes(packed))
}

/// The services `sm:` hands out, by name.
#[derive(Default)]
pub struct ServiceRegistry {
    services: Vec<(u64, ServiceFactory)>,
}

impl ServiceRegistry {
    /// Adds the service `name`, served by objects from `factory`. Returns `false`, and adds
    /// nothing, when the name is not a valid service name or is already registered.
    pub fn register(&mut self, name: &str, factory: ServiceFactory) -> bool {
        let Some(name) = service_name(name) else {
            return false;
        };
        if self.find(name).is_some() {
            return false;
        }
        self.services.push((name, factory));
        true
    }

    fn find(&self, name: u64) -> Option<&ServiceFactory> {
        self.services
            .iter()
            .find(|(registered, _)| *registered == name)
            .map(|(_, factory)| factory)
    }
}

/// Registers the `sm:` port with `kernel`, serving the services in `registry`. Call during
/// setup, before the guest runs.
pub fn install(
    kernel: &mut Kernel,
    trace: &mut Trace,
    registry: ServiceRegistry,
) -> Result<(), TraceError> {
    let trace_name = trace.name(PORT_NAME)?;
    let registry = Arc::new(registry);
    kernel.register_named_port(
        PORT_NAME,
        Box::new(move || {
            Box::new(ServiceManager {
                registry: Arc::clone(&registry),
                trace_name,
            })
        }),
    );
    Ok(())
}

/// One session to `sm:`.
struct ServiceManager {
    registry: Arc<ServiceRegistry>,
    trace_name: NameId,
}

impl SessionObject for ServiceManager {
    fn name(&self) -> &'static str {
        PORT_NAME
    }

    fn trace_name(&self) -> NameId {
        self.trace_name
    }

    /// A message that is not a well-formed TIPC request is reported as an unknown command
    /// with its message type, so it is logged and stops the guest instead of being guessed.
    fn handle_request(
        &mut self,
        sessions: &mut dyn Sessions,
        message: &mut IpcMessage,
    ) -> Result<(), UnknownCommand> {
        let (command, name) = {
            let unknown = |tag: u16| UnknownCommand {
                command: u32::from(tag),
            };
            let msg = Message::parse(message).map_err(|_| unknown(raw_tag(message)))?;
            let command = tipc::command(&msg).map_err(|_| unknown(msg.tag()))?;
            let name = msg.raw().first_chunk::<8>().map(|n| u64::from_le_bytes(*n));
            (command, name)
        };
        let reply = match (command, name) {
            (REGISTER_CLIENT, _) => tipc::write_reply(message, &Parts::default(), SUCCESS, &[]),
            (GET_SERVICE_HANDLE, Some(name)) => self.get_service_handle(sessions, message, name),
            _ => {
                return Err(UnknownCommand {
                    command: u32::from(command),
                });
            }
        };
        // A reply with no payload and at most one handle always fits; a failure is a host bug
        // reported like an unknown command rather than a silent success.
        reply.map(|_| ()).map_err(|_| UnknownCommand {
            command: u32::from(command),
        })
    }
}

impl ServiceManager {
    fn get_service_handle(
        &self,
        sessions: &mut dyn Sessions,
        message: &mut IpcMessage,
        name: u64,
    ) -> crate::Result<usize> {
        let Some(factory) = self.registry.find(name) else {
            return tipc::write_reply(message, &Parts::default(), NOT_REGISTERED, &[]);
        };
        let Some(handle) = sessions.open_session(factory()) else {
            return tipc::write_reply(message, &Parts::default(), OUT_OF_SESSIONS, &[]);
        };
        let parts = Parts {
            move_handles: &[handle],
            ..Parts::default()
        };
        tipc::write_reply(message, &parts, SUCCESS, &[])
    }
}

/// The message type of a message whose header could not be read.
fn raw_tag(message: &IpcMessage) -> u16 {
    u16::from_le_bytes([message[0], message[1]])
}

#[cfg(test)]
mod tests {
    use lince_kernel::{IpcMessage, SessionObject, Sessions, UnknownCommand};
    use lince_trace::{NameId, Trace};

    use super::{NOT_REGISTERED, ServiceManager, ServiceRegistry, service_name};
    use crate::hipc::{MESSAGE_SIZE, Message, Parts};
    use crate::tipc;

    struct Dummy(NameId);

    impl SessionObject for Dummy {
        fn name(&self) -> &'static str {
            "set"
        }

        fn trace_name(&self) -> NameId {
            self.0
        }

        fn handle_request(
            &mut self,
            _: &mut dyn Sessions,
            _: &mut IpcMessage,
        ) -> Result<(), UnknownCommand> {
            Ok(())
        }
    }

    /// Hands out handles 0x100, 0x101, … and keeps the objects.
    #[derive(Default)]
    struct FakeSessions(Vec<Box<dyn SessionObject>>);

    impl Sessions for FakeSessions {
        fn open_session(&mut self, object: Box<dyn SessionObject>) -> Option<u32> {
            self.0.push(object);
            u32::try_from(0xFF + self.0.len()).ok()
        }
    }

    fn manager() -> ServiceManager {
        let mut trace = Trace::new(4);
        let name = trace.name("set").unwrap();
        let mut registry = ServiceRegistry::default();
        assert!(registry.register("set", Box::new(move || Box::new(Dummy(name)))));
        ServiceManager {
            registry: registry.into(),
            trace_name: name,
        }
    }

    fn request(command: u16, payload: &[u8]) -> IpcMessage {
        let mut message = [0; MESSAGE_SIZE];
        tipc::write_request(&mut message, &Parts::default(), command, payload).unwrap();
        message
    }

    fn reply(message: &IpcMessage) -> (u32, Vec<u32>) {
        let msg = Message::parse(message).unwrap();
        (
            tipc::read_reply(&msg).unwrap().0,
            msg.move_handles().collect(),
        )
    }

    #[test]
    fn register_client_succeeds() {
        let mut message = request(0, &[0; 8]);
        manager()
            .handle_request(&mut FakeSessions::default(), &mut message)
            .unwrap();
        assert_eq!(reply(&message), (0, vec![]));
    }

    #[test]
    fn get_service_handle_moves_a_session_to_the_named_service() {
        let mut sessions = FakeSessions::default();
        let name = service_name("set").unwrap().to_le_bytes();
        let mut message = request(1, &name);
        manager()
            .handle_request(&mut sessions, &mut message)
            .unwrap();
        assert_eq!(reply(&message), (0, vec![0x100]));
        assert_eq!(sessions.0.len(), 1);
        assert_eq!(sessions.0[0].name(), "set");
    }

    #[test]
    fn get_service_handle_for_an_unknown_name_returns_an_error() {
        let mut sessions = FakeSessions::default();
        let name = service_name("fsp-srv").unwrap().to_le_bytes();
        let mut message = request(1, &name);
        manager()
            .handle_request(&mut sessions, &mut message)
            .unwrap();
        assert_eq!(reply(&message), (NOT_REGISTERED, vec![]));
        assert!(sessions.0.is_empty());
    }

    #[test]
    fn other_commands_and_cmif_messages_are_unknown() {
        let mut message = request(2, &[0; 8]);
        assert_eq!(
            manager().handle_request(&mut FakeSessions::default(), &mut message),
            Err(UnknownCommand { command: 2 })
        );
        let mut message = [0; MESSAGE_SIZE];
        crate::hipc::write(
            &mut message,
            &Parts {
                tag: 6,
                ..Parts::default()
            },
        )
        .unwrap();
        assert_eq!(
            manager().handle_request(&mut FakeSessions::default(), &mut message),
            Err(UnknownCommand { command: 6 })
        );
    }

    #[test]
    fn install_makes_the_port_reachable_by_name() {
        let mut trace = Trace::new(4);
        let mut kernel = lince_kernel::Kernel::new(&mut trace).unwrap();
        super::install(&mut kernel, &mut trace, ServiceRegistry::default()).unwrap();
        let mem = lince_mem::AddressSpace::new(lince_mem::SpaceConfig::for_host()).unwrap();
        let at = lince_common::GuestVa::new(0x1_0000);
        mem.map(
            at,
            lince_mem::GUEST_PAGE_SIZE,
            lince_mem::Permission::READ | lince_mem::Permission::WRITE,
        )
        .unwrap();
        mem.write_bytes(at, b"sm: ").unwrap();
        let mut regs = [0, at.get(), 0, 0, 0, 0, 0, 0];
        kernel.dispatch(&mem, 0x1F, &mut regs).unwrap();
        assert_eq!(regs[0], 0, "ConnectToNamedPort succeeded");
        assert_ne!(regs[1], 0, "with a session handle");
    }

    #[test]
    fn service_names_are_zero_padded_and_at_most_eight_bytes() {
        assert_eq!(service_name("sm:"), Some(0x3A_6D73));
        assert_eq!(
            service_name("fsp-srv").map(u64::to_le_bytes),
            Some(*b"fsp-srv\0")
        );
        assert_eq!(service_name("toolongname"), None);
        assert_eq!(service_name(""), None);
        let mut registry = ServiceRegistry::default();
        let mut trace = Trace::new(4);
        let id = trace.name("set").unwrap();
        assert!(registry.register("set", Box::new(move || Box::new(Dummy(id)))));
        assert!(
            !registry.register("set", Box::new(move || Box::new(Dummy(id)))),
            "registered twice"
        );
    }
}
