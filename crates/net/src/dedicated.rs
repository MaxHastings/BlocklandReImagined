//! The dedicated server's session setup from a content root, shared by
//! `bri-server` and headless tests so both host the same game.
use crate::{
    content_identity,
    host_setup::{HostSetup, HostedAddOns, MapSession, SessionContent},
};
use anyhow::{Context, Result};
use bri_package::{environment::Environment, packages::PackageSet};
use bri_sim::{
    definitions::Definitions,
    map::NativeMap,
    session::{Session, ToolCatalog},
    simulation::Simulation,
};
use glam::Vec3;
use std::path::Path;

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

/// A session ready to serve, and what the host reports about it.
pub struct Dedicated {
    pub session: Session,
    /// How this host sets up a session; also what keeps the Add-Ons' state
    /// when the session ends.
    pub setup: std::sync::Arc<HostSetup>,
    /// Every package this host loaded, hashed: what joiners must match.
    pub environment: Environment,
    pub spawn_points: Vec<Vec3>,
    pub tool_summary: serde_json::Value,
    pub unresolved_items: usize,
    pub pending_objects: Vec<String>,
    /// What merging other packages' content skipped, per kind.
    pub merge_notes: Vec<String>,
}

/// Loads the packages `content_root/packages.json` lists (the base game's
/// and the installed default Add-Ons when absent) and builds the session
/// for `world`.
pub fn load(content_root: &Path, world: bri_world::World) -> Result<Dedicated> {
    load_packages(content_root, &PackageSet::load_root(content_root)?, world)
}

/// [`load`] with an explicit package list.
pub fn load_packages(
    content_root: &Path,
    packages: &PackageSet,
    mut world: bri_world::World,
) -> Result<Dedicated> {
    packages.validate().into_result()?;
    let role = |role: &str| packages.role_dir(content_root, role);
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
    let environment = Environment::load(content_root, packages)?;
    // Other packages providing weapons merge onto the base pack.
    let weapon_extras = content_identity::kind_providers(content_root, packages, "weapons.json")?;
    let weapons = content_identity::WeaponContent::load_with(&weapons_dir, &weapon_extras)?;
    let item_physics = content_identity::ItemPhysicsContent::load_with(
        &item_presentation_dir,
        &weapons,
        &weapon_extras,
    )?;
    let mut vehicle_parts = Vec::new();
    for (dir, abs) in content_identity::kind_providers(content_root, packages, "vehicles.json")? {
        let part = bri_vehicles::Pack::load(abs.join("vehicles.json"))?;
        part.verify_assets(&abs)?;
        vehicle_parts.push((dir, part));
    }
    let (vehicle_pack, vehicle_notes) =
        bri_vehicles::Pack::load(vehicles_dir.join("vehicles.json"))?.merge(vehicle_parts);
    vehicle_pack.validate()?;
    let brick_extras = content_identity::brick_catalog_providers(content_root, packages)?;
    // Add-On bricks are named, printed and wrenched like base ones.
    let catalog = bri_sim::definitions::catalog_with(&catalog_dir, &brick_extras)?;
    let materials =
        serde_json::from_slice(&std::fs::read(materials_dir.join("brick-materials.json"))?)?;
    let effects = serde_json::from_slice(&std::fs::read(effects_dir.join("effects.json"))?)?;
    let mut tools = ToolCatalog::from_native(&catalog, &effects, &materials)?;
    tools.install_items(weapons.item_choices.iter().map(|(id, _)| id.clone()))?;
    tools.install_effects(
        weapons.emitter_choices.iter().map(|(id, _)| id.clone()),
        weapons.light_choices.iter().map(|(id, _)| id.clone()),
    )?;
    let tool_summary = serde_json::json!({"items":tools.items.len(),"prints":tools.prints.len(),"printable_definitions":tools.brick_print_aspects.len(),"lights":tools.lights.len(),"emitters":tools.emitters.len(),"default_print":tools.default_print});
    let unresolved_items = weapons.resolve_world_items(&mut world)?;
    let map = NativeMap::load(&map_bundle_dir, &world.map_id)?;
    let anchors = map.spawn_anchors()?;
    let mut simulation = Simulation::new(
        world,
        Definitions::load_with(&catalog_dir, &geometry_dir, &brick_extras)?,
        map.colliders,
    )?;
    simulation.attach_terrain(map.terrain, anchors)?;
    simulation.waters = map.waters;
    if let Some(skipped) = bri_sim::simulation::unloaded_summary(&simulation.state().unloaded) {
        eprintln!("{skipped}");
    }
    let spawn_points =
        bri_sim::spawn::candidates(&simulation.physics, &map.scene, &Default::default())?;
    let merge_notes = weapons
        .pack
        .diagnostics
        .iter()
        .filter(|d| d.starts_with("merge: "))
        .cloned()
        .chain(vehicle_notes.into_iter().map(|n| format!("merge: {n}")))
        .collect();
    let bot_kinds = content_identity::bot_kinds(content_root, packages)?;
    // What a Vehicle Spawn brick may hold, as the game's own host offers it.
    tools.install_special(
        audio_music(&audio_dir)?,
        vehicle_pack
            .definitions
            .iter()
            .filter(|d| d.family.spawnable())
            .map(|d| d.id.clone())
            .chain(bot_kinds.iter().map(|k| k.id.clone())),
    )?;
    // Add-On scripts run as they do in a game the client hosts; one broken
    // Add-On is left out and reported.
    let (server, problems) =
        bri_package_runtime::Catalog::load_skipping(content_root, packages, true);
    for problem in problems {
        eprintln!("Add-On left out: {problem}");
    }
    let avatar: bri_content::avatar::Package =
        serde_json::from_slice(&std::fs::read(avatar_dir.join("avatar.json"))?)?;
    avatar.validate()?;
    // The Blockhead's mount points (`mountObject`), from its rig.
    let bytes = std::fs::read(avatar_dir.join(&avatar.rig))?;
    anyhow::ensure!(
        format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&bytes)) == avatar.rig_sha256,
        "Avatar rig checksum mismatch"
    );
    let rig: bri_content::avatar::Rig = serde_json::from_slice(&bytes)?;
    rig.validate()?;
    let body_mounts = bri_sim::session::shape_mount_points(&rig.shape);
    let setup = HostSetup {
        lan: false,
        content: SessionContent {
            tool_catalog: tools,
            weapon_pack: weapons.pack,
            item_bounds: item_physics.bounds,
            avatar_catalog: avatar,
            body_mounts,
            vehicle_pack,
            bot_kinds,
            event_catalog: bri_events::Catalog::load(events_dir.join("catalog.json"))?,
            event_sounds: audio_event_sounds(&audio_dir)?,
        },
        maps: Vec::new(),
        settings: None,
        passwords: None,
        add_ons: (!server.packages.is_empty()).then(|| HostedAddOns {
            server: std::sync::Arc::new(server),
            mode: None,
            saves: None,
        }),
        load_map: None,
        copies: None,
        game_version: None,
    };
    let hosted = setup.hosted(&simulation.state().map_id)?;
    let (session, spawn_points) = setup.session(
        &hosted,
        MapSession {
            simulation,
            spawn_points,
            breakables: map.breakables,
            tutorial: None,
        },
    )?;
    Ok(Dedicated {
        session,
        setup: std::sync::Arc::new(setup),
        environment,
        spawn_points,
        tool_summary,
        unresolved_items,
        pending_objects: map.pending_objects,
        merge_notes,
    })
}
