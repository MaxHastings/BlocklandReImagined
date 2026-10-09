//! A bot holding a wind-up melee weapon (held to charge, let go to strike,
//! let go early for a weaker stab, as the Butterfly Knife Add-On is built)
//! fights a player who sidesteps and finishes its wind-ups: each wind-up
//! it begins in a fight ends in a full strike within the weapon's own
//! charge time and the hold time its kind gives, unless one of them dies.
//! Before, an enemy stepping off aim made the bot put the charge away and
//! start again, and an idle click with it did the same, over and over.
//! Every number is read from the data.
use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_weapons::{Image, Item, ProjectileDef, State};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use serde_json::json;

const KNIFE: &str = "windup:weapon/knife";
const IMAGE: &str = "windup:image/knife";
const STRIKE: &str = "windup:projectile/strike";
const STAB: &str = "windup:projectile/stab";

/// The knife: Ready, a Charge it must be held through to Armed, an early
/// release's stab, and from Armed a full release's strike.
fn knife() -> bri_weapons::Pack {
    let state = |name: &str, ticks: u32, script: &str| State {
        name: name.into(),
        ticks,
        script: script.into(),
        wait: true,
        ..Default::default()
    };
    let states = vec![
        State {
            timeout: Some(1),
            ..state("Activate", 30, "")
        },
        State {
            down: Some(2),
            ..state("Ready", 0, "")
        },
        State {
            timeout: Some(5),
            up: Some(3),
            wait: false,
            ..state("Charge", 60, "onCharge")
        },
        State {
            timeout: Some(4),
            ..state("EarlyRelease", 18, "onStab")
        },
        State {
            timeout: Some(1),
            ..state("StopFire", 18, "onStopFire")
        },
        State {
            up: Some(6),
            ..state("Armed", 0, "")
        },
        State {
            timeout: Some(1),
            ..state("Fire", 18, "onFire")
        },
    ];
    let blade = |id: &str, damage: f32| ProjectileDef {
        id: id.into(),
        name: id.into(),
        speed: 40.0,
        gravity: 0.0,
        lifetime_ticks: 12,
        damage,
        ..Default::default()
    };
    let mut pack = bri_weapons::testing::pack();
    pack.images.insert(
        IMAGE.into(),
        Image {
            id: IMAGE.into(),
            name: "knife".into(),
            projectile: Some(STRIKE.into()),
            arm_ready: true,
            states,
            scripts: serde_json::from_value(json!({
                "oncharge": {"arm": "spearready"},
                "onfire": {"arm": "spearthrow", "fire": true},
                "onstab": {"fire": true, "projectile": STAB},
                "onstopfire": {"arm": "root"}
            }))
            .unwrap(),
            ..Default::default()
        },
    );
    pack.items.insert(
        KNIFE.into(),
        Item {
            id: KNIFE.into(),
            name: "knife".into(),
            ui_name: "Knife".into(),
            image: IMAGE.into(),
            ..Default::default()
        },
    );
    for p in [blade(STRIKE, 100.0), blade(STAB, 30.0)] {
        pack.projectiles.insert(p.id.clone(), p);
    }
    pack
}

/// The player sidesteps back and forth, `pace` turns a second.
fn steps(s: &mut Session, player: OwnerId, n: usize, sequence: &mut u64, pace: f32) {
    for _ in 0..n {
        *sequence += 1;
        let t = *sequence as f32 / bri_weapons::TICK_HZ as f32;
        let input = MoveInput {
            right: (t * pace * std::f32::consts::TAU).sin(),
            ..MoveInput::default()
        };
        s.movement(player, *sequence, input).unwrap();
        s.step().unwrap();
    }
}

