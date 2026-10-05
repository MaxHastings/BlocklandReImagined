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
        team: None,
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

/// One builder's bots side with each other only outside a mini-game. This
/// mini-game has no teams, so (audit T1, a Deathmatch of brick bots) every
/// player in it, one builder's bots included, is everyone's enemy.
#[test]
fn bots_of_one_builder_fight_in_a_mini_game_without_teams() {
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
    assert!(
        bots.iter()
            .any(|bot| !vitals[bot].alive || vitals[bot].health < 100.0),
        "one builder's bots fought: {:?}",
        bots.iter().map(|b| vitals[b].health).collect::<Vec<_>>()
    );
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

/// A closed room of 6-unit-high opaque columns round x 26..34, z 26..34
/// with a bot's spawn brick inside, a portal inside it facing the bot and
/// its partner out in the open to the north. Returns the bricks and where
/// to stand so the bot sees you through the portal, `beyond` units past it.
fn portal_room(owner: OwnerId, beyond: f32) -> (Vec<Brick>, Vec3) {
    let mut bricks = vec![bot_brick([30.0, 0.1, 31.0], owner)];
    let mut column = |x: f32, z: f32| {
        for y in [1.5, 4.5] {
            bricks.push(Brick::new(
                ContentRef::Resolved(fixture::TALL.into()),
                [x + 0.25, y, z + 0.25],
                owner,
            ));
        }
    };
    for i in 0..16 {
        let at = 26.0 + i as f32 * 0.5;
        column(at, 26.0);
        column(at + 0.5, 34.0);
        column(26.0, at + 0.5);
        column(34.0, at);
    }
    let portal = |position: [f32; 3]| {
        let mut b = Brick::new(ContentRef::Resolved(PORTAL.into()), position, owner);
        b.name = Some("Portal_a".into());
        b
    };
    bricks.push(portal([30.0, 1.5, 28.25]));
    bricks.push(portal([30.0, 1.5, 10.25]));
    // Where `beyond` past the inner portal, straight out from the bot, comes
    // out by its partner.
    let sim = fixture::synthetic_simulation(&bricks).unwrap();
    let passages = sim.passages();
    let inner = passages
        .list
        .iter()
        .find(|p| p.centre.z > 20.0 && p.normal.z > 0.5)
        .expect("the inner portal opens toward the bot");
    let seen = Vec3::new(30.25, 0.05, inner.centre.z - beyond);
    (bricks, inner.carry.transform_point3(seen))
}

const PORTAL: &str = bri_sim::testing::PORTAL;

fn in_room(p: Vec3) -> bool {
    (26.0..34.5).contains(&p.x) && (26.0..34.5).contains(&p.z)
}

#[test]
fn a_bot_sees_its_enemy_through_a_portal_and_walks_through_after_them() {
    let mut s = session();
    let (bricks, stand) = portal_room(1, 6.0);
    assert!(!in_room(stand), "the builder stands outside: {stand}");
    // The mini-game puts its members at a spawn point: this one.
    s.set_spawn_points(vec![stand]).unwrap();
    let human = s.join("Builder".into(), stand, true).unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    let bricks = bricks
        .into_iter()
        .map(|mut b| {
            b.owner = human;
            b
        })
        .collect();
    load(&mut s, human, bricks);
    minigame(&mut s, human, TOOLS_ONLY);
    steps(&mut s, &[human], 60, &mut sequence);
    let bot = bots(&s)[0];
    assert!(in_room(feet(&s, bot)), "the bot starts in its room");
    let mut out_after = None;
    for tick in 0..120 * 20 {
        steps(&mut s, &[human], 1, &mut sequence);
        let now = feet(&s, bot);
        if out_after.is_none() && !in_room(now) {
            out_after = Some(tick);
        }
        if (now - feet(&s, human)).length() < 3.0 {
            eprintln!("out after {out_after:?}, reached the builder after {tick} ticks");
            break;
        }
    }
    // Seen through the portal at once, it heads straight there.
    assert!(
        out_after.is_some_and(|t| t < 120 * 3),
        "the bot left its closed room through the portal: {out_after:?}"
    );
    assert!(
        (feet(&s, bot) - feet(&s, human)).length() < 3.0,
        "reached the builder through the portal: bot {} builder {}",
        feet(&s, bot),
        feet(&s, human)
    );
}

#[test]
fn an_armed_bot_shoots_through_a_portal_from_where_it_stands() {
    let mut s = session();
    let (bricks, stand) = portal_room(1, 7.0);
    // The mini-game puts its members at a spawn point: this one.
    s.set_spawn_points(vec![stand]).unwrap();
    let human = s.join("Builder".into(), stand, true).unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    let bricks = bricks
        .into_iter()
        .map(|mut b| {
            b.owner = human;
            b
        })
        .collect();
    load(&mut s, human, bricks);
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
    let bot = bots(&s)[0];
    let mut hit = None;
    for tick in 0..120 * 15 {
        steps(&mut s, &[human], 1, &mut sequence);
        if s.vitals()[&human].health < 100.0 {
            hit = Some(tick);
            break;
        }
    }
    let hit = hit.expect("the bot hit the builder");
    eprintln!("first hit after {hit} ticks");
    assert!(
        in_room(feet(&s, bot)),
        "it shot through the portal, not after walking out: {}",
        feet(&s, bot)
    );
}

/// The Blockhead Bot's kind, changed by `change`, as the only kind.
fn only_kind(s: &mut Session, change: impl FnOnce(&mut bri_sim::bot_kind::BotKind)) {
    let mut kinds = bri_sim::bot_kind::BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots;
    change(&mut kinds[0]);
    s.set_bot_kinds(kinds).unwrap();
}

fn bite(damage: f32) -> bri_sim::bot_kind::BotMelee {
    serde_json::from_value(serde_json::json!({ "damage": damage, "seconds": 1.0 })).unwrap()
}

fn archetype_of(s: &Session, owner: OwnerId) -> String {
    let state = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap_or_else(|| panic!("{owner} in the snapshot: {:?}", s.vitals().get(&owner)));
    s.archetypes().resolve(state.archetype).id.clone()
}

#[test]
fn an_empty_handed_bot_that_bites_closes_in_and_bites_once_a_second() {
    let mut s = session();
    only_kind(&mut s, |k| k.melee = Some(bite(15.0)));
    let human = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    load(&mut s, human, vec![bot_brick([10.0, 0.1, 30.0], human)]);
    minigame(&mut s, human, TOOLS_ONLY);
    steps(&mut s, &[human], 60, &mut sequence);
    let bot = bots(&s)[0];
    let mut hits = Vec::new();
    let mut health = 100.0;
    for tick in 0..120 * 12 {
        steps(&mut s, &[human], 1, &mut sequence);
        let now = s.vitals()[&human].health;
        if now < health {
            hits.push((tick, health - now));
            health = now;
        }
        if hits.len() == 2 {
            break;
        }
    }
    assert_eq!(hits.len(), 2, "bitten twice: {hits:?}");
    assert!(
        hits.iter().all(|(_, d)| (*d - 15.0).abs() < 0.01),
        "each bite takes its damage: {hits:?}"
    );
    assert!(
        hits[1].0 - hits[0].0 >= 119,
        "a second between bites: {hits:?}"
    );
    assert!(
        (feet(&s, bot) - feet(&s, human)).length() < 3.5,
        "it closed in to bite"
    );
}

#[test]
fn a_bot_keeps_its_kinds_body_in_and_out_of_a_mini_game() {
    let mut s = session();
    let quake = "v20.player.playerquakearmor";
    only_kind(&mut s, |k| k.body = Some(quake.into()));
    let human = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    load(&mut s, human, vec![bot_brick([10.0, 0.1, 30.0], human)]);
    steps(&mut s, &[human], 30, &mut sequence);
    let bot = bots(&s)[0];
    assert_eq!(archetype_of(&s, bot), quake);
    // The mini-game's player type does not replace it, nor its respawns.
    minigame(&mut s, human, TOOLS_ONLY);
    steps(&mut s, &[human], 60, &mut sequence);
    assert_eq!(archetype_of(&s, bot), quake, "in the game");
    s.command(human, 102, Command::MiniGame(MiniGameRequest::Reset))
        .unwrap();
    steps(&mut s, &[human], 60, &mut sequence);
    // A reset makes the brick's bot anew.
    let bot = bots(&s)[0];
    assert_eq!(archetype_of(&s, bot), quake, "after the game reset");
    s.command(human, 103, Command::MiniGame(MiniGameRequest::End))
        .unwrap();
    steps(&mut s, &[human], 60, &mut sequence);
    let bot = bots(&s)[0];
    assert_eq!(archetype_of(&s, bot), quake, "after the game ended");
    // A body no enabled Add-On has makes no bot, and its builder hears why.
    let mut s = session();
    only_kind(&mut s, |k| k.body = Some("nowhere:archetype/none".into()));
    let human = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    load(&mut s, human, vec![bot_brick([10.0, 0.1, 30.0], human)]);
    steps(&mut s, &[human], 30, &mut sequence);
    assert!(bots(&s).is_empty());
}

/// Deep water, 4 units across and 6 deep, centred on (x, z), over the
/// plate a bot spawns from.
fn pool(x: f32, z: f32, owner: OwnerId) -> Brick {
    Brick::new(
        ContentRef::Resolved(bri_sim::testing::DEEP_WATER.into()),
        [x, 3.2, z],
        owner,
    )
}

fn in_pool(p: Vec3, x: f32, z: f32) -> bool {
    (p.x - x).abs() <= 2.05 && (p.z - z).abs() <= 2.05 && p.y < 6.2
}

#[test]
fn a_swimming_bot_roams_its_water_at_every_depth_and_never_leaves_it() {
    let mut s = session();
    only_kind(&mut s, |k| {
        k.moves = bri_sim::bot_kind::Moves::Swim;
        k.wander_radius = 8.0;
    });
    let human = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    load(
        &mut s,
        human,
        vec![pool(12.0, 30.0, human), bot_brick([12.0, 0.1, 30.0], human)],
    );
    steps(&mut s, &[human], 30, &mut sequence);
    let bot = bots(&s)[0];
    let (mut low, mut high) = (f32::MAX, f32::MIN);
    for _ in 0..120 * 40 {
        steps(&mut s, &[human], 1, &mut sequence);
        let at = feet(&s, bot);
        assert!(in_pool(at, 12.0, 30.0), "stayed in its water: {at}");
        low = low.min(at.y);
        high = high.max(at.y);
    }
    assert!(high - low > 1.5, "swam up and down: {low} to {high}");
    // An enemy on dry land: it comes to the water's edge nearest them and
    // no farther.
    s.set_spawn_points(vec![Vec3::new(6.0, 0.05, 30.0)])
        .unwrap();
    minigame(&mut s, human, TOOLS_ONLY);
    let mut nearest = f32::MAX;
    for _ in 0..120 * 10 {
        steps(&mut s, &[human], 1, &mut sequence);
        let at = feet(&s, bot);
        assert!(in_pool(at, 12.0, 30.0), "stayed in its water: {at}");
        nearest = nearest.min(at.x);
    }
    assert!(nearest < 10.6, "came to the near edge: {nearest}");
}

#[test]
fn a_swimming_bot_goes_up_after_a_swimmer_and_bites_them() {
    let mut s = session();
    only_kind(&mut s, |k| {
        k.moves = bri_sim::bot_kind::Moves::Swim;
        k.melee = Some(bite(20.0));
    });
    // The builder floats up in the pool (the mini-game puts its members
    // there); the bot starts on the floor.
    s.set_spawn_points(vec![Vec3::new(13.0, 3.0, 31.0)])
        .unwrap();
    let human = s
        .join("Builder".into(), Vec3::new(13.0, 3.0, 31.0), true)
        .unwrap();
    let mut sequence = 0;
    load(
        &mut s,
        human,
        vec![pool(12.0, 30.0, human), bot_brick([11.0, 0.1, 29.0], human)],
    );
    minigame(&mut s, human, TOOLS_ONLY);
    steps(&mut s, &[human], 60, &mut sequence);
    let bot = bots(&s)[0];
    let mut bitten = None;
    for tick in 0..120 * 15 {
        steps(&mut s, &[human], 1, &mut sequence);
        assert!(in_pool(feet(&s, bot), 12.0, 30.0), "stayed in its water");
        if s.vitals()[&human].health < 100.0 {
            bitten = Some(tick);
            break;
        }
    }
    let bitten = bitten.expect("the bot bit the swimmer");
    eprintln!("bitten after {bitten} ticks, bot at {}", feet(&s, bot));
}

/// A charged weapon (the Spear) is held back until it is ready, then let
/// go to throw; tapping it as a gun only ever aborts the charge.
#[test]
fn a_bot_with_a_spear_holds_it_back_then_throws_it() {
    let mut s = session();
    let human = s
        .join("Builder".into(), Vec3::new(-20.0, 0.05, 36.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    load(&mut s, human, vec![bot_brick([-20.0, 0.1, 20.0], human)]);
    minigame(
        &mut s,
        human,
        [
            Some(bri_weapons::testing::SPEAR_ITEM.into()),
            None,
            None,
            None,
            None,
        ],
    );
    steps(&mut s, &[human], 60, &mut sequence);
    let mut hit = None;
    for tick in 0..120 * 20 {
        steps(&mut s, &[human], 1, &mut sequence);
        if s.vitals()[&human].health < 100.0 {
            hit = Some(tick);
            break;
        }
    }
    eprintln!("speared after {hit:?} ticks");
    assert!(hit.is_some(), "the bot threw its spear and hit the builder");
}

/// A bot that sees an enemy warns its side: one standing out of sight of
/// the enemy, but within the warner's, goes to look where the enemy was
/// (Bot_Hole's `hAlertOtherBots`). Without the warning it stays home.
#[test]
fn a_bot_that_sees_an_enemy_warns_its_side() {
    for alerts in [false, true] {
        let mut s = session();
        only_kind(&mut s, |k| {
            k.sight = 32.0;
            k.wander_radius = 0.0;
            k.side = Some("pack".into());
            k.alerts_allies = alerts;
            k.melee = Some(bite(5.0));
        });
        let human = s
            .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
            .unwrap();
        let mut sequence = 0;
        steps(&mut s, &[human], 10, &mut sequence);
        // The mini-game puts the human at a spawn, (4.5, ±4.5): the near
        // bot sees it (20 to 29 away), the far one, 14 behind the near one,
        // is out of its sight (34 to 43).
        load(
            &mut s,
            human,
            vec![
                bot_brick([10.0, 0.1, 24.0], human),
                bot_brick([10.0, 0.1, 38.0], human),
            ],
        );
        minigame(&mut s, human, TOOLS_ONLY);
        steps(&mut s, &[human], 30, &mut sequence);
        let far = *bots(&s)
            .iter()
            .max_by(|a, b| feet(&s, **a).z.total_cmp(&feet(&s, **b).z))
            .unwrap();
        let start = feet(&s, far).distance(feet(&s, human));
        steps(&mut s, &[human], 120 * 3, &mut sequence);
        let now = feet(&s, far).distance(feet(&s, human));
        if alerts {
            assert!(
                now < start - 6.0,
                "the warned bot came to look: {start} to {now}"
            );
        } else {
            assert!(
                now > start - 1.0,
                "unwarned, it stayed home: {start} to {now}"
            );
        }
    }
}

/// A kind's `behaviours` weights: one with chase turned off sees the enemy
/// out of its reach and stays at its post; the same kind otherwise goes
/// after them.
#[test]
fn a_bot_whose_kind_never_chases_holds_its_post() {
    for chases in [true, false] {
        let mut s = session();
        only_kind(&mut s, |k| {
            k.sight = 40.0;
            k.wander_radius = 0.0;
            k.melee = Some(bite(5.0));
            if !chases {
                k.behaviours.insert("chase".into(), 0.0);
            }
        });
        let human = s
            .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
            .unwrap();
        let mut sequence = 0;
        steps(&mut s, &[human], 10, &mut sequence);
        load(&mut s, human, vec![bot_brick([10.0, 0.1, 24.0], human)]);
        minigame(&mut s, human, TOOLS_ONLY);
        steps(&mut s, &[human], 30, &mut sequence);
        let bot = bots(&s)[0];
        let start = feet(&s, bot).distance(feet(&s, human));
        steps(&mut s, &[human], 120 * 3, &mut sequence);
        let now = feet(&s, bot).distance(feet(&s, human));
        if chases {
            assert!(now < start - 6.0, "it gave chase: {start} to {now}");
        } else {
            assert!(now > start - 1.0, "it held its post: {start} to {now}");
        }
    }
}

/// A wall between the bot and its enemy with only a low gap in it, too
/// low to walk through upright: the bot crouches through it.
#[test]
fn a_bot_crawls_through_a_low_gap() {
    let mut s = session();
    only_kind(&mut s, |k| k.melee = Some(bite(5.0)));
    let human = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    // The wall along x = 8, see-through, too long to go round; from
    // z = 12 to 14, where the bot's way to the builder's spawn crosses it,
    // its columns are raised to leave a gap 1.6 high under them.
    let mut bricks = glass_wall(8.0, -40.0, 70.0, human);
    for b in &mut bricks {
        if (12.0..=14.0).contains(&(b.position[2] - 0.25)) {
            b.position[1] += 1.6;
        }
    }
    bricks.push(bot_brick([16.0, 0.1, 30.0], human));
    load(&mut s, human, bricks);
    minigame(&mut s, human, TOOLS_ONLY);
    steps(&mut s, &[human], 30, &mut sequence);
    let bot = bots(&s)[0];
    let mut closest = f32::MAX;
    let mut crossed_at = None;
    let mut last = feet(&s, bot);
    for _ in 0..120 * 10 {
        steps(&mut s, &[human], 1, &mut sequence);
        let now = feet(&s, bot);
        if last.x >= 8.0 && now.x < 8.0 {
            crossed_at = Some((now.z, now.y));
        }
        last = now;
        closest = closest.min(now.distance(feet(&s, human)));
    }
    let (z, y) = crossed_at.expect("the bot got past the wall");
    assert!(
        (11.5..=14.5).contains(&z) && y < 1.0,
        "under the wall, through the gap: z {z}, height {y}"
    );
    assert!(closest < 3.5, "and on to its enemy: {closest}");
}

/// A bot approaches a normal doorway in a long brick wall instead of treating
/// the visually open doorway as blocked by its jambs.
#[test]
fn a_bot_enters_through_a_brick_building_doorway() {
    let mut s = session();
    only_kind(&mut s, |k| k.melee = Some(bite(5.0)));
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 30.0)])
        .unwrap();
    let human = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    let mut bricks = glass_wall(8.0, 22.0, 38.0, human);
    // Three half-unit columns make a 1.5-unit doorway. A player can fit, but
    // only when the approach is centred on it. The bot starts behind the wall.
    bricks.retain(|b| !(29.5..=30.5).contains(&(b.position[2] - 0.25)));
    bricks.push(bot_brick([16.0, 0.1, 30.0], human));
    load(&mut s, human, bricks);
    minigame(&mut s, human, TOOLS_ONLY);
    steps(&mut s, &[human], 30, &mut sequence);
    let bot = bots(&s)[0];
    let mut crossed_at = None;
    let mut last = feet(&s, bot);
    let mut closest = f32::MAX;
    for _ in 0..120 * 15 {
        steps(&mut s, &[human], 1, &mut sequence);
        let now = feet(&s, bot);
        if last.x >= 8.0 && now.x < 8.0 {
            crossed_at.get_or_insert(now.z);
        }
        last = now;
        closest = closest.min(now.distance(feet(&s, human)));
    }
    let z = crossed_at.expect("the bot entered through the doorway");
    assert!((28.5..=31.5).contains(&z), "crossed in the doorway: {z}");
    assert!(closest < 3.5, "reached the builder inside: {closest}");
}

