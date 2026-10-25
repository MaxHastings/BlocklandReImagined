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
    /// What a shot can meet of the body, one cylinder per bone with its
    /// damage multiplier ([`crate::bones`]). None: it is hit as one box.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hit_bones: Vec<bri_content::hit_bones::HitBone>,
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
    /// With no `land` sound, a landing sounds like a footfall on the surface
    /// underfoot (AA's `PlayLanded` plays the floor material's foot-contact
    /// sound).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub land_on_surface: bool,
    /// Sounds tied to a clip (lower-case name): when a body starts the clip
    /// the sound plays at the body once the clip has run `seconds`. A death
    /// clip's body-fall is one.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub clips: std::collections::BTreeMap<String, ClipSound>,
    /// Footfalls the clips themselves mark (lower-case clip name to the
    /// moments its feet land), in place of the stride timer: the body steps
    /// when a marked moment of the clip it is playing goes by, on the surface
    /// under that foot, so the sound follows the animation. A body with none
    /// keeps the timer (v20's Blockhead).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub steps: std::collections::BTreeMap<String, Vec<StepEvent>>,
    /// Frames a second of the clips, for `steps` marked by `frame`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<f32>,
    /// How loud each marked footfall is to those who listen (bots hear it as
    /// they hear fighting), 0 to 1: the same whatever the gait, since the gait
    /// only shows in how often the clip's marks come. A mark can say its own
    /// (`StepEvent::noise`). None: footfalls make no noise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_noise: Option<f32>,
    /// The noise of landing from a fall: `factor` times the speed fallen
    /// over the larger of the body's jump speed and `jump_floor`, so a fall
    /// at jump speed is `factor` and faster ones are louder (capped at 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub land_noise: Option<LandNoise>,
    /// A sound for entering each posture (`stand`, `crouch`, `prone`), heard
    /// where the body is when its posture changes.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub postures: std::collections::BTreeMap<String, String>,
    /// Taking off in a jump sounds like a footfall on the surface underfoot,
    /// once, as landing does with `land_on_surface`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub takeoff_on_surface: bool,
    /// What a footfall leaves on the surface it lands on, by surface kind
    /// (`*` for any other): a mark on the ground and a burst of dust. A kind
    /// listed nowhere leaves nothing.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub contacts: std::collections::BTreeMap<String, FootContact>,
}
/// [`Sounds::land_noise`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandNoise {
    pub factor: f32,
    pub jump_floor: f32,
}
/// What one footfall leaves on a surface ([`Sounds::contacts`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootContact {
    /// An effect (a dust puff) burst where the foot lands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    /// A mark left on the ground under it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decal: Option<FootDecal>,
}
/// A footprint ([`FootContact::decal`]): the mark's texture, size across in
/// units at scale 1, seconds it stays and its colour and opacity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootDecal {
    pub texture: String,
    pub size: f32,
    pub seconds: f32,
    #[serde(default = "FootDecal::white")]
    pub color: [f32; 4],
}
impl FootDecal {
    fn white() -> [f32; 4] {
        [1.0; 4]
    }
}
/// One footfall a clip marks ([`Sounds::steps`]): exactly one of `seconds`
/// or `frame` says when.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepEvent {
    /// Seconds into the clip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f32>,
    /// A frame of the clip, counted at [`Sounds::fps`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<u32>,
    /// The body's node for the foot that lands; the step sounds where it is
    /// and on the surface under it. None: at the body's feet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foot: Option<String>,
    /// Another sound source in place of the surface's footfall: a ladder's
    /// climb sound, say. Heard at `volume` times its own out to `radius`
    /// (0 or none: its own), and it leaves no mark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f32>,
    /// How loud this mark is to those who listen, 0 to 1, in place of
    /// [`Sounds::step_noise`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise: Option<f32>,
}
/// A sound tied to the moment of a clip ([`Sounds::clips`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipSound {
    /// The sound profile.
    pub sound: String,
    /// Seconds into the clip.
    pub seconds: f32,
}
impl Sounds {
    /// Most clips with a sound of their own.
    pub const MAX_CLIPS: usize = 32;
    /// Longest sound name, as cues carry it.
    pub const MAX_NAME: usize = 128;
    /// Most footstep variants.
    pub const MAX_FOOTSTEPS: usize = 8;
    /// Most footfalls one clip marks.
    pub const MAX_STEP_EVENTS: usize = 16;
    /// Most surface kinds with their own footfalls.
    pub const MAX_SURFACES: usize = 16;
    /// The postures a body enters, as `postures` names them.
    pub const POSTURES: [&'static str; 3] = ["stand", "crouch", "prone"];
    /// Most surface kinds (and `*`) with footfall marks and dust.
    pub const MAX_CONTACTS: usize = 17;
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
            && self.clips.len() <= Self::MAX_CLIPS
            && self.clips.iter().all(|(clip, c)| {
                !clip.is_empty()
                    && clip.len() <= 64
                    && !clip.chars().any(char::is_control)
                    && !c.sound.is_empty()
                    && c.sound.len() <= Self::MAX_NAME
                    && !c.sound.chars().any(char::is_control)
                    && c.seconds.is_finite()
                    && (0.0..=60.0).contains(&c.seconds)
            })
            && self
                .step_noise
                .is_none_or(|n| n.is_finite() && (0.0..=1.0).contains(&n))
            && self.land_noise.as_ref().is_none_or(|l| {
                l.factor.is_finite()
                    && (0.0..=10.0).contains(&l.factor)
                    && l.jump_floor.is_finite()
                    && l.jump_floor > 0.0
            })
            && self.postures.len() <= Self::POSTURES.len()
            && self.postures.iter().all(|(posture, sound)| {
                Self::POSTURES.contains(&posture.as_str())
                    && !sound.is_empty()
                    && sound.len() <= Self::MAX_NAME
                    && !sound.chars().any(char::is_control)
            })
            && self.contacts.len() <= Self::MAX_CONTACTS
            && self.contacts.iter().all(|(kind, c)| {
                (kind == "*" || bri_content::surface::valid_name(kind))
                    && c.effect.as_ref().is_none_or(|e| {
                        !e.is_empty()
                            && e.len() <= Self::MAX_NAME
                            && !e.chars().any(char::is_control)
                    })
                    && c.decal.as_ref().is_none_or(|d| {
                        !d.texture.is_empty()
                            && d.texture.len() <= Self::MAX_NAME
                            && !d.texture.chars().any(char::is_control)
                            && d.size.is_finite()
                            && (0.02..=4.0).contains(&d.size)
                            && d.seconds.is_finite()
                            && (0.5..=600.0).contains(&d.seconds)
                            && d.color
                                .iter()
                                .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
                    })
            })
            && self.steps.len() <= Self::MAX_CLIPS
            && self
                .fps
                .is_none_or(|f| f.is_finite() && (1.0..=240.0).contains(&f))
            && self.steps.iter().all(|(clip, events)| {
                !clip.is_empty()
                    && clip.len() <= 64
                    && *clip == clip.to_ascii_lowercase()
                    && !clip.chars().any(char::is_control)
                    && (1..=Self::MAX_STEP_EVENTS).contains(&events.len())
                    && events.iter().all(|e| {
                        e.foot.as_ref().is_none_or(|f| {
                            !f.is_empty() && f.len() <= 64 && !f.chars().any(char::is_control)
                        }) && e.sound.as_ref().is_none_or(|s| {
                            !s.is_empty()
                                && s.len() <= Self::MAX_NAME
                                && !s.chars().any(char::is_control)
                        }) && e
                            .volume
                            .is_none_or(|v| v.is_finite() && (0.0..=4.0).contains(&v))
                            && e.radius
                                .is_none_or(|r| r.is_finite() && (0.0..=10_000.0).contains(&r))
                            && (e.sound.is_some() || (e.volume.is_none() && e.radius.is_none()))
                            && match (e.seconds, e.frame) {
                                (Some(s), None) => s.is_finite() && (0.0..=60.0).contains(&s),
                                (None, Some(_)) => self.fps.is_some(),
                                _ => false,
                            }
                    })
            })
    }
    /// When in its clip `event` lands, in seconds.
    pub fn step_time(&self, event: &StepEvent) -> Option<f32> {
        match (event.seconds, event.frame, self.fps) {
            (Some(s), None, _) => Some(s),
            (None, Some(frame), Some(fps)) => Some(frame as f32 / fps),
            _ => None,
        }
    }
    /// Ground between footfalls for a body `stand_height` tall at `scale`.
    /// The footfalls on surface `kind`.
    pub fn footsteps_on(&self, kind: &str) -> &[String] {
        self.surfaces.get(kind).unwrap_or(&self.footsteps)
    }
    /// How loud landing at `fall` units/s is for a body whose jump speed is
    /// `jump`, 0 to 1 ([`Sounds::land_noise`]).
    pub fn land_noise_for(&self, fall: f32, jump: f32) -> f32 {
        self.land_noise.as_ref().map_or(0.0, |l| {
            bri_console::Clamp::clamped(l.factor * fall / jump.max(l.jump_floor), 0.0, 1.0)
        })
    }
    /// What a footfall on surface `kind` leaves, if anything.
    pub fn contact_on(&self, kind: &str) -> Option<&FootContact> {
        self.contacts.get(kind).or_else(|| self.contacts.get("*"))
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
    /// Seconds a thread action a rule starts or ends (`play_thread` on a body
    /// thread, a posture change) blends from the pose before it, like the
    /// locomotion's own transition: AA's `BlendChangeTime`, .25. 0 (v20's
    /// bodies) cuts, 0 to 2.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub action_blend: f32,
    /// How the ground locomotion clip is picked and timed: v20's, or Army Ops' four-way pick.
    #[serde(
        default,
        skip_serializing_if = "bri_content::body::LocomotionPick::is_torque"
    )]
    pub locomotion_pick: bri_content::body::LocomotionPick,
    /// Seconds a dead body lies before it is gone (v20's `$CorpseTimeoutValue`,
    /// 5): the server clears its collision then and clients stop drawing it.
    #[serde(
        default = "default_corpse_seconds",
        skip_serializing_if = "is_default_corpse_seconds"
    )]
    pub corpse_seconds: f32,
    /// The last of `corpse_seconds` the body fades out over, instead of
    /// vanishing in v20's puff; 0 vanishes at once.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub corpse_fade: f32,
}
fn is_zero(v: &f32) -> bool {
    *v == 0.0
}
pub use bri_content::body::{CORPSE_SECONDS, MAX_CORPSE_SECONDS};
fn default_corpse_seconds() -> f32 {
    CORPSE_SECONDS
}
fn is_default_corpse_seconds(v: &f32) -> bool {
    *v == CORPSE_SECONDS
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
    /// How much of a body that has lain dead `lying` seconds still shows:
    /// `None` once `corpse_seconds` are up, otherwise its opacity, 1 until
    /// `corpse_fade` before the end and falling to 0 across it.
    pub fn corpse_opacity(&self, lying: f32) -> Option<f32> {
        let left = self.corpse_seconds - lying;
        if left <= 0.0 {
            return None;
        }
        Some(if self.corpse_fade > 0.0 {
            (left / self.corpse_fade).min(1.0)
        } else {
            1.0
        })
    }
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
                && self.look.action_blend.is_finite()
                && (0.0..=2.0).contains(&self.look.action_blend)
                && self.look.corpse_seconds.is_finite()
                && (0.0..=MAX_CORPSE_SECONDS).contains(&self.look.corpse_seconds)
                && self.look.corpse_fade.is_finite()
                && (0.0..=self.look.corpse_seconds).contains(&self.look.corpse_fade)
                && self.sounds.valid()
                && self.hit_bones.len() <= bri_content::hit_bones::MAX_HIT_BONES
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
        for bone in &self.hit_bones {
            bone.validate()?;
        }
        bri_content::body::validate_animations(&self.look.animations)?;
        self.look.deaths.validate()?;
        if !self.look.deaths.is_empty() {
            ensure!(
                !self.look.deaths.crouched.is_empty(),
                "deaths needs `crouched` clips: a body that dies crouched must not fall as a standing one"
            );
            ensure!(
                self.movement.prone.is_none() || !self.look.deaths.prone.is_empty(),
                "deaths needs `prone` clips: this body can lie prone"
            );
        }
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
            hit_bones: Vec::new(),
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
                action_blend: 0.0,
                locomotion_pick: Default::default(),
                corpse_seconds: CORPSE_SECONDS,
                corpse_fade: 0.0,
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
    fn a_body_fades_over_the_end_of_its_corpse_seconds() {
        let mut look = Archetype::v20(PlayerType::Standard).look;
        // v20: whole for five seconds, then gone.
        assert_eq!(look.corpse_opacity(4.9), Some(1.0));
        assert_eq!(look.corpse_opacity(5.0), None);
        look.corpse_seconds = 20.0;
        look.corpse_fade = 4.0;
        assert_eq!(look.corpse_opacity(15.0), Some(1.0));
        assert_eq!(look.corpse_opacity(18.0), Some(0.5));
        assert_eq!(look.corpse_opacity(20.0), None);
        // A fade longer than the time the body lies is refused.
        let mut table = Archetypes::default();
        let mut lasting = Archetype::v20(PlayerType::Standard);
        lasting.id = "moon:archetype/lasting".into();
        lasting.look = look;
        lasting.look.corpse_fade = 21.0;
        assert!(table.add(lasting).is_err());
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
    #[test]
    fn a_clip_sound_names_a_sound_and_a_moment_in_the_clip() {
        let table = Archetypes::default();
        let mut body = table.resolve(PlayerType::Horse.archetype()).clone();
        let clip = |sound: &str, seconds: f32| ClipSound {
            sound: sound.into(),
            seconds,
        };
        body.sounds.clips.insert("fall".into(), clip("thud", 0.6));
        assert!(body.validate().is_ok());
        for bad in [clip("", 0.6), clip("thud", -1.0), clip("thud", f32::NAN)] {
            body.sounds.clips.insert("fall".into(), bad);
            assert!(body.validate().is_err());
        }
    }
}
