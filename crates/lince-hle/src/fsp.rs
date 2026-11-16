// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `fsp-srv` on a host directory (T-064), from
//! docs/research/fsp-srv-filesystem-protocol.md (Switch 1 knowledge; assumed unchanged on
//! Switch 2, doc 08).
//!
//! `OpenSdCardFileSystem` hands out an `IFileSystem` rooted at a directory the user chose
//! ([`SdCard`]). It answers `GetEntryType`, `OpenFile` and `OpenDirectory`; the `IFile` answers
//! `Read`, `Write`, `Flush`, `SetSize` and `GetSize`, the `IDirectory` `Read` and
//! `GetEntryCount`. Opening for writing or appending needs an SD card the user made writable
//! ([`SdCard::writable`]); otherwise it fails.
//!
//! # Threads
//! Every host file-system access runs on the service-I/O thread through a deferred reply
//! ([`Sessions::defer`]); on the guest core the objects only read the request, read the path
//! from guest memory and check it lexically (invariant 3). If no service-I/O thread runs, the
//! request is an unknown command rather than I/O on the guest core.
//!
//! # Failures
//! A request that fails on the service-I/O thread is recorded in `lince-trace` as a
//! `HostIoFailed` event naming the object, the command and the class of the error (T-065),
//! once per triple. The guest's result does not depend on the class.
//!
//! # Confinement
//! A guest path never reaches outside the root: on the guest core it is split on `/` and any
//! `..` component, backslash or colon rejects it; on the service-I/O thread the joined path is
//! canonicalized and must still start with the canonical root, which stops symbolic links
//! that point outside. Each command opens the path again after that check; a link swapped in
//! between by another host process could still escape. The guest cannot create links (no
//! command makes one), so only the user's own host processes could, which Lince does not
//! defend against.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use lince_common::GuestVa;
use lince_kernel::{
    DeferredReply, IPC_MESSAGE_SIZE, IpcMessage, SessionObject, Sessions, UnknownCommand,
};
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE};
use lince_trace::{HostIoError, NameId, Recorder, ThreadRole, Trace, TraceError};
use parking_lot::Mutex;

use crate::hipc::Message;
use crate::sm::{OUT_OF_SESSIONS, ServiceRegistry};
use crate::stubs::{read_request, write_reply};

const SUCCESS: u32 = 0;
/// `PathNotFound`, module 2 description 1 (research note, `Error_codes` row).
// GUESS: also the result for a malformed request, a path outside the root and a host I/O
// failure; the note gives no per-command or per-case result (§4).
pub const PATH_NOT_FOUND: u32 = 0x202;
/// The path buffer: 0x301 bytes, NUL included (note §2).
const PATH_SIZE: usize = 0x301;
/// `OpenFile` mode bits (note §3): read, write, append (a write may grow the file).
const MODE_READ: u32 = 1;
const MODE_WRITE: u32 = 2;
const MODE_APPEND: u32 = 4;
/// `Write` option bit 0: flush after writing (note §3).
const WRITE_OPTION_FLUSH: u32 = 1;
/// A write past the end of a file opened without the append bit (note §3: module 2,
/// description 0x3839).
pub const OUT_OF_RANGE: u32 = 0x0030_7202;
/// A data buffer the guest cannot read or write (`Read`/`Write`), or smaller than the size
/// asked for (`Write`).
// GUESS: the kernel's invalid-address result; the note gives no `fsp-srv` result for a bad
// buffer (§4).
pub const BAD_BUFFER: u32 = 0xE601;
/// Bytes a `Read` or `Write` copies through the host per step.
const IO_CHUNK: usize = 64 * 1024;
/// `GetEntryType` values (note §3).
const ENTRY_DIRECTORY: u32 = 0;
const ENTRY_FILE: u32 = 1;
/// `OpenDirectory` filter bits (note §3): list directories, list files, report every file
/// size as 0.
const LIST_DIRECTORIES: u32 = 1;
const LIST_FILES: u32 = 2;
const NO_FILE_SIZE: u32 = 0x8000_0000;
/// A `DirectoryEntry`: name at 0 (0x301 bytes, NUL included), type `s8` at 0x304, size `u64`
/// at 0x308 (note §3; 0x310 is the naturally aligned size libnx implies).
const ENTRY_SIZE: usize = 0x310;
const ENTRY_TYPE_AT: usize = 0x304;
const ENTRY_SIZE_AT: usize = 0x308;

