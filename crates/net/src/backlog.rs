//! The brick changes a host keeps after broadcasting them, so a player
//! whose connection dropped can resume with the bricks they still hold and
//! receive only what changed since ([`crate::protocol::Hello::kept`])
//! instead of the whole world.
use bri_world::{Brick, BrickId};
use std::collections::{BTreeMap, VecDeque};

/// Most changed bricks the backlog holds; older updates are forgotten
/// first, and a resume from before them downloads the whole world.
pub const MOST_BRICKS: usize = 20_000;
/// Most updates the backlog holds (at 20 updates a second, a few minutes
/// of a busy build).
pub const MOST_UPDATES: usize = 4096;

/// Recent updates' brick changes by the cursor each update moved to.
#[derive(Debug)]
pub struct Backlog {
    /// Changes since any cursor at or after this are all here.
    floor: u64,
    updates: VecDeque<(u64, BTreeMap<BrickId, Option<Brick>>)>,
    bricks: usize,
}
impl Backlog {
    /// An empty backlog current to `cursor`.
    pub fn new(cursor: u64) -> Self {
        Self {
            floor: cursor,
            updates: VecDeque::new(),
            bricks: 0,
        }
    }
    /// The world was replaced (a map change at `cursor`): nothing held
    /// before applies, and a client current to `cursor` itself may hold
    /// the old world, so only cursors after it can resume over their
    /// bricks.
    pub fn reset(&mut self, cursor: u64) {
        self.updates.clear();
        self.bricks = 0;
        self.floor = cursor.saturating_add(1);
    }
    /// Keep an update's changes; `cursor` is the one it moved clients to.
    pub fn record(&mut self, cursor: u64, bricks: &BTreeMap<BrickId, Option<Brick>>) {
        if bricks.is_empty() {
            return;
        }
        if bricks.len() > MOST_BRICKS {
            // One update bigger than the whole backlog (a build loaded):
            // nothing before it can be resumed over either.
            self.updates.clear();
            self.bricks = 0;
            self.floor = cursor;
            return;
        }
        self.updates.push_back((cursor, bricks.clone()));
        self.bricks += bricks.len();
        while self.bricks > MOST_BRICKS || self.updates.len() > MOST_UPDATES {
            let Some((forgotten, changes)) = self.updates.pop_front() else {
                break;
            };
            self.bricks -= changes.len();
            self.floor = forgotten;
        }
    }
    /// Every brick changed after `cursor` up to `now` (the host's current
    /// cursor), latest change per brick, or `None` when the backlog no
    /// longer reaches back that far or `cursor` is not one the host moved
    /// clients to.
    pub fn changes_since(&self, cursor: u64, now: u64) -> Option<BTreeMap<BrickId, Option<Brick>>> {
        if cursor < self.floor || cursor > now {
            return None;
        }
        let mut changes = BTreeMap::new();
        for (at, bricks) in &self.updates {
            if *at > cursor {
                changes.extend(bricks.iter().map(|(id, b)| (*id, b.clone())));
            }
        }
        Some(changes)
    }
    pub fn len(&self) -> usize {
        self.bricks
    }
    pub fn is_empty(&self) -> bool {
        self.bricks == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_world::ContentRef;
    fn brick(id: u64) -> Brick {
        Brick::new(
            ContentRef::Resolved("plate".into()),
            [id as f32, 0.1, 0.0],
            1,
        )
    }
    fn update(ids: impl IntoIterator<Item = (u64, bool)>) -> BTreeMap<BrickId, Option<Brick>> {
        ids.into_iter()
            .map(|(id, present)| (id, present.then(|| brick(id))))
            .collect()
    }
    #[test]
    fn changes_since_a_cursor_merge_later_updates_latest_wins() {
        let mut backlog = Backlog::new(10);
        backlog.record(11, &update([(1, true), (2, true)]));
        backlog.record(12, &update([(2, false), (3, true)]));
        backlog.record(13, &update([(3, true)]));
        let since_10 = backlog.changes_since(10, 13).unwrap();
        assert_eq!(since_10.len(), 3);
        assert!(since_10[&1].is_some());
        assert!(since_10[&2].is_none(), "removed after it was planted");
        assert!(since_10[&3].is_some());
        let since_12 = backlog.changes_since(12, 13).unwrap();
        assert_eq!(since_12.keys().copied().collect::<Vec<_>>(), [3]);
        assert!(backlog.changes_since(13, 13).unwrap().is_empty());
        assert!(
            backlog.changes_since(9, 13).is_none(),
            "before the host kept changes"
        );
        assert!(
            backlog.changes_since(14, 13).is_none(),
            "a cursor the host never reached"
        );
    }
    #[test]
    fn forgetting_old_updates_raises_the_floor() {
        let mut backlog = Backlog::new(0);
        let per_update = MOST_BRICKS / 4;
        for i in 1..=5u64 {
            let base = (i - 1) * per_update as u64;
            backlog.record(
                i,
                &update((1..=per_update as u64).map(|k| (base + k, true))),
            );
        }
        assert!(backlog.len() <= MOST_BRICKS);
        assert!(
            backlog.changes_since(0, 5).is_none(),
            "the first update was forgotten"
        );
        assert_eq!(backlog.changes_since(1, 5).unwrap().len(), 4 * per_update);
        backlog.record(6, &update((1..=MOST_BRICKS as u64 + 1).map(|k| (k, true))));
        assert!(
            backlog.is_empty(),
            "an update bigger than the backlog empties it"
        );
        assert!(backlog.changes_since(5, 6).is_none());
        assert!(backlog.changes_since(6, 6).unwrap().is_empty());
    }
    #[test]
    fn a_map_change_forgets_everything_including_its_own_cursor() {
        let mut backlog = Backlog::new(0);
        backlog.record(1, &update([(1, true)]));
        backlog.reset(1);
        assert!(
            backlog.changes_since(1, 1).is_none(),
            "cursor 1 may hold the old map"
        );
        backlog.record(2, &update([(5, true)]));
        assert_eq!(backlog.changes_since(2, 2).unwrap().len(), 0);
        assert!(backlog.changes_since(1, 2).is_none());
    }
}