/// A bot holding only `item` against a player sidestepping at `pace`: each
/// wind-up it begins in the fight ends in a full strike within the charge
/// and hold time, unless one of them dies; at least one strike lands.
fn duel(pack: bri_weapons::Pack, item: &str, pace: f32) {
    let image = pack.images[&pack.items[item].image].clone();
    // The charge: the state whose timeout leads to one that fires on
    // release, and the state that release fires.
    let charge = image
        .states
        .iter()
        .position(|s| {
            s.timeout
                .and_then(|t| image.states.get(t))
                .is_some_and(|armed| image.fires_on_release(armed))
        })
        .expect("a wind-up weapon");
    let armed = image.states[charge].timeout.unwrap();
    let fire = image.states[armed].up.unwrap();
    let kind = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots
    .remove(0);
    let window = u64::from(image.states[charge].ticks)
        + (kind.hold_seconds * bri_weapons::TICK_HZ as f32).ceil() as u64;
    let mut s = fixture::synthetic().unwrap().session;
    s.set_weapon_pack(pack).unwrap();
    s.set_tool_catalog(ToolCatalog {
        items: [item.to_string()].into(),
        vehicles: [fixture::BOT.to_string()].into(),
        vehicle_bricks: [fixture::PLATE.to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    let stands = Vec3::new(0.0, 0.05, 16.0);
    s.set_spawn_points(vec![stands]).unwrap();
    let host = s.join("Player".into(), stands, true).unwrap();
    let mut sequence = 0;
    steps(&mut s, host, 10, &mut sequence, pace);
    let mut world = World::new("Wind-up".into(), "chaos/map".into(), vec![[1.0; 4]]);
    for (i, z) in [8.25].into_iter().enumerate() {
        let mut pad = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            [0.25, 0.1, z],
            host,
        );
        pad.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(fixture::BOT.into()),
            recolor: false,
            team: None,
        }));
        world.bricks.insert(i as u64 + 1, pad);
    }
    world.next_brick_id = 2;
    s.command(
        host,
        100,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    s.command(
        host,
        101,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout: [Some(item.into()), None, None, None, None],
                ..Default::default()
            },
        }),
    )
    .unwrap();
    steps(&mut s, host, 60, &mut sequence, pace);
    let bots: Vec<OwnerId> = s.names().keys().copied().filter(|o| s.is_bot(*o)).collect();
    assert_eq!(bots.len(), 1, "a bot on its pad");
    let name = |n: usize| image.states[n].name.as_str();
    let mut last = vec![String::new(); 1];
    let mut began: Vec<Option<u64>> = vec![None; 1];
    let mut strikes = 0;
    let mut broken = Vec::new();
    for tick in 0..120 * 20u64 {
        steps(&mut s, host, 1, &mut sequence, pace);
        let vitals = s.vitals();
        let alive = |o: &OwnerId| vitals.get(o).is_some_and(|v| v.alive);
        let fighting: Vec<OwnerId> = s
            .bot_thoughts()
            .iter()
            .filter(|t| t.acted.contains("trigger fight"))
            .map(|t| t.bot)
            .collect();
        for (n, bot) in bots.iter().enumerate() {
            let state = s
                .weapon_view()
                .images
                .get(bot)
                .and_then(|i| i.iter().find(|i| i.hand == 0 && i.image == image.id))
                .map(|i| i.state.clone())
                .unwrap_or_default();
            if state == last[n] {
                continue;
            }
            if let Some(start) = began[n] {
                if state == name(fire) {
                    strikes += 1;
                    if tick > start + window {
                        broken.push(format!(
                            "bot {bot}: struck {} ticks into a wind-up",
                            tick - start
                        ));
                    }
                    began[n] = None;
                } else if state != name(armed) {
                    if alive(bot) && alive(&host) {
                        broken.push(format!(
                            "bot {bot}: a wind-up begun at tick {start} ended in {state:?} at {tick}"
                        ));
                    }
                    began[n] = None;
                }
            }
            // Only the wind-ups it begins fighting: an idle click lets go
            // early, as a player's does.
            if state == name(charge) && fighting.contains(bot) {
                began[n] = Some(tick);
            }
            last[n] = state;
        }
    }
    assert!(broken.is_empty(), "pace {pace}: {broken:#?}");
    assert!(strikes > 0, "pace {pace}: no full strike landed");
}

#[test]
fn a_bot_finishes_its_wind_ups_against_a_sidestepping_enemy() {
    for pace in [0.25, 0.5, 1.0] {
        duel(knife(), KNIFE, pace);
    }
}

/// The same with the Butterfly Knife Add-On as imported.
#[test]
#[ignore = "requires generated content with the bundled Add-Ons; set BRI_CONTENT"]
fn a_bot_finishes_the_imported_butterfly_knifes_wind_ups() {
    let root = fixture::content_root().expect("set BRI_CONTENT to generated content");
    let part = bri_weapons::Pack::from_json(
        &std::fs::read(root.join("addons/weapon_butterflyknife/assets/weapons.json")).unwrap(),
    )
    .unwrap();
    let item = part.items.keys().next().unwrap().clone();
    let mut pack = bri_weapons::testing::pack();
    pack.items.extend(part.items);
    pack.images.extend(part.images);
    pack.projectiles.extend(part.projectiles);
    for pace in [0.25, 0.5, 1.0] {
        duel(pack.clone(), &item, pace);
    }
}

