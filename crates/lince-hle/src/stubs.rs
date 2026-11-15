// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The first service stubs (T-054): `set`, `fsp-srv`, `time:u` and `hid` with the commands the
//! B2 program calls (docs/research/b2-svcs-and-ipc.md S2–S5). Each interface is a table of
//! commands and the fixed reply each one gives (ADR-0024: declarative descriptions). Every
//! command logs its name the first time it runs; any command not in the table is an
//! [`UnknownCommand`].
//!
//! The objects speak CMIF and accept both request message types (C3: 4 and 6). Interfaces a
//! command returns serve one object per session (no domains, ADR-0024 B2 scope). The file
//! system on a host directory is T-062: here `OpenSdCardFileSystem` returns an object with no
//! commands.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use lince_kernel::{IpcMessage, SessionObject, Sessions, UnknownCommand};
use lince_trace::{NameId, Trace, TraceError};

use crate::cmif::{self, Header, IN_MAGIC, OUT_MAGIC};
use crate::fsp::SdCard;
use crate::hipc::{Message, Parts};
use crate::sm::{OUT_OF_SESSIONS, ServiceRegistry};

/// Message type of a request in the pre-5.0.0 layout (C3); the B2 program sends it.
pub(crate) const TYPE_REQUEST_OLD: u16 = 4;
const SUCCESS: u32 = 0;

/// What a command replies.
enum Reply {
    /// Success with no output.
    Empty,
    /// Success with one `u64` output the function computes.
    Value(fn() -> u64),
    /// Success, moving a handle to a new session served by the interface.
    Object(&'static Interface),
}

struct Command {
    id: u32,
    /// The name in logs, `<interface>.<command>` with the public command name (LEGAL-12).
    name: &'static str,
    reply: Reply,
}

struct Interface {
    name: &'static str,
    commands: &'static [Command],
}

/// `set` (`ISettingsServer`, S2).
static SET: Interface = Interface {
    name: "set",
    commands: &[Command {
        id: 0,
        name: "set.GetLanguageCode",
        reply: Reply::Value(language_code),
    }],
};

/// `fsp-srv` (S3).
static FSP_SRV: Interface = Interface {
    name: "fsp-srv",
    commands: &[
        Command {
            id: 1,
            name: "fsp-srv.SetCurrentProcess",
            reply: Reply::Empty,
        },
        Command {
            id: 18,
            name: "fsp-srv.OpenSdCardFileSystem",
            reply: Reply::Object(&FILE_SYSTEM),
        },
    ],
};

/// `IFileSystem` (S3); its commands come with the host directory (T-062).
static FILE_SYSTEM: Interface = Interface {
    name: "fsp-srv.IFileSystem",
    commands: &[],
};

/// `time:u` (S4).
static TIME_U: Interface = Interface {
    name: "time:u",
    commands: &[Command {
        id: 0,
        name: "time:u.GetStandardUserSystemClock",
        reply: Reply::Object(&SYSTEM_CLOCK),
    }],
};

/// `ISystemClock` (S4).
static SYSTEM_CLOCK: Interface = Interface {
    name: "time.ISystemClock",
    commands: &[Command {
        id: 0,
        name: "time.ISystemClock.GetCurrentTime",
        reply: Reply::Value(posix_time),
    }],
};

/// `hid` (S5).
static HID: Interface = Interface {
    name: "hid",
    commands: &[Command {
        id: 0,
        name: "hid.CreateAppletResource",
        reply: Reply::Object(&APPLET_RESOURCE),
    }],
};

/// `IAppletResource` (S5); `GetSharedMemoryHandle`'s parameters are unknown (research O2).
static APPLET_RESOURCE: Interface = Interface {
    name: "hid.IAppletResource",
    commands: &[],
};

/// The services the B2 program reaches through `sm:`, ready for [`crate::sm::install`]. With
/// `sd`, `fsp-srv` serves that host directory ([`crate::fsp`]); without, it is the stub whose
/// file system has no commands.
pub fn b2_services(trace: &mut Trace, sd: Option<SdCard>) -> Result<ServiceRegistry, TraceError> {
    let mut registry = ServiceRegistry::default();
    let stubbed: &[&'static Interface] = match sd {
        Some(sd) => {
            let added = crate::fsp::register(&mut registry, trace, sd)?;
            debug_assert!(added, "fsp-srv registered twice");
            &[&SET, &TIME_U, &HID]
        }
        None => &[&SET, &FSP_SRV, &TIME_U, &HID],
    };
    for &interface in stubbed {
        let resolved = resolve(interface, trace)?;
        let added = registry.register(
            interface.name,
            Box::new(move || {
                Box::new(StubObject {
                    resolved: Arc::clone(&resolved),
                })
            }),
        );
        // The names are distinct constants; the tests check each one is reachable.
        debug_assert!(added, "{} registered twice", interface.name);
    }
    Ok(registry)
}

/// "en-US", the language code bytes zero-padded to a `u64` (S2).
// GUESS: the stub's language; research S2 does not fix one.
fn language_code() -> u64 {
    u64::from_le_bytes(*b"en-US\0\0\0")
}

/// Seconds since the POSIX epoch by the host clock (S4); 0 if the host clock is before it.
fn posix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// An interface with its trace names registered, built during setup.
struct Resolved {
    name: &'static str,
    trace_name: NameId,
    commands: Vec<Entry>,
}

struct Entry {
    id: u32,
    trace_name: NameId,
    action: Action,
}

enum Action {
    Empty,
    Value(fn() -> u64),
    Object(Arc<Resolved>),
}

fn resolve(interface: &'static Interface, trace: &mut Trace) -> Result<Arc<Resolved>, TraceError> {
    let mut commands = Vec::with_capacity(interface.commands.len());
    for command in interface.commands {
        let action = match command.reply {
            Reply::Empty => Action::Empty,
            Reply::Value(value) => Action::Value(value),
            Reply::Object(object) => Action::Object(resolve(object, trace)?),
        };
        commands.push(Entry {
            id: command.id,
            trace_name: trace.name(command.name)?,
            action,
        });
    }
    Ok(Arc::new(Resolved {
        name: interface.name,
        trace_name: trace.name(interface.name)?,
        commands,
    }))
}

/// One session to a stubbed interface.
struct StubObject {
    resolved: Arc<Resolved>,
}

impl SessionObject for StubObject {
    fn name(&self) -> &'static str {
        self.resolved.name
    }

