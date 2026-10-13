//! Why an enabled Add-On is not in a game, in the words the Add-Ons screen
//! and `/body` show: a game mode uses it, or it needs a package world.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use std::path::PathBuf;

fn packages() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages")
}

fn load(add_ons: &[(&str, &str, Side)]) -> Catalog {
    let set = PackageSet {
        schema_version: 1,
        packages: add_ons
            .iter()
            .map(|(id, dir, side)| PackageEntry {
                id: (*id).into(),
                version: "1.0.0".into(),
                side: *side,
                dir: (*dir).into(),
                role: None,
            })
            .collect(),
    };
    Catalog::load(&packages(), &set, true).unwrap_or_else(|e| panic!("{e:#?}"))
}

/// The Playground: its mode uses the Stomper, so a Custom game leaves the
/// Stomper's body out, the way Halo Slayer left out Master Chief.
fn playground() -> Catalog {
    load(&[
        (
            "playground-runner",
            "playground/playground-runner",
            Side::Server,
        ),
        (
            "playground-runner-look",
            "playground/playground-runner-look",
            Side::Client,
        ),
        (
            "playground-runner-hud",
            "playground/playground-runner-hud",
            Side::Client,
        ),
        ("sample-trooper", "samples/sample-trooper", Side::Server),
        (
            "playground-rifle",
            "playground/playground-rifle",
            Side::Shared,
        ),
        (
            "playground-buggy",
            "playground/playground-buggy",
            Side::Shared,
        ),
        ("blockhead_bot", "blockhead_bot", Side::Shared),
        (
            "playground-stomper",
            "playground/playground-stomper",
            Side::Shared,
        ),
        (
            "playground-stomper-look",
            "playground/playground-stomper-look",
            Side::Client,
        ),
        (
            "playground-stomper-rules",
            "playground/playground-stomper-rules",
            Side::Server,
        ),
        (
            "playground-fizzer",
            "playground/playground-fizzer",
            Side::Shared,
        ),
        (
            "playground-fizzer-look",
            "playground/playground-fizzer-look",
            Side::Client,
        ),
        (
            "playground-fizzer-rules",
            "playground/playground-fizzer-rules",
            Side::Server,
        ),
        (
            "playground-mode",
            "playground/playground-mode",
            Side::Server,
        ),
    ])
}

#[test]
fn a_body_a_game_mode_uses_says_which_mode_plays_it() {
    let catalog = playground();
    let mode = "Mixed Add-On Playground";
    let stomper = catalog.plays_only("playground-stomper").unwrap();
    assert_eq!(
        stomper,
        format!(
            "Plays only in {mode} games: a Custom game leaves out the rules and bodies a game mode uses. Pick {mode} as the game mode in Start Game to play it."
        )
    );
    assert_eq!(
        catalog.plays_only("playground-mode").unwrap(),
        format!("Plays only when you pick {mode} as the game mode in Start Game.")
    );
    // Weapons and vehicles load whatever the game: nothing to say.
    assert_eq!(catalog.plays_only("playground-rifle"), None);
    assert_eq!(catalog.plays_only("playground-buggy"), None);
    // What a Custom game on a base map leaves out, and why.
    let custom = catalog.left_out(None, "Slate");
    let ids: Vec<&str> = custom.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&"playground-stomper"), "{ids:?}");
    assert!(!ids.contains(&"playground-rifle"), "{ids:?}");
    let (_, why) = custom
        .iter()
        .find(|(id, _)| id == "playground-stomper")
        .unwrap();
    assert_eq!(why, &stomper);
    // The same choice the host makes.
    let hosted = catalog.for_base_map().unwrap();
    for (id, _) in &custom {
        assert!(!hosted.packages.contains_key(id), "{id} runs after all");
    }
    // The mode's own game leaves nothing of its own out.
    assert!(
        catalog
            .left_out(Some("playground-mode:mode/playground"), "Slate")
            .is_empty()
    );
}

/// The Stress Lab's Add-Ons, with or without their game mode.
fn stress_lab(mode: bool) -> Catalog {
    let mut entries = vec![PackageEntry {
        id: "v20-bricks".into(),
        version: "4.0.0".into(),
        side: Side::Shared,
        dir: "unused".into(),
        role: Some("brick_catalog".into()),
    }];
    for (id, side) in [
        ("stresslab-world", Side::Server),
        ("stresslab-creeper", Side::Server),
        ("stresslab-creeper-model", Side::Client),
        ("stresslab-economy", Side::Server),
        ("stresslab-hud", Side::Client),
        ("stresslab-mode", Side::Server),
    ] {
        if mode || id != "stresslab-mode" {
            entries.push(PackageEntry {
                id: id.into(),
                version: "1.0.0".into(),
                side,
                dir: format!("stresslab/{id}"),
                role: None,
            });
        }
    }
    let set = PackageSet {
        schema_version: 1,
        packages: entries,
    };
    Catalog::load(&packages(), &set, true).unwrap_or_else(|e| panic!("{e:#?}"))
}

#[test]
fn an_add_on_needing_a_world_says_which_map_plays_it() {
    // With its mode on, the mode is what plays them.
    let creeper = stress_lab(true).plays_only("stresslab-creeper").unwrap();
    assert!(
        creeper.starts_with("Plays only in Stress Lab games"),
        "{creeper}"
    );
    // Without it, the world is what the economy needs.
    let catalog = stress_lab(false);
    assert_eq!(
        catalog.plays_only("stresslab-world").unwrap(),
        "Plays only on its own map: pick Stress Lab Strata as the map in Start Game."
    );
    let economy = catalog.plays_only("stresslab-economy").unwrap();
    assert!(
        economy.starts_with("Plays only on Stress Lab Strata's map"),
        "{economy}"
    );
    assert_eq!(catalog.plays_only("stresslab-creeper"), None);
    let out = catalog.left_out(None, "Slate");
    let ids: Vec<&str> = out.iter().map(|(id, _)| id.as_str()).collect();
    // The miner's HUD needs the economy, which needs the world.
    assert_eq!(
        ids,
        ["stresslab-economy", "stresslab-hud", "stresslab-world"],
        "{out:?}"
    );
    assert!(
        catalog
            .left_out(None, "stresslab-world:world/strata")
            .is_empty()
    );
}
