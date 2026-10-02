//! Bot_Hole bots on import: a hole bot `PlayerData` becomes a body and a bot
//! kind, its hole brick keeps one, and the Zombie port adds what its scripts
//! did. Run on our CC0 stand-in (`tests/fixtures/ports/Bot_Zombie`).
use bri_addon_import::{Options, import};
use std::path::{Path, PathBuf};

fn fresh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bri-bot-holes-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_zombie_port_gives_its_bot_a_look_its_arms_out_and_its_bite_turns_bots() {
    let dir = fresh("zombie");
    let out = dir.join("bot_zombie");
    let report = import(&Options {
        input: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ports/Bot_Zombie"),
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert!(
        report.needs_behaviour.iter().all(|b| b.port.is_some()),
        "every function is ported"
    );

    // The kind, from the stand-in's h settings (Bot_Hole's brick units are
    // half a world unit; a search radius loses 5 first) and the port.
    let pack = bri_sim::bot_kind::BotPack::from_json(
        &std::fs::read(out.join("assets/bots.json")).unwrap(),
    )
    .unwrap();
    let [kind] = &pack.bots[..] else {
        panic!("one kind: {:?}", pack.bots)
    };
    assert_eq!(kind.id, "bot_zombie:bot/zombieholebot");
    assert_eq!(kind.name, "Shambler");
    assert_eq!(kind.side.as_deref(), Some("shamblers"));
    assert_eq!(kind.sight, 20.0);
    assert_eq!(kind.wander_radius, 12.0);
    assert_eq!(
        kind.body.as_deref(),
        Some("bot_zombie:archetype/zombieholebot")
    );
    let melee = kind.melee.as_ref().expect("it swipes");
    assert_eq!(melee.damage, 9.0);
    assert_eq!(melee.action.as_deref(), Some("activate2"));
    assert_eq!(melee.converts_below, Some(0.5));
    assert_eq!(kind.emote.as_deref(), Some("hug"));
    let look = kind.look.as_ref().expect("its paint");
    assert_eq!(look.colors["head"], [0.5, 0.6, 0.4, 1.0]);
    assert_eq!(look.colors["torso"], [0.3, 0.3, 0.3, 1.0]);
    assert_eq!(look.face.as_deref(), Some("smileyEvil1"));

    // Its body is a player type, and its hole brick keeps one of it.
    let body: serde_json::Value = serde_json::from_slice(
        &std::fs::read(out.join("assets/archetypes/zombieholebot.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(body["max_health"], 40.0);
    let catalog: bri_content::brick::Catalog = serde_json::from_slice(
        &std::fs::read(out.join("assets/brick-catalog/stock-catalog.json")).unwrap(),
    )
    .unwrap();
    let hole = catalog
        .bricks
        .iter()
        .find(|b| b.display_name == "Zombie Hole")
        .unwrap();
    assert_eq!(hole.bot.as_deref(), Some("bot_zombie:bot/zombieholebot"));
    let manifest = std::fs::read_to_string(out.join("package.json")).unwrap();
    assert!(manifest.contains("\"bot_zombie:bots/main\""), "{manifest}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_shark_port_makes_a_swimmer_that_bites_and_dies_on_land() {
    let dir = fresh("shark");
    let out = dir.join("bot_shark");
    let report = import(&Options {
        input: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ports/Bot_Shark"),
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    let pack = bri_sim::bot_kind::BotPack::from_json(
        &std::fs::read(out.join("assets/bots.json")).unwrap(),
    )
    .unwrap();
    let [kind] = &pack.bots[..] else {
        panic!("one kind: {:?}", pack.bots)
    };
    assert_eq!(kind.name, "Biter");
    assert_eq!(kind.moves, bri_sim::bot_kind::Moves::Swim);
    // Bot_Hole's loop is 3 s; the stand-in gives up after 3 of them.
    assert_eq!(kind.out_of_water_seconds, Some(9.0));
    // The bite it makes on contact, not the 0 its datablock says.
    assert_eq!(kind.melee.as_ref().unwrap().damage, 35.0);
    let look = kind.look.as_ref().unwrap();
    assert_eq!(look.colors["torso"], [0.8, 0.8, 0.85, 1.0]);
    assert_eq!(look.colors["larm"], [0.95, 0.95, 0.95, 1.0]);
    // It swims: the speeds its script zeroes are given.
    let body: serde_json::Value = serde_json::from_slice(
        &std::fs::read(out.join("assets/archetypes/sharkholebot.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(body["movement"]["underwater_forward"], 10.0);
    std::fs::remove_dir_all(dir).unwrap();
}
