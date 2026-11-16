// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `fsp-srv` on a host directory (T-064 step 2, read-only), from
//! docs/research/fsp-srv-filesystem-protocol.md (Switch 1 knowledge; assumed unchanged on
//! Switch 2, doc 08).
//!
//! `OpenSdCardFileSystem` hands out an `IFileSystem` rooted at a directory the user chose
//! ([`SdCard`]). It answers `GetEntryType` and `OpenFile` (read mode); the `IFile` answers `Read` and
//! `GetSize`. Writes and directories are step 3: those commands are unknown commands for now.
//!
//! # Threads
//! Every host file-system access runs on the service-I/O thread through a deferred reply
//! ([`Sessions::defer`]); on the guest core the objects only read the request, read the path
//! from guest memory and check it lexically (invariant 3). If no service-I/O thread runs, the
//! request is an unknown command rather than I/O on the guest core.
//!
//! # Confinement
//! A guest path never reaches outside the root: on the guest core it is split on `/` and any
//! `..` component, backslash or colon rejects it; on the service-I/O thread the joined path is
//! canonicalized and must still start with the canonical root, which stops symbolic links
//! that point outside. Each command opens the path again after that check; a link swapped in
//! between by another host process could still escape. The guest cannot create links (this
//! step is read-only), so only the user's own host processes could, which Lince does not
//! defend against.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lince_common::GuestVa;
use lince_kernel::{
    DeferredReply, IPC_MESSAGE_SIZE, IpcMessage, SessionObject, Sessions, UnknownCommand,
};
use lince_mem::AddressSpace;
use lince_trace::{NameId, Trace, TraceError};

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
/// `OpenFile` mode bit 0; bits 1 (write) and 2 (append) come with step 3.
const MODE_READ: u32 = 1;
/// Bytes a `Read` copies through the host per step.
const READ_CHUNK: usize = 64 * 1024;
/// `GetEntryType` values (note §3).
const ENTRY_DIRECTORY: u32 = 0;
const ENTRY_FILE: u32 = 1;

const SET_CURRENT_PROCESS: u32 = 1;
const OPEN_SD_CARD_FILE_SYSTEM: u32 = 18;
const GET_ENTRY_TYPE: u32 = 7;
const OPEN_FILE: u32 = 8;
const READ: u32 = 0;
const GET_SIZE: u32 = 4;

/// The host directory the guest's SD card is.
pub struct SdCard {
    root: PathBuf,
    mem: Arc<AddressSpace>,
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
        Ok(Self { root, mem })
    }
}

struct Shared {
    root: PathBuf,
    mem: Arc<AddressSpace>,
    fsp: NameId,
    file_system: NameId,
    file: NameId,
    set_current_process: NameId,
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
        fsp: trace.name("fsp-srv")?,
        file_system: trace.name("fsp-srv.IFileSystem")?,
        file: trace.name("fsp-srv.IFile")?,
        set_current_process: trace.name("fsp-srv.SetCurrentProcess")?,
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
        if command != GET_ENTRY_TYPE && command != OPEN_FILE {
            return Err(unknown);
        }
        let mode = word(params, 0);
        if command == OPEN_FILE && mode.is_some_and(|mode| mode & !MODE_READ != 0) {
            // Writing and appending come with T-064 step 3.
            return Err(unknown);
        }
        let Some(path) = guest_path(&self.0, &msg) else {
            return reply_now(message, PATH_NOT_FOUND, unknown);
        };
        let root = self.0.root.clone();
        if command == GET_ENTRY_TYPE {
            let work = Box::new(move || entry_type(&root, &path));
            return defer(sessions, "fsp-srv.GetEntryType", work, unknown);
        }
        let file = Box::new(OpenedFile(Arc::clone(&self.0), Arc::new(path.clone())));
        let Some(handle) = sessions.open_session(file) else {
            return reply_now(message, OUT_OF_SESSIONS, unknown);
        };
        let work = Box::new(move || open_file(&root, &path, handle));
        defer(sessions, "fsp-srv.OpenFile", work, unknown)
    }
}

/// An `IFile` opened for reading. It keeps the path, not a host handle: each command opens the
/// file again on the service-I/O thread.
struct OpenedFile(Arc<Shared>, Arc<PathBuf>);