const SET_CURRENT_PROCESS: u32 = 1;
const OPEN_SD_CARD_FILE_SYSTEM: u32 = 18;
const GET_ENTRY_TYPE: u32 = 7;
const OPEN_FILE: u32 = 8;
const OPEN_DIRECTORY: u32 = 9;
const READ_DIRECTORY: u32 = 0;
const GET_ENTRY_COUNT: u32 = 1;
const READ: u32 = 0;
const WRITE: u32 = 1;
const FLUSH: u32 = 2;
const SET_SIZE: u32 = 3;
const GET_SIZE: u32 = 4;

/// The host directory the guest's SD card is. Read-only unless [`SdCard::writable`].
pub struct SdCard {
    root: PathBuf,
    mem: Arc<AddressSpace>,
    writable: bool,
}

impl SdCard {
    /// Serves `root`, an existing directory, to the guest whose memory is `mem`. Setup only:
    /// it reads the host file system to canonicalize `root`.
    ///
    /// # Errors
    /// `root` does not exist, cannot be read or is not a directory.
    pub fn new(root: &Path, mem: Arc<AddressSpace>) -> io::Result<Self> {
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is not a directory", root.display()),
            ));
        }
        Ok(Self {
            root,
            mem,
            writable: false,
        })
    }

    /// Lets the guest change the files under the root: open them for writing or appending,
    /// write, flush and resize them. Off unless the user asks (maintainer, 2026-11-16).
    #[must_use]
    pub fn writable(mut self) -> Self {
        self.writable = true;
        self
    }
}

struct Shared {
    root: PathBuf,
    mem: Arc<AddressSpace>,
    writable: bool,
    fsp: NameId,
    file_system: NameId,
    file: NameId,
    directory: NameId,
    set_current_process: NameId,
    /// Records why a request failed on the host. Only the service-I/O thread locks it, so it
    /// is never contended and the guest core never waits for it.
    failures: Mutex<Recorder>,
}

/// Registers `fsp-srv` serving `sd`. `false` if a service of that name is already there.
pub(crate) fn register(
    registry: &mut ServiceRegistry,
    trace: &mut Trace,
    sd: SdCard,
) -> Result<bool, TraceError> {
    let shared = Arc::new(Shared {
        root: sd.root,
        mem: sd.mem,
        writable: sd.writable,
        fsp: trace.name("fsp-srv")?,
        file_system: trace.name("fsp-srv.IFileSystem")?,
        file: trace.name("fsp-srv.IFile")?,
        directory: trace.name("fsp-srv.IDirectory")?,
        set_current_process: trace.name("fsp-srv.SetCurrentProcess")?,
        failures: Mutex::new(trace.recorder(ThreadRole::Other)),
    });
    Ok(registry.register(
        "fsp-srv",
        Box::new(move || Box::new(FspSrv(Arc::clone(&shared)))),
    ))
}

/// A session to `fsp-srv`.
struct FspSrv(Arc<Shared>);

impl SessionObject for FspSrv {
    fn name(&self) -> &'static str {
        "fsp-srv"
    }

    fn trace_name(&self) -> NameId {
        self.0.fsp
    }

    fn handle_request(
        &mut self,
        sessions: &mut dyn Sessions,
        message: &mut IpcMessage,
    ) -> Result<(), UnknownCommand> {
        let (_, command, _) = read_request(message)?;
        let unknown = UnknownCommand { command };
        let reply = match command {
            SET_CURRENT_PROCESS => {
                sessions.log_stub(self.0.set_current_process);
                write_reply(message, SUCCESS, None, &[])
            }
            OPEN_SD_CARD_FILE_SYSTEM => {
                let object = Box::new(FileSystem(Arc::clone(&self.0)));
                match sessions.open_session(object) {
                    Some(handle) => write_reply(message, SUCCESS, Some(handle), &[]),
                    None => write_reply(message, OUT_OF_SESSIONS, None, &[]),
                }
            }
            _ => return Err(unknown),
        };
        reply.map(|_| ()).map_err(|_| unknown)
    }
}

/// An `IFileSystem` on the root.
struct FileSystem(Arc<Shared>);

