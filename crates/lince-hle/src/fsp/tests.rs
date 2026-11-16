// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `fsp-srv` through the IPC framework, on a scratch host directory. The fake sessions run each
//! deferred work at once, in place of the service-I/O thread.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lince_common::GuestVa;
use lince_kernel::{DeferredReply, IpcMessage, SessionObject, UnknownCommand};
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission, SpaceConfig};
use lince_trace::Trace;

use super::{FileSystem, FspSrv, OUT_OF_RANGE, PATH_NOT_FOUND, SdCard, Shared};
use crate::cmif::{self, Header, IN_MAGIC, OUT_MAGIC};
use crate::hipc::{MESSAGE_SIZE, Map, Message, Parts, Pointer};
use crate::sm::service_name;
use crate::stubs::TYPE_REQUEST_OLD;
use crate::testing::FakeSessions;

const PATH_AT: u64 = 0x1_0000;
const OUT_AT: u64 = 0x1_1000;
const GET_ENTRY_TYPE: u32 = 7;
const OPEN_FILE: u32 = 8;
const READ: u32 = 0;
const WRITE: u32 = 1;
const FLUSH: u32 = 2;
const SET_SIZE: u32 = 3;
const GET_SIZE: u32 = 4;

/// A scratch directory holding the SD-card root (`data.bin` with bytes 0 to 99, an empty
/// directory `dir`) and, beside the root, `outside.txt`, which the guest must never reach.
struct Scratch {
    base: PathBuf,
    shared: Arc<Shared>,
    mem: Arc<AddressSpace>,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).expect("remove the scratch directory");
    }
}

fn scratch(test: &str) -> Scratch {
    scratch_with(test, false)
}

/// A scratch SD card the guest may write to when `writable`.
fn scratch_with(test: &str, writable: bool) -> Scratch {
    let base = std::env::temp_dir().join(format!("lince-fsp-{}-{test}", std::process::id()));
    if base.exists() {
        fs::remove_dir_all(&base).expect("clear the scratch directory");
    }
    let root = base.join("sd");
    fs::create_dir_all(root.join("dir")).expect("create the root");
    fs::write(root.join("data.bin"), (0u8..100).collect::<Vec<_>>()).expect("write data");
    fs::write(base.join("outside.txt"), b"secret").expect("write outside");

    let mem = Arc::new(AddressSpace::new(SpaceConfig::for_host()).expect("address space"));
    for at in [PATH_AT, OUT_AT] {
        mem.map(
            GuestVa::new(at),
            GUEST_PAGE_SIZE,
            Permission::READ | Permission::WRITE,
        )
        .expect("map");
    }
    let sd = SdCard::new(&root, Arc::clone(&mem)).expect("root");
    let mut trace = Trace::new(16);
    let shared = Arc::new(Shared {
        root: sd.root,
        mem: sd.mem,
        writable,
        fsp: trace.name("fsp-srv").expect("name"),
        file_system: trace.name("fsp-srv.IFileSystem").expect("name"),
        file: trace.name("fsp-srv.IFile").expect("name"),
        set_current_process: trace.name("fsp-srv.SetCurrentProcess").expect("name"),
    });
    Scratch { base, shared, mem }
}

impl Scratch {
    fn root(&self) -> &Path {
        &self.shared.root
    }

    fn file_system(&self) -> Box<dyn SessionObject> {
        Box::new(FileSystem(Arc::clone(&self.shared)))
    }

    /// A CMIF request for `command` with `params`, the guest path `path` (NUL added) in a
    /// pointer buffer and a receive buffer of `out` bytes.
    fn request(&self, command: u32, params: &[u8], path: Option<&[u8]>, out: u64) -> IpcMessage {
        let mut pointers = Vec::new();
        if let Some(path) = path {
            let mut raw = path.to_vec();
            raw.push(0);
            self.mem
                .write_bytes(GuestVa::new(PATH_AT), &raw)
                .expect("write the path");
            pointers.push(Pointer {
                index: 0,
                address: PATH_AT,
                size: 0x301,
            });
        }
        let receives = [Map {
            address: OUT_AT,
            size: out,
            attribute: 0,
        }];
        let parts = Parts {
            tag: TYPE_REQUEST_OLD,
            pointers: &pointers,
            receives: if out > 0 { &receives } else { &[] },
            ..Parts::default()
        };
        let header = Header {
            magic: IN_MAGIC,
            value: command,
            ..Header::default()
        };
        let mut message = [0; MESSAGE_SIZE];
        cmif::write(&mut message, &parts, &header, params).expect("request fits");
        message
    }
}

