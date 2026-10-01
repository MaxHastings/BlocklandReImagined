//! A map keyed by brick id that grows a page at a time.
//!
//! Brick ids are handed out in order, so the bricks of a build, a copy or a
//! world sit in runs of neighbouring ids. Keeping them in pages of
//! [`PAGE`] ids means the map grows by one small page when a run reaches a
//! new one. A hash map of every brick instead moves all of them into a
//! table twice the size each time it fills: at a million bricks that is a
//! tick lost all at once, as a copy job plants or puts bricks back.
use bri_world::BrickId;
use rustc_hash::FxHashMap;

/// Ids per page.
const PAGE: usize = 256;

pub struct IdMap<T> {
    pages: FxHashMap<BrickId, Page<T>>,
    len: usize,
}
struct Page<T> {
    slots: Box<[Option<T>; PAGE]>,
    live: usize,
}
impl<T> Default for IdMap<T> {
    fn default() -> Self {
        Self {
            pages: FxHashMap::default(),
            len: 0,
        }
    }
}
fn split(id: BrickId) -> (BrickId, usize) {
    (id / PAGE as BrickId, (id % PAGE as BrickId) as usize)
}
impl<T> IdMap<T> {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn get(&self, id: BrickId) -> Option<&T> {
        let (page, slot) = split(id);
        self.pages.get(&page)?.slots[slot].as_ref()
    }
    pub fn contains(&self, id: BrickId) -> bool {
        self.get(id).is_some()
    }
    /// Put `value` in for `id`: what `id` had before.
    pub fn insert(&mut self, id: BrickId, value: T) -> Option<T> {
        let (page, slot) = split(id);
        let page = self.pages.entry(page).or_insert_with(|| Page {
            slots: Box::new(std::array::from_fn(|_| None)),
            live: 0,
        });
        let old = page.slots[slot].replace(value);
        if old.is_none() {
            page.live += 1;
            self.len += 1;
        }
        old
    }
    pub fn remove(&mut self, id: BrickId) -> Option<T> {
        let (key, slot) = split(id);
        let page = self.pages.get_mut(&key)?;
        let value = page.slots[slot].take()?;
        page.live -= 1;
        self.len -= 1;
        if page.live == 0 {
            self.pages.remove(&key);
        }
        Some(value)
    }
}

/// A set of brick ids, grown a page at a time as [`IdMap`] is.
#[derive(Default)]
pub struct IdSet(IdMap<()>);
impl IdSet {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn contains(&self, id: BrickId) -> bool {
        self.0.contains(id)
    }
    /// Add `id`: false when it was in already.
    pub fn insert(&mut self, id: BrickId) -> bool {
        self.0.insert(id, ()).is_none()
    }
    pub fn remove(&mut self, id: BrickId) -> bool {
        self.0.remove(id).is_some()
    }
}
impl FromIterator<BrickId> for IdSet {
    fn from_iter<I: IntoIterator<Item = BrickId>>(ids: I) -> Self {
        let mut set = Self::default();
        for id in ids {
            set.insert(id);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_come_and_go_with_their_ids() {
        let mut map = IdMap::default();
        assert_eq!(map.insert(3, 'a'), None);
        assert_eq!(map.insert(3, 'b'), Some('a'));
        map.insert(PAGE as BrickId * 7 + 1, 'c');
        assert_eq!((map.len(), map.pages.len()), (2, 2));
        assert_eq!(map.get(3), Some(&'b'));
        assert_eq!(map.get(4), None);
        assert_eq!(map.remove(3), Some('b'));
        assert_eq!(map.remove(3), None);
        assert_eq!((map.len(), map.pages.len()), (1, 1));
        let mut set: IdSet = [1, 2, 2].into_iter().collect();
        assert_eq!(set.len(), 2);
        assert!(!set.insert(1));
        assert!(set.remove(1) && !set.contains(1));
    }
}
