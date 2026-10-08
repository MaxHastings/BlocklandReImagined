//! Shared arena and scoring for the bot gauntlet (`bot_gauntlet.rs`).
//!
//! Scores only what a viewer would see as bad play: a bot pressing to move
//! but going nowhere, strolling about while there is a fight or an
//! objective, walking circles, flip-flopping, dying to the void or its own
//! side, shooting away from its target or into an ally, and bots standing
//! inside each other. Everything is read from the public session after
//! ordinary ticks; nothing here steers a bot.
#![allow(dead_code)]

pub mod shares;
pub mod tuning;

use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const TICKS_PER_SECOND: usize = 120;

/// Extra rounds for long soak runs: `BRI_GAUNTLET_ROUNDS=4` plays every
/// scenario four times as long. The default is one round.
pub fn rounds() -> usize {
    std::env::var("BRI_GAUNTLET_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &usize| *n > 0)
        .unwrap_or(1)
}

/// A synthetic session with bots, vehicles and the made-up arsenal.
pub struct Arena {
    pub s: Session,
    pub humans: Vec<OwnerId>,
    /// What a scripted player presses; the rest stand still.
    pub moves: BTreeMap<OwnerId, MoveInput>,
    seq: u64,
    cmd: u64,
}

impl Arena {
    pub fn new(vehicle_kinds: &[&str]) -> Self {
        Self::custom(&[], &[], vehicle_kinds)
    }

    /// The synthetic session with extra bricks (`(id, copied from)`) and
    /// extra items (`(item id, image id)`, copies of the gun) for a
    /// scenario's own package.
    pub fn custom(bricks: &[(&str, &str)], items: &[(&str, &str)], vehicle_kinds: &[&str]) -> Self {
        let mut simulation = fixture::synthetic_simulation(&[]).unwrap();
        for (id, base) in bricks {
            let mut d = simulation.definitions.entries[*base].clone();
            d.mesh.id = id.to_string();
            simulation.definitions.entries.insert(id.to_string(), d);
        }
        let mut s = Session::new(simulation);
        // A fixed load pace, so a scenario plays the same on every machine.
        s.set_load_pace(bri_sim::session::LoadPace::Bricks(4096));
        let (mut pack, mut item_ids) = fixture::synthetic_weapons().unwrap();
        for (item_id, image_id) in items {
            let mut item = pack.items[bri_weapons::testing::GUN_ITEM].clone();
            let mut image = pack.images[bri_weapons::testing::GUN_IMAGE].clone();
            item.id = item_id.to_string();
            item.image = image_id.to_string();
            image.id = image_id.to_string();
            pack.images.insert(image.id.clone(), image);
            pack.items.insert(item.id.clone(), item);
            item_ids.push(item_id.to_string());
        }
        s.set_weapon_pack(pack).unwrap();
        s.set_item_bounds(
            item_ids
                .iter()
                .cloned()
                .chain(bri_weapons::CORE_TOOLS.iter().map(|s| s.to_string()))
                .map(|id| {
                    (
                        id,
                        bri_weapons::ItemBounds {
                            min: [-0.3, -0.1, -0.5],
                            max: [0.3, 0.2, 0.5],
                        },
                    )
                })
                .collect(),
        )
        .unwrap();
        let (vehicles, _) = fixture::synthetic_vehicles().unwrap();
        s.set_vehicle_pack(vehicles, blockhead_kinds()).unwrap();
        s.set_spawn_points(vec![Vec3::new(92.0, 0.05, 92.0)])
            .unwrap();
        s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
            .unwrap();
        let mut arena = Self {
            s,
            humans: Vec::new(),
            moves: BTreeMap::new(),
            seq: 1 << 40,
            cmd: 1000,
        };
        arena.catalog(&[], vehicle_kinds);
        // A tuning seed other than 0 plays the same scenario on other
        // random streams: bots' generators follow their ids and the tick.
        let seed = tuning::seed();
        for i in 0..seed {
            arena.join(
                &format!("Seed {i}"),
                Vec3::new(90.0 - 2.0 * i as f32, 0.05, 92.0),
            );
        }
        arena.step(seed as usize * 7);
        arena
    }

    fn catalog(&mut self, kinds: &[String], vehicle_kinds: &[&str]) {
        let mut vehicles: Vec<String> = vec![fixture::BOT.into()];
        vehicles.extend(kinds.iter().cloned());
        vehicles.extend(vehicle_kinds.iter().map(|v| v.to_string()));
        self.s
            .set_tool_catalog(ToolCatalog {
                vehicles: vehicles.into_iter().collect(),
                vehicle_bricks: [fixture::PLATE.to_string()].into(),
                ..Default::default()
            })
            .unwrap();
    }

    /// Install one server package made of `behaviour` (its behaviour.json
    /// past the script) and `script`, with `capabilities`.
    pub fn package(
        &mut self,
        id: &str,
        capabilities: &[&str],
        behaviour: serde_json::Value,
        script: &str,
    ) {
        use bri_package::packages::{PackageEntry, PackageSet, Side};
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "bri-gauntlet-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("package.json"),
            serde_json::json!({"schema_version":1,"id":id,"version":"1.0.0","api":1,
                "name":"Gauntlet rules","license":"CC0-1.0","capabilities":capabilities,
                "provides":[{"kind":"behaviour","id":format!("{id}:behaviour/main"),"file":"behaviour.json"},
                {"kind":"script","id":format!("{id}:script/main"),"file":"main.rhai"}]})
            .to_string(),
        )
        .unwrap();
        let mut behaviour = behaviour;
        behaviour["schema_version"] = 1.into();
        behaviour["script"] = "main.rhai".into();
        std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
        std::fs::write(dir.join("main.rhai"), script).unwrap();
        let catalog = bri_package_runtime::Catalog::load(
            &root,
            &PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: id.into(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: id.into(),
                    role: None,
                }],
            },
            true,
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&root);
        self.s
            .install_packages(std::sync::Arc::new(catalog), None)
            .unwrap();
    }

    /// The Blockhead Bot's kind plus `extra`, spawnable from a brick.
    pub fn kinds(&mut self, extra: Vec<bri_sim::bot_kind::BotKind>, vehicle_kinds: &[&str]) {
        let mut kinds = blockhead_kinds();
        let ids: Vec<String> = extra.iter().map(|k| k.id.clone()).collect();
        kinds.extend(extra);
        self.s.set_bot_kinds(kinds).unwrap();
        self.catalog(&ids, vehicle_kinds);
    }

    /// A builder standing far from the arena, out of every bot's sight.
    pub fn join(&mut self, name: &str, at: Vec3) -> OwnerId {
        let who = self.s.join(name.into(), at, true).unwrap();
        self.humans.push(who);
        self.step(5);
        who
    }

    pub fn command(&mut self, who: OwnerId, command: Command) {
        self.cmd += 1;
        self.s.command(who, self.cmd, command).unwrap();
    }

    /// Load `bricks` as `owner`'s, added to what stands.
    pub fn load(&mut self, owner: OwnerId, bricks: Vec<Brick>) {
        let before = self.s.simulation().state().bricks.len();
        let mut world = World::new("Gauntlet".into(), "chaos/map".into(), vec![[1.0; 4]; 8]);
        let count = bricks.len();
        for (i, mut brick) in bricks.into_iter().enumerate() {
            brick.owner = owner;
            world.bricks.insert(i as u64 + 1, brick);
        }
        world.next_brick_id = count as u64 + 1;
        self.command(
            owner,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            },
        );
        for _ in 0..600 {
            self.step(1);
            if self.s.simulation().state().bricks.len() >= before + count && !self.s.build_loading()
            {
                break;
            }
        }
        assert_eq!(
            self.s.simulation().state().bricks.len(),
            before + count,
            "every brick placed"
        );
    }

    /// `owner` makes a mini-game; returns its id.
    pub fn minigame(&mut self, owner: OwnerId, settings: Settings) -> u64 {
        self.command(
            owner,
            Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
        );
        self.step(2);
        self.s
            .minigame_views()
            .iter()
            .find(|g| g.owner == owner)
            .expect("mini-game made")
            .id
    }

    pub fn join_game(&mut self, who: OwnerId, game: u64) {
        self.command(who, Command::MiniGame(MiniGameRequest::Join { game }));
        self.step(2);
    }

    /// Step the world, the builders standing still (or as `moves` says).
    pub fn step(&mut self, ticks: usize) {
        for _ in 0..ticks {
            self.seq += 1;
            for human in &self.humans {
                let input = self.moves.get(human).cloned().unwrap_or_default();
                self.s.movement(*human, self.seq, input).unwrap();
            }
            self.s.step().unwrap();
        }
    }

    pub fn bots(&self) -> Vec<OwnerId> {
        self.s
            .names()
            .keys()
            .copied()
            .filter(|o| self.s.is_bot(*o))
            .collect()
    }

    pub fn feet(&self, who: OwnerId) -> Option<Vec3> {
        self.s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == who)
            .map(|(p, _)| Vec3::from(p.feet))
    }

    /// Each bot's side: the builder of the spawn brick nearest where it
    /// first stands (bricks bots spawn by), or `fallback`.
    pub fn sides_by_spawn(&self, spawns: &[(Vec3, u32)]) -> BTreeMap<OwnerId, u32> {
        self.bots()
            .into_iter()
            .filter_map(|bot| {
                let at = self.feet(bot)?;
                let side = spawns
                    .iter()
                    .min_by(|a, b| a.0.distance(at).total_cmp(&b.0.distance(at)))?
                    .1;
                Some((bot, side))
            })
            .collect()
    }
}

