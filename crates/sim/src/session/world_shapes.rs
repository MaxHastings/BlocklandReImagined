//! Boxes Add-Ons draw in the world for every player (`show_shapes`, the
//! `effects` capability): translucent faces, frames and a label, as
//! Torque Add-Ons draw with scaled `StaticShape`s (the New Duplicator's
//! selection box). The host keeps each package's sets by key and
//! replicates the sets that changed; a joiner gets them all. A set that
//! goes with a player goes when they leave.
use super::*;
use bri_package_runtime::ops::{MAX_SHAPES, WorldShape, shape_key};

/// Most sets of shapes a server keeps at once.
pub const MAX_SHAPE_SETS: usize = 1024;

#[derive(Debug, Default)]
pub(super) struct WorldShapes {
    /// By `package/key` (`package/owner/key` for a player's own).
    sets: BTreeMap<String, (Option<OwnerId>, Arc<Vec<WorldShape>>)>,
    /// Counts changes, so the host compares sets only when one changed.
    revision: u64,
}

/// Shape checks for a set from the network.
pub fn check_world_shapes(key: &str, shapes: &[WorldShape]) -> Result<()> {
    ensure!(
        key.len() <= 256 && !key.is_empty() && !key.chars().any(char::is_control),
        "Invalid world shape key"
    );
    ensure!(
        shapes.len() <= MAX_SHAPES && shapes.iter().all(WorldShape::check),
        "Invalid world shapes"
    );
    Ok(())
}

impl Session {
    /// Every set of world shapes now, by key.
    pub fn world_shapes(&self) -> BTreeMap<String, Arc<Vec<WorldShape>>> {
        self.world_shapes
            .sets
            .iter()
            .map(|(k, (_, s))| (k.clone(), s.clone()))
            .collect()
    }
    /// Changes when any set does.
    pub fn world_shapes_revision(&self) -> u64 {
        self.world_shapes.revision
    }
    /// Replace `package`'s set `key` (with `owner`'s, if given); an empty
    /// one takes it away.
    pub(super) fn show_shapes(
        &mut self,
        package: &str,
        owner: Option<OwnerId>,
        key: &str,
        shapes: Vec<WorldShape>,
    ) -> Result<()> {
        ensure!(shape_key(key), "Invalid shape key {key:?}");
        if let Some(owner) = owner {
            ensure!(self.peers.contains_key(&owner), "No such player");
        }
        let full = match owner {
            Some(owner) => format!("{package}/{owner}/{key}"),
            None => format!("{package}/{key}"),
        };
        let sets = &mut self.world_shapes.sets;
        if shapes.is_empty() {
            if sets.remove(&full).is_some() {
                self.world_shapes.revision += 1;
            }
            return Ok(());
        }
        check_world_shapes(&full, &shapes)?;
        if sets.get(&full).is_some_and(|(_, s)| **s == shapes) {
            return Ok(());
        }
        ensure!(
            sets.contains_key(&full) || sets.len() < MAX_SHAPE_SETS,
            "Too many sets of world shapes: hide ones no longer shown"
        );
        sets.insert(full, (owner, Arc::new(shapes)));
        self.world_shapes.revision += 1;
        Ok(())
    }
    /// A leaving player's sets go with them.
    pub(super) fn forget_world_shapes(&mut self, owner: OwnerId) {
        let before = self.world_shapes.sets.len();
        self.world_shapes.sets.retain(|_, (o, _)| *o != Some(owner));
        if self.world_shapes.sets.len() != before {
            self.world_shapes.revision += 1;
        }
    }
}
