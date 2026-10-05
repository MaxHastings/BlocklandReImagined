//! Environmental choices exercised through the authoritative session, with
//! invented content. No test assigns a bot a seat, moves its body or fires
//! its weapon on its behalf: the ordinary brain must make those decisions.
use bri_admin::{Action, Request};
use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::{
    bot_kind::BotPack,
    player::MoveInput,
    session::{Command, MiniGameRequest, PackageArg, PackageCommand, Session, ToolCatalog},
};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::{Quat, Vec3};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const ROVER: &str = "test:vehicle/survey-rover";
const CARRIER: &str = "test:vehicle/field-carrier";
const CHARGED: &str = "test:vehicle/charged-rover";
const UNARMED: &str = "test:vehicle/utility-chassis";
const TALL_GUNNER: &str = "test:vehicle/elevated-gunner";
// An unarmed chassis whose contact is harmless, so a long pursuit observes
// driving rather than ending in the target's death and respawn.
const PURSUER: &str = "test:vehicle/harmless-pursuer";
const CHARGE_TICKS: u64 = 24;
const CHARGE_STEPS: u8 = 3;
const ENEMY: Vec3 = Vec3::new(-12.0, 0.05, 8.0);
const VEHICLE: Vec3 = Vec3::new(-30.0, 0.6, 30.0);
const TOOLS_ONLY: [Option<String>; 5] = [None, None, None, None, None];

fn session(interact: f32) -> Session {
    let mut s = fixture::synthetic().unwrap().session;
    let (mut weapons, _) = fixture::synthetic_weapons().unwrap();
    // A living target must outlast assembly and lifecycle checks. This
    // changes only invented ammunition damage, not how a bot chooses or fires.
    weapons
        .projectiles
        .get_mut(bri_weapons::testing::GUN_PROJECTILE)
        .unwrap()
        .damage = 1.0;
    s.set_weapon_pack(weapons).unwrap();
    let (mut pack, _) = fixture::synthetic_vehicles().unwrap();
    for (id, order, dimensions) in [
        (ROVER, [2, 1, 0], [2.6, 1.0, 4.8]),
        (CARRIER, [1, 0, 2], [3.0, 1.4, 5.0]),
        (CHARGED, [2, 1, 0], [2.6, 1.0, 4.8]),
        (UNARMED, [2, 1, 0], [2.6, 1.0, 4.8]),
        (TALL_GUNNER, [0, 1, 2], [3.0, 1.4, 5.0]),
        (PURSUER, [2, 1, 0], [2.6, 1.0, 4.8]),
    ] {
        let mut d = bri_vehicles::testing::car();
        let seats = bri_vehicles::testing::tank().seats;
        d.id = id.into();
        d.name = id.into();
        d.seats = order.map(|i| seats[i].clone()).into();
        if id == TALL_GUNNER {
            // Ground-level players can board the lower passenger seat, but
            // the gunner is too high to click directly. Players use Next Seat.
            d.seats[2].transform.position[1] = 6.0;
        }
        d.bounds_min = [-dimensions[0] * 0.5, 0.4, -dimensions[2] * 0.5];
        d.bounds_max = [
            dimensions[0] * 0.5,
            0.4 + dimensions[1],
            dimensions[2] * 0.5,
        ];
        d.collision_hulls = vec![bri_vehicles::testing::box_hull(d.bounds_min, d.bounds_max)];
        // These armed chassis need a living combat target long enough to
        // observe crew assembly and several shots. Keep physical contact and
        // pushing, but use low authored runover damage in these gun fixtures.
        // The unarmed regression below restores full contact damage.
        d.runover_damage = 0.05;
        d.weapon = bri_vehicles::testing::tank().weapon;
        let weapon = d.weapon.as_mut().unwrap();
        // A different, nonballistic mounted weapon. Bots have empty hands,
        // so any such projectile must have come from the actual vehicle gun.
        weapon.projectile = bri_weapons::testing::GUN_PROJECTILE.into();
        weapon.speed = 100.0;
        weapon.cooldown_ticks = 120;
        if id == CHARGED {
            weapon.charge_ticks = CHARGE_TICKS;
            weapon.charge_steps = CHARGE_STEPS;
        }
        weapon.sound.clear();
        weapon.effect.clear();
        if id == UNARMED || id == PURSUER {
            d.weapon = None;
            for seat in &mut d.seats {
                seat.weapon = false;
            }
            d.runover_speed = 2.0;
            d.runover_damage = if id == UNARMED { 20.0 } else { 0.0 };
        }
        pack.definitions.push(d);
    }
    let mut kinds = BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots;
    kinds[0].behaviours.insert("interact".into(), interact);
    s.set_vehicle_pack(pack, kinds).unwrap();
    s.set_tool_catalog(ToolCatalog {
        vehicles: [
            fixture::BOT,
            ROVER,
            CARRIER,
            CHARGED,
            UNARMED,
            TALL_GUNNER,
            PURSUER,
        ]
        .map(String::from)
        .into(),
        vehicle_bricks: [fixture::PLATE.into()].into(),
        ..Default::default()
    })
    .unwrap();
    s.set_spawn_points(vec![ENEMY]).unwrap();
    s
}

fn bot_brick(at: Vec3, owner: OwnerId) -> Brick {
    let mut brick = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        (at + Vec3::new(0.25, 0.0, 0.25)).to_array(),
        owner,
    );
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(fixture::BOT.into()),
        recolor: false,
        team: None,
    }));
    brick
}

struct Game {
    s: Session,
    human: OwnerId,
    sequence: u64,
    vehicle: u64,
}
impl Game {
    fn new(kind: &str, count: usize, interact: f32, foreign: bool) -> Self {
        Self::with_session(session(interact), kind, count, foreign)
    }

    fn with_session(s: Session, kind: &str, count: usize, foreign: bool) -> Self {
        let positions: Vec<_> = (0..count)
            .map(|i| Vec3::new(-37.0, 0.1, 28.0 + i as f32 * 5.0))
            .collect();
        Self::with_layout(s, kind, &positions, foreign, false)
    }

