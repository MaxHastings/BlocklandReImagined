//! Public-session checks for what a bot notices: a glance turns its
//! ordinary aim, and a reaction delays its first shot by how alert it was.
//! Fixtures are invented; the readout is checked against the bot's actual
//! look and the human's actual health, never used to steer the bot.
use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::{
    bot_kind::{BotKind, BotPack},
    player::MoveInput,
    session::{BotThought, CameraView, Command, MiniGameRequest, Session, ToolCatalog},
};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use serde_json::json;

const HOME: Vec3 = Vec3::new(-40.0, 0.1, 30.0);
const SEEN: Vec3 = Vec3::new(-40.0, 0.05, 44.0);
const HIDDEN: Vec3 = Vec3::new(-80.0, 0.05, 90.0);

fn blockhead() -> BotKind {
    let mut kind = BotPack::from_json(include_bytes!(
        "../../../packages/blockhead_bot/assets/bots.json"
    ))
    .unwrap()
    .bots
    .remove(0);
    kind.sight = 32.0;
    kind.chase_radius = 0.0;
    kind.wander_radius = 4.0;
    kind.alerts_allies = false;
    kind.aim_error_degrees = 0.0;
    kind.turn_degrees = 720.0;
    kind.behaviours.insert("interact".into(), 0.0);
    kind.behaviours.insert("fly".into(), 0.0);
    kind.behaviours.insert("chase".into(), 0.0);
    kind
}

struct Game {
    s: Session,
    human: OwnerId,
    bot: OwnerId,
    sequence: u64,
}
impl Game {
    /// A bot of `kind` on its brick and a human 14 units away; in a
    /// mini-game with a hitscan gun when `armed`.
    fn new(kind: BotKind, armed: bool) -> Self {
        Self::with_bots(kind, armed, &[HOME])
    }
    /// As [`Self::new`], with a bot's brick at each of `homes`; `bot` is
    /// the first.
    fn with_bots(kind: BotKind, armed: bool, homes: &[Vec3]) -> Self {
        let mut s = fixture::synthetic().unwrap().session;
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
        s.set_weapon_pack(pack).unwrap();
        s.set_bot_kinds(vec![kind]).unwrap();
        s.set_tool_catalog(ToolCatalog {
            vehicles: [fixture::BOT.to_string()].into(),
            vehicle_bricks: [fixture::PLATE.to_string()].into(),
            ..Default::default()
        })
        .unwrap();
        s.set_spawn_points(vec![SEEN]).unwrap();
        let human = s.join("Watcher".into(), SEEN, true).unwrap();
        let mut world = World::new("Perception".into(), "chaos/map".into(), vec![[1.0; 4]]);
        for (id, home) in homes.iter().enumerate() {
            let mut brick = Brick::new(
                ContentRef::Resolved(fixture::PLATE.into()),
                (*home + Vec3::new(0.25, 0.0, 0.25)).to_array(),
                human,
            );
            brick.vehicle = Some(Box::new(VehicleSpawn {
                vehicle: ContentRef::Resolved(fixture::BOT.into()),
                recolor: false,
                team: None,
            }));
            world.bricks.insert(id as u64 + 1, brick);
        }
        world.next_brick_id = homes.len() as u64 + 1;
        s.command(
            human,
            1,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            },
        )
        .unwrap();
        let mut g = Self {
            s,
            human,
            bot: 0,
            sequence: 1 << 40,
        };
        g.steps(5, false);
        if armed {
            g.sequence += 1;
            g.s.command(
                human,
                g.sequence,
                Command::MiniGame(MiniGameRequest::Create {
                    color: 0,
                    settings: Settings {
                        loadout: [
                            Some(bri_weapons::testing::GUN_ITEM.into()),
                            None,
                            None,
                            None,
                            None,
                        ],
                        ..Default::default()
                    },
                }),
            )
            .unwrap();
        }
        g.bot = g.bots()[0];
        g
    }
    /// The bots, the one nearest the first home first.
    fn bots(&self) -> Vec<OwnerId> {
        let mut bots: Vec<OwnerId> = self
            .s
            .names()
            .keys()
            .copied()
            .filter(|o| self.s.is_bot(*o))
            .collect();
        let d = |o: &OwnerId| Vec3::from(self.player(*o).feet).distance(HOME);
        bots.sort_by(|a, b| d(a).total_cmp(&d(b)));
        bots
    }
    fn player(&self, owner: OwnerId) -> bri_sim::player::PlayerState {
        self.s
            .snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == owner)
            .unwrap()
    }
    fn thought(&self) -> BotThought {
        self.thought_of(self.bot)
    }
    fn thought_of(&self, bot: OwnerId) -> BotThought {
        self.s
            .bot_thoughts()
            .into_iter()
            .find(|b| b.bot == bot)
            .unwrap()
    }
    /// Steps, the human looking straight at the bot's head when `stare`.
    fn steps(&mut self, ticks: usize, stare: bool) {
        for _ in 0..ticks {
            let mut input = MoveInput::default();
            if stare {
                let delta = Vec3::from(self.player(self.bot).feet)
                    - Vec3::from(self.player(self.human).feet);
                let across = Vec3::new(delta.x, 0.0, delta.z).length();
                input.yaw = delta.x.atan2(-delta.z);
                input.pitch = delta.y.atan2(across);
            }
            self.sequence += 1;
            self.s.movement(self.human, self.sequence, input).unwrap();
            self.s.step().unwrap();
        }
    }
    fn drop_at(&mut self, at: Vec3) {
        self.sequence += 1;
        self.s
            .command(
                self.human,
                self.sequence,
                Command::DropPlayerAtCamera(Some(CameraView {
                    eye: [at.x, at.y + 1.6, at.z],
                    yaw: 0.0,
                    pitch: 0.0,
                })),
            )
            .unwrap();
        self.steps(1, false);
        assert!(Vec3::from(self.player(self.human).feet).distance(at) < 0.2);
    }
    fn tick(&self) -> u64 {
        self.s.simulation().state().tick
    }
}

