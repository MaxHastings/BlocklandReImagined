//! Headless native-content host; no renderer or window dependencies.
use anyhow::{Context, Result, ensure};
use bri_net::{
    content_identity,
    server::{self, ServerOptions},
};
use bri_package::{environment::Environment, packages::PackageSet};
use bri_sim::{
    definitions::Definitions,
    map::NativeMap,
    session::{Session, ToolCatalog},
    simulation::Simulation,
};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
/// Music-brick loop ids declared by the audio pack.
fn audio_music(audio: &std::path::Path) -> Result<Vec<String>> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(audio.join("manifest.json"))?)?;
    Ok(manifest["triggers"]
        .as_array()
        .context("Missing audio triggers")?
        .iter()
        .filter(|t| {
            t["key"]
                .as_str()
                .is_some_and(|k| k.starts_with("music-brick:"))
        })
        .filter_map(|t| t["sound"].as_str().map(str::to_string))
        .collect())
}
fn audio_event_sounds(audio: &std::path::Path) -> Result<Vec<String>> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(audio.join("manifest.json"))?)?;
    Ok(manifest["sounds"]
        .as_array()
        .context("Missing audio sounds")?
        .iter()
        .filter(|s| {
            s["lists"]
                .as_array()
                .is_some_and(|l| l.iter().any(|v| v == "event-param:Sound"))
        })
        .filter_map(|s| s["id"].as_str().map(str::to_string))
        .collect())
}
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 4 || args.len() == 5,
        "Usage: bri-server <content-root> <world.json> <state-dir> <listen-address> [run-seconds]
         The content root's packages.json lists the packages to load (the base game's list when absent)."
    );
    let content_root = PathBuf::from(&args[0]);
    let world_path = PathBuf::from(&args[1]);
    let state_dir = PathBuf::from(&args[2]);
    // Session log and crash reports beside the server binary (or in its state).
    if let Err(error) = bri_crash::install("bri-server", &bri_crash::default_directories(&state_dir)) {
        eprintln!("Crash capture unavailable: {error}");
    }
    let bind = args[3]
        .to_str()
        .context("Invalid listen address")?
        .parse()?;
    let seconds = if args.len() == 5 {
        let seconds: u64 = args[4].to_str().context("Invalid duration")?.parse()?;
        ensure!((1..=86400).contains(&seconds), "Duration out of range");
        Some(seconds)
    } else {
        None
    };
    let packages = PackageSet::load_root(&content_root)?;
    let role = |role: &str| packages.role_dir(&content_root, role);
    let catalog_dir = role("brick_catalog")?;
    let geometry_dir = role("geometry")?;
    let map_bundle_dir = role("map_bundle")?;
    let materials_dir = role("brick_materials")?;
    let effects_dir = role("effects")?;
    let avatar_dir = role("avatar")?;
    let audio_dir = role("audio")?;
    let weapons_dir = role("weapons")?;
    let item_presentation_dir = role("item_presentation")?;
    let vehicles_dir = role("vehicles")?;
    let events_dir = role("events")?;
    // Hash before loading simulation so missing/corrupt packages fail before a
    // socket is opened or server state is written.
    let environment = Environment::load(&content_root, &packages)?;
    let content_id = environment.digest();
    let weapons = content_identity::WeaponContent::load(&weapons_dir)?;
    let item_physics = content_identity::ItemPhysicsContent::load(&item_presentation_dir, &weapons)?;
    let vehicle_pack = bri_vehicles::Pack::load(vehicles_dir.join("vehicles.json"))?;
    let catalog = serde_json::from_slice(&std::fs::read(catalog_dir.join("stock-catalog.json"))?)?;
    let materials = serde_json::from_slice(&std::fs::read(materials_dir.join("brick-materials.json"))?)?;
    let effects = serde_json::from_slice(&std::fs::read(effects_dir.join("effects.json"))?)?;
    let mut tools = ToolCatalog::from_native(&catalog, &effects, &materials)?;
    tools.install_items(weapons.item_choices.iter().map(|(id, _)| id.clone()))?;
    let tool_summary = serde_json::json!({"items":tools.items.len(),"prints":tools.prints.len(),"printable_definitions":tools.brick_print_aspects.len(),"lights":tools.lights.len(),"emitters":tools.emitters.len(),"default_print":tools.default_print});
    let mut world = bri_world::persistence::load_startup(&world_path)?;
    let unresolved_items = weapons.resolve_world_items(&mut world)?;
    let map = NativeMap::load(&map_bundle_dir, &world.map_id)?;
    let anchors = map.spawn_anchors()?;
    let mut simulation = Simulation::new(
        world,
        Definitions::load(&catalog_dir, &geometry_dir)?,
        map.colliders,
    )?;
    simulation.attach_terrain(map.terrain, anchors)?;
    simulation.waters = map.waters;
    let spawn_points =
        bri_sim::spawn::candidates(&simulation.physics, &map.scene, &Default::default())?;
    let mut session = Session::new(simulation);
    session.set_breakables(map.breakables)?;
    session.set_vehicle_pack(vehicle_pack)?;
    session.set_spawn_points(spawn_points.clone())?;
    tools.install_special(
        audio_music(&audio_dir)?,
        session.vehicle_choices().into_iter().map(|(id, _)| id),
    )?;
    session.set_tool_catalog(tools)?;
    session.set_weapon_pack(weapons.pack)?;
    session.set_event_catalog(
        bri_events::Catalog::load(events_dir.join("catalog.json"))?,
        audio_event_sounds(&audio_dir)?,
    )?;
    session.set_item_bounds(item_physics.bounds)?;
    session.set_avatar_catalog(serde_json::from_slice(&std::fs::read(
        avatar_dir.join("avatar.json"),
    )?)?)?;
    let initial_static_items = session.weapon_view().static_items.len();
    let server = server::start_with_admin_store_and_limit(
        session,
        ServerOptions {
            bind,
            environment: environment.clone(),
            spawn_points,
            certificate: Some(server::HostCertificate::load_or_create(&state_dir)?),
            map_loader: None,
        },
        64,
        state_dir.join("administration.json"),
    )?;
    std::fs::create_dir_all(&state_dir)?;
    std::fs::write(state_dir.join("server-cert.der"), &server.certificate)?;
    std::fs::write(
        state_dir.join("host.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"schema_version":1,"address":server.address.to_string(),"content_id":content_id,"environment":environment,"tool_catalog":tool_summary,"initial_static_items":initial_static_items,"unresolved_item_references":unresolved_items,"certificate":"server-cert.der","pending_map_objects":map.pending_objects}),
        )?,
    )?;
    println!(
        "Headless host listening on {}. Public connection metadata: {}",
        server.address,
        state_dir.join("host.json").display()
    );
    if let Some(seconds) = seconds {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
    } else {
        tokio::signal::ctrl_c().await?;
    }
    let report = server.stop().await?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let save = state_dir.join(format!("world-{timestamp}.json"));
    bri_world::persistence::save_new(&save, &report.native_world)?;
    std::fs::write(
        state_dir.join("last-run.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"ticks":report.ticks,"dropped_ticks":report.dropped_ticks,"dropped_cues":report.dropped_cues,"weapon_adapter_gaps":report.weapon_adapter_gaps,"joins":report.joins,"resumes":report.resumes,"commands":report.commands,"rejected":report.rejected,"notices":report.notices,"saved_world":save.file_name().unwrap().to_string_lossy()}),
        )?,
    )?;
    println!(
        "Saved native world at tick {} to {}",
        report.ticks,
        save.display()
    );
    Ok(())
}