/// The result, the moved handles and the first output word of a reply.
fn read_reply(message: &IpcMessage) -> (u32, Vec<u32>, u64) {
    let msg = Message::parse(message).expect("reply parses");
    let (header, params) = cmif::read(&msg, OUT_MAGIC).expect("reply header");
    let value = params
        .first_chunk::<8>()
        .map_or(0, |b| u64::from_le_bytes(*b));
    (header.value, msg.move_handles().collect(), value)
}

/// Sends `message` to `object` with deferred work run at once; returns the reply and whether
/// it releases the sessions the request opened.
fn call(
    object: &mut dyn SessionObject,
    sessions: &mut FakeSessions,
    mut message: IpcMessage,
) -> ((u32, Vec<u32>, u64), bool) {
    sessions.run_deferred = true;
    object
        .handle_request(sessions, &mut message)
        .expect("a known command");
    match sessions.deferred.pop() {
        Some(DeferredReply::Keep(reply)) => (read_reply(&reply), false),
        Some(DeferredReply::Release(reply)) => (read_reply(&reply), true),
        None => (read_reply(&message), false),
    }
}

fn entry_type(s: &Scratch, path: &[u8]) -> (u32, u64) {
    let message = s.request(GET_ENTRY_TYPE, &[], Some(path), 0);
    let ((result, _, value), _) =
        call(&mut *s.file_system(), &mut FakeSessions::default(), message);
    (result, value & 0xFFFF_FFFF)
}

#[test]
fn get_entry_type_tells_files_from_directories() {
    let s = scratch("entry-type");
    assert_eq!(entry_type(&s, b"data.bin"), (0, 1));
    assert_eq!(entry_type(&s, b"/dir"), (0, 0));
    assert_eq!(
        entry_type(&s, b"/./dir//"),
        (0, 0),
        "empty and . parts are skipped"
    );
    assert_eq!(entry_type(&s, b"missing.bin").0, PATH_NOT_FOUND);
}

#[test]
fn paths_that_leave_the_root_are_not_found_without_touching_the_host() {
    let s = scratch("escape");
    for path in [
        &b"../outside.txt"[..],
        b"dir/../data.bin",
        b"..\\outside.txt",
        b"C:/Windows",
        b"@Sdcard:/data.bin",
    ] {
        let message = s.request(GET_ENTRY_TYPE, &[], Some(path), 0);
        let mut sessions = FakeSessions::default();
        let (reply, _) = call(&mut *s.file_system(), &mut sessions, message);
        assert_eq!(reply.0, PATH_NOT_FOUND, "{}", String::from_utf8_lossy(path));
        assert!(
            sessions.deferred.is_empty(),
            "rejected before any host access"
        );
    }
    let mut unterminated = [b'a'; 0x301];
    unterminated[0x300] = b'b';
    let mut message = s.request(GET_ENTRY_TYPE, &[], None, 0);
    s.mem
        .write_bytes(GuestVa::new(PATH_AT), &unterminated)
        .expect("write");
    message = {
        let parts = Parts {
            tag: TYPE_REQUEST_OLD,
            pointers: &[Pointer {
                index: 0,
                address: PATH_AT,
                size: 0x301,
            }],
            ..Parts::default()
        };
        let header = Header {
            magic: IN_MAGIC,
            value: GET_ENTRY_TYPE,
            ..Header::default()
        };
        cmif::write(&mut message, &parts, &header, &[]).expect("fits");
        message
    };
    let (reply, _) = call(&mut *s.file_system(), &mut FakeSessions::default(), message);
    assert_eq!(reply.0, PATH_NOT_FOUND, "a path with no NUL");
}

