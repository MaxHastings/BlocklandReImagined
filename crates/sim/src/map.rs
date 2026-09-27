//! Native map collision loading shared by headless servers and local sessions.
use anyhow::{Context, Result, ensure};
use bri_content::{
    Terrain,
    interior::Interior,
    scene::{Kind, Scene},
};
use rapier3d::prelude::ColliderBuilder;
use std::path::{Path, PathBuf};

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
    pub colliders: Vec<ColliderBuilder>,
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
    /// Terrain region is an explicit cell rectangle, including periodic repeats.
    /// The caller must load further regions before allowing traversal beyond it.
    pub fn load(root: &Path, map_id: &str, terrain_region: [i32; 4]) -> Result<Self> {
        ensure!(
            terrain_region[2] > terrain_region[0] && terrain_region[3] > terrain_region[1],
            "Invalid terrain region"
        );
        ensure!(
            i64::from(terrain_region[2]) - i64::from(terrain_region[0]) <= 1024
                && i64::from(terrain_region[3]) - i64::from(terrain_region[1]) <= 1024,
            "Terrain region exceeds per-load budget"
        );
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
                Kind::Interior | Kind::Terrain => {
                    let id = node
                        .asset
                        .as_ref()
                        .context("Missing map geometry reference")?;
                    let file = native_file(
                        root,
                        bundle["assets"][id]
                            .as_str()
                            .context("Missing native geometry file")?,
                    )?;
                    let bytes = std::fs::read(file)?;
                    if matches!(node.kind, Kind::Interior) {
                        let interior: Interior = serde_json::from_slice(&bytes)?;
                        interior.validate()?;
                        colliders.push(
                            bri_physics::content::interior_collider(
                                &interior.details[0],
                                transform,
                            )?
                            .user_data(MapSurface::Interior as u128),
                        );
                    } else {
                        let terrain: Terrain = serde_json::from_slice(&bytes)?;
                        terrain.validate()?;
                        let spacing: f32 = node
                            .properties
                            .get("squaresize")
                            .context("Missing terrain spacing")?
                            .parse()?;
                        ensure!(
                            spacing.is_finite() && spacing > 0.0,
                            "Invalid terrain spacing"
                        );
                        colliders.push(
                            bri_physics::content::terrain_collider(
                                &terrain,
                                spacing,
                                terrain_region,
                                transform,
                            )?
                            .user_data(MapSurface::Terrain as u128),
                        );
                    }
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
        ensure!(!colliders.is_empty(), "Native map has no physical geometry");
        Ok(Self {
            scene,
            colliders,
            waters,
            pending_objects,
        })
    }
}