/// The shipped `bots.json`, as text: the tuning tools read its dials.
pub const BOTS_JSON: &[u8] = include_bytes!("../../../../packages/blockhead_bot/assets/bots.json");

/// The shipped kinds, with this thread's tuning dials applied
/// ([`tuning::with_dials`]; none outside the tuning tools).
pub fn blockhead_kinds() -> Vec<bri_sim::bot_kind::BotKind> {
    let kinds = bri_sim::bot_kind::BotPack::from_json(BOTS_JSON)
        .unwrap()
        .bots;
    tuning::apply_dials(kinds)
}

/// A body attack (`BotKind::melee`).
pub fn melee(damage: f32, reach: f32) -> bri_sim::bot_kind::BotMelee {
    serde_json::from_value(serde_json::json!({
        "damage": damage, "reach": reach, "seconds": 1.0
    }))
    .unwrap()
}

/// A 1x1 plate spawning `kind` (a bot or a vehicle) in the stud cell at `at`.
pub fn spawner(kind: &str, at: Vec3) -> Brick {
    let mut brick = brick(fixture::PLATE, at + Vec3::new(0.25, 0.1, 0.25));
    brick.vehicle = Some(Box::new(VehicleSpawn {
        vehicle: ContentRef::Resolved(kind.into()),
        recolor: false,
        team: None,
    }));
    brick
}

