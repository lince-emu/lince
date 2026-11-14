// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Once-per-name log records in per-thread buffers, drained off the critical threads.
//!
//! A [`Trace`] is owned by one thread that is not render-critical: it registers names, hands a
//! [`Recorder`] to each recording thread and drains the buffers. Recorders only ever do
//! non-blocking work: a bounded duplicate check and a `try_send` into a bounded channel. No
//! I/O, locks or allocation happen on the recording path (ARCH-03, ARCH-06).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
use std::time::Instant;

use crate::error::{Result, TraceError};
use crate::event::{Event, EventKind, ThreadRole};

/// A registered name; the argument of [`EventKind::Stub`] events (payloads hold no strings,
/// PAT-07).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NameId(u32);

/// One drained event and the role of the thread that recorded it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    /// Role of the recording thread.
    pub role: ThreadRole,
    /// The event.
    pub event: Event,
}

/// The result of [`Trace::drain`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// Events in timestamp order.
    pub records: Vec<Record>,
    /// Events lost since the last drain because a buffer or a duplicate set was full.
    pub dropped: u64,
}

struct Buffer {
    role: ThreadRole,
    rx: Receiver<[u64; 3]>,
    dropped: Arc<AtomicU64>,
}

/// Owner of the name table and of every recorder's buffer.
pub struct Trace {
    epoch: Instant,
    capacity: usize,
    names: Vec<&'static str>,
    buffers: Vec<Buffer>,
}

impl Trace {
    /// Largest per-recorder capacity: the duplicate check is a linear scan on the recording
    /// path, so its length is kept small.
    pub const MAX_CAPACITY: usize = 256;

    /// A trace whose recorders each buffer up to `capacity` undrained events and remember up
    /// to `capacity` distinct names. `capacity` is clamped to `1..=MAX_CAPACITY`.
    pub fn new(capacity: usize) -> Self {
        Self {
            epoch: Instant::now(),
            capacity: capacity.clamp(1, Self::MAX_CAPACITY),
            names: Vec::new(),
            buffers: Vec::new(),
        }
    }

    /// Registers `name`, or returns its id if it is already registered. Call during setup,
    /// not on a render-critical thread.
    pub fn name(&mut self, name: &'static str) -> Result<NameId> {
        if let Some(index) = self.names.iter().position(|n| *n == name) {
            return Ok(NameId(Self::index_to_id(index)?));
        }
        let id = NameId(Self::index_to_id(self.names.len())?);
        self.names.push(name);
        Ok(id)
    }

    fn index_to_id(index: usize) -> Result<u32> {
        u32::try_from(index).map_err(|_| TraceError::TooManyNames { registered: index })
    }

    /// The name a [`EventKind::Stub`] event refers to; `None` for other kinds.
    pub fn stub_name(&self, event: &Event) -> Option<&'static str> {
        if event.kind != EventKind::Stub {
            return None;
        }
        let index = usize::try_from(event.arg).ok()?;
        self.names.get(index).copied()
    }

    /// A recorder for one thread with the given role. Move it to that thread.
    pub fn recorder(&mut self, role: ThreadRole) -> Recorder {
        let (tx, rx) = sync_channel(self.capacity);
        let dropped = Arc::new(AtomicU64::new(0));
        self.buffers.push(Buffer {
            role,
            rx,
            dropped: Arc::clone(&dropped),
        });
        Recorder {
            epoch: self.epoch,
            tx,
            seen: Vec::with_capacity(self.capacity),
            dropped,
        }
    }

    /// Takes every buffered event. Allocates; call off render-critical threads. Buffers whose
    /// recorder was dropped are released once empty.
    pub fn drain(&mut self) -> Drained {
        let mut drained = Drained::default();
        self.buffers.retain(|buffer| {
            drained.dropped += buffer.dropped.swap(0, Ordering::Relaxed);
            loop {
                match buffer.rx.try_recv() {
                    Ok(words) => drained.records.push(Record {
                        role: buffer.role,
                        event: Event::decode(words),
                    }),
                    Err(TryRecvError::Empty) => return true,
                    Err(TryRecvError::Disconnected) => return false,
                }
            }
        });
        drained.records.sort_by_key(|record| record.event.ts_ns);
        drained
    }
}

/// The recording end used by one thread. Never blocks, allocates or performs I/O.
pub struct Recorder {
    epoch: Instant,
    tx: SyncSender<[u64; 3]>,
    /// Keys already recorded; never grows past its initial capacity.
    seen: Vec<(u16, u64)>,
    dropped: Arc<AtomicU64>,
}

impl Recorder {
    /// Records that the stub `name` ran, the first time only. Returns whether it was recorded.
    pub fn stub(&mut self, name: NameId) -> bool {
        self.once(EventKind::Stub, u64::from(name.0))
    }

    /// Records `kind` with `arg` the first time this recorder sees that pair; later calls with
    /// the same pair do nothing. Once `capacity` pairs were recorded, new pairs are counted as
    /// dropped. An event that could not be sent is not remembered as seen. Returns whether the
    /// event was recorded.
    pub fn once(&mut self, kind: EventKind, arg: u64) -> bool {
        let key = (kind.to_raw(), arg);
        if self.seen.contains(&key) {
            return false;
        }
        if self.seen.len() == self.seen.capacity() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let event = Event {
            ts_ns: u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX),
            frame: 0,
            kind,
            arg,
        };
        match self.tx.try_send(event.encode()) {
            Ok(()) => {
                self.seen.push(key);
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
}
