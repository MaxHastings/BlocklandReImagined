//! Changed bricks, for replication and for the gameplay systems that follow
//! brick changes. Replication takes the set of bricks changed since its last
//! update. Each system reads only what changed since it last looked, so a
//! brick is reconciled once per change instead of once per tick until the
//! next update (a streamed load changes thousands of bricks a tick).
use bri_world::BrickId;
use std::collections::BTreeSet;

/// A system that reads brick changes with its own cursor.
#[derive(Debug, Clone, Copy)]
pub(super) enum Reader {
    Items,
    Events,
    Vehicles,
    Packages,
}
const READERS: usize = 4;

#[derive(Debug, Default)]
pub(super) struct Dirty {
    /// Changed since replication last took them.
    set: BTreeSet<BrickId>,
    /// Changed since the event phase last read ([`Reader::Events`]): an
    /// input fired between event phases runs these bricks' new programs.
    /// Kept apart from `set` so when replication sends never changes play.
    events_unread: BTreeSet<BrickId>,
    /// Every change in order, repeats included, from log position `base`.
    log: Vec<BrickId>,
    base: u64,
    /// Log position each reader has read up to.
    read: [u64; READERS],
}
impl Dirty {
    pub fn insert(&mut self, id: BrickId) -> bool {
        self.log.push(id);
        self.events_unread.insert(id);
        self.set.insert(id)
    }
    pub fn extend(&mut self, ids: impl IntoIterator<Item = BrickId>) {
        for id in ids {
            self.insert(id);
        }
    }
    /// Changed since the event phase last read its changes.
    pub fn unread_by_events(&self, id: &BrickId) -> bool {
        self.events_unread.contains(id)
    }
    /// What [`Self::unread_by_events`] holds, in id order.
    pub fn events_unread(&self) -> impl Iterator<Item = BrickId> + '_ {
        self.events_unread.iter().copied()
    }
    /// Replication's view: everything changed since its last take.
    pub fn take(&mut self) -> BTreeSet<BrickId> {
        std::mem::take(&mut self.set)
    }
    /// Bricks changed since `reader` last read, once each, in id order.
    pub fn read(&mut self, reader: Reader) -> BTreeSet<BrickId> {
        let from = (self.read[reader as usize] - self.base) as usize;
        let changed = self.log[from..].iter().copied().collect();
        self.skip(reader);
        changed
    }
    /// Mark everything read without looking (the reader has no state yet
    /// and scans the whole world when it starts).
    pub fn skip(&mut self, reader: Reader) {
        if matches!(reader, Reader::Events) {
            self.events_unread.clear();
        }
        self.read[reader as usize] = self.base + self.log.len() as u64;
        let oldest = self.read.iter().copied().min().unwrap_or(self.base);
        if oldest > self.base {
            self.log.drain(..(oldest - self.base) as usize);
            self.base = oldest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_reader_sees_each_change_once_and_replication_keeps_its_set() {
        let mut dirty = Dirty::default();
        dirty.extend([3, 1, 3]);
        assert_eq!(dirty.read(Reader::Items), BTreeSet::from([1, 3]));
        assert!(dirty.read(Reader::Items).is_empty());
        dirty.insert(7);
        assert_eq!(dirty.read(Reader::Items), BTreeSet::from([7]));
        assert_eq!(dirty.read(Reader::Events), BTreeSet::from([1, 3, 7]));
        assert_eq!(dirty.take(), BTreeSet::from([1, 3, 7]));
        // A change after replication took the set still reaches readers.
        dirty.insert(9);
        assert_eq!(dirty.read(Reader::Vehicles), BTreeSet::from([1, 3, 7, 9]));
        for reader in [Reader::Items, Reader::Events, Reader::Packages] {
            dirty.skip(reader);
        }
        assert!(dirty.log.is_empty(), "Read changes are dropped");
    }
    /// What the event phase has not read yet does not depend on when
    /// replication takes its set.
    #[test]
    fn the_event_phase_unread_set_ignores_replication() {
        let mut dirty = Dirty::default();
        dirty.extend([4, 2]);
        dirty.take();
        assert!(dirty.unread_by_events(&4));
        assert_eq!(dirty.events_unread().collect::<Vec<_>>(), [2, 4]);
        assert_eq!(dirty.read(Reader::Events), BTreeSet::from([2, 4]));
        assert!(!dirty.unread_by_events(&4));
        dirty.insert(5);
        dirty.read(Reader::Items);
        assert!(
            dirty.unread_by_events(&5),
            "only the event phase's read clears it"
        );
    }
}