impl SessionObject for FileSystem {
    fn name(&self) -> &'static str {
        "fsp-srv.IFileSystem"
    }

    fn trace_name(&self) -> NameId {
        self.0.file_system
    }

    fn handle_request(
        &mut self,
        sessions: &mut dyn Sessions,
        message: &mut IpcMessage,
    ) -> Result<(), UnknownCommand> {
        let (msg, command, params) = read_request(message)?;
        let unknown = UnknownCommand { command };
        if ![GET_ENTRY_TYPE, OPEN_FILE, OPEN_DIRECTORY].contains(&command) {
            return Err(unknown);
        }
        let mode = word(params, 0).unwrap_or(0);
        if command == OPEN_FILE && mode & !(MODE_READ | MODE_WRITE | MODE_APPEND) != 0 {
            return Err(unknown);
        }
        // The `OpenDirectory` filter is a u64; only the three bits the note names are known.
        let filter = dword(params, 0).unwrap_or(0);
        let known = u64::from(LIST_DIRECTORIES | LIST_FILES | NO_FILE_SIZE);
        if command == OPEN_DIRECTORY && filter & !known != 0 {
            return Err(unknown);
        }
        if command == OPEN_FILE && mode & (MODE_WRITE | MODE_APPEND) != 0 && !self.0.writable {
            // GUESS: the note gives no result for a write-protected file system (§4).
            return reply_now(message, PATH_NOT_FOUND, unknown);
        }
        let Some(path) = guest_path(&self.0, &msg) else {
            return reply_now(message, PATH_NOT_FOUND, unknown);
        };
        let op = Op::new(&self.0, self.0.file_system, command);
        if command == GET_ENTRY_TYPE {
            let work = Box::new(move || entry_type(&op, &path));
            return defer(sessions, "fsp-srv.GetEntryType", work, unknown);
        }
        if command == OPEN_DIRECTORY {
            let directory = Box::new(OpenedDirectory {
                shared: Arc::clone(&self.0),
                path: Arc::new(path.clone()),
                // Only known bits remain, all below 2^32.
                filter: u32::try_from(filter & known).unwrap_or(0),
                cursor: Arc::new(AtomicU64::new(0)),
            });
            let Some(handle) = sessions.open_session(directory) else {
                return reply_now(message, OUT_OF_SESSIONS, unknown);
            };
            let work = Box::new(move || open_directory(&op, &path, handle));
            return defer(sessions, "fsp-srv.OpenDirectory", work, unknown);
        }
        let file = Box::new(OpenedFile {
            shared: Arc::clone(&self.0),
            path: Arc::new(path.clone()),
            mode,
        });
        let Some(handle) = sessions.open_session(file) else {
            return reply_now(message, OUT_OF_SESSIONS, unknown);
        };
        let work = Box::new(move || open_file(&op, &path, handle));
        defer(sessions, "fsp-srv.OpenFile", work, unknown)
    }
}

/// An opened `IFile`. It keeps the path and the `OpenFile` mode, not a host handle: each
/// command opens the file again on the service-I/O thread.
struct OpenedFile {
    shared: Arc<Shared>,
    path: Arc<PathBuf>,
    mode: u32,
}