/// A server package whose command puts the game in one team with friendly
/// fire on.
fn one_team_rules() -> std::sync::Arc<bri_package_runtime::Catalog> {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "bri-windup-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let dir = root.join("windup-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        json!({
            "schema_version": 1, "id": "windup-test", "version": "1.0.0", "api": 1,
            "name": "Wind-up test", "license": "CC0-1.0", "capabilities": ["minigame"],
            "provides": [
                {"kind": "behaviour", "id": "windup-test:behaviour/main", "file": "behaviour.json"},
                {"kind": "script", "id": "windup-test:script/main", "file": "main.rhai"}
            ]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("behaviour.json"),
        json!({"schema_version": 1, "script": "main.rhai", "commands": [{"name": "teams", "args": []}]})
            .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("main.rhai"),
        "fn cmd_teams(p) {\n    set_teams(player(p).minigame, [#{ name: \"Friends\", color: 0 }], #{ friendly_fire: true });\n}\n",
    )
    .unwrap();
    let catalog = bri_package_runtime::Catalog::load(
        &root,
        &PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "windup-test".into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: "windup-test".into(),
                role: None,
            }],
        },
        true,
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    std::sync::Arc::new(catalog)
}

/// An idle bot fidgets with what it holds, but never at someone it could
/// hurt: a teammate beside it, with friendly fire on, keeps all its health
/// while the bot goofs with a wind-up knife (a weapon it reads only by its
/// states, which counts as one that hurts).
#[test]
fn an_idle_bot_never_clicks_a_wind_up_knife_at_a_teammate_it_could_hurt() {
    let mut s = fixture::synthetic().unwrap().session;
    s.set_weapon_pack(knife()).unwrap();
    s.set_tool_catalog(ToolCatalog {
        items: [
            KNIFE.to_string(),
            bri_weapons::testing::GUN_ITEM.to_string(),
        ]
        .into(),
        vehicles: [fixture::BOT.to_string()].into(),
        vehicle_bricks: [fixture::PLATE.to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    s.install_packages(one_team_rules(), None).unwrap();
    // A kind whose idle goofs are all the tool click, so the run sees them.
    let mut kinds = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots;
    kinds[0].surprise.strength = 1.0;
    kinds[0].surprise.flavours = bri_sim::bot_kind::FLAVOURS
        .iter()
        .map(|f| (f.to_string(), if *f == "tool" { 1.0 } else { 0.0 }))
        .collect();
    s.set_bot_kinds(kinds).unwrap();
    let stands = Vec3::new(0.0, 0.05, 10.0);
    s.set_spawn_points(vec![stands]).unwrap();
    let player = s.join("Teammate".into(), stands, true).unwrap();
    let mut sequence = 0;
    let still = 0.0;
    steps(&mut s, player, 10, &mut sequence, still);
    let mut world = World::new("Goof".into(), "chaos/map".into(), vec![[1.0; 4]]);
    let mut pad = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        [0.25, 0.1, 8.25],
        player,
    );
    pad.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(fixture::BOT.into()),
        recolor: false,
        team: None,
    }));
    world.bricks.insert(1, pad);
    world.next_brick_id = 2;
    let mut command = 100;
    let mut send = |s: &mut Session, c: Command| {
        command += 1;
        s.command(player, command, c).unwrap();
    };
    send(
        &mut s,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    );
    send(
        &mut s,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                // A second weapon, so a goof's swap can take the knife out.
                loadout: [
                    Some(bri_weapons::testing::GUN_ITEM.into()),
                    Some(KNIFE.into()),
                    None,
                    None,
                    None,
                ],
                weapon_damage: true,
                ..Default::default()
            },
        }),
    );
    send(
        &mut s,
        Command::Package(bri_sim::session::PackageCommand {
            package: "windup-test".into(),
            command: "teams".into(),
            args: vec![],
        }),
    );
    steps(&mut s, player, 60, &mut sequence, still);
    let game = s.minigame_views()[0].id;
    let team = s.minigame_views()[0].teams[0].id.0;
    let bot = *s.names().keys().find(|o| s.is_bot(**o)).unwrap();
    for target in [player, bot] {
        send(
            &mut s,
            Command::MiniGame(MiniGameRequest::SetTeam {
                game,
                target,
                team: Some(team),
            }),
        );
    }
    let mut goofed = false;
    for _ in 0..120 * 60 {
        steps(&mut s, player, 1, &mut sequence, still);
        goofed |= s
            .bot_thoughts()
            .iter()
            .any(|t| t.bot == bot && t.acted.contains("goof"));
        assert_eq!(
            s.vitals()[&player].health,
            100.0,
            "the bot's idle click hurt its teammate"
        );
    }
    assert!(goofed, "the bot never goofed beside its teammate");
}