impl SessionObject for OpenedFile {
    fn name(&self) -> &'static str {
        "fsp-srv.IFile"
    }

    fn trace_name(&self) -> NameId {
        self.0.file
    }

    fn handle_request(
        &mut self,
        sessions: &mut dyn Sessions,
        message: &mut IpcMessage,
    ) -> Result<(), UnknownCommand> {
        let (msg, command, params) = read_request(message)?;
        let unknown = UnknownCommand { command };
        let (root, path) = (self.0.root.clone(), Arc::clone(&self.1));
        match command {
            GET_SIZE => {
                let work = Box::new(move || file_size(&root, &path));
                defer(sessions, "fsp-srv.IFile.GetSize", work, unknown)
            }
            READ => {
                // Option u32, padding u32, offset s64, size u64; data to the first receive
                // buffer (note §3).
                let (Some(offset), Some(size), Some(out)) =
                    (dword(params, 8), dword(params, 16), msg.receives().next())
                else {
                    return reply_now(message, PATH_NOT_FOUND, unknown);
                };
                let mem = Arc::clone(&self.0.mem);
                let at = GuestVa::new(out.address);
                let len = size.min(out.size);
                let range = ReadRange { offset, len, at };
                let work = Box::new(move || read_file(&mem, &root, &path, range));
                defer(sessions, "fsp-srv.IFile.Read", work, unknown)
            }
            _ => Err(unknown),
        }
    }
}

/// `GetEntryType` on the service-I/O thread: 0 for a directory, 1 for a file.
fn entry_type(root: &Path, path: &Path) -> DeferredReply {
    DeferredReply::Keep(
        match confined(root, path).and_then(|path| path.metadata()) {
            Ok(meta) if meta.is_dir() => reply(SUCCESS, None, &ENTRY_DIRECTORY.to_le_bytes()),
            Ok(_) => reply(SUCCESS, None, &ENTRY_FILE.to_le_bytes()),
            Err(_) => reply(PATH_NOT_FOUND, None, &[]),
        },
    )
}

/// `OpenFile` on the service-I/O thread: hands out `handle` if `path` is a file; otherwise the
/// kernel closes that session, which the guest never learns.
fn open_file(root: &Path, path: &Path, handle: u32) -> DeferredReply {
    match confined(root, path).and_then(|path| path.metadata()) {
        Ok(meta) if meta.is_file() => DeferredReply::Keep(reply(SUCCESS, Some(handle), &[])),
        _ => DeferredReply::Release(reply(PATH_NOT_FOUND, None, &[])),
    }
}

/// `IFile` `GetSize` on the service-I/O thread.
fn file_size(root: &Path, path: &Path) -> DeferredReply {
    DeferredReply::Keep(
        match confined(root, path).and_then(|path| path.metadata()) {
            Ok(meta) => reply(SUCCESS, None, &meta.len().to_le_bytes()),
            Err(_) => reply(PATH_NOT_FOUND, None, &[]),
        },
    )
}

/// What an `IFile` `Read` asks for: up to `len` bytes from `offset`, into guest memory at
/// `at`.
#[derive(Clone, Copy)]
struct ReadRange {
    offset: u64,
    len: u64,
    at: GuestVa,
}

/// `IFile` `Read` on the service-I/O thread: the bytes `range` asks for, written to guest
/// memory, replying with the count. The calling guest thread waits meanwhile, so its buffer
/// is not in use (ADR-0019 shares guest memory between threads).
fn read_file(mem: &AddressSpace, root: &Path, path: &Path, range: ReadRange) -> DeferredReply {
    let read = confined(root, path).and_then(|path| read_into(mem, &path, range));
    DeferredReply::Keep(match read {
        Ok(count) => reply(SUCCESS, None, &count.to_le_bytes()),
        Err(_) => reply(PATH_NOT_FOUND, None, &[]),
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
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the path leaves the SD-card root",
        ))
    }
}

/// Copies up to `range.len` bytes of `path` from `range.offset` into guest memory at
/// `range.at`, [`READ_CHUNK`] bytes at a time, so a large read never buffers more than one
/// chunk on the host. Returns the count: fewer at the end of the file, none past it.
// GUESS: a short read at the end of the file succeeds with the shorter count (note §4).
fn read_into(mem: &AddressSpace, path: &Path, range: ReadRange) -> io::Result<u64> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(range.offset))?;
    let mut chunk = vec![0u8; READ_CHUNK];
    let mut done = 0u64;
    while done < range.len {
        let want =
            usize::try_from(range.len - done).map_or(READ_CHUNK, |left| left.min(READ_CHUNK));
        let got = file.read(&mut chunk[..want])?;
        if got == 0 {
            break;
        }
        mem.write_bytes(range.at.wrapping_add(done), &chunk[..got])
            .map_err(|e| io::Error::other(e.to_string()))?;
        done += got as u64;
    }
    Ok(done)
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
