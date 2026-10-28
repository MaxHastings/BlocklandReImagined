//! One authoritative journal for pending membership, delivery and retirement.
//! Brick state remains gameplay truth; adapters consume terminal barriers.
use super::*;
use std::sync::Arc;
pub(crate) type SectionKey = (u64, u64);
#[derive(Default)]
pub(crate) struct DamageSections {
    entries: BTreeMap<SectionKey, Section>,
    owners: BTreeMap<BrickId, BTreeSet<SectionKey>>,
    collecting: BTreeSet<SectionKey>,
    changed: BTreeSet<SectionKey>,
    writer: Option<SectionKey>,
}
enum Phase {
    Collecting,
    Interrupted,
    Ready { due: u64 },
    Retired { at: u64 },
}
struct Section {
    members: u32,
    references: usize,
    phase: Phase,
    planned: Arc<BTreeSet<BrickId>>,
    parents: Arc<BTreeSet<BrickId>>,
}
impl Section {
    fn empty() -> Self {
        Self {
            members: 0,
            references: 0,
            phase: Phase::Collecting,
            planned: Default::default(),
            parents: Default::default(),
        }
    }
}
impl Simulation {
    pub(crate) fn begin_section(
        &mut self,
        key: SectionKey,
        planned: Arc<BTreeSet<BrickId>>,
        parents: Arc<BTreeSet<BrickId>>,
    ) {
        debug_assert!(!self.sections.entries.contains_key(&key));
        self.sections.entries.insert(
            key,
            Section {
                planned,
                parents,
                ..Section::empty()
            },
        );
        self.sections.collecting.insert(key);
    }
    pub(crate) fn set_section_writer(&mut self, key: Option<SectionKey>) -> Option<SectionKey> {
        std::mem::replace(&mut self.sections.writer, key)
    }
    pub(crate) fn section_writer(&self) -> Option<SectionKey> {
        self.sections.writer
    }
    pub(crate) fn section_changed(&mut self, id: BrickId) {
        for key in &self.sections.collecting {
            if self.sections.writer == Some(*key) {
                continue;
            }
            let section = self.sections.entries.get_mut(key).unwrap();
            if section.planned.contains(&id) || section.parents.contains(&id) {
                section.phase = Phase::Interrupted;
            }
        }
    }
    pub(crate) fn section_interrupted(&self, key: SectionKey) -> bool {
        self.sections
            .entries
            .get(&key)
            .is_some_and(|s| !matches!(s.phase, Phase::Collecting))
    }
    pub(crate) fn section_member(&mut self, key: SectionKey, id: BrickId) {
        if self
            .sections
            .owners
            .get(&id)
            .is_some_and(|keys| keys.contains(&key))
        {
            return;
        }
        let root = self.damage_parent(id).unwrap_or(id);
        let section = self
            .sections
            .entries
            .entry(key)
            .or_insert_with(Section::empty);
        debug_assert!(matches!(section.phase, Phase::Collecting));
        section.members += 1;
        for source in [Some(id), (root != id).then_some(root)]
            .into_iter()
            .flatten()
        {
            if self.sections.owners.entry(source).or_default().insert(key) {
                self.sections.entries.get_mut(&key).unwrap().references += 1;
            }
        }
    }
    pub(crate) fn seal_section(&mut self, key: SectionKey, delay: u64) {
        let now = self.state().tick;
        if let Some(section) = self.sections.entries.get_mut(&key)
            && matches!(section.phase, Phase::Collecting | Phase::Interrupted)
        {
            section.phase = if section.references == 0 {
                Phase::Retired { at: now }
            } else {
                Phase::Ready {
                    due: now.saturating_add(delay),
                }
            };
            section.planned = Default::default();
            section.parents = Default::default();
            self.sections.collecting.remove(&key);
            self.sections.changed.insert(key);
        }
    }
    pub(crate) fn section_due(&self, id: BrickId) -> Option<u64> {
        let committed = self
            .sections
            .owners
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|key| {
                self.sections.entries.get(key).and_then(|s| match s.phase {
                    Phase::Collecting | Phase::Interrupted => Some(u64::MAX),
                    Phase::Ready { due } => Some(due),
                    Phase::Retired { .. } => None,
                })
            })
            .max();
        if self.sections.collecting.iter().any(|key| {
            let s = &self.sections.entries[key];
            s.planned.contains(&id) || s.parents.contains(&id)
        }) {
            Some(u64::MAX)
        } else {
            committed
        }
    }
    pub(crate) fn section_forget(&mut self, id: BrickId) {
        self.section_changed(id);
        let at = self.state().tick;
        if let Some(keys) = self.sections.owners.remove(&id) {
            for key in keys {
                if let Some(section) = self.sections.entries.get_mut(&key) {
                    section.references = section.references.saturating_sub(1);
                    if section.references == 0 && matches!(section.phase, Phase::Ready { .. }) {
                        section.phase = Phase::Retired { at };
                        self.sections.changed.insert(key);
                    }
                }
            }
        }
    }
    pub(crate) fn take_section_barriers(&mut self, limit: usize) -> Vec<(SectionKey, u32, bool)> {
        (0..limit)
            .filter_map(|_| self.sections.changed.pop_first())
            .filter_map(|key| {
                self.sections
                    .entries
                    .get(&key)
                    .map(|s| (key, s.members, matches!(s.phase, Phase::Retired { .. })))
            })
            .collect()
    }
    pub(crate) fn retire_section_journal(&mut self) {
        let tick = self.state().tick;
        self.sections.entries.retain(|key, s| match s.phase {
            Phase::Retired { at } => {
                tick <= at.saturating_add(3600) || self.sections.changed.contains(key)
            }
            _ => true,
        });
    }
}