#[test]
fn a_symbolic_link_out_of_the_root_is_not_followed() {
    let s = scratch("symlink");
    let target = s.root().parent().expect("base").join("outside.txt");
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&target, s.root().join("link"));
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&target, s.root().join("link"));
    // Creating a link needs a privilege some Windows hosts lack; then there is nothing to test.
    if linked.is_err() {
        return;
    }
    assert_eq!(entry_type(&s, b"link").0, PATH_NOT_FOUND);
    let message = s.request(OPEN_FILE, &1u32.to_le_bytes(), Some(b"link"), 0);
    let ((result, handles, _), released) =
        call(&mut *s.file_system(), &mut FakeSessions::default(), message);
    assert_eq!((result, handles, released), (PATH_NOT_FOUND, vec![], true));
}

#[test]
fn an_opened_file_reads_and_reports_its_size() {
    let s = scratch("read");
    let mut sessions = FakeSessions::default();
    let message = s.request(OPEN_FILE, &1u32.to_le_bytes(), Some(b"/data.bin"), 0);
    let ((result, handles, _), released) = call(&mut *s.file_system(), &mut sessions, message);
    assert_eq!((result, handles, released), (0, vec![0x100], false));
    let mut file = sessions.objects.remove(0);
    assert_eq!(file.name(), "fsp-srv.IFile");

    let read = |file: &mut dyn SessionObject, offset: u64, size: u64, out: u64| {
        let mut params = vec![0u8; 8];
        params.extend(offset.to_le_bytes());
        params.extend(size.to_le_bytes());
        let message = s.request(READ, &params, None, out);
        let ((result, _, count), _) = call(file, &mut FakeSessions::default(), message);
        (result, count)
    };
    assert_eq!(read(&mut *file, 10, 20, 64), (0, 20));
    let mut data = [0u8; 20];
    s.mem
        .read_bytes(GuestVa::new(OUT_AT), &mut data)
        .expect("read back");
    assert_eq!(data.to_vec(), (10u8..30).collect::<Vec<_>>());
    assert_eq!(read(&mut *file, 90, 20, 64), (0, 10), "short at the end");
    assert_eq!(
        read(&mut *file, 200, 20, 64),
        (0, 0),
        "nothing past the end"
    );
    assert_eq!(read(&mut *file, 0, 1000, 16), (0, 16), "at most the buffer");

    let message = s.request(GET_SIZE, &[], None, 0);
    let ((result, _, size), _) = call(&mut *file, &mut FakeSessions::default(), message);
    assert_eq!((result, size), (0, 100));
}

#[test]
fn opening_a_missing_file_releases_its_session() {
    let s = scratch("missing");
    let mut sessions = FakeSessions::default();
    let message = s.request(OPEN_FILE, &1u32.to_le_bytes(), Some(b"nope.bin"), 0);
    let ((result, handles, _), released) = call(&mut *s.file_system(), &mut sessions, message);
    assert_eq!((result, handles, released), (PATH_NOT_FOUND, vec![], true));
    let message = s.request(OPEN_FILE, &1u32.to_le_bytes(), Some(b"dir"), 0);
    let ((result, _, _), released) = call(&mut *s.file_system(), &mut sessions, message);
    assert_eq!(
        (result, released),
        (PATH_NOT_FOUND, true),
        "a directory is no file"
    );
}

#[test]
fn a_read_only_sd_card_refuses_writing_modes_without_touching_the_host() {
    let s = scratch("read-only");
    for mode in [2u32, 3, 4, 7] {
        let mut sessions = FakeSessions::default();
        let message = s.request(OPEN_FILE, &mode.to_le_bytes(), Some(b"data.bin"), 0);
        let ((result, handles, _), _) = call(&mut *s.file_system(), &mut sessions, message);
        assert_eq!((result, handles), (PATH_NOT_FOUND, vec![]), "mode {mode}");
        assert!(sessions.objects.is_empty() && sessions.deferred.is_empty());
    }
    let data = fs::read(s.root().join("data.bin")).expect("read data");
    assert_eq!(data, (0u8..100).collect::<Vec<_>>(), "unchanged");
}

