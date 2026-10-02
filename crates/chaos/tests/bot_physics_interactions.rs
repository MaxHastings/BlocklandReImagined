//! Contact mechanics used by a bot's walking controls and by human moves.
//! These are physical movement checks, not a promise that flat-ground walking
//! gives a heavy Steel Ball enough speed to hurt somebody.
use bri_chaos::fixture;
use bri_package_runtime::ops::ObjectRef;
use bri_sim::{
    player::MoveInput,
    session::{ActionAim, Command, MiniGameRequest, PackageCommand, Session},
};
use bri_vehicles::{schema::Family, testing};
use glam::Vec3;

const BALL: &str = "test:vehicle/unfamiliar-contact-body";

fn contact_pack(mass: f32) -> bri_vehicles::Pack {
    let mut pack = testing::pack();
    pack.definitions.retain(|d| d.family == Family::Ball);
    let d = &mut pack.definitions[0];
    d.id = BALL.into();
    d.mass = mass;
    d.bounds_min = [-1.25; 3];
    d.bounds_max = [1.25; 3];
    d.collision_hulls = vec![testing::box_hull(d.bounds_min, d.bounds_max)];
    d.inertia_box = [1.9365; 3];
    d.friction = 0.8;
    d.restitution = 0.2;
    d.drag = 0.4;
    d.angular_drag = 0.3;
    // `shove` is run-over policy, not authority to transfer contact momentum.
    d.shove = false;
    d.runover_speed = 12.0;
    d.harms_only_in_minigames = true;
    pack
}

fn session(mass: f32) -> Session {
    let mut s = Session::new(fixture::synthetic_simulation(&[]).unwrap());
    s.set_vehicle_pack(contact_pack(mass), Vec::new()).unwrap();
    s
}

fn feed(s: &mut Session, moves: &[(u64, MoveInput)], ticks: usize, sequence: &mut u64) {
    for _ in 0..ticks {
        *sequence += 1;
        for (owner, input) in moves {
            s.movement(*owner, *sequence, *input).unwrap();
        }
        s.step().unwrap();
    }
}

fn ball_position(s: &Session, id: u64) -> Vec3 {
    Vec3::from(
        s.vehicle_poses()
            .into_iter()
            .find(|v| v.id == id)
            .unwrap()
            .position,
    )
}

fn player_position(s: &Session, owner: u64) -> Vec3 {
    Vec3::from(
        s.snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == owner)
            .unwrap()
            .feet,
    )
}

fn walking() -> MoveInput {
    MoveInput {
        forward: 1.0,
        ..Default::default()
    }
}

fn roll(mass: f32) -> f32 {
    let mut s = session(mass);
    let p = s
        .join("Walker".into(), Vec3::new(0.0, 0.05, 6.0), false)
        .unwrap();
    let id = s
        .spawn_vehicle_at(p, BALL, Vec3::new(0.0, 1.3, 1.0), 0.0, Vec3::ZERO)
        .unwrap();
    let mut sequence = 0;
    feed(&mut s, &[(p, MoveInput::default())], 120, &mut sequence);
    let before = ball_position(&s, id);
    feed(&mut s, &[(p, walking())], 300, &mut sequence);
    let after = ball_position(&s, id);
    assert!(s.vitals()[&p].alive);
    assert_eq!(
        s.vitals()[&p].health,
        100.0,
        "slow contact causes no damage"
    );
    before.z - after.z
}

#[test]
fn normal_walking_rolls_an_unfamiliar_body_without_shove_policy() {
    let progress = roll(900.0);
    assert!(
        progress > 0.25,
        "a real contact rolled the heavy body: {progress}"
    );
}

#[test]
fn actual_mass_limits_the_momentum_walking_transfers() {
    let light = roll(90.0);
    let heavy = roll(9_000.0);
    assert!(
        light > heavy + 0.25,
        "lighter body moves farther: light {light}, heavy {heavy}"
    );
}

#[test]
fn walking_without_contact_never_moves_the_body() {
    let mut s = session(900.0);
    let p = s
        .join("Walker".into(), Vec3::new(6.0, 0.05, 6.0), false)
        .unwrap();
    let id = s
        .spawn_vehicle_at(p, BALL, Vec3::new(0.0, 1.3, 1.0), 0.0, Vec3::ZERO)
        .unwrap();
    let mut sequence = 0;
    feed(&mut s, &[(p, MoveInput::default())], 120, &mut sequence);
    let before = ball_position(&s, id);
    feed(&mut s, &[(p, walking())], 300, &mut sequence);
    assert!((ball_position(&s, id) - before).length() < 0.05);
}

