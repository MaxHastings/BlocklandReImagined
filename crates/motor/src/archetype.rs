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
use bri_content::body::{Animations, Sockets};
use bri_content::camera::CameraProfile;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Index into an [`Archetypes`] table. `0` is v20's standard player.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ArchetypeId(pub u16);

/// Most archetypes one table holds: v20's eight plus packages'.
pub const MAX_ARCHETYPES: usize = 256;
/// Most controller module bytes one table carries, all archetypes together.
pub const MAX_CONTROLLER_BYTES: usize = 1024 * 1024;
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
    /// Where items attach, the eye and the hip ([`Sockets`]); each left out
    /// is the Blockhead's node.
    #[serde(default, skip_serializing_if = "Sockets::is_default")]
    pub sockets: Sockets,
    /// Its own movement rules (`crate::controller`): the module the host
    /// runs and sends, so clients predict with exactly the same bytes.
    /// [`Archetypes::bind_controllers`] makes it run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<crate::controller::ControllerSource>,
    /// What it sounds like jumping, hurting and dying.
    #[serde(default, skip_serializing_if = "Sounds::is_standard")]
    pub sounds: Sounds,
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
/// An archetype's own sounds (`PlayerData.jumpSound`, and `painSound` and
/// `deathSound` under `useCustomPainEffects`), each an `AudioProfile` name.
/// None makes the standard player's sound; an empty name makes none, as a
/// Bot_Hole bot's `jumpSound = ""` did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sounds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death: Option<String>,
    /// Footfalls while it walks or runs on the ground, one sound picked
    /// from these each step (v20's Blockhead has none). Every player hears
    /// them where the feet are; clients play them from the poses they draw,
    /// so they cost nothing on the wire.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footsteps: Vec<String>,
    /// Footfalls by the surface kind underfoot (`bri_content::surface`),
    /// in place of `footsteps` on that kind.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub surfaces: std::collections::BTreeMap<String, Vec<String>>,
    /// Ground covered between two footfalls, in units at scale 1. None
    /// steps once per 1.1 standing heights, a stride most bodies take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stride: Option<f32>,
    /// Played where it lands after falling faster than
    /// [`Sounds::LAND_SPEED`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub land: Option<String>,
}
impl Sounds {
    /// Longest sound name, as cues carry it.
    pub const MAX_NAME: usize = 128;
    /// Most footstep variants.
    pub const MAX_FOOTSTEPS: usize = 8;
    /// Most surface kinds with their own footfalls.
    pub const MAX_SURFACES: usize = 16;
    /// The falling speed (units/s) a landing must reach to be heard.
    pub const LAND_SPEED: f32 = 4.0;
    pub fn is_standard(&self) -> bool {
        *self == Self::default()
    }
    fn valid(&self) -> bool {
        [&self.jump, &self.pain, &self.death, &self.land]
            .into_iter()
            .flatten()
            .chain(&self.footsteps)
            .all(|s| s.len() <= Self::MAX_NAME && !s.chars().any(char::is_control))
            && self.footsteps.len() <= Self::MAX_FOOTSTEPS
            && self.footsteps.iter().all(|s| !s.is_empty())
            && self.surfaces.len() <= Self::MAX_SURFACES
            && self.surfaces.iter().all(|(kind, ids)| {
                bri_content::surface::valid_name(kind)
                    && (1..=Self::MAX_FOOTSTEPS).contains(&ids.len())
                    && ids
                        .iter()
                        .all(|s| s.len() <= Self::MAX_NAME && !s.chars().any(char::is_control))
            })
            && self
                .stride
                .is_none_or(|s| s.is_finite() && (0.25..=32.0).contains(&s))
    }
    /// Ground between footfalls for a body `stand_height` tall at `scale`.
    /// The footfalls on surface `kind`.
    pub fn footsteps_on(&self, kind: &str) -> &[String] {
        self.surfaces.get(kind).unwrap_or(&self.footsteps)
    }
    pub fn stride_for(&self, stand_height: f32, scale: f32) -> f32 {
        self.stride.unwrap_or(stand_height * 1.1) * scale
    }
}
/// How clients draw an archetype and place its camera.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    /// Imported bodies can own their crouched idle/movement pose instead of
    /// receiving the dedicated Blockhead crouch thread over it.
    #[serde(
        default,
        skip_serializing_if = "bri_content::body::CrouchComposition::is_overlay"
    )]
    pub crouch_composition: bri_content::body::CrouchComposition,
    /// Model id: `v20.shape.<name>` for v20's shapes (the Blockhead is
    /// `v20.shape.m`, the horse [`HORSE_SHAPE`]), a package model, or
    /// [`NO_BODY`] to draw none.
    pub model: String,
    /// Third-person camera distance behind the eye.
    pub camera_distance: f32,
    /// The view stays first person whatever the camera toggle says, as
    /// v20's `PlayerData::firstPersonOnly` (Tier 2's Light MG gunner).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_person_only: bool,
    /// `thirdPersonOnly`: the camera stays behind the body.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub third_person_only: bool,
    /// The model's clip for each animation role (`idle`, `run`, `sit`...);
    /// a role left out asks for the Blockhead's clip name.
    #[serde(default, skip_serializing_if = "Animations::is_empty")]
    pub animations: Animations,
    /// Its own death clips by the side the killing hit came from, or at
    /// random; none plays the `death` role's clip.
    #[serde(default, skip_serializing_if = "bri_content::body::Deaths::is_empty")]
    pub deaths: bri_content::body::Deaths,
    /// The body's camera: field of view, first-person offset, bob, weapon
    /// sway and third-person pivot. The default is the stock camera.
    #[serde(default, skip_serializing_if = "CameraProfile::is_default")]
    pub camera: CameraProfile,
    /// The model's clip for each of its movement controller's intents
    /// (`crate::controller::ControllerState::intent`): `{"11": "triple"}`
    /// plays the model's `triple` sequence while the controller says 11.
    /// An intent with no clip, or a clip the model lacks, plays the clip
    /// the body's motion picks, as for every body.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub intent_clips: BTreeMap<u8, String>,
    /// A sound (a sound id of the Add-On's packs) heard where the body is
    /// each time its controller changes to that intent: the jump, the wall
    /// kick, the ground pound's thud.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub intent_sounds: BTreeMap<u8, String>,
    /// The speed (units/s) at which the clip for each locomotion role (`run`,
    /// `back`, `side`, `crouch_run`...) plays at 1x; the clip then plays
    /// faster or slower with the body's speed, as v20 plays its run by the
    /// ground its sequence covers. A role left out uses its clip's own
    /// ground motion, and a clip without any keeps its own time.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ground_speeds: bri_content::body::GroundSpeeds,
    /// The fastest a locomotion clip plays for its speed (v20's
    /// `maxTimeScale`, 1.5).
    #[serde(
        default = "default_max_time_scale",
        skip_serializing_if = "is_default_max_time_scale"
    )]
    pub max_time_scale: f32,
}
/// v20's `PlayerData::maxTimeScale`.
pub const MAX_TIME_SCALE: f32 = 1.5;
fn default_max_time_scale() -> f32 {
    MAX_TIME_SCALE
}
fn is_default_max_time_scale(v: &f32) -> bool {
    *v == MAX_TIME_SCALE
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
                && self
                    .look
                    .intent_clips
                    .values()
                    .chain(self.look.intent_sounds.values())
                    .all(|name| {
                        !name.is_empty() && name.len() <= 160 && !name.chars().any(char::is_control)
                    })
                && self.look.max_time_scale.is_finite()
                && (0.1..=16.0).contains(&self.look.max_time_scale)
                && self.sounds.valid()
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
        self.sockets.validate()?;
        bri_content::body::validate_animations(&self.look.animations)?;
        self.look.deaths.validate()?;
        bri_content::body::validate_ground_speeds(&self.look.ground_speeds)?;
        self.look.camera.validate()?;
        if let Some(source) = &self.controller {
            source.validate()?;
        }
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
                crouch_composition: Default::default(),
                model: if kind == PlayerType::Horse {
                    HORSE_SHAPE.into()
                } else {
                    "v20.shape.m".into()
                },
                camera_distance: 8.0,
                first_person_only: false,
                third_person_only: false,
                animations: Animations::new(),
                deaths: Default::default(),
                camera: CameraProfile::default(),
                intent_clips: BTreeMap::new(),
                intent_sounds: BTreeMap::new(),
                ground_speeds: Default::default(),
                max_time_scale: MAX_TIME_SCALE,
            },
            uses_items: true,
            sockets: Sockets::default(),
            controller: None,
            sounds: Sounds {
                jump: (kind == PlayerType::Horse).then(|| "HorseJumpSound".into()),
                ..Default::default()
            },
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
    /// [`Self::eye`], or the `sit` pose's when `seated`.
    pub fn posed_eye(&self, state: &crate::player::PlayerState, seated: bool) -> glam::Vec3 {
        state.posed_eye(&self.tuning(state.archetype, state.scale), seated)
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
    /// The bodies a player may be offered (a brick's ChangeDatablock list,
    /// the mini-game's player types, `/body`): v20's selectable player
    /// types, then every package archetype with a name, in table order.
    pub fn choosable(&self) -> impl Iterator<Item = (ArchetypeId, &Archetype)> {
        PlayerType::ALL
            .into_iter()
            .map(|t| (t.archetype(), self.resolve(t.archetype())))
            .chain(
                self.iter()
                    .skip(PlayerType::EVERY.len())
                    .filter(|(_, a)| !a.name.is_empty()),
            )
    }
    /// The choosable body a player names: by archetype id, v20 datablock
    /// name or shown name, ignoring case.
    pub fn choice(&self, named: &str) -> Option<ArchetypeId> {
        let named = named.trim();
        let by_datablock = PlayerType::from_datablock_name(named).map(PlayerType::archetype);
        self.choosable()
            .find(|(id, a)| {
                Some(*id) == by_datablock
                    || a.id.eq_ignore_ascii_case(named)
                    || a.name.eq_ignore_ascii_case(named)
            })
            .map(|(id, _)| id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (ArchetypeId, &Archetype)> {
        self.0
            .iter()
            .enumerate()
            .map(|(i, a)| (ArchetypeId(i as u16), a))
    }
    /// Make every archetype's controller run: `compile` turns a module into
    /// a controller (the engine's WebAssembly host), once per distinct
    /// module. An archetype whose module does not compile moves by the
    /// motor alone; its id and the reason are returned, for the host to
    /// refuse the Add-On and a client to report.
    pub fn bind_controllers(
        &mut self,
        compile: &mut dyn FnMut(
            &crate::controller::ControllerSource,
        ) -> Result<crate::controller::Controller>,
    ) -> Vec<(String, anyhow::Error)> {
        let mut built: Vec<(
            &crate::controller::ControllerSource,
            crate::controller::Controller,
        )> = Vec::new();
        let mut failed = Vec::new();
        let mut bound = Vec::with_capacity(self.0.len());
        for archetype in &self.0 {
            let Some(source) = &archetype.controller else {
                bound.push(None);
                continue;
            };
            if let Some((_, c)) = built.iter().find(|(s, _)| *s == source) {
                bound.push(Some(c.clone()));
                continue;
            }
            match compile(source) {
                Ok(c) => {
                    built.push((source, c.clone()));
                    bound.push(Some(c));
                }
                Err(e) => {
                    failed.push((archetype.id.clone(), e));
                    bound.push(None);
                }
            }
        }
        for (archetype, controller) in self.0.iter_mut().zip(bound) {
            archetype.movement.controller = controller;
        }
        failed
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
        ensure!(
            self.0
                .iter()
                .filter_map(|a| a.controller.as_ref())
                .map(|c| c.module.len())
                .sum::<usize>()
                <= MAX_CONTROLLER_BYTES,
            "Archetype controllers exceed {MAX_CONTROLLER_BYTES} bytes"
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

    #[test]
    fn the_horse_jumps_with_its_own_sound() {
        let table = Archetypes::default();
        let horse = table.resolve(PlayerType::Horse.archetype());
        assert_eq!(horse.sounds.jump.as_deref(), Some("HorseJumpSound"));
        let standard = table.resolve(PlayerType::Standard.archetype());
        assert!(standard.sounds.is_standard());
        let mut loud = horse.clone();
        loud.sounds.pain = Some("x".repeat(Sounds::MAX_NAME + 1));
        assert!(loud.validate().is_err());
    }
}
