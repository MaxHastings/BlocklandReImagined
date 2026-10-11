//! Private-content acceptance for the native P90, on the exact v0.2.8 runtime.
//! Run ignored tests with BRI_P90_TEST_ROOT pointing at the builder output and
//! BRI_P90_CONTENT_ROOT pointing at the unmodified installed game's content.
//! That folder needs the `packages.json` an installed game has; a checkout's
//! generated `content/` has none (copy `crates/package/base-packages.json`
//! there as `packages.json` for the run).
use anyhow::{Context, Result, ensure};
use bri_net::{
    client::Client,
    packages::{PackageShelf, fetch_missing},
    server,
};
use bri_package::{
    environment::{Environment, hash_dir},
    packages::{PackageEntry, PackageSet, Side},
    sync::Cache,
};
use bri_package_runtime::Catalog;
use bri_progress::Progress;
use bri_sim::{
    player::MoveInput,
    session::{Command, MiniGameRequest, Session, ToolInventory},
};
use glam::Vec3;
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};
mod common;

const DATA: &str = "weapon_rallypack_p90";
const RULES: &str = "weapon_rallypack_p90-rules";
const ITEM: &str = "weapon_rallypack_p90:weapon/r_p90item";

fn root() -> Result<PathBuf> {
    Ok(std::env::var_os("BRI_P90_TEST_ROOT")
        .context("BRI_P90_TEST_ROOT required")?
        .into())
}
fn set() -> PackageSet {
    PackageSet {
        schema_version: 1,
        packages: vec![
            PackageEntry {
                id: DATA.into(),
                version: "0.1.0".into(),
                side: Side::Shared,
                dir: DATA.into(),
                role: None,
            },
            PackageEntry {
                id: RULES.into(),
                version: "0.1.0".into(),
                side: Side::Server,
                dir: RULES.into(),
                role: None,
            },
        ],
    }
}
fn game() -> Result<Session> {
    game_with(&set())
}
fn game_with(packages: &PackageSet) -> Result<Session> {
    let root = root()?;
    let installed = PathBuf::from(
        std::env::var_os("BRI_P90_CONTENT_ROOT").context("BRI_P90_CONTENT_ROOT required")?,
    );
    let installed_set = PackageSet::parse(&std::fs::read(installed.join("packages.json"))?)?;
    let weapons_dir = &installed_set
        .packages
        .iter()
        .find(|e| e.id == "v20-weapons")
        .context("base weapons")?
        .dir;
    let base = bri_weapons::Pack::from_json(&std::fs::read(
        installed.join(weapons_dir).join("weapons.json"),
    )?)?;
    let addon =
        bri_weapons::Pack::from_json(&std::fs::read(root.join(DATA).join("assets/weapons.json"))?)?;
    let (weapons, problems) = base.merge(vec![(format!("{DATA}/assets"), addon)]);
    ensure!(problems.is_empty(), "merge problems: {problems:?}");
    weapons.validate()?;
    let catalog = Catalog::load(&root, packages, true).map_err(|d| anyhow::anyhow!("{d:?}"))?;
    let mut game = common::session();
    game.set_weapon_pack(weapons)?;
    game.install_packages(Arc::new(catalog), None)?;
    game.set_spawn_loadout(ToolInventory {
        slots: vec![Some(ITEM.into()), None, None, None, None],
        selected: None,
    })?;
    Ok(game)
}
fn value(game: &Session, owner: u64, key: &str) -> Value {
    game.package_store()
        .unwrap()
        .player(
            RULES,
            &bri_package_runtime::state::PlayerKey::session(owner),
            key,
        )
        .unwrap()
        .clone()
}
fn steps(game: &mut Session, n: usize) -> Result<()> {
    for _ in 0..n {
        game.step()?;
    }
    ensure!(
        game.package_diagnostics().is_empty(),
        "{:?}",
        game.package_diagnostics()
    );
    Ok(())
}
fn send(game: &mut Session, owner: u64, seq: &mut u64, command: Command) -> Result<()> {
    *seq += 1;
    game.command(owner, *seq, command)?;
    Ok(())
}
fn fire(game: &mut Session, owner: u64, seq: &mut u64, held: usize) -> Result<()> {
    send(game, owner, seq, Command::WeaponTrigger { down: true })?;
    // Keep the ordinary input lease alive while holding the trigger.
    for _ in 0..held {
        *seq += 1;
        game.movement(owner, *seq, MoveInput::default())?;
        game.step()?;
    }
    send(game, owner, seq, Command::WeaponTrigger { down: false })?;
    steps(game, 12)
}