impl SessionObject for OpenedFile {
    fn name(&self) -> &'static str {
        "fsp-srv.IFile"
    }

    fn trace_name(&self) -> NameId {
        self.shared.file
    }

    fn handle_request(
        &mut self,
        sessions: &mut dyn Sessions,
        message: &mut IpcMessage,
    ) -> Result<(), UnknownCommand> {
        let (msg, command, params) = read_request(message)?;
        let unknown = UnknownCommand { command };
        let op = Op::new(&self.shared, self.shared.file, command);
        let path = Arc::clone(&self.path);
        let may_write = self.mode & (MODE_WRITE | MODE_APPEND) != 0;
        // GUESS: a command the open mode does not allow fails like a missing path; the note
        // gives no result for it (§4).
        match command {
            GET_SIZE => {
                let work = Box::new(move || file_size(&op, &path));
                defer(sessions, "fsp-srv.IFile.GetSize", work, unknown)
            }
            FLUSH => {
                let work = Box::new(move || flush_file(&op, &path, may_write));
                defer(sessions, "fsp-srv.IFile.Flush", work, unknown)
            }
            SET_SIZE => {
                let Some(size) = dword(params, 0).filter(|_| may_write) else {
                    return reply_now(message, PATH_NOT_FOUND, unknown);
                };
                let work = Box::new(move || set_size(&op, &path, size));
                defer(sessions, "fsp-srv.IFile.SetSize", work, unknown)
            }
            READ | WRITE => {
                // Option u32, padding u32, offset s64, size u64; the data is in the first
                // receive (Read) or send (Write) buffer (note §3).
                let buffer = if command == READ {
                    msg.receives().next()
                } else {
                    msg.sends().next()
                };
                let allowed = if command == READ {
                    self.mode & MODE_READ != 0
                } else {
                    may_write
                };
                let (Some(option), Some(offset), Some(size), Some(buffer), true) = (
                    word(params, 0),
                    dword(params, 8),
                    dword(params, 16),
                    buffer,
                    allowed,
                ) else {
                    return reply_now(message, PATH_NOT_FOUND, unknown);
                };
                if command == WRITE && size > buffer.size {
                    // Writing less than asked while reporting success would lose data.
                    return reply_now(message, BAD_BUFFER, unknown);
                }
                // GUESS: a `Read` larger than its buffer reads only what fits (note §4).
                let range = Transfer {
                    offset,
                    len: size.min(buffer.size),
                    at: GuestVa::new(buffer.address),
                };
                if command == READ {
                    let work = Box::new(move || read_file(&op, &path, range));
                    return defer(sessions, "fsp-srv.IFile.Read", work, unknown);
                }
                let write = WriteFlags {
                    append: self.mode & MODE_APPEND != 0,
                    flush: option & WRITE_OPTION_FLUSH != 0,
                };
                let work = Box::new(move || write_file(&op, &path, range, write));
                defer(sessions, "fsp-srv.IFile.Write", work, unknown)
            }
            _ => Err(unknown),
        }
    }
}

/// An opened `IDirectory`. Like [`OpenedFile`] it keeps no host handle: each command lists the
/// directory again, sorted by name so that `cursor`, the entries already read, stays
/// meaningful between commands.
struct OpenedDirectory {
    shared: Arc<Shared>,
    path: Arc<PathBuf>,
    filter: u32,
    /// Only the service-I/O thread reads or writes it, one deferred request at a time, so a
    /// relaxed load and store never race.
    cursor: Arc<AtomicU64>,
}

impl SessionObject for OpenedDirectory {
    fn name(&self) -> &'static str {
        "fsp-srv.IDirectory"
    }

    fn trace_name(&self) -> NameId {
        self.shared.directory
    }

    fn handle_request(
        &mut self,
        sessions: &mut dyn Sessions,
        message: &mut IpcMessage,
    ) -> Result<(), UnknownCommand> {
        let (msg, command, _) = read_request(message)?;
        let unknown = UnknownCommand { command };
        let listing = Listing {
            op: Op::new(&self.shared, self.shared.directory, command),
            path: Arc::clone(&self.path),
            filter: self.filter,
        };
        match command {
            GET_ENTRY_COUNT => {
                let work = Box::new(move || entry_count(&listing));
                defer(sessions, "fsp-srv.IDirectory.GetEntryCount", work, unknown)
            }
            READ_DIRECTORY => {
                // The entries go to the first receive buffer (note §3).
                let Some(buffer) = msg.receives().next() else {
                    return reply_now(message, BAD_BUFFER, unknown);
                };
                let cursor = Arc::clone(&self.cursor);
                let room = buffer.size / ENTRY_SIZE as u64;
                let at = GuestVa::new(buffer.address);
                let work = Box::new(move || read_directory(&listing, &cursor, room, at));
                defer(sessions, "fsp-srv.IDirectory.Read", work, unknown)
            }
            _ => Err(unknown),
        }
    }
}

/// The object and command a deferred request came from, so the service-I/O thread can record
/// why it failed (T-065).
struct Op {
    shared: Arc<Shared>,
    object: NameId,
    command: u16,
}

impl Op {
    fn new(shared: &Arc<Shared>, object: NameId, command: u32) -> Self {
        Self {
            shared: Arc::clone(shared),
            object,
            // Every command an object defers is below 0x100.
            command: u16::try_from(command).unwrap_or(u16::MAX),
        }
    }

    fn root(&self) -> &Path {
        &self.shared.root
    }

    fn mem(&self) -> &AddressSpace {
        &self.shared.mem
    }

