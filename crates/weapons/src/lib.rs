//! Versioned native weapon content. No legacy parser is linked into this crate.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub mod debris;
mod merge;
pub mod rotation;
pub mod testing;
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
    /// Torque's `stateTransitionOnLoaded` and `OnNotLoaded`, checked before
    /// the ammo transitions: whether the image is loaded
    /// (`setImageLoaded`). Mounting loads it; a [`Magazine`] keeps it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_loaded: Option<usize>,
    pub script: String,
    pub sequence: String,
    /// The holder's arm animation (thread 2) played on entering the state,
    /// as v20 scripts did with `playThread(2, armAttack)` in `onPreFire`:
    /// `armattack` for a swing, `root` to stop.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub arm: String,
    /// The holder's thread-3 animation played on entering the state, as
    /// v20 scripts' `playThread(3, shiftLeft)`: the other arm's move, a
    /// gesture over whatever thread 2 plays; `root` stops it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub gesture: String,
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
/// Presentation only: each player's own game zooms its own view, draws
/// its own scope and moves its own aim, so none of it costs bandwidth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zoom {
    /// Horizontal field of view while aiming, 5 to 85 degrees.
    pub fov: f32,
    /// The right mouse button aims too (aim down sights). The holder jets
    /// as well unless `jets` is false or their body cannot jet.
    #[serde(default)]
    pub on_jet: bool,
    /// With `on_jet`, whether pressing jet also jets. `false` makes the
    /// right mouse button the scope's alone while the weapon is in hand;
    /// the holder jets again when they put it away.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub jets: bool,
    /// The game's crosshair shows while aiming (a scope draws its own).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub crosshair: bool,
    /// Aiming switches a third-person view to first person until released.
    #[serde(default)]
    pub first_person: bool,
    /// Further magnifications the mouse wheel steps through while aiming,
    /// each a narrower field of view than the one before (up to
    /// [`Zoom::MAX_LEVELS`], 5 to 85 degrees): rolled forward zooms in,
    /// back zooms out, and the wheel does not change tools meanwhile. The
    /// step taken is kept while the weapon stays in hand.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<f32>,
    /// Mouse look speed while aiming, as a multiple of the player's own.
    /// Look already slows as the view narrows (v20 scales it by the field
    /// of view), so 1 keeps that feel; 0.1 to 4.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub sensitivity: f32,
    /// A scope's picture, drawn over the whole screen while aiming in
    /// first person: a PNG named without `.png`, relative to the folder
    /// `weapons.json` is in (as an item's `icon`), transparent where the
    /// lens shows the world. It is fitted to the screen's height and
    /// centred, the rest of the screen is black, and the weapon itself is
    /// not drawn while it shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<String>,
    /// The aim drifting while aiming: a steady figure of eight, as a
    /// marksman's breathing moves a scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sway: Option<Sway>,
}
impl Zoom {
    pub const MAX_LEVELS: usize = 8;
    /// The field of view at wheel step `level` (0 is `fov`), clamped to the
    /// steps there are.
    pub fn level_fov(&self, level: usize) -> f32 {
        match level.min(self.levels.len()) {
            0 => self.fov,
            n => self.levels[n - 1],
        }
    }
    pub fn validate(&self) -> std::result::Result<(), String> {
        let fov = |f: f32| f.is_finite() && (5.0..=85.0).contains(&f);
        if !fov(self.fov) {
            return Err("fov 5 to 85 degrees".into());
        }
        if self.levels.len() > Self::MAX_LEVELS
            || !self.levels.iter().all(|f| fov(*f))
            || !std::iter::once(self.fov)
                .chain(self.levels.iter().copied())
                .zip(self.levels.iter().copied())
                .all(|(wider, narrower)| narrower < wider)
        {
            return Err(format!(
                "levels: up to {} fields of view, 5 to 85 degrees, each narrower than the last",
                Self::MAX_LEVELS
            ));
        }
        if !(self.sensitivity.is_finite() && (0.1..=4.0).contains(&self.sensitivity)) {
            return Err("sensitivity 0.1 to 4".into());
        }
        if let Some(o) = &self.overlay
            && (o.is_empty()
                || o.len() > 128
                || o.starts_with('/')
                || o.contains(':')
                || o.contains('\\')
                || o.split('/').any(|part| part == ".." || part.is_empty()))
        {
            return Err("overlay: a PNG path inside the Add-On, without .png".into());
        }
        if let Some(s) = &self.sway {
            s.validate()?;
        }
        Ok(())
    }
}
/// How far and how fast an aim sways ([`Zoom::sway`]). The drift is a
/// Lissajous figure of eight, `degrees` across and half as high, once
/// round every `seconds`: the same path on every machine, eased in as
/// the player aims and out as they stop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sway {
    /// How far the aim drifts each side, 0 to 5 degrees.
    pub degrees: f32,
    /// Seconds for one round of the figure of eight, 0.5 to 30.
    pub seconds: f32,
    /// The drift while crouched, as a multiple of standing's: 0 steadies
    /// the aim completely, 1 not at all.
    #[serde(default = "Sway::crouched_default")]
    pub crouched: f32,
    /// The drift while walking or in the air, as a multiple of standing
    /// still's, 1 to 4.
    #[serde(default = "Sway::moving_default")]
    pub moving: f32,
}
impl Sway {
    fn crouched_default() -> f32 {
        0.35
    }
    fn moving_default() -> f32 {
        2.0
    }
    fn validate(&self) -> std::result::Result<(), String> {
        let within = |v: f32, lo: f32, hi: f32| v.is_finite() && (lo..=hi).contains(&v);
        if within(self.degrees, 0.0, 5.0)
            && within(self.seconds, 0.5, 30.0)
            && within(self.crouched, 0.0, 1.0)
            && within(self.moving, 1.0, 4.0)
        {
            Ok(())
        } else {
            Err("sway: degrees 0 to 5, seconds 0.5 to 30, crouched 0 to 1, moving 1 to 4".into())
        }
    }
    /// The drift `(yaw, pitch)` in radians at `phase` rounds into the
    /// figure of eight, before any easing: `degrees` side to side, half as
    /// far up and down at twice the rate.
    pub fn offset(&self, phase: f64) -> (f32, f32) {
        let a = (self.degrees as f64).to_radians();
        let t = phase * std::f64::consts::TAU;
        ((a * t.sin()) as f32, (0.5 * a * (2.0 * t).sin()) as f32)
    }
}
fn one() -> f32 {
    1.0
}
fn is_one(v: &f32) -> bool {
    *v == 1.0
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
    /// The holder's body nodes hidden while this image is held, as a v20
    /// Add-On's `onMount` did with `%obj.hideNode("lhand")` and its
    /// `onUnMount` undid: a model that draws its own hands hides the
    /// Blockhead's (`lhand`, `rhand`, `lhook`, `rhook`). Up to 16 names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide_nodes: Vec<String>,
    /// Held up with both arms (`armReadyBoth`), as `onMount`'s
    /// `%obj.playThread(2, armReadyBoth)` did, not with the mount hand's
    /// arm alone.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub both_arms: bool,
    /// Held, the image takes its holder's spray colour (the palette colour
    /// they last picked) as a colour spray can does: a tool that paints
    /// with that colour shows it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub paint_tint: bool,
    /// The image held in the left hand alongside this one (dual pistols),
    /// mounted and unmounted with it, as v20's akimbo gun mounted its left
    /// gun in image slot 1. A state script `onFireAkimbo` pulls the left
    /// image's trigger for one tick; both hands share the holder's ammo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_image: Option<String>,
    /// Rounds in a magazine and a reserve to reload it from, kept by the
    /// engine for every gun that declares one: tactical packs' magazines
    /// (Tier+Tactical's `ammo` system, the Adventure Pack's), in data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magazine: Option<Magazine>,
    /// More projectiles each shot fires after the image's own, as v20
    /// shotguns' `onFire` fired a slug or close blast after the pellets:
    /// each volley its own projectile, count and spread, from the same
    /// muzzle along the same aim, inheriting the shot's recoil.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volleys: Vec<Volley>,
    /// What a gun's last few rounds fire instead of `shot` and `volleys`,
    /// when its magazine holds [`Magazine::last_rounds`] or fewer: a
    /// two-barrel gun's single barrel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_shot: Option<LastShot>,
    /// Shots fired on entering a state whose script is not `onFire`, by
    /// that script, lowercase: v20 guns whose fire states each ran a
    /// script of their own (`onFire2`, `onFire3`) with its own spread and
    /// recoil, as a heavy gun's fire spreads wider as it keeps firing.
    /// Each takes rounds (unless [`Shot::free`]) and fires `volleys` as
    /// `onFire`'s shot does.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub state_shots: BTreeMap<String, Shot>,
    /// A grenade cooked in the hand ([`Cook`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cook: Option<Cook>,
    /// What the image's own state scripts do, by script name in lower case
    /// (`oncharge`, `onfire`, `onfiretwo`): entering a state whose `script`
    /// is listed does this instead of the game's built-in handling of that
    /// name. A port writes here what a v20 Add-On's `Image::on...` function
    /// did.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scripts: BTreeMap<String, Script>,
}
impl Image {
    /// Every projectile the image can launch: its own, its shots' moving
    /// and rested ones, its volleys' and its scripts'.
    pub fn projectile_refs(&self) -> impl Iterator<Item = &String> {
        let shots = self
            .shot
            .iter()
            .chain(self.last_shot.iter().map(|l| &l.shot))
            .chain(self.state_shots.values());
        let volleys = self
            .volleys
            .iter()
            .chain(self.last_shot.iter().flat_map(|l| &l.volleys));
        self.projectile
            .iter()
            .chain(shots.flat_map(Shot::projectile_refs))
            .chain(volleys.map(|v| &v.projectile))
            .chain(self.scripts.values().filter_map(|s| s.projectile.as_ref()))
    }
}
/// [`Image::cook`]: a fuse that starts burning in the hand, as v20 grenade
/// scripts timed one (`getSimTime` as the pin drops, a schedule to go off
/// in the hand). The image's next shot carries what is left of it and goes
/// off when it runs out; held that long, it goes off in the hand instead:
/// the image's projectile explodes above the holder, who puts it away and
/// keeps the grenade. Putting it away first puts the fuse out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cook {
    /// The state script that lights the fuse (`onPinDrop`), lowercase.
    pub script: String,
    /// How long the fuse burns, in ticks (120 a second), 1 to 36000.
    pub fuse_ticks: u32,
    /// Where it goes off in the hand, in units above the holder's feet.
    #[serde(default)]
    pub burst_height: f32,
    /// Shown in the middle of the holder's screen while it burns, every
    /// `print_ticks` from the first, for `print_seconds`: `{seconds}` is
    /// the time left to a tenth (`3.9`, `1`) and `{s}` an `s` unless that is
    /// exactly 1. `first_print` replaces the first. Empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub print: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_print: String,
    /// 1 to 1200.
    #[serde(default = "twelve")]
    pub print_ticks: u32,
    /// 0 to 10.
    #[serde(default)]
    pub print_seconds: f32,
}
fn twelve() -> u32 {
    12
}
impl Cook {
    /// What the holder reads `burned` ticks after the fuse was lit, if
    /// it is a print tick: the first at `print_ticks`, showing it whole.
    pub fn print_at(&self, burned: u32) -> Option<String> {
        if self.print.is_empty() || burned == 0 || !burned.is_multiple_of(self.print_ticks) {
            return None;
        }
        if burned == self.print_ticks && !self.first_print.is_empty() {
            return Some(self.first_print.clone());
        }
        // Counted down a step at a time from the whole fuse, as the
        // scripts' own text was.
        let left = self
            .fuse_ticks
            .checked_sub(burned - self.print_ticks)
            .filter(|l| *l > 0)?;
        let tenths = (left as f32 / 12.0).round() as u32;
        let seconds = if tenths.is_multiple_of(10) {
            format!("{}", tenths / 10)
        } else {
            format!("{}.{}", tenths / 10, tenths % 10)
        };
        Some(
            self.print
                .replace("{seconds}", &seconds)
                .replace("{s}", if tenths == 10 { "" } else { "s" }),
        )
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.script.is_empty()
                && self.script.len() <= 64
                && self.script == self.script.to_ascii_lowercase()
                && (1..=36_000).contains(&self.fuse_ticks)
                && self.burst_height.is_finite()
                && (-10.0..=10.0).contains(&self.burst_height)
                && (1..=1200).contains(&self.print_ticks)
                && (0.0..=10.0).contains(&self.print_seconds)
                && self.print.len() <= 255
                && self.first_print.len() <= 255,
            "Invalid cook"
        );
        Ok(())
    }
}
/// [`Image::last_shot`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LastShot {
    pub shot: Shot,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volleys: Vec<Volley>,
}
/// [`Image::volleys`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Volley {
    /// A projectile of the pack (or one it depends on).
    pub projectile: String,
    /// 1 to 64.
    pub projectiles: u32,
    /// v20's `%spread`, 0 to 1, as [`Shot::spread`].
    #[serde(default)]
    pub spread: f32,
}
/// [`Image::magazine`]. Each holder has a magazine per item and a reserve
/// per `ammo` type, shared by every gun of that type. A shot takes
/// `per_shot` rounds and is refused (the gun clicks) without them or while
/// reloading; an empty magazine reloads itself when there is reserve, and
/// the light key reloads one that is not full. Rounds move when the reload
/// ends, all at once, or one at a time with `one_by_one` (a shotgun's
/// shells, which a pull of the trigger interrupts). Switching away cancels
/// a reload.
///
/// The magazine sets the image's flags for its states. With `checks`, the
/// image's states run the magazine as a v20 script ammo system did
/// (Tier+Tactical's): its state scripts set the flags (`TT_onLoadCheck`'s
/// `setImageLoaded` and `setImageAmmo`), which keep their value until the
/// next check; the rounds move only as the image enters its
/// `reload_state`, so a reload is the image's own states, with no timer;
/// and an empty gun neither fires nor reloads by itself, its checks send
/// it to its reload states. Without `checks`, an image whose states use
/// `loaded`/`not_loaded` is loaded while its magazine has a shot and no
/// reload is under way, and has ammo while there is reserve to reload
/// from; any other image has ammo exactly when its magazine has a shot and
/// no reload is under way, so `ammo`/`no_ammo` states follow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Magazine {
    /// Rounds a full magazine holds, 1 to 1000.
    pub size: u32,
    /// The reserve it reloads from, by name (`9mm`, `shells`): 1 to 32
    /// letters, digits, `.`, `_` or `-`.
    pub ammo: String,
    /// Rounds one shot uses, 1 to `size`.
    #[serde(default = "one_u32")]
    pub per_shot: u32,
    /// How long a reload takes (each round's with `one_by_one`), in ticks
    /// (120 a second), 1 to 1200.
    pub reload_ticks: u32,
    /// Reload one round at a time.
    #[serde(default)]
    pub one_by_one: bool,
    /// The reserve a holder starts with for this ammo, 0 to 100000.
    #[serde(default)]
    pub reserve: u32,
    /// The most reserve a holder carries of this ammo, 1 to 100000.
    #[serde(default = "max_reserve")]
    pub max_reserve: u32,
    /// The holder's arm animation as a reload starts (`shiftDown`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reload_sequence: String,
    /// The sound as a reload starts, and as a click with nothing to shoot.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reload_sound: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub empty_sound: String,
    /// What the ammo display calls it; the `ammo` name when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display: String,
    /// The state script of the image's own reload (`onReloaded`): a reload
    /// under way moves its rounds as the image enters a state running it,
    /// so they arrive with the reload's own animation and sound, or when
    /// `reload_ticks` are up if that comes first.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reload_state: String,
    /// The flags each state script sets on entering its state, by script
    /// name (`TT_onLoadCheck`), up to 16.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub checks: BTreeMap<String, Check>,
    /// With `checks`: the flags as a reload starts (the light key, or
    /// reserve for an empty gun), and as its rounds arrive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_reload: Option<Check>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_loaded: Option<Check>,
    /// With this many rounds or fewer left (at least one), a pull fires the
    /// image's `last_shot` and takes them all, even fewer than `per_shot`:
    /// a two-barrel gun's last barrel. 0 for none.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub last_rounds: u32,
    /// The image states (by name, any case) the light key reloads in. In
    /// any other state, or when no reload can start (a full magazine, no
    /// reserve, one already under way), the key works the light as usual,
    /// as the hl2 ammo system's packaged `serverCmdLight` did and
    /// Tier+Tactical's (`Ready`, `Empty`, `EmptyFire`). With `checks`, no
    /// reload starts outside them either. Empty: the key reloads in any
    /// state and never works the light. At most 8.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub light_states: Vec<String>,
}
/// [`Magazine::checks`]: the flags a script sets, each left as it was when
/// absent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loaded: Option<Cond>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ammo: Option<Cond>,
    /// When it leaves the hand loaded, it takes a shot's rounds itself, so
    /// the shot its states fire next is [`Shot::free`] (Tier+Tactical's
    /// burst check: `TT_canFire`, then `TT_decrementAmmo`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spend: bool,
    /// While a reload of the magazine is under way the hand is not loaded,
    /// whatever `loaded` says, so a reload begun under another image of
    /// the same gun carries on in this one (Tier+Tactical's
    /// `TT_forceToolReload`: a scope handing its reload to the unscoped
    /// image).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keeps_reload: bool,
}
/// A flag's value in a [`Check`]: `true`, `false`, or true when any of the
/// listed facts about the holder's magazine holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Cond {
    Is(bool),
    Any(Vec<Fact>),
}
/// What a [`Cond`] can ask of the magazine and its reserve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fact {
    /// At least one shot's rounds in the magazine, and not.
    Shot,
    Empty,
    /// The magazine is full, and not.
    Full,
    NotFull,
    /// Some reserve of its ammo, and none.
    Reserve,
    NoReserve,
}
impl Cond {
    /// The flag, from the magazine's rounds and whether there is reserve.
    pub fn holds(&self, magazine: &Magazine, rounds: u32, reserve: bool) -> bool {
        match self {
            Cond::Is(v) => *v,
            Cond::Any(facts) => facts.iter().any(|f| match f {
                Fact::Shot => magazine.fires(rounds),
                Fact::Empty => !magazine.fires(rounds),
                Fact::Full => rounds >= magazine.size,
                Fact::NotFull => rounds < magazine.size,
                Fact::Reserve => reserve,
                Fact::NoReserve => !reserve,
            }),
        }
    }
}
fn max_reserve() -> u32 {
    100_000
}
impl Magazine {
    /// Whether `rounds` in the magazine make a shot.
    pub fn fires(&self, rounds: u32) -> bool {
        rounds >= self.per_shot || self.last(rounds)
    }
    /// Whether a shot with `rounds` in the magazine is its last
    /// ([`Self::last_rounds`]).
    pub fn last(&self, rounds: u32) -> bool {
        rounds > 0 && rounds <= self.last_rounds
    }
    /// Whether the image's states run it ([`Magazine::checks`]).
    pub fn scripted(&self) -> bool {
        !self.checks.is_empty()
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=1000).contains(&self.size)
                && (1..=self.size).contains(&self.per_shot)
                && self.last_rounds <= self.size
                && (1..=1200).contains(&self.reload_ticks)
                && self.reserve <= 100_000
                && (1..=100_000).contains(&self.max_reserve),
            "Invalid magazine numbers"
        );
        ensure!(
            (1..=32).contains(&self.ammo.len())
                && self
                    .ammo
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "Invalid magazine ammo name"
        );
        ensure!(
            self.reload_sequence.len() <= 64
                && self
                    .reload_sequence
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && self.display.len() <= 64
                && self.reload_state.len() <= 64,
            "Invalid magazine reload sequence, display or reload state"
        );
        for sound in [&self.reload_sound, &self.empty_sound] {
            ensure!(sound.len() <= 128, "Invalid magazine sound");
        }
        ensure!(
            self.checks.len() <= 16
                && (self.checks.is_empty() || !self.reload_state.is_empty())
                && self.checks.keys().all(|k| (1..=64).contains(&k.len()))
                && self
                    .checks
                    .values()
                    .chain(&self.on_reload)
                    .chain(&self.on_loaded)
                    .flat_map(|c| [&c.loaded, &c.ammo])
                    .flatten()
                    .all(|c| !matches!(c, Cond::Any(f) if f.is_empty() || f.len() > 6)),
            "Invalid magazine checks"
        );
        ensure!(
            self.light_states.len() <= 8
                && self
                    .light_states
                    .iter()
                    .all(|s| (1..=64).contains(&s.len())),
            "Invalid magazine light states"
        );
        Ok(())
    }
    /// What the ammo display calls it.
    pub fn name(&self) -> &str {
        if self.display.is_empty() {
            &self.ammo
        } else {
            &self.display
        }
    }
}
/// One image state script as data: what a v20 `Image::onCharge`,
/// `onFire` or a custom `stateScript` function did to its holder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    /// The holder's arm animation (thread 2) started first, as
    /// `%obj.playThread(2, ...)`: `spearReady` while charging,
    /// `spearThrow` or `armattack` on the swing, `root` to lower the arm.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arm: String,
    /// Launch a projectile as `Parent::onFire` does: aimed and spread like
    /// the image's own shots, after the arm animation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fire: bool,
    /// The projectile `fire` launches instead of the image's own: a second
    /// attack of one weapon, as a script that spawned its own
    /// `ProjectileData` (a knife's quick jab beside its charged stab).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projectile: Option<String>,
    /// The held item is used up: it leaves the holder's tools and hand, as
    /// a thrown grenade's script cleared `%obj.tool[%slot]` and called
    /// `serverCmdUnUseTool`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub use_up: bool,
}
/// An arm animation name: letters, digits and `_`, up to 64 bytes.
fn is_sequence_name(name: &str) -> bool {
    name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
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
    /// The image leaving the hand: another tool drawn, the hand emptied,
    /// a rule mounting another image (v20 `onUnMount`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unmount: Option<String>,
    /// The image coming into the hand: drawn, or mounted by a rule (v20
    /// `onMount`). It runs after the one leaving's `unmount`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount: Option<String>,
}
impl ImageCommands {
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
            && self.jet.is_none()
            && self.light.is_none()
            && self.wheel.is_none()
            && self.cancel.is_none()
            && self.unmount.is_none()
            && self.mount.is_none()
    }
    /// Whether the image runs `command` (`package:command`) from any of its
    /// moments: a state, jet, light, wheel, cancel, unmount or mount.
    pub fn runs(&self, command: &str) -> bool {
        self.states.values().any(|c| c == command)
            || [
                &self.jet,
                &self.light,
                &self.wheel,
                &self.cancel,
                &self.unmount,
                &self.mount,
            ]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    /// Projectiles per shot, 1 to 64 (`%shellcount`); 1 when left out.
    #[serde(default = "one_projectile")]
    pub projectiles: u32,
    /// v20's `%spread`: each projectile's velocity turns by random Euler
    /// angles of up to ±5π·spread radians about each axis.
    #[serde(default)]
    pub spread: f32,
    /// Speed the shooter loses along their aim, in units per second.
    #[serde(default)]
    pub recoil: f32,
    /// The speed lost along the aim's vertical part, in place of `recoil`
    /// there, 0 to 100: Tier+Tactical's `TT_knockback(%obj, 0, 0, -1)`
    /// pushes a machine gunner only up or down as they fire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recoil_vertical: Option<f32>,
    /// The spread while the shooter moves faster than `moving_speed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moving_spread: Option<f32>,
    /// Units per second above which the shooter counts as moving, 0 to 50.
    #[serde(default = "default_moving_speed")]
    pub moving_speed: f32,
    /// The projectile a shot on the move flies in place of the image's
    /// (Tier+Tactical's Sport Rifle fires a weaker round when not still),
    /// of this pack or one it depends on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moving_projectile: Option<String>,
    /// A steadier shot when the holder has not fired for a while (and,
    /// with `still`, stands still): the first shot of a burst.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rested: Option<Rested>,
    /// Each projectile arrives instantly along a ray instead of flying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hitscan: Option<Hitscan>,
    /// The holder's view shakes with each shot. Drawn only by the holder's
    /// own game, from the shot it already sees: nothing is sent for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kick: Option<Kick>,
    /// The projectiles' size against their holder's, 0.1 to 10, as a v20
    /// script's `scale` on the projectiles it made: it grows their look
    /// and blast and, unless [`ProjectileDef::fixed_damage`], their
    /// damage. Their speed is the shot's.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub scale: f32,
    /// It takes no rounds from the magazine, as a script that fired
    /// without spending any (Tier+Tactical's light machine gun's
    /// `onFire2`, a free second round each cycle).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub free: bool,
}
/// [`ProjectileDef::slow`], Tier+Tactical's `TT_dampenVelocity(%col,
/// divisor)` in a bullet's `damage`: each hit divides the player's velocity
/// by `divisor` and lowers their speeds to a share that falls with every
/// hit, from halfway to the floor `1 / (3 · divisor)` down to it, back to
/// normal 200 ms after the last.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slow {
    /// 1 to 10.
    pub divisor: f32,
}
impl Slow {
    /// The share of their speeds a player keeps after a hit, from what
    /// they kept before it (None when not slowed).
    pub fn after_hit(&self, before: Option<f32>) -> f32 {
        let floor = 1.0 / (3.0 * self.divisor);
        match before {
            None => (1.0 + floor) / 2.0,
            Some(m) if m > floor => (m / self.divisor).max(floor),
            Some(m) => m,
        }
    }
    /// How long a slowdown lasts after the last hit, in ticks (120 a
    /// second): `TT_slow`'s 200 ms.
    pub const TICKS: u64 = 24;
}
/// [`Shot::kick`]: a Torque `CameraShake` on the shooter's own view, in
/// its units (about 10 degrees of turn per unit of amplitude), fading out
/// over `seconds`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kick {
    /// 0 to 1.
    pub amplitude: f32,
    /// Shakes a second, 0.1 to 30.
    #[serde(default = "default_kick_frequency")]
    pub frequency: f32,
    /// 0.05 to 2.
    #[serde(default = "default_kick_seconds")]
    pub seconds: f32,
}
fn default_kick_frequency() -> f32 {
    2.0
}
fn default_kick_seconds() -> f32 {
    0.5
}
fn default_moving_speed() -> f32 {
    0.1
}
fn one_projectile() -> u32 {
    1
}
impl Shot {
    /// The projectiles the shot flies in place of the image's.
    pub fn projectile_refs(&self) -> impl Iterator<Item = &String> {
        self.moving_projectile
            .iter()
            .chain(self.rested.as_ref().and_then(|r| r.projectile.as_ref()))
    }
    /// One projectile straight along the aim: an image without `shot`.
    pub const SINGLE: Shot = Shot {
        projectiles: 1,
        spread: 0.0,
        recoil: 0.0,
        recoil_vertical: None,
        moving_spread: None,
        moving_speed: 0.1,
        moving_projectile: None,
        rested: None,
        hitscan: None,
        kick: None,
        scale: 1.0,
        free: false,
    };
    /// The velocity the recoil adds to a shooter aiming along `direction`
    /// (a unit vector, Y up), before the projectiles inherit it.
    pub fn recoil_velocity(&self, direction: glam::Vec3) -> glam::Vec3 {
        let vertical = self.recoil_vertical.unwrap_or(self.recoil);
        -glam::Vec3::new(
            direction.x * self.recoil,
            direction.y * vertical,
            direction.z * self.recoil,
        )
    }
    /// The spread of a shot from a holder moving at `speed`, `idle_ticks`
    /// after their last shot (None for never): moving spread while moving,
    /// else the rested spread once rested, else `spread`.
    pub fn spread_for(&self, speed: f32, idle_ticks: Option<u64>) -> f32 {
        if let Some(moving) = self.moving_spread
            && speed > self.moving_speed
        {
            return moving;
        }
        match self.rested_at(speed, idle_ticks) {
            Some(r) => r.spread,
            None => self.spread,
        }
    }
    /// The projectile a shot flies in place of the image's, if any: the
    /// moving one while moving, else the rested one once rested.
    pub fn projectile_for(&self, speed: f32, idle_ticks: Option<u64>) -> Option<&String> {
        if speed > self.moving_speed
            && let Some(moving) = &self.moving_projectile
        {
            return Some(moving);
        }
        self.rested_at(speed, idle_ticks)?.projectile.as_ref()
    }
    /// [`Shot::rested`] when a shot `idle_ticks` after the last, at `speed`,
    /// is rested.
    fn rested_at(&self, speed: f32, idle_ticks: Option<u64>) -> Option<&Rested> {
        self.rested.as_ref().filter(|r| {
            (!r.still || speed <= self.moving_speed)
                && idle_ticks.is_none_or(|t| t >= u64::from(r.after_ticks))
        })
    }
}
/// [`Shot::rested`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rested {
    /// Ticks since the holder's last shot (120 a second), 1 to 1200.
    pub after_ticks: u32,
    /// The spread of that shot, 0 to 1.
    pub spread: f32,
    /// Only while the holder moves no faster than the shot's
    /// `moving_speed`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub still: bool,
    /// The projectile that shot flies in place of the image's, of this
    /// pack or one it depends on (Tier+Tactical's Assault Rifle fires a
    /// truer round after a pause).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projectile: Option<String>,
}
/// [`Shot::hitscan`]: the image's projectile arrives at once where a ray
/// from the muzzle (or the eye) first meets something, and does there what
/// it would have done on landing: its contact, damage, push, brick impact
/// and explosion. v20 raycast weapons worked this way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hitscan {
    /// Units, 1 to 2000, times the shooter's scale.
    pub range: f32,
    /// The range while the shooter moves faster than the shot's
    /// `moving_speed` (Tier+Tactical's guns reach less on the move).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moving_range: Option<f32>,
    /// Cast from the eye along the look rather than from the muzzle, so a
    /// scope's shot lands on its crosshair.
    #[serde(default)]
    pub from_eye: bool,
    /// The streak each player draws from the muzzle to where the ray ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracer: Option<Tracer>,
    /// A projectile exploded where it lands, by id or datablock name, from
    /// this pack or any other loaded (`raycastExplosionProjectile`): its
    /// explosion's effects and sound, and its blast, in place of the landing
    /// projectile's own explosion.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub explosion: String,
    /// A projectile flown from the muzzle to where the ray ended, by id or
    /// datablock name, from this pack or any other loaded: a raycasting
    /// script's `raycastTracerProjectile`, seen by everyone, which still
    /// pushes and knocks loose bricks as it lands.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub flown: String,
    /// Sounds where it lands on a player, and on anything else
    /// (`raycastExplosionPlayerSound`, `raycastExplosionBrickSound`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub player_sound: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub other_sound: String,
}
/// A hitscan shot's streak, drawn on every player's screen from their own
/// copy of the weapons pack: the shot sends only where it ended.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tracer {
    /// RGBA, 0 to 1.
    pub color: [f32; 4],
    /// Units, up to 1.
    #[serde(default = "default_tracer_width")]
    pub width: f32,
    /// Up to 2.
    #[serde(default = "default_tracer_seconds")]
    pub seconds: f32,
}
fn default_tracer_width() -> f32 {
    0.04
}
fn default_tracer_seconds() -> f32 {
    0.08
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
    /// Only scripts put it in the world (v20's `ItemData` with no
    /// `uiName`, such as a dead player's ammo bag): no spawn list, loadout
    /// or `/give` offers it, and it needs no `ui_name`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
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
            hidden: false,
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
    /// Explode on this bounce (a grenade that pops on its third knock);
    /// 0 leaves bounces to `arm_ticks` and the lifetime.
    #[serde(skip_serializing_if = "is_zero_u32")]
    pub max_bounces: u32,
    /// Smaller projectiles it throws out as it flies, bounces or explodes
    /// (flak sparks, a molotov's embers, a cluster bomb): one set, or a
    /// list of up to 4 (a grenade's shrapnel and its smoke trails).
    #[serde(
        default,
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub children: Vec<Children>,
    /// Hurts whatever stands near it every so often while it lives (fire,
    /// gas, a lingering ember).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aura: Option<Aura>,
    /// Slows a player it hits directly, as Tier+Tactical's submachine gun
    /// and machine gun bullets do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slow: Option<Slow>,
    /// Its direct damage stays as authored at any scale, where v20's own
    /// `ProjectileData::damage` scaled it with the projectile: projectiles
    /// whose `damage` method dealt `directDamage` as it was.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fixed_damage: bool,
}
fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}
/// Projectiles a projectile throws out in random directions, each `speed`
/// fast plus `inherit` of its parent's velocity. Directions come from the
/// tick and the parent, so the host and every player agree with nothing
/// sent. A child may not have children of its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Children {
    /// The child projectile, of this pack or one it depends on.
    pub projectile: String,
    /// How many each time, 1 to 16.
    #[serde(default = "one_u32")]
    pub count: u32,
    /// Units per second, 0 to 500.
    #[serde(default)]
    pub speed: f32,
    /// Share of the parent's velocity each child keeps, 0 to 1.
    #[serde(default)]
    pub inherit: f32,
    /// Every this many ticks of flight (120 a second, at least 4); 0 never.
    #[serde(default)]
    pub every_ticks: u32,
    /// Each time it bounces.
    #[serde(default)]
    pub on_bounce: bool,
    /// When it explodes.
    #[serde(default)]
    pub on_explode: bool,
    /// Each child goes off after a random number of ticks in this range,
    /// inclusive, as scripts scheduled each one's `explode` (cluster
    /// bomblets bursting one after another); 0 to 36000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuse_ticks: Option<[u32; 2]>,
}
fn one_u32() -> u32 {
    1
}
/// [`ProjectileDef::children`]: one set as an object, or a list.
fn one_or_many<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Children>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(Children),
        Many(Vec<Children>),
    }
    Ok(match Option::<OneOrMany>::deserialize(d)? {
        None => Vec::new(),
        Some(OneOrMany::One(c)) => vec![c],
        Some(OneOrMany::Many(c)) => c,
    })
}
/// Damage to everything within `radius` every `every_ticks` while the
/// projectile lives, stuck or flying, under the same rules as its
/// explosion's splash. Unlike an explosion it does not fall off with
/// distance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aura {
    /// Units, up to 16.
    pub radius: f32,
    /// Each pulse, up to 100.
    pub damage: f32,
    /// Ticks between pulses, 4 to 1200.
    pub every_ticks: u32,
    /// `$DamageType::<name>` for the kill message; empty is the
    /// projectile's `radius_damage_type`.
    #[serde(default)]
    pub damage_type: String,
    /// Sets those it hurts burning this long, up to 30 seconds.
    #[serde(default)]
    pub burn_seconds: f32,
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
            max_bounces: 0,
            children: Vec::new(),
            aura: None,
            slow: None,
            fixed_damage: false,
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
    /// Projectiles this pack's images launch that a package it depends on
    /// declares: Tier 2's guns fire Tier 1's `weapon_package_tier1:projectile/...`,
    /// an Add-On gun the base game's `v20.projectile.gunprojectile`.
    /// They resolve when the packs are merged ([`Pack::merge`]): an image
    /// whose projectile no merged package provides is dropped there.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub external_projectiles: BTreeSet<String>,
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
        self.named_damage_type(reference)
            .or_else(|| self.damage_types.get("default"))
    }
    /// Whether the pack has the damage type `reference` names itself,
    /// not only the `default` [`Pack::damage_type`] falls back to.
    pub fn has_damage_type(&self, reference: &str) -> bool {
        self.named_damage_type(reference).is_some()
    }
    fn named_damage_type(&self, reference: &str) -> Option<&DamageType> {
        let name = reference.trim();
        let name = match name.get(..13) {
            Some(prefix) if prefix.eq_ignore_ascii_case("$damagetype::") => &name[13..],
            _ => name,
        };
        self.damage_types.get(&name.to_ascii_lowercase())
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
        ensure!(
            self.external_projectiles.len() <= 4096
                && self.external_projectiles.iter().all(|p| {
                    !p.is_empty()
                        && p.len() <= 128
                        && !p.chars().any(char::is_control)
                        && !self.projectiles.contains_key(p)
                }),
            "Invalid external_projectiles: at most 4096 ids of up to 128 characters \
             this pack does not declare itself"
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
                (item.hidden || !item.ui_name.trim().is_empty())
                    && item.ui_name.len() <= 128
                    && !item.ui_name.chars().any(char::is_control),
                "Item {id} needs a ui_name, the name players pick it by, unless hidden"
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
                        .is_none_or(is_image_command)
                    && image
                        .commands
                        .unmount
                        .as_deref()
                        .is_none_or(is_image_command)
                    && image.commands.mount.as_deref().is_none_or(is_image_command),
                "Invalid image command {id}"
            );
            let shot_ok = |s: &Shot| {
                (1..=64).contains(&s.projectiles)
                    && (0.0..=1.0).contains(&s.spread)
                    && (0.0..=100.0).contains(&s.recoil)
                    && s.recoil_vertical.is_none_or(|v| (0.0..=100.0).contains(&v))
                    && s.moving_spread.is_none_or(|m| (0.0..=1.0).contains(&m))
                    && (0.0..=50.0).contains(&s.moving_speed)
                    && (0.1..=10.0).contains(&s.scale)
                    && s.kick.is_none_or(|k| {
                        (0.0..=1.0).contains(&k.amplitude)
                            && (0.1..=30.0).contains(&k.frequency)
                            && (0.05..=2.0).contains(&k.seconds)
                    })
                    && s.rested.as_ref().is_none_or(|r| {
                        (1..=1200).contains(&r.after_ticks) && (0.0..=1.0).contains(&r.spread)
                    })
            };
            let volleys_ok = |volleys: &[Volley]| {
                volleys.len() <= 4
                    && volleys.iter().all(|v| {
                        (1..=64).contains(&v.projectiles) && (0.0..=1.0).contains(&v.spread)
                    })
            };
            ensure!(
                image.last_shot.as_ref().is_none_or(|l| shot_ok(&l.shot)
                    && l.shot.hitscan.is_none()
                    && volleys_ok(&l.volleys))
                    && image.last_shot.is_some()
                        == image.magazine.as_ref().is_some_and(|m| m.last_rounds > 0),
                "Invalid last shot of image {id}: a shot and volleys as its own, no hitscan, \
                 with a magazine whose last_rounds is set"
            );
            ensure!(
                image.shot.as_ref().is_none_or(shot_ok),
                "Invalid image shot {id}: 1 to 64 projectiles, spreads 0 to 1, recoil 0 to 100, \
                 moving_speed 0 to 50, scale 0.1 to 10, rested after 1 to 1200 ticks, kick amplitude 0 to 1, \
                 frequency 0.1 to 30, seconds 0.05 to 2"
            );
            ensure!(
                image.state_shots.len() <= 8
                    && image.state_shots.iter().all(|(script, s)| {
                        !script.is_empty()
                            && script.len() <= 64
                            && *script == script.to_ascii_lowercase()
                            && script != "onfire"
                            && shot_ok(s)
                            && s.hitscan.is_none()
                    }),
                "Invalid state shots of image {id}: at most 8, by lowercase state script other \
                 than onfire, each a shot as its own without hitscan"
            );
            if let Some(cook) = &image.cook {
                cook.validate()
                    .with_context(|| format!("image {id}: fuse_ticks 1 to 36000, burst_height -10 to 10, print_ticks 1 to 1200, print_seconds 0 to 10, a lowercase script"))?;
                ensure!(
                    image.projectile.is_some(),
                    "Image {id} cooks but has no projectile to go off"
                );
            }
            ensure!(
                volleys_ok(&image.volleys),
                "Invalid volleys of image {id}: at most 4, each 1 to 64 projectiles, spread 0 to 1"
            );
            if let Some(h) = image.shot.as_ref().and_then(|s| s.hitscan.as_ref()) {
                ensure!(
                    [&h.explosion, &h.flown, &h.player_sound, &h.other_sound]
                        .iter()
                        .all(|t| t.len() <= 128),
                    "Invalid hitscan of image {id}: explosion, flown projectile and sounds \
                     up to 128 bytes"
                );
                ensure!(
                    image.projectile.is_some()
                        && (1.0..=2000.0).contains(&h.range)
                        && h.moving_range.is_none_or(|r| (1.0..=2000.0).contains(&r))
                        && h.tracer.is_none_or(|t| {
                            t.color.iter().all(|c| (0.0..=1.0).contains(c))
                                && t.width > 0.0
                                && t.width <= 1.0
                                && t.seconds > 0.0
                                && t.seconds <= 2.0
                        }),
                    "Invalid hitscan of image {id}: it needs a projectile, range 1 to 2000, \
                     tracer colour 0 to 1, width to 1, seconds to 2"
                );
            }
            ensure!(
                image
                    .eye_rotation
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 360.0),
                "Invalid image eye_rotation {id}"
            );
            ensure!(
                image.hide_nodes.len() <= 16
                    && image.hide_nodes.iter().all(|n| {
                        (1..=32).contains(&n.len())
                            && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    }),
                "Invalid image hide_nodes {id}: up to 16 body node names"
            );
            if let Some(zoom) = &image.zoom {
                zoom.validate()
                    .map_err(|e| anyhow::anyhow!("Invalid image zoom {id}: {e}"))?;
            }
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
                    state.loaded,
                    state.not_loaded,
                ]
                .into_iter()
                .flatten()
                {
                    ensure!(index < image.states.len(), "Invalid state target");
                }
            }
            for p in image.projectile_refs() {
                ensure!(
                    self.projectiles.contains_key(p) || self.external_projectiles.contains(p),
                    "Missing projectile {p} of image {id}: neither this pack's nor \
                     listed in external_projectiles"
                );
            }
            if let Some(magazine) = &image.magazine {
                magazine
                    .validate()
                    .with_context(|| format!("magazine of image {id}"))?;
            }
            if let Some(left) = &image.left_image {
                let held = self
                    .images
                    .get(left)
                    .ok_or_else(|| anyhow::anyhow!("Missing left_image {left} of image {id}"))?;
                ensure!(
                    held.left_image.is_none(),
                    "left_image {left} of image {id} has a left_image of its own"
                );
            }
            ensure!(
                image.scripts.len() <= 16
                    && image.scripts.iter().all(|(script, s)| {
                        !script.is_empty()
                            && script.len() <= 64
                            && script
                                .bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                            && is_sequence_name(&s.arm)
                            && (s.fire || s.projectile.is_none())
                    }),
                "Invalid image scripts {id}: up to 16, lower-case names, arm letters, digits and _, projectile only with fire"
            );
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
                "Invalid projectile scalar {id}"
            );
            ensure!(
                p.elasticity <= 1.0 && p.friction <= 1.0 && p.speed <= 10000.0,
                "Invalid trajectory"
            );
            ensure!(
                p.max_bounces <= 64,
                "Invalid max_bounces of projectile {id}: 0 to 64"
            );
            ensure!(
                p.children.len() <= 4,
                "Projectile {id} has more than 4 sets of children"
            );
            for c in &p.children {
                let child = self.projectiles.get(&c.projectile).ok_or_else(|| {
                    anyhow::anyhow!("Missing child projectile {} of {id}", c.projectile)
                })?;
                ensure!(
                    child.children.is_empty(),
                    "Child projectile {} of {id} has children of its own",
                    c.projectile
                );
                ensure!(
                    (1..=16).contains(&c.count)
                        && (0.0..=500.0).contains(&c.speed)
                        && (0.0..=1.0).contains(&c.inherit)
                        && (c.every_ticks == 0 || c.every_ticks >= 4)
                        && (c.every_ticks > 0 || c.on_bounce || c.on_explode)
                        && c.fuse_ticks.is_none_or(|[a, b]| a <= b && b <= 36_000),
                    "Invalid children of projectile {id}: count 1 to 16, speed 0 to 500, \
                     inherit 0 to 1, every_ticks 0 or at least 4, some moment to throw them, \
                     fuse_ticks rising and at most 36000"
                );
            }
            if let Some(a) = &p.aura {
                ensure!(
                    (0.0..=16.0).contains(&a.radius)
                        && (0.0..=100.0).contains(&a.damage)
                        && (4..=1200).contains(&a.every_ticks)
                        && (0.0..=30.0).contains(&a.burn_seconds),
                    "Invalid aura of projectile {id}: radius to 16, damage to 100, \
                     every_ticks 4 to 1200, burn_seconds to 30"
                );
            }
            if let Some(slow) = p.slow {
                ensure!(
                    (1.0..=10.0).contains(&slow.divisor),
                    "Invalid slow of projectile {id}: divisor 1 to 10"
                );
            }
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
