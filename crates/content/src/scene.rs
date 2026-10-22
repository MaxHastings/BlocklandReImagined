//! Native scene placements and retained declarative environment settings.
use crate::interior::Interior;
use bri_console::Clamp;
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// A `Sun`'s light direction, pointing away from the sun in Torque's Z-up
/// space, from its `azimuth` and `elevation` in degrees (`Sun::packUpdate`
/// with `MathUtils::getVectorFromAngles`). Missions also carry a `direction`
/// field, but it is a stale dynamic field the engine never reads. The trig is
/// passed in so the importer can use portable sines.
pub fn sun_direction(
    azimuth: f32,
    elevation: f32,
    sin: fn(f32) -> f32,
    cos: fn(f32) -> f32,
) -> Vec3 {
    let yaw = azimuth.clamped(0.0, 359.0).to_radians();
    let pitch = elevation.clamped(-360.0, 360.0).to_radians();
    -Vec3::new(sin(yaw) * cos(pitch), cos(yaw) * cos(pitch), sin(pitch)).normalize()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Scene {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub nodes: Vec<Node>,
    /// Provenance-only requirements. Runtime must not execute original scripts.
    #[serde(default)]
    pub pending_scripts: Vec<PendingScript>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PendingScript {
    pub section: String,
    pub first_line: usize,
    pub last_line: usize,
    pub sha256: String,
}
impl PendingScript {
    pub fn diagnostic(&self) -> String {
        format!(
            "Mission behavior requires native adaptation: {} object export, source lines {}-{}, SHA-256 {}",
            self.section, self.first_line, self.last_line, self.sha256
        )
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub parent: Option<usize>,
    pub kind: Kind,
    pub transform: [f32; 16],
    pub asset: Option<String>,
    pub properties: BTreeMap<String, String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Group,
    Metadata,
    Interior,
    Terrain,
    StaticModel,
    DatablockModel,
    Spawn,
    Sky,
    Sun,
    Water,
    Precipitation,
    Foliage,
    Bounds,
    /// An authored map light (a map Add-On's; [`AuthoredLight`]). Its
    /// `properties` hold `type`, `color`, `brightness`, `inner` and `outer`.
    Light,
    /// A map teleporter's entrance or exit ([`Teleporter`]). Its
    /// `properties` hold `role` (`entrance` or `exit`), `channel` and
    /// `radius`.
    Teleporter,
    /// A climb volume (a ladder): `properties` hold `half` (half extents
    /// "x y z" in the node's axes), `speed` (units a second along the
    /// rungs), `strafe` (1 to let the strafe keys move sideways) and an
    /// `id`. The climber faces the node's local +Z, as at a teleporter exit.
    Climb,
    /// A named box a rule can ask about ([`Volume`]): `properties` hold
    /// `half` (half extents "x y z" in the node's axes), an optional `id` and
    /// `priority` (a whole number, higher wins), and any other text a rule
    /// reads back (a place's name, what it is for).
    Volume,
    Unadapted,
}

/// map2dif's light types, which a map light keeps
/// (docs/architecture/map-packages.md).
pub const LIGHT_TYPES: [&str; 7] = [
    "omni", "spot", "strobe", "pulse", "pulse2", "flicker", "runway",
];

/// A [`Kind::Light`] node read: where it is, its type and how it lights.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthoredLight {
    pub node: usize,
    pub kind: String,
    pub position: [f32; 3],
    /// Linear colour times brightness.
    pub color: [f32; 3],
    /// Full brightness within `inner`, none beyond `outer`.
    pub inner: f32,
    pub outer: f32,
    /// How a strobe, pulse, pulse2, flicker or runway light changes.
    pub animation: Option<crate::light_animation::LightAnimation>,
    /// A spot light's cone.
    pub spot: Option<SpotCone>,
}

/// Where a spot light shines: full light within `inner` degrees of
/// `direction` (the node's +Z, the native frame's turn of glTF's -Z),
/// fading to none at `outer`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotCone {
    pub direction: [f32; 3],
    pub inner: f32,
    pub outer: f32,
}

/// Height of one brick plate: v20 stacks bricks on this lattice, anchored at
/// world height zero.
pub const PLATE_HEIGHT: f32 = 0.2;

impl Scene {
    /// The height to move this map by so the interior floor under its first
    /// spawn lies on the nearest plate plane. Map floors need not lie on the
    /// lattice: Bedroom's carpet is at 286.312, so v20 rests bricks 0.088
    /// above it (every stock Bedroom save has its baseplates at 286.4), and
    /// Tutorial's floor is at 94.406, where its own saved bricks dip 0.006 in.
    /// The nearest plane is where v20's shipped layouts sit on every stock
    /// map, so moving the whole map there, rather than moving the lattice,
    /// keeps those layouts in place and brings the floor to meet them. Maps
    /// without an interior floor under the spawn (terrain maps) do not move.
    pub fn floor_lift<'a>(&self, interiors: impl Fn(&str) -> Option<&'a Interior>) -> f32 {
        let Some(spawn) = self.nodes.iter().find(|n| matches!(n.kind, Kind::Spawn)) else {
            return 0.0;
        };
        let spawn = Vec3::new(
            spawn.transform[12],
            spawn.transform[13],
            spawn.transform[14],
        );
        let mut floor = None::<f32>;
        for node in self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Interior))
        {
            let Some(interior) = node.asset.as_deref().and_then(&interiors) else {
                continue;
            };
            let Some(detail) = interior.details.first() else {
                continue;
            };
            let matrix = Mat4::from_cols_array(&node.transform);
            for triangle in &detail.collision_triangles {
                let [a, b, c] = triangle.map(|p| matrix.transform_point3(Vec3::from(p)));
                let flat = (a.y - b.y).abs() < 1e-3 && (a.y - c.y).abs() < 1e-3;
                if flat
                    && a.y <= spawn.y
                    && floor.is_none_or(|f| a.y > f)
                    && covers([a, b, c], spawn)
                {
                    floor = Some(a.y);
                }
            }
        }
        floor.map_or(0.0, |floor| {
            (floor / PLATE_HEIGHT).round() * PLATE_HEIGHT - floor
        })
    }
    /// The plate a flat map stands on: v21's "flat maps", the ones
    /// Steam's Environment window lays a ground and a sea over. A map is
    /// flat when it has no terrain and no models, every interior is
    /// collision only (draws nothing, as v20's `slate.dif` plate does), and
    /// a level face of them lies under the first spawn. None for every
    /// other map: a ground or a sea there would cut through what it built.
    pub fn plate<'a>(&self, interiors: impl Fn(&str) -> Option<&'a Interior>) -> Option<Plate> {
        let spawn = self.nodes.iter().find(|n| matches!(n.kind, Kind::Spawn))?;
        let spawn = Vec3::new(
            spawn.transform[12],
            spawn.transform[13],
            spawn.transform[14],
        );
        if self.nodes.iter().any(|n| {
            matches!(
                n.kind,
                Kind::Terrain | Kind::StaticModel | Kind::DatablockModel
            )
        }) {
            return None;
        }
        let mut faces = Vec::new();
        for node in self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Interior))
        {
            let detail = interiors(node.asset.as_deref()?)?.details.first()?;
            if !detail.surfaces.is_empty() {
                return None;
            }
            let matrix = Mat4::from_cols_array(&node.transform);
            for triangle in &detail.collision_triangles {
                let t = triangle.map(|p| matrix.transform_point3(Vec3::from(p)));
                if (t[0].y - t[1].y).abs() < 1e-3 && (t[0].y - t[2].y).abs() < 1e-3 {
                    faces.push(t);
                }
            }
        }
        let top = faces
            .iter()
            .filter(|t| t[0].y <= spawn.y && covers(**t, spawn))
            .map(|t| t[0].y)
            .max_by(f32::total_cmp)?;
        let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in faces
            .iter()
            .filter(|t| (t[0].y - top).abs() < 1e-3)
            .flatten()
        {
            min = [min[0].min(p.x), min[1].min(p.z)];
            max = [max[0].max(p.x), max[1].max(p.z)];
        }
        Some(Plate { top, min, max })
    }
    /// Every [`Kind::Light`] node, read and checked.
    pub fn lights(&self) -> anyhow::Result<Vec<AuthoredLight>> {
        use anyhow::{Context, ensure};
        let number = |node: &Node, key: &str, default: f32| -> anyhow::Result<f32> {
            let value = node
                .properties
                .get(key)
                .map(|v| v.trim().parse::<f32>())
                .transpose()
                .with_context(|| format!("Light {}: {key} is not a number", node.name))?
                .unwrap_or(default);
            ensure!(
                value.is_finite(),
                "Light {}: {key} is not finite",
                node.name
            );
            Ok(value)
        };
        let mut out = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if !matches!(node.kind, Kind::Light) {
                continue;
            }
            let kind = node.properties.get("type").map_or("omni", String::as_str);
            ensure!(
                LIGHT_TYPES.contains(&kind),
                "Light {}: type {kind} is not one of {}",
                node.name,
                LIGHT_TYPES.join(", ")
            );
            let color: Vec<f32> = node
                .properties
                .get("color")
                .map_or("1 1 1", String::as_str)
                .split_whitespace()
                .map(str::parse)
                .collect::<Result<_, _>>()
                .with_context(|| format!("Light {}: color is three numbers", node.name))?;
            ensure!(
                color.len() == 3 && color.iter().all(|c| c.is_finite() && *c >= 0.0),
                "Light {}: color is three numbers of 0 or more",
                node.name
            );
            let brightness = number(node, "brightness", 1.0)?;
            let (inner, outer) = (number(node, "inner", 0.0)?, number(node, "outer", 10.0)?);
            ensure!(
                brightness >= 0.0 && inner >= 0.0 && outer > inner && outer <= 10_000.0,
                "Light {}: needs brightness 0 or more and 0 <= inner < outer <= 10000",
                node.name
            );
            let animation = crate::light_animation::LightAnimation::read(
                kind,
                &node.properties,
                [
                    color[0] * brightness,
                    color[1] * brightness,
                    color[2] * brightness,
                ],
                brightness,
            )
            .with_context(|| format!("Light {}", node.name))?;
            let spot = if kind == "spot" {
                let (inner, outer) = (
                    number(node, "inner_cone", 0.0)?,
                    number(node, "outer_cone", 45.0)?,
                );
                ensure!(
                    (0.0..90.0).contains(&inner) && inner <= outer && outer <= 90.0,
                    "Light {}: needs 0 <= inner_cone <= outer_cone <= 90 degrees",
                    node.name
                );
                let direction = Mat4::from_cols_array(&node.transform)
                    .transform_vector3(Vec3::Z)
                    .normalize_or_zero();
                ensure!(
                    direction != Vec3::ZERO,
                    "Light {}: a spot light needs a direction",
                    node.name
                );
                Some(SpotCone {
                    direction: direction.to_array(),
                    inner,
                    outer,
                })
            } else {
                None
            };
            out.push(AuthoredLight {
                node: index,
                animation,
                spot,
                kind: kind.to_owned(),
                position: [node.transform[12], node.transform[13], node.transform[14]],
                color: [
                    color[0] * brightness,
                    color[1] * brightness,
                    color[2] * brightness,
                ],
                inner,
                outer,
            });
        }
        Ok(out)
    }
    /// Move every placement up by `height` (see [`Scene::floor_lift`]).
    /// Terrain origins and water volumes live outside the scene; callers
    /// raise them by the same height.
    pub fn lift(&mut self, height: f32) {
        for node in &mut self.nodes {
            node.transform[13] += height;
        }
    }
}
/// Most teleporter nodes a scene may hold.
pub const MAX_TELEPORTERS: usize = 256;
/// Largest teleporter entrance radius, units.
pub const MAX_TELEPORTER_RADIUS: f32 = 100.0;

