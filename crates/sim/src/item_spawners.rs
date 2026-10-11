//! Persistent brick properties drive transient static items and respawn clocks.
use anyhow::{Context, Result, ensure};
use bri_weapons::ItemBounds;
use bri_world::{Brick, BrickId, ContentRef};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_STATIC_ITEMS: usize = 4096;
/// An `available_at` no tick reaches: the item stays gone until a reset or
/// an event restores it ([`bri_world::ItemRestore::Reset`], `hideItem`).
pub const NEVER: u64 = u64::MAX;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticItem {
    pub brick: BrickId,
    pub item: String,
    pub position: [f32; 3],
    pub direction: u8,
    pub available_at: u64,
    /// The palette colour of the brick it stands on, for an item whose
    /// image takes paint (a flag shows its stand's team colour).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint: Option<u8>,
}
impl StaticItem {
    pub fn rotation(&self) -> Quat {
        facing(self.direction)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.brick > 0
                && !self.item.is_empty()
                && self.item.len() <= 128
                && !self.item.chars().any(char::is_control)
                && (2..=5).contains(&self.direction)
                && self.position.iter().all(|v| v.is_finite() && v.abs() < 1e7),
            "Invalid static item view"
        );
        Ok(())
    }
}
/// The Torque quaternion conjugation plus Z-up basis gives negative native
/// yaw for East. Selectors are world axes, independent of brick rotation.
pub fn facing(direction: u8) -> Quat {
    let angle = match direction {
        3 => -std::f32::consts::FRAC_PI_2,
        4 => std::f32::consts::PI,
        5 => std::f32::consts::FRAC_PI_2,
        _ => 0.,
    };
    Quat::from_rotation_y(angle)
}
pub fn placement(
    brick: &Brick,
    mesh: &bri_content::brick::Brick,
    item: ItemBounds,
) -> Result<Vec3> {
    brick.item_spawn.validate()?;
    item.validate()?;
    let rotated = item.transformed(Vec3::ZERO, facing(brick.item_spawn.direction));
    let low = Vec3::from(rotated.min);
    let high = Vec3::from(rotated.max);
    let item_half = (high - low) * 0.5;
    let mut brick_half = Vec3::new(
        mesh.footprint_studs[0] as f32 * 0.25,
        mesh.height_plates as f32 * 0.1,
        mesh.footprint_studs[1] as f32 * 0.25,
    );
    if brick.quarter_turns % 2 == 1 {
        std::mem::swap(&mut brick_half.x, &mut brick_half.z);
    }
    let axis = match brick.item_spawn.position {
        0 => Vec3::Y,
        1 => Vec3::NEG_Y,
        2 => Vec3::NEG_Z,
        3 => Vec3::X,
        4 => Vec3::Z,
        5 => Vec3::NEG_X,
        _ => unreachable!(),
    };
    // Preserve pivot-to-box-center offset, including asymmetric authored bounds.
    Ok(Vec3::from(brick.position) - (low + high) * 0.5 + axis * (brick_half + item_half))
}

