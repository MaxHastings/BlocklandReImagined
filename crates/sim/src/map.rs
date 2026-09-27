//! Native map collision loading shared by headless servers and local sessions.
use anyhow::{Context, Result, ensure};
use bri_content::{
    Terrain,
    interior::Interior,
    scene::{Kind, Scene},
    terrain_field::{TerrainField, TerrainInstance},
};
use bri_physics::terrain::{BodyFocusPolicy, Focus, StreamingConfig, TerrainColliders};
use glam::Vec3;
use rapier3d::prelude::{ColliderBuilder, PhysicsWorld};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Classification for read-only client environment queries. Simulation replaces
/// these tags with its own map authority tag when inserting map collision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u128)]
pub enum MapSurface {
    Terrain = 1,
    Interior = 2,
    Static = 3,
}
impl MapSurface {
    pub fn from_tag(tag: u128) -> Option<Self> {
        match tag {
            1 => Some(Self::Terrain),
            2 => Some(Self::Interior),
            3 => Some(Self::Static),
            _ => None,
        }
    }
}

pub struct NativeMap {
    pub scene: Scene,
    /// Interior and static model collision. Terrain is streamed separately.
    pub colliders: Vec<ColliderBuilder>,
    /// Exact, unbounded terrain placements shared by collision, queries and rendering.
    pub terrain: Vec<Arc<TerrainField>>,
    pub waters: Vec<bri_content::water::Water>,
    /// Retained scene objects which this adapter does not yet give collision.
    pub pending_objects: Vec<String>,
}
fn native_file(root: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..",
        "Invalid native asset filename"
    );
    Ok(root.join(name))
}
impl NativeMap {
    /// Permanently loaded terrain around every authored spawn region, so
    /// spawn selection and newly joined players always stand on collision.
    pub fn spawn_anchors(&self) -> Result<Vec<Focus>> {
        self.scene
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Spawn))
            .map(|node| {
                let radius = node
                    .properties
                    .get("radius")
                    .map(|v| v.parse::<f32>())
                    .transpose()?
                    .unwrap_or(0.0);
                ensure!(
                    radius.is_finite() && (0.0..=10000.).contains(&radius),
                    "Invalid authored spawn radius"
                );
                Ok(Focus {
                    center: Vec3::new(node.transform[12], node.transform[13], node.transform[14]),
                    radius: radius + TerrainStream::BODY_MARGIN,
                })
            })
            .collect()
    }
    pub fn load(root: &Path, map_id: &str) -> Result<Self> {
        let bundle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("bundle.json"))?)?;
        let entry = bundle["maps"]
            .as_array()
            .context("Missing native map index")?
            .iter()
            .find(|m| m["id"].as_str() == Some(map_id))
            .context("Unknown native map")?;
        let scene: Scene = serde_json::from_slice(&std::fs::read(native_file(
            root,
            entry["file"].as_str().context("Missing scene filename")?,
        )?)?)?;
        ensure!(
            scene.schema_version == 1 && scene.id == map_id,
            "Native map schema/identity mismatch"
        );
        let mut colliders = Vec::new();
        let waters: Vec<bri_content::water::Water> = bundle
            .get("waters")
            .and_then(|w| w.get(map_id))
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or_default();
        for water in &waters {
            water.validate()?;
            ensure!(
                scene
                    .nodes
                    .get(water.node)
                    .is_some_and(|n| matches!(n.kind, Kind::Water)),
                "Water refers to wrong scene node"
            );
        }
        if bundle.get("waters").and_then(|w| w.get(map_id)).is_some() {
            let expected: Vec<_> = scene
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| matches!(n.kind, Kind::Water))
                .map(|(i, _)| i)
                .collect();
            ensure!(
                waters.iter().map(|w| w.node).collect::<Vec<_>>() == expected,
                "Native water index disagrees with map"
            );
        }
        let mut pending_objects: Vec<_> = scene
            .pending_scripts
            .iter()
            .map(|p| p.diagnostic())
            .collect();
        for node in &scene.nodes {
            ensure!(
                node.transform.iter().all(|v| v.is_finite()),
                "Non-finite map transform"
            );
            let transform = glam::Mat4::from_cols_array(&node.transform);
            match node.kind {
                Kind::Interior => {
                    let id = node.asset.as_ref().context("Missing interior reference")?;
                    let file = native_file(
                        root,
                        bundle["assets"][id]
                            .as_str()
                            .context("Missing native interior file")?,
                    )?;
                    let interior: Interior = serde_json::from_slice(&std::fs::read(file)?)?;
                    interior.validate()?;
                    colliders.push(
                        bri_physics::content::interior_collider(&interior.details[0], transform)?
                            .user_data(MapSurface::Interior as u128),
                    );
                }
                Kind::StaticModel | Kind::DatablockModel if node.asset.is_some() => {
                    let id = node.asset.as_ref().context("Missing static model asset")?;
                    let path = native_file(
                        root,
                        bundle["assets"][id]
                            .as_str()
                            .context("Missing static model file")?,
                    )?;
                    let shape: bri_content::shape::Shape =
                        serde_json::from_slice(&std::fs::read(path)?)?;
                    colliders.extend(
                        bri_physics::content::static_shape_colliders(&shape, transform)?
                            .into_iter()
                            .map(|c| c.user_data(MapSurface::Static as u128)),
                    );
                    if let Some(pending) = node.properties.get("native_behavior_pending") {
                        pending_objects.push(format!("{}: {pending}", node.name));
                    }
                }
                Kind::StaticModel => anyhow::bail!("Missing static model asset: {}", node.name),
                Kind::DatablockModel | Kind::Water | Kind::Unadapted => {
                    pending_objects.push(node.name.clone())
                }
                _ => {}
            }
        }
        let instances: Vec<TerrainInstance> = match bundle
            .get("terrains")
            .context("Map bundle has no converted terrain instances")?
            .get(map_id)
        {
            Some(value) => {
                serde_json::from_value(value.clone()).context("Invalid native terrain instances")?
            }
            None => Vec::new(),
        };
        let terrain = bri_content::terrain_field::map_fields(&scene, instances, |id| {
            let file = native_file(
                root,
                bundle["assets"][id]
                    .as_str()
                    .context("Missing native terrain file")?,
            )?;
            Ok(serde_json::from_slice::<Terrain>(&std::fs::read(file)?)?)
        })?;
        ensure!(
            !colliders.is_empty() || !terrain.is_empty(),
            "Native map has no physical geometry"
        );
        Ok(Self {
            scene,
            colliders,
            terrain,
            waters,
            pending_objects,
        })
    }
}