    fn with_layout(
        mut s: Session,
        kind: &str,
        positions: &[Vec3],
        foreign: bool,
        barrier: bool,
    ) -> Self {
        let human = s
            .join_verified(
                "Target".into(),
                ENEMY,
                true,
                Some(bri_admin::Principal([1; 32])),
            )
            .unwrap();
        let owner = if foreign {
            s.join_verified(
                "Other builder".into(),
                Vec3::new(90.0, 0.05, 90.0),
                true,
                Some(bri_admin::Principal([2; 32])),
            )
            .unwrap()
        } else {
            human
        };
        let mut world = World::new("Crew".into(), "chaos/map".into(), vec![[1.0; 4]]);
        world.owners = s.simulation().state().owners.clone();
        for (i, at) in positions.iter().copied().enumerate() {
            world.bricks.insert(i as u64 + 1, bot_brick(at, human));
        }
        let count = positions.len();
        let mut vehicle_brick = bot_brick(Vec3::new(VEHICLE.x, 0.1, VEHICLE.z), owner);
        vehicle_brick.vehicle.as_mut().unwrap().vehicle = ContentRef::Resolved(kind.into());
        world.bricks.insert(count as u64 + 1, vehicle_brick);
        if barrier {
            // A solid six-unit enclosure whose raycast flag is off, like
            // the existing bot glass-wall fixtures. Its doorless interior
            // contains the whole chassis; sight is insufficient for boarding.
            let mut column = |x, z| {
                for y in [1.5, 4.5] {
                    let mut b =
                        Brick::new(ContentRef::Resolved(fixture::TALL.into()), [x, y, z], human);
                    b.raycast = false;
                    world.bricks.insert(world.bricks.len() as u64 + 1, b);
                }
            };
            for i in 0..14 {
                let z = 27.25 + i as f32 * 0.5;
                column(-32.25, z);
                column(-27.25, z);
            }
            for i in 1..10 {
                let x = -32.25 + i as f32 * 0.5;
                column(x, 27.25);
                column(x, 33.75);
            }
        }
        world.next_brick_id = world.bricks.len() as u64 + 1;
        let expected_bricks = world.bricks.len();
        s.set_load_pace(bri_sim::session::LoadPace::Bricks(256));
        s.command(
            human,
            1,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: true,
            },
        )
        .unwrap();
        for _ in 0..5 {
            s.step().unwrap();
        }
        assert!(!s.build_loading(), "fixture finished its authored load");
        assert_eq!(
            s.simulation().state().bricks.len(),
            expected_bricks,
            "every fixture brick fits the grid and was placed; {:?}",
            s.chat()
        );
        assert_eq!(
            s.simulation().state().bricks[&(count as u64 + 1)].owner,
            owner,
            "vehicle ownership survived the load"
        );
        s.command(
            human,
            2,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: Settings {
                    loadout: TOOLS_ONLY,
                    ..Default::default()
                },
            }),
        )
        .unwrap();
        let vehicle = s
            .vehicle_infos()
            .into_iter()
            .find(|v| v.definition == kind)
            .expect("the spawn brick made the vehicle")
            .id;
        Self {
            s,
            human,
            sequence: 1 << 40,
            vehicle,
        }
    }

    fn send(&mut self, owner: OwnerId, command: Command) {
        self.sequence += 1;
        self.s.command(owner, self.sequence, command).unwrap();
    }

    fn steps(&mut self, ticks: usize) {
        self.steps_with_input(ticks, MoveInput::default());
    }

    fn steps_with_input(&mut self, ticks: usize, input: MoveInput) {
        for _ in 0..ticks {
            self.sequence += 1;
            self.s.movement(self.human, self.sequence, input).unwrap();
            self.s.step().unwrap();
        }
    }

    fn bots(&self) -> Vec<OwnerId> {
        self.s
            .names()
            .keys()
            .copied()
            .filter(|p| self.s.is_bot(*p))
            .collect()
    }

    fn diagnostics(&self) -> String {
        format!(
            "vehicles {:?}; thoughts {:?}; poses {:?}; players {:?}; vitals {:?}",
            self.s.vehicle_infos(),
            self.s.bot_thoughts(),
            self.s.vehicle_poses(),
            self.s.snapshot().players,
            self.s.vitals()
        )
    }

    fn occupants(&self) -> Vec<Option<OwnerId>> {
        self.s
            .vehicle_infos()
            .into_iter()
            .find(|v| v.id == self.vehicle)
            .expect("vehicle still exists")
            .occupants
    }

    fn position(&self) -> Vec3 {
        self.s
            .vehicle_poses()
            .into_iter()
            .find(|v| v.id == self.vehicle)
            .expect("vehicle still exists")
            .position
            .into()
    }

    fn crew(&mut self, driver: usize, gunner: usize) -> (OwnerId, OwnerId) {
        for _ in 0..120 * 15 {
            self.steps(1);
            let seats = self.occupants();
            if let (Some(a), Some(b)) = (seats[driver], seats[gunner]) {
                assert!(self.s.is_bot(a) && self.s.is_bot(b), "bots formed the crew");
                assert_ne!(a, b, "a seat cannot have two roles' occupants");
                return (a, b);
            }
        }
        panic!(
            "driver and gunner never formed a crew: {:?}; {}",
            self.occupants(),
            self.diagnostics()
        );
    }
}

#[test]
fn a_passenger_switches_to_the_elevated_empty_gunner_seat() {
    let mut g = Game::new(TALL_GUNNER, 2, 1.0, false);
    let (driver, gunner) = g.crew(0, 2);
    assert_eq!(
        g.occupants()[1],
        None,
        "passenger moved into the useful role"
    );
    assert_ne!(driver, gunner);
    let mut fired = false;
    for _ in 0..120 * 8 {
        g.steps(1);
        if g.s
            .weapon_view()
            .fired()
            .any(|shot| shot.source.0 == gunner)
        {
            fired = true;
            break;
        }
    }
    assert!(
        fired,
        "the promoted gunner must use the actual mounted weapon"
    );
}

