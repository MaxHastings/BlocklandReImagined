//! Live tuning of an Add-On body: the numbers in its archetype file
//! (`movement`, `camera`, `sockets`, `mount_points`, `ground_speeds`...)
//! changed while the game runs, so "doesn't feel right" is fixed in
//! minutes rather than edit, relaunch, re-host. The host applies a change
//! to the archetype file's JSON, rebuilds the body from it exactly as the
//! game loads it, and every player gets the new body; the host's Tune
//! window can then write the same changes into the Add-On's file.
//!
//! A field is named by its path in the archetype file, dot-separated, with
//! array indices as numbers: `movement.jump_speed`,
//! `sockets.right_hand.offset.1`, `mount_points.0.position.2`. Paths are
//! the file's own, so whatever the file can say can be tuned, and a field
//! the file leaves out is added with the value the body has now.
use crate::archetype::Archetype;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value};

/// Most changes one `/tune` applies.
pub const MAX_CHANGES: usize = 64;
/// Longest field path.
pub const MAX_PATH: usize = 96;

/// What a field tunes, for grouping in the Tune window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Movement,
    /// Where the body holds items and sees from: the hand and eye sockets
    /// and the eye heights.
    Hands,
    Camera,
    /// Sitting and being sat on: the seat socket, the seated eye and the
    /// body's own rider seats.
    Seats,
    Animation,
    Health,
}
impl Group {
    pub const ALL: [Group; 6] = [
        Group::Movement,
        Group::Hands,
        Group::Camera,
        Group::Seats,
        Group::Animation,
        Group::Health,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Group::Movement => "Movement",
            Group::Hands => "Hands and eye",
            Group::Camera => "Camera",
            Group::Seats => "Seats",
            Group::Animation => "Animation",
            Group::Health => "Health",
        }
    }
}

/// One number of a body the Tune window shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Its path in the archetype file.
    pub path: String,
    pub group: Group,
    pub label: String,
    /// The body's value now.
    pub value: f32,
}
impl Field {
    /// A slider's span around the value: offsets and positions move either
    /// way, everything else from zero up to well past the value. The text
    /// box beside it takes any value the archetype accepts.
    pub fn range(&self) -> (f32, f32) {
        let signed = self.path.contains(".offset.")
            || self.path.contains(".position.")
            || self.path.contains("eye_offset")
            || self.path.ends_with("forward")
            || self.path.starts_with("camera.third_person");
        let span = self.value.abs().max(1.0);
        if signed {
            (self.value - span, self.value + span)
        } else {
            (0.0, (self.value.abs() * 2.0).max(1.0))
        }
    }
}

/// "jump_speed" as "Jump speed".
fn words(key: &str) -> String {
    let mut out = key.replace('_', " ");
    if let Some(first) = out.get(0..1) {
        let upper = first.to_ascii_uppercase();
        out.replace_range(0..1, &upper);
    }
    out
}
const AXES: [&str; 3] = ["X", "Y", "Z"];

/// The motor constants grouped elsewhere than Movement.
fn movement_group(key: &str) -> Group {
    match key {
        "stand_eye" | "crouch_eye" | "eye_forward" => Group::Hands,
        "sit_eye" | "sit_eye_forward" => Group::Seats,
        _ => Group::Movement,
    }
}