/// A brick of `definition` centred at `at`.
pub fn brick(definition: &str, at: Vec3) -> Brick {
    Brick::new(ContentRef::Resolved(definition.into()), at.to_array(), 0)
}

/// A flat floor of baseplates (8 x 8 units each) with its top at `top`,
/// covering `cells` baseplates across x and z from corner `min`.
pub fn floor(min: Vec3, cells: [i32; 2], top: f32) -> Vec<Brick> {
    let mut out = Vec::new();
    for i in 0..cells[0] {
        for j in 0..cells[1] {
            out.push(brick(
                fixture::BASEPLATE,
                Vec3::new(
                    min.x + 4.0 + i as f32 * 8.0,
                    top - 0.1,
                    min.z + 4.0 + j as f32 * 8.0,
                ),
            ));
        }
    }
    out
}

/// A wall of tall columns (3 units high) from `a` to `b` along one axis.
pub fn wall(a: Vec3, b: Vec3) -> Vec<Brick> {
    let mut out = Vec::new();
    let length = a.distance(b);
    let steps = (length / 0.5).round() as i32;
    for i in 0..=steps {
        let p = a.lerp(b, i as f32 / steps.max(1) as f32);
        out.push(brick(
            fixture::TALL,
            Vec3::new(
                (p.x * 2.0).floor() / 2.0 + 0.25,
                p.y + 1.5,
                (p.z * 2.0).floor() / 2.0 + 0.25,
            ),
        ));
    }
    out
}

