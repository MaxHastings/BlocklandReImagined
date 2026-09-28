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

/// Every explosion that throws debris, keyed by lower-case explosion name.
pub fn explosion_debris(pack: &Pack) -> BTreeMap<String, DebrisSpec> {
    // Later definitions of a name replace earlier ones, as datablocks do.
    let by_name: BTreeMap<String, &Definition> = pack
        .definitions
        .iter()
        .map(|d| (d.name.to_ascii_lowercase(), d))
        .collect();
    // Fields with inheritance (`datablock X(a : b)`), nearest first.
    let field = |d: &Definition, key: &str| -> Option<String> {
        let mut at = Some(d);
        for _ in 0..16 {
            let d = at?;
            if let Some(v) = d.fields.get(key) {
                return Some(text(v).to_owned());
            }
            at = d
                .parent
                .as_ref()
                .and_then(|p| by_name.get(&p.to_ascii_lowercase()).copied());
        }
        None
    };
    let num = |d: &Definition, key: &str, default: f32| {
        field(d, key)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    };
    let flag = |d: &Definition, key: &str, default: bool| match field(d, key)
        .map(|v| v.to_ascii_lowercase())
        .as_deref()
    {
        Some("1" | "true") => true,
        Some("0" | "false") => false,
        _ => default,
    };
    let mut out = BTreeMap::new();
    for e in pack
        .definitions
        .iter()
        .filter(|d| d.class.eq_ignore_ascii_case("ExplosionData"))
    {
        let Some(debris) = field(e, "debris")
            .and_then(|n| by_name.get(&n.to_ascii_lowercase()).copied())
            .filter(|d| d.class.eq_ignore_ascii_case("DebrisData"))
        else {
            continue;
        };
        let model = field(debris, "shapefile").unwrap_or_default();
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
        let spec = DebrisSpec {
            name: debris.name.clone(),
            model,
            emitters: field(debris, "emitters")
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            count: num(e, "debrisnum", 1.0).clamp(0.0, 1000.0) as u32,
            count_variance: num(e, "debrisnumvariance", 0.0).clamp(0.0, 1000.0) as u32,
            theta: [
                num(e, "debristhetamin", 0.0).clamp(0.0, 180.0),
                num(e, "debristhetamax", 90.0).clamp(0.0, 180.0),
            ],
            phi: [
                num(e, "debrisphimin", 0.0).clamp(0.0, 360.0),
                num(e, "debrisphimax", 360.0).clamp(0.0, 360.0),
            ],
            launch_speed: num(e, "debrisvelocity", 2.0),
            launch_variance: num(e, "debrisvelocityvariance", 0.0).abs(),
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
        };
        out.insert(e.name.to_ascii_lowercase(), spec);
    }
    out
}