/// How far the bot's look is off the human, in degrees across.
fn off_human(g: &Game) -> f32 {
    let bot = g.player(g.bot);
    let to = Vec3::from(g.player(g.human).feet) - Vec3::from(bot.feet);
    let wanted = to.x.atan2(-to.z);
    let mut d = (wanted - bot.yaw) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d.abs().to_degrees()
}

fn watched(salience: f32) -> Option<(BotThought, f32)> {
    let mut kind = blockhead();
    kind.perception.salience = salience;
    let mut g = Game::new(kind, false);
    for _ in 0..120 * 8 {
        g.steps(1, true);
        let thought = g.thought();
        if let Some(n) = thought.noticed
            && n.why == "glance: watched"
        {
            // Hold the stare to the end of the glance: the bot's ordinary
            // look has turned to the human by then.
            while g.tick() + 1 < n.until {
                g.steps(1, true);
            }
            return Some((g.thought(), off_human(&g)));
        }
    }
    None
}

#[test]
fn a_long_stare_turns_an_idle_bots_head() {
    // Sight 32: a stare reaches 0.3 of it, doubled to cover the 14 units.
    let (thought, off) = watched(2.0).expect("a long stare draws a glance");
    assert_eq!(thought.behaviour, "wander", "{thought:?}");
    assert!(
        off < 12.0,
        "the bot looks at its watcher: {off} degrees off"
    );
}

#[test]
fn no_glance_at_salience_zero() {
    assert!(watched(0.0).is_none());
}

/// The bot, armed and back to strolling, sees the human step out again:
/// ticks from that sight to the human's next wound, and the bot's reaction
/// readout then.
fn first_wound(reaction: f32) -> (u64, Option<bri_sim::session::BotNotice>) {
    let mut kind = blockhead();
    kind.reaction_seconds = 0.5;
    kind.chase_radius = 128.0;
    kind.memory_seconds = 0.5;
    kind.perception.alertness = reaction;
    kind.perception.relaxed_scale = 3.0;
    kind.perception.away_scale = 1.0;
    let mut g = Game::new(kind, true);
    // Armed and fighting: it has hurt the human once.
    let mut armed = false;
    for _ in 0..120 * 10 {
        g.steps(1, false);
        if g.s.vitals()[&g.human].health < 100.0 {
            armed = true;
            break;
        }
    }
    assert!(armed, "the bot never hurt the human: {:?}", g.thought());
    // Out of sight until the bot forgets and strolls again.
    g.drop_at(HIDDEN);
    g.steps(360, false);
    let idle = g.thought();
    assert_eq!((idle.behaviour, idle.visible), ("wander", None), "{idle:?}");
    g.drop_at(SEEN);
    let health = g.s.vitals()[&g.human].health;
    let mut seen = None;
    for _ in 0..120 * 10 {
        g.steps(1, false);
        let thought = g.thought();
        if seen.is_none() && thought.visible == Some(g.human) {
            seen = Some(g.tick());
        }
        if g.s.vitals()[&g.human].health < health {
            return (g.tick() - seen.expect("seen before hurt"), thought.noticed);
        }
    }
    panic!("the bot never fired again: {:?}", g.thought());
}

