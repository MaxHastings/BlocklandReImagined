//! Original imported CTF policy, ordinary item/zone mechanics and real winners.
//! Original content stays outside Git; no pickup, zone or plan injection.
use bri_chaos::fixture;
use bri_events::{Row, Slot, Target, Value};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package::setting::SettingValue;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, SettingEdit, TeamEdit};
use bri_world::{Brick, ContentRef, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;

const MAP: &str = "v20/add-ons/map_slate/slate.mis";
const CTF: &str = "gamemode_slayer_ctf-rules";
const SOURCE: &str = "gamemode_slayer_ctf:brick/brickslyrctfflagdata";
const RETURN: &str = "gamemode_slayer_ctf:brick/brickslyrctfflagreturndata";

#[test]
#[ignore = "needs freshly imported Slayer CTF desired-state policy; set BRI_CONTENT"]
fn imported_ctf_bot_takes_the_actual_flag_and_returns_it_for_its_team() {
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
    let palette: Vec<_> = (0..8)
        .map(|i| [i as f32 / 7.0, 0.3, 1.0 - i as f32 / 7.0, 1.0])
        .collect();
    let initial = World::new("CTF creator journey".into(), MAP.into(), palette.clone());
    let mut s = bri_net::dedicated::load_packages(&root, &packages, initial)
        .unwrap()
        .session;
    s.set_lan_host(true);
    let at = Vec3::new(-80.0, 0.05, 80.0);
    s.set_spawn_points(vec![at]).unwrap();
    let author = s.join("CTF author".into(), at, true).unwrap();
    let mut world = World::new("Two-color flag return".into(), MAP.into(), palette);
    let mut hole = Brick::new(
        ContentRef::Resolved("v20/brick/brickvehiclespawndata".into()),
        [0.0, 0.1, 20.0],
        author,
    );
    hole.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(fixture::BOT.into()),
        recolor: false,
        team: None,
    }));
    world.bricks.insert(1, hole);
    let height = |id: &str| s.simulation().definitions.entries[id].mesh.height_plates as f32 * 0.1;
    for (id, kind, x, z, color) in [
        (2, SOURCE, 6.0, 20.0, 2),
        (3, SOURCE, -6.0, 20.0, 1),
        (4, RETURN, -8.0, 22.0, 1),
    ] {
        let mut b = Brick::new(
            ContentRef::Resolved(kind.into()),
            [x, height(kind), z],
            author,
        );
        b.color = color;
        if id == 2 {
            b.events.push(Row {
                enabled: true,
                input: "onFlagReturned".into(),
                output: "setColorFX".into(),
                target: Target::Slot(Slot::SelfBrick),
                params: vec![Value::Int(1)],
                conditions: vec![],
                delay_ms: 0,
                preserved: None,
            });
        }
        world.bricks.insert(id, b);
    }
    world.next_brick_id = 5;
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
    for _ in 0..20 {
        seq += 1;
        s.movement(author, seq, MoveInput::default()).unwrap();
        s.step().unwrap();
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
    for (sequence, target) in [(103, author), (104, bot)] {
        s.command(
            author,
            sequence,
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
    for _ in 0..120 * 35 {
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
            break;
        }
    }
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
    assert_eq!(
        s.simulation().state().bricks[&2].color_effect,
        1,
        "real owner-policy onFlagReturned input"
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
