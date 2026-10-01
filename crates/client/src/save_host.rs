//! Host a saved build headlessly, as the game does: Load Bricks reads it, a
//! fresh host session on its map loads it to the end, and the joined
//! player's client builds its brick chunks, query mirror and prediction
//! mirror from the replicated world. No window, no input. Used by the
//! `saves_host_probe` tool and the fixed save corpus test.
use crate::{
    content::{ClientContent, LOADABLE_MAPS},
    saves::{Entry, Store},
};
use anyhow::{Context, Result, ensure};
use bri_sim::session::{Command, Reply, Session};
use bri_world::{ContentRef, World};
use std::{collections::BTreeMap, sync::Arc};

pub struct SaveHost {
    pub content: ClientContent,
    weapons: bri_net::content_identity::WeaponContent,
    item_bounds: BTreeMap<String, bri_weapons::ItemBounds>,
    vehicles: bri_vehicles::Pack,
    meshes: BTreeMap<String, bri_content::brick::Brick>,
    pub materials: crate::materials::BrickMaterials,
    palette: crate::world_chunks::BrickPalette,
}
impl SaveHost {
    pub fn new(content: ClientContent) -> Result<Self> {
        let weapons = content.paths.weapon_content()?;
        let item_bounds = content.paths.item_physics(&weapons)?.bounds;
        let vehicles = content.paths.vehicle_pack()?;
        let materials = crate::materials::BrickMaterials::load(&content.paths.brick_materials)?;
        let palette = crate::world_chunks::BrickPalette::new(&materials)?;
        let mut host = Self {
            content,
            weapons,
            item_bounds,
            vehicles,
            meshes: BTreeMap::new(),
            materials,
            palette,
        };
        let (probe, _) = host.session(LOADABLE_MAPS[3])?;
        host.meshes = probe
            .simulation()
            .definitions
            .entries
            .iter()
            .map(|(id, d)| (id.clone(), d.mesh.clone()))
            .collect();
        Ok(host)
    }

    fn session(&self, map: &str) -> Result<(Session, crate::content::LoadedMap)> {
        let mut loaded = self.content.load_map(map, None)?;
        let simulation = std::mem::replace(
            &mut loaded.simulation,
            bri_sim::simulation::Simulation::new(
                World::new("placeholder".into(), map.into(), vec![[1.0; 4]]),
                bri_sim::definitions::Definitions {
                    entries: BTreeMap::new(),
                },
                vec![],
            )?,
        );
        let mut session = Session::new(simulation);
        session.set_weapon_pack(self.weapons.pack.clone())?;
        session.set_item_bounds(self.item_bounds.clone())?;
        session.set_vehicle_pack(self.vehicles.clone(), self.content.paths.bot_kinds()?)?;
        session.set_event_catalog(
            self.content.events.clone(),
            self.content
                .event_sounds
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>(),
        )?;
        session.set_spawn_points(loaded.spawn_points.clone())?;
        Ok((session, loaded))
    }

    /// Host `entry` on its map (Slate for a loose save) and build what a
    /// joined client builds. The number of bricks placed.
    pub fn host(&self, entry: &Entry) -> Result<usize> {
        let build = Store::read(entry)?;
        let map = if LOADABLE_MAPS.contains(&entry.map_id.as_str()) {
            entry.map_id.as_str()
        } else {
            LOADABLE_MAPS[3]
        };
        let (mut session, loaded) = self.session(map)?;
        let host = loaded
            .spawn_points
            .iter()
            .find_map(|p| session.join("Host".into(), *p, true).ok())
            .context("No spawn for the host")?;
        let reply = session.command(
            host,
            1,
            Command::LoadBuild {
                build: Box::new(build),
                ownership: false,
            },
        )?;
        ensure!(
            matches!(reply, Reply::Loaded { .. }),
            "Unexpected load reply {reply:?}"
        );
        while session.build_loading() {
            session.step()?;
        }
        let state = session.simulation().state();
        let world = Arc::new(bri_net::protocol::PublicWorld {
            name: state.name.clone(),
            map_id: state.map_id.clone(),
            palette: state.palette.clone(),
            bricks: bri_net::protocol::public_bricks(&state.bricks),
        });
        let placed = world.bricks.len();
        let definitions = session.simulation().definitions.clone();
        let waters = session.simulation().waters.clone();
        drop(session);
        let mut building =
            crate::building::Building::new(definitions.clone(), loaded.query_colliders.clone())?;
        building.sync_world(&world)?;
        let mut mirror = bri_sim::prediction::CollisionMirror::new(
            definitions,
            loaded.query_colliders.clone(),
            waters,
        );
        mirror.sync(&world.bricks)?;
        crate::world_chunks::ChunkedWorld::default()
            .update(
                world,
                None,
                &self.meshes,
                &self.palette,
                Some(&self.materials),
                usize::MAX / 4,
            )
            .context("Building brick chunks")?;
        Ok(placed)
    }

    /// Print names in `build` this client has no image for, with brick counts.
    pub fn unknown_prints(&self, build: &bri_world::build::SavedBuild) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for brick in build.world.bricks.values().chain(&build.world.unloaded) {
            let Some(print) = &brick.print else { continue };
            let (ContentRef::Resolved(name) | ContentRef::Unresolved { name, .. }) = print;
            if self.materials.bundle.resolve(name).is_none() {
                *out.entry(name.clone()).or_default() += 1;
            }
        }
        out
    }
}
