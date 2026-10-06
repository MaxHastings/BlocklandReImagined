//! One memory of what a bot gave up on, and until when: a seat or body
//! claim that failed, an item it passed over, a goal no plan reached. Each
//! entry lapses at its tick; a full memory forgets the entry that lapses
//! first.

/// Things given up on, each until a tick; at most `N` of them.
#[derive(Clone, Debug)]
pub(super) struct Cooldowns<K, const N: usize> {
    entries: Vec<(K, u64)>,
}

impl<K, const N: usize> Default for Cooldowns<K, N> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<K: PartialEq, const N: usize> Cooldowns<K, N> {
    /// Gives `key` up until `until`, replacing any earlier entry for it.
    pub(super) fn give_up(&mut self, key: K, until: u64) {
        self.entries.retain(|(k, _)| *k != key);
        if self.entries.len() >= N
            && let Some(first) = (0..self.entries.len()).min_by_key(|&i| self.entries[i].1)
        {
            self.entries.remove(first);
        }
        self.entries.push((key, until));
    }
    /// Whether `key` is still given up on at `tick`.
    pub(super) fn cooling(&self, key: &K, tick: u64) -> bool {
        self.entries.iter().any(|(k, until)| k == key && tick < *until)
    }
    /// Whether `key` is in the memory at all, lapsed or not.
    pub(super) fn holds(&self, key: &K) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }
    /// Forgets the entries lapsed by `tick`, and those `keep` rejects.
    pub(super) fn prune(&mut self, tick: u64, mut keep: impl FnMut(&K) -> bool) {
        self.entries.retain(|(k, until)| tick < *until && keep(k));
    }
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }
    /// What it has given up on, lapsed or not.
    pub(super) fn iter(&self) -> impl Iterator<Item = &K> {
        self.entries.iter().map(|(k, _)| k)
    }
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::Cooldowns;

    #[test]
    fn a_full_memory_forgets_what_lapses_first() {
        let mut c = Cooldowns::<char, 2>::default();
        c.give_up('a', 30);
        c.give_up('b', 10);
        c.give_up('c', 20);
        assert!(c.cooling(&'a', 0) && c.cooling(&'c', 0) && !c.holds(&'b'));
        c.give_up('a', 5);
        assert!(!c.cooling(&'a', 5) && c.holds(&'a'));
        c.prune(5, |_| true);
        assert!(!c.holds(&'a') && c.len() == 1);
    }
}