/// Four allies make for one narrow doorway at once. Each one ahead going
/// the same way is followed, not walked round, so the file keeps moving:
/// every bot gets through and none is left wedged at the jambs.
#[test]
fn allies_file_through_one_narrow_doorway_without_deadlock() {
    let mut s = session();
    only_kind(&mut s, |k| {
        k.melee = Some(bite(5.0));
        k.side = Some("file".into());
    });
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 30.0)])
        .unwrap();
    let human = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    let mut bricks = glass_wall(8.0, 22.0, 38.0, human);
    bricks.retain(|b| !(29.5..=30.5).contains(&(b.position[2] - 0.25)));
    for z in [27.0, 29.0, 31.0, 33.0] {
        bricks.push(bot_brick([16.0, 0.1, z], human));
    }
    load(&mut s, human, bricks);
    minigame(&mut s, human, TOOLS_ONLY);
    steps(&mut s, &[human], 30, &mut sequence);
    let all = bots(&s);
    assert_eq!(all.len(), 4);
    let mut through = std::collections::BTreeMap::new();
    for tick in 0..120 * 30 {
        steps(&mut s, &[human], 1, &mut sequence);
        for bot in &all {
            if feet(&s, *bot).x < 8.0 {
                through.entry(*bot).or_insert(tick);
            }
        }
        if through.len() == all.len() {
            break;
        }
    }
    eprintln!("through the doorway at ticks {through:?}");
    assert_eq!(
        through.len(),
        all.len(),
        "every bot got through: {through:?}"
    );
}

