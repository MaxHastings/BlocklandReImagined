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
    ("density", "density"),
    ("drag", "drag"),
    ("maxstepheight", "step_height"),
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
    let standing_width = fields
        .get("boundingbox")
        .and_then(|value| body_box(value))
        .filter(|[x, y, _]| x == y)
        .map(|[x, _, _]| x);
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
            if let Some(n) = number(v).filter(|n| match key {
                "density" | "drag" => *n > 0.0,
                "maxstepheight" => *n >= 0.0,
                _ => true,
            }) {
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
        } else if matches!(
            key,
            "boundingbox" | "crouchboundingbox" | "proneboundingbox"
        ) {
            if let Some([x, y, z]) = body_box(value).filter(|[x, y, _]| x == y) {
                if key == "boundingbox" {
                    // PlayerData stores these in quarter-world units. v20's
                    // Player collision step applies 0.25 independently of
                    // the object's scale (see docs/player-simulation.md).
                    movement.insert("width".into(), json!(x * 0.25));
                    movement.insert("stand_height".into(), json!(z * 0.25));
                } else if standing_width != Some(x) {
                    // The motor has one horizontal width for all stances.
                    // An external base's width is not known at this conversion seam.
                    left_out.push(key.to_owned());
                } else if key == "crouchboundingbox" {
                    movement.insert("crouch_height".into(), json!(z * 0.25));
                } else if fields
                    .get("crouchboundingbox")
                    .is_some_and(|c| body_box(c) == Some([x, y, z]))
                {
                    // The motor has no separate prone stance; an identical box is lossless.
                } else {
                    left_out.push(key.to_owned());
                }
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

/// Static authored boxes, including Torque's vectorScale literal. No script execution.
fn body_box(value: &str) -> Option<[f32; 3]> {
    let value = value.trim();
    let (vector, scale) = if value
        .split_once('(')
        .is_some_and(|(function, _)| function.trim().eq_ignore_ascii_case("vectorscale"))
    {
        let inner = value.split_once('(')?.1.strip_suffix(')')?;
        let (vector, scale) = inner.rsplit_once(',')?;
        (crate::literal(vector.trim()), number(scale.trim())?)
    } else {
        (crate::literal(value), 1.0)
    };
    let mut parts = vector.split_whitespace();
    let mut next = || Some(parts.next()?.parse::<f32>().ok()? * scale);
    let result = [next()?, next()?, next()?];
    (parts.next().is_none()
        && result
            .iter()
            .all(|v| v.is_finite() && *v > 0.0 && *v <= 1000.0))
    .then_some(result)
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
    fn authored_square_boxes_and_static_vector_scale_keep_collision_dimensions() {
        let fields = BTreeMap::from([
            (
                "boundingbox".into(),
                "vectorScale(\"3.5 3.5 1.8\", 4)".into(),
            ),
            (
                "crouchboundingbox".into(),
                "vectorScale(\"3.5 3.5 1.8\", 4)".into(),
            ),
            (
                "proneboundingbox".into(),
                "vectorScale(\"3.5 3.5 1.8\", 4)".into(),
            ),
        ]);
        let converted = convert(&fields, None);
        let m = &converted.archetype["movement"];
        assert_eq!(m["width"], json!(3.5));
        assert_eq!(m["stand_height"], json!(1.8_f32));
        assert_eq!(m["crouch_height"], m["stand_height"]);
        assert!(converted.left_out.is_empty());
        let vanilla = BTreeMap::from([
            (
                "boundingbox".into(),
                "vectorScale(\"1.25 1.25 2.65\", 4)".into(),
            ),
            (
                "crouchboundingbox".into(),
                "vectorScale(\"1.25 1.25 1.0\", 4)".into(),
            ),
        ]);
        let vanilla = convert(&vanilla, None);
        assert_eq!(vanilla.archetype["movement"]["width"], json!(1.25));
        assert_eq!(
            vanilla.archetype["movement"]["stand_height"],
            json!(2.65_f32)
        );
        assert_eq!(vanilla.archetype["movement"]["crouch_height"], json!(1.0));
        for malformed in [
            "vectorScale(\"1 1 1\", $scale)",
            "1 1 nan",
            "1 1 -1",
            "1 1 1 2",
        ] {
            assert!(body_box(malformed).is_none(), "{malformed}");
        }
        let rectangle = BTreeMap::from([("boundingbox".into(), "2 3 4".into())]);
        assert_eq!(convert(&rectangle, None).left_out, ["boundingbox"]);
        assert_eq!(
            body_box("VectorScale (\"1 1 2\", 3)"),
            Some([3.0, 3.0, 6.0])
        );
    }

    #[test]
    fn authored_buoyancy_and_step_keep_motor_bounds() {
        let fields = BTreeMap::from([
            ("density".into(), "0.98".into()),
            ("drag".into(), "0.02".into()),
            ("maxstepheight".into(), "0".into()),
        ]);
        let converted = convert(&fields, None);
        assert!(converted.left_out.is_empty());
        assert_eq!(converted.archetype["movement"]["density"], json!(0.98_f32));
        assert_eq!(converted.archetype["movement"]["drag"], json!(0.02_f32));
        assert_eq!(converted.archetype["movement"]["step_height"], json!(0.0));
        let unsupported = BTreeMap::from([
            ("density".into(), "0".into()),
            ("drag".into(), "0".into()),
            ("maxstepheight".into(), "-1".into()),
        ]);
        assert_eq!(
            convert(&unsupported, None).left_out,
            ["density", "drag", "maxstepheight"]
        );
    }

    #[test]
    fn differing_or_unknown_stance_widths_are_reported_instead_of_lost() {
        let fields = BTreeMap::from([
            ("boundingbox".into(), "1 1 2".into()),
            ("crouchboundingbox".into(), "2 2 1".into()),
            ("proneboundingbox".into(), "2 2 1".into()),
        ]);
        let converted = convert(&fields, None);
        assert_eq!(converted.archetype["movement"]["width"], json!(0.25));
        assert!(
            converted.archetype["movement"]
                .get("crouch_height")
                .is_none()
        );
        assert_eq!(
            converted.left_out,
            ["crouchboundingbox", "proneboundingbox"]
        );
        let external = BTreeMap::from([("crouchboundingbox".into(), "1 1 1".into())]);
        assert_eq!(
            convert(&external, Some("other:body".into())).left_out,
            ["crouchboundingbox"]
        );
    }

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