#[test]
#[ignore = "requires Maxwell's privately converted P90 assets"]
fn p90_fires_reloads_preserves_magazines_and_resets_on_respawn() -> Result<()> {
    let mut game = game()?;
    let host = game.join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)?;
    let mut seq = 0;
    steps(&mut game, 5)?;
    send(
        &mut game,
        host,
        &mut seq,
        Command::EquipTool { slot: Some(0) },
    )?;
    steps(&mut game, 30)?;
    assert_eq!(value(&game, host, "rounds"), 50);
    fire(&mut game, host, &mut seq, 20)?;
    let left = value(&game, host, "rounds").as_i64().unwrap();
    assert!(
        (46..50).contains(&left),
        "rounds after 20 held ticks: {left}"
    );
    assert!(
        game.weapon_view()
            .projectiles
            .iter()
            .any(|p| p.definition.ends_with("r_assaultrifleprojectile"))
    );
    send(&mut game, host, &mut seq, Command::ToggleLight)?;
    steps(&mut game, 260)?;
    assert_eq!(value(&game, host, "rounds"), 51, "chambered round retained");
    assert_eq!(
        value(&game, host, "spares"),
        serde_json::json!([50, left - 1])
    );
    fire(&mut game, host, &mut seq, 50)?;
    send(&mut game, host, &mut seq, Command::ToggleLight)?;
    steps(&mut game, 40)?;
    send(&mut game, host, &mut seq, Command::EquipTool { slot: None })?;
    steps(&mut game, 280)?;
    assert_eq!(value(&game, host, "gun")["reloading"], false);
    assert_ne!(
        value(&game, host, "rounds"),
        51,
        "cancelled reload never refills"
    );
    send(&mut game, host, &mut seq, Command::Suicide)?;
    steps(&mut game, 180)?;
    send(&mut game, host, &mut seq, Command::Respawn)?;
    steps(&mut game, 10)?;
    assert_eq!(value(&game, host, "rounds"), 50);
    assert_eq!(value(&game, host, "spares"), serde_json::json!([50, 50]));
    Ok(())
}

#[test]
#[ignore = "requires Maxwell's privately converted P90 assets"]
fn p90_exhausted_ammo_stays_idle_after_trigger_release() -> Result<()> {
    let mut game = game()?;
    let host = game.join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)?;
    let mut seq = 0;
    steps(&mut game, 5)?;
    send(
        &mut game,
        host,
        &mut seq,
        Command::EquipTool { slot: Some(0) },
    )?;
    steps(&mut game, 30)?;
    fire(&mut game, host, &mut seq, 2600)?;
    assert_eq!(value(&game, host, "rounds"), 0);
    assert_eq!(value(&game, host, "spares"), serde_json::json!([]));
    steps(&mut game, 120)?;
    assert_eq!(value(&game, host, "gun")["reloading"], false);
    assert_eq!(game.weapon_view().images[&host][0].state, "Ready");
    Ok(())
}

