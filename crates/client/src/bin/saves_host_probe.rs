//! Headless check of a whole saves folder, as the game hosts each save: the
//! game's own background converter turns every `.bls` into a native save,
//! Load Bricks reads it, a fresh host session on its map loads it to the
//! end, and the joined player's client builds its brick chunks, query
//! mirror and prediction mirror from the replicated world. No window, no
//! input; the saves folder is only read (conversions go to a temporary
//! folder).
//!
//! Usage: saves_host_probe <content-root> <saves-dir> <report.json>
//! Prints one line per failing save and a summary; the report lists every
//! save with its outcome and the prints this client does not have.
use anyhow::{Context, Result, ensure};
use bri_client::{
    content::{ClientContent, LOADABLE_MAPS},
    old_saves::{Converter, OldSaves},
    saves::Store,
};
use bri_sim::session::{Command, Reply, Session};
use bri_world::{ContentRef, World};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

struct Setup {
    content: ClientContent,
    weapons: bri_net::content_identity::WeaponContent,
    item_bounds: BTreeMap<String, bri_weapons::ItemBounds>,
    vehicles: bri_vehicles::Pack,
    meshes: BTreeMap<String, bri_content::brick::Brick>,
    materials: bri_client::materials::BrickMaterials,
    palette: bri_client::world_chunks::BrickPalette,
}
impl Setup {
    fn session(&self, map: &str) -> Result<(Session, bri_client::content::LoadedMap)> {
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
        session.set_vehicle_pack(self.vehicles.clone())?;
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
    fn host(&self, entry: &bri_client::saves::Entry) -> Result<usize> {
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
        let mut building = bri_client::building::Building::new(
            definitions.clone(),
            loaded.query_colliders.clone(),
        )?;
        building.sync_world(&world)?;
        let mut mirror = bri_sim::prediction::CollisionMirror::new(
            definitions,
            loaded.query_colliders.clone(),
            waters,
        );
        mirror.sync(&world.bricks)?;
        bri_client::world_chunks::ChunkedWorld::default()
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
}

/// Print names in `build` this client has no image for.
fn unknown_prints(
    build: &bri_world::build::SavedBuild,
    materials: &bri_client::materials::BrickMaterials,
) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for brick in build.world.bricks.values().chain(&build.world.unloaded) {
        let Some(print) = &brick.print else { continue };
        let (ContentRef::Resolved(name) | ContentRef::Unresolved { name, .. }) = print;
        if materials.bundle.resolve(name).is_none() {
            *out.entry(name.clone()).or_default() += 1;
        }
    }
    out
}

fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 3,
        "Usage: saves_host_probe <content-root> <saves-dir> <report.json>"
    );
    let state = std::env::temp_dir().join(format!("saves_host_probe-{}", std::process::id()));
    std::fs::create_dir_all(&state)?;
    let result = run(&args, &state);
    let _ = std::fs::remove_dir_all(&state);
    result
}

fn run(args: &[PathBuf], state: &std::path::Path) -> Result<()> {
    let content = ClientContent::load(&args[0])?;
    let weapons = content.paths.weapon_content()?;
    let item_bounds = content.paths.item_physics(&weapons)?.bounds;
    let vehicles = content.paths.vehicle_pack()?;
    let materials = bri_client::materials::BrickMaterials::load(&content.paths.brick_materials)?;
    let palette = bri_client::world_chunks::BrickPalette::new(&materials)?;
    let old = OldSaves::new(args[1].clone(), state.join("converted-saves"), vec![]);
    old.set_converter(Converter::new(&content)?);
    old.start();
    while old.busy() {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let store = Store::new(state, &content, Some(old.clone()));
    let mut setup = Setup {
        content,
        weapons,
        item_bounds,
        vehicles,
        meshes: BTreeMap::new(),
        materials,
        palette,
    };
    let (probe, _) = setup.session(LOADABLE_MAPS[3])?;
    setup.meshes = probe
        .simulation()
        .definitions
        .entries
        .iter()
        .map(|(id, d)| (id.clone(), d.mesh.clone()))
        .collect();
    drop(probe);

    let entries: Vec<_> = store
        .list()?
        .into_iter()
        .filter(|e| e.path.starts_with(old.cache()))
        .collect();
    let mut saves = vec![];
    let (mut failed, mut with_unknown_prints) = (0, 0);
    for entry in &entries {
        let label = format!("{}/{}", entry.info.map, entry.info.name);
        let unknown = Store::read(entry)
            .map(|b| unknown_prints(&b, &setup.materials))
            .unwrap_or_default();
        if !unknown.is_empty() {
            with_unknown_prints += 1;
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| setup.host(entry)))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("panicked")));
        let result = match &outcome {
            Ok(placed) => json!({ "placed": placed }),
            Err(error) => {
                failed += 1;
                println!("FAILED {label}: {error:#}");
                json!({ "error": format!("{error:#}") })
            }
        };
        saves.push(json!({
            "save": label,
            "listed_bricks": entry.info.brick_count,
            "unknown_prints": unknown,
            "result": result,
        }));
    }
    let summary = json!({
        "saves": entries.len(),
        "failed": failed,
        "with_unknown_prints": with_unknown_prints,
    });
    println!("{summary}");
    let report: Value = json!({ "summary": summary, "saves": saves });
    std::fs::write(&args[2], serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
