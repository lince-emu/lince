// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The per-process handle table (ADR-0024 B2 scope).
//!
//! A handle names a slot and the generation the slot had when the handle was made, so a closed
//! handle does not name the object a later handle put in the same slot. Known B2 limitation:
//! generations wrap after 65 535 reuses of one slot, and a handle closed exactly that many
//! reuses earlier names the slot's object again.

/// Most handles a process may hold at once.
// GUESS: Horizon's limit comes from the process's resource limit, which B2 does not model.
const MAX_HANDLES: usize = 1024;

/// Bits of a handle that hold the slot index; the generation sits above them.
const INDEX_BITS: u32 = 15;

/// What a handle refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Object {
    /// A thread, by its index in the thread table.
    Thread(usize),
    /// The readable end of an event, by its index in the event list.
    ReadableEvent(usize),
    /// The writable end of an event, by its index in the event list.
    WritableEvent(usize),
}

#[derive(Clone, Copy)]
struct Slot {
    generation: u32,
    object: Option<Object>,
}

/// Open handles, indexed by slot.
#[derive(Default)]
pub(crate) struct Handles {
    slots: Vec<Slot>,
}

impl Handles {
    /// Whether [`open`](Self::open) can succeed; checked before creating an object that would
    /// otherwise be left without a handle.
    pub(crate) fn has_room(&self) -> bool {
        self.slots.len() < MAX_HANDLES || self.slots.iter().any(|slot| slot.object.is_none())
    }

    /// Opens a handle to `object`; `None` once [`MAX_HANDLES`] are open.
    pub(crate) fn open(&mut self, object: Object) -> Option<u32> {
        let index = match self.slots.iter().position(|slot| slot.object.is_none()) {
            Some(index) => index,
            None if self.slots.len() < MAX_HANDLES => {
                self.slots.push(Slot {
                    generation: 0,
                    object: None,
                });
                self.slots.len() - 1
            }
            None => return None,
        };
        let slot = &mut self.slots[index];
        // Generations start at 1, so no handle is 0, and stay below bit 31, so no handle is a
        // pseudo-handle such as `CURRENT_THREAD`.
        slot.generation = slot.generation % ((1 << (31 - INDEX_BITS)) - 1) + 1;
        slot.object = Some(object);
        Some(slot.generation << INDEX_BITS | u32::try_from(index).ok()?)
    }

    /// The object `handle` refers to, if it is open.
    pub(crate) fn get(&self, handle: u32) -> Option<Object> {
        let index = usize::try_from(handle & ((1 << INDEX_BITS) - 1)).ok()?;
        let slot = self.slots.get(index)?;
        (slot.generation == handle >> INDEX_BITS)
            .then_some(slot.object)
            .flatten()
    }

    /// Closes `handle`; returns the object it referred to, or `None` if it was not open.
    pub(crate) fn close(&mut self, handle: u32) -> Option<Object> {
        let object = self.get(handle)?;
        let index = usize::try_from(handle & ((1 << INDEX_BITS) - 1)).ok()?;
        self.slots[index].object = None;
        Some(object)
    }
}

#[cfg(test)]
mod tests {
    use super::{Handles, MAX_HANDLES, Object};

    #[test]
    fn a_closed_handle_never_names_the_next_object_in_its_slot() {
        let mut handles = Handles::default();
        let first = handles.open(Object::ReadableEvent(0)).expect("open");
        assert_eq!(handles.close(first), Some(Object::ReadableEvent(0)));
        let second = handles.open(Object::WritableEvent(1)).expect("open");
        assert_ne!(first, second);
        assert_eq!(handles.get(first), None);
        assert_eq!(handles.close(first), None, "closing twice fails");
        assert_eq!(handles.get(second), Some(Object::WritableEvent(1)));
    }

    #[test]
    fn handles_are_never_zero_and_run_out_at_the_limit() {
        let mut handles = Handles::default();
        let mut last = 0;
        for n in 0..MAX_HANDLES {
            assert!(handles.has_room());
            last = handles.open(Object::Thread(n)).expect("under the limit");
            assert_ne!(last, 0);
        }
        assert!(!handles.has_room());
        assert_eq!(handles.open(Object::Thread(0)), None);
        handles.close(last);
        assert!(handles.has_room(), "a closed slot is room again");
    }

    #[test]
    fn generations_wrap_from_the_highest_back_to_one_below_bit_31() {
        let mut handles = Handles::default();
        let first = handles.open(Object::Thread(0)).expect("open");
        assert_eq!(first, 1 << 15, "generation 1, slot 0");
        let mut handle = first;
        for _ in 1..u32::from(u16::MAX) {
            handles.close(handle);
            handle = handles.open(Object::Thread(0)).expect("open");
        }
        assert_eq!(handle, u32::from(u16::MAX) << 15, "the highest generation");
        assert!(handle < 1 << 31);
        handles.close(handle);
        assert_eq!(handles.open(Object::Thread(0)), Some(first), "wrapped to 1");
    }
}