#[test]
fn unfamiliar_reordered_seats_form_crews_drive_and_fire_their_actual_weapon() {
    for (kind, driver_seat, gunner_seat, passenger_seat) in [(ROVER, 2, 0, 1), (CARRIER, 1, 2, 0)] {
        let mut g = Game::new(kind, 2, 1.0, false);
        let initial = g.position();
        g.steps(10);
        assert!(
            g.bots().iter().all(|b| g.s.mounted(*b).is_none()),
            "boarding requires reaching the chassis, not remote reservation"
        );
        let (driver, gunner) = g.crew(driver_seat, gunner_seat);
        assert_eq!(
            g.occupants()[passenger_seat],
            None,
            "useful roles before passengers"
        );
        assert_eq!(g.s.mounted(driver), Some((g.vehicle, driver_seat as u8)));
        assert_eq!(g.s.mounted(gunner), Some((g.vehicle, gunner_seat as u8)));
        let mut moved = 0.0f32;
        let mut independently_aimed_shot = false;
        for _ in 0..120 * 8 {
            g.steps(1);
            moved = moved.max(g.position().distance(initial));
            let pose =
                g.s.vehicle_poses()
                    .into_iter()
                    .find(|v| v.id == g.vehicle)
                    .unwrap();
            let hull_forward = Quat::from_array(pose.rotation) * Vec3::NEG_Z;
            for shot in g.s.weapon_view().fired() {
                assert_eq!(
                    shot.definition,
                    bri_weapons::testing::GUN_PROJECTILE,
                    "only the mounted gun is armed"
                );
                assert_eq!(shot.source.0, gunner, "the actual gunner owns the shot");
                let target_feet = Vec3::from(
                    g.s.snapshot()
                        .players
                        .into_iter()
                        .find(|p| p.owner == g.human)
                        .unwrap()
                        .feet,
                );
                let toward = (target_feet + Vec3::Y * 1.0 - shot.origin).normalize();
                let flight = shot.velocity.normalize();
                if flight.dot(toward) > 0.95 && flight.dot(hull_forward) < 0.995 {
                    independently_aimed_shot = true;
                }
            }
            if moved > 5.0 && independently_aimed_shot {
                break;
            }
        }
        assert!(moved > 5.0, "normal wheel physics moved {kind}: {moved}");
        assert!(
            independently_aimed_shot,
            "the gunner aimed at the target independently of the chassis in {kind}"
        );
    }
}

#[test]
fn a_lone_driver_does_not_wait_forever_for_an_absent_gunner() {
    let mut g = Game::new(ROVER, 1, 1.0, false);
    let initial = g.position();
    let mut moved = 0.0f32;
    for _ in 0..120 * 15 {
        g.steps(1);
        moved = moved.max(g.position().distance(initial));
        if moved > 5.0 {
            break;
        }
    }
    assert!(
        moved > 5.0,
        "a lone driver left within a bounded time: {moved}; {}",
        g.diagnostics()
    );
    assert!(
        g.occupants()[2].is_some_and(|b| g.s.is_bot(b)),
        "controls were chosen"
    );
    assert_eq!(g.occupants()[0], None, "no phantom gunner");
}

#[test]
fn a_foreign_vehicle_outside_the_bots_minigame_is_not_an_opportunity() {
    let mut g = Game::new(ROVER, 2, 1.0, true);
    let mut nearest = f32::MAX;
    for _ in 0..120 * 12 {
        g.steps(1);
        assert!(
            g.occupants().iter().all(Option::is_none),
            "authority applies to bots"
        );
        for player in g.s.snapshot().players {
            if g.s.is_bot(player.owner) {
                nearest = nearest.min(Vec3::from(player.feet).distance(ENEMY));
            }
        }
    }
    assert!(
        nearest < 8.0,
        "bots remained active and pursued instead: {nearest}"
    );
}

#[test]
fn kind_policy_can_disable_environmental_interactions() {
    let mut g = Game::new(ROVER, 2, 0.0, false);
    for _ in 0..120 * 10 {
        g.steps(1);
        assert!(
            g.occupants().iter().all(Option::is_none),
            "package policy decides use"
        );
    }
}

#[test]
fn removing_the_vehicle_returns_its_crew_to_walking() {
    let mut g = Game::new(ROVER, 2, 1.0, false);
    let (driver, gunner) = g.crew(2, 0);
    g.send(g.human, Command::Admin(Request::new(Action::ClearVehicles)));
    assert!(g.s.vehicle_infos().is_empty());
    assert_eq!(g.s.mounted(driver), None);
    assert_eq!(g.s.mounted(gunner), None);
    let before =
        g.s.snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == driver)
            .unwrap()
            .feet;
    g.steps(120 * 3);
    let after =
        g.s.snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == driver)
            .unwrap()
            .feet;
    assert!(
        Vec3::from(before).distance(after.into()) > 0.5,
        "driver resumed its foot motor"
    );
}

#[test]
fn a_dead_driver_releases_its_seat_without_removing_the_living_gunner() {
    let mut g = Game::new(ROVER, 2, 1.0, false);
    let (driver, gunner) = g.crew(2, 0);
    g.send(driver, Command::Suicide);
    assert_eq!(
        g.s.mounted(driver),
        None,
        "death releases occupancy immediately"
    );
    assert_ne!(g.occupants()[2], Some(driver));
    assert!(
        g.s.vitals()[&gunner].alive,
        "the other crew member survives"
    );
    g.steps(120 * 2);
    assert!(
        g.s.vitals()[&gunner].alive,
        "crew survives a driver casualty"
    );
}

#[test]
fn a_human_occupant_wins_over_intentions_and_hostile_bots_do_not_join_their_crew() {
    let mut g = Game::new(ROVER, 2, 1.0, false);
    // The bots are walking toward opportunities. The player uses the real
    // admin drop command, then lands on the chassis through normal contact.
    g.steps(15);
    g.send(
        g.human,
        Command::DropPlayerAtCamera(Some(bri_sim::session::CameraView {
            eye: (g.position() + Vec3::Y * 4.0).to_array(),
            yaw: 0.0,
            pitch: 0.0,
        })),
    );
    for _ in 0..120 {
        g.steps(1);
        if g.s.mounted(g.human).is_some() {
            break;
        }
    }
    assert_eq!(
        g.s.mounted(g.human),
        Some((g.vehicle, 0)),
        "human landed in first free seat"
    );
    g.send(g.human, Command::SwitchSeat(1));
    g.send(g.human, Command::SwitchSeat(1));
    assert_eq!(
        g.s.mounted(g.human),
        Some((g.vehicle, 2)),
        "player chose the controls"
    );
    for _ in 0..120 * 5 {
        g.steps(1);
        assert_eq!(
            g.s.mounted(g.human),
            Some((g.vehicle, 2)),
            "a bot cannot evict the player"
        );
        assert_eq!(
            g.occupants(),
            vec![None, None, Some(g.human)],
            "hostile occupants make the rest of this chassis unsuitable"
        );
    }
}

