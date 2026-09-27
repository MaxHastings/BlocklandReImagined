//! Server-side bots placed by vehicle spawn bricks ("Blockhead Bot").
//!
//! A bot is an ordinary session player without a connection: it has a body,
//! inventory, health, minigame membership and replicated pose, so every
//! gameplay rule applies to it unchanged. Its brain produces one movement
//! input per tick and fires its weapon at hostile players it can see. Bots
//! follow the minigame of their spawn brick's owner and are harmless outside
//! minigames.
use super::*;
use bri_weapons::ActorId;

pub const BOT_KINDS: [(&str, &str); 1] = [("bot.blockhead", "Blockhead Bot")];
const MAX_BOTS: usize = 16;
const SIGHT: f32 = 80.0;
const WANDER_RADIUS: f32 = 12.0;

#[derive(Default)]
pub(super) struct Bots {
    by_brick: BTreeMap<BrickId, OwnerId>,
    brains: BTreeMap<OwnerId, Brain>,
}
struct Brain {
    brick: BrickId,
    home: Vec3,
    sequence: u64,
    goal: Option<Vec3>,
    next_goal: u64,
    last_position: Vec3,
    stuck: u32,
    fire_down: bool,
    rng: u64,
}
impl Brain {
    fn random(&mut self) -> f32 {
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }
}
impl Bots {
    pub(super) fn is_bot(&self, owner: OwnerId) -> bool {
        self.brains.contains_key(&owner)
    }
    pub(super) fn home(&self, owner: OwnerId) -> Option<Vec3> {
        self.brains.get(&owner).map(|b| b.home)
    }
}
pub fn is_bot_kind(id: &str) -> bool {
    BOT_KINDS.iter().any(|(k, _)| *k == id)
}

fn yaw_to(delta: Vec3) -> f32 {
    delta.x.atan2(-delta.z)
}
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

