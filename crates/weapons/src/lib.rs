//! Versioned native weapon content. No legacy parser is linked into this crate.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub mod debris;
mod merge;
pub mod rotation;
pub use merge::{resource_root, sound_root};
pub mod runtime;
pub use runtime::*;
/// 3 adds explosion vertical impulse and per-type vehicle damage scale.
pub const SCHEMA: u32 = 3;
pub const TICK_HZ: u32 = 120;
/// Authored DTS object bounds converted offline to native coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ItemBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}
impl ItemBounds {
    /// Half a unit each way: the stand-in box for an item whose model gave
    /// no bounds (an Add-On item with no art, or one imported without its
    /// item physics), so a gap in presentation never refuses the item.
    pub const FALLBACK: Self = Self {
        min: [-0.25; 3],
        max: [0.25; 3],
    };
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (0..3).all(|a| self.min[a].is_finite()
                && self.max[a].is_finite()
                && self.min[a] <= self.max[a]
                && self.min[a].abs() <= 1000.
                && self.max[a].abs() <= 1000.),
            "Invalid authored item bounds"
        );
        Ok(())
    }
    pub fn transformed(&self, position: glam::Vec3, rotation: glam::Quat) -> Self {
        let mut min = glam::Vec3::splat(f32::INFINITY);
        let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
        for bits in 0..8 {
            let p = glam::Vec3::from_array(std::array::from_fn(|axis| {
                if bits & (1 << axis) == 0 {
                    self.min[axis]
                } else {
                    self.max[axis]
                }
            }));
            let p = position + rotation * p;
            min = min.min(p);
            max = max.max(p);
        }
        Self {
            min: min.to_array(),
            max: max.to_array(),
        }
    }
    /// Depth of a rotated box's lowest corner below its centre.
    pub fn lowest(half: glam::Vec3, rotation: glam::Quat) -> f32 {
        let m = glam::Mat3::from_quat(rotation);
        (m.row(1).abs() * half).element_sum()
    }
    pub fn overlaps(&self, other: &Self) -> bool {
        (0..3).all(|a| self.min[a] <= other.max[a] && self.max[a] >= other.min[a])
    }
}
/// Arms (right, left) raised by an image's script instead of its `armReady`
/// field: `onMount`/`onCharge` calls to `playThread(1, armReady*)` in the
/// Akimbo Guns and Item_Sports scripts. Other images follow `armReady`.
pub fn scripted_arm_pose(image: &str, state: &str) -> Option<(bool, bool)> {
    let name = image
        .rsplit('.')
        .next()
        .unwrap_or(image)
        .to_ascii_lowercase();
    match name.as_str() {
        "lefthandedgunimage" | "basketballshootimage" | "dodgeballimage" => Some((true, true)),
        "basketballimage" => Some((true, false)),
        "footballimage" if matches!(state, "Charge" | "Armed") => Some((true, false)),
        _ => None,
    }
}
pub fn native_id(kind: &str, name: &str) -> String {
    format!("v20.{kind}.{}", name.to_ascii_lowercase())
}
/// `setSprayCanColor` copies each `bluePaint*` datablock as
/// `color<N>Paint*` for palette index N. Effects carry that name so the
/// presentation tints the blue can's particles with the palette colour.
pub fn paint_effect(definition: &str, paint: Option<u8>) -> String {
    match (paint, definition.get(..9)) {
        (Some(paint), Some(prefix)) if prefix.eq_ignore_ascii_case("bluepaint") => {
            format!("color{paint}Paint{}", &definition[9..])
        }
        _ => definition.to_owned(),
    }
}
/// Inverse of [`paint_effect`]: the palette index and the `bluePaint*` base.
pub fn paint_effect_base(definition: &str) -> Option<(u8, String)> {
    let rest = definition
        .get(..5)?
        .eq_ignore_ascii_case("color")
        .then(|| &definition[5..])?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let paint = rest[..digits].parse().ok()?;
    let suffix = rest[digits..]
        .get(..5)?
        .eq_ignore_ascii_case("paint")
        .then(|| &rest[digits + 5..])?;
    Some((paint, format!("bluePaint{suffix}")))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub path: String,
    pub sha256: String,
    pub line: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub class: String,
    pub parent: Option<String>,
    pub source: Evidence,
    pub fields: BTreeMap<String, String>,
}
/// One state of an image's state machine (`stateName[i]` and its fields).
/// Every field may be left out of `weapons.json`: an Add-On writes only
/// what it uses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default = "State::authored")]
pub struct State {
    pub name: String,
    pub ticks: u32,
    pub wait: bool,
    pub allow_change: bool,
    pub timeout: Option<usize>,
    pub down: Option<usize>,
    pub up: Option<usize>,
    pub ammo: Option<usize>,
    pub no_ammo: Option<usize>,
    pub script: String,
    pub sequence: String,
    /// The holder's arm animation (thread 2) played on entering the state,
    /// as v20 scripts did with `playThread(2, armAttack)` in `onPreFire`:
    /// `armattack` for a swing, `root` to stop.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub arm: String,
    pub sound: String,
    pub emitter: String,
    pub emitter_node: String,
    pub emitter_seconds: f32,
    pub eject_shell: bool,
}
impl State {
    /// What a field left out of `weapons.json` means: v20's
    /// `ShapeBaseImageData` defaults, which wait for their timeout and let
    /// the holder switch items.
    pub fn authored() -> Self {
        Self {
            wait: true,
            allow_change: true,
            ..Self::default()
        }
    }
}
fn yes() -> bool {
    true
}
fn is_true(b: &bool) -> bool {
    *b
}
fn zero3(v: &[f32; 3]) -> bool {
    *v == [0.0; 3]
}
/// Aiming with an image: the view zooms to `fov` while the zoom key is
/// held (and, with `on_jet`, while jet, the right mouse button, is held).
/// Presentation only: each player's own game zooms its own view.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zoom {
    /// Horizontal field of view while aiming, 5 to 85 degrees.
    pub fov: f32,
    /// The right mouse button aims too (aim down sights). Give the holder
    /// an archetype that cannot jet, or they jet as well.
    #[serde(default)]
    pub on_jet: bool,
    /// The game's crosshair shows while aiming (a scope draws its own).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub crosshair: bool,
    /// Aiming switches a third-person view to first person until released.
    #[serde(default)]
    pub first_person: bool,
}
/// A sound an Add-On's weapons pack ships: state `sound` fields and rules
/// name it by its key, like a v20 `AudioProfile`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundDef {
    /// A `.wav` or `.ogg` file, relative to the folder `weapons.json` is in.
    pub file: String,
    /// 0 to 1.
    #[serde(default = "full_volume")]
    pub volume: f32,
    /// Plays until its state ends (a charging hum), rather than once.
    #[serde(default)]
    pub looping: bool,
    /// Heard only by the player who caused it, at their ears (a reload
    /// click), rather than by everyone near where it happens.
    #[serde(default)]
    pub local: bool,
    /// Content-root-relative directory of the package holding `file`, set
    /// when packs are merged; None is this pack's own directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}