#[test]
fn trust_refusal_blocks_a_deliberate_contact_push() {
    let mut s = session(900.0);
    let owner = s
        .join("Owner".into(), Vec3::new(20.0, 0.05, 6.0), false)
        .unwrap();
    let walker = s
        .join("Walker".into(), Vec3::new(0.0, 0.05, 6.0), false)
        .unwrap();
    let id = s
        .spawn_vehicle_at(owner, BALL, Vec3::new(0.0, 1.3, 1.0), 0.0, Vec3::ZERO)
        .unwrap();
    assert!(!s.may_move(walker, ObjectRef::Vehicle(id)));
    let mut sequence = 0;
    feed(
        &mut s,
        &[
            (owner, MoveInput::default()),
            (walker, MoveInput::default()),
        ],
        120,
        &mut sequence,
    );
    let before = ball_position(&s, id);
    feed(
        &mut s,
        &[(owner, MoveInput::default()), (walker, walking())],
        300,
        &mut sequence,
    );
    assert!(
        (ball_position(&s, id) - before).length() < 0.05,
        "untrusted walking supplied no contact push"
    );
    assert!(
        player_position(&s, walker).z > before.z + 1.5,
        "body still blocks the walker"
    );
}

#[test]
fn a_blocked_body_and_its_walker_do_not_pass_through_a_wall() {
    let mut s = session(900.0);
    let p = s
        .join("Walker".into(), Vec3::new(0.0, 0.05, -8.0), false)
        .unwrap();
    let id = s
        .spawn_vehicle_at(p, BALL, Vec3::new(0.0, 1.3, -12.0), 0.0, Vec3::ZERO)
        .unwrap();
    let mut sequence = 0;
    feed(&mut s, &[(p, MoveInput::default())], 120, &mut sequence);
    feed(&mut s, &[(p, walking())], 600, &mut sequence);
    // The fixture's wall front is z=-14.5. A radius-1.25 sphere must
    // remain on this side, along with the body walking into its back.
    assert!(ball_position(&s, id).z >= -13.3);
    assert!(player_position(&s, p).z > -13.0);
}

