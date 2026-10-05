//! Public-session checks for evidence, hostility and weapon understanding.
//! Fixtures are invented; diagnostic thoughts are checked against physical
//! movement and successful combat, never used to assign a bot's decisions.
use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::{
    bot_kind::{BotHold, BotPack},
    player::MoveInput,
    session::{
        ActionAim, BotTask, BotThought, CameraView, Command, MiniGameRequest, PackageArg,
        PackageCommand, Session, ToolCatalog,
    },
};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const HOME: Vec3 = Vec3::new(-40.0, 0.1, 30.0);
const SEEN: Vec3 = Vec3::new(-40.0, 0.05, 48.0);
const HIDDEN: Vec3 = Vec3::new(-80.0, 0.05, 90.0);

fn session(memory: f32, hitscan: bool) -> Session {
    let mut s = fixture::synthetic().unwrap().session;
    let mut kinds = BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots;
    let kind = &mut kinds[0];
    kind.sight = 32.0;
    kind.chase_radius = 128.0;
    kind.wander_radius = 0.0;
    kind.memory_seconds = memory;
    kind.alerts_allies = false;
    kind.reaction_seconds = 0.0;
    kind.aim_error_degrees = 0.0;
    kind.turn_degrees = 720.0;
    kind.behaviours.insert("interact".into(), 0.0);
    kind.behaviours.insert("fly".into(), 0.0);
    if hitscan {
        // The ordinary projectile would reach less than one unit. A guard
        // must understand the ray's range rather than chase into melee range.
        kind.behaviours.insert("chase".into(), 0.0);
        let mut pack = bri_weapons::testing::pack();
        let image = pack
            .images
            .get_mut(bri_weapons::testing::GUN_IMAGE)
            .unwrap();
        image.bot = None;
        image.shot = Some(bri_weapons::Shot {
            hitscan: Some(
                serde_json::from_value(json!({
                    "range": 48.0, "from_eye": true, "damage": 7.0
                }))
                .unwrap(),
            ),
            ..bri_weapons::Shot::SINGLE
        });
        let projectile = pack
            .projectiles
            .get_mut(bri_weapons::testing::GUN_PROJECTILE)
            .unwrap();
        projectile.speed = 1.0;
        projectile.lifetime_ticks = 1;
        s.set_weapon_pack(pack).unwrap();
    }
    s.set_bot_kinds(kinds).unwrap();
    s.set_tool_catalog(ToolCatalog {
        vehicles: [fixture::BOT.to_string()].into(),
        vehicle_bricks: [fixture::PLATE.to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    s.set_spawn_points(vec![SEEN]).unwrap();
    s
}

fn feet(s: &Session, owner: OwnerId) -> Vec3 {
    Vec3::from(
        s.snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == owner)
            .unwrap()
            .feet,
    )
}

fn thought(s: &Session, bot: OwnerId) -> BotThought {
    s.bot_thoughts().into_iter().find(|b| b.bot == bot).unwrap()
}

struct Game {
    s: Session,
    human: OwnerId,
    bot: OwnerId,
    sequence: u64,
}
impl Game {
    fn new(memory: f32, hitscan: bool) -> Self {
        Self::setup(
            memory,
            hitscan,
            false,
            hitscan.then_some(bri_weapons::testing::GUN_ITEM),
        )
    }

    fn with_rules(memory: f32, hitscan: bool) -> Self {
        Self::setup(
            memory,
            hitscan,
            true,
            hitscan.then_some(bri_weapons::testing::GUN_ITEM),
        )
    }

    fn setup(memory: f32, hitscan: bool, rules: bool, item: Option<&str>) -> Self {
        let mut s = session(memory, hitscan);
        if rules {
            s.install_packages(rules_catalog(), None).unwrap();
        }
        let human = s.join("Observed human".into(), SEEN, true).unwrap();
        let mut brick = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            (HOME + Vec3::new(0.25, 0.0, 0.25)).to_array(),
            human,
        );
        brick.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(fixture::BOT.into()),
            recolor: false,
            team: None,
        }));
        let mut world = World::new("Knowledge".into(), "chaos/map".into(), vec![[1.0; 4]]);
        world.bricks.insert(1, brick);
        world.next_brick_id = 2;
        s.command(
            human,
            1,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            },
        )
        .unwrap();
        let mut sequence = 1 << 40;
        for _ in 0..5 {
            sequence += 1;
            s.movement(human, sequence, MoveInput::default()).unwrap();
            s.step().unwrap();
        }
        assert_eq!(s.simulation().state().bricks.len(), 1);
        s.command(
            human,
            sequence + 1,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: Settings {
                    loadout: [item.map(String::from), None, None, None, None],
                    ..Default::default()
                },
            }),
        )
        .unwrap();
        sequence += 1;
        let mut g = Self {
            s,
            human,
            bot: 0,
            sequence,
        };
        g.steps(120);
        g.bot =
            *g.s.names()
                .keys()
                .find(|owner| g.s.is_bot(**owner))
                .unwrap();
        let seen = thought(&g.s, g.bot);
        assert_eq!(
            seen.visible,
            Some(human),
            "fixture establishes direct sight: {seen:?}"
        );
        assert_eq!(seen.remembered.unwrap().subject, human);
        g
    }

    fn send(&mut self, owner: OwnerId, command: Command) {
        self.sequence += 1;
        self.s.command(owner, self.sequence, command).unwrap();
    }

    fn steps(&mut self, ticks: usize) {
        for _ in 0..ticks {
            self.sequence += 1;
            self.s
                .movement(self.human, self.sequence, MoveInput::default())
                .unwrap();
            self.s.step().unwrap();
        }
    }

    fn hide(&mut self, at: Vec3) {
        self.send(
            self.human,
            Command::DropPlayerAtCamera(Some(CameraView {
                eye: [at.x, at.y + 1.6, at.z],
                yaw: 0.0,
                pitch: 0.0,
            })),
        );
        self.steps(1);
        assert!(
            feet(&self.s, self.human).distance(at) < 0.2,
            "human moved to the intended hidden location"
        );
        assert!(feet(&self.s, self.human).distance(feet(&self.s, self.bot)) > 32.0);
        assert_eq!(thought(&self.s, self.bot).visible, None);
        // The hold rule keeps a chase through a moment out of sight
        // (`Ask::paused`), still heading for where the enemy was seen; past
        // the hold it searches.
        let seen = thought(&self.s, self.bot).remembered.unwrap().position;
        let hold = (BotHold::default().seconds * 120.0) as usize;
        for _ in 0..=hold {
            let now = thought(&self.s, self.bot);
            if now.behaviour != "chase" {
                break;
            }
            assert_eq!(now.goal, Some(seen), "a held chase heads for the sighting");
            self.steps(1);
        }
        assert_eq!(thought(&self.s, self.bot).behaviour, "search");
    }
}

