//! Loading a base map for a hosted session: the one path a game the client
//! hosts, a dedicated server and Change Map on either take.
use crate::{
    content_identity::WeaponContent,
    host_setup::{LoadMap, MapSession},
};
use anyhow::{Context, Result, ensure};
use bri_content::{
    scene::{Kind, Scene},
    terrain_field::TerrainField,
};
use bri_package::packages::PackageSet;
use bri_sim::{
    definitions::Definitions,
    map::{Breakable, NativeMap},
    map_catalog::{MapCatalog, short},
    session::MapListing,
    simulation::Simulation,
    tutorial::TutorialMap,
};
use glam::Vec3;
use rapier3d::prelude::ColliderBuilder;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// The packs a host loads maps from.
#[derive(Clone)]
pub struct MapContent {
    /// Every map this content can load, base and Add-On
    /// ([`bri_sim::map_catalog`]).
    pub maps: MapCatalog,
    pub brick_catalog: PathBuf,
    pub geometry: PathBuf,
    /// The Tutorial's lessons; a root without them cannot host the Tutorial.
    pub tutorial: Option<PathBuf>,
    /// Other packages' bricks (`content_identity::brick_catalog_providers`).
    pub brick_extras: Vec<(String, PathBuf)>,
    pub brick_geometry: std::collections::BTreeMap<String, bri_content::brick::Brick>,
    pub weapons: WeaponContent,
    /// The brick definitions, read and built on first use and shared by
    /// every clone: they depend on the packs, not the map, so a Host, a
    /// Join and each Change Map reuse them rather than reading every brick
    /// mesh and building every collider again.
    pub definitions: Arc<std::sync::Mutex<Option<Definitions>>>,
    /// The waters the Environment window lays over a flat map's plate.
    pub environments: Arc<bri_content::environment_pack::Pack>,
}

/// A map loaded for a session, with what a client hosting it also needs.
pub struct LoadedMap {
    pub simulation: Simulation,
    pub scene: Scene,
    /// The first spawn point.
    pub spawn: [f32; 3],
    pub spawn_points: Vec<Vec3>,
    /// Map objects not loaded yet and brick item references left unresolved.
    pub pending_objects: Vec<String>,
    pub unresolved_items: usize,
    /// Shared-shape copies for the client-side query mirror; server authority
    /// remains in simulation. No original assets are read by either consumer.
    pub query_colliders: Vec<ColliderBuilder>,
    /// Exact terrain placements for client collision streaming and queries.
    pub terrain: Vec<Arc<TerrainField>>,
    /// The Tutorial map's lesson zones and brick layouts.
    pub tutorial: Option<TutorialMap>,
    /// Glass shapes a fast player smashes.
    pub breakables: Vec<Breakable>,
    /// Teleporter entrances and exits.
    pub teleporters: Vec<bri_content::scene::Teleporter>,
    /// Named boxes rules look up by point.
    pub volumes: Vec<bri_content::scene::Volume>,
    /// The map's keyed movers (doors).
    pub movers: Vec<bri_sim::map::MapMover>,
    /// The plate a flat map stands on.
    pub plate: Option<bri_content::scene::Plate>,
}

impl LoadedMap {
    /// What a host's session takes from the map.
    pub fn into_session(self) -> MapSession {
        MapSession {
            simulation: self.simulation,
            spawn_points: self.spawn_points,
            breakables: self.breakables,
            teleporters: self.teleporters,
            volumes: self.volumes,
            movers: self.movers,
            tutorial: self.tutorial,
        }
    }
}

impl MapContent {
    /// The packs `packages` names under `content_root`.
    pub fn from_root(
        content_root: &Path,
        packages: &PackageSet,
        weapons: WeaponContent,
    ) -> Result<Self> {
        let role = |role: &str| packages.role_dir(content_root, role);
        Ok(Self {
            maps: catalog(content_root, packages)?,
            brick_catalog: role("brick_catalog")?,
            geometry: role("geometry")?,
            tutorial: match role("tutorial") {
                Ok(dir) => Some(dir),
                Err(error) => {
                    bri_console::fallback::report(
                        bri_console::fallback::kind::DROPPED_ITEM,
                        "",
                        "tutorial role",
                        &format!("no tutorial pack ({error:#})"),
                    );
                    None
                }
            },
            brick_extras: crate::content_identity::brick_catalog_providers(content_root, packages)?,
            brick_geometry: crate::content_identity::brick_geometry_assets(content_root, packages)?,
            weapons,
            definitions: Default::default(),
            environments: Arc::new(environments(content_root, packages)?),
        })
    }