/// One bot's samples for the jank scores.
#[derive(Default)]
struct Track {
    /// Recent (feet, wanted to move) samples, newest last.
    window: VecDeque<(Vec3, bool)>,
    /// Feet every tick for the circling window.
    path: VecDeque<Vec3>,
    last_behaviour: Option<&'static str>,
    last_heading: Option<(f32, u64)>,
    was_alive: bool,
    fallen: bool,
    /// The last tick it stood on something: a fall's airborne window starts
    /// there.
    grounded_at: u64,
    /// Ticks in a row in a flavour interrupt (`surprise`).
    goofing: u64,
    /// The behaviour a respawned bot's brain still holds from the life it
    /// lost, and the tick it came back (`last_behaviour`).
    carried: Option<(&'static str, u64)>,
}

/// What looks bad on camera, summed over every bot.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub name: String,
    pub bot_ticks: u64,
    /// Ticks a bot wanted to move but went under half a unit in a second.
    pub stuck: u64,
    /// Ticks a bot strolled about with work to do (an enemy or objective).
    pub idle: u64,
    /// Ticks inside a four-second window that walked over six units but
    /// ended under a fifth of that from where it began.
    pub circling: u64,
    /// Behaviour changes.
    pub switches: u64,
    /// Walking direction reversals inside a quarter second.
    pub reversals: u64,
    /// Ticks a bot stood within 0.9 units of an ally (stacked bodies).
    pub clumped: u64,
    pub kills: u64,
    pub team_kills: u64,
    pub self_kills: u64,
    /// Deaths with no killer (falls, the void, drowning).
    pub accidents: u64,
    pub deaths: u64,
    /// Lives that dropped under the floor (fell off the arena) by their own
    /// doing.
    pub fell: u64,
    /// Lives knocked off the arena: an enemy's weapon pushed them after
    /// they last stood on something (a shove that worked).
    pub knocked_off: u64,
    pub shots: u64,
    /// Bots' launches of projectiles that only push (a shove, a push
    /// broom): no damage of their own.
    pub push_fired: u64,
    /// Pushes a bot planned with worth (`BotPlannedHarm::push` > 0, a
    /// trade its side takes), and of those, the ones it launched.
    pub push_planned: u64,
    pub planned_push_fired: u64,
    /// Shots more than 25 degrees off the shooter's visible target.
    pub off_target: u64,
    /// Shots whose line passes nearest an ally.
    pub at_ally: u64,
    /// Health each side's players took from their own side, and from the
    /// other sides, over the match.
    pub team_damage: f32,
    pub enemy_damage: f32,
    /// The same by the side that did it: (to its own side, to the others).
    pub damage_by_side: BTreeMap<u32, (f32, f32)>,
    /// Bot ticks with a planned shot that would hurt its own side as much
    /// as its enemies, or kill a teammate (`BotThought::planned`).
    pub bad_plans: u64,
    /// Scenario-specific progress (captures, laps, goals).
    pub progress: BTreeMap<String, i64>,
    /// Bots seen at least once.
    pub bots: usize,
    /// Ticks in each behaviour, and each change between two.
    pub behaviours: BTreeMap<&'static str, u64>,
    pub transitions: BTreeMap<(&'static str, &'static str), u64>,
    /// Stuck ticks by behaviour, and a few samples (bot, behaviour, feet,
    /// next waypoint).
    pub stuck_in: BTreeMap<&'static str, u64>,
    pub stuck_at: Vec<(OwnerId, &'static str, Vec3, Option<Vec3>)>,
    /// Circling ticks by behaviour.
    pub circling_in: BTreeMap<&'static str, u64>,
    /// Distinct choices in effect (choice point and option, `surprise`)
    /// summed over each bot's minutes: variety per bot-minute.
    pub distinct_picks: u64,
    /// Ticks a bot spent in a flavour interrupt, and the longest run.
    pub goof: u64,
    pub longest_goof: u64,
    /// Switches to an option other than the plain pick.
    pub surprised: u64,
    /// Behaviour share report (`shares`): ticks each living bot spent in
    /// each kind of activity (a behaviour, `goof`, `vehicle`), mounted
    /// bots included, and the total.
    pub kinds: BTreeMap<String, u64>,
    pub kind_ticks: u64,
    /// Kinds the chooser scored above zero at least once (offered).
    pub offered: BTreeSet<String>,
    /// Ticks each other choice point's option was in effect
    /// (`aim:feet`), and each choice point's total.
    pub options: BTreeMap<String, u64>,
    pub domain_ticks: BTreeMap<String, u64>,
    /// Goof ticks and bot ticks in each ten-second window.
    pub goof_windows: Vec<(u64, u64)>,
    /// Wall time stepping the session, for the frame cost per bot.
    pub step_nanos: u64,
}

impl Report {
    /// Each side that hurt its own side hurt its enemies more.
    pub fn each_side_hurts_its_own_less(&self) -> Result<(), String> {
        match self
            .damage_by_side
            .iter()
            .find(|(_, (own, enemy))| *own > 0.0 && own >= enemy)
        {
            Some((side, (own, enemy))) => Err(format!(
                "{}: side {side} hurt its own {own} and its enemies {enemy}",
                self.name
            )),
            None => Ok(()),
        }
    }
    pub fn share(&self, ticks: u64) -> f32 {
        ticks as f32 / self.bot_ticks.max(1) as f32
    }
    pub fn per_bot_minute(&self, count: u64) -> f32 {
        count as f32 / (self.bot_ticks.max(1) as f32 / (60.0 * TICKS_PER_SECOND as f32))
    }
    pub fn print(&self) {
        if tuning::quiet() {
            return;
        }
        eprintln!(
            "GAUNTLET {}: bots={} bot-min={:.1} stuck={:.1}% idle={:.1}% circling={:.1}% \
             switches/min={:.1} reversals/min={:.1} clumped={:.1}% kills={} team_kills={} \
             self_kills={} accidents={} fell={} knocked_off={} deaths={} shots={} push_fired={} push_planned={} planned_push_fired={} off_target={} at_ally={}              team_damage={:.0} enemy_damage={:.0} \
             progress={:?}",
            self.name,
            self.bots,
            self.bot_ticks as f32 / 7200.0,
            100.0 * self.share(self.stuck),
            100.0 * self.share(self.idle),
            100.0 * self.share(self.circling),
            self.per_bot_minute(self.switches),
            self.per_bot_minute(self.reversals),
            100.0 * self.share(self.clumped),
            self.kills,
            self.team_kills,
            self.self_kills,
            self.accidents,
            self.fell,
            self.knocked_off,
            self.deaths,
            self.shots,
            self.push_fired,
            self.push_planned,
            self.planned_push_fired,
            self.off_target,
            self.at_ally,
            self.team_damage,
            self.enemy_damage,
            self.progress
        );
        let mut top: Vec<_> = self.transitions.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let shares: Vec<String> = self
            .behaviours
            .iter()
            .map(|(b, t)| format!("{b}={:.0}%", 100.0 * self.share(*t)))
            .collect();
        eprintln!(
            "GAUNTLET {} surprise: variety={:.1} distinct picks/bot-min goof={:.2}% \
             longest goof={:.1}s surprised={:.1}/bot-min",
            self.name,
            self.per_bot_minute(self.distinct_picks),
            100.0 * self.share(self.goof),
            self.longest_goof as f32 / TICKS_PER_SECOND as f32,
            self.per_bot_minute(self.surprised),
        );
        eprintln!(
            "GAUNTLET {} behaviours: {} top changes: {:?}",
            self.name,
            shares.join(" "),
            &top[..top.len().min(6)]
        );
        if self.circling > 0 {
            eprintln!("GAUNTLET {} circling in {:?}", self.name, self.circling_in);
        }
        if self.stuck > 0 {
            eprintln!(
                "GAUNTLET {} stuck in {:?} e.g. {:?}",
                self.name,
                self.stuck_in,
                &self.stuck_at[..self.stuck_at.len().min(8)]
            );
        }
    }
}

/// A scenario's report is done: print it and its behaviour shares, hand
/// it to a tuning tool running the scenario, and fail on an enforced
/// share band (`shares`).
pub fn finish(report: &Report) {
    report.print();
    tuning::hand_over(report);
    if !tuning::quiet() {
        shares::check(report);
    }
}

/// Steps `arena` one tick, adding the time to `report`'s frame cost.
pub fn timed_step(arena: &mut Arena, report: &mut Report) {
    let started = std::time::Instant::now();
    arena.step(1);
    report.step_nanos += started.elapsed().as_nanos() as u64;
}

/// Samples a session every tick into a [`Report`].
pub struct Scorer {
    pub report: Report,
    tracks: BTreeMap<OwnerId, Track>,
    seen_shots: BTreeSet<u64>,
    /// Projectiles that hurt nothing (a can's paint): not shots.
    harmless: BTreeSet<String>,
    /// Of those, the ones that push a player.
    pushers: BTreeSet<String>,
    /// The last tick each bot had a push planned with worth.
    push_plans: BTreeMap<OwnerId, u64>,
    seen_deaths: usize,
    last_death_tick: u64,
    /// Damage results already counted: those at or before this tick.
    last_damage_tick: Option<u64>,
    /// Floor height: under `floor - 2` is fallen.
    floor: f32,
    seen: BTreeSet<OwnerId>,
    /// Choices each bot had in effect, by bot and minute.
    picks: BTreeSet<(OwnerId, u64, String)>,
    /// The option last in effect, by bot and choice point.
    decided: BTreeMap<(OwnerId, &'static str), String>,
    /// The first tick sampled: goof windows count from it.
    first_tick: Option<u64>,
}

impl Scorer {
    pub fn new(name: &str, floor: f32) -> Self {
        Self {
            report: Report {
                name: name.into(),
                ..Default::default()
            },
            tracks: BTreeMap::new(),
            seen_shots: BTreeSet::new(),
            harmless: fixture::synthetic_weapons()
                .unwrap()
                .0
                .projectiles
                .into_values()
                .filter(|p| p.damage <= 0.0 && p.explosion.damage <= 0.0)
                .map(|p| p.id)
                .collect(),
            push_plans: BTreeMap::new(),
            pushers: fixture::synthetic_weapons()
                .unwrap()
                .0
                .projectiles
                .into_values()
                .filter(|p| {
                    p.damage <= 0.0
                        && p.explosion.damage <= 0.0
                        && (p.impulse > 0.0 || p.vertical > 0.0)
                })
                .map(|p| p.id)
                .collect(),
            seen_deaths: 0,
            last_death_tick: 0,
            last_damage_tick: None,
            floor,
            seen: BTreeSet::new(),
            picks: BTreeSet::new(),
            decided: BTreeMap::new(),
            first_tick: None,
        }
    }