#[derive(Default)]
pub struct ItemSpawners {
    pub bounds: BTreeMap<String, ItemBounds>,
    pub items: BTreeMap<BrickId, StaticItem>,
    /// Items that take the paint of the brick they stand on.
    pub painted: BTreeSet<String>,
    contacts: ContactIndex,
}
impl ItemSpawners {
    pub fn validate_edit(
        &self,
        world: &bri_world::World,
        id: BrickId,
        edit: &bri_world::authority::Edit,
    ) -> Result<()> {
        if self.bounds.is_empty() {
            return Ok(());
        }
        if let bri_world::authority::Edit::Properties(properties) = edit {
            properties.item_spawn.validate()?;
            if let Some(ContentRef::Resolved(item)) = &properties.item_spawn.item {
                // A brick keeps an item this server leaves out (see
                // `reconcile`), so a wrench Send that leaves it alone passes.
                let kept = world
                    .bricks
                    .get(&id)
                    .is_some_and(|b| b.item_spawn.item == properties.item_spawn.item);
                ensure!(
                    kept || self.bounds.contains_key(item),
                    "Missing authored item bounds: {item}"
                );
                let others = world
                    .bricks
                    .iter()
                    .filter(|(other, brick)| {
                        **other != id
                            && matches!(brick.item_spawn.item, Some(ContentRef::Resolved(_)))
                    })
                    .count();
                ensure!(others < MAX_STATIC_ITEMS, "Static item capacity exceeded");
            }
        }
        Ok(())
    }
    pub fn validate_append<'a>(
        &self,
        world: &bri_world::World,
        bricks: impl IntoIterator<Item = &'a Brick>,
    ) -> Result<()> {
        if self.bounds.is_empty() {
            return Ok(());
        }
        let spawns = |b: &Brick| matches!(&b.item_spawn.item, Some(ContentRef::Resolved(item)) if self.bounds.contains_key(item));
        let mut count = world.bricks.values().filter(|b| spawns(b)).count();
        // An item this server doesn't have (its Add-On is off) is left out
        // when the brick is placed, as `reconcile` does; the save still loads.
        count += bricks.into_iter().filter(|b| spawns(b)).count();
        ensure!(count <= MAX_STATIC_ITEMS, "Static item capacity exceeded");
        Ok(())
    }
    pub fn new(bounds: BTreeMap<String, ItemBounds>) -> Self {
        Self {
            bounds,
            ..Default::default()
        }
    }
    pub fn contacts(&self, bounds: ItemBounds) -> BTreeSet<u64> {
        self.contacts.query(bounds)
    }
    fn remove(&mut self, id: BrickId) {
        self.items.remove(&id);
        self.contacts.remove(id);
    }
    pub fn reconcile(
        &mut self,
        id: BrickId,
        brick: Option<&Brick>,
        definitions: &crate::definitions::Definitions,
        tick: u64,
    ) -> Result<()> {
        let Some(brick) = brick else {
            self.remove(id);
            return Ok(());
        };
        let Some(ContentRef::Resolved(item)) = &brick.item_spawn.item else {
            self.remove(id);
            return Ok(());
        };
        // A saved item whose Add-On is off on this server spawns nothing; the
        // brick keeps naming it, so it comes back when the Add-On does.
        let Some(&bounds) = self.bounds.get(item) else {
            self.remove(id);
            return Ok(());
        };
        let definition = definitions.get(brick)?;
        let position = placement(brick, &definition.mesh, bounds)?;
        ensure!(
            self.items.contains_key(&id) || self.items.len() < MAX_STATIC_ITEMS,
            "Static item capacity exceeded"
        );
        let available_at = self
            .items
            .get(&id)
            .filter(|old| old.item == *item)
            .map_or(tick, |old| old.available_at);
        self.contacts.insert(
            id,
            bounds.transformed(position, facing(brick.item_spawn.direction)),
        );
        self.items.insert(
            id,
            StaticItem {
                brick: id,
                item: item.clone(),
                position: position.to_array(),
                direction: brick.item_spawn.direction,
                available_at,
                paint: self.painted.contains(item).then_some(brick.color),
            },
        );
        Ok(())
    }
    /// `fxDTSBrick::setItem` deletes the brick's Item and creates a fresh one.
    /// The wrench's Send always sets the item (`IDB`), as does the `setItem`
    /// event, so either brings back an item still faded from a pickup.
    /// Direction, position and respawn-time changes move or retime the same
    /// Item and leave its clock alone, as `reconcile` does.
    pub fn restock(&mut self, brick: BrickId, tick: u64) {
        if let Some(item) = self.items.get_mut(&brick) {
            item.available_at = item.available_at.min(tick);
        }
    }
    /// `Item::Respawn`: the picked-up static item fades out, cannot be picked
    /// up, and fades back in at `available_again` (the brick's respawn time),
    /// or never on its own when that is `None` (a reset or `restoreItem`
    /// brings it back). Refused when the item is already gone, so one take
    /// is one pickup however many players touch it in the same tick.
    pub fn picked_up(&mut self, brick: BrickId, tick: u64, available_again: Option<u64>) -> Result<()> {
        let item = self.items.get_mut(&brick).context("Missing static item")?;
        ensure!(tick >= item.available_at, "Static item has not respawned");
        item.available_at = available_again.unwrap_or(NEVER);
        Ok(())
    }
    /// `hideItem`: take the item away until a reset or `restoreItem` brings
    /// it back. Nothing picks it up meanwhile; nobody is credited.
    pub fn hide(&mut self, brick: BrickId) {
        if let Some(item) = self.items.get_mut(&brick) {
            item.available_at = NEVER;
        }
    }
    /// Whether the brick's item is there to be taken this tick.
    pub fn available(&self, brick: BrickId, tick: u64) -> bool {
        self.items.get(&brick).is_some_and(|i| tick >= i.available_at)
    }
}

/// Eight-unit broadphase buckets. Oversized authored shapes use a bounded
/// fallback set rather than expanding untrusted bounds into millions of buckets.
#[derive(Default)]
pub struct ContactIndex {
    bounds: BTreeMap<u64, ItemBounds>,
    buckets: BTreeMap<[i32; 3], BTreeSet<u64>>,
    large: BTreeSet<u64>,
}
fn cells(bounds: ItemBounds) -> Option<Vec<[i32; 3]>> {
    let min = bounds.min.map(|v| (v / 8.).floor() as i32);
    let max = bounds.max.map(|v| (v / 8.).floor() as i32);
    let count = (0..3).fold(1_u64, |n, a| {
        n.saturating_mul((i64::from(max[a]) - i64::from(min[a]) + 1).max(0) as u64)
    });
    if count > 128 {
        return None;
    }
    let mut cells = Vec::with_capacity(count as usize);
    for x in min[0]..=max[0] {
        for y in min[1]..=max[1] {
            for z in min[2]..=max[2] {
                cells.push([x, y, z]);
            }
        }
    }
    Some(cells)
}
impl ContactIndex {
    pub fn insert(&mut self, id: u64, bounds: ItemBounds) {
        if self.bounds.get(&id) == Some(&bounds) {
            return;
        }
        self.remove(id);
        if let Some(cells) = cells(bounds) {
            for cell in cells {
                self.buckets.entry(cell).or_default().insert(id);
            }
        } else {
            self.large.insert(id);
        }
        self.bounds.insert(id, bounds);
    }
    pub fn remove(&mut self, id: u64) {
        if let Some(bounds) = self.bounds.remove(&id) {
            if let Some(cells) = cells(bounds) {
                for cell in cells {
                    if let Some(bucket) = self.buckets.get_mut(&cell) {
                        bucket.remove(&id);
                        if bucket.is_empty() {
                            self.buckets.remove(&cell);
                        }
                    }
                }
            } else {
                self.large.remove(&id);
            }
        }
    }
    pub fn query(&self, bounds: ItemBounds) -> BTreeSet<u64> {
        let Some(cells) = cells(bounds) else {
            return self
                .bounds
                .iter()
                .filter(|(_, b)| b.overlaps(&bounds))
                .map(|(id, _)| *id)
                .collect();
        };
        let mut candidates = self.large.clone();
        for cell in cells {
            if let Some(ids) = self.buckets.get(&cell) {
                candidates.extend(ids);
            }
        }
        candidates.retain(|id| self.bounds[id].overlaps(&bounds));
        candidates
    }
}