#[test]
fn an_unseen_enemy_keeps_its_observed_position_and_timestamp() {
    let mut a = Game::new(8.0, false);
    let mut b = Game::new(8.0, false);
    let evidence = thought(&a.s, a.bot).remembered.unwrap();
    assert!(
        (feet(&a.s, a.bot) - HOME).length() > 1.0,
        "the brain was actively chasing before sight was lost"
    );
    a.hide(HIDDEN);
    b.hide(Vec3::new(-10.0, 0.05, 90.0));
    let start = feet(&a.s, a.bot);
    for _ in 0..120 {
        for g in [&mut a, &mut b] {
            g.steps(1);
            let now = thought(&g.s, g.bot);
            assert_eq!(now.visible, None);
            let remembered = now.remembered.expect("valid dated memory is retained");
            assert_eq!(remembered.position, evidence.position);
            assert_eq!(
                remembered.observed, evidence.observed,
                "relay/search does not manufacture a new observation"
            );
            assert_eq!(remembered.expires, evidence.expires);
            assert_eq!(now.behaviour, "search");
            if now.goal.is_some() {
                assert_eq!(now.goal, Some(evidence.position));
            } else {
                assert!(
                    feet(&g.s, g.bot).distance(Vec3::from(evidence.position)) < 1.6,
                    "looking around is allowed only after reaching the remembered place"
                );
            }
        }
        assert!(
            (feet(&a.s, a.bot) - feet(&b.s, b.bot)).length() < 0.001,
            "different hidden transforms must not change the same brain's search"
        );
    }
    assert!(
        feet(&a.s, a.bot).distance(Vec3::from(evidence.position))
            < start.distance(Vec3::from(evidence.position)) - 1.0,
        "remembering results in travel to the observation, not merely a diagnostic entry"
    );
}

#[test]
fn expired_evidence_stops_searching_without_refresh_from_the_hidden_actor() {
    let mut g = Game::new(1.0, false);
    let old = thought(&g.s, g.bot).remembered.unwrap();
    g.hide(HIDDEN);
    let before = thought(&g.s, g.bot);
    assert_eq!(before.behaviour, "search");
    assert_eq!(before.goal, Some(old.position));
    while g.s.simulation().state().tick <= old.expires {
        g.steps(1);
    }
    let after = thought(&g.s, g.bot);
    assert!(after.remembered.is_none(), "expired evidence: {after:?}");
    assert!(after.task.is_none());
    assert_ne!(after.behaviour, "search");
    assert_ne!(after.goal, Some(old.position));
}