/// A [`Kind::Teleporter`] node read: a player who steps within `radius` of
/// an entrance comes out at an exit of the same `channel`, facing `yaw`.
#[derive(Clone, Debug, PartialEq)]
pub struct Teleporter {
    pub node: usize,
    pub entrance: bool,
    pub channel: String,
    /// Feet position.
    pub position: [f32; 3],
    /// Facing about +Y, as a player's yaw: the node's local +Z (a map
    /// Add-On Empty's Blender +Y), so 0 faces native -Z and a quarter turn
    /// faces +X.
    pub yaw: f32,
    pub radius: f32,
}
/// Most climb volumes a scene may hold.
pub const MAX_CLIMBS: usize = 256;
/// Most [`Kind::Volume`] nodes a scene may hold.
pub const MAX_VOLUMES: usize = 1024;
/// Most extra properties one volume carries, and the longest key or value.
pub const MAX_VOLUME_PROPERTIES: usize = 16;
pub const MAX_VOLUME_TEXT: usize = 128;

/// A [`Kind::Volume`] node read: a box in the map that rules look up by
/// point (`volume_at`). The engine names no kind of volume; what one means is
/// the rules' (a voice zone, a place's name, an objective).
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    pub node: usize,
    pub id: String,
    /// Among volumes holding the same point, the higher comes first.
    pub priority: i32,
    pub center: [f32; 3],
    /// Half extents along the node's own axes.
    pub half: [f32; 3],
    /// The node's transform inverted, to put a point in the box's frame.
    pub to_local: [f32; 16],
    /// Every property but `half`, `id` and `priority`.
    pub properties: BTreeMap<String, String>,
}
impl Volume {
    /// Whether the world point `p` is inside the box (its faces count).
    pub fn contains(&self, p: [f32; 3]) -> bool {
        let local = Mat4::from_cols_array(&self.to_local).transform_point3(Vec3::from(p));
        local.abs().cmple(Vec3::from(self.half)).all()
    }
}
impl Scene {
    /// Every static model with a `mover` property, read and checked:
    /// the node's index and its keyed motion ([`crate::mover`]).
    pub fn movers(&self) -> anyhow::Result<Vec<(usize, crate::mover::MoverDef)>> {
        use anyhow::Context;
        let mut out: Vec<(usize, crate::mover::MoverDef)> = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            let Some(text) = node.properties.get("mover") else {
                continue;
            };
            anyhow::ensure!(
                matches!(node.kind, Kind::StaticModel | Kind::DatablockModel)
                    && node.asset.is_some(),
                "Mover {}: only a static model with an asset can move",
                node.name
            );
            let mut def: crate::mover::MoverDef = serde_json::from_str(text)
                .with_context(|| format!("Mover {}: its definition", node.name))?;
            if def.keys.is_empty() {
                def.keys.push(crate::mover::MoverKey::default());
            }
            def.validate()?;
            anyhow::ensure!(
                !out.iter().any(|(_, other)| other.id == def.id),
                "Two movers are called {}",
                def.id
            );
            out.push((index, def));
        }
        anyhow::ensure!(
            out.len() <= crate::mover::MAX_MOVERS,
            "A map holds at most {} movers",
            crate::mover::MAX_MOVERS
        );
        Ok(out)
    }
    /// Every [`Kind::Climb`] node, read and checked.
    pub fn climbs(&self) -> anyhow::Result<Vec<crate::mover::Climb>> {
        use anyhow::{Context, ensure};
        let mut out = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if !matches!(node.kind, Kind::Climb) {
                continue;
            }
            let numbers = |key: &str| -> anyhow::Result<Vec<f32>> {
                node.properties
                    .get(key)
                    .map(|v| {
                        v.split_whitespace()
                            .map(|w| w.parse::<f32>())
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()
                    .with_context(|| format!("Climb {}: {key} is not numbers", node.name))
                    .map(Option::unwrap_or_default)
            };
            let half = numbers("half")?;
            ensure!(
                half.len() == 3
                    && half
                        .iter()
                        .all(|h| h.is_finite() && *h > 0.0 && *h <= 1000.0),
                "Climb {}: half needs three sizes above 0",
                node.name
            );
            let speed = numbers("speed")?.first().copied().unwrap_or(8.0);
            ensure!(
                speed.is_finite() && speed > 0.0 && speed <= 100.0,
                "Climb {}: speed must be above 0 and at most 100",
                node.name
            );
            let matrix = Mat4::from_cols_array(&node.transform);
            let forward = matrix.transform_vector3(Vec3::Z);
            ensure!(
                matrix.w_axis.is_finite() && forward.is_finite(),
                "Climb {}: placement is not finite",
                node.name
            );
            let yaw = if forward.x.abs() + forward.z.abs() > 1e-6 {
                forward.x.atan2(-forward.z)
            } else {
                0.0
            };
            out.push(crate::mover::Climb {
                node: index,
                id: node
                    .properties
                    .get("id")
                    .cloned()
                    .unwrap_or_else(|| node.name.clone()),
                center: matrix.w_axis.truncate(),
                half: Vec3::new(half[0], half[1], half[2]),
                yaw,
                speed,
                strafe: node
                    .properties
                    .get("strafe")
                    .is_some_and(|v| v.trim() == "1"),
            });
        }
        ensure!(
            out.len() <= MAX_CLIMBS,
            "A map holds at most {MAX_CLIMBS} climb volumes"
        );
        Ok(out)
    }
    /// Every [`Kind::Volume`] node, read and checked, highest `priority`
    /// first and then in scene order.
    pub fn volumes(&self) -> anyhow::Result<Vec<Volume>> {
        use anyhow::{Context, ensure};
        let mut out = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if !matches!(node.kind, Kind::Volume) {
                continue;
            }
            let half = node
                .properties
                .get("half")
                .map(|v| {
                    v.split_whitespace()
                        .map(|w| w.parse::<f32>())
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()
                .with_context(|| format!("Volume {}: half is not numbers", node.name))?
                .unwrap_or_default();
            ensure!(
                half.len() == 3 && half.iter().all(|h| h.is_finite() && *h > 0.0),
                "Volume {}: half needs three sizes above 0",
                node.name
            );
            let priority = node
                .properties
                .get("priority")
                .map(|v| v.trim().parse::<i32>())
                .transpose()
                .with_context(|| format!("Volume {}: priority is not a whole number", node.name))?
                .unwrap_or(0);
            let matrix = Mat4::from_cols_array(&node.transform);
            ensure!(
                matrix.determinant().is_finite() && matrix.determinant().abs() > 1e-9,
                "Volume {}: placement cannot be inverted",
                node.name
            );
            let properties: BTreeMap<String, String> = node
                .properties
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "half" | "id" | "priority"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            ensure!(
                properties.len() <= MAX_VOLUME_PROPERTIES
                    && properties
                        .iter()
                        .all(|(k, v)| k.len() <= MAX_VOLUME_TEXT && v.len() <= MAX_VOLUME_TEXT),
                "Volume {}: at most {MAX_VOLUME_PROPERTIES} properties, each up to {MAX_VOLUME_TEXT} bytes",
                node.name
            );
            out.push(Volume {
                node: index,
                id: node
                    .properties
                    .get("id")
                    .cloned()
                    .unwrap_or_else(|| node.name.clone()),
                priority,
                center: matrix.w_axis.truncate().to_array(),
                half: [half[0], half[1], half[2]],
                to_local: matrix.inverse().to_cols_array(),
                properties,
            });
        }
        ensure!(
            out.len() <= MAX_VOLUMES,
            "A map holds at most {MAX_VOLUMES} volumes"
        );
        out.sort_by_key(|v| std::cmp::Reverse(v.priority));
        Ok(out)
    }
    /// Every [`Kind::Teleporter`] node, read and checked.
    pub fn teleporters(&self) -> anyhow::Result<Vec<Teleporter>> {
        use anyhow::{Context, ensure};
        let mut out = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if !matches!(node.kind, Kind::Teleporter) {
                continue;
            }
            let entrance = match node.properties.get("role").map(String::as_str) {
                Some("entrance") => true,
                Some("exit") => false,
                other => anyhow::bail!(
                    "Teleporter {}: role {other:?} is neither entrance nor exit",
                    node.name
                ),
            };
            let channel = node.properties.get("channel").cloned().unwrap_or_default();
            ensure!(
                channel.len() <= 64,
                "Teleporter {}: channel is longer than 64 bytes",
                node.name
            );
            let radius = node
                .properties
                .get("radius")
                .map(|v| v.trim().parse::<f32>())
                .transpose()
                .with_context(|| format!("Teleporter {}: radius is not a number", node.name))?
                .unwrap_or(1.0);
            ensure!(
                radius.is_finite() && radius > 0.0 && radius <= MAX_TELEPORTER_RADIUS,
                "Teleporter {}: radius must be above 0 and at most {MAX_TELEPORTER_RADIUS}",
                node.name
            );
            let matrix = Mat4::from_cols_array(&node.transform);
            let position = matrix.w_axis.truncate();
            let forward = matrix.transform_vector3(Vec3::Z);
            ensure!(
                position.is_finite() && forward.is_finite(),
                "Teleporter {}: placement is not finite",
                node.name
            );
            let yaw = if forward.x.abs() + forward.z.abs() > 1e-6 {
                forward.x.atan2(-forward.z)
            } else {
                0.0
            };
            out.push(Teleporter {
                node: index,
                entrance,
                channel,
                position: position.to_array(),
                yaw,
                radius,
            });
        }
        ensure!(
            out.len() <= MAX_TELEPORTERS,
            "A map holds at most {MAX_TELEPORTERS} teleporters"
        );
        Ok(out)
    }
}