/// Terrain collision streamed around every moving body of one physics world
/// plus permanent anchors (authored spawns). Queries that may reach beyond the
/// loaded tiles use `cast_ray` against the exact fields instead.
pub struct TerrainStream {
    colliders: TerrainColliders,
    policy: BodyFocusPolicy,
}
impl TerrainStream {
    /// Tile edge in cells; Torque terrain blocks are 256 cells wide.
    const TILE_CELLS: i32 = 64;
    /// Covers the longest body-relative physics query (150-unit tool reach).
    const BODY_MARGIN: f32 = 160.0;

    pub fn new(fields: Vec<Arc<TerrainField>>, tag: u128, anchors: Vec<Focus>) -> Result<Self> {
        let tile_cells = fields
            .iter()
            .fold(Self::TILE_CELLS, |tile, f| gcd(tile, f.side()));
        let mut colliders = TerrainColliders::new(
            fields,
            StreamingConfig {
                tile_cells,
                ..Default::default()
            },
            tag,
        )?;
        colliders.set_anchors(anchors);
        Ok(Self {
            colliders,
            policy: BodyFocusPolicy {
                margin: Self::BODY_MARGIN,
                ..Default::default()
            },
        })
    }
    pub fn fields(&self) -> &[Arc<TerrainField>] {
        self.colliders.fields()
    }
    pub fn active_tiles(&self) -> usize {
        self.colliders.active_count()
    }
    /// Load tiles around every non-fixed body and refresh the query pipeline
    /// when the loaded set changed.
    pub fn update(&mut self, physics: &mut PhysicsWorld) {
        let foci = bri_physics::terrain::body_foci(physics, self.policy);
        if self.colliders.update(physics, &foci).changed() {
            physics.detect_collisions(&(), &());
        }
    }
    /// Nearest exact terrain hit as (distance, normal), independent of loaded
    /// tiles. Distances are in units of the normalized direction.
    pub fn cast_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(f32, Vec3)> {
        cast_terrain(self.fields(), origin, direction, max_distance)
    }
}

