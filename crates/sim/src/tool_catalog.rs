//! Shared native tool bindings for dedicated and local hosts; no UI dependency.
use crate::session::ToolCatalog;
use anyhow::{Context, Result, ensure};
use bri_content::{brick::Catalog, brick_materials::Bundle, effects::Library};
use std::collections::BTreeSet;

impl ToolCatalog {
    /// Construct the exact server allowlist exposed by the native client menus.
    /// The session additionally checks definition bindings before installation.
    pub fn from_native(catalog: &Catalog, effects: &Library, materials: &Bundle) -> Result<Self> {
        ensure!(
            catalog.schema_version == 1 && catalog.bricks.len() <= 100_000,
            "Invalid native brick catalog"
        );
        effects.validate()?;
        materials.validate()?;
        let mut ids = BTreeSet::new();
        let mut brick_print_aspects = std::collections::BTreeMap::new();
        for brick in &catalog.bricks {
            ensure!(ids.insert(&brick.id), "Duplicate native brick definition");
            if let Some(aspect) = brick.print_aspect_ratio.as_ref().filter(|a| !a.is_empty()) {
                brick_print_aspects.insert(brick.id.clone(), aspect.clone());
            }
        }
        let default = materials
            .resolve("Letters/A")
            .context("Native tool catalog requires the original Letters/A default")?;
        Ok(Self {
            lights: effects
                .lights
                .iter()
                .filter(|e| !e.name.is_empty())
                .map(|e| e.id.clone())
                .collect(),
            emitters: effects
                .emitters
                .iter()
                .filter(|e| !e.name.is_empty())
                .map(|e| e.id.clone())
                .collect(),
            prints: materials
                .prints
                .iter()
                .map(|p| (p.id.clone(), p.aspect.clone()))
                .collect(),
            brick_print_aspects,
            brick_names: catalog
                .bricks
                .iter()
                .filter(|b| !b.display_name.is_empty())
                .map(|b| {
                    let name = format!("{}/{}/{}", b.category, b.subcategory, b.display_name);
                    (b.id.clone(), name)
                })
                .collect(),
            default_print: Some(default.id.clone()),
            items: BTreeSet::new(),
            sounds: BTreeSet::new(),
            vehicles: BTreeSet::new(),
            sound_bricks: catalog
                .bricks
                .iter()
                .filter(|b| b.special_kind.as_deref() == Some("Sound"))
                .map(|b| b.id.clone())
                .collect(),
            vehicle_bricks: catalog
                .bricks
                .iter()
                .filter(|b| b.special_kind.as_deref() == Some("VehicleSpawn"))
                .map(|b| b.id.clone())
                .collect(),
        })
    }
    /// Install the music loops and vehicles the host actually supports.
    pub fn install_special(
        &mut self,
        sounds: impl IntoIterator<Item = String>,
        vehicles: impl IntoIterator<Item = String>,
    ) -> Result<()> {
        let sounds: BTreeSet<_> = sounds.into_iter().collect();
        let vehicles: BTreeSet<_> = vehicles.into_iter().collect();
        ensure!(
            sounds.len() <= 1024 && vehicles.len() <= 1024,
            "Too many sound/vehicle choices"
        );
        for id in sounds.iter().chain(vehicles.iter()) {
            bri_world::ContentRef::Resolved(id.clone()).validate()?;
        }
        self.sounds = sounds;
        self.vehicles = vehicles;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        definitions::Definitions,
        session::{Command, Reply, Session},
        simulation::Simulation,
    };
    use bri_world::{ContentRef, World};
    use glam::Vec3;
    use rapier3d::prelude::{ColliderBuilder, Vector};

    #[test]
    #[ignore = "requires converted native content; explicit headless asset integration"]
    fn native_dedicated_catalog_installs_and_plants_original_default_print() -> Result<()> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog_dir = root.join("content/stock-catalog-004");
        let catalog: Catalog =
            serde_json::from_slice(&std::fs::read(catalog_dir.join("stock-catalog.json"))?)?;
        let effects: Library = serde_json::from_slice(&std::fs::read(
            root.join("content/effects-pass-004/effects.json"),
        )?)?;
        let materials: Bundle = serde_json::from_slice(&std::fs::read(
            root.join("content/brick-materials-002/brick-materials.json"),
        )?)?;
        let tools = ToolCatalog::from_native(&catalog, &effects, &materials)?;
        assert_eq!(
            (
                tools.prints.len(),
                tools.brick_print_aspects.len(),
                tools.lights.len(),
                tools.emitters.len()
            ),
            (77, 7, 13, 102)
        );
        let default = materials.resolve("Letters/A").unwrap().id.clone();
        assert_eq!(tools.default_print.as_deref(), Some(default.as_str()));
        let mut missing = materials.clone();
        missing.prints.retain(|p| p.id != default);
        assert!(ToolCatalog::from_native(&catalog, &effects, &missing).is_err());
        let simulation = Simulation::new(
            World::new("Dedicated test".into(), "fixture".into(), vec![[1.; 4]]),
            Definitions::load(&catalog_dir, &root.join("content/maps-pass-008"))?,
            vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
        )?;
        let mut session = Session::new(simulation);
        session.set_tool_catalog(tools)?;
        let owner = session.join("Builder".into(), Vec3::new(0., 0.05, 0.), false)?;
        let Reply::Planted(id) = session.command(
            owner,
            1,
            Command::Plant {
                definition: "v20/brick/brick2x2fprintdata".into(),
                position: [0.5, 0.1, -3.5],
                quarter_turns: 0,
                color: 0,
            },
        )?
        else {
            anyhow::bail!("Expected planted brick")
        };
        assert_eq!(
            session.snapshot().world.bricks[&id].print,
            Some(ContentRef::Resolved(default.clone()))
        );
        std::fs::create_dir_all(root.join("artifacts/native-dedicated-content"))?;
        std::fs::write(
            root.join("artifacts/native-dedicated-content/tool-catalog.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"schema_version":1,"passed":true,"prints":77,"printable_definitions":7,"lights":13,"emitters":102,"planted_default_print":default,"missing_default_rejected":true,"runtime_legacy_dependencies":false}),
            )?,
        )?;
        Ok(())
    }
}
