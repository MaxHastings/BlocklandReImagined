//! Blockhead Bots on the content-free map: they find their way round builds,
//! fight inside their builder's mini-game, keep to their side and exist only
//! while an Add-On provides their kind. Every run is the same run: bots draw
//! from seeded generators and the walk grid is searched in a fixed order.
use bri_admin::{Action, Request};
use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;

fn session() -> Session {
    let mut s = fixture::synthetic().unwrap().session;
    s.set_tool_catalog(ToolCatalog {
        vehicles: [fixture::BOT, HORSE, CANNON, JEEP].map(String::from).into(),
        vehicle_bricks: [fixture::PLATE.to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    s
}

/// A 1x1 plate spawning a bot, centred in the stud cell at `at`.
fn bot_brick(at: [f32; 3], owner: OwnerId) -> Brick {
    spawn_brick(fixture::BOT, at, owner)
}

/// A 1x1 plate spawning `kind`, a bot or a vehicle.
fn spawn_brick(kind: &str, at: [f32; 3], owner: OwnerId) -> Brick {
    let at = [at[0] + 0.25, at[1], at[2] + 0.25];
    let mut brick = Brick::new(ContentRef::Resolved(fixture::PLATE.into()), at, owner);
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(kind.into()),
        recolor: false,
    }));
    brick
}

/// A see-through wall of 3-unit columns along x = `x` from `z0` to `z1`:
/// bots see through it (raycasting off) but cannot walk through or jump it.
fn glass_wall(x: f32, z0: f32, z1: f32, owner: OwnerId) -> Vec<Brick> {
    let mut out = Vec::new();
    let mut z = z0;
    while z <= z1 {
        let mut column = Brick::new(
            ContentRef::Resolved(fixture::TALL.into()),
            [x + 0.25, 1.5, z + 0.25],
            owner,
        );
        column.raycast = false;
        out.push(column);
        z += 0.5;
    }
    out
}