/// Every number of `a` the Tune window offers, in group order.
pub fn fields(a: &Archetype) -> Vec<Field> {
    let mut out = Vec::new();
    let mut push = |path: String, group: Group, label: String, value: f32| {
        if value.is_finite() {
            out.push(Field {
                path,
                group,
                label,
                value,
            });
        }
    };
    if let Ok(Value::Object(movement)) = serde_json::to_value(&a.movement) {
        for (key, v) in &movement {
            // Floats only: flags, enums and tick counts are not dials.
            if let Some(v) = v.as_f64().filter(|_| v.is_f64()) {
                push(
                    format!("movement.{key}"),
                    movement_group(key),
                    words(key),
                    v as f32,
                );
            }
        }
    }
    for (name, socket) in [
        ("right_hand", a.sockets.hand(0)),
        ("left_hand", a.sockets.hand(1)),
        ("eye", Some(a.sockets.eye())),
        ("seat", Some(a.sockets.seat.clone().unwrap_or_default())),
    ] {
        let Some(socket) = socket else { continue };
        let group = if name == "seat" {
            Group::Seats
        } else {
            Group::Hands
        };
        for (i, axis) in AXES.iter().enumerate() {
            push(
                format!("sockets.{name}.offset.{i}"),
                group,
                format!("{} {axis}", words(name)),
                socket.offset[i],
            );
        }
    }
    push(
        "camera_distance".into(),
        Group::Camera,
        "Third person distance".into(),
        a.look.camera_distance,
    );
    let camera = &a.look.camera;
    push(
        "camera.fov".into(),
        Group::Camera,
        "Field of view".into(),
        camera.fov.unwrap_or(90.0),
    );
    for (i, axis) in AXES.iter().enumerate() {
        push(
            format!("camera.eye_offset.{i}"),
            Group::Camera,
            format!("First person offset {axis}"),
            camera.eye_offset[i],
        );
    }
    // Optional camera parts, shown with the values they have when added.
    let part = |name: &str, current: Option<Value>| -> Map<String, Value> {
        let value = current.unwrap_or_else(|| Value::Object(Map::new()));
        let filled = match name {
            "bob" => serde_json::from_value::<bri_content::camera::Bob>(value)
                .ok()
                .and_then(|b| serde_json::to_value(b).ok()),
            "sway" => serde_json::from_value::<bri_content::camera::Sway>(value)
                .ok()
                .and_then(|s| serde_json::to_value(s).ok()),
            _ => serde_json::from_value::<bri_content::camera::ThirdPerson>(value)
                .ok()
                .and_then(|t| serde_json::to_value(t).ok()),
        };
        match filled {
            Some(Value::Object(m)) => m,
            _ => Map::new(),
        }
    };
    for (name, current) in [
        ("bob", camera.bob.and_then(|b| serde_json::to_value(b).ok())),
        (
            "sway",
            camera.sway.and_then(|s| serde_json::to_value(s).ok()),
        ),
        (
            "third_person",
            camera
                .third_person
                .and_then(|t| serde_json::to_value(t).ok()),
        ),
    ] {
        for (key, v) in part(name, current) {
            if let Some(v) = v.as_f64() {
                push(
                    format!("camera.{name}.{key}"),
                    Group::Camera,
                    format!("{} {}", words(name), key.replace('_', " ")),
                    v as f32,
                );
            }
        }
    }
    for (i, seat) in a.mount_points.iter().enumerate() {
        for (j, axis) in AXES.iter().enumerate() {
            push(
                format!("mount_points.{i}.position.{j}"),
                Group::Seats,
                format!("Rider seat {} {axis}", i + 1),
                seat.position[j],
            );
        }
    }
    for (role, speed) in &a.look.ground_speeds {
        push(
            format!("ground_speeds.{role}"),
            Group::Animation,
            format!("{} clip speed", words(role)),
            *speed,
        );
    }
    push(
        "max_time_scale".into(),
        Group::Animation,
        "Fastest clip rate".into(),
        a.look.max_time_scale,
    );
    push(
        "max_health".into(),
        Group::Health,
        "Max health".into(),
        a.max_health,
    );
    out.sort_by_key(|f| f.group);
    out
}

/// The top-level archetype file keys a path may start with.
const TUNABLE: &[&str] = &[
    "movement",
    "sockets",
    "camera_distance",
    "camera",
    "mount_points",
    "ground_speeds",
    "max_time_scale",
    "max_health",
];

