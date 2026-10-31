//! Original imported CTF policy, ordinary item/zone mechanics and real winners.
//! Original content stays outside Git; no pickup, zone or plan injection.
use bri_chaos::fixture;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package::setting::SettingValue;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, SettingEdit, TeamEdit};
use bri_world::{Brick, ContentRef, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;

const MAP: &str = "v20/add-ons/map_bedroom/bedroom.mis";
const CTF: &str = "gamemode_slayer_ctf-rules";
const SOURCE: &str = "gamemode_slayer_ctf:brick/brickslyrctfflagdata";
const RETURN: &str = "gamemode_slayer_ctf:brick/brickslyrctfflagreturndata";

#[test]
#[ignore = "needs imported CTF content and the private Beta16 save; set BRI_CONTENT and BRI_TOWER_SAVE"]
fn beta16_tower_flag_near_wall() {
    tower_flag(Vec3::new(121.25, 315.6, 74.25));
}

#[test]
#[ignore = "needs imported CTF content and the private Beta16 save; set BRI_CONTENT and BRI_TOWER_SAVE"]
fn beta16_tower_flag_in_room() {
    tower_flag(Vec3::new(120.0, 315.2, 68.0));
}

fn tower_flag(flag: Vec3) {
    let root = fixture::content_root().expect("set BRI_CONTENT to regenerated content");
    let behaviour: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join(format!("addons/{CTF}/behaviour.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        behaviour["bot_objectives"], true,
        "regenerate the current CTF owner policy"
    );
    let mut packages = PackageSet::load_root(&root).unwrap();
    packages.packages.retain(|p| p.role.is_some());
    for id in [
        "blockhead_bot",
        "bot_hole",
        "gamemode_slayer",
        "gamemode_slayer_ctf",
    ] {
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join(format!("addons/{id}/package.json"))).unwrap(),
        )
        .unwrap();
        packages.packages.push(PackageEntry {
            id: id.into(),
            version: manifest["version"].as_str().unwrap().into(),
            side: Side::Shared,
            dir: format!("addons/{id}"),
            role: None,
        });
    }
    bri_package::library::follow_manifest_sides(&root, &mut packages);
    bri_package::library::follow_companions(&root, &mut packages);
    let path = std::env::var("BRI_TOWER_SAVE")
        .expect("set BRI_TOWER_SAVE to the Beta16 tower native save");
    let mut world = bri_world::persistence::load_startup(std::path::Path::new(&path))
        .unwrap()
        .world;
    let initial = World::new(
        "CTF creator journey".into(),
        MAP.into(),
        world.palette.clone(),
    );
    let mut s = bri_net::dedicated::load_packages(&root, &packages, initial)
        .unwrap()
        .session;
    s.set_lan_host(true);
    let at = Vec3::new(112.0, 286.61, 93.0);
    s.set_spawn_points(vec![at]).unwrap();
    let author = s.join("CTF author".into(), at, true).unwrap();
    let mut replaced = 0;
    let mut flag_replaced = false;
    let (flag_x, flag_y, flag_z) = (flag.x, flag.y, flag.z);
    let flag_half_height = s.simulation().definitions.entries[SOURCE]
        .mesh
        .height_plates as f32
        * 0.1;
    let ids: Vec<_> = world.bricks.keys().copied().collect();
    for id in ids {
        let b = world.bricks.get_mut(&id).unwrap();
        b.owner = author;
        b.events.clear();
        if matches!(&b.definition, ContentRef::Resolved(id) if id == "v20/brick/brickspawnpointdata")
            && Vec3::from(b.position).distance(Vec3::new(120.9194, 315.6, 74.8856)) < 3.0
        {
            b.definition = ContentRef::Resolved(SOURCE.into());
            b.position[0] = flag_x;
            b.position[1] = flag_y + flag_half_height;
            b.position[2] = flag_z;
            b.color = 2;
            b.vehicle = None;
            flag_replaced = true;
        } else if matches!(&b.definition, ContentRef::Resolved(id) if id == "bot_zombie:brick/brickzombie_holespawndata")
        {
            b.definition = ContentRef::Resolved("v20/brick/brickvehiclespawndata".into());
            b.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(fixture::BOT.into()),
                recolor: false,
                team: None,
            }));
            replaced += 1;
        } else {
            b.vehicle = None;
        }
    }
    assert!(replaced > 0, "no saved zombie spawn bricks found");
    assert!(flag_replaced, "no player spawn at the reported target");
    let height = |id: &str| s.simulation().definitions.entries[id].mesh.height_plates as f32 * 0.1;
    let first_new = world.next_brick_id;
    for (offset, kind, x, y, z, color) in [
        (0, SOURCE, 112.75, 286.61, 91.75, 1),
        (1, RETURN, 110.75, 286.61, 91.75, 1),
    ] {
        let mut b = Brick::new(
            ContentRef::Resolved(kind.into()),
            [x, y + height(kind), z],
            author,
        );
        b.color = color;
        world.bricks.insert(first_new + offset, b);
    }
    world.next_brick_id = first_new + 2;
    s.command(
        author,
        100,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    let mut seq = 1u64 << 40;
    for _ in 0..1200 {
        seq += 1;
        s.movement(author, seq, MoveInput::default()).unwrap();
        s.step().unwrap();
        if !s.build_loading() && s.names().keys().any(|id| s.is_bot(*id)) {
            break;
        }
    }
    assert!(!s.build_loading(), "tower build did not finish loading");
    for (definition, count) in [(SOURCE, 2), (RETURN, 1)] {
        let placed = s
            .simulation()
            .state()
            .bricks
            .values()
            .filter(|b| matches!(&b.definition, ContentRef::Resolved(id) if id == definition))
            .count();
        assert_eq!(
            placed,
            count,
            "flag bricks {definition} not placed; unloaded={:?}",
            s.simulation().state().unloaded
        );
    }
    let bot = *s
        .names()
        .keys()
        .find(|id| s.is_bot(**id))
        .expect("actual spawn-brick bot");
    s.command(
        author,
        101,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: [None, None, None, None, None],
                player_type: "v20.player.playernojet".into(),
                ..Default::default()
            },
        }),
    )
    .unwrap();
    let game = s.minigame_views()[0].id;
    let setting = |key: &str, value| SettingEdit {
        key: key.into(),
        value: Some(value),
    };
    s.command(
        author,
        102,
        Command::MiniGame(MiniGameRequest::AddOnSettings {
            game,
            settings: vec![
                setting(
                    "gamemode_slayer-rules:mode",
                    SettingValue::Text("Slayer_CTF".into()),
                ),
                setting(
                    "gamemode_slayer-rules:pre_round_seconds",
                    SettingValue::Int(0),
                ),
                setting(
                    "gamemode_slayer_ctf-rules:flag_returns_to_win",
                    SettingValue::Int(1),
                ),
                setting(
                    "gamemode_slayer_ctf-rules:points_flag",
                    SettingValue::Int(20),
                ),
            ],
            teams: Some(vec![
                TeamEdit {
                    id: None,
                    name: "Copper".into(),
                    color: 1,
                    settings: vec![],
                },
                TeamEdit {
                    id: None,
                    name: "Indigo".into(),
                    color: 2,
                    settings: vec![],
                },
            ]),
            quiet: true,
            reset: false,
        }),
    )
    .unwrap();
    let team = s.minigame_views()[0].teams[0].id.0;
    let mut actors: Vec<_> = s
        .names()
        .keys()
        .copied()
        .filter(|id| s.is_bot(*id))
        .collect();
    actors.push(author);
    for (n, target) in actors.into_iter().enumerate() {
        s.command(
            author,
            103 + n as u64,
            Command::MiniGame(MiniGameRequest::SetTeam {
                game,
                target,
                team: Some(team),
            }),
        )
        .unwrap();
    }
    let mut carried = false;
    let mut composed = false;
    let mut trace = Vec::new();
    let mut pickup_tick = None;
    let mut finish_tick = None;
    let mut jet_used = false;
    let mut below_ticks = 0;
    let mut longest_below = 0;
    for tick in 0..120 * 180 {
        seq += 1;
        s.movement(author, seq, MoveInput::default()).unwrap();
        s.step().unwrap();
        carried |= s.weapon_view().images.get(&bot).is_some_and(|images| {
            images.iter().any(|i| {
                i.hand == 3
                    && i.image
                        .starts_with("gamemode_slayer_ctf:image/slyrctf_flagimage")
            })
        });
        if carried && pickup_tick.is_none() {
            pickup_tick = Some(tick);
        }
        jet_used |= s
            .motion_states()
            .iter()
            .any(|(state, _)| state.owner == bot && state.jetting);
        if !carried
            && s.motion_states().iter().any(|(state, _)| {
                let p = Vec3::from(state.feet);
                state.owner == bot
                    && (p.x - flag_x).abs() < 2.0
                    && (p.z - flag_z).abs() < 2.0
                    && p.y < flag_y - 3.0
            })
        {
            below_ticks += 1;
            longest_below = longest_below.max(below_ticks);
        } else {
            below_ticks = 0;
        }
        if let Some(detail) = s
            .bot_thoughts()
            .into_iter()
            .find(|t| t.bot == bot)
            .and_then(|t| t.objective_detail)
        {
            composed |=
                detail.provider == "declared package pickup/zone" && detail.route.len() == 2;
            let current = (detail.action, detail.route);
            if trace.len() < 16 && trace.last() != Some(&current) {
                trace.push(current);
            }
        }
        if s.round_results().any(|r| r.game == game) {
            finish_tick = Some(tick);
            break;
        }
    }
    eprintln!(
        "tower CTF bot {bot}: pickup tick {pickup_tick:?}, capture tick {finish_tick:?}, jet used {jet_used}, longest directly below {longest_below} ticks"
    );
    assert!(!jet_used, "tower runner used jets");
    assert!(
        pickup_tick.is_some_and(|tick| tick < 120 * 60),
        "tower flag was not picked up within 60 seconds: {pickup_tick:?}"
    );
    assert!(
        longest_below < 120 * 5,
        "runner remained directly below the flag for {longest_below} ticks"
    );
    assert!(
        carried,
        "real carried image missing: thoughts={:?},plans={trace:?},diagnostics={:?},actors={:?}",
        s.bot_thoughts(),
        s.package_diagnostics(),
        s.snapshot().players
    );
    assert!(
        composed,
        "actual shared pickup/return composition missing: {trace:?}"
    );
    assert_eq!(s.vitals()[&bot].score, 20, "native return scoring");
    assert_eq!(
        s.vitals()[&author].score,
        0,
        "author did not perform the return"
    );
    assert!(
        s.round_results()
            .any(|r| r.game == game && r.teams.contains(&bri_minigames::TeamId(team))),
        "real CTF team winner missing"
    );
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
    assert!(s.take_event_diagnostics().is_empty());
}
