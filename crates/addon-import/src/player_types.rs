//! An Add-On's `PlayerData` as a package archetype: the engine fields the
//! motor has constants for, over the datablock it inherits. Packages lay
//! these over a player for a while (`push_archetype`, as Support_AltDatablock
//! pushed them) or make a player one (`set_archetype`).
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// A Torque tick, in seconds: per-tick energy fields become per-second.
const TICK: f32 = 0.032;
/// `PlayerStandardArmor.mass`, which every stock player type keeps: forces
/// over it are speeds.
const STANDARD_MASS: f32 = 90.0;

/// Speeds and angles carried over as they are, by Torque field.
const SAME: &[(&str, &str)] = &[
    ("maxforwardspeed", "forward"),
    ("maxbackwardspeed", "backward"),
    ("maxsidespeed", "sideways"),
    ("maxforwardcrouchspeed", "crouch_forward"),
    ("maxbackwardcrouchspeed", "crouch_backward"),
    ("maxsidecrouchspeed", "crouch_sideways"),
    ("maxunderwaterforwardspeed", "underwater_forward"),
    ("maxunderwaterbackwardspeed", "underwater_backward"),
    ("maxunderwatersidespeed", "underwater_sideways"),
    ("aircontrol", "air_control"),
    ("runsurfaceangle", "slope_degrees"),
    ("jumpsurfaceangle", "jump_surface_degrees"),
    ("maxenergy", "max_energy"),
    ("minjetenergy", "min_jet_energy"),
];
/// Per-tick fields, made per second.
const PER_TICK: &[(&str, &str)] = &[
    ("jetenergydrain", "jet_drain"),
    ("rechargerate", "recharge"),
];
/// Forces, over the mass.
const OVER_MASS: &[(&str, &str)] = &[("runforce", "acceleration"), ("jumpforce", "jump_speed")];
/// Engine fields with no effect at these values (v20's own), so leaving
/// them out loses nothing: running and jumping that cost no energy.
const FREE_AT_ZERO: &[&str] = &[
    "runenergydrain",
    "minrunenergy",
    "jumpenergydrain",
    "minjumpenergy",
];

/// What a `PlayerData` became.
#[derive(Debug)]
pub struct Converted {
    /// The archetype file (`ArchetypeDef`).
    pub archetype: Value,
    /// Fields it set that no archetype field carries, by name.
    pub left_out: Vec<String>,
}

/// `fields`: the datablock's own fields and those of its ancestors in the
/// same Add-On (the nearest wins), lower-case. `base`: the archetype it
/// inherits from outside them, None for v20's standard player.
pub fn convert(fields: &BTreeMap<String, String>, base: Option<String>) -> Converted {
    let mut movement = Map::new();
    let mut left_out = Vec::new();
    let mass = fields
        .get("mass")
        .and_then(|v| number(v))
        .filter(|m| *m > 0.0)
        .unwrap_or(STANDARD_MASS);
    let mut archetype = json!({ "schema_version": 1 });
    for (key, value) in fields {
        let key = key.as_str();
        let v = crate::literal(value);
        if let Some((_, to)) = SAME.iter().find(|(k, _)| *k == key) {
            if let Some(n) = number(v) {
                movement.insert((*to).into(), json!(n));
                continue;
            }
        } else if let Some((_, to)) = PER_TICK.iter().find(|(k, _)| *k == key) {
            if let Some(n) = number(v) {
                movement.insert((*to).into(), json!(n / TICK));
                continue;
            }
        } else if let Some((_, to)) = OVER_MASS.iter().find(|(k, _)| *k == key) {
            if let Some(n) = number(v) {
                movement.insert((*to).into(), json!(n / mass));
                continue;
            }
        } else if key == "canjet" {
            if let Some(b) = flag(v) {
                movement.insert("can_jet".into(), json!(b));
                continue;
            }
        } else if key == "maxdamage" {
            if let Some(n) = number(v) {
                archetype["max_health"] = json!(n);
                continue;
            }
        } else if key == "showenergybar" {
            if let Some(b) = flag(v) {
                archetype["energy_bar"] = json!(b);
                continue;
            }
        } else if key == "uiname" {
            archetype["name"] = json!(v);
            continue;
        } else if key == "mass" || (FREE_AT_ZERO.contains(&key) && number(v) == Some(0.0)) {
            continue;
        }
        left_out.push(key.to_owned());
    }
    if let Some(base) = base {
        archetype["base"] = json!(base);
    }
    if !movement.is_empty() {
        archetype["movement"] = Value::Object(movement);
    }
    Converted {
        archetype,
        left_out,
    }
}

/// A number, or a product or quotient of numbers as datablocks write them
/// (`25 * 180`, `8.3 * 90`).
fn number(v: &str) -> Option<f32> {
    let mut words = v.split_whitespace();
    let mut n: f32 = words.next()?.parse().ok()?;
    while let Some(op) = words.next() {
        let next: f32 = words.next()?.parse().ok()?;
        n = match op {
            "*" => n * next,
            "/" if next != 0.0 => n / next,
            _ => return None,
        };
    }
    n.is_finite().then_some(n)
}

fn flag(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_machine_gunner_keeps_what_the_motor_has() {
        let fields: BTreeMap<String, String> = [
            ("runforce", "25 * 180"),
            ("runenergydrain", "0"),
            ("maxforwardspeed", "4"),
            ("maxforwardcrouchspeed", "0"),
            ("jumpforce", "9 * 90"),
            ("canjet", "0"),
            ("jetenergydrain", "0.5"),
            ("uiname", "\"\""),
            ("showenergybar", "false"),
            ("firstpersononly", "1"),
            ("issurvivor", "1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        let c = convert(&fields, None);
        let m = &c.archetype["movement"];
        assert_eq!(m["acceleration"], json!(50.0));
        assert_eq!(m["forward"], json!(4.0));
        assert_eq!(m["crouch_forward"], json!(0.0));
        assert_eq!(m["jump_speed"], json!(9.0));
        assert_eq!(m["can_jet"], json!(false));
        assert!((m["jet_drain"].as_f64().unwrap() - 15.625).abs() < 1e-4);
        assert_eq!(c.archetype["name"], json!(""));
        assert_eq!(c.archetype["energy_bar"], json!(false));
        assert!(c.archetype.get("base").is_none());
        assert_eq!(c.left_out, ["firstpersononly", "issurvivor"]);
        assert_eq!(number("8.3 * 90"), Some(8.3 * 90.0));
        assert_eq!(number("$foo"), None);
    }
}
