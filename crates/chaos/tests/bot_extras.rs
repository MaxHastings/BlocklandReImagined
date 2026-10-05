//! The bots' small extra options (`session::bots::extras`: idle play,
//! crouching under fire, dodge hops, clicking bricks, handing a spare
//! weapon) and their looks and names (`looks`), through the authoritative
//! session with invented content. No test moves a bot or presses its
//! controls: the ordinary brain chooses, and each property is compared
//! against the same scene with that option weighed 0 or its cause absent.
use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::bot_kind::{BotKind, BotPack};
use bri_sim::player::MoveInput;
use bri_sim::session::{ActionAim, Command, MiniGameRequest, Session, ToolCatalog, ToolInventory};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;

const TOOLS_ONLY: [Option<String>; 5] = [None, None, None, None, None];

/// The Blockhead Bot's kind, changed by `change`, as the only kind.
fn blockhead(change: impl FnOnce(&mut BotKind)) -> Vec<BotKind> {
    let mut kinds = BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots;
    change(&mut kinds[0]);
    kinds
}

fn catalog() -> ToolCatalog {
    ToolCatalog {
        vehicles: [fixture::BOT].map(String::from).into(),
        vehicle_bricks: [fixture::PLATE.to_string()].into(),
        ..Default::default()
    }
}

/// The synthetic session with its gun's shots changed by `gun`.
fn session(gun: impl FnOnce(&mut bri_weapons::ProjectileDef)) -> Session {
    let mut s = fixture::synthetic().unwrap().session;
    let (mut weapons, _) = fixture::synthetic_weapons().unwrap();
    gun(weapons
        .projectiles
        .get_mut(bri_weapons::testing::GUN_PROJECTILE)
        .unwrap());
    s.set_weapon_pack(weapons).unwrap();
    s.set_tool_catalog(catalog()).unwrap();
    s
}

fn bot_brick(at: [f32; 3], owner: OwnerId) -> Brick {
    let mut brick = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        [at[0] + 0.25, at[1], at[2] + 0.25],
        owner,
    );
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(fixture::BOT.into()),
        recolor: false,
        team: None,
    }));
    brick
}

fn load(s: &mut Session, owner: OwnerId, bricks: Vec<Brick>, sequence: &mut u64) {
    let mut world = World::new("Extras".into(), "chaos/map".into(), vec![[1.0; 4]]);
    for (i, brick) in bricks.into_iter().enumerate() {
        world.bricks.insert(i as u64 + 1, brick);
    }
    world.next_brick_id = world.bricks.len() as u64 + 1;
    let count = world.bricks.len();
    *sequence += 1;
    s.command(
        owner,
        *sequence,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    steps(s, &[owner], 5, sequence);
    assert_eq!(
        s.simulation().state().bricks.len(),
        count,
        "every brick placed"
    );
}

fn minigame(s: &mut Session, owner: OwnerId, loadout: [Option<String>; 5], sequence: &mut u64) {
    *sequence += 1;
    s.command(
        owner,
        *sequence,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout,
                ..Default::default()
            },
        }),
    )
    .unwrap();
}

fn steps(s: &mut Session, humans: &[OwnerId], ticks: usize, sequence: &mut u64) {
    for _ in 0..ticks {
        *sequence += 1;
        for human in humans {
            s.movement(*human, *sequence, MoveInput::default()).unwrap();
        }
        s.step().unwrap();
    }
}

fn state(s: &Session, owner: OwnerId) -> bri_sim::player::PlayerState {
    s.snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .expect("player in the snapshot")
}

fn feet(s: &Session, owner: OwnerId) -> Vec3 {
    Vec3::from(state(s, owner).feet)
}

fn bots(s: &Session) -> Vec<OwnerId> {
    s.names().keys().copied().filter(|o| s.is_bot(*o)).collect()
}

fn flat(v: Vec3) -> f32 {
    Vec3::new(v.x, 0.0, v.z).length()
}