    /// The maps Change Map offers: every loadable base map but the Tutorial,
    /// in the bundle's order.
    pub fn maps(&self) -> Result<Vec<MapListing>> {
        Ok(change_map_list(&self.maps))
    }

    /// Every brick's definition, Add-On bricks included (see
    /// [`Self::definitions`]'s field).
    pub fn definitions(&self) -> Result<Definitions> {
        let mut kept = self
            .definitions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(definitions) = &*kept {
            return Ok(definitions.clone());
        }
        let definitions = Definitions::load_with_geometry(
            &self.brick_catalog,
            &self.geometry,
            &self.brick_extras,
            &self.brick_geometry,
        )
        .context("Loading native brick definitions")?;
        *kept = Some(definitions.clone());
        Ok(definitions)
    }
    /// Loads `world` on its map. Start Game and Change Map offer only
    /// [`MapCatalog`] maps; a saved world names its own.
    pub fn load(&self, mut world: bri_world::World) -> Result<LoadedMap> {
        let map_id = world.map_id.clone();
        let mut unresolved_items = self.weapons.resolve_world_items(&mut world)?;
        let definitions = self.definitions()?;
        let native = NativeMap::load(self.maps.root(&map_id)?, &map_id)
            .context("Loading native map collision")?;
        ensure!(
            native
                .scene
                .nodes
                .iter()
                .any(|n| matches!(n.kind, Kind::Spawn)),
            "Native map has no authored spawn"
        );
        let query_colliders = native.colliders.clone();
        let terrain = native.terrain.clone();
        let anchors = native.spawn_anchors()?;
        let mut simulation = Simulation::new(world, definitions, native.colliders)
            .context("Building native simulation")?;
        simulation.attach_terrain(native.terrain, anchors)?;
        simulation.set_map_surfaces(native.surfaces);
        simulation.set_map_regions(native.regions)?;
        simulation.set_map_waters(native.waters, native.plate, self.environments.clone());
        let spawn_points =
            bri_sim::spawn::candidates(&simulation.physics, &native.scene, &Default::default())?;
        let spawn = spawn_points
            .first()
            .context("Native map has no spawn point")?
            .to_array();
        let tutorial = if map_id == bri_sim::tutorial::MAP_ID {
            let pack = self.tutorial.as_ref().context("No tutorial pack")?;
            let (index, mut part1, mut part2) =
                bri_sim::tutorial::load_pack(pack).context("Loading the tutorial pack")?;
            unresolved_items += self.weapons.resolve_world_items(&mut part1)?;
            unresolved_items += self.weapons.resolve_world_items(&mut part2)?;
            let collision = bri_sim::tutorial::load_target_collision(pack, &index)
                .context("Loading the tutorial targets")?;
            Some(TutorialMap::new(
                &native.scene,
                index,
                part1,
                part2,
                collision,
            )?)
        } else {
            None
        };
        let mut pending_objects = native.pending_objects;
        pending_objects.extend(bri_sim::simulation::unloaded_summary(
            &simulation.state().unloaded,
        ));
        if unresolved_items > 0 {
            pending_objects.push(format!(
                "{unresolved_items} unresolved brick item references retained"
            ));
        }
        Ok(LoadedMap {
            simulation,
            scene: native.scene,
            spawn,
            spawn_points,
            pending_objects,
            unresolved_items,
            query_colliders,
            terrain,
            tutorial,
            breakables: native.breakables,
            teleporters: native.teleporters,
            volumes: native.volumes,
            movers: native.movers,
            plate: native.plate,
        })
    }

    /// Change Map's loader: a new empty world on the chosen map, painted
    /// with `palette` (the server's colorset).
    pub fn loader(self, palette: Vec<[f32; 4]>) -> LoadMap {
        Arc::new(move |map: &str| {
            let world = blank_world(&self.maps, palette.clone(), map)?;
            Ok(self.load(world)?.into_session())
        })
    }
}

/// The Environment window's skies, grounds and waters: an empty pack for a
/// package list without that role.
pub fn environments(
    content_root: &Path,
    packages: &PackageSet,
) -> Result<bri_content::environment_pack::Pack> {
    match packages.role_dir(content_root, "environments") {
        Ok(dir) => {
            bri_content::environment_pack::Pack::load(&dir).context("Loading the environment pack")
        }
        Err(error) => {
            bri_console::fallback::report(
                bri_console::fallback::kind::DROPPED_ITEM,
                "",
                "environments role",
                &format!("an empty environment pack ({error:#})"),
            );
            Ok(bri_content::environment_pack::Pack {
                schema_version: bri_content::environment_pack::SCHEMA,
                ..Default::default()
            })
        }
    }
}

