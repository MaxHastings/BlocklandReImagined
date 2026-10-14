//! Torque's fxShapeReplicator, placed once at conversion: a replicator
//! scatters `shapeCount` copies of `shapeFile` around itself on the
//! terrain, from its own `seed`, and each copy becomes an ordinary static
//! model of the map. The copies are drawn from the same random sequence as
//! Torque's (`MRandomLCG`), but the original also refused spots by
//! collision with interiors, water and the other copies, which this does
//! only for water and slope, so a forest can differ tree for tree.
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use bri_content::{
    Terrain,
    scene::{Kind, Node, Scene},
};
use glam::{EulerRot, Mat4, Quat, Vec3};
use std::collections::BTreeMap;

/// Torque's Park–Miller generator (`MRandomLCG`).
struct Lcg(i32);
impl Lcg {
    const QUOTIENT: i32 = i32::MAX / 16807;
    const REMAINDER: i32 = i32::MAX % 16807;
    fn next(&mut self) -> i32 {
        if self.0 <= Self::QUOTIENT {
            self.0 = self.0.wrapping_mul(16807) % i32::MAX;
        } else {
            let high = self.0 / Self::QUOTIENT;
            let low = self.0 % Self::QUOTIENT;
            let test = 16807i32
                .wrapping_mul(low)
                .wrapping_sub(Self::REMAINDER.wrapping_mul(high));
            self.0 = if test > 0 {
                test
            } else {
                test.wrapping_add(i32::MAX)
            };
        }
        self.0
    }
    fn range(&mut self, min: f32, max: f32) -> f32 {
        let unit = (f64::from(self.next()) / f64::from(i32::MAX)).abs() as f32;
        min + (max - min) * unit
    }
}

/// A terrain's heights in Torque's frame (z up), repeating as the legacy
/// terrain does.
pub struct Ground<'a> {
    pub terrain: &'a Terrain,
    /// The terrain node's native origin (its transform's translation).
    pub origin: [f32; 3],
    pub spacing: f32,
}
impl Ground<'_> {
    fn sample(&self, x: i64, y: i64) -> f32 {
        let side = i64::from(self.terrain.side);
        self.terrain.elevations[(y.rem_euclid(side) * side + x.rem_euclid(side)) as usize]
    }
    /// The height and the up-facing share of the normal at Torque (x, y).
    fn at(&self, x: f32, y: f32) -> (f32, f32) {
        let gx = (x - self.origin[0]) / self.spacing;
        let gy = (y + self.origin[2]) / self.spacing;
        let (x0, y0) = (gx.floor(), gy.floor());
        let (fx, fy) = (gx - x0, gy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let h00 = self.sample(x0, y0);
        let h10 = self.sample(x0 + 1, y0);
        let h01 = self.sample(x0, y0 + 1);
        let h11 = self.sample(x0 + 1, y0 + 1);
        let height = h00 * (1.0 - fx) * (1.0 - fy)
            + h10 * fx * (1.0 - fy)
            + h01 * (1.0 - fx) * fy
            + h11 * fx * fy;
        let dx = ((h10 - h00) * (1.0 - fy) + (h11 - h01) * fy) / self.spacing;
        let dy = ((h01 - h00) * (1.0 - fx) + (h11 - h10) * fx) / self.spacing;
        (height, 1.0 / (1.0 + dx * dx + dy * dy).sqrt())
    }
}

fn numbers<const N: usize>(p: &BTreeMap<String, String>, key: &str, default: [f32; N]) -> [f32; N] {
    p.get(key)
        .and_then(|v| {
            let v: Vec<f32> = v
                .split_whitespace()
                .take(N)
                .filter_map(|x| x.parse().ok())
                .collect();
            <[f32; N]>::try_from(v).ok()
        })
        .filter(|v| v.iter().all(|x| x.is_finite()))
        .unwrap_or(default)
}
fn flag(p: &BTreeMap<String, String>, key: &str, default: bool) -> bool {
    p.get(key).map_or(default, |v| {
        v.trim() != "0" && !v.trim().eq_ignore_ascii_case("false")
    })
}

/// The highest water surface over Torque (x, y), from the scene's water
/// blocks (each spans its position plus its scale).
fn water_top(scene: &Scene, x: f32, y: f32) -> Option<f32> {
    scene
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, Kind::Water))
        .filter_map(|n| {
            let p = numbers(&n.properties, "position", [0.0; 3]);
            let s = numbers(&n.properties, "scale", [1.0; 3]);
            let repeats = flag(&n.properties, "repeatterrain", true);
            let inside =
                repeats || ((p[0]..=p[0] + s[0]).contains(&x) && (p[1]..=p[1] + s[1]).contains(&y));
            inside.then_some(p[2] + s[2])
        })
        .reduce(f32::max)
}