/// A tiny real Add-On exercising the same add/rest commands game rules use.
/// The directory exists only while the catalog reads its own invented data.
fn rules_catalog() -> Arc<bri_package_runtime::Catalog> {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Temp(std::env::temp_dir().join(format!(
        "bri-bot-interactions-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("crew-test");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "crew-test", "version": "1.0.0", "api": 1,
        "name": "Crew test rules", "license": "CC0-1.0", "capabilities": ["bots", "player", "minigame"],
        "provides": [
            {"kind": "behaviour", "id": "crew-test:behaviour/main", "file": "behaviour.json"},
            {"kind": "script", "id": "crew-test:script/main", "file": "main.rhai"}
        ]
    });
    let behaviour = json!({
        "schema_version": 1, "script": "main.rhai",
        "commands": [{"name": "add", "args": []}, {"name": "rest", "args": ["int", "bool"]},
            {"name": "teams", "args": []}, {"name": "arm", "args": ["int"]}]
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
    std::fs::write(
        dir.join("main.rhai"),
        r#"
fn cmd_add(p) { add_bot(player(p).minigame, #{ kind: "bot.blockhead", name: "Rules Driver" }); }
fn cmd_rest(p, b, on) { rest_bot(b, on); }
fn cmd_teams(p) { set_teams(player(p).minigame, [#{ name: "Crew", color: 0 }]); }
fn cmd_arm(p, b) { give_item(b, "v20.weapon.gunitem", false); }
"#,
    )
    .unwrap();
    Arc::new(
        bri_package_runtime::Catalog::load(
            &root.0,
            &PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: "crew-test".into(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: "crew-test".into(),
                    role: None,
                }],
            },
            true,
        )
        .unwrap(),
    )
}

fn rules_command(name: &str, args: Vec<PackageArg>) -> Command {
    Command::Package(PackageCommand {
        package: "crew-test".into(),
        command: name.into(),
        args,
    })
}

#[test]
fn rules_rest_releases_a_driver_and_resume_can_make_a_new_decision() {
    let mut s = session(1.0);
    s.install_packages(rules_catalog(), None).unwrap();
    let mut g = Game::with_session(s, ROVER, 0, false);
    g.s.set_spawn_points(vec![Vec3::new(-37.0, 0.05, 28.0)])
        .unwrap();
    g.send(g.human, rules_command("add", vec![]));
    g.steps(2);
    let bot = g.bots()[0];
    for _ in 0..120 * 12 {
        g.steps(1);
        if g.s.mounted(bot).is_some() {
            break;
        }
    }
    assert_eq!(
        g.s.mounted(bot),
        Some((g.vehicle, 2)),
        "rules bot chose controls"
    );
    g.send(
        g.human,
        rules_command(
            "rest",
            vec![PackageArg::Int(bot as i64), PackageArg::Bool(true)],
        ),
    );
    g.steps(120);
    assert_eq!(
        g.s.mounted(bot),
        None,
        "rest relinquishes control and occupancy"
    );
    let rested =
        g.s.snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == bot)
            .unwrap()
            .feet;
    g.steps(120 * 3);
    let now =
        g.s.snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == bot)
            .unwrap()
            .feet;
    assert!(
        Vec3::from(rested).distance(now.into()) < 0.1,
        "rested brain remains still"
    );
    assert!(g.occupants().iter().all(Option::is_none));
    g.send(
        g.human,
        rules_command(
            "rest",
            vec![PackageArg::Int(bot as i64), PackageArg::Bool(false)],
        ),
    );
    for _ in 0..120 * 12 {
        g.steps(1);
        if g.s.mounted(bot).is_some() {
            break;
        }
    }
    assert_eq!(
        g.s.mounted(bot),
        Some((g.vehicle, 2)),
        "resuming makes a fresh legal choice"
    );
    assert!(
        g.s.package_diagnostics().is_empty(),
        "rules commands completed"
    );
}

#[test]
fn a_solid_sight_transparent_barrier_does_not_turn_reach_into_boarding() {
    let mut g = Game::with_layout(
        session(1.0),
        ROVER,
        &[Vec3::new(-33.5, 0.1, 29.0)],
        false,
        true,
    );
    let bot = g.bots()[0];
    let mut physically_near = false;
    for _ in 0..120 * 8 {
        g.steps(1);
        assert!(
            g.occupants().iter().all(Option::is_none),
            "a colliding wall cannot be crossed by boarding through it"
        );
        let feet = Vec3::from(
            g.s.snapshot()
                .players
                .into_iter()
                .find(|p| p.owner == bot)
                .unwrap()
                .feet,
        );
        let pose =
            g.s.vehicle_poses()
                .into_iter()
                .find(|v| v.id == g.vehicle)
                .unwrap();
        let seat =
            Vec3::from(pose.position) + Quat::from_array(pose.rotation) * Vec3::new(0.0, 1.4, -1.0);
        if feet.distance(seat) < 4.0 && feet.x < -32.5 {
            physically_near = true;
            let eye = feet + Vec3::Y * 1.6;
            assert!(
                g.s.simulation().sight(eye, seat, 8.0).is_some(),
                "this is visible, not a hidden opportunity"
            );
            let delta = seat - eye;
            assert!(
                g.s.simulation()
                    .target_bricks_always(eye, delta.normalize(), delta.length())
                    .unwrap()
                    .is_some(),
                "the solid wall actually crosses the boarding segment"
            );
        }
    }
    assert!(
        physically_near,
        "the body reached mounting distance outside the enclosure"
    );
}

#[test]
fn a_charged_mounted_weapon_is_held_until_ready_then_released_and_rearmed() {
    let mut g = Game::new(CHARGED, 2, 1.0, false);
    let (_, gunner) = g.crew(2, 0);
    let mut shot_ids = std::collections::BTreeSet::new();
    let mut first_shot = None;
    let mut heard_charge = false;
    let mut charge_trace = Vec::new();
    for tick in 1..=120 * 10 {
        g.steps(1);
        for (owner, notice) in g.s.take_private_notices() {
            if owner == gunner && matches!(notice, bri_sim::session::Notice::Bottom { .. }) {
                heard_charge = true;
                if charge_trace.len() < 16 {
                    charge_trace.push((tick, format!("{notice:?}")));
                }
            }
        }
        for p in g.s.weapon_view().fired() {
            if p.source.0 == gunner
                && p.definition == bri_weapons::testing::GUN_PROJECTILE
                && shot_ids.insert(p.id)
                && first_shot.is_none()
            {
                first_shot = Some(tick);
            }
        }
        if shot_ids.len() >= 2 {
            break;
        }
    }
    assert!(heard_charge, "the real mounted weapon reported charging");
    assert!(
        first_shot
            .is_some_and(|tick| tick >= (CHARGE_TICKS * u64::from(CHARGE_STEPS - 1)) as usize),
        "the gunner held for the weapon's authored charge stages: {first_shot:?}; charge notices {charge_trace:?}; {}",
        g.diagnostics()
    );
    assert!(
        shot_ids.len() >= 2,
        "the gunner released the charged gun and began another shot: {shot_ids:?}; {}",
        g.diagnostics()
    );
}

#[test]
fn an_armed_bot_passenger_aims_in_world_space_on_a_rotated_stationary_chassis() {
    let mut s = session(1.0);
    s.install_packages(rules_catalog(), None).unwrap();
    let mut g = Game::with_session(s, ROVER, 2, false);
    g.send(g.human, rules_command("teams", vec![]));
    g.steps(1);
    let game = g.s.minigame_views()[0].id;
    let team = g.s.minigame_views()[0].teams[0].id.0;
    for ally in [g.human].into_iter().chain(g.bots()) {
        g.send(
            g.human,
            Command::MiniGame(MiniGameRequest::SetTeam {
                game,
                target: ally,
                team: Some(team),
            }),
        );
    }
    // With no enemy yet, the human takes the controls through ordinary
    // contact and seat-switch commands. The bots must fill their own roles.
    g.send(
        g.human,
        Command::DropPlayerAtCamera(Some(bri_sim::session::CameraView {
            eye: (g.position() + Vec3::Y * 4.0).to_array(),
            yaw: 0.0,
            pitch: 0.0,
        })),
    );
    for _ in 0..120 {
        g.steps(1);
        if g.s.mounted(g.human).is_some() {
            break;
        }
    }
    assert_eq!(g.s.mounted(g.human), Some((g.vehicle, 0)));
    g.send(g.human, Command::SwitchSeat(1));
    g.send(g.human, Command::SwitchSeat(1));
    assert_eq!(g.s.mounted(g.human), Some((g.vehicle, 2)));
    g.send(
        g.human,
        Command::DropPlayerAtCamera(Some(bri_sim::session::CameraView {
            eye: g.position().to_array(),
            yaw: 1.1,
            pitch: 0.0,
        })),
    );
    // Jump is the normal wheeled-vehicle brake; it holds the human's
    // chassis against approaching actors without altering its world pose.
    let brake = MoveInput {
        jump: true,
        ..Default::default()
    };
    g.steps_with_input(120, brake);
    let target = g.s.join("Hostile target".into(), ENEMY, false).unwrap();
    g.send(target, Command::MiniGame(MiniGameRequest::Join { game }));
    let mut passenger = None;
    for _ in 0..120 * 15 {
        g.steps_with_input(1, brake);
        if let Some(bot) = g.occupants()[1].filter(|b| g.s.is_bot(*b)) {
            passenger = Some(bot);
            if g.occupants()[0].is_some_and(|gunner| g.s.is_bot(gunner)) {
                break;
            }
        }
    }
    let passenger =
        passenger.unwrap_or_else(|| panic!("a bot chose the passenger role; {}", g.diagnostics()));
    assert!(
        g.occupants()[0].is_some_and(|bot| g.s.is_bot(bot)),
        "the other bot filled the actual gunner seat; {}",
        g.diagnostics()
    );
    // Rules give the seated passenger a weapon. Its ordinary brain equips,
    // aims and fires it; the test never supplies bot controls or a trigger.
    g.send(
        g.human,
        rules_command("arm", vec![PackageArg::Int(passenger as i64)]),
    );
    g.steps_with_input(120, brake);
    let initial = g.position();
    let mut aimed = false;
    let mut seen_shots: std::collections::BTreeSet<_> =
        g.s.weapon_view().fired().map(|shot| shot.id).collect();
    for _ in 0..120 * 10 {
        g.steps_with_input(1, brake);
        let pose =
            g.s.vehicle_poses()
                .into_iter()
                .find(|v| v.id == g.vehicle)
                .unwrap();
        let forward = Quat::from_array(pose.rotation) * Vec3::NEG_Z;
        assert!(
            forward.x.abs() > 0.5,
            "the chassis has a materially rotated heading"
        );
        assert!(
            g.position().distance(initial) < 0.5,
            "the human driver holds the chassis still: initial {initial}, now {}; {}",
            g.position(),
            g.diagnostics()
        );
        assert_eq!(g.s.mounted(g.human), Some((g.vehicle, 2)));
        if g.occupants()[1] == Some(passenger) {
            let target_feet = Vec3::from(
                g.s.snapshot()
                    .players
                    .into_iter()
                    .find(|p| p.owner == target)
                    .unwrap()
                    .feet,
            );
            for shot in
                g.s.weapon_view()
                    .fired()
                    .filter(|p| p.source.0 == passenger)
            {
                if !seen_shots.insert(shot.id) {
                    continue;
                }
                assert_eq!(shot.definition, bri_weapons::testing::GUN_PROJECTILE);
                let toward = (target_feet + Vec3::Y - shot.origin).normalize();
                if shot.velocity.normalize().dot(toward) > 0.95
                    && Vec3::from(pose.velocity).length() < 0.5
                {
                    aimed = true;
                }
            }
        }
        if aimed {
            break;
        }
    }
    assert!(
        aimed,
        "the passenger's real hand projectile follows world aim while the chassis is stationary; {}",
        g.diagnostics()
    );
    assert!(g.s.package_diagnostics().is_empty());
}

#[test]
fn a_driver_brakes_for_a_grounded_ally_before_forward_or_reverse_contact() {
    for (target, direction) in [
        (ENEMY, Vec3::NEG_Z),
        (Vec3::new(-30.0, 0.05, 55.0), Vec3::Z),
    ] {
        let mut s = session(1.0);
        s.install_packages(rules_catalog(), None).unwrap();
        s.set_spawn_points(vec![target]).unwrap();
        let mut g = Game::with_session(s, ROVER, 1, false);
        let bot = g.bots()[0];
        let friend =
            g.s.join("Crewmate".into(), Vec3::new(90.0, 0.05, 90.0), true)
                .unwrap();
        g.send(g.human, rules_command("teams", vec![]));
        g.steps(1);
        let game = g.s.minigame_views()[0].id;
        let team = g.s.minigame_views()[0].teams[0].id.0;
        for teammate in [bot, friend] {
            g.send(
                g.human,
                Command::MiniGame(MiniGameRequest::SetTeam {
                    game,
                    target: teammate,
                    team: Some(team),
                }),
            );
        }
        for _ in 0..120 * 12 {
            g.steps(1);
            if g.s.mounted(bot).is_some() {
                break;
            }
        }
        assert_eq!(g.s.mounted(bot), Some((g.vehicle, 2)), "driver boarded");
        let initial = g.position();
        let friend_at = initial + direction * 3.2;
        // Drop the human onto the ground outside the hull, rather than on
        // its roof: boarding and physical contact cannot explain a stop.
        g.send(
            friend,
            Command::DropPlayerAtCamera(Some(bri_sim::session::CameraView {
                eye: [friend_at.x, 1.65, friend_at.z],
                yaw: 0.0,
                pitch: 0.0,
            })),
        );
        let mut advance = 0.0f32;
        for _ in 0..60 {
            g.steps(1);
            advance = advance.max((g.position() - initial).dot(direction));
            assert_eq!(
                g.s.mounted(friend),
                None,
                "the ally stands outside the chassis"
            );
        }
        assert!(
            advance < 0.5,
            "driver advanced toward its {direction} ally: {advance}"
        );
        let friend_now = Vec3::from(
            g.s.snapshot()
                .players
                .into_iter()
                .find(|p| p.owner == friend)
                .unwrap()
                .feet,
        );
        assert!(
            Vec3::new(friend_now.x - friend_at.x, 0.0, friend_now.z - friend_at.z).length() < 0.35,
            "the car stopped before physically pushing its ally"
        );
        assert_eq!(g.s.vitals()[&friend].health, 100.0);
        g.send(
            friend,
            Command::DropPlayerAtCamera(Some(bri_sim::session::CameraView {
                eye: [friend_at.x - 20.0, 1.65, friend_at.z],
                yaw: 0.0,
                pitch: 0.0,
            })),
        );
        let stopped = g.position();
        let mut moved = 0.0f32;
        for _ in 0..120 * 3 {
            g.steps(1);
            moved = moved.max(g.position().distance(stopped));
        }
        assert!(
            moved > 2.0,
            "driver resumed pursuit after the ally cleared: {moved}; {}",
            g.diagnostics()
        );
        assert!(g.s.package_diagnostics().is_empty());
    }
}

#[test]
fn an_unarmed_chassis_pursues_and_hurts_by_physical_runover() {
    let target = Vec3::new(-29.75, 0.05, 8.0);
    let mut s = session(1.0);
    s.set_spawn_points(vec![target]).unwrap();
    let mut g = Game::with_session(s, UNARMED, 1, false);
    let bot = g.bots()[0];
    let mut contact = None;
    for _ in 0..120 * 18 {
        g.steps(1);
        assert_eq!(
            g.s.weapon_view().fired().count(),
            0,
            "this chassis has no gun"
        );
        if g.s.vitals()[&g.human].health < 100.0 {
            let human = Vec3::from(
                g.s.snapshot()
                    .players
                    .into_iter()
                    .find(|p| p.owner == g.human)
                    .unwrap()
                    .feet,
            );
            contact = Some(g.position().distance(human));
            assert_eq!(
                g.s.mounted(bot),
                Some((g.vehicle, 2)),
                "damage happened while operating the actual chassis"
            );
            break;
        }
    }
    assert!(
        contact.is_some_and(|distance| distance < 4.0),
        "an unarmed driver reached physical contact and runover damage: {contact:?}; {}",
        g.diagnostics()
    );
}

#[test]
fn a_chassis_replans_or_stops_at_a_gap_only_a_pedestrian_can_fit() {
    let target = Vec3::new(-29.75, 0.05, 8.0);
    let mut s = session(1.0);
    s.set_spawn_points(vec![target]).unwrap();
    let mut g = Game::with_session(s, UNARMED, 1, false);
    let bot = g.bots()[0];
    let initial = g.position();
    for _ in 0..120 * 12 {
        g.steps(1);
        if g.s.mounted(bot).is_some() && initial.distance(g.position()) > 3.0 {
            break;
        }
    }
    assert_eq!(g.s.mounted(bot), Some((g.vehicle, 2)));
    assert!(
        initial.distance(g.position()) > 3.0,
        "driver was pursuing before the change"
    );
    let wall_z = ((g.position().z - 4.0) / 0.5).floor() * 0.5 + 0.25;
    let mut world = World::new("Narrow gap".into(), "chaos/map".into(), vec![[1.0; 4]]);
    world.owners = g.s.simulation().state().owners.clone();
    // The gap x=-31..-28.5 is 2.5 units wide, enough for a Blockhead but
    // smaller than this chassis's authored 2.6-unit width in every turn.
    for x in (0..39)
        .map(|i| -50.25 + i as f32 * 0.5)
        .chain((0..38).map(|i| -28.25 + i as f32 * 0.5))
    {
        for y in [1.5, 4.5] {
            let id = world.bricks.len() as u64 + 1;
            world.bricks.insert(
                id,
                Brick::new(
                    ContentRef::Resolved(fixture::TALL.into()),
                    [x, y, wall_z],
                    g.human,
                ),
            );
        }
    }
    world.next_brick_id = world.bricks.keys().next_back().copied().unwrap() + 1;
    let count = world.bricks.len();
    let before = g.s.simulation().state().bricks.len();
    g.send(
        g.human,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: true,
        },
    );
    g.steps(5);
    assert_eq!(
        g.s.simulation().state().bricks.len(),
        before + count,
        "new geometry was placed"
    );
    let mut last = g.position();
    let mut relinquished = false;
    for _ in 0..120 * 12 {
        g.steps(1);
        let now = g.position();
        relinquished |= g.s.mounted(bot).is_none();
        if last.z >= wall_z && now.z < wall_z {
            assert!(
                now.x < -51.0 || now.x > -8.75,
                "the chassis crossed outside a wall end, never through its narrow opening: {now}; {}",
                g.diagnostics()
            );
        }
        assert!(
            now.y < 2.0,
            "the grounded chassis cannot use pedestrian jumps or jets"
        );
        last = now;
    }
    assert!(
        last.z < wall_z || relinquished,
        "a chassis that could not route around the wall relinquished its driver within a bounded time; {}",
        g.diagnostics()
    );
}

/// The gunner may fire while the independent driver pursues a retreating
/// visible human. All movement, boarding and firing are native controls.
#[test]
fn an_armed_crew_advances_while_its_visible_target_retreats() {
    for (kind, driver_seat, gunner_seat) in [(ROVER, 2, 0), (CARRIER, 1, 2)] {
        let mut g = Game::new(kind, 2, 1.0, false);
        let (driver, gunner) = g.crew(driver_seat, gunner_seat);
        let chassis = g.position();
        let human = Vec3::from(
            g.s.snapshot()
                .players
                .iter()
                .find(|p| p.owner == g.human)
                .unwrap()
                .feet,
        );
        let away = Vec3::new(human.x - chassis.x, 0.0, human.z - chassis.z).normalize();
        let input = MoveInput {
            forward: 1.0,
            yaw: away.x.atan2(-away.z),
            ..Default::default()
        };
        let mut requested = 0;
        let mut shots = std::collections::BTreeSet::new();
        for _ in 0..120 * 4 {
            g.steps_with_input(1, input);
            let thought =
                g.s.bot_thoughts()
                    .into_iter()
                    .find(|t| t.bot == driver)
                    .unwrap();
            requested += usize::from(
                thought.visible == Some(g.human)
                    && thought.behaviour == "chase"
                    && thought.goal.is_some(),
            );
            for shot in g.s.weapon_view().fired().filter(|p| p.source.0 == gunner) {
                shots.insert(shot.id);
            }
        }
        let after = Vec3::from(
            g.s.snapshot()
                .players
                .iter()
                .find(|p| p.owner == g.human)
                .unwrap()
                .feet,
        );
        let human_retreat = (after - human).dot(away);
        let advance = (g.position() - chassis).dot(away);
        eprintln!(
            "{kind} retreat: human_retreat={human_retreat:.3} chassis_advance={advance:.3} requested_driver_ticks={requested} observed_mounted_projectiles={}",
            shots.len()
        );
        assert!(
            human_retreat > 8.0,
            "human actually retreated {human_retreat}; {}",
            g.diagnostics()
        );
        assert!(
            requested > 120,
            "driver requested visible pursuit; {}",
            g.diagnostics()
        );
        assert!(
            advance > 3.0,
            "ordinary wheels advanced {kind}: {advance}; {}",
            g.diagnostics()
        );
        assert!(
            !shots.is_empty(),
            "actual gunner fired during pursuit; {}",
            g.diagnostics()
        );
        assert_eq!(g.s.mounted(driver), Some((g.vehicle, driver_seat as u8)));
        assert_eq!(g.s.mounted(gunner), Some((g.vehicle, gunner_seat as u8)));
    }
}

/// Different unseen target transforms cannot steer the same chassis search.
/// Ordinary admin DropPlayerAtCamera creates the paired human hiding setup;
/// the bots themselves still choose and execute normal wheel controls.
#[test]
fn a_crew_pursues_only_the_last_observation_after_its_target_hides() {
    for (kind, driver_seat, gunner_seat) in [(ROVER, 2, 0), (CARRIER, 1, 2)] {
        let mut a = Game::new(kind, 2, 1.0, false);
        let mut b = Game::new(kind, 2, 1.0, false);
        let (driver, _) = a.crew(driver_seat, gunner_seat);
        let (other_driver, _) = b.crew(driver_seat, gunner_seat);
        assert_eq!(driver, other_driver);
        a.steps(60);
        b.steps(60);
        let old =
            a.s.bot_thoughts()
                .into_iter()
                .find(|t| t.bot == driver)
                .unwrap()
                .remembered
                .expect("driver really observed the human");
        let before = a.position();
        let toward =
            Vec3::new(old.position[0] - before.x, 0.0, old.position[2] - before.z).normalize();
        for (g, x) in [(&mut a, -150.0), (&mut b, 150.0)] {
            g.send(
                g.human,
                Command::DropPlayerAtCamera(Some(bri_sim::session::CameraView {
                    eye: [x, 1.65, -150.0],
                    yaw: 0.0,
                    pitch: 0.0,
                })),
            );
        }
        let mut searches = 0;
        for _ in 0..120 * 2 {
            a.steps(1);
            b.steps(1);
            let one =
                a.s.bot_thoughts()
                    .into_iter()
                    .find(|t| t.bot == driver)
                    .unwrap();
            let two =
                b.s.bot_thoughts()
                    .into_iter()
                    .find(|t| t.bot == driver)
                    .unwrap();
            assert_eq!(one.visible, None, "human is genuinely unseen");
            assert_eq!(two.visible, None, "paired human is genuinely unseen");
            for thought in [&one, &two] {
                let evidence = thought.remembered.expect("dated observation remains valid");
                assert_eq!(evidence.position, old.position);
                assert_eq!(evidence.observed, old.observed);
                assert_eq!(evidence.expires, old.expires);
            }
            assert_eq!(
                one.goal, two.goal,
                "hidden transforms cannot change requested travel"
            );
            assert_eq!(one.next, two.next);
            assert!(
                a.position().distance(b.position()) < 0.001,
                "actual chassis trajectory ignores unseen transform"
            );
            searches += usize::from(one.behaviour == "search" && one.goal.is_some());
        }
        assert!(
            searches > 60,
            "driver actually requested last-observed search; {}",
            a.diagnostics()
        );
        let advance = (a.position() - before).dot(toward);
        eprintln!(
            "{kind} hidden: chassis_advance_to_observation={advance:.3} search_with_goal_ticks={searches} fixed_evidence={old:?}"
        );
        assert!(
            advance > 1.0,
            "native chassis moves toward dated observation {advance}; {}",
            a.diagnostics()
        );
        assert_eq!(a.s.mounted(driver), Some((a.vehicle, driver_seat as u8)));
    }
}

/// A bot from a brick at the origin boards a chassis about 40 units out
/// and pursues a visible human about 60 units out who then runs away. The
/// mount's own pursuit leash (`mounted` in the kind) keeps the chase going
/// past the on-foot chase radius, and a target behind the hull is turned
/// toward rather than driven at in reverse.
#[test]
fn a_mounted_driver_keeps_closing_on_a_retreating_target_beyond_its_walking_leash() {
    for (human_at, label) in [
        (Vec3::new(-44.0, 0.05, 44.0), "diagonal"),
        (Vec3::new(-52.0, 0.05, 30.0), "renamed-flank"),
    ] {
        let mut s = session(1.0);
        s.set_spawn_points(vec![human_at]).unwrap();
        let mut g = Game::with_layout(s, PURSUER, &[Vec3::new(0.0, 0.1, 0.0)], false, false);
        let bot = g.bots()[0];
        let home = Vec3::new(0.25, 0.0, 0.25);
        let mut boarded = false;
        for _ in 0..120 * 20 {
            g.steps(1);
            if g.s.mounted(bot) == Some((g.vehicle, 2)) {
                boarded = true;
                break;
            }
        }
        assert!(
            boarded,
            "the bot boarded the chassis to pursue ({label}); {}",
            g.diagnostics()
        );
        let human = |g: &Game| {
            Vec3::from(
                g.s.snapshot()
                    .players
                    .into_iter()
                    .find(|p| p.owner == g.human)
                    .unwrap()
                    .feet,
            )
        };
        let start_chassis = g.position();
        let start_human = human(&g);
        let away = Vec3::new(start_human.x, 0.0, start_human.z).normalize();
        let input = MoveInput {
            forward: 1.0,
            yaw: away.x.atan2(-away.z),
            ..Default::default()
        };
        let gap = |g: &Game| {
            let d = human(g) - g.position();
            Vec3::new(d.x, 0.0, d.z).length()
        };
        let initial_gap = gap(&g);
        let mut beyond_walking_leash = false;
        let mut reversing = 0usize;
        let mut far_ticks = 0usize;
        let mut trace = Vec::new();
        let mut last = g.position();
        for tick in 0..120 * 8 {
            g.steps_with_input(1, input);
            let thought =
                g.s.bot_thoughts()
                    .into_iter()
                    .find(|t| t.bot == bot)
                    .unwrap();
            let now = g.position();
            let flat_home = Vec3::new(now.x - home.x, 0.0, now.z - home.z).length();
            beyond_walking_leash |= flat_home > 52.0;
            if tick % 60 == 0 && trace.len() < 20 {
                trace.push((tick, thought.behaviour, now, gap(&g)));
            }
            assert!(
                !matches!(thought.behaviour, "return" | "wander"),
                "the driver kept pursuing its visible target instead of heading home ({label}) at {tick}: {thought:?}; trace={trace:?}"
            );
            assert_eq!(
                g.s.mounted(bot),
                Some((g.vehicle, 2)),
                "still driving ({label})"
            );
            let pose =
                g.s.vehicle_poses()
                    .into_iter()
                    .find(|v| v.id == g.vehicle)
                    .unwrap();
            let forward = Quat::from_array(pose.rotation) * Vec3::NEG_Z;
            let step = now - last;
            last = now;
            if gap(&g) > 12.0 {
                far_ticks += 1;
                reversing += usize::from(Vec3::new(step.x, 0.0, step.z).dot(forward) < -0.01);
            }
        }
        let final_gap = gap(&g);
        let retreat = (human(&g) - start_human).dot(away);
        eprintln!(
            "{label}: human_retreat={retreat:.2} gap {initial_gap:.2}->{final_gap:.2} reversing={reversing}/{far_ticks} chassis {start_chassis}->{}",
            g.position()
        );
        assert!(
            retreat > 20.0,
            "the human genuinely retreated ({label}): {retreat}"
        );
        assert!(
            beyond_walking_leash,
            "the pursuit went past the on-foot chase radius ({label}); trace={trace:?}"
        );
        assert!(
            final_gap < initial_gap - 5.0,
            "the chassis closed on its retreating target ({label}): {initial_gap} -> {final_gap}; trace={trace:?}"
        );
        assert!(
            reversing * 10 <= far_ticks,
            "a far target is turned toward, not chased in reverse ({label}): {reversing}/{far_ticks}"
        );
    }
}

#[test]
fn a_drivers_free_seats_are_read_from_the_vehicles_own_seat_data() {
    // The carrier is invented content: its seats, their order and roles are
    // only data. A bot at its controls publishes its free seats, and a bot
    // weighing a seat on it scores that as using what an ally exposes.
    let mut g = Game::new(CARRIER, 3, 1.0, false);
    let mut used = false;
    for _ in 0..120 * 15 {
        g.steps(1);
        used = g.s.bot_thoughts().iter().any(|t| {
            t.team
                .terms
                .iter()
                .any(|(option, _, uses, _)| *option == "interact" && *uses > 0.0)
        });
        if used {
            break;
        }
    }
    assert!(used, "no seat offer was read: {}", g.diagnostics());
}
