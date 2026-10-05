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
        let mut world = World::new("Perception".into(), "chaos/map".into(), vec![[1.0; 4]]);
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
        g.bot = *g.s.names().keys().find(|o| g.s.is_bot(**o)).unwrap();
        g
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
        self.s
            .bot_thoughts()
            .into_iter()
            .find(|b| b.bot == self.bot)
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

fn watched(gaze: f32) -> Option<(BotThought, f32)> {
    let mut kind = blockhead();
    kind.perception.gaze = gaze;
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
    let (thought, off) = watched(40.0).expect("a long stare draws a glance");
    assert_eq!(thought.behaviour, "wander", "{thought:?}");
    assert!(
        off < 12.0,
        "the bot looks at its watcher: {off} degrees off"
    );
}

#[test]
fn no_glance_at_weight_zero() {
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
    kind.perception.reaction = reaction;
    kind.perception.relaxed_scale = 3.0;
    kind.perception.away_scale = 1.0;
    kind.perception.jitter = 0.0;
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
    assert!(none.is_none(), "weight 0 records no reaction: {none:?}");
    let (relaxed, notice) = first_wound(1.0);
    let notice = notice.expect("the reaction is in the readout");
    assert!(notice.why.starts_with("reacting: relaxed"), "{notice:?}");
    // 0.5 s plain; three times that from a stroll.
    assert_eq!(notice.until - notice.since, 180);
    assert!(
        relaxed >= plain + 100,
        "relaxed {relaxed} ticks vs plain {plain}"
    );
}
