//! What a chaos bot does next. The same brains drive players inside one
//! process ([`crate::local`]) and real clients over loopback
//! ([`crate::net`]): each tick they look at a [`View`] of the world and
//! return a movement input and, now and then, a command. Commands mix what
//! players do (build, fire, drive, chat) with hostile nonsense (NaN
//! positions, bad slots, overlapping loads) the host must refuse cleanly.
use bri_admin::{Action, Request};
use bri_sim::{
    player::MoveInput,
    session::{ActionAim, Command, ToolAction, WrenchProperties},
};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, PI};

/// SplitMix64: small, seedable, the same everywhere.
#[derive(Clone, Debug)]
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        (!items.is_empty()).then(|| &items[self.below(items.len())])
    }
    /// A float that is usually ordinary and sometimes hostile.
    pub fn nasty(&mut self, ordinary: f32) -> f32 {
        match self.below(40) {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => f32::MAX,
            4 => -0.0,
            5 => 1e30,
            6 => f32::MIN_POSITIVE,
            _ => ordinary,
        }
    }
}

/// What a bot can see: enough to aim, build near itself and find vehicles.
#[derive(Clone, Debug, Default)]
pub struct View {
    pub me: OwnerId,
    pub feet: Option<Vec3>,
    pub alive: bool,
    pub mounted: bool,
    /// Bricks by id: position and definition.
    pub bricks: Vec<(u64, Vec3, String)>,
    pub vehicles: Vec<Vec3>,
    pub players: Vec<(OwnerId, Vec3)>,
}

/// Content a fixture offers the bots.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub bricks: Vec<BrickKind>,
    pub vehicles: Vec<String>,
    pub saves: Vec<World>,
    pub palette: usize,
    pub extent: f32,
}