#[test]
fn a_known_dead_subject_invalidates_a_live_search_before_its_expiry() {
    let mut g = Game::new(8.0, false);
    g.hide(HIDDEN);
    let old = thought(&g.s, g.bot).remembered.unwrap();
    assert_eq!(thought(&g.s, g.bot).behaviour, "search");
    g.send(g.human, Command::Suicide);
    g.steps(2);
    assert!(!g.s.vitals()[&g.human].alive, "subject genuinely died");
    assert!(
        g.s.simulation().state().tick < old.expires,
        "expiry cannot explain the cancellation"
    );
    let after = thought(&g.s, g.bot);
    assert!(
        after.remembered.is_none() && after.task.is_none(),
        "{after:?}"
    );
    assert_ne!(after.behaviour, "search");
    assert_ne!(after.goal, Some(old.position));
}

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
        "bri-bot-knowledge-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("knowledge-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), json!({
        "schema_version": 1, "id": "knowledge-test", "version": "1.0.0", "api": 1,
        "name": "Knowledge test", "license": "CC0-1.0", "capabilities": ["bots", "player", "minigame"],
        "provides": [
            {"kind": "behaviour", "id": "knowledge-test:behaviour/main", "file": "behaviour.json"},
            {"kind": "script", "id": "knowledge-test:script/main", "file": "main.rhai"}
        ]
    }).to_string()).unwrap();
    std::fs::write(
        dir.join("behaviour.json"),
        json!({
            "schema_version": 1, "script": "main.rhai",
            "commands": [{"name": "teams", "args": []}, {"name": "add", "args": []},
                {"name": "remove", "args": ["int"]}]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("main.rhai"),
        r#"
fn cmd_teams(p) {
    set_teams(player(p).minigame, [#{ name: "Friends", color: 0 }], #{ friendly_fire: true });
}
fn cmd_add(p) {
    add_bot(player(p).minigame, #{ kind: "bot.blockhead", name: "Rules observer" });
}
fn cmd_remove(p, bot) { remove_bot(bot); }
"#,
    )
    .unwrap();
    Arc::new(
        bri_package_runtime::Catalog::load(
            &root.0,
            &PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: "knowledge-test".into(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: "knowledge-test".into(),
                    role: None,
                }],
            },
            true,
        )
        .unwrap(),
    )
}

fn rules(name: &str) -> Command {
    Command::Package(PackageCommand {
        package: "knowledge-test".into(),
        command: name.into(),
        args: vec![],
    })
}

fn set_team(g: &mut Game, target: OwnerId, team: Option<u32>) {
    let game = g.s.minigame_views()[0].id;
    g.send(
        g.human,
        Command::MiniGame(MiniGameRequest::SetTeam { game, target, team }),
    );
}

#[test]
fn becoming_an_ally_invalidates_remembered_hostility_before_expiry() {
    let mut g = Game::with_rules(8.0, false);
    g.send(g.human, rules("teams"));
    g.steps(1);
    let team = g.s.minigame_views()[0].teams[0].id.0;
    let bot = g.bot;
    set_team(&mut g, bot, Some(team));
    g.steps(2);
    g.hide(HIDDEN);
    let old = thought(&g.s, g.bot).remembered.unwrap();
    assert_eq!(thought(&g.s, g.bot).behaviour, "search");
    let human = g.human;
    set_team(&mut g, human, Some(team));
    g.steps(2);
    assert_eq!(g.s.vitals()[&g.human].team, Some(team));
    assert_eq!(g.s.vitals()[&g.bot].team, Some(team));
    assert!(g.s.simulation().state().tick < old.expires);
    let after = thought(&g.s, g.bot);
    assert!(
        after.remembered.is_none() && after.task.is_none(),
        "{after:?}"
    );
    assert_ne!(after.behaviour, "search");
    assert_ne!(after.goal, Some(old.position));
}

#[test]
fn rules_bots_do_not_target_same_team_humans_or_bots_with_friendly_fire_on() {
    let mut g = Game::with_rules(8.0, true);
    g.send(g.human, rules("teams"));
    g.s.set_spawn_points(vec![Vec3::new(-44.0, 0.05, 32.0)])
        .unwrap();
    g.send(g.human, rules("add"));
    g.steps(1);
    let rules_bot =
        *g.s.names()
            .keys()
            .find(|o| g.s.is_bot(**o) && **o != g.bot)
            .unwrap();
    let team = g.s.minigame_views()[0].teams[0].id.0;
    for owner in [g.human, g.bot, rules_bot] {
        set_team(&mut g, owner, Some(team));
    }
    g.steps(3);
    assert!(
        feet(&g.s, rules_bot).distance(feet(&g.s, g.human)) < 32.0,
        "ally is in sensor range"
    );
    assert!(feet(&g.s, rules_bot).distance(feet(&g.s, g.bot)) < 32.0);
    for _ in 0..360 {
        g.steps(1);
        for bot in [g.bot, rules_bot] {
            let now = thought(&g.s, bot);
            assert!(
                now.visible.is_none() && now.remembered.is_none(),
                "allied actor was treated as hostile: {now:?}"
            );
            assert_eq!(g.s.vitals()[&bot].health, 100.0);
        }
        assert_eq!(g.s.vitals()[&g.human].health, 100.0);
    }
    // Friendly fire is actually enabled: a human can hurt an ally, while
    // that permission still must not turn the ally into a bot enemy.
    g.send(g.human, Command::EquipTool { slot: Some(0) });
    g.steps(16);
    let ray = (feet(&g.s, g.bot) + Vec3::Y - (feet(&g.s, g.human) + Vec3::Y * 1.6)).normalize();
    g.sequence += 1;
    g.s.command_with_aim(
        g.human,
        g.sequence,
        Command::WeaponTrigger { down: true },
        Some(ActionAim {
            yaw: ray.x.atan2(-ray.z),
            pitch: ray.y.asin(),
        }),
    )
    .unwrap();
    g.steps(3);
    g.send(g.human, Command::WeaponTrigger { down: false });
    assert!(
        g.s.vitals()[&g.bot].health < 100.0,
        "allied damage is permitted"
    );
    for bot in [g.bot, rules_bot] {
        let now = thought(&g.s, bot);
        assert!(now.visible.is_none() && now.remembered.is_none(), "{now:?}");
    }
    // The same armed brains must acquire and damage the human once that
    // relationship ends. This distinguishes alliance from disabled combat.
    let human = g.human;
    set_team(&mut g, human, None);
    g.steps(1);
    assert_eq!(thought(&g.s, rules_bot).visible, Some(human));
    for _ in 0..600 {
        if g.s.vitals()[&human].health < 100.0 {
            break;
        }
        g.steps(1);
    }
    assert!(
        g.s.vitals()[&human].health < 100.0,
        "the fixture permits combat against a real enemy"
    );
}

#[test]
fn a_hitscan_guard_understands_ray_range_and_hits_without_chasing_into_melee() {
    let mut g = Game::new(8.0, true);
    assert_eq!(thought(&g.s, g.bot).behaviour, "fight");
    let mut nearest = f32::INFINITY;
    for _ in 0..600 {
        nearest = nearest.min(feet(&g.s, g.bot).distance(feet(&g.s, g.human)));
        if g.s.vitals()[&g.human].health < 100.0 {
            break;
        }
        g.steps(1);
    }
    assert!(
        g.s.vitals()[&g.human].health < 100.0,
        "the hitscan actually inflicted damage"
    );
    assert!(
        nearest > 10.0,
        "ranged shot rather than approaching the projectile's tiny range: {nearest}"
    );
}

#[test]
fn removing_a_rules_bot_releases_its_reservation_before_the_lease_expires() {
    const CART: &str = "test:vehicle/reservation-cart";
    let mut s = session(8.0, false);
    let mut kinds = BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots;
    kinds[0].wander_radius = 0.0;
    kinds[0].behaviours.insert("interact".into(), 1.0);
    kinds[0].behaviours.insert("fly".into(), 0.0);
    let (mut pack, _) = fixture::synthetic_vehicles().unwrap();
    let mut cart = bri_vehicles::testing::car();
    cart.id = CART.into();
    cart.seats.truncate(1);
    pack.definitions.push(cart);
    s.set_vehicle_pack(pack, kinds).unwrap();
    s.set_tool_catalog(ToolCatalog {
        vehicles: [CART.to_string()].into(),
        vehicle_bricks: [fixture::PLATE.to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    s.install_packages(rules_catalog(), None).unwrap();
    // Far enough off that walking to the cart, boarding and driving gets
    // there sooner than walking: only then does the seat serve the chase.
    let far = HOME + Vec3::new(0.0, -0.05, 36.0);
    let human = s.join("Reservation target".into(), far, true).unwrap();
    let mut brick = Brick::new(
        ContentRef::Resolved(fixture::PLATE.into()),
        (HOME + Vec3::new(8.25, 0.0, 0.25)).to_array(),
        human,
    );
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(CART.into()),
        recolor: false,
        team: None,
    }));
    let mut world = World::new("Reservations".into(), "chaos/map".into(), vec![[1.0; 4]]);
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    let mut g = Game {
        s,
        human,
        bot: 0,
        sequence: 1 << 40,
    };
    g.send(
        human,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    );
    g.steps(5);
    assert_eq!(g.s.simulation().state().bricks.len(), 1);
    g.s.set_spawn_points(vec![far]).unwrap();
    g.send(
        human,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout: [None, None, None, None, None],
                ..Default::default()
            },
        }),
    );
    g.s.set_spawn_points(vec![HOME]).unwrap();
    g.send(human, rules("add"));
    g.steps(3);
    let first = *g.s.names().keys().find(|o| g.s.is_bot(**o)).unwrap();
    let old = thought(&g.s, first);
    let Some(BotTask::Seat {
        vehicle,
        seat,
        deadline,
        ..
    }) = old.task
    else {
        panic!("fixture establishes an actual seat commitment: {old:?}");
    };
    assert_eq!(seat, 0);
    assert_eq!(
        g.s.mounted(first),
        None,
        "approach has not yet become occupancy"
    );
    g.send(
        human,
        Command::Package(PackageCommand {
            package: "knowledge-test".into(),
            command: "remove".into(),
            args: vec![PackageArg::Int(first as i64)],
        }),
    );
    assert!(
        !g.s.names().contains_key(&first),
        "the old claimant was removed"
    );
    g.send(human, rules("add"));
    g.steps(3);
    let second = *g.s.names().keys().find(|o| g.s.is_bot(**o)).unwrap();
    assert_ne!(first, second);
    assert!(
        g.s.simulation().state().tick < deadline,
        "the original lease is still live"
    );
    let replacement = thought(&g.s, second);
    assert!(
        matches!(replacement.task, Some(BotTask::Seat { vehicle: v, seat: 0, .. }) if v == vehicle),
        "the same scarce seat is immediately available: {replacement:?}"
    );
}

