//! `ExplosionData.debris`: the `DebrisData` an explosion throws (vehicle
//! wreckage, the Jeep's tires, the tank shell's spark streaks).
//!
//! The pack keeps every imported datablock's literal fields in
//! [`crate::Pack::definitions`]; this lowers the debris ones to typed values
//! with the engine defaults (`ExplosionData` and `DebrisData` constructors),
//! so current packs need no re-import.
use crate::{Definition, Pack};
use std::collections::BTreeMap;

/// One explosion's debris: how many pieces it throws, where, and how each
/// piece moves (`Explosion::launchDebris`, `Debris::onAdd` / `advanceTime`).
#[derive(Debug, Clone, PartialEq)]
pub struct DebrisSpec {
    /// `DebrisData` name.
    pub name: String,
    /// Model's source path (`Add-Ons/Vehicle_Jeep/jeepTire.dts`), or empty.
    pub model: String,
    /// Trail emitters by name (`emitters`).
    pub emitters: Vec<String>,
    /// `debrisNum` and `debrisNumVariance`.
    pub count: u32,
    pub count_variance: u32,
    /// `debrisThetaMin/Max` (from the explosion's normal) and
    /// `debrisPhiMin/Max` (around it), degrees.
    pub theta: [f32; 2],
    pub phi: [f32; 2],
    /// `debrisVelocity` and `debrisVelocityVariance`.
    pub launch_speed: f32,
    pub launch_variance: f32,
    /// `DebrisData.velocity`/`velocityVariance`: replaces the launch speed
    /// when nonzero.
    pub speed: f32,
    pub speed_variance: f32,
    pub lifetime: f32,
    pub lifetime_variance: f32,
    /// Degrees per second.
    pub spin: [f32; 2],
    pub elasticity: f32,
    pub friction: f32,
    pub bounces: u32,
    pub bounce_variance: u32,
    pub static_on_max_bounce: bool,
    pub snap_on_max_bounce: bool,
    /// Fades out over its last second.
    pub fade: bool,
    /// Times Torque's debris gravity of 9.81.
    pub gravity: f32,
    pub terminal_velocity: f32,
    /// `useRadiusMass` with `baseRadius`.
    pub radius_mass: Option<f32>,
}

fn text(v: &str) -> &str {
    v.trim().trim_matches('"').trim()
}

