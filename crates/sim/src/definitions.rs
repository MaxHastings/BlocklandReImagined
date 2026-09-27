use anyhow::{Context, Result, ensure};
use bri_content::{
    brick::{Brick, Catalog},
    collision::{CollisionBody, CollisionLibrary},
};
use bri_world::{Brick as Placed, ContentRef};
use rapier3d::prelude::SharedShape;
use std::{collections::BTreeMap, path::Path};
#[derive(Clone)]
pub struct Definition {
    pub mesh: Brick,
    pub collision: CollisionBody,
    pub shape: SharedShape,
    pub indestructible: bool,
    pub requires_behavior_adapter: bool,
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
            let requires_behavior_adapter = entry
                .other_properties
                .get("iswaterbrick")
                .is_some_and(|v| v == "true" || v == "1")
                // These stock declarations depend on script callbacks, not just
                // geometry. Keep the gap explicit until native adapters exist.
                || matches!(entry.id.as_str(),
                    "v20/brick/brickpumpkinbasedata" |
                    "v20/brick/bricktreasurechestdata" |
                    "v20/brick/bricktreasurechestopendata" |
                    "v20/brick/brickcheckpointdata" |
                    "v20/brick/brickteledoordata");
            ensure!(
                out.entries
                    .insert(
                        entry.id,
                        Definition {
                            mesh,
                            collision,
                            shape,
                            indestructible: entry.indestructible,
                            requires_behavior_adapter
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