    fn trace_name(&self) -> NameId {
        self.resolved.trace_name
    }

    /// A message that is not a CMIF request is reported as an unknown command with its
    /// message type, like `sm:` does.
    fn handle_request(
        &mut self,
        sessions: &mut dyn Sessions,
        message: &mut IpcMessage,
    ) -> Result<(), UnknownCommand> {
        let command = request_command(message)?;
        let unknown = UnknownCommand { command };
        let entry = self
            .resolved
            .commands
            .iter()
            .find(|entry| entry.id == command)
            .ok_or(unknown)?;
        sessions.log_stub(entry.trace_name);
        let reply = match &entry.action {
            Action::Empty => write_reply(message, SUCCESS, None, &[]),
            Action::Value(value) => write_reply(message, SUCCESS, None, &value().to_le_bytes()),
            Action::Object(object) => {
                let object = Box::new(StubObject {
                    resolved: Arc::clone(object),
                });
                match sessions.open_session(object) {
                    Some(handle) => write_reply(message, SUCCESS, Some(handle), &[]),
                    None => write_reply(message, OUT_OF_SESSIONS, None, &[]),
                }
            }
        };
        // These replies always fit; a failure is a host bug reported like an unknown command
        // rather than a silent success.
        reply.map(|_| ()).map_err(|_| unknown)
    }
}

/// The method id of a CMIF request.
fn request_command(message: &IpcMessage) -> Result<u32, UnknownCommand> {
    read_request(message).map(|(_, command, _)| command)
}