/// Nearest exact terrain hit as (distance, normal) over `fields`. Distances
/// are in units of the normalized direction.
pub fn cast_terrain(
    fields: &[Arc<TerrainField>],
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<(f32, Vec3)> {
    bri_physics::terrain::cast_ray_fields(fields, origin, direction, max_distance).map(|hit| {
        // Fields are two-sided; report the face toward the ray like physics queries.
        let normal = if hit.normal.dot(direction) > 0.0 {
            -hit.normal
        } else {
            hit.normal
        };
        (hit.distance, normal)
    })
}

fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{MoveInput, Player, PlayerTuning};
    use bri_content::{TerrainLayer, terrain_field::*};

    fn field(repeat: bool) -> Arc<TerrainField> {
        let side = 128u32;
        let count = (side * side) as usize;
        let elevations = (0..count)
            .map(|i| {
                let (x, y) = ((i % 128) as f32, (i / 128) as f32);
                20.0 + (x * 0.1).sin() * 4.0 + (y * 0.07).cos() * 3.0
            })
            .collect();
        let terrain = Terrain {
            schema_version: 1,
            id: "t".into(),
            side,
            elevations,
            primary_layers: vec![0; count],
            layers: vec![TerrainLayer {
                slot: 0,
                material: "m".into(),
                weights: vec![255; count],
            }],
        };
        let instance = TerrainInstance {
            schema_version: TERRAIN_INSTANCE_SCHEMA,
            node: 0,
            terrain: "t".into(),
            square_size: 8.0,
            origin: [-512.0, 0.0, 512.0],
            repeat,
            repeat_source: RepeatSource::Authored,
            empty_runs: vec![],
            detail: None,
            bump: TerrainBump {
                texture: None,
                scale: 1.0,
                offset: 0.01,
                zero_scale: 8,
            },
            diagnostics: vec![],
        };
        Arc::new(TerrainField::new(terrain, &instance).unwrap())
    }

    #[test]
    fn players_stand_on_streamed_terrain_far_from_the_primary_block() -> Result<()> {
        let field = field(true);
        // Twelve blocks away, far beyond any finite patch around the origin.
        let (x, z) = (12_345.0, -9_876.0);
        let ground = field.height(x, z).unwrap();
        let mut physics = bri_physics::new_world();
        let mut stream = TerrainStream::new(vec![field.clone()], 7, Vec::new())?;
        let mut player = Player::spawn(
            &mut physics,
            1,
            Vec3::new(x, ground + 2.0, z),
            PlayerTuning::default(),
        )?;
        stream.update(&mut physics);
        assert!(stream.active_tiles() > 0);
        for _ in 0..240 {
            stream.update(&mut physics);
            player.step(&mut physics, MoveInput::default())?;
            physics.step();
        }
        let feet = Vec3::from(player.state().feet);
        assert!(player.state().grounded, "{:?}", player.state());
        assert!((feet.y - field.height(feet.x, feet.z).unwrap()).abs() < 0.1);
        // Walking a full block over keeps collision streaming under the body.
        for _ in 0..1200 {
            stream.update(&mut physics);
            player.step(
                &mut physics,
                MoveInput {
                    forward: 1.0,
                    ..Default::default()
                },
            )?;
            physics.step();
        }
        let moved = Vec3::from(player.state().feet);
        assert!(moved.distance(feet) > 20.0, "{moved} vs {feet}");
        assert!((moved.y - field.height(moved.x, moved.z).unwrap()).abs() < 0.5);
        // Exact rays reach terrain even where no tile is loaded.
        let far = Vec3::new(-40_000.0, 200.0, 33_000.0);
        let (distance, normal) = stream.cast_ray(far, Vec3::NEG_Y, 500.0).unwrap();
        assert!((far.y - distance - field.height(far.x, far.z).unwrap()).abs() < 0.01);
        assert!(normal.y > 0.5);
        Ok(())
    }

    #[test]
    fn non_repeating_terrain_has_no_collision_outside_its_block() -> Result<()> {
        let field = field(false);
        let mut physics = bri_physics::new_world();
        let anchors = vec![Focus {
            center: Vec3::new(5_000.0, 20.0, 5_000.0),
            radius: 100.0,
        }];
        let mut stream = TerrainStream::new(vec![field], 7, anchors)?;
        stream.update(&mut physics);
        assert_eq!(stream.active_tiles(), 0);
        assert!(
            stream
                .cast_ray(Vec3::new(5_000.0, 200.0, 5_000.0), Vec3::NEG_Y, 500.0)
                .is_none()
        );
        Ok(())
    }
}