#[test]
fn unknown_modes_and_directories_are_unknown_commands_for_now() {
    let s = scratch("unknown");
    let mut sessions = FakeSessions {
        run_deferred: true,
        ..FakeSessions::default()
    };
    let mut message = s.request(OPEN_FILE, &8u32.to_le_bytes(), Some(b"data.bin"), 0);
    assert_eq!(
        s.file_system().handle_request(&mut sessions, &mut message),
        Err(UnknownCommand { command: OPEN_FILE })
    );
    let mut message = s.request(9, &[], Some(b"dir"), 0);
    assert_eq!(
        s.file_system().handle_request(&mut sessions, &mut message),
        Err(UnknownCommand { command: 9 }),
        "OpenDirectory comes with step 3b"
    );
    assert!(sessions.objects.is_empty() && sessions.deferred.is_empty());
}

#[test]
fn without_a_service_io_thread_nothing_touches_the_host() {
    let s = scratch("no-io");
    let mut sessions = FakeSessions::default();
    let mut message = s.request(GET_ENTRY_TYPE, &[], Some(b"data.bin"), 0);
    assert_eq!(
        s.file_system().handle_request(&mut sessions, &mut message),
        Err(UnknownCommand {
            command: GET_ENTRY_TYPE
        })
    );
    assert_eq!(sessions.deferred, []);
}

#[test]
fn fsp_srv_opens_the_sd_card_and_logs_set_current_process() {
    let s = scratch("fsp-srv");
    let mut fsp = FspSrv(Arc::clone(&s.shared));
    let mut sessions = FakeSessions::default();
    let message = s.request(1, &0u64.to_le_bytes(), None, 0);
    let ((result, _, _), _) = call(&mut fsp, &mut sessions, message);
    assert_eq!(result, 0);
    assert_eq!(sessions.stubs, [s.shared.set_current_process]);
    let message = s.request(18, &[], None, 0);
    let ((result, handles, _), _) = call(&mut fsp, &mut sessions, message);
    assert_eq!((result, handles), (0, vec![0x100]));
    assert_eq!(sessions.objects[0].name(), "fsp-srv.IFileSystem");
}

#[test]
fn the_registry_serves_fsp_srv_from_the_sd_card_and_rejects_a_file_as_root() {
    let s = scratch("registry");
    let mut trace = Trace::new(16);
    let sd = SdCard::new(s.root(), Arc::clone(&s.mem)).expect("root");
    let registry = crate::stubs::b2_services(&mut trace, Some(sd)).expect("services");
    for name in ["fsp-srv", "set", "time:u", "hid"] {
        assert!(
            registry.find(service_name(name).expect("name")).is_some(),
            "{name}"
        );
    }
    let mut fsp = (registry
        .find(service_name("fsp-srv").expect("name"))
        .expect("registered"))();
    let message = s.request(18, &[], None, 0);
    let ((result, _, _), _) = call(&mut *fsp, &mut FakeSessions::default(), message);
    assert_eq!(result, 0, "the real fsp-srv answers OpenSdCardFileSystem");
    assert!(SdCard::new(&s.root().join("data.bin"), Arc::clone(&s.mem)).is_err());
}

#[test]
fn a_read_larger_than_one_chunk_arrives_whole() {
    const BIG_AT: u64 = 0x10_0000;
    const LEN: u64 = 150_000;
    let s = scratch("big-read");
    let content: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    fs::write(s.root().join("big.bin"), &content).expect("write big");
    s.mem
        .map(
            GuestVa::new(BIG_AT),
            0x4_0000,
            Permission::READ | Permission::WRITE,
        )
        .expect("map");
    let mut sessions = FakeSessions::default();
    let message = s.request(OPEN_FILE, &1u32.to_le_bytes(), Some(b"big.bin"), 0);
    call(&mut *s.file_system(), &mut sessions, message);
    let mut file = sessions.objects.remove(0);

    let mut params = vec![0u8; 8];
    params.extend(1000u64.to_le_bytes());
    params.extend(LEN.to_le_bytes());
    let mut message = s.request(READ, &params, None, 0);
    let parts = Parts {
        tag: TYPE_REQUEST_OLD,
        receives: &[Map {
            address: BIG_AT,
            size: LEN,
            attribute: 0,
        }],
        ..Parts::default()
    };
    let header = Header {
        magic: IN_MAGIC,
        value: READ,
        ..Header::default()
    };
    cmif::write(&mut message, &parts, &header, &params).expect("fits");
    let ((result, _, count), _) = call(&mut *file, &mut FakeSessions::default(), message);
    assert_eq!((result, count), (0, LEN));
    let mut data = vec![0u8; 150_000];
    s.mem
        .read_bytes(GuestVa::new(BIG_AT), &mut data)
        .expect("read back");
    assert_eq!(data, content[1000..151_000]);
}