/// The invisible plate a flat map stands on ([`Scene::plate`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plate {
    /// Its top face's height, in the scene's placement.
    pub top: f32,
    /// The top face's extent, X and Z.
    pub min: [f32; 2],
    pub max: [f32; 2],
}
/// Whether the XZ projection of `triangle` contains `point`.
fn covers([a, b, c]: [Vec3; 3], point: Vec3) -> bool {
    let side = |p: Vec3, q: Vec3| (point.x - q.x) * (p.z - q.z) - (p.x - q.x) * (point.z - q.z);
    let d = [side(a, b), side(b, c), side(c, a)];
    d.iter().all(|&v| v >= 0.0) || d.iter().all(|&v| v <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interior::Detail;

    fn node(kind: Kind, y: f32, asset: Option<&str>) -> Node {
        let mut transform = Mat4::IDENTITY.to_cols_array();
        transform[13] = y;
        Node {
            name: String::new(),
            parent: None,
            kind,
            transform,
            asset: asset.map(str::to_owned),
            properties: BTreeMap::new(),
        }
    }

    #[test]
    fn volumes_hold_points_in_their_own_axes_and_come_in_priority_order() {
        let volume = |name: &str, priority: &str, turn: f32, extra: &[(&str, &str)]| {
            let mut n = node(Kind::Volume, 0.0, None);
            n.name = name.into();
            n.transform = Mat4::from_rotation_translation(
                glam::Quat::from_rotation_y(turn),
                Vec3::new(10.0, 0.0, 0.0),
            )
            .to_cols_array();
            n.properties = BTreeMap::from([
                ("half".into(), "4 2 1".into()),
                ("priority".into(), priority.into()),
            ]);
            n.properties
                .extend(extra.iter().map(|(k, v)| ((*k).into(), (*v).into())));
            n
        };
        let scene = Scene {
            schema_version: 1,
            id: "test".into(),
            name: "test".into(),
            nodes: vec![
                volume("low", "0", 0.0, &[]),
                volume("high", "5", 0.0, &[("place", "Hall")]),
                // A quarter turn puts its long side along Z.
                volume("turned", "0", std::f32::consts::FRAC_PI_2, &[]),
            ],
            pending_scripts: vec![],
        };
        let found = scene.volumes().unwrap();
        let names: Vec<&str> = found.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(names, ["high", "low", "turned"]);
        assert_eq!(found[0].properties["place"], "Hall");
        assert!(!found[0].properties.contains_key("half"));
        assert!(found[0].contains([13.5, 1.0, 0.5]));
        assert!(!found[0].contains([15.0, 0.0, 0.0]));
        // Four units along the turned volume's local X is world Z.
        assert!(found[2].contains([10.0, 0.0, 3.5]));
        assert!(!found[2].contains([13.5, 0.0, 0.0]));
        let mut bad = scene;
        bad.nodes[0]
            .properties
            .insert("half".into(), "4 0 1".into());
        assert!(bad.volumes().is_err());
    }

    #[test]
    fn teleporters_read_their_role_channel_radius_and_facing() {
        let mut entrance = node(Kind::Teleporter, 2.0, None);
        entrance.name = "in".into();
        entrance.properties = BTreeMap::from([
            ("role".into(), "entrance".into()),
            ("channel".into(), "blue".into()),
            ("radius".into(), "2.5".into()),
        ]);
        // A quarter turn about +Y turns local +Z to +X.
        let mut exit = node(Kind::Teleporter, 0.0, None);
        exit.transform = Mat4::from_rotation_translation(
            glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Vec3::new(10.0, 1.0, -4.0),
        )
        .to_cols_array();
        exit.properties = BTreeMap::from([("role".into(), "exit".into())]);
        let scene = Scene {
            schema_version: 1,
            id: "test".into(),
            name: "test".into(),
            nodes: vec![node(Kind::Spawn, 0.0, None), entrance, exit],
            pending_scripts: vec![],
        };
        let found = scene.teleporters().unwrap();
        assert_eq!(found.len(), 2);
        assert!(found[0].entrance);
        assert_eq!(
            (found[0].node, found[0].channel.as_str(), found[0].radius),
            (1, "blue", 2.5)
        );
        assert_eq!(found[0].position, [0.0, 2.0, 0.0]);
        // Unturned, it faces native +Z: half a turn.
        assert!((found[0].yaw.abs() - std::f32::consts::PI).abs() < 1e-5);
        assert!(!found[1].entrance);
        assert_eq!((found[1].channel.as_str(), found[1].radius), ("", 1.0));
        assert!((found[1].yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5);

        let mut bad = scene;
        bad.nodes[1].properties.insert("role".into(), "door".into());
        assert!(bad.teleporters().is_err());
        bad.nodes[1]
            .properties
            .insert("role".into(), "entrance".into());
        bad.nodes[1].properties.insert("radius".into(), "0".into());
        assert!(bad.teleporters().is_err());
    }

    #[test]
    fn floor_under_spawn_moves_to_the_nearest_plate_plane() {
        // Bedroom: an interior placed at 348.812 with its carpet 62.5 below.
        let carpet = [
            [-10.0, -62.5, -10.0],
            [10.0, -62.5, -10.0],
            [0.0, -62.5, 10.0],
        ];
        let interior = Interior {
            schema_version: 1,
            id: "room".into(),
            details: vec![Detail {
                minimum_pixels: 0,
                materials: vec![],
                surfaces: vec![],
                lightmaps: vec![],
                collision_triangles: vec![carpet],
                convex_hulls: vec![],
                ambient: [0; 4],
                alarm_ambient: [0; 4],
                has_alarm: false,
            }],
            subobjects: vec![],
            vehicle_collision: None,
        };
        let mut scene = Scene {
            schema_version: 1,
            id: "room".into(),
            name: "Room".into(),
            nodes: vec![
                node(Kind::Spawn, 287.415, None),
                node(Kind::Interior, 348.812, Some("room")),
            ],
            pending_scripts: vec![],
        };
        let lift = scene.floor_lift(|id| (id == "room").then_some(&interior));
        assert!((lift - 0.088).abs() < 1e-3, "{lift}");
        scene.lift(lift);
        assert!((scene.nodes[1].transform[13] - 62.5 - 286.4).abs() < 1e-3);
        // Nothing under the spawn: the map stays put.
        scene.nodes[0].transform[12] = 100.0;
        assert_eq!(scene.floor_lift(|_| Some(&interior)), 0.0);
    }

    #[test]
    fn a_plate_that_draws_nothing_under_the_spawn_is_flat() {
        let face = |y: f32| {
            vec![
                [[-50.0, y, -50.0], [50.0, y, -50.0], [50.0, y, 50.0]],
                [[-50.0, y, -50.0], [50.0, y, 50.0], [-50.0, y, 50.0]],
                // The plate's underside, lower down.
                [
                    [-50.0, y - 1.0, -50.0],
                    [50.0, y - 1.0, -50.0],
                    [50.0, y - 1.0, 50.0],
                ],
            ]
        };
        let interior = |surfaces| Interior {
            schema_version: 1,
            id: "plate".into(),
            details: vec![Detail {
                minimum_pixels: 0,
                materials: vec![],
                surfaces,
                lightmaps: vec![],
                collision_triangles: face(-0.5),
                convex_hulls: vec![],
                ambient: [0; 4],
                alarm_ambient: [0; 4],
                has_alarm: false,
            }],
            subobjects: vec![],
            vehicle_collision: None,
        };
        let plate = interior(vec![]);
        let mut scene = Scene {
            schema_version: 1,
            id: "slate".into(),
            name: "Slate".into(),
            nodes: vec![
                node(Kind::Spawn, 2.0, None),
                node(Kind::Interior, 1.0, Some("plate")),
                node(Kind::Water, 0.0, None),
            ],
            pending_scripts: vec![],
        };
        assert_eq!(
            scene.plate(|_| Some(&plate)),
            Some(Plate {
                top: 0.5,
                min: [-50.0, -50.0],
                max: [50.0, 50.0]
            })
        );
        // An interior that draws (Bedroom's room) is no plate.
        let room = interior(vec![crate::interior::Surface {
            source_index: 0,
            material: 0,
            flags: 0,
            vertices: vec![],
            triangles: vec![],
            lightmap: None,
            alarm_lightmap: None,
        }]);
        assert_eq!(scene.plate(|_| Some(&room)), None);
        // Nor is a map with terrain or models, or a spawn off the plate.
        scene.nodes.push(node(Kind::StaticModel, 0.0, None));
        assert_eq!(scene.plate(|_| Some(&plate)), None);
        scene.nodes.pop();
        scene.nodes[0].transform[12] = 100.0;
        assert_eq!(scene.plate(|_| Some(&plate)), None);
    }
}

#[cfg(test)]
mod sun_tests {
    use super::sun_direction;
    use glam::Vec3;

    #[test]
    fn sun_angles_point_the_light_away_from_the_sun() {
        // Slate Sea: azimuth 315, elevation 45; its stale `direction` field
        // says 0.577 0.577 -0.577 instead.
        let d = sun_direction(315.0, 45.0, f32::sin, f32::cos);
        assert!(
            (d - Vec3::new(0.5, -0.5, -0.70710677)).length() < 1e-5,
            "{d}"
        );
        // Bedroom Dark's field points straight up; its sun is overhead.
        let d = sun_direction(250.0, 90.0, f32::sin, f32::cos);
        assert!((d - Vec3::NEG_Z).length() < 1e-5, "{d}");
        // Out-of-range azimuths clamp to Torque's 0..359.
        assert_eq!(
            sun_direction(400.0, 30.0, f32::sin, f32::cos),
            sun_direction(359.0, 30.0, f32::sin, f32::cos)
        );
    }
}