#[test]
fn becoming_an_ally_cancels_an_armed_hand_spear_without_throwing_it() {
    let mut g = Game::setup(8.0, false, true, Some(bri_weapons::testing::SPEAR_ITEM));
    g.send(g.human, rules("teams"));
    g.steps(1);
    let team = g.s.minigame_views()[0].teams[0].id.0;
    let bot = g.bot;
    set_team(&mut g, bot, Some(team));
    let armed = |g: &Game| {
        g.s.weapon_view().images.get(&g.bot).is_some_and(|images| {
            images.iter().any(|image| {
                image.image == bri_weapons::testing::SPEAR_IMAGE && image.state == "Armed"
            })
        })
    };
    for _ in 0..360 {
        if armed(&g) {
            break;
        }
        g.steps(1);
    }
    assert!(
        armed(&g),
        "the real spear reached its release-to-fire state"
    );
    assert_eq!(thought(&g.s, bot).visible, Some(g.human));
    assert!(feet(&g.s, bot).distance(feet(&g.s, g.human)) > 10.0);
    let existing: std::collections::BTreeSet<_> =
        g.s.weapon_view().projectiles.iter().map(|p| p.id).collect();
    let human = g.human;
    set_team(&mut g, human, Some(team));
    assert!(
        armed(&g),
        "team assignment itself does not reset the weapon"
    );
    for _ in 0..3 {
        g.steps(1);
        let now = thought(&g.s, bot);
        assert!(now.visible.is_none() && now.remembered.is_none(), "{now:?}");
        assert!(
            !g.s.weapon_view().fired().any(|p| {
                p.source.0 == bot
                    && p.definition == bri_weapons::testing::SPEAR_PROJECTILE
                    && !existing.contains(&p.id)
            }),
            "cancelling hostility must abort charge rather than fire on release"
        );
    }
    // The weapon remains usable after cancellation. Ending the alliance
    // must allow this same brain to arm and throw again.
    set_team(&mut g, human, None);
    let mut thrown = false;
    for _ in 0..360 {
        g.steps(1);
        thrown |= g.s.weapon_view().fired().any(|p| {
            p.source.0 == bot
                && p.definition == bri_weapons::testing::SPEAR_PROJECTILE
                && !existing.contains(&p.id)
        });
        if thrown {
            break;
        }
    }
    assert!(
        thrown,
        "cancellation preserves subsequent legitimate combat"
    );
}
