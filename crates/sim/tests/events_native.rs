//! Every converted vanilla save runs its wrench events on the native engine.
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{Session, ToolCatalog},
    simulation::Simulation,
};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, path::Path};

fn json(path: &Path) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

#[test]
#[ignore = "requires the converted native worlds, event catalog and content packs"]
fn vanilla_save_events_install_and_run() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let content = root.join("content");
    let catalog = bri_events::Catalog::load(content.join("events-pack-002/catalog.json"))?;
    let audio = json(&content.join("audio-pack-001/manifest.json"))?;
    let sounds: Vec<String> = audio["sounds"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| {
            s["lists"]
                .as_array()
                .is_some_and(|l| l.iter().any(|v| v == "event-param:Sound"))
        })
        .filter_map(|s| s["id"].as_str().map(str::to_string))
        .collect();
    let music: Vec<String> = audio["sounds"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| {
            s["lists"]
                .as_array()
                .is_some_and(|l| l.iter().any(|v| v == "event-param:Music"))
        })
        .filter_map(|s| s["id"].as_str().map(str::to_string))
        .collect();
    let brick_catalog =
        serde_json::from_value(json(&content.join("stock-catalog-004/stock-catalog.json"))?)?;
    let effects = serde_json::from_value(json(&content.join("effects-pass-004/effects.json"))?)?;
    let materials = serde_json::from_value(json(
        &content.join("brick-materials-001/brick-materials.json"),
    )?)?;
    let weapons = bri_weapons::Pack::from_json(&std::fs::read(
        content.join("weapons-pack-004/weapons.json"),
    )?)?;
    let vehicles = bri_vehicles::Pack::load(content.join("vehicles-pack-009/vehicles.json"))?;
    let mut totals = BTreeMap::<String, usize>::new();
    let mut worlds = 0;
    for entry in std::fs::read_dir(content.join("worlds-pass-005"))? {
        let path = entry?.path();
        if !path.to_string_lossy().ends_with(".world.json") {
            continue;
        }
        let world = bri_world::persistence::load(&path)?;
        if !world.bricks.values().any(|b| !b.events.is_empty()) {
            continue;
        }
        worlds += 1;
        let name = world.name.clone();
        let definitions = Definitions::load(
            &content.join("stock-catalog-004"),
            &content.join("maps-pass-003"),
        )?;
        let mut s = Session::new(Simulation::new(
            world,
            definitions,
            vec![
                ColliderBuilder::cuboid(500.0, 0.5, 500.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )?);
        let mut tools = ToolCatalog::from_native(&brick_catalog, &effects, &materials)?;
        tools.install_items(weapons.items.keys().cloned())?;
        s.set_weapon_pack(weapons.clone())?;
        s.set_vehicle_pack(vehicles.clone())?;
        tools.install_special(
            music.clone(),
            s.vehicle_choices().into_iter().map(|(id, _)| id),
        )?;
        s.set_tool_catalog(tools)?;
        s.set_event_catalog(catalog.clone(), sounds.clone())?;
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)])?;
        let owner = s.join("Tester".into(), Vec3::new(0.0, 0.05, 0.0), true)?;
        s.movement(owner, 1, MoveInput::default())?;
        s.step()?;
        let install: Vec<_> = s.take_event_diagnostics();
        assert!(
            install.iter().all(|d| !d.contains("events disabled")),
            "{name}: {install:?}"
        );
        let triggered: Vec<_> = s
            .simulation()
            .state()
            .bricks
            .iter()
            .filter(|(_, b)| {
                b.events.iter().any(|r| {
                    matches!(
                        r.input.as_str(),
                        "onActivate" | "onPlayerTouch" | "onProjectileHit"
                    )
                })
            })
            .map(|(id, b)| (*id, b.events[0].input.clone()))
            .collect();
        for (brick, input) in &triggered {
            s.fire_brick_input(*brick, input, Some(owner));
        }
        for tick in 0..1200u64 {
            s.movement(owner, tick + 2, MoveInput::default())?;
            s.step()?;
        }
        let diagnostics = s.take_event_diagnostics();
        for d in &diagnostics {
            let key = d
                .split(": ")
                .skip(1)
                .collect::<Vec<_>>()
                .join(": ")
                .chars()
                .take(90)
                .collect::<String>();
            *totals.entry(key).or_default() += 1;
        }
        eprintln!(
            "{name}: fired {} bricks, {} diagnostics, {} pending",
            triggered.len(),
            diagnostics.len(),
            s.pending_events()
        );
    }
    eprintln!("{worlds} evented worlds; diagnostics by kind:");
    for (kind, count) in &totals {
        eprintln!("  {count:5}  {kind}");
    }
    assert!(worlds > 0);
    Ok(())
}
