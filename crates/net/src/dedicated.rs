//! The dedicated server's session setup from a content root, shared by
//! `bri-server` and headless tests so both host the same game.
use crate::content_identity;
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
    let item_physics =
        content_identity::ItemPhysicsContent::load_with(&item_presentation_dir, &weapons, &weapon_extras)?;
    let mut vehicle_parts = Vec::new();
    for (dir, abs) in content_identity::kind_providers(content_root, packages, "vehicles.json")? {
        let part = bri_vehicles::Pack::load(abs.join("vehicles.json"))?;
        part.verify_assets(&abs)?;
        vehicle_parts.push((dir, part));
    }
    let (vehicle_pack, vehicle_notes) =
        bri_vehicles::Pack::load(vehicles_dir.join("vehicles.json"))?.merge(vehicle_parts);
    vehicle_pack.validate()?;
    let catalog = serde_json::from_slice(&std::fs::read(catalog_dir.join("stock-catalog.json"))?)?;
    let materials =
        serde_json::from_slice(&std::fs::read(materials_dir.join("brick-materials.json"))?)?;
    let effects = serde_json::from_slice(&std::fs::read(effects_dir.join("effects.json"))?)?;
    let mut tools = ToolCatalog::from_native(&catalog, &effects, &materials)?;
    tools.install_items(weapons.item_choices.iter().map(|(id, _)| id.clone()))?;
    let tool_summary = serde_json::json!({"items":tools.items.len(),"prints":tools.prints.len(),"printable_definitions":tools.brick_print_aspects.len(),"lights":tools.lights.len(),"emitters":tools.emitters.len(),"default_print":tools.default_print});
    let unresolved_items = weapons.resolve_world_items(&mut world)?;
    let map = NativeMap::load(&map_bundle_dir, &world.map_id)?;
    let anchors = map.spawn_anchors()?;
    let mut simulation = Simulation::new(
        world,
        Definitions::load_with(
            &catalog_dir,
            &geometry_dir,
            &content_identity::brick_catalog_providers(content_root, packages)?,
        )?,
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
    Ok(Dedicated {
        session,
        environment,
        spawn_points,
        tool_summary,
        unresolved_items,
        pending_objects: map.pending_objects,
        merge_notes,
    })
}