/// How far a loose ball near two allied bots moves, with no enemy about.
fn idle_push(idle_play: Option<f32>) -> f32 {
    let mut s = session(|_| {});
    let (mut pack, _) = fixture::synthetic_vehicles().unwrap();
    for d in &mut pack.definitions {
        if d.id == bri_vehicles::testing::BALL {
            d.shove = true;
        }
    }
    s.set_vehicle_pack(
        pack,
        blockhead(|k| {
            k.wander_radius = 2.0;
            if let Some(w) = idle_play {
                k.extras.insert("idle_play".into(), w);
            }
        }),
    )
    .unwrap();
    s.set_tool_catalog(catalog()).unwrap();
    // The builder plays far out of sight: the two bots are allies with no
    // enemy about, in a game whose rules let them move bodies.
    let human = s
        .join("Builder".into(), Vec3::new(90.0, 0.05, 90.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    load(
        &mut s,
        human,
        vec![
            bot_brick([-24.0, 0.1, 14.0], human),
            bot_brick([-34.0, 0.1, 26.0], human),
        ],
        &mut sequence,
    );
    s.set_spawn_points(vec![Vec3::new(90.0, 0.05, 90.0)]).unwrap();
    sequence += 1;
    s.command(
        human,
        sequence,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout: TOOLS_ONLY,
                vehicle_damage: true,
                ..Default::default()
            },
        }),
    )
    .unwrap();
    steps(&mut s, &[human], 60, &mut sequence);
    let ball = s
        .spawn_vehicle_at(
            human,
            bri_vehicles::testing::BALL,
            Vec3::new(-30.0, 2.1, 18.0),
            0.0,
            Vec3::ZERO,
        )
        .unwrap();
    let at = |s: &Session| -> Vec3 {
        s.vehicle_poses()
            .into_iter()
            .find(|v| v.id == ball)
            .unwrap()
            .position
            .into()
    };
    steps(&mut s, &[human], 120, &mut sequence);
    let before = at(&s);
    for i in 0..120 * 20 {
        steps(&mut s, &[human], 1, &mut sequence);
        if std::env::var_os("BRI_DEBUG_EXTRAS").is_some() && i % 240 == 0 {
            let b = bots(&s);
            let t = s.bot_thoughts();
            eprintln!(
                "{idle_play:?} ball {:?} bots {:?} {:?} {:?}",
                at(&s),
                feet(&s, b[0]),
                feet(&s, b[1]),
                t.iter().map(|t| (t.behaviour, format!("{:?}", t.task))).collect::<Vec<_>>()
            );
        }
    }
    flat(at(&s) - before)
}

#[test]
fn an_idle_bot_near_a_loose_ball_pushes_it() {
    let played = idle_push(None);
    let off = idle_push(Some(0.0));
    assert!(off < 0.3, "without idle play it stays put: {off}");
    assert!(
        played > off + 1.0,
        "the idle bots pushed the ball along: {played} against {off} without idle play"
    );
}