    /// Records `error` once per object, command and class, and builds the reply `code`: the
    /// guest gets the same result whatever the class.
    fn fail(&self, error: HostIoError, code: u32) -> IpcMessage {
        self.shared
            .failures
            .lock()
            .host_io_failed(self.object, self.command, error);
        reply(code, None, &[])
    }

    /// A host error, recorded by class; the guest sees `PathNotFound`.
    // GUESS: every host failure is `PathNotFound` to the guest; the note gives no
    // per-cause result (§4).
    fn host(&self, error: &io::Error) -> IpcMessage {
        self.fail(classify(error), PATH_NOT_FOUND)
    }
}

/// The class of each host error kind [`classify`] tells apart. `io::ErrorKind` grows with
/// Rust releases; a kind not listed here is `Other`.
const CLASSES: [(io::ErrorKind, HostIoError); 5] = [
    (io::ErrorKind::NotFound, HostIoError::NotFound),
    (
        io::ErrorKind::PermissionDenied,
        HostIoError::PermissionDenied,
    ),
    (io::ErrorKind::NotADirectory, HostIoError::WrongType),
    (io::ErrorKind::IsADirectory, HostIoError::WrongType),
    (io::ErrorKind::StorageFull, HostIoError::StorageFull),
];

/// The class of a host error, for the trace; never its text or path (PAT-07).
fn classify(error: &io::Error) -> HostIoError {
    if matches!(error.get_ref(), Some(inner) if inner.is::<OutsideRoot>()) {
        return HostIoError::OutsideRoot;
    }
    CLASSES
        .iter()
        .find(|(kind, _)| *kind == error.kind())
        .map_or(HostIoError::Other, |&(_, class)| class)
}

/// What an `IDirectory` lists: `path` under the root, filtered by the `OpenDirectory` bits.
struct Listing {
    op: Op,
    path: Arc<PathBuf>,
    filter: u32,
}

/// One listed entry: its name, whether it is a directory, and its size (0 for a directory).
struct Entry {
    name: String,
    directory: bool,
    size: u64,
}

impl Listing {
    /// The entries the filter keeps, sorted by name. Service-I/O thread only. An entry whose
    /// name is not UTF-8 or does not fit the 0x301-byte field, or that is a symbolic link out
    /// of the root, is left out.
    // GUESS: the note gives no rule for names a guest could not hold; leaving them out keeps
    // every listed name one `OpenFile` accepts.
    fn entries(&self) -> io::Result<Vec<Entry>> {
        let dir = confined(self.op.root(), &self.path)?;
        let mut entries = Vec::new();
        for item in dir.read_dir()? {
            let item = item?;
            let Ok(name) = item.file_name().into_string() else {
                continue;
            };
            let Ok(meta) = confined(self.op.root(), &item.path()).and_then(|real| real.metadata())
            else {
                continue;
            };
            let directory = meta.is_dir();
            let wanted = if directory {
                LIST_DIRECTORIES
            } else {
                LIST_FILES
            };
            if name.len() >= PATH_SIZE || self.filter & wanted == 0 {
                continue;
            }
            let size = if directory || self.filter & NO_FILE_SIZE != 0 {
                0
            } else {
                meta.len()
            };
            entries.push(Entry {
                name,
                directory,
                size,
            });
        }
        entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }
}

/// `OpenDirectory` on the service-I/O thread: hands out `handle` if `path` is a directory;
/// otherwise the kernel closes that session, which the guest never learns.
fn open_directory(op: &Op, path: &Path, handle: u32) -> DeferredReply {
    match confined(op.root(), path).and_then(|path| path.metadata()) {
        Ok(meta) if meta.is_dir() => DeferredReply::Keep(reply(SUCCESS, Some(handle), &[])),
        Ok(_) => DeferredReply::Release(op.fail(HostIoError::WrongType, PATH_NOT_FOUND)),
        Err(error) => DeferredReply::Release(op.host(&error)),
    }
}

/// `IDirectory` `GetEntryCount` on the service-I/O thread: every entry the filter keeps.
// GUESS: the total, not the entries left to read (libnx calls it the total entry count).
fn entry_count(listing: &Listing) -> DeferredReply {
    DeferredReply::Keep(match listing.entries() {
        Ok(entries) => reply(SUCCESS, None, &(entries.len() as u64).to_le_bytes()),
        Err(error) => listing.op.host(&error),
    })
}