/// Every map `packages` makes loadable under `content_root`: the base
/// bundle's and each enabled Add-On's.
pub fn catalog(content_root: &Path, packages: &PackageSet) -> Result<MapCatalog> {
    MapCatalog::new(
        &packages.role_dir(content_root, "map_bundle")?,
        &crate::content_identity::map_providers(content_root, packages)?,
    )
}

fn change_map_list(maps: &MapCatalog) -> Vec<MapListing> {
    maps.listed()
        .filter(|m| m.id != bri_sim::tutorial::MAP_ID)
        .map(|m| MapListing {
            id: m.id.clone(),
            name: m.name.clone(),
        })
        .collect()
}

/// A new empty world on the map `name`: its full map id or the map's
/// short name (`slate`, `bedroom`, `slatedesert`, an Add-On map's own
/// name), with the game's default paint palette, like choosing a map in
/// Start Game.
pub fn blank_world(
    maps: &MapCatalog,
    palette: Vec<[f32; 4]>,
    name: &str,
) -> Result<bri_world::World> {
    let wanted = name.to_ascii_lowercase().replace(['_', ' '], "");
    let map = maps
        .listed()
        .find(|m| m.id.eq_ignore_ascii_case(name) || short(&m.id) == wanted)
        .with_context(|| {
            let mut names: Vec<_> = maps.listed().map(|m| short(&m.id)).collect();
            names.sort();
            format!("Unknown map `{name}`. Maps: {}", names.join(", "))
        })?;
    let world = bri_world::World::new(map.name.clone(), map.id.clone(), palette);
    world.validate()?;
    Ok(world)
}

/// The UI pack's default paint palette, every division's colors in order.
pub fn ui_palette(ui_pack: &Path) -> Result<Vec<[f32; 4]>> {
    let ui: serde_json::Value =
        serde_json::from_slice(&std::fs::read(ui_pack.join("ui-pack.json"))?)?;
    Ok(ui["data"]["brick_colorset"]
        .as_array()
        .context("Missing default paint palette")?
        .iter()
        .map(|division| serde_json::from_value::<Vec<[f32; 4]>>(division["colors"].clone()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<(tempfile::TempDir, MapCatalog, PathBuf)> {
        let dir = tempfile::tempdir()?;
        let (maps, ui) = (dir.path().join("maps"), dir.path().join("ui"));
        std::fs::create_dir_all(&maps)?;
        std::fs::create_dir_all(&ui)?;
        std::fs::write(
            maps.join("bundle.json"),
            r#"{"maps":[{"id":"v20/add-ons/map_slate/slate.mis","name":"Slate"},{"id":"v20/add-ons/map_slate_desert/slatedesert.mis","name":"Slate Desert"},{"id":"v20/add-ons/map_tutorial/tutorial.mis","name":"Tutorial"},{"id":"v20/add-ons/map_moon/moon.mis","name":"Moon"}]}"#,
        )?;
        std::fs::write(
            ui.join("ui-pack.json"),
            r#"{"data":{"brick_colorset":[{"name":"A","colors":[[1,0,0,1],[0,1,0,1]]},{"name":"B","colors":[[0,0,1,1]]}]}}"#,
        )?;
        let catalog = MapCatalog::new(&maps, &[])?;
        Ok((dir, catalog, ui))
    }

    #[test]
    fn a_map_name_starts_an_empty_world_with_the_default_palette() -> Result<()> {
        let (_dir, maps, ui) = fixture()?;
        let world = blank_world(&maps, ui_palette(&ui)?, "Slate")?;
        assert_eq!(world.map_id, "v20/add-ons/map_slate/slate.mis");
        assert_eq!(world.name, "Slate");
        assert!(world.bricks.is_empty());
        assert_eq!(world.palette.len(), 3);
        let desert = blank_world(&maps, ui_palette(&ui)?, "slate_desert")?;
        assert_eq!(
            desert.map_id,
            "v20/add-ons/map_slate_desert/slatedesert.mis"
        );
        // A map in the bundle that does not load natively is not offered.
        let unknown = blank_world(&maps, vec![[1.0; 4]], "moon")
            .unwrap_err()
            .to_string();
        assert!(
            unknown.contains("slate, slatedesert, tutorial"),
            "{unknown}"
        );
        Ok(())
    }

    #[test]
    fn change_map_offers_the_loadable_maps_but_the_tutorial() -> Result<()> {
        let (_dir, maps, _ui) = fixture()?;
        let listed: Vec<_> = change_map_list(&maps).into_iter().map(|m| m.name).collect();
        assert_eq!(listed, ["Slate", "Slate Desert"]);
        Ok(())
    }
}
