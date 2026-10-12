//! Work counts for the bot step (v0.2.10 performance waste audit, findings
//! 5 and 6): who sides with whom is derived from the live state at most
//! once per pair per tick, and a bot looks around for a player to goof at
//! every tenth of a second, not every tick.
use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;

fn bot_brick(at: [f32; 3], owner: OwnerId) -> Brick {
    let at = [at[0] + 0.25, at[1], at[2] + 0.25];
    let mut brick = Brick::new(ContentRef::Resolved(fixture::PLATE.into()), at, owner);
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(fixture::BOT.into()),
        recolor: false,
        team: None,
    }));
    brick
}
fn steps(s: &mut Session, human: OwnerId, ticks: usize, sequence: &mut u64) {
    for _ in 0..ticks {
        *sequence += 1;
        s.movement(human, *sequence, MoveInput::default()).unwrap();
        s.step().unwrap();
    }
}

#[test]
fn relations_once_per_pair_per_tick_and_goof_looks_every_tenth_second() {
    let mut s = fixture::synthetic().unwrap().session;
    s.set_tool_catalog(ToolCatalog {
        vehicles: [fixture::BOT.to_string()].into(),
        vehicle_bricks: [fixture::PLATE.to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    let human = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 30.0), true)
        .unwrap();
    let mut sequence = 0;
    steps(&mut s, human, 10, &mut sequence);
    let mut world = World::new("Bots".into(), "chaos/map".into(), vec![[1.0; 4]]);
    for i in 0..8u64 {
        let at = [-14.0 + 4.0 * i as f32, 0.1, 20.0 + 3.0 * (i % 2) as f32];
        world.bricks.insert(i + 1, bot_brick(at, human));
    }
    world.next_brick_id = 9;
    s.command(
        human,
        100,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    s.command(
        human,
        101,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout: [None, None, None, None, None],
                ..Default::default()
            },
        }),
    )
    .unwrap();
    steps(&mut s, human, 60, &mut sequence);
    let bots = s.names().keys().filter(|o| s.is_bot(**o)).count();
    assert!(bots >= 6, "{bots} bots spawned");
    let players = bots + 1;
    let (asks0, answers0, looks0) = s.bot_work_counts();
    const TICKS: u64 = 240;
    steps(&mut s, human, TICKS as usize, &mut sequence);
    let (asks, answers, looks) = s.bot_work_counts();
    let (asks, answers, looks) = (asks - asks0, answers - answers0, looks - looks0);
    eprintln!(
        "{bots} bots, {TICKS} ticks: relation asks {asks}, derived {answers}, goof looks {looks}"
    );
    let pairs = (players * players) as u64;
    assert!(
        answers <= TICKS * pairs,
        "relations derived more than once per pair per tick: {answers} > {TICKS} x {pairs}"
    );
    // Idle bots ask about twice per pair per tick; fighting ones many times
    // more (the 16-bot profile). The bound above is the work count; this
    // only checks that the cached answer is what the repeats get.
    assert!(
        asks > answers + TICKS * pairs / 2,
        "the per-tick answer is not reused: {asks} asks for {answers} derivations"
    );
    assert!(
        looks <= bots as u64 * (TICKS / 12 + 1),
        "goof look-arounds every tick: {looks} in {TICKS} ticks for {bots} bots"
    );
}