/// `IDirectory` `Read` on the service-I/O thread: the next entries after `cursor`, at most
/// `room` of them, written to guest memory at `at`; replies with the count, 0 when none
/// remain (note §3). The cursor moves only when every entry reached the guest.
fn read_directory(listing: &Listing, cursor: &AtomicU64, room: u64, at: GuestVa) -> DeferredReply {
    let entries = match listing.entries() {
        Ok(entries) => entries,
        Err(error) => return DeferredReply::Keep(listing.op.host(&error)),
    };
    let start = cursor.load(Ordering::Relaxed);
    let mut count = 0u64;
    let mut raw = [0u8; ENTRY_SIZE];
    for entry in entries
        .iter()
        .skip(usize::try_from(start).unwrap_or(usize::MAX))
        .take(usize::try_from(room).unwrap_or(usize::MAX))
    {
        raw.fill(0);
        raw[..entry.name.len()].copy_from_slice(entry.name.as_bytes());
        raw[ENTRY_TYPE_AT] = u8::from(!entry.directory);
        raw[ENTRY_SIZE_AT..].copy_from_slice(&entry.size.to_le_bytes());
        let to = at.wrapping_add(count * ENTRY_SIZE as u64);
        if listing.op.mem().write_bytes(to, &raw).is_err() {
            return DeferredReply::Keep(listing.op.fail(HostIoError::GuestMemory, BAD_BUFFER));
        }
        count += 1;
    }
    cursor.store(start + count, Ordering::Relaxed);
    DeferredReply::Keep(reply(SUCCESS, None, &count.to_le_bytes()))
}

/// `GetEntryType` on the service-I/O thread: 0 for a directory, 1 for a file.
fn entry_type(op: &Op, path: &Path) -> DeferredReply {
    DeferredReply::Keep(
        match confined(op.root(), path).and_then(|path| path.metadata()) {
            Ok(meta) if meta.is_dir() => reply(SUCCESS, None, &ENTRY_DIRECTORY.to_le_bytes()),
            Ok(_) => reply(SUCCESS, None, &ENTRY_FILE.to_le_bytes()),
            Err(error) => op.host(&error),
        },
    )
}

/// `OpenFile` on the service-I/O thread: hands out `handle` if `path` is a file; otherwise the
/// kernel closes that session, which the guest never learns.
fn open_file(op: &Op, path: &Path, handle: u32) -> DeferredReply {
    match confined(op.root(), path).and_then(|path| path.metadata()) {
        Ok(meta) if meta.is_file() => DeferredReply::Keep(reply(SUCCESS, Some(handle), &[])),
        Ok(_) => DeferredReply::Release(op.fail(HostIoError::WrongType, PATH_NOT_FOUND)),
        Err(error) => DeferredReply::Release(op.host(&error)),
    }
}

/// `IFile` `GetSize` on the service-I/O thread.
fn file_size(op: &Op, path: &Path) -> DeferredReply {
    DeferredReply::Keep(
        match confined(op.root(), path).and_then(|path| path.metadata()) {
            Ok(meta) => reply(SUCCESS, None, &meta.len().to_le_bytes()),
            Err(error) => op.host(&error),
        },
    )
}

/// What an `IFile` `Read` or `Write` moves: up to `len` bytes at file `offset`, from or to
/// guest memory at `at`.
#[derive(Clone, Copy)]
struct Transfer {
    offset: u64,
    len: u64,
    at: GuestVa,
}

/// `IFile` `Read` on the service-I/O thread: the bytes `range` asks for, written to guest
/// memory, replying with the count. The calling guest thread waits meanwhile, so its buffer
/// is not in use (ADR-0019 shares guest memory between threads).
fn read_file(op: &Op, path: &Path, range: Transfer) -> DeferredReply {
    let read = confined(op.root(), path).and_then(|path| read_into(op.mem(), &path, range));
    DeferredReply::Keep(match read {
        Ok(Some(count)) => reply(SUCCESS, None, &count.to_le_bytes()),
        Ok(None) => op.fail(HostIoError::GuestMemory, BAD_BUFFER),
        Err(error) => op.host(&error),
    })
}

/// How an `IFile` `Write` may change the file.
#[derive(Clone, Copy)]
struct WriteFlags {
    /// Opened with the append bit: a write may grow the file.
    append: bool,
    /// The write option asked for a flush afterwards.
    flush: bool,
}

