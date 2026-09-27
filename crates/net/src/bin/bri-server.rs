//! Headless native-content host; no renderer or window dependencies.
use anyhow::{Context, Result, ensure};
use bri_net::{
    content_identity,
    server::{self, ServerOptions},
};
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
        args.len() == 17 || args.len() == 18,
        "Usage: bri-server <catalog-dir> <native-content> <world.json> <map-bundle> <brick-materials-dir> <effects-dir> <avatar-dir> <effects-runtime-dir> <audio-dir> <weather-dir> <foliage-dir> <weapons-dir> <item-presentation-dir> <vehicles-dir> <events-dir> <state-dir> <listen-address> [run-seconds]"
    );
    let mut paths: Vec<_> = args[..16].iter().map(PathBuf::from).collect();
    let events_dir = paths.remove(14);
    let vehicles_dir = paths.remove(13);
    // Session log and crash reports beside the server binary (or in its state).
    if let Err(error) = bri_crash::install("bri-server", &bri_crash::default_directories(&paths[13])) {
        eprintln!("Crash capture unavailable: {error}");
    }
    let bind = args[16]
        .to_str()
        .context("Invalid listen address")?
        .parse()?;
    let seconds = if args.len() == 18 {
        let seconds: u64 = args[17].to_str().context("Invalid duration")?.parse()?;
        ensure!((1..=86400).contains(&seconds), "Duration out of range");
        Some(seconds)
    } else {
        None
    };
    // Hash before loading simulation so missing/corrupt resources fail before a
    // socket is opened or server state is written. These are native bundles only.
    let content_id = content_identity::fingerprint_runtime(
        &paths[0], &paths[1], &paths[3], &paths[4], &paths[5], &paths[6],
    )?;
    let content_id = content_identity::with_effects_runtime(&content_id, &paths[7])?;
    let content_id = content_identity::with_audio(&content_id, &paths[8])?;
    let content_id = content_identity::with_weather(&content_id, &paths[9])?;
    let content_id = content_identity::with_foliage(&content_id, &paths[10])?;
    let weapons = content_identity::WeaponContent::load(&paths[11])?;
    let content_id = weapons.extend_identity(&content_id);
    let item_physics = content_identity::ItemPhysicsContent::load(&paths[12], &weapons)?;
    let content_id = item_physics.extend_identity(&content_id);
    let content_id = content_identity::with_vehicles(&content_id, &vehicles_dir)?;
    let content_id = content_identity::with_events(&content_id, &events_dir)?;
    let vehicle_pack = bri_vehicles::Pack::load(vehicles_dir.join("vehicles.json"))?;
    let catalog = serde_json::from_slice(&std::fs::read(paths[0].join("stock-catalog.json"))?)?;
    let materials = serde_json::from_slice(&std::fs::read(paths[4].join("brick-materials.json"))?)?;
    let effects = serde_json::from_slice(&std::fs::read(paths[5].join("effects.json"))?)?;
    let mut tools = ToolCatalog::from_native(&catalog, &effects, &materials)?;
    tools.install_items(weapons.item_choices.iter().map(|(id, _)| id.clone()))?;
    let tool_summary = serde_json::json!({"items":tools.items.len(),"prints":tools.prints.len(),"printable_definitions":tools.brick_print_aspects.len(),"lights":tools.lights.len(),"emitters":tools.emitters.len(),"default_print":tools.default_print});
    let mut world = bri_world::persistence::load_startup(&paths[2])?;
    let unresolved_items = weapons.resolve_world_items(&mut world)?;
    let map = NativeMap::load(&paths[3], &world.map_id)?;
    let anchors = map.spawn_anchors()?;
    let mut simulation = Simulation::new(
        world,
        Definitions::load(&paths[0], &paths[1])?,
        map.colliders,
    )?;
    simulation.attach_terrain(map.terrain, anchors)?;
    simulation.waters = map.waters;
    let spawn_points =
        bri_sim::spawn::candidates(&simulation.physics, &map.scene, &Default::default())?;
    let mut session = Session::new(simulation);
    session.set_vehicle_pack(vehicle_pack)?;
    session.set_spawn_points(spawn_points.clone())?;
    tools.install_special(
        audio_music(&paths[8])?,
        session.vehicle_choices().into_iter().map(|(id, _)| id),
    )?;
    session.set_tool_catalog(tools)?;
    session.set_weapon_pack(weapons.pack)?;
    session.set_event_catalog(
        bri_events::Catalog::load(events_dir.join("catalog.json"))?,
        audio_event_sounds(&paths[8])?,
    )?;
    session.set_item_bounds(item_physics.bounds)?;
    session.set_avatar_catalog(serde_json::from_slice(&std::fs::read(
        paths[6].join("avatar.json"),
    )?)?)?;
    let initial_static_items = session.weapon_view().static_items.len();
    let server = server::start_with_admin_store_and_limit(
        session,
        ServerOptions {
            bind,
            content_id: content_id.clone(),
            spawn_points,
            certificate: Some(server::HostCertificate::load_or_create(&paths[13])?),
            map_loader: None,
        },
        64,
        paths[13].join("administration.json"),
    )?;
    std::fs::create_dir_all(&paths[13])?;
    std::fs::write(paths[13].join("server-cert.der"), &server.certificate)?;
    std::fs::write(
        paths[13].join("host.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"schema_version":1,"address":server.address.to_string(),"content_id":content_id,"content_identity_version":content_identity::CONTENT_IDENTITY_VERSION,"tool_catalog":tool_summary,"initial_static_items":initial_static_items,"unresolved_item_references":unresolved_items,"certificate":"server-cert.der","pending_map_objects":map.pending_objects}),
        )?,
    )?;
    println!(
        "Headless host listening on {}. Public connection metadata: {}",
        server.address,
        paths[13].join("host.json").display()
    );
    if let Some(seconds) = seconds {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
    } else {
        tokio::signal::ctrl_c().await?;
    }
    let report = server.stop().await?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let save = paths[13].join(format!("world-{timestamp}.json"));
    bri_world::persistence::save_new(&save, &report.native_world)?;
    std::fs::write(
        paths[13].join("last-run.json"),
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