/// Opens `name` on `s` with `mode` and returns the `IFile`.
fn open(s: &Scratch, name: &[u8], mode: u32) -> Box<dyn SessionObject> {
    let mut sessions = FakeSessions::default();
    let message = s.request(OPEN_FILE, &mode.to_le_bytes(), Some(name), 0);
    let ((result, _, _), _) = call(&mut *s.file_system(), &mut sessions, message);
    assert_eq!(result, 0, "open");
    sessions.objects.remove(0)
}

/// Writes `data` at `offset` through `file` with `option`, from guest memory at `OUT_AT`.
fn write(s: &Scratch, file: &mut dyn SessionObject, offset: u64, data: &[u8], option: u32) -> u32 {
    s.mem
        .write_bytes(GuestVa::new(OUT_AT), data)
        .expect("stage the data");
    let mut params = option.to_le_bytes().to_vec();
    params.extend([0; 4]);
    params.extend(offset.to_le_bytes());
    params.extend((data.len() as u64).to_le_bytes());
    let parts = Parts {
        tag: TYPE_REQUEST_OLD,
        sends: &[Map {
            address: OUT_AT,
            size: data.len() as u64,
            attribute: 0,
        }],
        ..Parts::default()
    };
    let header = Header {
        magic: IN_MAGIC,
        value: WRITE,
        ..Header::default()
    };
    let mut message = [0; MESSAGE_SIZE];
    cmif::write(&mut message, &parts, &header, &params).expect("fits");
    call(file, &mut FakeSessions::default(), message).0.0
}

fn simple(s: &Scratch, file: &mut dyn SessionObject, command: u32, params: &[u8]) -> u32 {
    let message = s.request(command, params, None, 0);
    call(file, &mut FakeSessions::default(), message).0.0
}

#[test]
fn a_writable_sd_card_writes_inside_the_file() {
    let s = scratch_with("write", true);
    let mut file = open(&s, b"data.bin", 3);
    assert_eq!(write(&s, &mut *file, 10, b"hello", 0), 0);
    assert_eq!(write(&s, &mut *file, 0, b"!", 1), 0, "with a flush");
    assert_eq!(simple(&s, &mut *file, FLUSH, &[]), 0);
    let data = fs::read(s.root().join("data.bin")).expect("read data");
    assert_eq!(
        (&data[10..15], data[0], data.len()),
        (&b"hello"[..], b'!', 100)
    );
}

#[test]
fn a_write_past_the_end_needs_the_append_bit() {
    let s = scratch_with("append", true);
    let mut file = open(&s, b"data.bin", 2);
    assert_eq!(write(&s, &mut *file, 98, b"abcd", 0), OUT_OF_RANGE);
    assert_eq!(
        fs::read(s.root().join("data.bin")).expect("read").len(),
        100,
        "nothing written"
    );
    let mut file = open(&s, b"data.bin", 2 | 4);
    assert_eq!(write(&s, &mut *file, 98, b"abcd", 0), 0);
    let data = fs::read(s.root().join("data.bin")).expect("read");
    assert_eq!((data.len(), &data[98..]), (102, &b"abcd"[..]));
}