impl Session {
    pub fn is_bot(&self, owner: OwnerId) -> bool {
        self.bots.is_bot(owner)
    }
    pub(super) fn bot_home(&self, owner: OwnerId) -> Option<Vec3> {
        self.bots.home(owner)
    }
    /// Reconcile bots with spawn bricks naming a bot kind.
    pub(super) fn reconcile_bot_brick(&mut self, brick_id: BrickId, wanted: Option<&str>) -> Result<()> {
        let current = self.bots.by_brick.get(&brick_id).copied();
        let wants_bot = wanted.is_some_and(is_bot_kind);
        match (current, wants_bot) {
            (Some(bot), false) => {
                if self.peers.contains_key(&bot) {
                    self.disconnect(bot)?;
                    self.departed.remove(&bot);
                }
                self.bots.by_brick.remove(&brick_id);
                self.bots.brains.remove(&bot);
            }
            (None, true) if self.bots.brains.len() < MAX_BOTS => {
                let Some(brick) = self.simulation.state().bricks.get(&brick_id) else {
                    return Ok(());
                };
                let home = Vec3::from(brick.position) + Vec3::Y * 0.3;
                let name = BOT_KINDS
                    .iter()
                    .find(|(k, _)| Some(*k) == wanted)
                    .map_or("Bot", |(_, n)| n)
                    .to_string();
                // A crowded spawn is retried on later ticks.
                if let Ok(bot) = self.join_inner(name, home, false, true, None) {
                    self.bots.by_brick.insert(brick_id, bot);
                    self.bots.brains.insert(
                        bot,
                        Brain {
                            brick: brick_id,
                            home,
                            sequence: 0,
                            goal: None,
                            next_goal: 0,
                            last_position: home,
                            stuck: 0,
                            fire_down: false,
                            rng: 0x2545_F491_4F6C_DD1D ^ bot.wrapping_mul(0x9E37_79B9),
                        },
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn bot_bricks(&self) -> Vec<BrickId> {
        self.bots.by_brick.keys().copied().collect()
    }
    /// Bots whose brick changed kind, or vanished, are removed by reconcile;
    /// this also retries bricks that could not spawn their bot yet.
    fn bot_bricks_pending(&self) -> Vec<BrickId> {
        self.bots
            .brains
            .values()
            .filter(|b| !self.simulation.state().bricks.contains_key(&b.brick))
            .map(|b| b.brick)
            .collect()
    }
    /// Minigame membership follows the spawn brick owner.
    fn sync_bot_minigames(&mut self) -> Result<()> {
        let bots: Vec<(OwnerId, BrickId)> =
            self.bots.brains.iter().map(|(o, b)| (*o, b.brick)).collect();
        for (bot, brick) in bots {
            let Some(owner) = self.simulation.state().bricks.get(&brick).map(|b| b.owner) else {
                continue;
            };
            let wanted = self
                .peers
                .get(&owner)
                .and_then(|p| self.minigames.player(p.combat.player).ok())
                .and_then(|p| p.game);
            let Some(peer) = self.peers.get(&bot) else {
                continue;
            };
            let player = peer.combat.player;
            let current = self.minigames.player(player).ok().and_then(|p| p.game);
            if current != wanted {
                let effects = self
                    .minigames
                    .host_place(player, wanted)
                    .map_err(|e| anyhow::anyhow!("Bot minigame: {e}"))?;
                self.apply_minigame_effects(effects)?;
            }
        }
        Ok(())
    }
    /// One brain tick per bot: pick a target or wander goal, queue a movement
    /// input and pulse the weapon trigger.
    pub(super) fn step_bots(&mut self) -> Result<()> {
        if self.bots.brains.is_empty() {
            return Ok(());
        }
        for brick in self.bot_bricks_pending() {
            self.reconcile_bot_brick(brick, None)?;
        }
        let tick = self.simulation.state().tick;
        if tick.is_multiple_of(30) {
            self.sync_bot_minigames()?;
        }
        let bots: Vec<OwnerId> = self.bots.brains.keys().copied().collect();
        for bot in bots {
            let Some(peer) = self.peers.get(&bot) else {
                continue;
            };
            if !peer.combat.alive {
                if tick >= peer.combat.respawn_tick + 120 {
                    let _ = self.request_respawn(bot);
                }
                continue;
            }
            let state = peer.player.state().clone();
            let feet = Vec3::from(state.feet);
            let eye = peer.player.eye();
            // Nearest visible hostile human player.
            let target = self
                .peers
                .iter()
                .filter(|(owner, p)| {
                    !self.bots.is_bot(**owner) && p.combat.alive && **owner != bot
                })
                .filter(|(owner, _)| self.can_damage_player(bot, **owner, false))
                .map(|(owner, p)| (*owner, p.player.eye()))
                .filter(|(_, target)| target.distance(eye) < SIGHT)
                .filter(|(_, target)| {
                    let delta = *target - eye;
                    let distance = delta.length();
                    distance > 0.1
                        && self
                            .simulation
                            .target(eye, delta / distance, distance.min(149.0))
                            .ok()
                            .flatten()
                            .is_none_or(|hit| hit.distance > distance - 0.5)
                })
                .min_by(|a, b| a.1.distance(eye).total_cmp(&b.1.distance(eye)));
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            let moved = feet.distance(brain.last_position);
            brain.last_position = feet;
            let mut input = MoveInput {
                yaw: state.yaw,
                pitch: 0.0,
                ..Default::default()
            };
            let mut fire = false;
            if let Some((_, target_eye)) = target {
                let delta = target_eye - eye;
                let flat = Vec3::new(delta.x, 0.0, delta.z);
                input.yaw = wrap(yaw_to(delta));
                input.pitch = delta.y.atan2(flat.length()).clamp(-1.5, 1.5);
                let distance = delta.length();
                input.forward = if distance > 14.0 {
                    1.0
                } else if distance < 5.0 {
                    -1.0
                } else {
                    0.0
                };
                // Strafe back and forth so bots are not trivial targets.
                input.right = if (tick / 90 + bot).is_multiple_of(2) { 0.7 } else { -0.7 };
                fire = wrap(input.yaw - state.yaw).abs() < 0.2;
                if delta.y > 3.0 && distance < 20.0 {
                    input.jet = true;
                }
            } else {
                if brain.goal.is_none() || tick >= brain.next_goal {
                    let angle = brain.random() * std::f32::consts::TAU;
                    let radius = brain.random() * WANDER_RADIUS;
                    brain.goal = Some(brain.home + Vec3::new(angle.sin(), 0.0, angle.cos()) * radius);
                    brain.next_goal = tick + 240 + (brain.random() * 480.0) as u64;
                }
                if let Some(goal) = brain.goal {
                    let delta = goal - feet;
                    let flat = Vec3::new(delta.x, 0.0, delta.z);
                    if flat.length() > 1.0 {
                        input.yaw = wrap(yaw_to(flat));
                        input.forward = 0.6;
                    } else {
                        brain.goal = None;
                    }
                }
            }
            // Walking into something: hop over it.
            if (input.forward != 0.0 || input.right != 0.0) && moved < 0.01 {
                brain.stuck += 1;
            } else {
                brain.stuck = 0;
            }
            input.jump = brain.stuck > 20 && brain.stuck % 40 < 5;
            if brain.stuck > 200 {
                brain.goal = None;
                brain.stuck = 0;
            }
            brain.sequence += 1;
            let sequence = brain.sequence;
            let fire_changed = fire != brain.fire_down;
            // Pulse the trigger so semi-automatic weapons keep firing.
            let pulse = fire && tick.is_multiple_of(40);
            brain.fire_down = fire && !pulse;
            self.movement(bot, sequence, input)?;
            if fire {
                self.bot_arm(bot)?;
            }
            let direction = Vec3::new(
                input.yaw.sin() * input.pitch.cos(),
                input.pitch.sin(),
                -input.yaw.cos() * input.pitch.cos(),
            );
            if fire_changed || pulse {
                let down = fire && !pulse;
                if self.weapons.image_state(ActorId(bot), 0).is_some() || !down {
                    let _ = self.weapon_trigger(bot, down, direction);
                    if down {
                        self.note_shot(bot);
                    }
                }
            }
        }
        Ok(())
    }
    /// Equip the first real weapon (not a building tool) in the inventory.
    fn bot_arm(&mut self, bot: OwnerId) -> Result<()> {
        let Some(actor) = self.weapons.actor(ActorId(bot)) else {
            return Ok(());
        };
        let weapon = actor.inventory.iter().position(|item| {
            item.as_deref()
                .is_some_and(|id| !bri_weapons::CORE_TOOLS.contains(&id))
        });
        if weapon.is_some() && actor.selected != weapon {
            let _ = self.equip_tool(bot, weapon);
        }
        Ok(())
    }
    pub fn bot_choices() -> Vec<(String, String)> {
        BOT_KINDS
            .iter()
            .map(|(id, name)| (id.to_string(), name.to_string()))
            .collect()
    }
}