#[test]
fn a_falling_contact_hazard_credits_its_walker_instead_of_its_builder() {
    // Gravity supplies attack energy. Walking into its side supplies only
    // real contact momentum and attribution, never a fabricated attack force.
    let mut s = session(10.0);
    let mut pack = testing::pack();
    pack.definitions.retain(|d| d.family == Family::Ball);
    let d = &mut pack.definitions[0];
    d.id = BALL.into();
    d.mass = 10.0;
    d.bounds_min = [-1.25; 3];
    d.bounds_max = [1.25; 3];
    d.collision_hulls = vec![testing::box_hull(d.bounds_min, d.bounds_max)];
    d.inertia_box = [1.9365; 3];
    d.runover_speed = 2.0;
    d.runover_damage = 1000.0;
    d.harms_only_in_minigames = true;
    s.set_vehicle_pack(pack, Vec::new()).unwrap();
    let builder = s
        .join("Builder".into(), Vec3::new(20.0, 0.05, 6.0), false)
        .unwrap();
    let walker = s
        .join("Walker".into(), Vec3::new(0.0, 0.05, 6.0), false)
        .unwrap();
    let victim = s
        .join("Victim".into(), Vec3::new(0.0, 0.05, -0.5), false)
        .unwrap();
    s.set_spawn_points(vec![player_position(&s, builder)])
        .unwrap();
    s.command(
        builder,
        1,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                loadout: Default::default(),
                ..Default::default()
            },
        }),
    )
    .unwrap();
    let game = s.minigame_views()[0].id;
    for other in [walker, victim] {
        s.set_spawn_points(vec![player_position(&s, other)])
            .unwrap();
        s.command(other, 1, Command::MiniGame(MiniGameRequest::Join { game }))
            .unwrap();
    }
    let mut sequence = 0;
    feed(
        &mut s,
        &[
            (builder, MoveInput::default()),
            (walker, MoveInput::default()),
            (victim, MoveInput::default()),
        ],
        330,
        &mut sequence,
    );
    // Rejoining at an elevated spawn starts both bodies stationary.
    // Subsequent input and gravity supply all their motion.
    s.disconnect(walker).unwrap();
    s.resume(walker, Vec3::new(0.0, 14.0, 3.25)).unwrap();
    s.set_spawn_points(vec![Vec3::new(0.0, 14.0, 3.25)])
        .unwrap();
    s.command(walker, 2, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();
    let id = s
        .spawn_vehicle_at(builder, BALL, Vec3::new(0.0, 15.25, 1.0), 0.0, Vec3::ZERO)
        .unwrap();
    assert!(s.may_move(walker, ObjectRef::Vehicle(id)));
    let mut fastest = 0.0_f32;
    for _ in 0..600 {
        feed(
            &mut s,
            &[
                (builder, MoveInput::default()),
                (walker, walking()),
                (victim, MoveInput::default()),
            ],
            1,
            &mut sequence,
        );
        fastest = fastest.max(
            s.vehicle_poses()
                .into_iter()
                .find(|v| v.id == id)
                .map_or(0.0, |v| Vec3::from(v.velocity).length()),
        );
        if !s.vitals()[&victim].alive {
            break;
        }
    }
    let vitals = s.vitals();
    assert!(
        !vitals[&victim].alive,
        "the gravity-driven hazard reached its victim: ball {:?}, walker {:?}, victim {:?}, fastest {fastest}, health {}",
        ball_position(&s, id),
        player_position(&s, walker),
        player_position(&s, victim),
        vitals[&victim].health
    );
    assert_eq!(
        vitals[&walker].score,
        1,
        "the mover receives the kill: ball {:?}, walker {:?}, fastest {fastest}, builder score {}",
        ball_position(&s, id),
        player_position(&s, walker),
        vitals[&builder].score
    );
    assert_eq!(
        vitals[&builder].score, 0,
        "ownership does not steal contact credit"
    );
}

const PUSHER: &str = "test:bot/physical-opportunities";
const ALLY: &str = "test:bot/stationary-ally";

fn ai_kind(id: &str, enabled: bool, interactions: bool) -> bri_sim::bot_kind::BotKind {
    let mut kind = bri_sim::bot_kind::BotKind {
        id: id.into(),
        name: id.rsplit('/').next().unwrap().into(),
        side: Some("contact-test-side".into()),
        wander_radius: 0.0,
        aim_error_degrees: 0.0,
        ..Default::default()
    };
    kind.behaviours = bri_sim::bot_kind::BEHAVIOURS
        .into_iter()
        .map(|name| (name.into(), 0.0))
        .collect();
    if enabled {
        kind.behaviours.insert("chase".into(), 1.0);
        kind.behaviours
            .insert("interact".into(), if interactions { 1.0 } else { 0.0 });
    }
    kind
}

struct AiScene {
    session: Session,
    builder: u64,
    enemy: u64,
    bot: u64,
    ball: u64,
    sequence: u64,
    held: bool,
}

impl AiScene {
    fn new(permission: bool, ally: bool, held: bool) -> Self {
        Self::with_blocked_body(permission, ally, held, false)
    }