/// `IFile` `Write` on the service-I/O thread: copies `range.len` bytes from guest memory to the
/// file, [`IO_CHUNK`] bytes at a time. Without the append bit a write past the end fails with
/// [`OUT_OF_RANGE`] and changes nothing (note §3). A source buffer the guest cannot read
/// fails with [`BAD_BUFFER`] before the file is touched; only another guest thread unmapping
/// it during the copy can still leave the file partly written, and that too is reported as
/// [`BAD_BUFFER`]. With the append bit, an offset past the end grows the file and zero-fills
/// the gap, as the guest asked of its own SD card.
fn write_file(op: &Op, path: &Path, range: Transfer, flags: WriteFlags) -> DeferredReply {
    let written = if readable(op.mem(), range) {
        confined(op.root(), path).and_then(|path| write_into(op.mem(), &path, range, flags))
    } else {
        Ok(BAD_BUFFER)
    };
    DeferredReply::Keep(match written {
        Ok(BAD_BUFFER) => op.fail(HostIoError::GuestMemory, BAD_BUFFER),
        Ok(code) => reply(code, None, &[]),
        Err(error) => op.host(&error),
    })
}

/// The write itself, on a confined path; returns the result code for the guest.
fn write_into(
    mem: &AddressSpace,
    path: &Path,
    range: Transfer,
    flags: WriteFlags,
) -> io::Result<u32> {
    let mut file = OpenOptions::new().write(true).open(path)?;
    let end = range
        .offset
        .checked_add(range.len)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the write ends past 2^64"))?;
    if !flags.append && end > file.metadata()?.len() {
        return Ok(OUT_OF_RANGE);
    }
    file.seek(SeekFrom::Start(range.offset))?;
    let mut chunk = vec![0u8; chunk_len(range.len)];
    let mut done = 0u64;
    while done < range.len {
        let n = chunk_len(range.len - done);
        if mem
            .read_bytes(range.at.wrapping_add(done), &mut chunk[..n])
            .is_err()
        {
            return Ok(BAD_BUFFER);
        }
        file.write_all(&chunk[..n])?;
        done += n as u64;
    }
    if flags.flush {
        file.sync_data()?;
    }
    Ok(SUCCESS)
}

/// Whether the guest can read every page of `range`: one byte per page, so a bad buffer is
/// found before a write touches the file.
fn readable(mem: &AddressSpace, range: Transfer) -> bool {
    if range.len == 0 {
        return true;
    }
    let mut byte = [0u8];
    let first_page = range.at.get() / GUEST_PAGE_SIZE;
    let last = range.at.get().saturating_add(range.len - 1);
    (first_page..=last / GUEST_PAGE_SIZE).all(|page| {
        let at = (page * GUEST_PAGE_SIZE).max(range.at.get());
        mem.read_bytes(GuestVa::new(at), &mut byte).is_ok()
    })
}

/// Bytes the next copy step moves when `left` remain: at most [`IO_CHUNK`].
fn chunk_len(left: u64) -> usize {
    usize::try_from(left).map_or(IO_CHUNK, |left| left.min(IO_CHUNK))
}

/// `IFile` `Flush` on the service-I/O thread: syncs the file's data to the host disk. Nothing
/// to flush on a file not opened for writing.
// GUESS: flushing a read-only file succeeds (note §4 has no result for it).
fn flush_file(op: &Op, path: &Path, may_write: bool) -> DeferredReply {
    let flushed = confined(op.root(), path).and_then(|path| {
        if may_write {
            OpenOptions::new().write(true).open(path)?.sync_data()?;
        }
        Ok(())
    });
    DeferredReply::Keep(match flushed {
        Ok(()) => reply(SUCCESS, None, &[]),
        Err(error) => op.host(&error),
    })
}

/// `IFile` `SetSize` on the service-I/O thread: truncates or zero-extends the file.
fn set_size(op: &Op, path: &Path, size: u64) -> DeferredReply {
    let resized = confined(op.root(), path)
        .and_then(|path| OpenOptions::new().write(true).open(path)?.set_len(size));
    DeferredReply::Keep(match resized {
        Ok(()) => reply(SUCCESS, None, &[]),
        Err(error) => op.host(&error),
    })
}