/// Split `path` into its keys, refusing one that names nothing tunable.
pub fn parse(path: &str) -> Result<Vec<&str>> {
    ensure!(
        path.len() <= MAX_PATH && !path.is_empty(),
        "A tuning path is 1 to {MAX_PATH} characters"
    );
    let keys: Vec<&str> = path.split('.').collect();
    ensure!(
        keys.iter()
            .all(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')),
        "`{path}` is not a field path"
    );
    ensure!(
        TUNABLE.contains(&keys[0]),
        "`{path}` is not a number the Tune window changes"
    );
    Ok(keys)
}

/// What the archetype file gains for `keys` (a path's first keys) when it
/// leaves them out: the part as `a` has it now, so adding one value
/// changes nothing else. None when a number goes there.
pub fn seed(a: &Archetype, keys: &[&str]) -> Option<Value> {
    let empty = || Some(Value::Object(Map::new()));
    match keys {
        ["movement" | "ground_speeds" | "sockets" | "camera"] => empty(),
        ["camera", "bob" | "sway" | "third_person"] => empty(),
        ["camera", "eye_offset"] => serde_json::to_value(a.look.camera.eye_offset).ok(),
        ["sockets", name] => {
            let socket = match *name {
                "right_hand" => a.sockets.hand(0),
                "left_hand" => a.sockets.hand(1),
                "eye" => Some(a.sockets.eye()),
                "seat" => Some(a.sockets.seat.clone().unwrap_or_default()),
                _ => None,
            }?;
            serde_json::to_value(socket).ok()
        }
        ["sockets", name, "offset"] => {
            let socket = match *name {
                "right_hand" => a.sockets.hand(0),
                "left_hand" => a.sockets.hand(1),
                "eye" => Some(a.sockets.eye()),
                _ => a.sockets.seat.clone(),
            };
            serde_json::to_value(socket.map_or([0.0; 3], |s| s.offset)).ok()
        }
        ["mount_points"] => serde_json::to_value(&a.mount_points).ok(),
        _ => None,
    }
}

/// `value` as the file writes it: the shortest text that reads back as the
/// same `f32`, so 0.3 is written `0.3`.
pub fn number(value: f32) -> Result<Value> {
    ensure!(value.is_finite(), "A tuned value must be a finite number");
    let shortest: f64 = value.to_string().parse().context("Formatting a number")?;
    serde_json::Number::from_f64(shortest)
        .map(Value::Number)
        .context("A tuned value must be a finite number")
}

/// Set the number at `path` in the archetype file's JSON `def`, adding
/// what the file leaves out from `a`, the body as it is now.
pub fn set(def: &mut Value, a: &Archetype, path: &str, value: f32) -> Result<()> {
    let keys = parse(path)?;
    let number = number(value)?;
    let mut here = def;
    for (i, key) in keys.iter().enumerate() {
        let last = i + 1 == keys.len();
        here = match here {
            Value::Object(map) => {
                if !map.contains_key(*key) {
                    let filler = if last {
                        Value::Null
                    } else {
                        seed(a, &keys[..=i]).with_context(|| {
                            format!("`{path}` is not a number the Tune window changes")
                        })?
                    };
                    map.insert((*key).to_string(), filler);
                }
                map.get_mut(*key).expect("inserted")
            }
            Value::Array(items) => {
                let index: usize = key
                    .parse()
                    .with_context(|| format!("`{path}`: {key} is not an index"))?;
                items
                    .get_mut(index)
                    .with_context(|| format!("`{path}`: there is no item {index}"))?
            }
            _ => bail!("`{path}` goes inside a number"),
        };
    }
    ensure!(
        here.is_number() || here.is_null(),
        "`{path}` is not a number"
    );
    *here = number;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archetype::{Archetype, Archetypes};
    use serde_json::json;

    fn blockhead() -> Archetype {
        Archetypes::default().resolve(Default::default()).clone()
    }

    #[test]
    fn every_listed_field_sets_back_to_its_own_value() {
        let a = blockhead();
        let fields = fields(&a);
        for path in [
            "movement.jump_speed",
            "movement.stand_eye",
            "sockets.right_hand.offset.0",
            "sockets.seat.offset.2",
            "camera_distance",
            "camera.fov",
            "camera.eye_offset.1",
            "camera.bob.stride",
            "max_health",
        ] {
            assert!(fields.iter().any(|f| f.path == path), "{path} listed");
        }
        assert!(
            !fields.iter().any(|f| f.path == "movement.jump_delay_ticks"),
            "tick counts are not dials"
        );
        let groups: Vec<Group> = fields.iter().map(|f| f.group).collect();
        assert!(groups.is_sorted(), "grouped");
        // An empty file takes every field at the body's own value.
        let mut def = json!({ "schema_version": 1 });
        for f in &fields {
            set(&mut def, &a, &f.path, f.value).unwrap_or_else(|e| panic!("{}: {e}", f.path));
        }
        // A hand keeps its node: only the offset is new.
        assert_eq!(def["sockets"]["right_hand"]["node"], "Mount0");
        assert_eq!(def["camera"]["fov"], json!(90.0));
    }

    #[test]
    fn a_path_outside_the_dials_is_refused() {
        let a = blockhead();
        let mut def = json!({ "schema_version": 1, "movement": { "forward": 7 } });
        for bad in [
            "controller",
            "name",
            "movement",
            "sockets.tail.offset.0",
            "mount_points.9.position.0",
            "movement.forward.x",
            "../x",
            "",
        ] {
            assert!(set(&mut def, &a, bad, 1.0).is_err(), "{bad}");
        }
        assert!(set(&mut def, &a, "movement.forward", f32::NAN).is_err());
        set(&mut def, &a, "movement.forward", 9.3).unwrap();
        assert_eq!(def["movement"]["forward"], json!(9.3));
    }
}