    fn with_blocked_body(permission: bool, ally: bool, held: bool, blocked: bool) -> Self {
        use bri_world::{Brick, ContentRef, VehicleSpawn};
        let spawn = |id: &str, at| {
            let mut brick = Brick::new(ContentRef::Resolved(fixture::PLATE.into()), at, 1);
            brick.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(id.into()),
                recolor: false,
            }));
            brick
        };
        // The ordinary chase from (-5,6) to (0,-9) passes clear of the
        // ball. Choosing it requires a detour to its rear at (0,3.3).
        let mut bricks = vec![spawn(PUSHER, [-4.75, 0.1, 6.25])];
        if ally {
            bricks.push(spawn(ALLY, [0.75, 0.1, -2.75]));
        }
        if blocked {
            // Solid columns that sight deliberately ignores. The body is
            // blocked at its equator while the bot still sees its enemy.
            for i in 0..8 {
                let mut wall = Brick::new(
                    ContentRef::Resolved(fixture::TALL.into()),
                    [-1.75 + i as f32 * 0.5, 1.5, -0.75],
                    1,
                );
                wall.raycast = false;
                bricks.push(wall);
            }
        }
        let mut world =
            bri_world::World::new("Contact AI".into(), "chaos/map".into(), vec![[1.0; 4]]);
        world
            .owners
            .insert(1, bri_world::OwnerRecord::new([1; 32], "Builder".into()));
        for (i, brick) in bricks.into_iter().enumerate() {
            world.bricks.insert(i as u64 + 1, brick);
        }
        world.next_brick_id = world.bricks.len() as u64 + 1;
        let mut colliders = vec![
            rapier3d::prelude::ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                .translation(Vec3::new(0.0, -0.5, 0.0)),
            rapier3d::prelude::ColliderBuilder::cuboid(0.5, 4.0, 20.0)
                .translation(Vec3::new(12.0, 4.0, 0.0)),
        ];
        if blocked {
            // Side guides prevent a later ordinary chase from rolling the
            // body along the front wall. Its rear remains accessible for an
            // actual motor contact, and the enemy remains in sight.
            for x in [-1.5, 1.5] {
                colliders.push(
                    rapier3d::prelude::ColliderBuilder::cuboid(0.25, 1.5, 1.25)
                        .translation(Vec3::new(x, 1.5, 0.75)),
                );
            }
            // Corner stops also resist backward rolling, leaving a central
            // opening wide enough for the walking actor to touch the body.
            for x in [-1.0, 1.0] {
                colliders.push(
                    rapier3d::prelude::ColliderBuilder::cuboid(0.125, 1.5, 0.125)
                        .translation(Vec3::new(x, 1.5, 1.75)),
                );
            }
        }
        let mut s = Session::new(
            bri_sim::simulation::Simulation::new(
                world,
                fixture::synthetic_definitions().unwrap(),
                colliders,
            )
            .unwrap(),
        );
        let mut pack = contact_pack(10.0);
        pack.definitions[0].runover_speed = 2.0;
        pack.definitions[0].runover_damage = 1.0;
        s.set_vehicle_pack(
            pack,
            vec![ai_kind(PUSHER, false, false), ai_kind(ALLY, false, false)],
        )
        .unwrap();
        if held {
            use bri_package::packages::{PackageEntry, PackageSet, Side};
            let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../packages/showcase");
            s.set_weapon_pack(
                bri_weapons::Pack::from_json(
                    &std::fs::read(root.join("gravity-gun-tool/assets/weapons.json")).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
            let packages = [
                ("gravity-gun", Side::Server),
                ("gravity-gun-tool", Side::Shared),
            ]
            .into_iter()
            .map(|(id, side)| PackageEntry {
                id: id.into(),
                version: "1.0.0".into(),
                side,
                dir: id.into(),
                role: None,
            })
            .collect();
            s.install_packages(
                std::sync::Arc::new(
                    bri_package_runtime::Catalog::load(
                        &root,
                        &PackageSet {
                            schema_version: 1,
                            packages,
                        },
                        true,
                    )
                    .unwrap(),
                ),
                None,
            )
            .unwrap();
        }
        let builder = s
            .join_verified(
                "Builder".into(),
                Vec3::new(20.0, 0.05, 6.0),
                false,
                Some(bri_admin::Principal([1; 32])),
            )
            .unwrap();
        assert_eq!(builder, 1, "owns the bot's authored spawn brick");
        let enemy = s
            .join("Enemy".into(), Vec3::new(0.0, 0.05, -9.0), false)
            .unwrap();
        s.set_spawn_points(vec![player_position(&s, builder)])
            .unwrap();
        s.command(
            builder,
            1,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: bri_minigames::Settings {
                    vehicle_damage: permission,
                    loadout: Default::default(),
                    ..Default::default()
                },
            }),
        )
        .unwrap();
        let game = s.minigame_views()[0].id;
        s.set_spawn_points(vec![player_position(&s, enemy)])
            .unwrap();
        s.command(enemy, 1, Command::MiniGame(MiniGameRequest::Join { game }))
            .unwrap();
        let mut sequence = 0;
        feed(
            &mut s,
            &[
                (builder, MoveInput::default()),
                (enemy, MoveInput::default()),
            ],
            330,
            &mut sequence,
        );
        let bot = s
            .names()
            .iter()
            .find(|(o, name)| s.is_bot(**o) && name.starts_with("physical-opportunities"))
            .map(|(o, _)| *o)
            .expect("the real session brain spawned");
        let ball = s
            .spawn_vehicle_at(
                builder,
                BALL,
                Vec3::new(0.0, 1.3, if blocked { 0.75 } else { 1.0 }),
                0.0,
                Vec3::ZERO,
            )
            .unwrap();
        assert_eq!(s.may_move(bot, ObjectRef::Vehicle(ball)), permission);
        let mut scene = Self {
            session: s,
            builder,
            enemy,
            bot,
            ball,
            sequence,
            held,
        };
        scene.tick(120);
        if held {
            scene
                .session
                .give_tool(enemy, "gravity-gun-tool:weapon/gravitygun", true)
                .unwrap();
            let at = ball_position(&scene.session, ball);
            let from = player_position(&scene.session, enemy) + Vec3::Y * 2.1;
            let d = at - from;
            let aim = ActionAim {
                yaw: d.x.atan2(-d.z),
                pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
            };
            scene
                .session
                .command_with_aim(
                    enemy,
                    2,
                    Command::Package(PackageCommand {
                        package: "gravity-gun".into(),
                        command: "grab".into(),
                        args: vec![],
                    }),
                    Some(aim),
                )
                .unwrap();
            assert_eq!(scene.session.held_by(enemy), Some(ObjectRef::Vehicle(ball)));
            scene.tick(120);
        }
        scene
    }

    fn tick(&mut self, ticks: usize) {
        let mut enemy_input = MoveInput::default();
        if self.held {
            let d = ball_position(&self.session, self.ball)
                - (player_position(&self.session, self.enemy) + Vec3::Y * 2.1);
            enemy_input.yaw = d.x.atan2(-d.z);
            enemy_input.pitch = d.y.atan2(Vec3::new(d.x, 0.0, d.z).length());
        }
        feed(
            &mut self.session,
            &[
                (self.builder, MoveInput::default()),
                (self.enemy, enemy_input),
            ],
            ticks,
            &mut self.sequence,
        );
    }

    fn run(&mut self, interactions: bool) -> (f32, f32) {
        self.session
            .set_bot_kinds(vec![
                ai_kind(PUSHER, true, interactions),
                ai_kind(ALLY, false, false),
            ])
            .unwrap();
        let before = ball_position(&self.session, self.ball);
        let rear = Vec3::new(before.x, 0.0, before.z + 1.25 + 0.625 + 0.4);
        let mut nearest_rear = f32::MAX;
        for _ in 0..420 {
            self.tick(1);
            let at = player_position(&self.session, self.bot);
            nearest_rear = nearest_rear.min(Vec3::new(at.x - rear.x, 0.0, at.z - rear.z).length());
        }
        (
            before.z - ball_position(&self.session, self.ball).z,
            nearest_rear,
        )
    }
}