fn load(s: &mut Session, owner: OwnerId, bricks: Vec<Brick>) {
    let mut world = World::new("Bots".into(), "chaos/map".into(), vec![[1.0; 4]]);
    for (i, brick) in bricks.into_iter().enumerate() {
        world.bricks.insert(i as u64 + 1, brick);
    }
    world.next_brick_id = world.bricks.len() as u64 + 1;
    let count = world.bricks.len();
    s.command(
        owner,
        100,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    let mut sequence = 1 << 40;
    steps(s, &[owner], 5, &mut sequence);
    assert_eq!(
        s.simulation().state().bricks.len(),
        count,
        "every brick placed"
    );
}

fn minigame(s: &mut Session, owner: OwnerId, loadout: [Option<String>; 5]) {
    s.command(
        owner,
        101,
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

/// Step the world, humans standing still.
fn steps(s: &mut Session, humans: &[OwnerId], ticks: usize, sequence: &mut u64) {
    for _ in 0..ticks {
        *sequence += 1;
        for human in humans {
            s.movement(*human, *sequence, MoveInput::default()).unwrap();
        }
        s.step().unwrap();
    }
}

fn feet(s: &Session, owner: OwnerId) -> Vec3 {
    Vec3::from(
        s.snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == owner)
            .expect("player in the snapshot")
            .feet,
    )
}

fn bots(s: &Session) -> Vec<OwnerId> {
    s.names().keys().copied().filter(|o| s.is_bot(*o)).collect()
}

const HORSE: &str = bri_vehicles::testing::HORSE;
const CANNON: &str = bri_vehicles::testing::CANNON;
const JEEP: &str = bri_vehicles::testing::CAR;
const TOOLS_ONLY: [Option<String>; 5] = [None, None, None, None, None];

#[test]
fn a_bot_walks_round_a_wall_it_sees_through_to_reach_its_enemy() {
    let mut s = session();
    let human = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    // The bot starts east of a wall it cannot cross; the builder waits west
    // of it. Unarmed, the bot closes in: the only way is round an end.
    let mut bricks = vec![bot_brick([16.0, 0.1, 30.0], human)];
    bricks.extend(glass_wall(8.0, 22.0, 38.0, human));
    load(&mut s, human, bricks);
    minigame(&mut s, human, TOOLS_ONLY);
    steps(&mut s, &[human], 60, &mut sequence);
    let bot = bots(&s)[0];
    let mut crossed_at = None;
    let mut last = feet(&s, bot);
    for tick in 0..120 * 25 {
        steps(&mut s, &[human], 1, &mut sequence);
        let now = feet(&s, bot);
        if last.x >= 8.0 && now.x < 8.0 {
            crossed_at = Some(now.z);
        }
        last = now;
        if (now - feet(&s, human)).length() < 4.0 && crossed_at.is_some() {
            eprintln!("reached the builder after {tick} ticks");
            break;
        }
    }
    let crossed = crossed_at.expect("the bot got past the wall");
    assert!(
        !(21.5..=38.5).contains(&crossed),
        "went round an end of the wall, not through it: crossed at z {crossed}"
    );
    assert!(
        (feet(&s, bot) - feet(&s, human)).length() < 4.0,
        "reached the builder: bot {} builder {}",
        feet(&s, bot),
        feet(&s, human)
    );
}

#[test]
fn an_armed_bot_takes_a_moment_to_react_then_hits_its_enemy() {
    let mut s = session();
    let human = s
        .join("Builder".into(), Vec3::new(-20.0, 0.05, 40.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    load(&mut s, human, vec![bot_brick([-20.0, 0.1, 20.0], human)]);
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
    );
    steps(&mut s, &[human], 60, &mut sequence);
    // Spawn protection lasts 2.5 s; the bot needs its reaction time too.
    let mut hit_after = None;
    for tick in 0..120 * 20 {
        steps(&mut s, &[human], 1, &mut sequence);
        if s.vitals()[&human].health < 100.0 {
            hit_after = Some(tick);
            break;
        }
    }
    let hit_after = hit_after.expect("the bot hit the builder");
    eprintln!("first hit after {hit_after} ticks");
    // Same world, same inputs: the same fight, tick for tick.
    let mut again = session();
    let human2 = again
        .join("Builder".into(), Vec3::new(-20.0, 0.05, 40.0), true)
        .unwrap();
    let mut sequence2 = 0;
    steps(&mut again, &[human2], 10, &mut sequence2);
    load(
        &mut again,
        human2,
        vec![bot_brick([-20.0, 0.1, 20.0], human2)],
    );
    minigame(
        &mut again,
        human2,
        [
            Some(bri_weapons::testing::GUN_ITEM.into()),
            None,
            None,
            None,
            None,
        ],
    );
    steps(&mut again, &[human2], 60, &mut sequence2);
    steps(&mut again, &[human2], hit_after, &mut sequence2);
    assert_eq!(again.vitals()[&human2].health, 100.0, "no earlier hit");
    steps(&mut again, &[human2], 1, &mut sequence2);
    assert!(
        again.vitals()[&human2].health < 100.0,
        "hit on the same tick"
    );
}

#[test]
fn bots_of_one_builder_are_on_one_side() {
    let mut s = session();
    // The builder stands far out of sight; the two bots only see each other.
    let human = s
        .join("Builder".into(), Vec3::new(90.0, 0.05, 90.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    load(
        &mut s,
        human,
        vec![
            bot_brick([-40.0, 0.1, 40.0], human),
            bot_brick([-40.0, 0.1, 50.0], human),
        ],
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
    );
    steps(&mut s, &[human], 120 * 12, &mut sequence);
    let bots = bots(&s);
    assert_eq!(bots.len(), 2);
    let vitals = s.vitals();
    for bot in bots {
        assert_eq!(
            vitals[&bot].health, 100.0,
            "bot {bot} was not attacked by its ally"
        );
    }
}

#[test]
fn without_a_bot_add_on_a_spawn_brick_makes_no_bot() {
    let mut s = session();
    s.set_bot_kinds(Vec::new()).unwrap();
    assert!(s.bot_choices().is_empty());
    let human = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    load(&mut s, human, vec![bot_brick([6.0, 0.1, 30.0], human)]);
    steps(&mut s, &[human], 60, &mut sequence);
    assert!(bots(&s).is_empty());
    // Turning the kind back on brings the bot.
    s.set_bot_kinds(
        bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots,
    )
    .unwrap();
    steps(&mut s, &[human], 60, &mut sequence);
    assert_eq!(bots(&s).len(), 1);
}

/// v20's `ServerCmdClearBots` deletes every player object no client
/// controls: bots, and the horses, boats, cannons and turrets spawn bricks
/// make, since those are players there. A mount someone rides stays, and
/// physics vehicles are `/clearVehicles`' business.
#[test]
fn clear_bots_also_clears_the_mounts_nobody_rides() {
    let mut s = session();
    let admin = s
        .join("Admin".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[admin], 10, &mut sequence);
    load(
        &mut s,
        admin,
        vec![
            spawn_brick(HORSE, [0.0, 0.1, 25.0], admin),
            spawn_brick(HORSE, [20.0, 0.1, 30.0], admin),
            spawn_brick(CANNON, [-20.0, 0.1, 30.0], admin),
            spawn_brick(JEEP, [40.0, 0.1, 30.0], admin),
            bot_brick([-40.0, 0.1, 30.0], admin),
        ],
    );
    steps(&mut s, &[admin], 120, &mut sequence);
    let kinds = |s: &Session| {
        let mut out: Vec<String> = s
            .vehicle_infos()
            .into_iter()
            .map(|v| v.definition)
            .collect();
        out.sort();
        out
    };
    assert_eq!(kinds(&s), [CANNON, HORSE, HORSE, JEEP]);
    assert_eq!(bots(&s).len(), 1);
    // A rider drops onto the near horse from above and takes its seat.
    let rider = s
        .join("Rider".into(), Vec3::new(0.25, 4.0, 25.25), true)
        .unwrap();
    for _ in 0..240 {
        if s.mounted(rider).is_some() {
            break;
        }
        steps(&mut s, &[admin, rider], 1, &mut sequence);
    }
    let (ridden, _) = s.mounted(rider).expect("rode the near horse");
    s.take_notices();
    s.command(
        admin,
        sequence + 1,
        Command::Admin(Request::new(Action::ClearBots)),
    )
    .unwrap();
    sequence += 1;
    steps(&mut s, &[admin, rider], 5, &mut sequence);
    assert!(bots(&s).is_empty(), "the bot went");
    assert_eq!(kinds(&s), [HORSE, JEEP], "the idle horse and cannon went");
    assert_eq!(s.mounted(rider).map(|m| m.0), Some(ridden), "still riding");
    assert!(
        s.chat()
            .iter()
            .any(|line| line.text.ends_with("cleared all bots (3).")),
        "{:?}",
        s.chat().iter().map(|l| &l.text).collect::<Vec<_>>()
    );
}

/// A bot the server has no room for is never silent: its builder is told.
#[test]
fn a_bot_over_the_server_limit_tells_its_builder() {
    let mut s = session();
    let builder = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 60.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[builder], 10, &mut sequence);
    let bricks = (0..17)
        .map(|i| bot_brick([-40.0 + 5.0 * i as f32, 0.1, 20.0], builder))
        .collect();
    load(&mut s, builder, bricks);
    steps(&mut s, &[builder], 30, &mut sequence);
    assert_eq!(bots(&s).len(), 16);
    let told: Vec<String> = s
        .take_private_notices()
        .into_iter()
        .filter_map(|(to, n)| match n {
            bri_sim::session::Notice::Center { text, .. } if to == builder => Some(text),
            _ => None,
        })
        .collect();
    assert!(
        told.iter()
            .any(|t| t.ends_with("Server is limited to 16 bots")),
        "{told:?}"
    );
}