/// Reads a CMIF request: the message, its method id and the bytes after the CMIF header
/// (parameters, then padding). A message that is not a CMIF request (C3: type 4 or 6) is an
/// unknown command named by its message type.
pub(crate) fn read_request(
    message: &IpcMessage,
) -> Result<(Message<'_>, u32, &[u8]), UnknownCommand> {
    let unknown = |tag: u16| UnknownCommand {
        command: u32::from(tag),
    };
    let msg = Message::parse(message)
        .map_err(|_| unknown(u16::from_le_bytes([message[0], message[1]])))?;
    let tag = msg.tag();
    if tag != TYPE_REQUEST_OLD && tag != cmif::TYPE_REQUEST {
        return Err(unknown(tag));
    }
    let (header, params) = cmif::read(&msg, IN_MAGIC).map_err(|_| unknown(tag))?;
    Ok((msg, header.value, params))
}

/// Writes a CMIF reply with `result`, an optional moved handle and `params`.
pub(crate) fn write_reply(
    message: &mut IpcMessage,
    result: u32,
    handle: Option<u32>,
    params: &[u8],
) -> crate::Result<usize> {
    let handles = handle.as_slice();
    let parts = Parts {
        move_handles: handles,
        ..Parts::default()
    };
    let header = Header {
        magic: OUT_MAGIC,
        value: result,
        ..Header::default()
    };
    cmif::write(message, &parts, &header, params)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use lince_kernel::{IpcMessage, SessionObject, UnknownCommand};
    use lince_trace::Trace;

    use super::{FSP_SRV, HID, Interface, SET, StubObject, TIME_U, TYPE_REQUEST_OLD, resolve};
    use crate::cmif::{self, Header, IN_MAGIC, OUT_MAGIC};
    use crate::hipc::{MESSAGE_SIZE, Message, Parts};
    use crate::sm::service_name;
    use crate::testing::FakeSessions;
    use crate::tipc;

    fn object(interface: &'static Interface, trace: &mut Trace) -> Box<dyn SessionObject> {
        Box::new(StubObject {
            resolved: resolve(interface, trace).unwrap(),
        })
    }

    fn request(tag: u16, command: u32, pid: Option<u64>, params: &[u8]) -> IpcMessage {
        let mut message = [0; MESSAGE_SIZE];
        let parts = Parts {
            tag,
            pid,
            ..Parts::default()
        };
        let header = Header {
            magic: IN_MAGIC,
            value: command,
            ..Header::default()
        };
        cmif::write(&mut message, &parts, &header, params).unwrap();
        message
    }

    /// Sends `command` and returns the result, the moved handles and the first output word.
    fn call(
        object: &mut dyn SessionObject,
        sessions: &mut FakeSessions,
        command: u32,
    ) -> (u32, Vec<u32>, u64) {
        let mut message = request(TYPE_REQUEST_OLD, command, None, &[]);
        object.handle_request(sessions, &mut message).unwrap();
        let msg = Message::parse(&message).unwrap();
        let (header, params) = cmif::read(&msg, OUT_MAGIC).unwrap();
        let value = u64::from_le_bytes(*params.first_chunk::<8>().unwrap());
        (header.value, msg.move_handles().collect(), value)
    }

    #[test]
    fn set_get_language_code_returns_en_us_and_logs_its_name() {
        let mut trace = Trace::new(16);
        let mut set = object(&SET, &mut trace);
        let mut sessions = FakeSessions::default();
        let expected = u64::from_le_bytes(*b"en-US\0\0\0");
        assert_eq!(call(&mut *set, &mut sessions, 0), (0, vec![], expected));
        call(&mut *set, &mut sessions, 0);
        let name = trace.name("set.GetLanguageCode").unwrap();
        assert_eq!(
            sessions.stubs,
            [name, name],
            "each call logs the command's name"
        );
    }

    #[test]
    fn fsp_srv_sets_the_process_and_opens_the_sd_card_file_system() {
        let mut trace = Trace::new(16);
        let mut fsp = object(&FSP_SRV, &mut trace);
        let mut sessions = FakeSessions::default();
        let mut message = request(TYPE_REQUEST_OLD, 1, Some(0), &0u64.to_le_bytes());
        fsp.handle_request(&mut sessions, &mut message).unwrap();
        let msg = Message::parse(&message).unwrap();
        assert_eq!(cmif::read(&msg, OUT_MAGIC).unwrap().0.value, 0);
        let (result, handles, _) = call(&mut *fsp, &mut sessions, 18);
        assert_eq!((result, handles), (0, vec![0x100]));
        assert_eq!(sessions.objects[0].name(), "fsp-srv.IFileSystem");
        let mut file_system = sessions.objects.remove(0);
        let mut message = request(TYPE_REQUEST_OLD, 8, None, &[]);
        assert_eq!(
            file_system.handle_request(&mut sessions, &mut message),
            Err(UnknownCommand { command: 8 }),
            "the file system has no commands before T-062"
        );
    }

    #[test]
    fn time_u_opens_a_user_clock_that_reads_the_host_time() {
        let mut trace = Trace::new(16);
        let mut time = object(&TIME_U, &mut trace);
        let mut sessions = FakeSessions::default();
        assert_eq!(call(&mut *time, &mut sessions, 0).0, 0);
        assert_eq!(sessions.objects[0].name(), "time.ISystemClock");
        let mut clock = sessions.objects.remove(0);
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (result, handles, now) = call(&mut *clock, &mut sessions, 0);
        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!((result, handles), (0, vec![]));
        assert!((before..=after).contains(&now));
    }

    #[test]
    fn hid_creates_an_applet_resource_with_a_pid() {
        let mut trace = Trace::new(16);
        let mut hid = object(&HID, &mut trace);
        let mut sessions = FakeSessions::default();
        // The 5.0.0+ request type is accepted too.
        let mut message = request(cmif::TYPE_REQUEST, 0, Some(0), &0u64.to_le_bytes());
        hid.handle_request(&mut sessions, &mut message).unwrap();
        let msg = Message::parse(&message).unwrap();
        assert_eq!(cmif::read(&msg, OUT_MAGIC).unwrap().0.value, 0);
        assert_eq!(msg.move_handles().collect::<Vec<_>>(), [0x100]);
        assert_eq!(sessions.objects[0].name(), "hid.IAppletResource");
    }

    #[test]
    fn a_full_process_gets_an_error_and_no_handle() {
        let mut trace = Trace::new(16);
        let mut hid = object(&HID, &mut trace);
        let mut sessions = FakeSessions {
            full: true,
            ..FakeSessions::default()
        };
        let (result, handles, _) = call(&mut *hid, &mut sessions, 0);
        assert_eq!((result, handles), (crate::sm::OUT_OF_SESSIONS, vec![]));
    }

    #[test]
    fn other_commands_and_message_types_are_unknown() {
        let mut trace = Trace::new(16);
        let mut set = object(&SET, &mut trace);
        let mut sessions = FakeSessions::default();
        let mut message = request(TYPE_REQUEST_OLD, 4, None, &[]);
        assert_eq!(
            set.handle_request(&mut sessions, &mut message),
            Err(UnknownCommand { command: 4 })
        );
        let mut message = [0; MESSAGE_SIZE];
        tipc::write_request(&mut message, &Parts::default(), 0, &[]).unwrap();
        assert_eq!(
            set.handle_request(&mut sessions, &mut message),
            Err(UnknownCommand { command: 16 })
        );
        assert!(sessions.stubs.is_empty(), "nothing ran");
    }

    #[test]
    fn the_registry_serves_each_b2_service_by_name() {
        let mut trace = Trace::new(16);
        let registry = super::b2_services(&mut trace, None).unwrap();
        for (name, object) in [
            ("set", "set"),
            ("fsp-srv", "fsp-srv"),
            ("time:u", "time:u"),
            ("hid", "hid"),
        ] {
            let factory = registry.find(service_name(name).unwrap()).unwrap();
            assert_eq!(factory().name(), object);
        }
    }
}