/// A mini-game with one bot armed with `loadout` and the host as its
/// target; the probe package spawns the bot and counts its hits on the host
/// (the host takes no damage). Returns the game, the host, the bot and the
/// probe folder, which must outlive the game.
fn bot_duel(loadout: [Option<String>; 5]) -> Result<(Session, u64, u64, tempfile::TempDir)> {
    use bri_sim::{bot_kind::BotKind, session::PackageCommand};
    let root = root()?;
    let probe = tempfile::Builder::new()
        .prefix("bot-probe-")
        .tempdir_in(&root)?;
    let write = |name: &str, value: Value| -> Result<()> {
        std::fs::write(probe.path().join(name), serde_json::to_vec(&value)?)?;
        Ok(())
    };
    write(
        "package.json",
        serde_json::json!({
            "schema_version":1,"api":1,"id":"bot_probe","version":"0.1.0",
            "name":"Private bot probe","authors":[],"license":"proprietary",
            "dependencies":{},"capabilities":["bots","damage","player"],
            "provides":[
                {"kind":"behaviour","id":"bot_probe:behaviour/main","file":"behaviour.json"},
                {"kind":"script","id":"bot_probe:script/main","file":"probe.rhai"}]
        }),
    )?;
    write(
        "behaviour.json",
        serde_json::json!({
            "schema_version":1,"script":"probe.rhai","commands":[{"name":"spawn"},{"name":"arm"}],
            "on_damage":true,"state":{"player":{"hits":{"default":0,"persist":false}}}
        }),
    )?;
    std::fs::write(
        probe.path().join("probe.rhai"),
        "fn cmd_spawn(p) { add_bot(player(p).minigame, #{kind:\"probe\",name:\"Gunner\"}); }\n\
         fn cmd_arm(p) { for b in bots() { give_item(b, \"v20.weapon.gunitem\", false); } }\n\
         fn on_damage(p,a,n,i) { if !player(p).bot {set_player(p,\"hits\",get_player(p,\"hits\")+1);0.0} else {()} }",
    )?;
    let mut packages = set();
    packages.packages.push(PackageEntry {
        id: "bot_probe".into(),
        version: "0.1.0".into(),
        side: Side::Server,
        dir: probe.path().file_name().unwrap().to_str().unwrap().into(),
        role: None,
    });
    let mut game = game_with(&packages)?;
    let mut kind = BotKind {
        id: "probe".into(),
        name: "Probe".into(),
        aim_error_degrees: 0.0,
        ..Default::default()
    };
    kind.surprise.strength = 0.0;
    kind.perception.strength = 0.0;
    game.set_bot_kinds(vec![kind])?;
    game.set_spawn_points(vec![Vec3::new(0.0, 0.05, -10.0)])?;
    let host = game.join("Target".into(), Vec3::new(0.0, 0.05, -10.0), true)?;
    let settings = bri_minigames::Settings {
        loadout,
        ..Default::default()
    };
    game.command(
        host,
        1,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )?;
    game.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)])?;
    game.command(
        host,
        2,
        Command::Package(PackageCommand {
            package: "bot_probe".into(),
            command: "spawn".into(),
            args: vec![],
        }),
    )?;
    steps(&mut game, 5)?;
    let bot = game
        .vitals()
        .keys()
        .copied()
        .find(|p| game.is_bot(*p))
        .context("spawned bot")?;
    Ok((game, host, bot, probe))
}

#[test]
#[ignore = "requires Maxwell's privately converted P90 assets"]
fn bot_fires_package_p90_and_reloads_through_ordinary_controls() -> Result<()> {
    let (mut game, host, bot, _probe) = bot_duel([Some(ITEM.into()), None, None, None, None])?;
    let mut fired = false;
    let mut reloaded = false;
    for _ in 0..4200 {
        game.step()?;
        fired |= game
            .weapon_view()
            .projectiles
            .iter()
            .any(|p| p.definition.ends_with("r_assaultrifleprojectile"));
        reloaded |= value(&game, bot, "spares")
            .as_array()
            .is_some_and(|a| a.len() < 2);
        if fired && reloaded {
            break;
        }
    }
    assert!(
        fired,
        "the bot must fire without injected trigger commands: {:?}",
        game.bot_why()
    );
    assert!(
        reloaded,
        "the bot must finish an automatic empty-magazine reload: {:?}",
        game.bot_why()
    );
    let hits = game.package_store().unwrap().player(
        "bot_probe",
        &bri_package_runtime::PlayerKey::session(host),
        "hits",
    );
    assert!(
        hits.and_then(Value::as_i64).unwrap_or(0) > 0,
        "bot rounds must reach the target"
    );
    Ok(())
}