fn full_volume() -> f32 {
    1.0
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Image {
    pub id: String,
    pub name: String,
    pub model: String,
    pub projectile: Option<String>,
    pub mount_point: u32,
    pub offset: [f32; 3],
    pub eye_offset: [f32; 3],
    /// Euler XYZ degrees in the original Z-up frame, explicitly transformed by presentation.
    pub source_rotation_degrees: [f32; 3],
    pub correct_muzzle: bool,
    pub melee: bool,
    pub color: [f32; 4],
    pub color_shift: bool,
    pub arm_ready: bool,
    pub casing: String,
    pub min_shot_ticks: u32,
    pub states: Vec<State>,
    /// An Add-On tool's image: its `onFire` runs this Add-On command
    /// (`package:command`) for the holder, aimed where they look, instead
    /// of firing a projectile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// More of an Add-On tool's moments that run Add-On commands.
    #[serde(default, skip_serializing_if = "ImageCommands::is_empty")]
    pub commands: ImageCommands,
    /// Several projectiles per shot, spread and recoil. None fires one
    /// projectile straight along the aim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shot: Option<Shot>,
    /// First-person rotation beside `eye_offset` (`eyeRotation`), Euler
    /// XYZ degrees in the same frame as `source_rotation_degrees`.
    #[serde(skip_serializing_if = "zero3")]
    pub eye_rotation: [f32; 3],
    /// Aiming zoom (a scope, or aim down sights).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<Zoom>,
    /// The game's crosshair shows while this image is held.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub crosshair: bool,
    /// In first person an image with an `eye_offset` sits at the eye and
    /// offset alone, as Torque places it, so a scope's sight stays on the
    /// line of sight. `true` also moves it with the arm's actions (shift,
    /// plant, swing), as v20's own tools do.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub follow_arm: bool,
    /// Held, the image takes its holder's spray colour (the palette colour
    /// they last picked) as a colour spray can does: a tool that paints
    /// with that colour shows it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub paint_tint: bool,
    /// While its holder hangs on a rope (`tether`), each player's game
    /// draws the rope with this: v20 Add-Ons fired a stream of projectiles
    /// whose trails drew it, from the muzzle to the rope's end.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rope: Option<Rope>,
}
/// How a held image's rope is drawn ([`Image::rope`]): the trail of the
/// projectile v20 fired along it, swept from the image's muzzle to the
/// rope's anchor every frame, laying as many particles along the rope as
/// that projectile flying it at `speed` would. Presentation only: it costs
/// nothing on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rope {
    /// A projectile of the pack, whose `trail` draws the rope.
    pub projectile: String,
    /// Units a second, 1 to 1000: how fast v20 fired it along the rope.
    pub speed: f32,
}
/// Add-On commands (`package:command`) an image runs for its holder, aimed
/// where they look, beyond `command` (which is `onFire`'s): v20 Add-Ons
/// scripted these in their image's state callbacks and `onTrigger`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageCommands {
    /// By state script, lowercase (`oncharge`, `onfire`, `onabortcharge`):
    /// entering a state with that script runs the command. A charge
    /// (trigger held) and its release are two states of the image.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub states: BTreeMap<String, String>,
    /// Pressing jet while the image is in hand (v20 `onTrigger` slot 4, the
    /// right mouse button). Jetting players still jet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jet: Option<String>,
    /// Pressing the light key while the image is in hand (v20 Add-Ons
    /// packaged `serverCmdLight` for this, often to reload). The light
    /// does not toggle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<String>,
    /// Turning the mouse wheel while the trigger is held down with the
    /// image in hand: the command runs with the notches turned as its one
    /// `int` argument (positive away from the player, as a wheel rolled
    /// forward). With the trigger up the wheel switches tools as always.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wheel: Option<String>,
    /// Pressing the cancel key while the image is in hand (v20 Add-Ons
    /// packaged `serverCmdCancelBrick` for this: a rifle's grenade
    /// launcher, the next kind of round). The key still clears the
    /// player's ghost brick.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel: Option<String>,
}
impl ImageCommands {
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
            && self.jet.is_none()
            && self.light.is_none()
            && self.wheel.is_none()
            && self.cancel.is_none()
    }
    /// Whether the image runs `command` (`package:command`) from any of its
    /// moments: a state, jet, light, wheel or cancel.
    pub fn runs(&self, command: &str) -> bool {
        self.states.values().any(|c| c == command)
            || [&self.jet, &self.light, &self.wheel, &self.cancel]
                .into_iter()
                .any(|c| c.as_deref() == Some(command))
    }
    /// The command for entering a state with `script`, if any.
    pub fn for_script(&self, script: &str) -> Option<&String> {
        if script.is_empty() {
            return None;
        }
        self.states.get(&script.to_ascii_lowercase())
    }
}
/// A well-formed `package:command` an image may name.
pub fn is_image_command(c: &str) -> bool {
    c.len() <= 128
        && c.split_once(':').is_some_and(|(package, command)| {
            !package.is_empty()
                && !command.is_empty()
                && !command.contains(':')
                && c.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-:".contains(&b))
        })
}