/// The path in the request's first pointer buffer, joined to the root; `None` when there is
/// no such buffer, guest memory cannot be read, or the path is malformed or leaves the root.
fn guest_path(shared: &Shared, msg: &Message<'_>) -> Option<PathBuf> {
    let pointer = msg.pointers().next()?;
    let mut raw = [0u8; PATH_SIZE];
    let len = usize::from(pointer.size).min(PATH_SIZE);
    shared
        .mem
        .read_bytes(GuestVa::new(pointer.address), &mut raw[..len])
        .ok()?;
    lexical(&shared.root, &raw[..len])
}

/// `root` joined with the NUL-terminated guest path in `raw`, or `None` when it has no NUL,
/// is not UTF-8, or has a `..` component, a backslash or a colon. Empty and `.` components
/// are skipped, so `/a//./b` is `a/b`. Pure: no host access.
// GUESS: SD-card paths are relative to the root with an optional leading `/`; the
// `@Sdcard:/` form was not researched (note §4) and is rejected by the colon rule.
fn lexical(root: &Path, raw: &[u8]) -> Option<PathBuf> {
    let end = raw.iter().position(|&b| b == 0)?;
    let text = std::str::from_utf8(&raw[..end]).ok()?;
    let mut path = root.to_path_buf();
    for part in text.split('/') {
        if part == ".." || part.contains(['\\', ':']) {
            return None;
        }
        if !part.is_empty() && part != "." {
            path.push(part);
        }
    }
    Some(path)
}

/// `path` with every symbolic link resolved, if it exists and is still inside `root`.
/// Service-I/O thread only.
fn confined(root: &Path, path: &Path) -> io::Result<PathBuf> {
    let real = path.canonicalize()?;
    if real.starts_with(root) {
        Ok(real)
    } else {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, OutsideRoot))
    }
}

/// The error [`confined`] returns for a path that resolves outside the root, so [`classify`]
/// can tell it from any other refused access.
#[derive(Debug)]
struct OutsideRoot;

impl fmt::Display for OutsideRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the path leaves the SD-card root")
    }
}

impl std::error::Error for OutsideRoot {}

/// Copies up to `range.len` bytes of `path` from `range.offset` into guest memory at
/// `range.at`, [`IO_CHUNK`] bytes at a time, so a large read never buffers more than one
/// chunk on the host. Returns the count: fewer at the end of the file, none past it; `None`
/// when the guest buffer cannot be written.
// GUESS: a short read at the end of the file succeeds with the shorter count (note §4).
fn read_into(mem: &AddressSpace, path: &Path, range: Transfer) -> io::Result<Option<u64>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(range.offset))?;
    let mut chunk = vec![0u8; chunk_len(range.len)];
    let mut done = 0u64;
    while done < range.len {
        let got = file.read(&mut chunk[..chunk_len(range.len - done)])?;
        if got == 0 {
            break;
        }
        if mem
            .write_bytes(range.at.wrapping_add(done), &chunk[..got])
            .is_err()
        {
            return Ok(None);
        }
        done += got as u64;
    }
    Ok(Some(done))
}

/// Defers `work`; without a service-I/O thread the request is an unknown command, so the
/// kernel stops the guest instead of the object doing host I/O on the guest core.
fn defer(
    sessions: &mut dyn Sessions,
    name: &'static str,
    work: lince_kernel::DeferredWork,
    unknown: UnknownCommand,
) -> Result<(), UnknownCommand> {
    if sessions.defer(name, work) {
        Ok(())
    } else {
        Err(unknown)
    }
}

/// Replies `code` now, with no I/O.
fn reply_now(
    message: &mut IpcMessage,
    code: u32,
    unknown: UnknownCommand,
) -> Result<(), UnknownCommand> {
    write_reply(message, code, None, &[])
        .map(|_| ())
        .map_err(|_| unknown)
}

/// A reply message built on the service-I/O thread.
fn reply(code: u32, handle: Option<u32>, params: &[u8]) -> IpcMessage {
    let mut message = [0; IPC_MESSAGE_SIZE];
    write_reply(&mut message, code, handle, params)
        .expect("invariant: a reply with at most one handle and 8 bytes of output fits");
    message
}

/// The little-endian `u32` at `at` in `params`.
fn word(params: &[u8], at: usize) -> Option<u32> {
    params
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
}

/// The little-endian `u64` at `at` in `params`.
fn dword(params: &[u8], at: usize) -> Option<u64> {
    params
        .get(at..at + 8)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_le_bytes)
}

#[cfg(test)]
mod tests;