#[test]
fn the_real_brain_detours_to_push_an_unnamed_loose_hazard() {
    let mut enabled = AiScene::new(true, false, false);
    let (pushed, approach) = enabled.run(true);
    let mut ordinary = AiScene::new(true, false, false);
    let (incidental, ordinary_approach) = ordinary.run(false);
    assert!(
        approach < 0.9,
        "the bot chose the rear approach: {approach}"
    );
    assert!(
        pushed > 0.5,
        "ordinary body contact moved the selected hazard: {pushed}"
    );
    assert!(
        ordinary_approach > 1.5,
        "ordinary enemy pursuit follows a different path: {ordinary_approach}"
    );
    assert!(
        incidental.abs() < 0.1,
        "unselected hazard stays put: {incidental}"
    );
}

#[test]
fn the_brain_rejects_a_hazard_its_minigame_forbids_moving() {
    let mut scene = AiScene::new(false, false, false);
    let (pushed, approach) = scene.run(true);
    assert!(
        approach > 1.5,
        "no commitment to an illegal rear approach: {approach}"
    );
    assert!(
        pushed.abs() < 0.1,
        "no unauthorized contact transfer: {pushed}"
    );
}

#[test]
fn the_brain_rejects_a_hazard_currently_in_a_players_grip() {
    let mut scene = AiScene::new(true, false, true);
    let (pushed, approach) = scene.run(true);
    assert_eq!(
        scene.session.held_by(scene.enemy),
        Some(ObjectRef::Vehicle(scene.ball))
    );
    assert!(
        approach > 1.5,
        "a held hazard gets no rear approach: {approach}"
    );
    assert!(
        pushed.abs() < 0.2,
        "a stationary grip keeps its hazard: {pushed}"
    );
}