/// A plantable brick and its grid size.
#[derive(Clone, Debug)]
pub struct BrickKind {
    pub id: String,
    /// Footprint in studs (x, z) and height in plates.
    pub studs: [i32; 2],
    pub plates: i32,
}
impl Catalog {
    fn plates(&self, id: &str) -> i32 {
        self.bricks
            .iter()
            .find(|b| b.id == id)
            .map_or(1, |b| b.plates)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Goal {
    Wander,
    Build,
    Shoot,
    SeekVehicle,
    Drive,
    Idle,
}

pub struct Bot {
    pub owner: OwnerId,
    pub administrator: bool,
    rng: Rng,
    goal: Goal,
    goal_ticks: u32,
    yaw: f32,
    pitch: f32,
    trigger: bool,
    armed: bool,
}

const GOALS: [Goal; 6] = [
    Goal::Wander,
    Goal::Build,
    Goal::Shoot,
    Goal::SeekVehicle,
    Goal::Drive,
    Goal::Idle,
];

fn look_at(from: Vec3, to: Vec3) -> (f32, f32) {
    let d = to - from;
    let flat = (d.x * d.x + d.z * d.z).sqrt();
    (
        d.x.atan2(-d.z),
        d.y.atan2(flat).clamp(-FRAC_PI_2, FRAC_PI_2),
    )
}

impl Bot {
    pub fn new(owner: OwnerId, administrator: bool, seed: u64) -> Self {
        Self {
            owner,
            administrator,
            rng: Rng::new(seed),
            // Newcomers start by building or shooting, so even a short run
            // does both.
            goal: if seed.is_multiple_of(2) {
                Goal::Shoot
            } else {
                Goal::Build
            },
            goal_ticks: 360,
            yaw: 0.0,
            pitch: 0.0,
            trigger: false,
            armed: false,
        }
    }

    fn nearest(from: Vec3, points: impl Iterator<Item = Vec3>) -> Option<Vec3> {
        points.min_by(|a, b| {
            a.distance_squared(from)
                .total_cmp(&b.distance_squared(from))
        })
    }

    /// This tick's movement.
    pub fn movement(&mut self, view: &View) -> MoveInput {
        if self.goal_ticks == 0 {
            self.goal = if view.mounted && self.rng.chance(0.8) {
                Goal::Drive
            } else if !view.vehicles.is_empty() && self.rng.chance(0.3) {
                Goal::SeekVehicle
            } else {
                *self.rng.pick(&GOALS).unwrap()
            };
            self.goal_ticks = 60 + self.rng.below(600) as u32;
        }
        self.goal_ticks -= 1;
        let feet = view.feet.unwrap_or_default();
        let eye = feet + Vec3::Y * 2.2;
        let mut input = MoveInput::default();
        match self.goal {
            Goal::Wander => {
                self.yaw += self.rng.range(-0.1, 0.1);
                input.forward = 1.0;
                input.jump = self.rng.chance(0.02);
                input.jet = self.rng.chance(0.05);
            }
            Goal::Build => {
                self.pitch = -0.6;
                input.crouch = self.rng.chance(0.01);
            }
            Goal::Shoot => {
                let target = Self::nearest(
                    feet,
                    view.vehicles.iter().copied().chain(
                        view.players
                            .iter()
                            .filter(|(o, _)| *o != view.me)
                            .map(|(_, p)| *p + Vec3::Y),
                    ),
                );
                if let Some(target) = target {
                    (self.yaw, self.pitch) = look_at(eye, target);
                } else {
                    self.pitch = self.rng.range(-FRAC_PI_2, FRAC_PI_2);
                }
                input.right = self.rng.range(-1.0, 1.0);
            }
            Goal::SeekVehicle => {
                if let Some(target) = Self::nearest(feet, view.vehicles.iter().copied()) {
                    (self.yaw, _) = look_at(feet, target);
                    input.forward = 1.0;
                    input.jump = target.distance(feet) < 4.0 && self.rng.chance(0.2);
                }
            }
            Goal::Drive => {
                input.forward = self.rng.range(-1.0, 1.0);
                input.right = self.rng.range(-1.0, 1.0);
                input.jump = self.rng.chance(0.05);
                input.jet = self.rng.chance(0.1);
                self.yaw += self.rng.range(-0.2, 0.2);
                self.pitch += self.rng.range(-0.1, 0.1);
            }
            Goal::Idle => {}
        }
        // Keep out of the void so the bots stay where the action is.
        if feet.length() > 0.0 && (feet.x.abs() > 80.0 || feet.z.abs() > 80.0) {
            (self.yaw, _) = look_at(feet, Vec3::ZERO);
            input.forward = 1.0;
        }
        self.yaw = (self.yaw + PI).rem_euclid(2.0 * PI) - PI;
        self.pitch = self.pitch.clamp(-FRAC_PI_2, FRAC_PI_2);
        input.yaw = self.yaw;
        input.pitch = self.pitch;
        input
    }

    fn aim(&mut self) -> Option<ActionAim> {
        if self.rng.chance(0.1) {
            // Hostile aim the host must refuse or clamp.
            return Some(ActionAim {
                yaw: self.rng.nasty(self.yaw),
                pitch: self.rng.nasty(self.pitch),
            });
        }
        self.rng.chance(0.7).then_some(ActionAim {
            yaw: self.yaw,
            pitch: if self.rng.chance(0.2) {
                -FRAC_PI_2
            } else {
                self.pitch
            },
        })
    }

    /// A brick placement around `feet`: on the grid, on the floor or on
    /// top of a known brick, most of the time; overlapping players and
    /// other bricks often; off the grid or non-finite sometimes.
    fn placement(
        &mut self,
        catalog: &Catalog,
        view: &View,
        feet: Vec3,
        spread: i32,
    ) -> (String, [f32; 3], u8) {
        let Some(kind) = self.rng.pick(&catalog.bricks).cloned() else {
            return (String::new(), feet.to_array(), 0);
        };
        let turns = self.rng.below(4) as u8;
        let [w, d] = if turns.is_multiple_of(2) {
            kind.studs
        } else {
            [kind.studs[1], kind.studs[0]]
        };
        let size = [w, kind.plates, d];
        let jitter = |rng: &mut Rng| rng.below(2 * spread as usize + 1) as i32 - spread;
        let mut min = [
            (feet.x / 0.5).round() as i32 + jitter(&mut self.rng) - w / 2,
            0,
            (feet.z / 0.5).round() as i32 + jitter(&mut self.rng) - d / 2,
        ];
        if let Some((_, top, definition)) = self
            .rng
            .chance(0.5)
            .then(|| self.rng.pick(&view.bricks))
            .flatten()
        {
            let height = catalog.plates(definition);
            min[1] = ((top.y + height as f32 * 0.1) / 0.2).round() as i32;
            if self.rng.chance(0.7) {
                min[0] = (top.x / 0.5).round() as i32 + jitter(&mut self.rng).clamp(-1, 1) - w / 2;
                min[2] = (top.z / 0.5).round() as i32 + jitter(&mut self.rng).clamp(-1, 1) - d / 2;
            }
        }
        let cell = [0.5, 0.2, 0.5];
        let mut position: [f32; 3] =
            std::array::from_fn(|a| (min[a] as f32 + size[a] as f32 * 0.5) * cell[a]);
        if self.rng.chance(0.05) {
            for v in &mut position {
                let nudged = *v + self.rng.range(-0.2, 0.2);
                *v = self.rng.nasty(nudged);
            }
        }
        (kind.id, position, turns)
    }

    /// A random build around `feet`: overlapping itself and whatever is
    /// already there, with vehicle and bot spawn bricks mixed in.
    pub fn random_build(&mut self, catalog: &Catalog, feet: Vec3) -> World {
        if let Some(save) = self
            .rng
            .chance(0.5)
            .then(|| self.rng.pick(&catalog.saves))
            .flatten()
        {
            return save.clone();
        }
        let mut world = World::new(
            "Chaos build".into(),
            "chaos".into(),
            vec![[1.0; 4]; catalog.palette.max(1)],
        );
        let most = if self.rng.chance(0.1) { 2000 } else { 60 };
        let count = 1 + self.rng.below(most);
        let view = View::default();
        for i in 0..count {
            let spread = if count > 100 { 30 } else { 12 };
            let (definition, position, turns) = self.placement(catalog, &view, feet, spread);
            let mut brick = Brick::new(ContentRef::Resolved(definition), position, 0);
            brick.quarter_turns = turns;
            brick.color = self.rng.below(catalog.palette.max(1)) as u8;
            if self.rng.chance(0.05) {
                brick.vehicle = self.rng.pick(&catalog.vehicles).map(|v| {
                    Box::new(VehicleSpawn {
                        vehicle: ContentRef::Resolved(v.clone()),
                        recolor: self.rng.chance(0.5),
                        team: None,
                    })
                });
            }
            if i == 0 && self.rng.chance(0.7) {
                brick.vehicle = self.rng.pick(&catalog.vehicles).map(|v| {
                    Box::new(VehicleSpawn {
                        vehicle: ContentRef::Resolved(v.clone()),
                        recolor: true,
                        team: None,
                    })
                });
            }
            brick.colliding = !self.rng.chance(0.05);
            brick.raycast = !self.rng.chance(0.05);
            world.bricks.insert(i as u64 + 1, brick);
        }
        world.next_brick_id = count as u64 + 1;
        world
    }

    /// A command for this tick, if the bot does one, with its aim.
    pub fn command(
        &mut self,
        view: &View,
        catalog: &Catalog,
    ) -> Option<(Command, Option<ActionAim>)> {
        let feet = view.feet.unwrap_or_default();
        if !view.alive {
            return self.rng.chance(0.05).then_some((Command::Respawn, None));
        }
        // Hold the trigger for a while once pressed.
        if self.trigger && self.rng.chance(0.1) {
            self.trigger = false;
            return Some((Command::WeaponTrigger { down: false }, self.aim()));
        }
        let p = match self.goal {
            Goal::Build | Goal::Shoot => 0.15,
            _ => 0.03,
        };
        if self.administrator && self.rng.chance(1.0 / 240.0) {
            return Some((
                Command::LoadBuild {
                    build: Box::new(SavedBuild::new(self.random_build(catalog, feet))),
                    ownership: self.rng.chance(0.5),
                },
                None,
            ));
        }
        if !self.rng.chance(p) {
            return None;
        }
        let brick = || view.bricks.iter().map(|(id, _, _)| *id);
        let some_brick = |rng: &mut Rng| {
            let ids: Vec<u64> = brick().collect();
            rng.pick(&ids).copied().unwrap_or(rng.next_u64() % 64)
        };
        let command = match (self.goal, self.rng.below(24)) {
            (Goal::Build, 0..=9) | (_, 0..=1) => {
                let spread = if self.rng.chance(0.3) { 0 } else { 6 };
                let (definition, position, turns) = self.placement(catalog, view, feet, spread);
                Command::Plant {
                    definition,
                    position,
                    quarter_turns: if self.rng.chance(0.02) { 4 } else { turns },
                    color: self.rng.below(catalog.palette + 1) as u8,
                }
            }
            (Goal::Build, 10..=11) | (_, 18) if self.administrator => Command::LoadBuild {
                build: Box::new(SavedBuild::new(self.random_build(catalog, feet))),
                ownership: self.rng.chance(0.5),
            },
            (Goal::Shoot, 0..=13) | (_, 2..=3) => {
                if !self.armed || self.rng.chance(0.1) {
                    // Mostly the loadout's weapons (slots 3 and 4), sometimes
                    // a tool or a slot that does not exist.
                    self.armed = true;
                    Command::EquipTool {
                        slot: Some(if self.rng.chance(0.7) {
                            3 + self.rng.below(2)
                        } else {
                            self.rng.below(7)
                        }),
                    }
                } else {
                    self.trigger = true;
                    Command::WeaponTrigger { down: true }
                }
            }
            (_, 4) => Command::EquipTool {
                slot: self.rng.chance(0.8).then(|| self.rng.below(7)),
            },
            (_, 5) => Command::DropTool {
                slot: self.rng.below(6),
            },
            (_, 6) => Command::Activate,
            (_, 7) => Command::Chat(self.chat()),
            (_, 8) => {
                if self.rng.chance(0.3) {
                    Command::Suicide
                } else {
                    Command::ToggleLight
                }
            }
            (_, 9) => Command::Emote(
                ["alarm", "confusion", "love", "hate", "wat", "\u{0}", ""][self.rng.below(7)]
                    .into(),
            ),
            (_, 10) => Command::SwitchSeat(if self.rng.chance(0.5) { 1 } else { -1 }),
            (_, 11) => Command::Tool(ToolAction::UndoBrick),
            (_, 12) => Command::Tool(ToolAction::SetWrench {
                brick: some_brick(&mut self.rng),
                properties: WrenchProperties {
                    vehicle: self.rng.pick(&catalog.vehicles).cloned(),
                    recolor_vehicle: self.rng.chance(0.5),
                    raycast: !self.rng.chance(0.2),
                    colliding: !self.rng.chance(0.2),
                    visible: !self.rng.chance(0.2),
                    ..Default::default()
                },
            }),
            (_, 13) => Command::Tool(ToolAction::RespawnVehicle {
                brick: some_brick(&mut self.rng),
            }),
            (_, 14) => Command::UseSprayCan {
                color: self.rng.below(catalog.palette + 2) as u8,
            },
            (_, 15) => Command::UseFxCan {
                fx: self.rng.below(10) as u8,
            },
            (_, 16) => Command::Talking(self.rng.chance(0.5)),
            (_, 17) => Command::Wand,
            (_, 19) => Command::SaveBuild {
                events: self.rng.chance(0.5),
                ownership: self.rng.chance(0.5),
            },
            (_, 20) if self.administrator => Command::Admin(Request::new(self.admin_action())),
            (_, 21) => Command::ControlPlayer,
            (_, 22) => Command::ClearCheckpoint,
            _ => {
                let (definition, position, quarter_turns) = self.placement(catalog, view, feet, 2);
                Command::Plant {
                    definition,
                    position,
                    quarter_turns,
                    color: 0,
                }
            }
        };
        let aim = if matches!(command, Command::Admin(_)) {
            None
        } else {
            self.aim()
        };
        Some((command, aim))
    }

    fn admin_action(&mut self) -> Action {
        match self.rng.below(12) {
            // Rare: wipes are cheap to test but empty the playground.
            0 if self.rng.chance(0.1) => Action::ClearAllBricks,
            1 => Action::ClearVehicles,
            2 => Action::ClearBots,
            3 => Action::ResetVehicles,
            4 => Action::TimeScale {
                scale: [0.2, 0.5, 1.0, 2.0, 0.0, 5.0, f32::NAN][self.rng.below(7)],
            },
            5 => Action::DropPlayerAtCamera,
            6 => Action::DropCameraAtPlayer,
            7 => Action::Warp,
            8 => Action::RealBrickCount,
            9 => Action::CancelAllEvents,
            10 => Action::ReturnToPreviousPosition,
            _ => Action::TimeScale { scale: 1.0 },
        }
    }

    fn chat(&mut self) -> String {
        match self.rng.below(6) {
            0 => String::new(),
            1 => "x".repeat(self.rng.below(4000)),
            2 => "héllo 🧱 \u{202e}rtl\u{0}\u{7}".into(),
            3 => format!(
                "/{}",
                ["brickcount", "spy", "ret", "clearcheckpoint", "\u{1b}"][self.rng.below(5)]
            ),
            _ => format!("chaos {}", self.rng.next_u64()),
        }
    }
}