#[test]
fn a_jetting_bot_closes_on_an_enemy_on_a_high_brick_platform() {
    let mut s = session();
    only_kind(&mut s, |k| k.melee = Some(bite(5.0)));
    let spawn = Vec3::new(16.0, 6.05, 30.0);
    s.set_spawn_points(vec![spawn]).unwrap();
    let human = s.join("Builder".into(), spawn, true).unwrap();
    let mut sequence = 0;
    steps(&mut s, &[human], 10, &mut sequence);
    let mut bricks = vec![bot_brick([10.0, 0.1, 30.0], human)];
    for x in 14..=18 {
        for z in 28..=32 {
            for y in [1.5, 4.5] {
                let mut b = Brick::new(
                    ContentRef::Resolved(fixture::TALL.into()),
                    [x as f32 + 0.25, y, z as f32 + 0.25],
                    human,
                );
                b.raycast = false;
                bricks.push(b);
            }
        }
    }
    load(&mut s, human, bricks);
    minigame(&mut s, human, TOOLS_ONLY);
    steps(&mut s, &[human], 30, &mut sequence);
    let bot = bots(&s)[0];
    let mut closest = f32::MAX;
    let mut late_farthest = 0.0_f32;
    for tick in 0..120 * 20 {
        steps(&mut s, &[human], 1, &mut sequence);
        let distance = feet(&s, bot).distance(feet(&s, human));
        closest = closest.min(distance);
        if tick >= 120 * 15 {
            late_farthest = late_farthest.max(distance);
        }
    }
    assert!(
        closest < 4.5,
        "the bot reached the elevated enemy: {closest}"
    );
    assert!(
        late_farthest < 5.0,
        "it stayed controlled near melee: {late_farthest}"
    );
    assert!(s.vitals()[&human].health < 100.0, "it reached melee range");
}
