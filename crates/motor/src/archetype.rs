//! Player archetypes: what a player is. Movement, collision body, health,
//! mounting rules and look are data, so a package can declare a body the
//! engine has never seen (stress campaign W14). v20's datablocks are the
//! first entries of every table; packages append theirs. Both sides hold the
//! same table (the server sends it with the checkpoint), and a player's state
//! names its archetype by index, so client prediction moves every archetype
//! exactly as the server does.
use crate::player::PlayerTuning;
use crate::player_types::PlayerType;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Index into an [`Archetypes`] table. `0` is v20's standard player.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ArchetypeId(pub u16);

/// Most archetypes one table holds: v20's eight plus packages'.
pub const MAX_ARCHETYPES: usize = 256;
/// Most riders one rideable archetype seats.
pub const MAX_MOUNT_POINTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Archetype {
    /// Content id: `v20.player.<datablock>` or `<package>:archetype/<name>`.
    pub id: String,
    /// Shown in menus; empty for archetypes a player cannot pick.
    pub name: String,
    /// Motor constants and collision body at scale 1.
    pub movement: PlayerTuning,
    pub max_health: f32,
    /// The HUD shows jet energy.
    pub energy_bar: bool,
    /// Other players may mount it.
    pub rideable: bool,
    /// It may mount vehicles and rideable players.
    pub can_ride: bool,
    /// Where riders sit (`numMountPoints`, `mountNode[i]`, `mountThread[i]`),
    /// in seat order. A rideable archetype with none cannot be mounted.
    #[serde(default)]
    pub mount_points: Vec<MountPoint>,
    pub look: Look,
    /// It fires, switches and uses items (`PlayerData::onTrigger` and
    /// `serverCmdUseTool` running as usual). A frozen pre-round body
    /// (Slayer's `PlayerFrozenArmor`) does not.
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub uses_items: bool,
}
fn yes() -> bool {
    true
}
fn is_yes(v: &bool) -> bool {
    *v
}
/// One rider's seat on a rideable archetype.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountPoint {
    /// The model's mount node, which clients follow as it animates.
    pub node: String,
    /// The node's rest position from the feet, facing -Z, at scale 1. The
    /// host seats the rider here.
    pub position: [f32; 3],
    /// The rider's action thread while seated (`mountThread`): `root`
    /// stands, `sit` sits.
    pub pose: String,
}
impl MountPoint {
    /// The seat in the world for a mount at `feet`, facing `yaw`.
    pub fn seat(&self, feet: glam::Vec3, yaw: f32, scale: f32) -> glam::Vec3 {
        feet + glam::Quat::from_rotation_y(-yaw) * glam::Vec3::from(self.position) * scale
    }
}
/// How clients draw an archetype and place its camera.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    /// Model id: `v20.shape.<name>` for v20's shapes (the Blockhead is
    /// `v20.shape.m`, the horse [`HORSE_SHAPE`]), a package model, or
    /// [`NO_BODY`] to draw none.
    pub model: String,
    /// Third-person camera distance behind the eye.
    pub camera_distance: f32,
    /// `thirdPersonOnly`: the camera stays behind the body.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub third_person_only: bool,
}
/// v20's horse (`horse.dts`): players and entities with this look draw
/// and carry items as a horse.
pub const HORSE_SHAPE: &str = "v20.shape.horse";
/// No stock body: clients draw nothing for the archetype, so an Add-On's
/// client code can draw the player its own way.
pub const NO_BODY: &str = "none";
impl Look {
    pub fn is_horse(&self) -> bool {
        self.model.eq_ignore_ascii_case(HORSE_SHAPE)
    }
    pub fn hides_body(&self) -> bool {
        self.model.eq_ignore_ascii_case(NO_BODY)
    }
}
impl Archetype {
    pub fn validate(&self) -> Result<()> {
        self.movement.validate()?;
        ensure!(
            !self.id.is_empty()
                && self.id.len() <= 160
                && self.name.len() <= 64
                && !self.name.chars().any(char::is_control)
                && !self.look.model.is_empty()
                && self.look.model.len() <= 160
                && self.max_health.is_finite()
                && (1.0..=100_000.0).contains(&self.max_health)
                && self.look.camera_distance.is_finite()
                && (0.0..=64.0).contains(&self.look.camera_distance)
                && self.mount_points.len() <= MAX_MOUNT_POINTS
                && self.mount_points.iter().all(|m| {
                    !m.node.is_empty()
                        && m.node.len() <= 64
                        && !m.pose.is_empty()
                        && m.pose.len() <= 64
                        && m.position.iter().all(|v| v.is_finite() && v.abs() <= 64.0)
                }),
            "Invalid archetype {}",
            self.id
        );
        Ok(())
    }
    fn v20(kind: PlayerType) -> Self {
        Self {
            id: kind.id().into(),
            name: kind.name().into(),
            movement: kind.tuning(),
            max_health: kind.max_health(),
            energy_bar: kind.shows_energy(),
            rideable: kind.rideable(),
            can_ride: kind.can_ride(),
            mount_points: kind.mount_points(),
            look: Look {
                model: if kind == PlayerType::Horse {
                    HORSE_SHAPE.into()
                } else {
                    "v20.shape.m".into()
                },
                camera_distance: 8.0,
                third_person_only: false,
            },
            uses_items: true,
        }
    }
}

