//! What the player's game already knows about the world, offered to Add-On
//! code with the `world.read` capability: where players and vehicles are
//! this frame (as the game draws them), and the public state of the
//! server's Add-Ons. Nothing here is secret: it is what the player's own
//! screen and HUD already show, so reading it needs no more trust than
//! drawing does. The game fills a [`World`] each frame; the sandbox copies
//! out only what the Add-On asks for.
use crate::host::Mesh;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Floats per record `players` writes.
pub const PLAYER_RECORD: usize = 16;
/// Floats per record `vehicles` writes.
pub const VEHICLE_RECORD: usize = 16;
/// Floats per record `entities` writes.
pub const ENTITY_RECORD: usize = 8;
/// Floats `held` writes.
pub const HELD_RECORD: usize = 20;
/// Floats `environment` writes.
pub const ENVIRONMENT_RECORD: usize = 12;
/// Most records one `players` or `vehicles` call copies.
pub const MAX_RECORDS: usize = 1024;
/// Vehicle definitions one Add-On may name with `vehicle_kind`.
pub const MAX_KINDS: usize = 64;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct World {
    /// The viewing player.
    pub local: u64,
    pub players: Vec<Player>,
    pub vehicles: Vec<Vehicle>,
    /// Add-On creatures (package entities) as drawn.
    pub entities: Vec<Entity>,
    /// Public Add-On state the player receives, by package.
    pub state: BTreeMap<String, AddOnState>,
    pub environment: Environment,
    /// The models of the weapon images players hold now, by image id, for
    /// `image_mesh`.
    pub image_meshes: BTreeMap<String, Arc<Mesh>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Player {
    pub id: u64,
    pub alive: bool,
    pub feet: [f32; 3],
    /// Where they see from and the unit direction they look.
    pub eye: [f32; 3],
    pub look: [f32; 3],
    pub velocity: [f32; 3],
    pub crouched: bool,
    /// Their archetype's id (`namespace:archetype/name` or
    /// `v20.player.<datablock>`).
    pub archetype: String,
    /// The weapon image in their right hand (`namespace:image/name`), or
    /// empty.
    pub image: String,
    /// Their weapon images as drawn this frame.
    pub held: Vec<Held>,
}

/// One weapon image a player holds, where the game draws it this frame
/// (their own in first person where their first-person view puts it).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Held {
    pub hand: u8,
    /// Column-major model matrix of the image, in world units.
    pub transform: [f32; 16],
    /// Its `muzzlePoint`, or `None` when its model has none.
    pub muzzle: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Vehicle {
    pub id: u64,
    /// Its definition (`namespace:vehicle/name` or `v20.vehicle.name`).
    pub definition: String,
    pub position: [f32; 3],
    /// Unit quaternion, x y z w.
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
    /// Radius of a sphere around it.
    pub radius: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entity {
    pub id: u64,
    /// Its kind (`namespace:entity/name`).
    pub kind: String,
    /// Where it stands.
    pub feet: [f32; 3],
    pub yaw: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AddOnState {
    pub global: BTreeMap<String, serde_json::Value>,
    pub players: BTreeMap<u64, BTreeMap<String, serde_json::Value>>,
}

/// The scene's lighting, so an Add-On's shading matches the map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// Direction the sunlight travels (unit).
    pub sun_direction: [f32; 3],
    pub sun_color: [f32; 3],
    pub ambient: [f32; 3],
    /// Fog and horizon colour.
    pub sky: [f32; 3],
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            sun_direction: [-0.4, -0.8, -0.45],
            sun_color: [0.8, 0.78, 0.7],
            ambient: [0.35, 0.37, 0.42],
            sky: [0.55, 0.7, 0.9],
        }
    }
}

fn finite(values: impl IntoIterator<Item = f32>) -> impl Iterator<Item = f32> {
    values
        .into_iter()
        .map(|v| if v.is_finite() { v } else { 0.0 })
}

/// `name`'s index in `kinds` as a record number, -1 when it is not there.
fn kind(kinds: &[String], name: &str) -> f32 {
    kinds
        .iter()
        .position(|k| !name.is_empty() && k.eq_ignore_ascii_case(name))
        .map_or(-1.0, |k| k as f32)
}

