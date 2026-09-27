use anyhow::{Context, Result, ensure};
use bri_content::{
    brick::{Brick, Catalog},
    collision::{CollisionBody, CollisionLibrary},
};
use bri_world::{Brick as Placed, ContentRef};
use rapier3d::prelude::SharedShape;
use std::{collections::BTreeMap, path::Path};
/// Stock bricks whose behavior comes from their add-on script rather than
/// geometry alone; the session implements each natively.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Special {
    #[default]
    None,
    /// `isWaterBrick`: a swimmable, non-solid liquid volume.
    Water,
    Checkpoint,
    Teledoor,
    TreasureChest,
    /// The open chest shown briefly after a find.
    TreasureChestOpen,
    /// Uncarved pumpkin; a sword hit carves it.
    Pumpkin,
}
#[derive(Clone)]
pub struct Definition {
    pub mesh: Brick,
    pub collision: CollisionBody,
    pub shape: SharedShape,
    pub indestructible: bool,
    pub special: Special,
}
/// World-space box of a placed brick's logical grid volume.
pub fn brick_box(brick: &Placed, mesh: &Brick) -> (glam::Vec3, glam::Vec3) {
    let (x, z) = if brick.quarter_turns % 2 == 1 {
        (mesh.footprint_studs[1], mesh.footprint_studs[0])
    } else {
        (mesh.footprint_studs[0], mesh.footprint_studs[1])
    };
    let half = glam::Vec3::new(
        x as f32 * 0.25,
        mesh.height_plates as f32 * 0.1,
        z as f32 * 0.25,
    );
    let center = glam::Vec3::from(brick.position);
    (center - half, center + half)
}
/// The swimmable volume of a water brick. Presentation comes from the brick's
/// own mesh; this is only the liquid players move through.
pub fn brick_water(brick: &Placed, definition: &Definition) -> Option<bri_content::water::Water> {
    if definition.special != Special::Water {
        return None;
    }
    let (min, max) = brick_box(brick, &definition.mesh);
    let image = || bri_content::environment::Image {
        file: "brick-water".into(),
        source: String::new(),
        sha256: "0".repeat(64),
        width: 1,
        height: 1,
    };
    Some(bri_content::water::Water {
        schema_version: 1,
        node: 0,
        id: "brick-water".into(),
        min: min.to_array(),
        max: max.to_array(),
        repeat_period: None,
        liquid_type: "Water".into(),
        density: 1.0,
        viscosity: 15.0,
        surface: image(),
        shore: image(),
        reflection: None,
        opacity: 0.5,
        wave_amplitude: 0.0,
        flow: [0.0; 2],
        distortion: [0.0, 0.0, 1.0],
        tiles: [1.0; 2],
        depth_mask: false,
        depth_alpha: [0.0; 4],
        reflection_intensity: 0.0,
        parallax: 0.0,
        warnings: vec![],
    })
}
#[derive(Default, Clone)]
pub struct Definitions {
    pub entries: BTreeMap<String, Definition>,
}
impl Definitions {
    pub fn get(&self, brick: &Placed) -> Result<&Definition> {
        let ContentRef::Resolved(id) = &brick.definition else {
            anyhow::bail!("Unresolved brick definition")
        };
        self.entries
            .get(id)
            .with_context(|| format!("Missing native definition {id}"))
    }
    pub fn load(catalog_dir: &Path, content: &Path) -> Result<Self> {
        let catalog: Catalog =
            serde_json::from_slice(&std::fs::read(catalog_dir.join("stock-catalog.json"))?)?;
        let audit: serde_json::Value =
            serde_json::from_slice(&std::fs::read(catalog_dir.join("catalog-audit.json"))?)?;
        let library: CollisionLibrary =
            serde_json::from_slice(&std::fs::read(catalog_dir.join("native-collisions.json"))?)?;
        ensure!(
            catalog.schema_version == 1 && library.schema_version == 1,
            "Unsupported catalog/collision schema"
        );
        let mut collisions: BTreeMap<_, _> = library
            .bodies
            .into_iter()
            .map(|b| (b.id.clone(), b))
            .collect();
        let mut out = Self::default();
        for entry in catalog.bricks {
            let resolved = audit["resolved_meshes"]
                .as_array()
                .context("Missing native mesh bindings")?
                .iter()
                .find(|r| r["id"].as_str() == Some(&entry.id))
                .context("Missing catalog mesh binding")?;
            let file = resolved["native_mesh"]
                .as_str()
                .context("Missing mesh file")?;
            ensure!(
                !file.contains(['/', '\\', ':']),
                "Invalid native mesh filename"
            );
            let mesh: Brick = serde_json::from_slice(&std::fs::read(content.join(file))?)?;
            mesh.validate()?;
            ensure!(mesh.id == entry.mesh_id, "Mesh identity mismatch");
            let collision = collisions
                .remove(&entry.id)
                .context("Missing native collision recipe")?;
            let shape = bri_physics::content::collider(&collision)?
                .build()
                .shared_shape()
                .clone();
            let special = if entry
                .other_properties
                .get("iswaterbrick")
                .is_some_and(|v| v == "true" || v == "1")
            {
                Special::Water
            } else {
                match entry.id.as_str() {
                    "v20/brick/brickcheckpointdata" => Special::Checkpoint,
                    "v20/brick/brickteledoordata" => Special::Teledoor,
                    "v20/brick/bricktreasurechestdata" => Special::TreasureChest,
                    "v20/brick/bricktreasurechestopendata" => Special::TreasureChestOpen,
                    "v20/brick/brickpumpkinbasedata" => Special::Pumpkin,
                    _ => Special::None,
                }
            };
            ensure!(
                out.entries
                    .insert(
                        entry.id,
                        Definition {
                            mesh,
                            collision,
                            shape,
                            indestructible: entry.indestructible,
                            special,
                        }
                    )
                    .is_none(),
                "Duplicate brick definition"
            );
        }
        ensure!(collisions.is_empty(), "Unbound collision recipes");
        Ok(out)
    }
}