/// Every archetype a session knows, v20's first in [`PlayerType`] order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Archetypes(Vec<Archetype>);
impl Default for Archetypes {
    fn default() -> Self {
        Self(PlayerType::EVERY.into_iter().map(Archetype::v20).collect())
    }
}
impl Archetypes {
    /// Append an archetype; ids are unique.
    pub fn add(&mut self, archetype: Archetype) -> Result<ArchetypeId> {
        archetype.validate()?;
        ensure!(
            self.0.len() < MAX_ARCHETYPES,
            "At most {MAX_ARCHETYPES} archetypes"
        );
        ensure!(
            self.find(&archetype.id).is_none(),
            "Archetype {} is declared twice",
            archetype.id
        );
        self.0.push(archetype);
        Ok(ArchetypeId((self.0.len() - 1) as u16))
    }
    /// Change an archetype in place, keeping its id and index: players of
    /// it, and saves naming it, carry on with the new constants.
    pub fn replace(&mut self, id: ArchetypeId, archetype: Archetype) -> Result<()> {
        archetype.validate()?;
        let slot = self
            .0
            .get_mut(usize::from(id.0))
            .ok_or_else(|| anyhow::anyhow!("No archetype {}", id.0))?;
        ensure!(
            slot.id == archetype.id,
            "Archetype {} cannot become {}",
            slot.id,
            archetype.id
        );
        *slot = archetype;
        Ok(())
    }
    pub fn get(&self, id: ArchetypeId) -> Option<&Archetype> {
        self.0.get(usize::from(id.0))
    }
    /// The archetype, or the standard player for an id this table lacks.
    pub fn resolve(&self, id: ArchetypeId) -> &Archetype {
        self.get(id).unwrap_or(&self.0[0])
    }
    pub fn find(&self, id: &str) -> Option<ArchetypeId> {
        self.0
            .iter()
            .position(|a| a.id.eq_ignore_ascii_case(id))
            .map(|i| ArchetypeId(i as u16))
    }
    /// Motor constants for a player of this archetype and scale.
    pub fn tuning(&self, id: ArchetypeId, scale: f32) -> PlayerTuning {
        self.resolve(id).movement.clone().scaled(scale)
    }
    /// Where a player of this state's archetype and scale sees from.
    pub fn eye(&self, state: &crate::player::PlayerState) -> glam::Vec3 {
        state.eye(&self.tuning(state.archetype, state.scale))
    }
    /// Give every archetype drawn with `model` that declares no mount
    /// points that model's (its `mount<N>` nodes): a body need not be
    /// rideable for a rule to seat someone on it (`mountObject`).
    pub fn fill_mount_points(&mut self, model: &str, points: &[MountPoint]) -> Result<()> {
        ensure!(
            points.len() <= MAX_MOUNT_POINTS,
            "At most {MAX_MOUNT_POINTS} mount points"
        );
        for archetype in &mut self.0 {
            if archetype.look.model == model && archetype.mount_points.is_empty() {
                archetype.mount_points = points.to_vec();
                archetype.validate()?;
            }
        }
        Ok(())
    }
    pub fn iter(&self) -> impl Iterator<Item = (ArchetypeId, &Archetype)> {
        self.0
            .iter()
            .enumerate()
            .map(|(i, a)| (ArchetypeId(i as u16), a))
    }
    pub fn highest_max_health(&self) -> f32 {
        self.0.iter().map(|a| a.max_health).fold(0.0, f32::max)
    }
    /// A table received from a host: v20's entries first, every entry valid.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.0.len() >= PlayerType::EVERY.len() && self.0.len() <= MAX_ARCHETYPES,
            "Invalid archetype table"
        );
        for (i, a) in self.0.iter().enumerate() {
            a.validate()?;
            ensure!(
                self.find(&a.id) == Some(ArchetypeId(i as u16)),
                "Archetype {} is declared twice",
                a.id
            );
        }
        for kind in PlayerType::EVERY {
            ensure!(
                self.get(kind.archetype())
                    .is_some_and(|a| a.id == kind.id()),
                "Archetype table does not start with v20's"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v20_player_types_keep_their_index() {
        let table = Archetypes::default();
        table.validate().unwrap();
        for kind in PlayerType::EVERY {
            assert_eq!(table.find(kind.id()), Some(kind.archetype()));
            assert_eq!(table.resolve(kind.archetype()).movement, kind.tuning());
        }
    }

    #[test]
    fn a_package_archetype_is_appended_once() {
        let mut table = Archetypes::default();
        let mut moon = Archetype::v20(PlayerType::Standard);
        moon.id = "moon:archetype/moon".into();
        moon.movement.gravity = 3.4;
        let id = table.add(moon.clone()).unwrap();
        assert_eq!(usize::from(id.0), PlayerType::EVERY.len());
        assert_eq!(table.tuning(id, 1.0).gravity, 3.4);
        assert!(table.add(moon.clone()).is_err());
        moon.id = "moon:archetype/broken".into();
        moon.movement.gravity = f32::NAN;
        assert!(table.add(moon).is_err());
        let json = serde_json::to_string(&table).unwrap();
        let back: Archetypes = serde_json::from_str(&json).unwrap();
        back.validate().unwrap();
        assert_eq!(back, table);
    }
}
