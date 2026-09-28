//! Stock special bricks whose behavior lives in their add-on scripts:
//! checkpoints, teledoors, treasure chests and carvable pumpkins. (Water
//! bricks are liquid volumes in the simulation.)
use super::*;
use crate::definitions::Special;

const TREASURE_OPEN: &str = "v20/brick/bricktreasurechestopendata";
const TREASURE_CLOSED: &str = "v20/brick/bricktreasurechestdata";
const PUMPKIN_FACES: [&str; 3] = [
    "v20/brick/brickpumpkinfacedata",
    "v20/brick/brickpumpkinscareddata",
    "v20/brick/brickpumpkinasciidata",
];
const SWORD_PROJECTILE: &str = "v20.projectile.swordprojectile";
/// The chest shows open for two seconds (`schedule(2000, setDataBlock)`).
const CHEST_OPEN_TICKS: u64 = 240;
/// `lastTeledoorTime` guard: 30 ms.
const TELEDOOR_COOLDOWN_TICKS: u64 = 4;
/// `0.5 + (boundingBox / 4) / 2` for the standard player.
const TELEDOOR_OFFSET: f32 = 1.125;

/// Per-player progress with special bricks.
#[derive(Default)]
pub(super) struct Progress {
    checkpoint: Option<BrickId>,
    /// Name given to the next teledoor this player plants, pairing doors.
    teledoor_name: Option<(String, u8)>,
    last_teledoor: u64,
    /// Treasure chests found, by position and rotation like the original hash.
    chests: BTreeSet<([u32; 3], u8)>,
}

/// Session-wide special brick state.
#[derive(Default)]
pub(super) struct Specials {
    /// Open chests and when they close again.
    closing: BTreeMap<BrickId, u64>,
}

fn chest_key(brick: &Brick) -> ([u32; 3], u8) {
    (brick.position.map(f32::to_bits), brick.quarter_turns)
}

