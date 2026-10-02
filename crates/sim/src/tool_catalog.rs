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
            swap_sounds: catalog
                .bricks
                .iter()
                .filter_map(|b| {
                    let swap = b.swap.as_ref()?;
                    let own = |id: &str| id.rsplit_once('/').map(|(p, _)| p.to_owned());
                    if own(&swap.front) != own(&b.id) || own(&swap.back) != own(&b.id) {
                        return None;
                    }
                    let silent = b
                        .other_properties
                        .get("nobricksounds")
                        .is_some_and(|v| matches!(v.trim().trim_matches('"'), "1" | "true"));
                    let sound = b
                        .other_properties
                        .get("native_swap_sound")
                        .cloned()
                        .or_else(|| {
                            b.other_properties
                                .get("isdoor")
                                .filter(|v| matches!(v.trim().trim_matches('"'), "1" | "true"))
                                .map(|_| "v20/sound/brickchange".to_owned())
                        })?;
                    (!silent).then(|| (b.id.clone(), sound))
                })
                .collect(),
            swaps: catalog
                .bricks
                .iter()
                .filter_map(|b| {
                    let swap = b.swap.as_ref()?;
                    let own = |id: &str| id.rsplit_once('/').map(|(p, _)| p.to_owned());
                    (own(&swap.front) == own(&b.id) && own(&swap.back) == own(&b.id))
                        .then(|| (b.id.clone(), swap.clone()))
                })
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

    /// A made-up brick catalog of a plain plate and one printable brick,
    /// an empty effect library, and two letter prints whose first is the
    /// `Letters/A` default.
    fn synthetic_inputs() -> (Catalog, Library, Bundle, Definitions) {
        use bri_content::{
            brick::CatalogEntry,
            brick_materials::{Image, Package, Print, SURFACES, Source},
        };
        let entry = |id: &str, aspect: Option<&str>| CatalogEntry {
            id: id.into(),
            display_name: id.into(),
            category: "Test".into(),
            subcategory: "Test".into(),
            mesh_id: id.into(),
            collision_source: None,
            icon_source: String::new(),
            print_aspect_ratio: aspect.map(Into::into),
            orientation_fix: 0,
            can_cover: false,
            indestructible: false,
            special_kind: None,
            other_properties: Default::default(),
            reflection: None,
            link: None,
            stretch: None,
            swap: None,
            bot: None,
        };
        let catalog = Catalog {
            schema_version: 1,
            bricks: vec![
                entry(crate::testing::PLATE, None),
                entry(crate::testing::BRICK, Some("2x2f")),
            ],
        };
        let image = |path: &str| {
            let sha256 = format!("{:064x}", path.len());
            Image {
                path: format!("{path}.png"),
                width: 4,
                height: 4,
                sha256: sha256.clone(),
                source: Source {
                    path: format!("test/{path}.png"),
                    archive: None,
                    sha256,
                },
            }
        };
        let print = |name: &str| Print {
            id: format!("print/test_letters/{name}"),
            name: name.into(),
            aspect: "Letters".into(),
            package: "Print_Letters_Test".into(),
            aliases: vec![format!("Letters/{name}")],
            diffuse: image(&format!("prints/{name}")),
            icon: image(&format!("icons/{name}")),
        };
        let materials = Bundle {
            schema_version: bri_content::brick_materials::SCHEMA,
            surfaces: SURFACES
                .iter()
                .map(|s| (s.to_string(), image(&format!("surfaces/{s}"))))
                .collect(),
            prints: vec![print("A"), print("B")],
            packages: vec![Package {
                name: "Print_Letters_Test".into(),
                archive: "Print_Letters_Test.zip".into(),
                archive_sha256: "0".repeat(64),
                default_list_line: 1,
            }],
            evidence: vec![],
            excluded_installed_packages: vec![],
            warnings: vec![],
        };
        let effects = Library {
            schema_version: 1,
            lights: vec![],
            particles: vec![],
            emitters: vec![],
            textures: Default::default(),
        };
        (catalog, effects, materials, crate::testing::definitions())
    }

    /// The native inputs: the stock catalog, effects and print materials.
    fn native_inputs() -> Result<(Catalog, Library, Bundle, Definitions)> {
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
        let definitions = Definitions::load(&catalog_dir, &root.join("content/maps-pass-008"))?;
        Ok((catalog, effects, materials, definitions))
    }

    /// A brick's click swap is kept only when both targets are bricks of
    /// its own catalog: an Add-On cannot turn a brick into another's.
    #[test]
    fn a_click_swap_stays_within_its_own_catalog() -> Result<()> {
        use bri_content::brick::Swap;
        let (mut catalog, effects, materials, _) = synthetic_inputs();
        let mut door = serde_json::to_value(&catalog.bricks[0])?;
        let swap = |front: &str, back: &str| {
            serde_json::to_value(Swap {
                front: front.into(),
                back: back.into(),
            })
        };
        door["id"] = "doors:brick/door".into();
        door["swap"] = swap("doors:brick/open-a", "doors:brick/open-b")?;
        catalog.bricks.push(serde_json::from_value(door.clone())?);
        door["id"] = "doors:brick/thief".into();
        door["swap"] = swap("doors:brick/open-a", "other:brick/vault")?;
        catalog.bricks.push(serde_json::from_value(door)?);
        let tools = ToolCatalog::from_native(&catalog, &effects, &materials)?;
        assert_eq!(tools.swaps.keys().collect::<Vec<_>>(), ["doors:brick/door"]);
        Ok(())
    }

    /// The catalog lists every print, printable brick, named light and
    /// named emitter; it needs the `Letters/A` default, and a printable
    /// brick planted from it carries that default.
    fn installs_and_plants_the_default_print(
        (catalog, effects, materials, definitions): (Catalog, Library, Bundle, Definitions),
    ) -> Result<String> {
        let tools = ToolCatalog::from_native(&catalog, &effects, &materials)?;
        let printable: Vec<_> = catalog
            .bricks
            .iter()
            .filter(|b| b.print_aspect_ratio.as_ref().is_some_and(|a| !a.is_empty()))
            .map(|b| b.id.clone())
            .collect();
        assert_eq!(tools.prints.len(), materials.prints.len());
        assert_eq!(tools.brick_print_aspects.len(), printable.len());
        let named = |names: Vec<&String>| names.into_iter().filter(|n| !n.is_empty()).count();
        assert_eq!(
            tools.lights.len(),
            named(effects.lights.iter().map(|l| &l.name).collect())
        );
        assert_eq!(
            tools.emitters.len(),
            named(effects.emitters.iter().map(|e| &e.name).collect())
        );
        let default = materials.resolve("Letters/A").unwrap().id.clone();
        assert_eq!(tools.default_print.as_deref(), Some(default.as_str()));
        let mut missing = materials.clone();
        missing.prints.retain(|p| p.id != default);
        assert!(ToolCatalog::from_native(&catalog, &effects, &missing).is_err());
        // A 2x2 printable brick, the stock one on the native content.
        let brick = printable
            .iter()
            .find(|id| id.as_str() == "v20/brick/brick2x2fprintdata")
            .or(printable.first())
            .unwrap()
            .clone();
        let mesh = &definitions.entries[&brick].mesh;
        let position = [
            mesh.footprint_studs[0] as f32 * 0.25,
            mesh.height_plates as f32 * 0.1,
            -3.0 - mesh.footprint_studs[1] as f32 * 0.25,
        ];
        let simulation = Simulation::new(
            World::new("Dedicated test".into(), "fixture".into(), vec![[1.; 4]]),
            definitions,
            vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
        )?;
        let mut session = Session::new(simulation);
        session.set_tool_catalog(tools)?;
        let owner = session.join("Builder".into(), Vec3::new(0., 0.05, 0.), false)?;
        let Reply::Planted(id) = session.command(
            owner,
            1,
            Command::Plant {
                definition: brick,
                position,
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
        Ok(default)
    }

    #[test]
    fn synthetic_catalog_installs_and_plants_the_default_print() -> Result<()> {
        installs_and_plants_the_default_print(synthetic_inputs())?;
        Ok(())
    }

    /// The same on the stock content, whose counts it pins and records for
    /// the headless asset report.
    #[test]
    #[ignore = "requires generated v20 content"]
    fn native_dedicated_catalog_installs_and_plants_original_default_print() -> Result<()> {
        let (catalog, effects, materials, _) = native_inputs()?;
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
        let default = installs_and_plants_the_default_print(native_inputs()?)?;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
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