/// What v20 Add-Ons scripted in `onFire` with the common spread code
/// (`%shellcount`, `%spread`, `%obj.setVelocity(... getEyeVector() * -n)`):
/// the recoil first, then each projectile's velocity turned by its own
/// random angles.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    /// Projectiles per shot, 1 to 64 (`%shellcount`).
    pub projectiles: u32,
    /// v20's `%spread`: each projectile's velocity turns by random Euler
    /// angles of up to ±5π·spread radians about each axis.
    #[serde(default)]
    pub spread: f32,
    /// Speed the shooter loses along their aim, in units per second.
    #[serde(default)]
    pub recoil: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub ui_name: String,
    pub image: String,
    pub model: String,
    pub icon: String,
    pub can_drop: bool,
    pub sport: bool,
}
impl Default for Item {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            ui_name: String::new(),
            image: String::new(),
            model: String::new(),
            icon: String::new(),
            can_drop: true,
            sport: false,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Explosion {
    pub effect: String,
    pub damage: f32,
    pub radius: f32,
    pub impulse: f32,
    pub impulse_radius: f32,
    /// `impulseVertical`: a straight-up push alongside the radial one.
    pub impulse_vertical: f32,
    pub burn_seconds: f32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BrickImpact {
    pub radius: f32,
    pub direct: bool,
    pub force: f32,
    pub max_volume: f32,
    pub max_floating_volume: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectileDef {
    pub id: String,
    pub name: String,
    pub model: String,
    pub speed: f32,
    pub inherit: f32,
    pub gravity: f32,
    pub lifetime_ticks: u32,
    pub fade_ticks: u32,
    pub arm_ticks: u32,
    pub ballistic: bool,
    pub elasticity: f32,
    pub friction: f32,
    pub damage: f32,
    pub damage_type: String,
    pub radius_damage_type: String,
    pub impulse: f32,
    pub vertical: f32,
    pub explode_player: bool,
    pub explode_death: bool,
    pub collide_players: bool,
    pub explosion: Explosion,
    pub brick: BrickImpact,
    pub bounce_effect: String,
    pub stick_effect: String,
    pub blood_effect: String,
    pub bounce_angle: f32,
    pub min_stick_speed: f32,
    pub trail: String,
    pub sound: String,
    pub light_radius: f32,
    pub light_color: [f32; 3],
    pub sport_image: Option<String>,
    pub rest_speed: f32,
}
/// v20's `ProjectileData` defaults, for fields an Add-On leaves out.
impl Default for ProjectileDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            model: String::new(),
            speed: 50.0,
            inherit: 1.0,
            gravity: 1.0,
            lifetime_ticks: 240,
            fade_ticks: 240,
            arm_ticks: 0,
            ballistic: false,
            elasticity: 0.999,
            friction: 0.3,
            damage: 0.0,
            damage_type: String::new(),
            radius_damage_type: String::new(),
            impulse: 0.0,
            vertical: 0.0,
            explode_player: false,
            explode_death: false,
            collide_players: true,
            explosion: Explosion::default(),
            brick: BrickImpact::default(),
            bounce_effect: String::new(),
            stick_effect: String::new(),
            blood_effect: String::new(),
            bounce_angle: 0.0,
            min_stick_speed: 0.0,
            trail: String::new(),
            sound: String::new(),
            light_radius: 0.0,
            light_color: [0.0; 3],
            sport_image: None,
            rest_speed: 0.0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub path: String,
    pub sha256: String,
    pub native_file: Option<String>,
    pub diagnostics: Vec<String>,
    /// Content-root-relative directory of the package holding `native_file`,
    /// set when packs from several packages are merged; None is this pack's
    /// own directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}
/// `AddDamageType`: kill-message templates, `%1` the victim and `%2` the
/// killer, with `<bitmap:...>` death icons kept verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DamageType {
    pub name: String,
    pub suicide_message: String,
    pub murder_message: String,
    /// `$Damage::VehicleDamageScale`: vehicles take this share of the damage.
    pub vehicle_scale: f32,
    pub direct: bool,
}
impl DamageType {
    /// Lower-case ids of the icons both templates show.
    /// Take the `<bitmap:id>` tags of `icons` (lower case) out of both
    /// messages, leaving their text.
    pub fn remove_icons(&mut self, icons: &[String]) {
        for message in [&mut self.suicide_message, &mut self.murder_message] {
            let mut out = String::with_capacity(message.len());
            let mut removed = false;
            let mut rest = message.as_str();
            while let Some(start) = rest.find("<bitmap:") {
                let tag = &rest[start..];
                let Some(end) = tag.find('>') else {
                    break;
                };
                out.push_str(&rest[..start]);
                if icons.contains(&tag[8..end].to_ascii_lowercase()) {
                    removed = true;
                } else {
                    out.push_str(&tag[..=end]);
                }
                rest = &tag[end + 1..];
            }
            out.push_str(rest);
            // The space either side of the icon becomes one.
            if removed {
                out = out.split_whitespace().collect::<Vec<_>>().join(" ");
            }
            *message = out;
        }
    }
    pub fn icons(&self) -> impl Iterator<Item = String> + '_ {
        [&self.suicide_message, &self.murder_message]
            .into_iter()
            .flat_map(|m| m.split("<bitmap:").skip(1))
            .filter_map(|rest| rest.split_once('>'))
            .map(|(id, _)| id.to_ascii_lowercase())
    }
    /// Substitute names in one pass so a name containing `%1` stays literal.
    pub fn message(&self, victim: &str, killer: Option<&str>) -> String {
        let (template, killer) = match killer {
            Some(k) => (&self.murder_message, k),
            None => (&self.suicide_message, ""),
        };
        let mut out = String::new();
        let mut chars = template.chars().peekable();
        while let Some(c) = chars.next() {
            match (c, chars.peek()) {
                ('%', Some('1')) => {
                    chars.next();
                    out.push_str(victim);
                }
                ('%', Some('2')) => {
                    chars.next();
                    out.push_str(killer);
                }
                _ => out.push(c),
            }
        }
        out
    }
}
/// `ExplosionData` fields presented beside its native effects composite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplosionInfo {
    pub name: String,
    /// Original `soundProfile`, empty when silent.
    pub sound: String,
    /// `shakeCamera` with its `camShake*` fields.
    pub shake: Option<CameraShake>,
    /// `explosionShape` model (converted like projectile models), or empty.
    pub shape: String,
    /// `lifetimeMS` in seconds.
    pub seconds: f32,
    pub play_speed: f32,
    pub face_viewer: bool,
    /// `explosionScale` times `sizes[i]` at `times[i]` of the lifetime.
    pub scale: [f32; 3],
    pub sizes: Vec<([f32; 3], f32)>,
}
/// Torque `CameraShake`: per-axis sine offsets in the camera frame
/// (x right, y forward, z up) fading as `1 / (1 + t * falloff)^2`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraShake {
    pub frequency: [f32; 3],
    pub amplitude: [f32; 3],
    pub seconds: f32,
    pub radius: f32,
    pub falloff: f32,
}
/// Particles, emitters and lights an Add-On's weapons bring, in the base
/// game's effects library format, and its explosions' effects. Ids carry
/// the package's namespace (`pkg:emitter/flash`); particle textures are the
/// base game's. Image states and projectile trails name an emitter by id;
/// an explosion's effect is found by its explosion's name, the last part of
/// its id, as the base game's are.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackEffects {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub particles: Vec<bri_content::effects::Particle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emitters: Vec<bri_content::effects::Emitter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lights: Vec<bri_content::effects::Light>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub explosions: Vec<ExplosionEffect>,
}
/// What an explosion draws: emitters running for its lifetime, a light, and
/// a burst of one emitter's particles at once (`particleEmitter`,
/// `particleDensity`, `particleRadius`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplosionEffect {
    pub id: String,
    /// Seconds.
    pub lifetime: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emitters: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<String>,
    /// Emitter, particle count and radius.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burst: Option<(String, u32, f32)>,
}
/// The name an effect id is bound by: its last part (`pkg:explosion/boom`
/// is `boom`).
pub fn effect_symbol(id: &str) -> &str {
    id.rsplit(['/', ':']).next().unwrap_or(id)
}
impl PackEffects {
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
            && self.emitters.is_empty()
            && self.lights.is_empty()
            && self.explosions.is_empty()
    }
    /// Checks the definitions as the effects library would, textures aside:
    /// the client finds those among the base game's, or else among the
    /// Add-On's item presentation textures (a relative path key). An
    /// explosion may use the base game's emitters (`v20/emitter/...`).
    pub fn validate(&self) -> Result<()> {
        for p in &self.particles {
            ensure!(
                !p.texture.is_empty()
                    && p.texture.len() <= 256
                    && !p.texture.starts_with('/')
                    && !p.texture.contains(':')
                    && !p.texture.chars().any(char::is_control)
                    && p.texture.split('/').all(|s| !s.is_empty() && s != "..")
                    && !p.texture.contains('\\'),
                "particle {} names an invalid texture",
                p.id
            );
        }
        ensure!(
            [
                self.particles.len(),
                self.emitters.len(),
                self.lights.len(),
                self.explosions.len()
            ]
            .iter()
            .all(|n| *n <= 256),
            "too many weapon effects"
        );
        let library = bri_content::effects::Library {
            schema_version: 1,
            lights: self.lights.clone(),
            particles: self.particles.clone(),
            emitters: self.emitters.clone(),
            textures: self
                .particles
                .iter()
                .map(|p| (p.texture.clone(), "texture.png".to_owned()))
                .collect(),
        };
        library.validate()?;
        let mut ids = std::collections::BTreeSet::new();
        for e in &self.explosions {
            let emitter = |id: &String| {
                id.starts_with("v20/emitter/") || self.emitters.iter().any(|x| &x.id == id)
            };
            ensure!(
                ids.insert(e.id.to_ascii_lowercase())
                    && !e.id.is_empty()
                    && e.id.len() <= 160
                    && !e.id.chars().any(char::is_control)
                    && e.lifetime.is_finite()
                    && e.lifetime > 0.0
                    && e.lifetime <= 3600.0
                    && e.emitters.len() <= 8
                    && e.emitters.iter().all(emitter)
                    && e.light
                        .as_ref()
                        .is_none_or(|l| self.lights.iter().any(|x| &x.id == l))
                    && e.burst.as_ref().is_none_or(|(id, count, radius)| {
                        emitter(id) && *count <= 32768 && radius.is_finite() && *radius >= 0.0
                    }),
                "Invalid explosion effect {}",
                e.id
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pack {
    pub schema_version: u32,
    pub id: String,
    #[serde(default)]
    pub items: BTreeMap<String, Item>,
    #[serde(default)]
    pub images: BTreeMap<String, Image>,
    #[serde(default)]
    pub projectiles: BTreeMap<String, ProjectileDef>,
    /// Keyed by lower-case damage type name (`$DamageType::<name>`).
    #[serde(default)]
    pub damage_types: BTreeMap<String, DamageType>,
    /// Keyed by lower-case explosion datablock name.
    #[serde(default)]
    pub explosions: BTreeMap<String, ExplosionInfo>,
    /// Sounds the pack ships, keyed by lower-case profile name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sounds: BTreeMap<String, SoundDef>,
    /// Particles, emitters, lights and explosions the pack brings.
    #[serde(default, skip_serializing_if = "PackEffects::is_empty")]
    pub effects: PackEffects,
    #[serde(default)]
    pub definitions: Vec<Definition>,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}
impl Pack {
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= 32 * 1024 * 1024,
            "Weapon pack exceeds byte limit"
        );
        let mut pack: Self = serde_json::from_slice(bytes)?;
        pack.fill_ids();
        pack.validate()?;
        Ok(pack)
    }
    /// Items, images and projectiles written without an `id` take their
    /// key's, so an Add-On names each once.
    pub fn fill_ids(&mut self) {
        for (key, item) in &mut self.items {
            if item.id.is_empty() {
                item.id.clone_from(key);
            }
        }
        for (key, image) in &mut self.images {
            if image.id.is_empty() {
                image.id.clone_from(key);
            }
        }
        for (key, projectile) in &mut self.projectiles {
            if projectile.id.is_empty() {
                projectile.id.clone_from(key);
            }
        }
    }
    /// The sound a state or rule names, from this pack's own sounds.
    pub fn sound(&self, profile: &str) -> Option<&SoundDef> {
        self.sounds.get(&profile.to_ascii_lowercase())
    }
    /// The type named by a `$DamageType::<name>` reference; unknown names
    /// fall back to `Default` as an unset Torque global indexes type 0.
    pub fn damage_type(&self, reference: &str) -> Option<&DamageType> {
        let name = reference.trim();
        let name = match name.get(..13) {
            Some(prefix) if prefix.eq_ignore_ascii_case("$damagetype::") => &name[13..],
            _ => name,
        };
        self.damage_types
            .get(&name.to_ascii_lowercase())
            .or_else(|| self.damage_types.get("default"))
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == SCHEMA, "Unknown weapon schema");
        ensure!(
            self.items.len() <= 1024
                && self.images.len() <= 4096
                && self.projectiles.len() <= 4096
                && self.damage_types.len() <= 1024
                && self.explosions.len() <= 4096,
            "Definition budget exceeded"
        );
        for (key, t) in &self.damage_types {
            ensure!(
                key == &t.name.to_ascii_lowercase()
                    && [&t.suicide_message, &t.murder_message]
                        .iter()
                        .all(|m| m.len() <= 256 && !m.chars().any(char::is_control))
                    && t.icons().all(|id| {
                        id.len() <= 128
                            && id
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"/_-".contains(&b))
                    }),
                "Invalid damage type {key}"
            );
        }
        for (key, e) in &self.explosions {
            ensure!(
                key == &e.name.to_ascii_lowercase()
                    && e.sound.len() <= 128
                    && !e.sound.chars().any(char::is_control)
                    && [e.seconds, e.play_speed]
                        .into_iter()
                        .chain(e.scale)
                        .chain(
                            e.sizes
                                .iter()
                                .flat_map(|(s, t)| s.iter().copied().chain([*t]))
                        )
                        .all(|v| v.is_finite() && (0.0..=1000.0).contains(&v))
                    && e.sizes.len() <= 4
                    && e.shake.is_none_or(|s| {
                        s.frequency
                            .into_iter()
                            .chain(s.amplitude)
                            .chain([s.seconds, s.radius, s.falloff])
                            .all(|v| v.is_finite() && (0.0..=1000.0).contains(&v))
                    }),
                "Invalid explosion {key}"
            );
        }
        for (id, item) in &self.items {
            // An item with no image is picked up but held by nobody (an
            // ammo box, a health pack): equipping it mounts nothing.
            ensure!(
                id == &item.id && (item.image.is_empty() || self.images.contains_key(&item.image)),
                "Invalid item/image {id}"
            );
            // Players choose items by name (the inventory, spawn bricks,
            // /give), so a nameless item is refused here, where the fault
            // names its Add-On, not later in the server's item catalog.
            ensure!(
                !item.ui_name.trim().is_empty()
                    && item.ui_name.len() <= 128
                    && !item.ui_name.chars().any(char::is_control),
                "Item {id} needs a ui_name: the name players pick it by"
            );
        }
        for (id, image) in &self.images {
            ensure!(
                id == &image.id && image.states.len() <= 64,
                "Invalid image {id}"
            );
            ensure!(
                image
                    .offset
                    .into_iter()
                    .chain(image.eye_offset)
                    .chain(image.source_rotation_degrees)
                    .chain(image.color)
                    .all(f32::is_finite),
                "Invalid image transform/color"
            );
            ensure!(
                image.min_shot_ticks <= 36000 && image.mount_point < 32,
                "Invalid image mount/timing"
            );
            ensure!(
                image.command.as_deref().is_none_or(is_image_command)
                    && image.commands.states.len() <= 16
                    && image.commands.states.iter().all(|(script, c)| {
                        !script.is_empty()
                            && script.len() <= 64
                            && script
                                .bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                            && is_image_command(c)
                    })
                    && image.commands.jet.as_deref().is_none_or(is_image_command)
                    && image.commands.light.as_deref().is_none_or(is_image_command)
                    && image.commands.wheel.as_deref().is_none_or(is_image_command)
                    && image
                        .commands
                        .cancel
                        .as_deref()
                        .is_none_or(is_image_command),
                "Invalid image command {id}"
            );
            ensure!(
                image.rope.as_ref().is_none_or(|r| {
                    self.projectiles.contains_key(&r.projectile)
                        && (1.0..=1000.0).contains(&r.speed)
                }),
                "Invalid image rope {id}: a projectile of the pack, speed 1 to 1000"
            );
            ensure!(
                image.shot.is_none_or(|s| {
                    (1..=64).contains(&s.projectiles)
                        && (0.0..=1.0).contains(&s.spread)
                        && (0.0..=100.0).contains(&s.recoil)
                }),
                "Invalid image shot {id}: 1 to 64 projectiles, spread 0 to 1, recoil 0 to 100"
            );
            ensure!(
                image
                    .eye_rotation
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 360.0),
                "Invalid image eye_rotation {id}"
            );
            ensure!(
                image
                    .zoom
                    .is_none_or(|z| z.fov.is_finite() && (5.0..=85.0).contains(&z.fov)),
                "Invalid image zoom {id}: fov 5 to 85 degrees"
            );
            for state in &image.states {
                ensure!(
                    state.ticks <= 36000
                        && state.emitter_seconds.is_finite()
                        && (0.0..=300.0).contains(&state.emitter_seconds),
                    "Invalid state duration"
                );
                for index in [
                    state.timeout,
                    state.down,
                    state.up,
                    state.ammo,
                    state.no_ammo,
                ]
                .into_iter()
                .flatten()
                {
                    ensure!(index < image.states.len(), "Invalid state target");
                }
            }
            if let Some(p) = &image.projectile {
                ensure!(self.projectiles.contains_key(p), "Missing projectile {p}");
            }
        }
        for (id, p) in &self.projectiles {
            ensure!(
                id == &p.id && p.lifetime_ticks > 0 && p.lifetime_ticks <= 36000,
                "Invalid projectile lifetime"
            );
            ensure!(
                [
                    p.speed,
                    p.inherit,
                    p.gravity,
                    p.elasticity,
                    p.friction,
                    p.damage,
                    p.impulse,
                    p.vertical,
                    p.explosion.damage,
                    p.explosion.radius,
                    p.explosion.impulse,
                    p.explosion.impulse_radius,
                    p.rest_speed,
                    p.explosion.burn_seconds,
                    p.brick.radius,
                    p.brick.force,
                    p.brick.max_volume,
                    p.brick.max_floating_volume,
                    p.light_radius,
                    p.bounce_angle,
                    p.min_stick_speed
                ]
                .iter()
                .all(|n| n.is_finite() && *n >= 0.0 && *n <= 100000.0),
                "Invalid projectile scalar"
            );
            ensure!(
                p.elasticity <= 1.0 && p.friction <= 1.0 && p.speed <= 10000.0,
                "Invalid trajectory"
            );
        }
        self.effects.validate()?;
        ensure!(self.sounds.len() <= 1024, "Definition budget exceeded");
        for (key, sound) in &self.sounds {
            let lower = sound.file.to_ascii_lowercase();
            ensure!(
                key == &key.to_ascii_lowercase()
                    && !key.is_empty()
                    && key.len() <= 128
                    && !key.chars().any(char::is_control)
                    && sound.volume.is_finite()
                    && (0.0..=1.0).contains(&sound.volume)
                    && (lower.ends_with(".wav") || lower.ends_with(".ogg")),
                "Invalid sound {key}: keys are lower case, volume 0 to 1, file .wav or .ogg"
            );
        }
        let sound_paths = self
            .sounds
            .values()
            .flat_map(|s| std::iter::once(&s.file).chain(&s.package));
        for path in self
            .resources
            .iter()
            .flat_map(|r| r.native_file.iter().chain(&r.package))
            .chain(sound_paths)
        {
            ensure!(
                !path.starts_with('/')
                    && !path.contains(':')
                    && !path.contains('\\')
                    && !path.split('/').any(|part| part == ".."),
                "Unsafe native resource path"
            );
        }
        Ok(())
    }
}