/// Datablock fields by name, with inheritance (`datablock X(a : b)`),
/// nearest first. Later definitions of a name replace earlier ones, as
/// datablocks do.
struct Fields<'a> {
    by_name: BTreeMap<String, &'a Definition>,
}
impl<'a> Fields<'a> {
    fn new(pack: &'a Pack) -> Self {
        Self {
            by_name: pack
                .definitions
                .iter()
                .map(|d| (d.name.to_ascii_lowercase(), d))
                .collect(),
        }
    }
    fn get(&self, name: &str) -> Option<&'a Definition> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }
    fn field(&self, d: &Definition, key: &str) -> Option<String> {
        let mut at = Some(d);
        for _ in 0..16 {
            let d = at?;
            if let Some(v) = d.fields.get(key) {
                return Some(text(v).to_owned());
            }
            at = d.parent.as_ref().and_then(|p| self.get(p));
        }
        None
    }
    fn num(&self, d: &Definition, key: &str, default: f32) -> f32 {
        self.field(d, key)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }
    fn flag(&self, d: &Definition, key: &str, default: bool) -> bool {
        match self
            .field(d, key)
            .map(|v| v.to_ascii_lowercase())
            .as_deref()
        {
            Some("1" | "true") => true,
            Some("0" | "false") => false,
            _ => default,
        }
    }
    /// A Torque vector field (`"1 -1.3 1"`, z up) in native axes
    /// (x right, y up, -z forward).
    fn vector(&self, d: &Definition, key: &str, default: [f32; 3]) -> [f32; 3] {
        let v: Vec<f32> = self
            .field(d, key)
            .map(|s| {
                s.split_whitespace()
                    .filter_map(|n| n.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        let [x, y, z] = match v[..] {
            [x, y, z] if [x, y, z].iter().all(|c| c.is_finite()) => [x, y, z],
            _ => default,
        };
        [x, z, -y]
    }
    /// A `DebrisData` with the explosion that throws it, or on its own (a
    /// casing: the explosion's fields keep their defaults).
    fn debris(&self, debris: &Definition, e: Option<&Definition>) -> DebrisSpec {
        let (num, flag) = (
            |d: &Definition, k: &str, v: f32| self.num(d, k, v),
            |d: &Definition, k: &str, v: bool| self.flag(d, k, v),
        );
        let explosion = |k: &str, v: f32| e.map_or(v, |e| num(e, k, v));
        let model = self.field(debris, "shapefile").unwrap_or_default();
        let model = match model.strip_prefix("./") {
            // Relative to the script's add-on folder.
            Some(rest) => {
                let folder: Vec<&str> = debris.source.path.split('/').take(2).collect();
                format!("{}/{rest}", folder.join("/"))
            }
            None => model
                .strip_prefix("~/")
                .map_or(model.clone(), |r| format!("base/{r}")),
        };
        let spin = [
            num(debris, "minspinspeed", 0.0),
            num(debris, "maxspinspeed", 0.0),
        ];
        DebrisSpec {
            name: debris.name.clone(),
            model,
            emitters: self
                .field(debris, "emitters")
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            count: explosion("debrisnum", 1.0).clamp(0.0, 1000.0) as u32,
            count_variance: explosion("debrisnumvariance", 0.0).clamp(0.0, 1000.0) as u32,
            theta: [
                explosion("debristhetamin", 0.0).clamp(0.0, 180.0),
                explosion("debristhetamax", 90.0).clamp(0.0, 180.0),
            ],
            phi: [
                explosion("debrisphimin", 0.0).clamp(0.0, 360.0),
                explosion("debrisphimax", 360.0).clamp(0.0, 360.0),
            ],
            launch_speed: explosion("debrisvelocity", 2.0),
            launch_variance: explosion("debrisvelocityvariance", 0.0).abs(),
            speed: num(debris, "velocity", 0.0),
            speed_variance: num(debris, "velocityvariance", 0.0).abs(),
            lifetime: num(debris, "lifetime", 3.0).clamp(0.0, 60.0),
            lifetime_variance: num(debris, "lifetimevariance", 0.0).abs(),
            spin: [spin[0].min(spin[1]), spin[0].max(spin[1])],
            elasticity: num(debris, "elasticity", 0.3),
            friction: num(debris, "friction", 0.2),
            bounces: num(debris, "numbounces", 0.0).clamp(0.0, 64.0) as u32,
            bounce_variance: num(debris, "bouncevariance", 0.0).clamp(0.0, 64.0) as u32,
            static_on_max_bounce: flag(debris, "staticonmaxbounce", false),
            snap_on_max_bounce: flag(debris, "snaponmaxbounce", false),
            fade: flag(debris, "fade", true),
            gravity: num(debris, "gravmodifier", 1.0),
            terminal_velocity: num(debris, "terminalvelocity", 0.0),
            radius_mass: flag(debris, "useradiusmass", false)
                .then(|| num(debris, "baseradius", 1.0).max(0.01)),
        }
    }
}

/// Every explosion that throws debris, keyed by lower-case explosion name.
pub fn explosion_debris(pack: &Pack) -> BTreeMap<String, DebrisSpec> {
    let f = Fields::new(pack);
    let mut out = BTreeMap::new();
    for e in pack
        .definitions
        .iter()
        .filter(|d| d.class.eq_ignore_ascii_case("ExplosionData"))
    {
        let Some(debris) = f
            .field(e, "debris")
            .and_then(|n| f.get(&n))
            .filter(|d| d.class.eq_ignore_ascii_case("DebrisData"))
        else {
            continue;
        };
        out.insert(e.name.to_ascii_lowercase(), f.debris(debris, Some(e)));
    }
    out
}

/// What an image's `stateEjectShell` throws: its `casing` debris and how
/// the image throws it (`ShapeBaseImageData` shell fields, native axes).
#[derive(Debug, Clone, PartialEq)]
pub struct Casing {
    pub debris: DebrisSpec,
    /// `shellExitDir` in the image's frame.
    pub exit_direction: [f32; 3],
    /// `shellExitOffset` from the eject point.
    pub exit_offset: [f32; 3],
    /// `shellExitVariance`, degrees.
    pub exit_variance: f32,
    /// `shellVelocity`.
    pub velocity: f32,
}

/// Every image with a `casing` that names a `DebrisData`, keyed by image id.
/// Defaults are `ShapeBaseImageData`'s.
pub fn casings(pack: &Pack) -> BTreeMap<String, Casing> {
    let f = Fields::new(pack);
    let mut out = BTreeMap::new();
    for (id, image) in &pack.images {
        let Some(debris) = f
            .get(&image.casing)
            .filter(|d| !image.casing.is_empty() && d.class.eq_ignore_ascii_case("DebrisData"))
        else {
            continue;
        };
        let d = f.get(&image.name);
        let num = |k: &str, v: f32| d.map_or(v, |d| f.num(d, k, v));
        let vector = |k: &str, v: [f32; 3]| match d {
            Some(d) => f.vector(d, k, v),
            None => [v[0], v[2], -v[1]],
        };
        out.insert(
            id.clone(),
            Casing {
                debris: f.debris(debris, None),
                exit_direction: vector("shellexitdir", [1.0, 0.0, 1.0]),
                exit_offset: vector("shellexitoffset", [0.0; 3]),
                exit_variance: num("shellexitvariance", 20.0).clamp(0.0, 180.0),
                velocity: num("shellvelocity", 1.0).clamp(0.0, 200.0),
            },
        );
    }
    out
}