#[test]
fn set_size_truncates_and_extends_a_file_opened_for_writing() {
    let s = scratch_with("set-size", true);
    let mut file = open(&s, b"data.bin", 2);
    assert_eq!(simple(&s, &mut *file, SET_SIZE, &40u64.to_le_bytes()), 0);
    assert_eq!(
        fs::metadata(s.root().join("data.bin")).expect("meta").len(),
        40
    );
    assert_eq!(simple(&s, &mut *file, SET_SIZE, &60u64.to_le_bytes()), 0);
    let data = fs::read(s.root().join("data.bin")).expect("read");
    assert_eq!((data.len(), data[39], data[40]), (60, 39, 0));
}

#[test]
fn the_open_mode_limits_what_a_file_may_do() {
    let s = scratch_with("modes", true);
    let mut reader = open(&s, b"data.bin", 1);
    assert_eq!(write(&s, &mut *reader, 0, b"x", 0), PATH_NOT_FOUND);
    assert_eq!(
        simple(&s, &mut *reader, SET_SIZE, &1u64.to_le_bytes()),
        PATH_NOT_FOUND
    );
    assert_eq!(simple(&s, &mut *reader, FLUSH, &[]), 0, "nothing to flush");
    let mut writer = open(&s, b"data.bin", 2);
    let mut params = vec![0u8; 8];
    params.extend(0u64.to_le_bytes());
    params.extend(4u64.to_le_bytes());
    let message = s.request(READ, &params, None, 16);
    let ((result, _, _), _) = call(&mut *writer, &mut FakeSessions::default(), message);
    assert_eq!(result, PATH_NOT_FOUND, "a write-only file cannot be read");
    let data = fs::read(s.root().join("data.bin")).expect("read");
    assert_eq!(data, (0u8..100).collect::<Vec<_>>(), "unchanged");
}

/// A `Write` of `size` bytes at file offset 0 from guest memory at `at`, a send buffer of
/// `buffer` bytes.
fn write_from(file: &mut dyn SessionObject, at: u64, buffer: u64, size: u64) -> u32 {
    let mut params = vec![0u8; 16];
    params.extend(size.to_le_bytes());
    let parts = Parts {
        tag: TYPE_REQUEST_OLD,
        sends: &[Map {
            address: at,
            size: buffer,
            attribute: 0,
        }],
        ..Parts::default()
    };
    let header = Header {
        magic: IN_MAGIC,
        value: WRITE,
        ..Header::default()
    };
    let mut message = [0; MESSAGE_SIZE];
    cmif::write(&mut message, &parts, &header, &params).expect("fits");
    call(file, &mut FakeSessions::default(), message).0.0
}

#[test]
fn a_bad_source_buffer_fails_before_the_file_is_touched() {
    let s = scratch_with("bad-buffer", true);
    let mut file = open(&s, b"data.bin", 2);
    let unmapped = 0x50_0000;
    assert_eq!(write_from(&mut *file, unmapped, 8, 8), super::BAD_BUFFER);
    // Starts in the mapped page at OUT_AT and runs into the unmapped one after it.
    let straddling = OUT_AT + GUEST_PAGE_SIZE - 4;
    assert_eq!(write_from(&mut *file, straddling, 8, 8), super::BAD_BUFFER);
    assert_eq!(
        write_from(&mut *file, OUT_AT, 4, 8),
        super::BAD_BUFFER,
        "more than the buffer holds"
    );
    let data = fs::read(s.root().join("data.bin")).expect("read");
    assert_eq!(data, (0u8..100).collect::<Vec<_>>(), "unchanged");
}

#[test]
fn a_read_into_an_unmapped_buffer_is_a_bad_buffer() {
    let s = scratch("bad-read");
    let mut file = open(&s, b"data.bin", 1);
    let mut params = vec![0u8; 16];
    params.extend(8u64.to_le_bytes());
    let parts = Parts {
        tag: TYPE_REQUEST_OLD,
        receives: &[Map {
            address: 0x50_0000,
            size: 8,
            attribute: 0,
        }],
        ..Parts::default()
    };
    let header = Header {
        magic: IN_MAGIC,
        value: READ,
        ..Header::default()
    };
    let mut message = [0; MESSAGE_SIZE];
    cmif::write(&mut message, &parts, &header, &params).expect("fits");
    let ((result, _, _), _) = call(&mut *file, &mut FakeSessions::default(), message);
    assert_eq!(result, super::BAD_BUFFER);
}