/// Replace each replicator of `scene` with the static models it scatters
/// on `ground`, returning how many it placed. `asset` maps a mission
/// reference to its asset id.
pub fn expand(
    scene: &mut Scene,
    ground: Option<&Ground>,
    asset: &dyn Fn(&str) -> Result<String>,
) -> Result<usize> {
    let mut placed = Vec::new();
    for node in &scene.nodes {
        let p = &node.properties;
        if p.get("source_class").map(String::as_str) != Some("fxshapereplicator") {
            continue;
        }
        let Some(ground) = ground else {
            continue;
        };
        if !flag(p, "allowonterrain", true) {
            continue;
        }
        let shape = asset(
            p.get("shapefile")
                .context("Shape replicator lacks shapeFile")?,
        )?;
        let count = numbers(p, "shapecount", [10.0])[0].clamped(0.0, 10_000.0) as usize;
        let retries = numbers(p, "shaperetries", [100.0])[0].clamped(1.0, 1000.0) as usize;
        let seed = p
            .get("seed")
            .and_then(|s| s.trim().parse::<i64>().ok())
            .map_or(1376312589, |s| s as i32);
        let at = numbers(p, "position", [0.0; 3]);
        let inner = [
            numbers(p, "innerradiusx", [0.0])[0],
            numbers(p, "innerradiusy", [0.0])[0],
        ];
        let outer = [
            numbers(p, "outerradiusx", [100.0])[0],
            numbers(p, "outerradiusy", [100.0])[0],
        ];
        let slope = numbers(p, "allowedterrainslope", [90.0])[0].clamped(0.0, 90.0);
        let on_water = flag(p, "allowonwater", false);
        let offset = numbers(p, "offsetz", [0.0])[0];
        let rotate = (
            numbers(p, "shaperotatemin", [0.0; 3]),
            numbers(p, "shaperotatemax", [0.0; 3]),
        );
        let scale = (
            numbers(p, "shapescalemin", [1.0; 3]),
            numbers(p, "shapescalemax", [1.0; 3]),
        );
        let fix_aspect = flag(p, "fixshapeaspect", false);
        let mut random = Lcg(seed);
        for _ in 0..count {
            for _ in 0..retries {
                let hx = random.range(inner[0], outer[0]);
                let hy = random.range(inner[1], outer[1]);
                let angle = random.range(0.0, std::f32::consts::TAU);
                let (x, y) = (at[0] + hx * angle.cos(), at[1] + hy * angle.sin());
                let (z, up) = ground.at(x, y);
                if up < slope.to_radians().cos() - 1e-4 {
                    continue;
                }
                if !on_water && water_top(scene, x, y).is_some_and(|top| top > z) {
                    continue;
                }
                let r = [0, 1, 2].map(|k| random.range(rotate.0[k], rotate.1[k]).to_radians());
                let mut s = [0, 1, 2].map(|k| random.range(scale.0[k], scale.1[k]));
                if fix_aspect {
                    s = [s[0]; 3];
                }
                ensure!(
                    s.iter().all(|v| *v > 0.0),
                    "Shape replicator scale must be above 0"
                );
                // Torque (x, y, z) → native (x, z, -y).
                let rotation = Quat::from_euler(EulerRot::ZXY, r[2], r[0], r[1]);
                let transform = Mat4::from_scale_rotation_translation(
                    Vec3::new(s[0], s[2], s[1]),
                    Quat::from_xyzw(rotation.x, rotation.z, -rotation.y, rotation.w),
                    Vec3::new(x, z + offset, -y),
                );
                let mut properties = BTreeMap::from([
                    ("source_class".to_string(), "tsstatic".to_string()),
                    ("shapename".to_string(), p["shapefile"].clone()),
                    (
                        "native_replicated".to_string(),
                        "fxshapereplicator".to_string(),
                    ),
                ]);
                properties.insert("position".into(), format!("{x} {y} {}", z + offset));
                placed.push(Node {
                    name: String::new(),
                    parent: node.parent,
                    kind: Kind::StaticModel,
                    transform: transform.to_cols_array(),
                    asset: Some(shape.clone()),
                    properties,
                });
                break;
            }
        }
    }
    let count = placed.len();
    scene.nodes.extend(placed);
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torque_random_sequence() {
        // MRandomLCG from seed 1: 16807, 282475249, 1622650073.
        let mut r = Lcg(1);
        assert_eq!(
            [r.next(), r.next(), r.next()],
            [16807, 282475249, 1622650073]
        );
    }

    #[test]
    fn copies_land_on_the_terrain_above_water() {
        let side = 16u32;
        let terrain = Terrain {
            schema_version: bri_content::TERRAIN_SCHEMA,
            id: "t".into(),
            side,
            // Rising along x: 0 at x = 0 to 30 at x = 15.
            elevations: (0..side * side).map(|i| (i % side) as f32 * 2.0).collect(),
            layers: vec![],
            primary_layers: vec![0; (side * side) as usize],
        };
        let ground = Ground {
            terrain: &terrain,
            origin: [0.0, 0.0, 0.0],
            spacing: 8.0,
        };
        let properties = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>()
        };
        let mut scene = Scene {
            schema_version: 1,
            id: "s".into(),
            name: "s".into(),
            pending_scripts: vec![],
            nodes: vec![
                Node {
                    name: String::new(),
                    parent: None,
                    kind: Kind::Unadapted,
                    transform: Mat4::IDENTITY.to_cols_array(),
                    asset: None,
                    properties: properties(&[
                        ("source_class", "fxshapereplicator"),
                        ("shapefile", "./tree.dts"),
                        ("shapecount", "20"),
                        ("seed", "1376312589"),
                        ("position", "60 -60 0"),
                        ("outerradiusx", "40"),
                        ("outerradiusy", "40"),
                    ]),
                },
                Node {
                    name: String::new(),
                    parent: None,
                    kind: Kind::Water,
                    transform: Mat4::IDENTITY.to_cols_array(),
                    asset: None,
                    properties: properties(&[
                        ("position", "-1000 -1000 0"),
                        ("scale", "2000 2000 10"),
                        ("repeatterrain", "0"),
                    ]),
                },
            ],
        };
        let placed = expand(&mut scene, Some(&ground), &|r| Ok(format!("v20/{r}"))).unwrap();
        assert_eq!(placed, 20);
        for node in &scene.nodes[2..] {
            let m = Mat4::from_cols_array(&node.transform);
            let at = m.w_axis;
            let (height, _) = ground.at(at.x, -at.z);
            assert!((at.y - height).abs() < 1e-3, "on the ground");
            assert!(at.y >= 10.0, "not under the water");
            assert_eq!(node.asset.as_deref(), Some("v20/./tree.dts"));
        }
    }
}