/// A bot fighting a builder 14 units off. The builder fires every half
/// second when `fire` is set, `wide` radians off the bot when not 0.
/// Returns (ticks crouched, ticks off the ground, tick count).
fn duel(
    change: impl FnOnce(&mut BotKind),
    gun: impl FnOnce(&mut bri_weapons::ProjectileDef),
    fire: bool,
    wide: f32,
) -> (usize, usize) {
    let mut s = session(gun);
    s.set_bot_kinds(blockhead(change)).unwrap();
    let human = s
        .join("Builder".into(), Vec3::new(-20.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    load(
        &mut s,
        human,
        vec![bot_brick([-20.0, 0.1, 16.0], human)],
        &mut sequence,
    );
    minigame(
        &mut s,
        human,
        [
            Some(bri_weapons::testing::GUN_ITEM.into()),
            None,
            None,
            None,
            None,
        ],
        &mut sequence,
    );
    // Past spawn protection.
    steps(&mut s, &[human], 120 * 4, &mut sequence);
    let bot = bots(&s)[0];
    sequence += 1;
    s.command(human, sequence, Command::EquipTool { slot: Some(0) })
        .unwrap();
    let (mut crouched, mut airborne) = (0, 0);
    for tick in 0..120 * 15 {
        if fire && tick % 60 == 0 {
            let ray = (feet(&s, bot) + Vec3::Y - (feet(&s, human) + Vec3::Y * 2.1)).normalize();
            sequence += 1;
            s.command_with_aim(
                human,
                sequence,
                Command::WeaponTrigger { down: true },
                Some(ActionAim {
                    yaw: (ray.x.atan2(-ray.z) + wide + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI,
                    pitch: ray.y.asin(),
                }),
            )
            .unwrap();
        } else if fire && tick % 60 == 2 {
            sequence += 1;
            s.command(human, sequence, Command::WeaponTrigger { down: false })
                .unwrap();
        }
        steps(&mut s, &[human], 1, &mut sequence);
        let p = state(&s, bot);
        crouched += usize::from(p.crouched);
        airborne += usize::from(!p.grounded);
    }
    (crouched, airborne)
}

#[test]
fn a_bot_under_ranged_fire_crouches_more_than_one_not_under_fire() {
    // Hits that hurt without killing; dodging is left out of this one.
    let weak = |p: &mut bri_weapons::ProjectileDef| p.damage = 1.0;
    let no_dodge = |k: &mut BotKind| {
        k.extras.insert("dodge".into(), 0.0);
    };
    let (under_fire, _) = duel(no_dodge, weak, true, 0.0);
    let (quiet, _) = duel(no_dodge, weak, false, 0.0);
    let (off, _) = duel(
        |k| {
            k.extras.insert("dodge".into(), 0.0);
            k.extras.insert("crouch".into(), 0.0);
        },
        weak,
        true,
        0.0,
    );
    assert!(
        under_fire > quiet + 120,
        "crouched {under_fire} ticks under fire, {quiet} without"
    );
    assert!(
        under_fire > off + 120,
        "crouched {under_fire} ticks under fire, {off} with crouching off"
    );
}

#[test]
fn a_predicted_hit_triggers_a_hop_more_often_than_a_shot_that_misses() {
    // Slow, harmless-enough shots, so the path is seen in flight.
    let slow = |p: &mut bri_weapons::ProjectileDef| {
        p.damage = 1.0;
        p.speed = 20.0;
    };
    let no_crouch = |k: &mut BotKind| {
        k.extras.insert("crouch".into(), 0.0);
    };
    let (_, at_it) = duel(no_crouch, slow, true, 0.0);
    let (_, wide) = duel(no_crouch, slow, true, 0.6);
    let (_, off) = duel(
        |k| {
            k.extras.insert("crouch".into(), 0.0);
            k.extras.insert("dodge".into(), 0.0);
        },
        slow,
        true,
        0.0,
    );
    assert!(
        at_it > wide + 60,
        "off the ground {at_it} ticks when shot at, {wide} when shots go wide"
    );
    assert!(
        at_it > off + 60,
        "off the ground {at_it} ticks when shot at, {off} with dodging off"
    );
}

const DOOR: &str = "test/brick/door";
const DOOR_OPEN: &str = "test/brick/door-open";

/// A glass room round a bot, its one way out a door a click opens; the
/// builder stands outside in sight. Returns whether the door opened and
/// whether the bot got out.
fn door_room(activate: Option<f32>) -> (bool, bool) {
    use bri_content::brick::Swap;
    use bri_sim::testing as t;
    let mut definitions = t::definitions();
    let door = t::definition(DOOR, [1, 4], 15, bri_sim::definitions::Special::None, false);
    let open = t::definition(
        DOOR_OPEN,
        [1, 4],
        1,
        bri_sim::definitions::Special::None,
        false,
    );
    definitions.entries.insert(DOOR.into(), door);
    definitions.entries.insert(DOOR_OPEN.into(), open);
    let world = World::new("Door".into(), "chaos/map".into(), vec![[1.0; 4]]);
    let colliders = vec![
        rapier3d::prelude::ColliderBuilder::cuboid(100.0, 0.5, 100.0)
            .translation(rapier3d::prelude::Vector::new(0.0, -0.5, 0.0)),
    ];
    let mut s =
        Session::new(bri_sim::simulation::Simulation::new(world, definitions, colliders).unwrap());
    s.set_spawn_points(vec![Vec3::new(8.0, 0.05, 0.0)]).unwrap();
    let (weapons, _) = fixture::synthetic_weapons().unwrap();
    s.set_weapon_pack(weapons).unwrap();
    let (vehicles, _) = fixture::synthetic_vehicles().unwrap();
    s.set_vehicle_pack(
        vehicles,
        blockhead(|k| {
            if let Some(w) = activate {
                k.extras.insert("activate".into(), w);
            }
        }),
    )
    .unwrap();
    let mut tools = catalog();
    let swap = Swap {
        front: DOOR_OPEN.into(),
        back: DOOR_OPEN.into(),
    };
    tools.swaps.insert(DOOR.into(), swap);
    s.set_tool_catalog(tools).unwrap();
    let human = s
        .join("Builder".into(), Vec3::new(8.0, 0.05, 0.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    let glass = |x: f32, z: f32| {
        let mut b = Brick::new(ContentRef::Resolved(t::TALL.into()), [x, 1.5, z], human);
        b.raycast = false;
        b
    };
    let mut bricks = vec![bot_brick([-5.0, 0.1, 0.0], human)];
    for i in 0..14 {
        let z = -3.25 + i as f32 * 0.5;
        bricks.push(glass(-8.25, z));
        if z.abs() > 1.0 {
            bricks.push(glass(-1.75, z));
        }
    }
    for i in 1..12 {
        let x = -8.25 + i as f32 * 0.5;
        bricks.push(glass(x, -3.75));
        bricks.push(glass(x, 3.75));
    }
    bricks.push(Brick::new(
        ContentRef::Resolved(DOOR.into()),
        [-1.75, 1.5, 0.0],
        human,
    ));
    let door_id = bricks.len() as u64;
    load(&mut s, human, bricks, &mut sequence);
    minigame(&mut s, human, TOOLS_ONLY, &mut sequence);
    let mut out = false;
    for _ in 0..120 * 25 {
        steps(&mut s, &[human], 1, &mut sequence);
        if let Some(bot) = bots(&s).first() {
            out |= feet(&s, *bot).x > -1.0;
        }
    }
    let opened = matches!(
        &s.simulation().state().bricks[&door_id].definition,
        ContentRef::Resolved(id) if id == DOOR_OPEN
    );
    (opened, out)
}

#[test]
fn an_activatable_door_on_the_route_gets_activated() {
    let (opened, out) = door_room(None);
    assert!(opened, "the bot clicked the door in its way open");
    assert!(out, "and went through after its enemy");
    let (opened, out) = door_room(Some(0.0));
    assert!(!opened && !out, "with activation off it stays shut in");
}

/// Two bots of one builder on one side, no enemy about: one with a gun
/// and a launcher, the other empty-handed. Whether the second ends up
/// armed.
fn hand_over(hand_weapon: Option<f32>) -> bool {
    let mut s = session(|_| {});
    s.set_bot_kinds(blockhead(|k| {
        k.wander_radius = 3.0;
        if let Some(w) = hand_weapon {
            k.extras.insert("hand_weapon".into(), w);
        }
    }))
    .unwrap();
    let human = s
        .join("Builder".into(), Vec3::new(40.0, 0.05, 40.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    load(
        &mut s,
        human,
        vec![
            bot_brick([-24.0, 0.1, 20.0], human),
            bot_brick([-24.0, 0.1, 28.0], human),
        ],
        &mut sequence,
    );
    // In a game that hands out nothing, the builder far out of sight.
    s.set_spawn_points(vec![Vec3::new(90.0, 0.05, 90.0)]).unwrap();
    minigame(&mut s, human, TOOLS_ONLY, &mut sequence);
    steps(&mut s, &[human], 30, &mut sequence);
    let both = bots(&s);
    assert_eq!(both.len(), 2);
    let (giver, mate) = (both[0], both[1]);
    s.give_tool(giver, bri_weapons::testing::GUN_ITEM, true)
        .unwrap();
    s.give_tool(giver, bri_weapons::testing::ROCKET_ITEM, false)
        .unwrap();
    let armed = |s: &Session| {
        s.tool_inventories()
            .get(&mate)
            .is_some_and(|i| {
                i.slots.iter().flatten().any(|item| {
                    [
                        bri_weapons::testing::GUN_ITEM,
                        bri_weapons::testing::ROCKET_ITEM,
                    ]
                    .contains(&item.as_str())
                })
            })
    };
    for i in 0..120 * 30 {
        steps(&mut s, &[human], 1, &mut sequence);
        if std::env::var_os("BRI_DEBUG_EXTRAS").is_some() && i % 120 == 0 {
            let t = s.bot_thoughts();
            eprintln!(
                "{:?} {:?} {:?} {:?}",
                feet(&s, giver),
                feet(&s, mate),
                s.tool_inventories().get(&giver),
                t.iter().map(|t| (t.bot, t.behaviour)).collect::<Vec<_>>()
            );
        }
        if armed(&s) {
            return true;
        }
    }
    false
}

#[test]
fn a_spare_weapon_ends_up_with_an_unarmed_teammate() {
    assert!(hand_over(None), "the teammate was handed a weapon");
    assert!(
        !hand_over(Some(0.0)),
        "nobody hands one with the option off"
    );
}

fn avatar_pack() -> bri_content::avatar::Package {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1, "id": "test", "rig": "rig.json", "rig_sha256": "",
        "parts": {"hat": ["none"], "accent": ["none"], "pack": ["none"],
            "secondpack": ["none"], "chest": ["chest"], "hip": ["pants"],
            "rarm": ["rarm"], "larm": ["larm"], "rhand": ["rhand"],
            "lhand": ["lhand"], "rleg": ["rshoe"], "lleg": ["lshoe"]},
        "accents_allowed": {}, "faces": ["smiley", "grin", "frown"],
        "decals": ["AAA-None", "Alyx", "Stripes"], "surfaces": {}, "textures": {
            "smiley": {"file": "smiley.png", "sha256": "", "source": "", "width": 1, "height": 1},
            "grin": {"file": "grin.png", "sha256": "", "source": "", "width": 1, "height": 1},
            "frown": {"file": "frown.png", "sha256": "", "source": "", "width": 1, "height": 1},
            "Alyx": {"file": "alyx.png", "sha256": "", "source": "", "width": 1, "height": 1},
            "Stripes": {"file": "stripes.png", "sha256": "", "source": "", "width": 1, "height": 1},
            "AAA-None": {"file": "none.png", "sha256": "", "source": "", "width": 1, "height": 1}},
        "defaults": {"parts": {}, "colors": {"head": [1.0, 0.88, 0.61, 1.0],
            "torso": [0.9, 0.9, 0.9, 1.0], "hat": [1.0, 1.0, 0.0, 1.0],
            "accent": [0.0, 0.2, 0.64, 0.7], "pack": [0.0, 0.4, 0.8, 1.0],
            "secondpack": [0.0, 1.0, 0.0, 1.0], "hip": [0.0, 0.0, 1.0, 1.0],
            "rarm": [0.9, 0.0, 0.0, 1.0], "larm": [0.9, 0.0, 0.0, 1.0],
            "rhand": [1.0, 0.88, 0.61, 1.0], "lhand": [1.0, 0.88, 0.61, 1.0],
            "rleg": [0.0, 0.0, 1.0, 1.0], "lleg": [0.0, 0.0, 1.0, 1.0]}, "face": "smiley", "decal": "Alyx"}
    }))
    .unwrap()
}

/// Eight brick bots of one kind: their looks and their names.
fn crowd() -> (Session, Vec<OwnerId>) {
    let mut s = session(|_| {});
    s.set_avatar_catalog(avatar_pack()).unwrap();
    let human = s
        .join("Builder".into(), Vec3::new(40.0, 0.05, 40.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    let bricks = (0..8)
        .map(|i| bot_brick([-30.0 + i as f32 * 3.0, 0.1, 24.0], human))
        .collect();
    load(&mut s, human, bricks, &mut sequence);
    steps(&mut s, &[human], 30, &mut sequence);
    let bots = bots(&s);
    assert_eq!(bots.len(), 8);
    (s, bots)
}

#[test]
fn bots_of_one_kind_get_varied_seeded_looks() {
    let (s, bots) = crowd();
    let avatars = s.avatars();
    let looks: std::collections::BTreeSet<String> =
        bots.iter().map(|b| format!("{:?}", avatars[b])).collect();
    assert!(
        looks.len() >= bots.len() / 2,
        "{} distinct looks among {} bots",
        looks.len(),
        bots.len()
    );
    let pack = avatar_pack();
    for b in &bots {
        let a = &avatars[b];
        assert!(pack.faces.contains(&a.face) && pack.decals.contains(&a.decal));
        assert_eq!(a.colors["head"], pack.defaults.colors["head"], "skin kept");
    }
    // The same build again: the same looks.
    let (again, bots_again) = crowd();
    let avatars_again = again.avatars();
    for (a, b) in bots.iter().zip(&bots_again) {
        assert_eq!(avatars[a], avatars_again[b]);
    }
}

#[test]
fn brick_bot_names_come_from_first_names_with_no_duplicates() {
    let (s, bots) = crowd();
    let first_names = &blockhead(|_| {})[0].first_names;
    let names = s.names();
    let mut seen = std::collections::BTreeSet::new();
    for b in &bots {
        assert!(
            first_names.contains(&names[b]),
            "{} is one of the kind's first names",
            names[b]
        );
        assert!(seen.insert(names[b].to_lowercase()), "{} twice", names[b]);
    }
}
