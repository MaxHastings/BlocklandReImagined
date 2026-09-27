//! Versioned native weapon content. No legacy parser is linked into this crate.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    pub fn overlaps(&self, other: &Self) -> bool {
        (0..3).all(|a| self.min[a] <= other.max[a] && self.max[a] >= other.min[a])
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    pub sound: String,
    pub emitter: String,
    pub emitter_node: String,
    pub emitter_seconds: f32,
    pub eject_shell: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
pub struct BrickImpact {
    pub radius: f32,
    pub direct: bool,
    pub force: f32,
    pub max_volume: f32,
    pub max_floating_volume: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub path: String,
    pub sha256: String,
    pub native_file: Option<String>,
    pub diagnostics: Vec<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pack {
    pub schema_version: u32,
    pub id: String,
    pub items: BTreeMap<String, Item>,
    pub images: BTreeMap<String, Image>,
    pub projectiles: BTreeMap<String, ProjectileDef>,
    /// Keyed by lower-case damage type name (`$DamageType::<name>`).
    pub damage_types: BTreeMap<String, DamageType>,
    /// Keyed by lower-case explosion datablock name.
    pub explosions: BTreeMap<String, ExplosionInfo>,
    pub definitions: Vec<Definition>,
    pub resources: Vec<Resource>,
    pub diagnostics: Vec<String>,
}
impl Pack {
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= 32 * 1024 * 1024,
            "Weapon pack exceeds byte limit"
        );
        let pack: Self = serde_json::from_slice(bytes)?;
        pack.validate()?;
        Ok(pack)
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
                        .chain(e.sizes.iter().flat_map(|(s, t)| s.iter().copied().chain([*t])))
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
            ensure!(
                id == &item.id && self.images.contains_key(&item.image),
                "Invalid item/image {id}"
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
        for resource in &self.resources {
            if let Some(path) = &resource.native_file {
                ensure!(
                    !path.starts_with('/')
                        && !path.contains(':')
                        && !path.contains('\\')
                        && !path.split('/').any(|part| part == ".."),
                    "Unsafe native resource path"
                );
            }
        }
        Ok(())
    }
}