#[test]
fn a_relaxed_bot_returns_fire_after_its_longer_reaction() {
    let (plain, none) = first_wound(0.0);
    assert!(none.is_none(), "alertness 0 records no reaction: {none:?}");
    let (relaxed, notice) = first_wound(1.0);
    let notice = notice.expect("the reaction is in the readout");
    assert!(notice.why.starts_with("reacting: relaxed"), "{notice:?}");
    // 0.5 s plain; three times that from a stroll, give or take 30%.
    let delay = notice.until - notice.since;
    assert!((126..=234).contains(&delay), "{delay} ticks");
    assert!(
        relaxed >= plain + 60,
        "relaxed {relaxed} ticks vs plain {plain}"
    );
}

/// The human, armed, shoots the bot once from `at`: the bot's remembered
/// position for the human just after the hit, the human's feet, the
/// ally's remembered position once it acted on the bot's warning, and
/// whether the bot saw the human then.
fn shot_from(at: Vec3) -> (Vec3, Vec3, Option<Vec3>, bool) {
    let mut kind = blockhead();
    kind.chase_radius = 128.0;
    kind.alerts_allies = true;
    // It neither fires back nor goes looking here: only what it knows is
    // checked.
    kind.reaction_seconds = 5.0;
    kind.behaviours.insert("search".into(), 0.0);
    let mut g = Game::with_bots(kind, true, &[HOME, HOME + Vec3::new(5.0, 0.0, 0.0)]);
    let ally = g.bots()[1];
    g.drop_at(at);
    // Past the bots' spawn protection (300 ticks) and the human's weapon
    // lock after a teleport (3 s).
    g.steps(400, false);
    g.sequence += 1;
    g.s.command(g.human, g.sequence, Command::EquipTool { slot: Some(0) })
        .unwrap();
    g.steps(16, false);
    // Looking straight at it, it clicks the trigger until a shot lands.
    let mut hit = None;
    for n in 0..240 {
        if n % 30 == 2 || n % 30 == 10 {
            g.sequence += 1;
            g.s.command(
                g.human,
                g.sequence,
                Command::WeaponTrigger { down: n % 30 == 2 },
            )
            .unwrap();
        }
        g.steps(1, true);
        if g.s.vitals()[&g.bot].health < 100.0 {
            hit = Some(g.tick());
            break;
        }
    }
    let hit = hit.unwrap_or_else(|| panic!("the shots missed the bot: {:?}", g.thought()));
    g.sequence += 1;
    g.s.command(g.human, g.sequence, Command::WeaponTrigger { down: false })
        .unwrap();
    g.steps(1, false);
    let thought = g.thought();
    let seen = thought.visible == Some(g.human);
    let known = Vec3::from(thought.remembered.expect("it knows it was hurt").position);
    let human = Vec3::from(g.player(g.human).feet);
    let mut warned = None;
    for _ in 0..240 {
        g.steps(1, false);
        if let Some(r) = g.thought_of(ally).remembered
            && r.observed + 2 >= hit
        {
            warned = Some(Vec3::from(r.position));
            break;
        }
    }
    (known, human, warned, seen)
}

fn flat_distance(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, 0.0, a.z - b.z).length()
}

#[test]
fn a_hit_from_out_of_sight_gives_a_rough_place_and_the_ally_no_exact_one() {
    // 40 units off: in the gun's 48-unit range, beyond the bot's 32 sight.
    let (known, human, warned, seen) = shot_from(HOME + Vec3::new(0.0, 0.0, 40.0));
    assert!(!seen);
    let miss = flat_distance(known, human);
    assert!(miss >= 1.0, "it knows only roughly: {miss} units off");
    assert!(miss < 25.0, "but about the right way: {miss} units off");
    let warned = warned.expect("the ally heard the warning");
    assert!(
        flat_distance(warned, human) >= 0.5,
        "the ally gets the rough guess, not the spot"
    );
}

#[test]
fn a_hit_from_someone_in_sight_is_placed_exactly() {
    let (known, human, _, seen) = shot_from(SEEN);
    assert!(seen);
    assert!(flat_distance(known, human) < 0.05, "{known} vs {human}");
}
