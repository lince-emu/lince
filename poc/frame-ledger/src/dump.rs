// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ledger dump and its binary format (little-endian):
//!
//! ```text
//! "LNXL" · version u32 = 1 · thread count u32
//! per thread: role u8 · 3 zero bytes · event count u32 · lost events u64 ·
//!             events (3 × u64 each, see `event`)
//! ```

use std::fmt;
use std::io::{self, Read, Write};

use crate::event::{Event, ThreadRole};

const MAGIC: [u8; 4] = *b"LNXL";
const VERSION: u32 = 1;

/// Events of one thread, oldest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadDump {
    /// The thread's role.
    pub role: ThreadRole,
    /// Events recorded by this thread that the dump does not contain (overwritten, or being
    /// written during the snapshot). Non-zero means the oldest history is gone.
    pub lost_events: u64,
    /// Its events.
    pub events: Vec<Event>,
}

/// A snapshot of every registered thread.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dump {
    /// One entry per registered thread.
    pub threads: Vec<ThreadDump>,
}

/// Reading a dump failed.
#[derive(Debug)]
pub enum DumpError {
    /// The reader failed or ended early.
    Io(io::Error),
    /// Not a ledger dump.
    BadMagic,
    /// A format version this build cannot read.
    UnsupportedVersion(u32),
}

impl fmt::Display for DumpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "reading ledger dump: {e}"),
            Self::BadMagic => write!(f, "not a ledger dump"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported ledger dump version {v}"),
        }
    }
}

impl std::error::Error for DumpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::BadMagic | Self::UnsupportedVersion(_) => None,
        }
    }
}

impl From<io::Error> for DumpError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn read_thread(r: &mut impl Read) -> io::Result<ThreadDump> {
    let mut role = [0u8; 4];
    r.read_exact(&mut role)?;
    let count = read_u32(r)?;
    let lost_events = read_u64(r)?;
    let mut events = Vec::new();
    for _ in 0..count {
        let words = [read_u64(r)?, read_u64(r)?, read_u64(r)?];
        events.push(Event::decode(words));
    }
    Ok(ThreadDump {
        role: ThreadRole::from_raw(role[0]),
        lost_events,
        events,
    })
}

fn count_u32(n: usize) -> io::Result<u32> {
    u32::try_from(n).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "count too large"))
}

impl Dump {
    /// Writes the dump in the binary format.
    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(&MAGIC)?;
        w.write_all(&VERSION.to_le_bytes())?;
        w.write_all(&count_u32(self.threads.len())?.to_le_bytes())?;
        for t in &self.threads {
            w.write_all(&[t.role.to_raw(), 0, 0, 0])?;
            w.write_all(&count_u32(t.events.len())?.to_le_bytes())?;
            w.write_all(&t.lost_events.to_le_bytes())?;
            for word in t.events.iter().flat_map(|e| e.encode()) {
                w.write_all(&word.to_le_bytes())?;
            }
        }
        Ok(())
    }

    /// Reads a dump written by [`Dump::write_to`].
    pub fn read_from(r: &mut impl Read) -> Result<Self, DumpError> {
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if magic != MAGIC {
            return Err(DumpError::BadMagic);
        }
        let version = read_u32(r)?;
        if version != VERSION {
            return Err(DumpError::UnsupportedVersion(version));
        }
        let thread_count = read_u32(r)?;
        let mut threads = Vec::new();
        for _ in 0..thread_count {
            threads.push(read_thread(r)?);
        }
        Ok(Self { threads })
    }
}
