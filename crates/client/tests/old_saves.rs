//! The real v20 saves, as a player would bring them over: dropped into the
//! saves folder, converted by the game, and listed and loaded by Load Bricks.
//! The worlds pack keeps each stock save's original `.bls` beside its
//! offline conversion, so the game's result can be compared exactly.
use anyhow::{Context, Result, ensure};
use bri_client::{
    content::ClientContent,
    old_saves::{Converter, OldSaves},
    saves::Store,
};
use std::path::{Path, PathBuf};

fn content() -> Result<ClientContent> {
    let root = std::env::var_os("BRI_CONTENT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    );
    ClientContent::load(&root)
}

#[derive(serde::Deserialize)]
struct Report {
    saves: Vec<Entry>,
}
#[derive(serde::Deserialize)]
struct Entry {
    source: String,
    sha256: String,
    file: String,
}

/// Where two conversions of one save first part, for the failure message.
fn first_difference(game: &bri_world::World, offline: &bri_world::World) -> String {
    if game.bricks.len() != offline.bricks.len() {
        let lines = |w: &bri_world::World| -> std::collections::BTreeSet<u32> {
            w.bricks
                .values()
                .filter_map(|b| b.source_records.first().map(|r| r.line))
                .collect()
        };
        let (g, o) = (lines(game), lines(offline));
        return format!(
            "{} bricks in the game, {} offline; lines only in the game {:?}, only offline {:?}",
            game.bricks.len(),
            offline.bricks.len(),
            g.difference(&o).take(5).collect::<Vec<_>>(),
            o.difference(&g).take(5).collect::<Vec<_>>()
        );
    }
    for (a, b) in game.bricks.values().zip(offline.bricks.values()) {
        if a != b {
            return format!("game {a:?}\noffline {b:?}");
        }
    }
    let mut game = game.clone();
    game.bricks = offline.bricks.clone();
    format!(
        "world fields: encoding {:?} vs {:?}, palette equal {}, description equal {}, other {}",
        game.source_encoding,
        offline.source_encoding,
        game.palette == offline.palette,
        game.description == offline.description,
        game == *offline
    )
}

#[test]
#[ignore = "generated content (BRI_CONTENT or content/); no window"]
fn the_game_converts_every_stock_save_as_the_offline_converter_did() -> Result<()> {
    let content = content()?;
    let converter = Converter::new(&content)?;
    let worlds = &content.paths.worlds;
    let report: Report = serde_json::from_slice(&std::fs::read(worlds.join("report.json"))?)?;
    ensure!(report.saves.len() >= 30, "expected the stock saves");
    for save in &report.saves {
        let (folder, file) = save.source.split_once('/').context("map folder")?;
        let name = file.strip_suffix(".bls").context("bls")?;
        let map_id = bri_client::content::map_for_save_folder(folder).context("stock map")?;
        let bytes = std::fs::read(
            worlds
                .join("provenance")
                .join(format!("{}.source.bls", save.sha256)),
        )?;
        let converted = converter
            .convert(&bytes, name, map_id)
            .with_context(|| save.source.clone())?;
        // The stock pack leaves items to be resolved as a map loads.
        let mut offline = bri_world::persistence::load(&worlds.join(&save.file))?;
        content.weapons.resolve_world_items(&mut offline)?;
        ensure!(
            converted == offline,
            "{} converts differently in the game: {}",
            save.source,
            first_difference(&converted, &offline)
        );
    }
    Ok(())
}

#[test]
#[ignore = "generated content (BRI_CONTENT or content/); no window"]
fn dropped_v20_saves_list_and_load_in_load_bricks() -> Result<()> {
    let content = content()?;
    let worlds = &content.paths.worlds;
    let report: Report = serde_json::from_slice(&std::fs::read(worlds.join("report.json"))?)?;
    let state = tempfile::tempdir()?;
    let saves = state.path().join("saves");
    // Two saves in v20's layout and one loose.
    let picks = ["Bedroom/Demo Pong.bls", "Slate/", "Kitchen/"];
    let mut dropped = vec![];
    for pick in picks {
        let save = report
            .saves
            .iter()
            .find(|s| s.source.starts_with(pick))
            .with_context(|| pick.to_string())?;
        let target = if pick.starts_with("Kitchen") {
            saves.join(save.source.split_once('/').unwrap().1)
        } else {
            saves.join(&save.source)
        };
        std::fs::create_dir_all(target.parent().unwrap())?;
        let bytes = std::fs::read(
            worlds
                .join("provenance")
                .join(format!("{}.source.bls", save.sha256)),
        )?;
        std::fs::write(&target, &bytes)?;
        dropped.push((target, bytes, save));
    }
    let old = OldSaves::new(saves.clone(), state.path().join("converted-saves"));
    old.set_converter(Converter::new(&content)?);
    old.start();
    while old.busy() {
        std::thread::yield_now();
    }
    let store = Store::new(state.path(), &content, Some(old.clone()));
    let listed = store.list()?;
    for (path, bytes, save) in &dropped {
        let name = bri_client::saves::v20_save_name(&path.file_stem().unwrap().to_string_lossy())
            .context("save name")?;
        let map = if path.parent() == Some(saves.as_path()) {
            "Other".to_string()
        } else {
            store.map_name(
                bri_client::content::map_for_save_folder(save.source.split_once('/').unwrap().0)
                    .unwrap(),
            )
        };
        let entry = listed
            .iter()
            .find(|e| e.info.map == map && e.info.name == name && e.path.starts_with(old.cache()))
            .with_context(|| format!("{map}/{name} is not listed"))?;
        let build = store.load(&entry.info.map, &entry.info.name)?;
        let offline = bri_world::persistence::load(&worlds.join(&save.file))?;
        ensure!(
            build.world.bricks.len() == offline.bricks.len()
                && build.world.palette == offline.palette
                && entry.info.brick_count == Some(offline.bricks.len() as u32),
            "{name} lost bricks or colours"
        );
        ensure!(
            std::fs::read(path)? == *bytes,
            "{} was changed",
            path.display()
        );
    }
    // Demo Pong keeps its wrench events.
    let pong = store.load(
        &store.map_name(bri_client::content::map_for_save_folder("Bedroom").unwrap()),
        "Demo Pong.world.json",
    )?;
    ensure!(
        pong.world.bricks.values().any(|b| !b.events.is_empty()),
        "Demo Pong lost its events"
    );
    Ok(())
}

/// A v20 duplication keeps its bricks' names and events, bound as a save's
/// are, so a copy loaded from it plants them.
#[test]
#[ignore = "generated content (BRI_CONTENT or content/); no window"]
fn a_dropped_duplication_keeps_its_bricks_names_and_events() -> Result<()> {
    let content = content()?;
    let converter = Converter::new(&content)?;
    let source = format!(
        "Duplorcation save file\t2\n1\nDuplication saved by test\n{}Linecount 1\n",
        "0.5 0.25 0 1\n".repeat(64)
    ) + "2x2\" 0 0 0.3 0 1 5  0 0 1 1 1\n"
        + "+-NTOBJECTNAME _door\n"
        + "+-EVENT\t0\t1\tonActivate\t0\tSelf\t\tfireRelay\t\t\t\t\n";
    let (bricks, palette) = converter.read_duplication(source.as_bytes(), "dup")?;
    ensure!(palette.len() == 64);
    ensure!(bricks.len() == 1, "{} bricks", bricks.len());
    ensure!(
        bricks[0].name.as_deref() == Some("_door"),
        "{:?}",
        bricks[0].name
    );
    ensure!(bricks[0].events.len() == 1, "{:?}", bricks[0].events);
    Ok(())
}