#[test]
fn the_brain_keeps_a_loose_hazard_out_of_an_allies_corridor() {
    let mut scene = AiScene::new(true, true, false);
    let ally = scene
        .session
        .names()
        .iter()
        .find(|(o, name)| scene.session.is_bot(**o) && name.starts_with("stationary-ally"))
        .map(|(o, _)| *o)
        .unwrap();
    let before = player_position(&scene.session, ally);
    let (pushed, approach) = scene.run(true);
    assert!(
        approach > 1.5,
        "an ally in the roll corridor cancels the approach: {approach}"
    );
    assert!(
        pushed.abs() < 0.1,
        "the dangerous corridor prevents the push: {pushed}"
    );
    assert!(
        (player_position(&scene.session, ally) - before).length() < 0.1,
        "the ally was not bowled aside"
    );
}

#[test]
fn blocked_contact_does_not_renew_a_push_forever_and_retry_waits() {
    use bri_sim::session::BotTask;
    let mut scene = AiScene::with_blocked_body(true, false, false, true);
    scene
        .session
        .set_bot_kinds(vec![
            ai_kind(PUSHER, true, true),
            ai_kind(ALLY, false, false),
        ])
        .unwrap();
    let before = ball_position(&scene.session, scene.ball);
    let started = scene.session.simulation().state().tick;
    let mut acquired = false;
    let mut near_body = false;
    let mut deadline = 0;
    let mut abandoned = None;
    for _ in 0..900 {
        scene.tick(1);
        let tick = scene.session.simulation().state().tick;
        let thought = scene
            .session
            .bot_thoughts()
            .into_iter()
            .find(|t| t.bot == scene.bot)
            .unwrap();
        match thought.task {
            Some(BotTask::Push {
                vehicle,
                subject,
                deadline: until,
            }) if vehicle == scene.ball => {
                assert_eq!(subject, scene.enemy);
                acquired = true;
                deadline = until;
                let feet = player_position(&scene.session, scene.bot);
                near_body |= Vec3::new(feet.x - before.x, 0.0, feet.z - before.z).length() < 2.1;
            }
            None if acquired => {
                abandoned = Some(tick);
                assert_eq!(
                    thought.visible,
                    Some(scene.enemy),
                    "the enemy remains visible when the blocked task ends"
                );
                // The ordinary motor may exhaust its bounded replans before
                // the lease expires. That failure clears the navigation goal
                // and cools down the resource, just as lease expiry does.
                if tick < deadline {
                    assert!(
                        thought.goal.is_none(),
                        "early abandonment clears the stalled navigation goal: tick {tick}, deadline {deadline}"
                    );
                }
                break;
            }
            _ => {}
        }
    }
    assert!(acquired, "the brain initially chose the legal hazard");
    assert!(near_body, "the bot reached physical contact distance");
    let expired = abandoned.expect("a motionless body cannot keep renewing by contact impulse");
    assert!(
        expired < started + 900,
        "abandoned before the absolute 15-second age limit"
    );
    assert!(
        (ball_position(&scene.session, scene.ball) - before).length() < 0.1,
        "attempted contact did not move the blocked body"
    );
    for _ in 0..200 {
        scene.tick(1);
        let thought = scene
            .session
            .bot_thoughts()
            .into_iter()
            .find(|t| t.bot == scene.bot)
            .unwrap();
        assert!(
            !matches!(thought.task, Some(BotTask::Push { vehicle, .. }) if vehicle == scene.ball),
            "the failed body is not immediately reacquired during cooldown"
        );
    }
    assert!(
        (ball_position(&scene.session, scene.ball) - before).length() < 0.1,
        "the boxed body remains still during cooldown: before {before:?}, after {:?}, bot {:?}, abandoned {expired}, deadline {deadline}",
        ball_position(&scene.session, scene.ball),
        player_position(&scene.session, scene.bot)
    );
}