impl World {
    /// The `players` records: id, flags (1 the viewer, 2 alive, 4
    /// crouched), feet, eye, look, velocity, then their archetype and held
    /// image as indexes into `archetypes` and `images` (the Add-On's
    /// `archetype_kind` and `image_kind` names; -1 for any other).
    pub fn player_records(
        &self,
        archetypes: &[String],
        images: &[String],
        capacity: usize,
    ) -> Vec<f32> {
        let mut out = Vec::new();
        for p in self.players.iter().take(capacity.min(MAX_RECORDS)) {
            let flags = f32::from(
                u8::from(p.id == self.local)
                    | (u8::from(p.alive) << 1)
                    | (u8::from(p.crouched) << 2),
            );
            out.extend(finite(
                [p.id as f32, flags]
                    .into_iter()
                    .chain(p.feet)
                    .chain(p.eye)
                    .chain(p.look)
                    .chain(p.velocity)
                    .chain([kind(archetypes, &p.archetype), kind(images, &p.image)]),
            ));
        }
        out
    }
    /// The `held` record for `player`'s image in `hand`: its model matrix
    /// (16, column-major), the point it fires from (its muzzle, or the
    /// matrix's origin), and flags (1 it has a muzzle). `None` when that
    /// hand holds nothing drawn.
    pub fn held_record(&self, player: u64, hand: u8) -> Option<[f32; HELD_RECORD]> {
        let held = self
            .players
            .iter()
            .find(|p| p.id == player)?
            .held
            .iter()
            .find(|h| h.hand == hand)?;
        let t = held.transform;
        let muzzle = held.muzzle.unwrap_or([t[12], t[13], t[14]]);
        let mut out = [0.0; HELD_RECORD];
        for (o, v) in out.iter_mut().zip(finite(t.into_iter().chain(muzzle))) {
            *o = v;
        }
        out[19] = f32::from(u8::from(held.muzzle.is_some()));
        Some(out)
    }
    /// The `vehicles` records: id, kind (from `kinds`, -1 when unnamed),
    /// position, rotation, velocity, radius, then padding.
    pub fn vehicle_records(&self, kinds: &[String], capacity: usize) -> Vec<f32> {
        let mut out = Vec::new();
        for v in self.vehicles.iter().take(capacity.min(MAX_RECORDS)) {
            out.extend(finite(
                [v.id as f32, kind(kinds, &v.definition)]
                    .into_iter()
                    .chain(v.position)
                    .chain(v.rotation)
                    .chain(v.velocity)
                    .chain([v.radius])
                    .chain([0.0; 3]),
            ));
        }
        out
    }
    /// The `entities` records: id, feet xyz, yaw, then padding.
    pub fn entity_records(&self, capacity: usize) -> Vec<f32> {
        let mut out = Vec::new();
        for e in self.entities.iter().take(capacity.min(MAX_RECORDS)) {
            out.extend(finite(
                [e.id as f32]
                    .into_iter()
                    .chain(e.feet)
                    .chain([e.yaw])
                    .chain([0.0; 3]),
            ));
        }
        out
    }
    pub fn environment_record(&self) -> Vec<f32> {
        let e = &self.environment;
        finite(
            e.sun_direction
                .into_iter()
                .chain(e.sun_color)
                .chain(e.ambient)
                .chain(e.sky),
        )
        .collect()
    }
    /// A number from `package`'s public state: a global key (`player` < 0)
    /// or that player's key. An array gives its `index`th element; true
    /// and false are 1 and 0. NaN when there is no such number.
    pub fn state_number(&self, package: &str, key: &str, player: i64, index: i32) -> f32 {
        let Some(ns) = self.state.get(package) else {
            return f32::NAN;
        };
        let value = if player < 0 {
            ns.global.get(key)
        } else {
            ns.players.get(&(player as u64)).and_then(|m| m.get(key))
        };
        let value = match value {
            Some(serde_json::Value::Array(items)) => {
                usize::try_from(index).ok().and_then(|i| items.get(i))
            }
            other if index == 0 => other,
            _ => None,
        };
        match value {
            Some(serde_json::Value::Number(n)) => n.as_f64().map_or(f32::NAN, |n| n as f32),
            Some(serde_json::Value::Bool(b)) => f32::from(u8::from(*b)),
            _ => f32::NAN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_have_their_documented_shape() {
        let world = World {
            local: 2,
            players: vec![Player {
                id: 2,
                alive: true,
                feet: [1.0, 2.0, 3.0],
                eye: [1.0, 4.0, 3.0],
                look: [0.0, 0.0, -1.0],
                velocity: [f32::NAN, 0.0, 0.0],
                crouched: true,
                archetype: "zoo:archetype/cow".into(),
                image: String::new(),
                held: Vec::new(),
            }],
            vehicles: vec![Vehicle {
                id: 7,
                definition: "ball:vehicle/ball".into(),
                position: [5.0, 1.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                velocity: [1.0, 0.0, 0.0],
                radius: 1.25,
            }],
            ..Default::default()
        };
        let cows = ["other".to_string(), "Zoo:Archetype/Cow".to_string()];
        let p = world.player_records(&cows, &[], 8);
        assert_eq!(p.len(), PLAYER_RECORD);
        assert_eq!(&p[..5], &[2.0, 7.0, 1.0, 2.0, 3.0]);
        assert_eq!(&p[14..], &[1.0, -1.0], "archetype and image kinds");
        assert_eq!(p[10], -1.0, "look");
        assert_eq!(p[11], 0.0, "a non-finite number arrives as 0");
        let v = world.vehicle_records(&["other".into(), "ball:vehicle/ball".into()], 8);
        assert_eq!(v.len(), VEHICLE_RECORD);
        assert_eq!(&v[..2], &[7.0, 1.0]);
        assert_eq!(v[12], 1.25);
        assert_eq!(world.vehicle_records(&[], 8)[1], -1.0);
        assert!(world.player_records(&[], &[], 0).is_empty());
        let with = World {
            entities: vec![Entity {
                id: 5,
                kind: "zoo:entity/cow".into(),
                feet: [1.0, 0.0, 2.0],
                yaw: 0.5,
            }],
            ..Default::default()
        };
        assert_eq!(
            with.entity_records(4),
            [5.0, 1.0, 0.0, 2.0, 0.5, 0.0, 0.0, 0.0]
        );
        assert_eq!(world.environment_record().len(), ENVIRONMENT_RECORD);
        let mut transform = [0.0; 16];
        (transform[0], transform[5], transform[10], transform[15]) = (1.0, 1.0, 1.0, 1.0);
        transform[12..15].copy_from_slice(&[4.0, 5.0, 6.0]);
        let mut holding = world.clone();
        holding.players[0].held = vec![Held {
            hand: 0,
            transform,
            muzzle: None,
        }];
        let h = holding.held_record(2, 0).unwrap();
        assert_eq!(&h[16..], &[4.0, 5.0, 6.0, 0.0], "no muzzle: the origin");
        holding.players[0].held[0].muzzle = Some([4.0, 5.0, 5.0]);
        assert_eq!(&holding.held_record(2, 0).unwrap()[16..], &[4.0, 5.0, 5.0, 1.0]);
        assert!(holding.held_record(2, 1).is_none());
        assert!(holding.held_record(3, 0).is_none());
    }

    #[test]
    fn state_numbers_read_numbers_arrays_and_flags() {
        let mut ns = AddOnState::default();
        ns.global.insert("g".into(), serde_json::json!(4));
        ns.players.insert(
            3,
            [
                ("beam".to_string(), serde_json::json!([1, 12, 0])),
                ("on".to_string(), serde_json::json!(true)),
                ("name".to_string(), serde_json::json!("x")),
            ]
            .into(),
        );
        let world = World {
            state: [("gun".to_string(), ns)].into(),
            ..Default::default()
        };
        assert_eq!(world.state_number("gun", "g", -1, 0), 4.0);
        assert_eq!(world.state_number("gun", "beam", 3, 1), 12.0);
        assert_eq!(world.state_number("gun", "on", 3, 0), 1.0);
        assert!(world.state_number("gun", "beam", 3, 9).is_nan());
        assert!(world.state_number("gun", "name", 3, 0).is_nan());
        assert!(world.state_number("gun", "g", -1, 1).is_nan());
        assert!(world.state_number("other", "g", -1, 0).is_nan());
        assert!(world.state_number("gun", "beam", 4, 0).is_nan());
    }
}