impl Session {
    fn special_of(&self, brick: BrickId) -> Special {
        self.simulation
            .state()
            .bricks
            .get(&brick)
            .and_then(|b| self.simulation.definitions.get(b).ok())
            .map_or(Special::None, |d| d.special)
    }
    /// Add the default event rows the original `onPlant` callbacks add.
    pub(super) fn special_planted(&mut self, owner: OwnerId, brick: BrickId) -> Result<()> {
        let special = self.special_of(brick);
        let default_row =
            |input: &str, target: bri_events::Slot, sound: &str| bri_world::EventRow {
                preserved: None,
                enabled: true,
                input: input.into(),
                delay_ms: 0,
                target: bri_world::EventTarget::Slot(target),
                output: "playSound".into(),
                params: vec![bri_world::EventValue::Datablock(Some(sound.into()))],
            };
        let row = match special {
            Special::Checkpoint => Some(default_row(
                "onPlayerTouch",
                bri_events::Slot::SelfBrick,
                "v20/sound/beep_popup_sound",
            )),
            Special::TreasureChest => Some(default_row(
                "onActivate",
                bri_events::Slot::Client,
                "v20/sound/rewardsound",
            )),
            _ => None,
        };
        if let Some(row) = row
            && self.validate_event_rows(std::slice::from_ref(&row)).is_ok()
        {
            self.simulation.mutate(brick, |b| b.events.push(row))?;
        }
        if special == Special::Teledoor
            && let Some(peer) = self.peers.get_mut(&owner)
        {
            // Consecutive teledoors share a name so they lead to each other.
            let (name, count) = match peer.special.teledoor_name.take() {
                Some((name, count)) => (name, count),
                None => {
                    // `sha1(getTransform())` prefix in the original; any
                    // stable per-door hash serves.
                    let position = self.simulation.state().bricks[&brick].position;
                    let hash = position
                        .iter()
                        .flat_map(|v| v.to_bits().to_le_bytes())
                        .chain(brick.to_le_bytes())
                        .fold(0xcbf2_9ce4_8422_2325u64, |h, byte| {
                            (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
                        });
                    (format!("Teledoor_{:05x}", hash >> 44), 0)
                }
            };
            if count + 1 < 2 {
                peer.special.teledoor_name = Some((name.clone(), count + 1));
            }
            self.simulation.mutate(brick, |b| b.name = Some(name))?;
        }
        Ok(())
    }
    /// Touch with special behavior first, then the brick's own events.
    pub(super) fn special_touch(&mut self, owner: OwnerId, brick: BrickId) -> Result<bool> {
        match self.special_of(brick) {
            Special::Checkpoint => {
                if self.is_bot(owner) {
                    return Ok(true);
                }
                let Some(peer) = self.peers.get_mut(&owner) else {
                    return Ok(true);
                };
                if peer.special.checkpoint == Some(brick) {
                    return Ok(true);
                }
                peer.special.checkpoint = Some(brick);
                self.notify(
                    owner,
                    Notice::Bottom {
                        text: "\u{E004}Checkpoint reached! \u{E007}- Say /clearCheckpoint to go back to the beginning".into(),
                        seconds: 3.0,
                        hide_bar: false,
                    },
                );
                Ok(false)
            }
            Special::Teledoor => {
                self.fire_touch_events(owner, brick);
                self.teleport_through(owner, brick)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn teleport_through(&mut self, owner: OwnerId, from: BrickId) -> Result<()> {
        let tick = self.simulation.state().tick;
        let world = self.simulation.state();
        let Some(source) = world.bricks.get(&from) else {
            return Ok(());
        };
        let Some(name) = source.name.clone() else {
            return Ok(());
        };
        let Some(peer) = self.peers.get(&owner) else {
            return Ok(());
        };
        if tick.saturating_sub(peer.special.last_teledoor) < TELEDOOR_COOLDOWN_TICKS {
            return Ok(());
        }
        // The next teledoor with this name in the owner's bricks, in order.
        let doors: Vec<BrickId> = world
            .bricks
            .iter()
            .filter(|(_, b)| {
                b.owner == source.owner
                    && b.name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(&name))
            })
            .filter(|(_, b)| {
                self.simulation
                    .definitions
                    .get(b)
                    .is_ok_and(|d| d.special == Special::Teledoor)
            })
            .map(|(id, _)| *id)
            .collect();
        if doors.len() < 2 {
            return Ok(());
        }
        let index = doors.iter().position(|d| *d == from).unwrap_or(0);
        let target = doors[(index + 1) % doors.len()];
        let exit = &world.bricks[&target];
        let (min, _) = self.simulation.brick_box(target).unwrap();
        let exit_turns = exit.quarter_turns % 4;
        // Torque angle IDs step clockwise from +X (east); native -Z is north.
        let offset = match exit_turns {
            0 => Vec3::X,
            1 => Vec3::Z,
            2 => Vec3::NEG_X,
            _ => Vec3::NEG_Z,
        } * TELEDOOR_OFFSET;
        let feet = Vec3::new(exit.position[0], min.y + 0.1, exit.position[2]) + offset;
        let delta = (exit_turns + 4 - source.quarter_turns % 4) % 4;
        let peer = self.peers.get_mut(&owner).unwrap();
        let state = peer.player.state();
        let velocity = Vec3::from(state.velocity);
        // Torque plane (x, y) = native (x, -z); the exit reverses or turns it.
        let (vx, vy) = (velocity.x, -velocity.z);
        let (out_x, out_y, turn) = match delta {
            0 => (-vx, -vy, std::f32::consts::PI),
            1 => (-vy, vx, -std::f32::consts::FRAC_PI_2),
            2 => (vx, vy, 0.0),
            _ => (vy, -vx, std::f32::consts::FRAC_PI_2),
        };
        // Torque turns counter-clockwise about +Z; native yaw turns clockwise.
        let yaw = state.yaw - turn;
        let new_velocity = Vec3::new(out_x, 0.0, -out_y);
        peer.player
            .teleport(&mut self.simulation.physics, feet, yaw)?;
        peer.player.push(new_velocity);
        peer.inputs.clear();
        peer.special.last_teledoor = tick;
        self.fire_input(from, "onTeledoorEnter", Some(owner));
        self.fire_input(target, "onTeledoorExit", Some(owner));
        Ok(())
    }
    /// Activation of a treasure chest. Returns whether the brick's own
    /// onActivate events should run.
    pub(super) fn special_activate(&mut self, owner: OwnerId, brick: BrickId) -> Result<bool> {
        match self.special_of(brick) {
            Special::TreasureChestOpen => Ok(false),
            Special::TreasureChest => {
                let world = self.simulation.state();
                let key = chest_key(&world.bricks[&brick]);
                let total = world
                    .bricks
                    .values()
                    .filter(|b| {
                        self.simulation.definitions.get(b).is_ok_and(|d| {
                            matches!(
                                d.special,
                                Special::TreasureChest | Special::TreasureChestOpen
                            )
                        })
                    })
                    .count();
                let Some(peer) = self.peers.get_mut(&owner) else {
                    return Ok(false);
                };
                if !peer.special.chests.insert(key) {
                    let found = peer.special.chests.len();
                    self.notify(
                        owner,
                        Notice::Bottom {
                            text: format!(
                                "\u{E003}You already opened this treasure chest ({found} / {total} found)"
                            ),
                            seconds: 2.0,
                            hide_bar: false,
                        },
                    );
                    return Ok(false);
                }
                let found = peer.special.chests.len();
                let name = peer.name.clone();
                self.simulation.set_definition(brick, TREASURE_OPEN)?;
                self.dirty.insert(brick);
                let tick = self.simulation.state().tick;
                self.specials.closing.insert(brick, tick + CHEST_OPEN_TICKS);
                let text = if found >= total && total == 1 {
                    "\u{E003}You have found the treasure chest!".to_string()
                } else if found >= total {
                    self.system_chat(format!(
                        "\u{E003}{name}\u{E000} found all \u{E006}{total}\u{E000} treasure chests!"
                    ));
                    format!("\u{E003}You have found all {total} treasure chests!")
                } else {
                    format!("\u{E003}You have found {found} / {total} treasure chests!")
                };
                self.notify(
                    owner,
                    Notice::Bottom {
                        text,
                        seconds: 2.0,
                        hide_bar: false,
                    },
                );
                Ok(true)
            }
            _ => Ok(true),
        }
    }
    /// A projectile hit a brick: swords carve pumpkins.
    pub(super) fn special_projectile_hit(
        &mut self,
        source: OwnerId,
        brick: BrickId,
        projectile: &str,
    ) -> Result<()> {
        if projectile != SWORD_PROJECTILE || self.special_of(brick) != Special::Pumpkin {
            return Ok(());
        }
        let owner = self.simulation.state().bricks[&brick].owner;
        let trusted = owner == source
            || owner == 0
            || self
                .peers
                .get(&source)
                .is_some_and(|p| p.actor.administrator);
        if !trusted {
            return Ok(());
        }
        self.spawn_seed = self
            .spawn_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let face = PUMPKIN_FACES[(self.spawn_seed >> 33) as usize % PUMPKIN_FACES.len()];
        self.simulation.set_definition(brick, face)?;
        self.dirty.insert(brick);
        Ok(())
    }
    /// Close chests whose two seconds are up.
    pub(super) fn step_specials(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        let due: Vec<BrickId> = self
            .specials
            .closing
            .iter()
            .filter(|(_, at)| **at <= tick)
            .map(|(id, _)| *id)
            .collect();
        for brick in due {
            self.specials.closing.remove(&brick);
            if self.special_of(brick) == Special::TreasureChestOpen {
                self.simulation.set_definition(brick, TREASURE_CLOSED)?;
                self.dirty.insert(brick);
                let position = self.simulation.state().bricks[&brick].position;
                self.cues
                    .emit(tick, crate::presentation::CueKind::Plant, position);
            }
        }
        Ok(())
    }
    /// A checkpoint brick overrides where this player spawns.
    pub(super) fn checkpoint_spawn(&self, owner: OwnerId) -> Option<(Vec3, f32)> {
        let brick = self.peers.get(&owner)?.special.checkpoint?;
        if self.special_of(brick) != Special::Checkpoint {
            return None;
        }
        let (_, max) = self.simulation.brick_box(brick)?;
        let b = &self.simulation.state().bricks[&brick];
        let yaw = -f32::from(b.quarter_turns) * std::f32::consts::FRAC_PI_2;
        Some((Vec3::new(b.position[0], max.y, b.position[2]), yaw))
    }
    /// `/clearCheckpoint`: forget the checkpoint and respawn at the start.
    pub(super) fn clear_checkpoint(&mut self, owner: OwnerId) -> Result<()> {
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        if peer.special.checkpoint.take().is_none() {
            return Ok(());
        }
        let player = peer.combat.player;
        self.notify(owner, Notice::Chat("\u{E003}Checkpoint reset".into()));
        let effects = self
            .minigames
            .event_respawn(player)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.apply_minigame_effects(effects)
    }
    /// `/treasureStatus`.
    pub(super) fn treasure_status(&mut self, owner: OwnerId) -> Result<()> {
        let total = self
            .simulation
            .state()
            .bricks
            .values()
            .filter(|b| {
                self.simulation.definitions.get(b).is_ok_and(|d| {
                    matches!(
                        d.special,
                        Special::TreasureChest | Special::TreasureChestOpen
                    )
                })
            })
            .count();
        let found = self
            .peers
            .get(&owner)
            .context("Unknown connection")?
            .special
            .chests
            .len();
        let text = match (total, found) {
            (1, 1..) => "\u{E003}You already found the treasure chest".to_string(),
            (1, _) => "\u{E003}You haven't found the treasure chest yet".to_string(),
            (t, f) if f >= t => format!("\u{E003}You have found all {t} treasure chests!"),
            (t, f) => format!("\u{E003}You have found {f} of {t} treasure chests!"),
        };
        self.notify(
            owner,
            Notice::Bottom {
                text,
                seconds: 2.0,
                hide_bar: false,
            },
        );
        Ok(())
    }
}