#[test]
#[ignore = "requires Maxwell's privately converted P90 assets"]
fn p90_rounds_hurt_an_enemy_in_a_minigame() -> Result<()> {
    let mut game = game()?;
    let host = game.join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)?;
    let guest = game.join("Guest".into(), Vec3::new(0.0, 0.05, -8.0), false)?;
    let mut seq = 0;
    let mut settings = bri_minigames::Settings::default();
    settings.loadout[0] = Some(ITEM.into());
    send(
        &mut game,
        host,
        &mut seq,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )?;
    let id = game.minigame_views()[0].id;
    game.command(
        guest,
        1,
        Command::MiniGame(MiniGameRequest::Join { game: id }),
    )?;
    steps(&mut game, 320)?;
    send(
        &mut game,
        host,
        &mut seq,
        Command::EquipTool { slot: Some(0) },
    )?;
    steps(&mut game, 30)?;
    let before = game.vitals()[&guest].health;
    fire(&mut game, host, &mut seq, 2)?;
    steps(&mut game, 30)?;
    assert!(
        game.vitals()[&guest].health < before,
        "P90 shot must deal authoritative minigame damage"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Maxwell's privately converted P90 assets"]
async fn p90_downloads_to_a_clean_cache_and_equipment_reaches_a_late_joiner() -> Result<()> {
    let root = root()?;
    let set = set();
    let environment = Environment::load(&root, &set)?;
    let shelf = PackageShelf::new(&root, &set, &environment)?;
    let server = server::start(
        game()?,
        server::ServerOptions {
            packages: Some(Arc::new(shelf)),
            environment: environment.clone(),
            ..common::options()
        },
    )?;
    let cache_dir = tempfile::tempdir()?;
    let cache = Cache::open(cache_dir.path())?;
    let fetched = fetch_missing(
        server.address,
        &server.certificate,
        &cache,
        &Progress::default(),
    )
    .await?;
    assert_eq!(
        fetched.len(),
        1,
        "only shared data is sent; host rules stay on host"
    );
    assert_eq!(fetched[0].package.id, DATA);
    assert!(fetched[0].downloaded > 0);
    assert_eq!(hash_dir(&fetched[0].dir)?.0, fetched[0].package.hash);
    let received = bri_package_runtime::check::check(&fetched[0].dir);
    ensure!(
        received.ok,
        "downloaded package does not load: {:?}",
        received.diagnostics
    );
    let again = fetch_missing(
        server.address,
        &server.certificate,
        &cache,
        &Progress::default(),
    )
    .await?;
    assert_eq!(again[0].downloaded, 0);
    let refs = environment.client_packages();
    let mut host = Client::connect(
        server.address,
        &server.certificate,
        "Host".into(),
        refs.clone(),
        None,
    )
    .await?;
    host.command(Command::EquipTool { slot: Some(0) }).await?;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let guest = Client::connect(
        server.address,
        &server.certificate,
        "Guest".into(),
        refs,
        None,
    )
    .await?;
    assert_eq!(
        guest.replica.tools[&host.owner].slots[0].as_deref(),
        Some(ITEM)
    );
    assert_eq!(guest.replica.tools[&host.owner].selected, Some(0));
    drop(guest);
    drop(host);
    server.stop().await?;
    Ok(())
}

#[test]
#[ignore = "requires Maxwell's privately converted P90 assets"]
fn bot_switches_from_an_empty_p90_to_its_other_gun_and_never_fires_it_dry() -> Result<()> {
    use bri_sim::session::PackageCommand;
    // The P90 alone until it is dry (a bot given both prefers the v20 gun),
    // then the probe hands the bot a second gun.
    let (mut game, host, bot, _probe) = bot_duel([Some(ITEM.into()), None, None, None, None])?;
    // Rounds fired before the gun went dry keep flying for their lifetime;
    // only a round newer than the last one at that moment is a dry shot.
    let newest_p90 = |game: &Session| {
        game.weapon_view()
            .projectiles
            .iter()
            .filter(|p| p.definition.ends_with("r_assaultrifleprojectile"))
            .map(|p| p.id)
            .max()
    };
    let other_shot = |game: &Session| {
        game.weapon_view()
            .projectiles
            .iter()
            .any(|p| !p.definition.ends_with("r_assaultrifleprojectile"))
    };
    let is_dry = |game: &Session| {
        value(game, bot, "rounds") == 0
            && value(game, bot, "spares")
                .as_array()
                .is_some_and(Vec::is_empty)
    };
    let mut dry_at = None;
    for tick in 0..60_000 {
        game.step()?;
        if tick % 5000 == 0 {
            eprintln!(
                "tick {tick}: rounds {} spares {} {:?}",
                value(&game, bot, "rounds"),
                value(&game, bot, "spares"),
                game.bot_why()
            );
        }
        if is_dry(&game) {
            dry_at = Some(tick);
            break;
        }
    }
    ensure!(
        dry_at.is_some(),
        "the bot never emptied the P90: rounds {} spares {}: {:?}",
        value(&game, bot, "rounds"),
        value(&game, bot, "spares"),
        game.bot_why()
    );
    let last = newest_p90(&game).unwrap_or(0);
    let dry_shot = |game: &Session| newest_p90(game).is_some_and(|id| id > last);
    for _ in 0..900 {
        game.step()?;
        ensure!(
            !dry_shot(&game),
            "a bot with only an empty P90 fired it: {:?}",
            game.bot_why()
        );
    }
    game.command(
        host,
        3,
        Command::Package(PackageCommand {
            package: "bot_probe".into(),
            command: "arm".into(),
            args: vec![],
        }),
    )?;
    let mut other = false;
    for tick in 0..6000 {
        game.step()?;
        ensure!(
            !dry_shot(&game),
            "the bot fired the empty P90 with another gun to hand: {:?}",
            game.bot_why()
        );
        other |= other_shot(&game);
        if other && tick > 600 {
            break;
        }
    }
    ensure!(
        other,
        "the bot never took up its other gun once the P90 was empty: {:?}",
        game.bot_why()
    );
    Ok(())
}
