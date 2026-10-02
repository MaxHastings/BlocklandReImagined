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
/// Switches carried over as they are, by Torque field.
const FLAGS: &[(&str, &str)] = &[
    ("showenergybar", "energy_bar"),
    ("firstpersononly", "first_person_only"),
    ("thirdpersononly", "third_person_only"),
    ("rideable", "rideable"),
    ("canride", "can_ride"),
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

/// Marks Torque never reads, set for other Add-Ons' scripts to find
/// (Left4Block's survivors): a player type keeps none, as no script of ours
/// asks.
const SCRIPT_MARKS: &[&str] = &["issurvivor"];

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
        } else if key == "uiname" {
            archetype["name"] = json!(v);
            continue;
        } else if let Some((_, to)) = FLAGS.iter().find(|(k, _)| *k == key) {
            if let Some(b) = flag(v) {
                archetype[*to] = json!(b);
                continue;
            }
        } else if key == "cameramaxdist" {
            if let Some(n) = number(v) {
                archetype["camera_distance"] = json!(n);
                continue;
            }
        } else if key == "jumpdelay" {
            // Ticks of v20's 32 ms, as the motor's quarter ticks: a whole
            // count (the motor's u8), never `0.0`, which it refuses.
            if let Some(n) = number(v) {
                movement.insert(
                    "jump_delay_ticks".into(),
                    json!((n * 4.0).round().clamp(0.0, 255.0) as u8),
                );
                continue;
            }
        } else if key == "mass"
            || SCRIPT_MARKS.contains(&key)
            || (FREE_AT_ZERO.contains(&key) && number(v) == Some(0.0))
        {
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
/// (`25 * 180`, `8.3*90`).
pub(crate) fn number(v: &str) -> Option<f32> {
    let v = crate::literal(v);
    let mut terms = v.split(['*', '/']);
    let mut n: f32 = terms.next()?.trim().parse().ok()?;
    for (op, term) in v.chars().filter(|c| matches!(c, '*' | '/')).zip(terms) {
        let term: f32 = term.trim().parse().ok()?;
        n = match op {
            '*' => n * term,
            _ if term != 0.0 => n / term,
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
        assert!(c.left_out.is_empty(), "{:?}", c.left_out);
        assert_eq!(c.archetype["first_person_only"], json!(true));
        assert_eq!(number("8.3 * 90"), Some(8.3 * 90.0));
        assert_eq!(number("$foo"), None);
    }

    /// `jumpDelay` is the motor's whole count of quarter ticks: a float
    /// such as `0.0` failed every join with the Add-On on.
    #[test]
    fn a_jump_delay_is_a_whole_tick_count() {
        for (delay, ticks) in [("0", 0), ("3", 12), ("2.6", 10)] {
            let fields = BTreeMap::from([("jumpdelay".to_owned(), delay.to_owned())]);
            let m = &convert(&fields, None).archetype["movement"];
            assert_eq!(m["jump_delay_ticks"].as_u64(), Some(ticks), "{delay}");
        }
    }
}