    /// One tick. `sides` gives each bot's side (humans are no side);
    /// `work` says whether a bot has something better than strolling.
    pub fn sample(
        &mut self,
        s: &Session,
        sides: &BTreeMap<OwnerId, u32>,
        work: impl Fn(OwnerId) -> bool,
    ) {
        let tick = s.simulation().state().tick;
        let vitals = s.vitals();
        let states: BTreeMap<OwnerId, bri_sim::player::PlayerState> = s
            .motion_states()
            .into_iter()
            .map(|(p, _)| (p.owner, p))
            .collect();
        let thoughts: BTreeMap<OwnerId, bri_sim::session::BotThought> =
            s.bot_thoughts().into_iter().map(|t| (t.bot, t)).collect();
        let alive = |o: &OwnerId| vitals.get(o).is_some_and(|v| v.alive);
        for t in thoughts.values() {
            // A push planned with worth: a new one unless it was planned
            // a tick ago too.
            if t.planned
                .is_some_and(|p| p.push > 0.0 && p.net.is_some_and(|n| n > 0.0))
            {
                let last = self.push_plans.insert(t.bot, tick);
                if last.is_none_or(|t| t + 1 < tick) {
                    self.report.push_planned += 1;
                }
            }
            // By the rule the chooser and the fire gate trade by
            // (`BotPlannedHarm::net`).
            if t.planned
                .is_some_and(|p| p.net.is_none_or(|net| net <= 0.0))
            {
                self.report.bad_plans += 1;
            }
        }
        let trace = std::env::var("BRI_GAUNTLET_TRACE")
            .is_ok_and(|t| self.report.name.contains(&t))
            && tick.is_multiple_of(60);
        if trace {
            for (bot, t) in &thoughts {
                let Some(st) = states.get(bot) else { continue };
                eprintln!(
                    "T{tick} bot{bot} side={:?} alive={} at=({:.1},{:.1},{:.1}) yaw={:.2} {} vis={:?} goal={:?} next={:?} steps={} obj={:?} {:?}",
                    sides.get(bot),
                    alive(bot),
                    st.feet[0],
                    st.feet[1],
                    st.feet[2],
                    st.yaw,
                    t.behaviour,
                    t.visible,
                    t.goal
                        .map(|g| format!("({:.1},{:.1},{:.1})", g[0], g[1], g[2])),
                    t.next
                        .map(|g| format!("({:.1},{:.1},{:.1})", g[0], g[1], g[2])),
                    t.path_steps,
                    t.objective_diagnostic,
                    t.objective_detail
                        .as_ref()
                        .map(|d| (d.action.clone(), d.phase)),
                );
            }
        }
        for (bot, thought) in &thoughts {
            let Some(state) = states.get(bot) else {
                continue;
            };
            if alive(bot) {
                let first = *self.first_tick.get_or_insert(tick);
                shares::sample(
                    &mut self.report,
                    thought,
                    vitals[bot].mounted.is_some(),
                    tick,
                    first,
                );
            }
            let track = self.tracks.entry(*bot).or_default();
            if !alive(bot) || vitals[bot].mounted.is_some() {
                track.window.clear();
                track.path.clear();
                track.was_alive = false;
                continue;
            }
            self.seen.insert(*bot);
            if !track.was_alive {
                track.window.clear();
                track.path.clear();
                track.last_heading = None;
                track.fallen = false;
                // A respawn starts a new life, not a choice the bot made:
                // what it did before it died and what it does after are
                // not one behaviour switch. The brain keeps the choice it
                // died with until its new life's first think, so that one
                // change is not counted. (A killer's turn to its next
                // target is its own choice and still counts.)
                track.carried = Some((thought.behaviour, tick));
            }
            track.was_alive = true;
            self.report.bot_ticks += 1;
            // Variety: the distinct choices in effect each bot-minute.
            for d in &thought.surprise.decisions {
                let pick = format!("{}:{}", d.domain, d.chosen);
                if self
                    .picks
                    .insert((*bot, tick / (60 * TICKS_PER_SECOND as u64), pick))
                {
                    self.report.distinct_picks += 1;
                }
                let last = self.decided.insert((*bot, d.domain), d.chosen.clone());
                if d.varied && last.is_some_and(|l| l != d.chosen) {
                    self.report.surprised += 1;
                }
            }
            if thought.surprise.interrupt.is_some() {
                self.report.goof += 1;
                track.goofing += 1;
                self.report.longest_goof = self.report.longest_goof.max(track.goofing);
            } else {
                track.goofing = 0;
            }
            let feet = Vec3::from(state.feet);
            let flat = Vec3::new(feet.x, 0.0, feet.z);
            if state.grounded {
                track.grounded_at = tick;
            }
            if feet.y < self.floor - 2.0 {
                if !std::mem::replace(&mut track.fallen, true) {
                    // Knocked off when an enemy's weapon pushed it after it
                    // last stood on something; otherwise its own doing.
                    if s.pushed_by(*bot).is_some_and(|(by, at)| {
                        at >= track.grounded_at && sides.get(&by) != sides.get(bot)
                    }) {
                        self.report.knocked_off += 1;
                    } else {
                        self.report.fell += 1;
                    }
                }
                track.window.clear();
                track.path.clear();
                continue;
            }
            // Wanting to move: following a route, or chasing/searching/
            // returning/approaching an objective point not yet reached.
            let wants = thought.next.is_some()
                || matches!(thought.behaviour, "chase" | "search" | "return")
                    && thought
                        .goal
                        .is_some_and(|g| Vec3::from(g).distance(feet) > 1.5);
            track.window.push_back((flat, wants));
            if track.window.len() > TICKS_PER_SECOND {
                track.window.pop_front();
            }
            if track.window.len() == TICKS_PER_SECOND
                && track.window.iter().all(|(_, w)| *w)
                && track.window.front().unwrap().0.distance(flat) < 0.5
            {
                self.report.stuck += 1;
                *self.report.stuck_in.entry(thought.behaviour).or_default() += 1;
                if tick.is_multiple_of(240) {
                    self.report.stuck_at.push((
                        *bot,
                        thought.behaviour,
                        (feet * 10.0).round() / 10.0,
                        thought.next.map(|n| (Vec3::from(n) * 10.0).round() / 10.0),
                    ));
                }
            }
            track.path.push_back(flat);
            if track.path.len() > 4 * TICKS_PER_SECOND {
                track.path.pop_front();
            }
            if track.path.len() == 4 * TICKS_PER_SECOND {
                let length: f32 = track
                    .path
                    .iter()
                    .zip(track.path.iter().skip(1))
                    .map(|(a, b)| a.distance(*b))
                    .sum();
                let net = track.path.front().unwrap().distance(flat);
                if length > 6.0 && net < 0.2 * length {
                    self.report.circling += 1;
                    *self
                        .report
                        .circling_in
                        .entry(thought.behaviour)
                        .or_default() += 1;
                }
            }
            if thought.behaviour == "wander" && work(*bot) {
                self.report.idle += 1;
            }
            *self.report.behaviours.entry(thought.behaviour).or_default() += 1;
            if track
                .carried
                .is_some_and(|(_, at)| tick > at + TICKS_PER_SECOND as u64)
            {
                track.carried = None;
            }
            if let Some(last) = track.last_behaviour.filter(|b| *b != thought.behaviour) {
                if track.carried.is_some_and(|(b, _)| b == last) {
                    // Its new life's first choice.
                    track.carried = None;
                } else {
                    self.report.switches += 1;
                    *self
                        .report
                        .transitions
                        .entry((last, thought.behaviour))
                        .or_default() += 1;
                }
            }
            track.last_behaviour = Some(thought.behaviour);
            let v = Vec3::new(state.velocity[0], 0.0, state.velocity[2]);
            if v.length() > 2.0 && state.grounded {
                let heading = v.x.atan2(-v.z);
                if let Some((last, at)) = track.last_heading
                    && tick.saturating_sub(at) <= 30
                {
                    let turn = (heading - last + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    if turn.abs() > 2.6 {
                        self.report.reversals += 1;
                    }
                }
                track.last_heading = Some((heading, tick));
            }
            let side = sides.get(bot);
            // Inside a standing ally; one seated in a vehicle is not
            // crowded by a bot boarding or riding on that vehicle.
            if side.is_some()
                && states.iter().any(|(o, p)| {
                    o != bot
                        && alive(o)
                        && vitals.get(o).is_some_and(|v| v.mounted.is_none())
                        && sides.get(o) == side
                        && Vec3::from(p.feet).distance(feet) < 0.9
                })
            {
                self.report.clumped += 1;
            }
        }
        // Shots: new projectiles a bot fired.
        let view = s.weapon_view();
        for p in view.fired() {
            if !self.seen_shots.insert(p.id) {
                continue;
            }
            if self.harmless.contains(&p.definition) {
                if self.pushers.contains(&p.definition) && thoughts.contains_key(&p.source.0) {
                    self.report.push_fired += 1;
                    // Planned at a tick, fired at the next one's gate.
                    if self
                        .push_plans
                        .get(&p.source.0)
                        .is_some_and(|t| t + 1 >= tick)
                    {
                        self.report.planned_push_fired += 1;
                    }
                }
                continue;
            }
            let shooter = p.source.0;
            let Some(thought) = thoughts.get(&shooter) else {
                continue;
            };
            let direction = p.velocity.normalize_or_zero();
            if direction == Vec3::ZERO {
                continue;
            }
            self.report.shots += 1;
            let origin = p.origin;
            if let Some(target) = thought.visible
                && let Some(t) = states.get(&target)
            {
                // Off target: more than 25 degrees off every point of its
                // body, feet to head (up close a swing at the head is on
                // target, though well off a fixed point at the middle).
                let feet = Vec3::from(t.feet);
                // The point of its body (feet to head, 2.4 up) level with
                // the shooter, or the nearer end.
                let head = feet + Vec3::Y * 2.4;
                let nearest = Vec3::new(feet.x, origin.y, feet.z).max(feet).min(head);
                let off = |at: Vec3| {
                    (at - origin).normalize_or_zero().dot(direction) < 25f32.to_radians().cos()
                };
                if off(nearest) && off(feet + Vec3::Y * 1.2) {
                    self.report.off_target += 1;
                }
            }
            // The body nearest the shot's line, ahead within 40 units.
            let nearest = states
                .iter()
                .filter(|(o, _)| **o != shooter && alive(o))
                .filter_map(|(o, st)| {
                    let centre = Vec3::from(st.feet) + Vec3::Y * 1.0;
                    let along = (centre - origin).dot(direction);
                    if !(0.0..40.0).contains(&along) {
                        return None;
                    }
                    let miss = (centre - origin - direction * along).length();
                    (miss < 1.0).then_some((along, *o))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, hit)) = nearest
                && sides.get(&hit).is_some()
                && sides.get(&hit) == sides.get(&shooter)
            {
                self.report.at_ally += 1;
                if std::env::var("BRI_GAUNTLET_TRACE").is_ok_and(|t| self.report.name.contains(&t))
                {
                    eprintln!(
                        "T{tick} AT ALLY bot{shooter} from {origin:.1} along {direction:.2} \
                         nearest ally bot{hit} at {:?}, target {:?} at {:?}",
                        states.get(&hit).map(|st| st.feet),
                        thought.visible,
                        thought
                            .visible
                            .and_then(|t| states.get(&t))
                            .map(|st| st.feet),
                    );
                }
            }
        }
        // Deaths, once each.
        let deaths: Vec<_> = s
            .death_results()
            .filter(|d| d.tick > self.last_death_tick)
            .cloned()
            .collect();
        for d in deaths {
            self.last_death_tick = self.last_death_tick.max(d.tick);
            self.seen_deaths += 1;
            if !sides.contains_key(&d.victim) {
                continue;
            }
            self.report.deaths += 1;
            match d.killer {
                None => self.report.accidents += 1,
                Some(k) if k == d.victim => self.report.self_kills += 1,
                Some(k) if sides.get(&k).is_some() && sides.get(&k) == sides.get(&d.victim) => {
                    self.report.team_kills += 1
                }
                Some(_) => self.report.kills += 1,
            }
        }
        self.report.bots = self.seen.len();
        // Damage between players, by side: the health each new hit took.
        for d in s
            .damage_results()
            .filter(|d| self.last_damage_tick.is_none_or(|t| d.tick > t))
        {
            let (Some(by), Some(side)) = (d.source, sides.get(&d.victim)) else {
                continue;
            };
            let Some(theirs) = sides.get(&by).filter(|_| by != d.victim) else {
                continue;
            };
            let done = self.report.damage_by_side.entry(*theirs).or_default();
            if theirs == side {
                self.report.team_damage += d.amount;
                done.0 += d.amount;
            } else {
                self.report.enemy_damage += d.amount;
                done.1 += d.amount;
            }
        }
        self.last_damage_tick = Some(tick);
    }
}
