//! Transient damage families live at the common world-mutation boundary.
//! A deliberate edit to a part commits the remaining geometry; restoring or
//! deleting the authored original retires every part. Damage itself uses an
//! explicit transaction so it never masquerades as an authored edit.
use super::*;

#[derive(Default)]
pub(crate) struct DamageFamilies {
    families: BTreeMap<BrickId, Family>,
    parents: BTreeMap<BrickId, BrickId>,
    pub changed: BTreeSet<BrickId>,
    pub ended: BTreeSet<BrickId>,
    depth: u32,
}
struct Family {
    flags: [bool; 3],
    parts: BTreeSet<BrickId>,
}

impl Simulation {
    /// Timed restoration cannot make an original solid inside a live body.
    /// Inspect its saved collision policy without retiring any family/section.
    pub(crate) fn damage_restore_occupied(&self, id: BrickId) -> bool {
        let Some(brick) = self.state().bricks.get(&id) else {
            return false;
        };
        if self.damage_flags(id).is_some_and(|flags| !flags[2])
            || self
                .definitions
                .get(brick)
                .is_ok_and(|d| d.special == Special::Water)
        {
            return false;
        }
        self.engulfs_body(brick, true)
    }

    pub(crate) fn damage_parent(&self, id: BrickId) -> Option<BrickId> {
        self.damage.parents.get(&id).copied()
    }
    pub(crate) fn damage_index(&self) -> &Index {
        &self.index
    }
    pub(crate) fn damage_pieces(&self) -> impl Iterator<Item = BrickId> + '_ {
        self.damage.parents.keys().copied()
    }
    pub(crate) fn damage_flags(&self, id: BrickId) -> Option<[bool; 3]> {
        self.damage.families.get(&id).map(|f| f.flags)
    }
    pub(crate) fn begin_damage_family(&mut self, id: BrickId) {
        let b = &self.state().bricks[&id];
        let flags = [b.visible, b.raycast, b.colliding];
        self.damage.families.entry(id).or_insert(Family {
            flags,
            parts: Default::default(),
        });
    }
    pub(crate) fn register_damage_parts(&mut self, parent: BrickId, ids: &[BrickId]) {
        let family = self
            .damage
            .families
            .get_mut(&parent)
            .expect("damage original");
        for &id in ids {
            family.parts.insert(id);
            self.damage.parents.insert(id, parent);
        }
    }
    pub(crate) fn damage_part_count(&self) -> usize {
        self.damage.parents.len()
    }
    pub(crate) fn damage_family_part_count(&self, root: BrickId) -> usize {
        self.damage.families.get(&root).map_or(0, |f| f.parts.len())
    }
    pub(crate) fn damage_ended(&self, id: BrickId) -> bool {
        self.damage.ended.contains(&id)
    }
    pub(crate) fn damage_transaction<T>(
        &mut self,
        run: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let held = self.holding;
        self.holding = true;
        self.damage.depth += 1;
        let result = run(self);
        self.damage.depth -= 1;
        self.holding = held;
        if !held {
            self.detect_collisions();
        }
        result
    }
    pub(crate) fn restore_damage_families(&mut self, ids: &[BrickId]) -> Result<()> {
        if !ids.iter().any(|id| self.damage.families.contains_key(id)) {
            return Ok(());
        }
        self.damage_transaction(|sim| {
            for &id in ids {
                sim.finish_damage_family(id, true)?;
            }
            Ok(())
        })
    }
    fn finish_damage_family(&mut self, root: BrickId, restore: bool) -> Result<()> {
        let Some(family) = self.damage.families.remove(&root) else {
            return Ok(());
        };
        self.section_forget(root);
        let parts: Vec<_> = family
            .parts
            .into_iter()
            .filter(|id| self.state().bricks.contains_key(id))
            .collect();
        for id in &parts {
            self.damage.parents.remove(id);
        }
        self.damage.ended.insert(root);
        self.damage.changed.extend(parts.iter().copied());
        self.damage.changed.insert(root);
        if restore {
            let engine = Actor {
                administrator: true,
                ..Default::default()
            };
            self.remove_many(&engine, &parts)?;
            if self.state().bricks.contains_key(&root) {
                let [visible, raycast, colliding] = family.flags;
                self.authority.mutate(root, |b| {
                    b.visible = visible;
                    b.raycast = raycast;
                    b.colliding = colliding;
                    b.damage_hidden = false;
                    b.damage_section = None;
                })?;
                self.note_link(root);
                self.sync_flags(root);
            }
        } else {
            // Deliberate edits preserve the geometry the builder sees.
            let engine = Actor {
                administrator: true,
                ..Default::default()
            };
            self.remove_many(&engine, &[root])?;
            let dead: Vec<_> = parts
                .into_iter()
                .filter(|id| self.state().bricks.get(id).is_some_and(|b| b.damage_hidden))
                .collect();
            self.remove_many(&engine, &dead)?;
        }
        Ok(())
    }
    pub(super) fn damage_before_remove(&mut self, ids: &[BrickId]) -> Result<Vec<BrickId>> {
        let mut all: BTreeSet<_> = ids.iter().copied().collect();
        if self.damage.depth == 0 {
            let commits: BTreeSet<_> = ids
                .iter()
                .filter_map(|id| self.damage_parent(*id))
                .filter(|root| !all.contains(root))
                .collect();
            for root in commits {
                self.finish_damage_family(root, false)?;
            }
            for id in ids {
                if let Some(family) = self.damage.families.remove(id) {
                    self.damage.ended.insert(*id);
                    for part in family.parts {
                        self.damage.parents.remove(&part);
                        if self.state().bricks.contains_key(&part) {
                            all.insert(part);
                            self.damage.changed.insert(part);
                        }
                    }
                }
            }
        }
        for id in &all {
            self.section_forget(*id);
            if let Some(root) = self.damage.parents.remove(id)
                && let Some(family) = self.damage.families.get_mut(&root)
            {
                family.parts.remove(id);
            }
        }
        self.damage.changed.extend(all.iter().copied());
        Ok(all
            .into_iter()
            .filter(|id| self.state().bricks.contains_key(id))
            .collect())
    }
    pub(super) fn damage_after_mutation(&mut self, id: BrickId, before: &Brick) -> Result<()> {
        if self.state().bricks.get(&id) != Some(before) {
            self.section_changed(id);
        }
        if self.damage.depth != 0 {
            return Ok(());
        }
        if let Some(root) = self.damage_parent(id) {
            if self.state().bricks.get(&id) != Some(before) {
                self.finish_damage_family(root, false)?;
            }
        } else if self.damage.families.contains_key(&id) {
            let next = &self.state().bricks[&id];
            let revive = (!before.visible && next.visible)
                || (!before.raycast && next.raycast)
                || (!before.colliding && next.colliding);
            if revive {
                let mut next = next.clone();
                next.damage_hidden = false;
                next.damage_section = None;
                self.finish_damage_family(id, true)?;
                self.authority.mutate(id, |b| *b = next)?;
                self.sync_flags(id);
            }
        }
        Ok(())
    }
    pub(crate) fn take_damage_changes(&mut self) -> (BTreeSet<BrickId>, BTreeSet<BrickId>) {
        (
            std::mem::take(&mut self.damage.changed),
            std::mem::take(&mut self.damage.ended),
        )
    }
}
