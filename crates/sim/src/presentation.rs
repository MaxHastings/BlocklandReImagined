//! Server-issued transient presentation. This is not a gameplay command channel.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
pub const MAX_CUES: usize = 4096;
/// Upper bound on a brick blast's force and falloff radius.
pub const MAX_BRICK_FORCE: f32 = 1000.;
/// How clients draw a brick's death. v20 draws the two differently
/// (`blocklandv20.exe`; see docs/audits/brick-damage.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrickDeath {
    /// `killBrick` (hammer, wands, undo, chain kills): the brick hops,
    /// spins and falls straight through everything while it fades. It
    /// never collides with anything.
    Kill,
    /// `transmitBrickExplosion` (`fakeKillBrick`, weapon blasts): a physics
    /// body thrown from `origin` that tumbles against the world.
    Blast,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CueKind {
    Jump,
    Plant,
    /// A brick was killed or fake-killed; `death` says how clients draw
    /// it. Blasts throw debris from `origin`; `Cue::position` is the
    /// brick's center. The look travels with the cue because the brick may
    /// already be gone from the client's world.
    BrickKill {
        brick: u64,
        death: BrickDeath,
        definition: bri_world::ContentRef,
        quarter_turns: u8,
        color: u8,
        color_effect: u8,
        shape_effect: u8,
        print: Option<bri_world::ContentRef>,
        origin: [f32; 3],
        force: f32,
        radius: f32,
    },
    HammerHit,
    WrenchHit,
    WeaponSound {
        profile: String,
    },
    WeaponEffect {
        source: bri_weapons::TargetId,
        definition: String,
        node: String,
        seconds: f32,
        image: Option<String>,
        hand: Option<u8>,
        direction: Option<[f32; 3]>,
        scale: f32,
    },
    WeaponAnimation {
        actor: u64,
        thread: u8,
        sequence: String,
        image_hand: Option<u8>,
    },
    WeaponShell {
        actor: u64,
        image: String,
        hand: u8,
    },
    /// `Armor::damage` pain emote from the pain level summed over 300 ms;
    /// `cry` is `Player::playPain` after a hit of more than 10.
    Pain {
        actor: u64,
        level: f32,
        cry: bool,
    },
    /// `Armor::onDisabled`: death cry and death animation.
    Death {
        actor: u64,
    },
    /// A player's feet crossed a water surface at this speed: the entry
    /// splash and `impactWater*` sound, or the `exitingWater` sound.
    Water {
        actor: u64,
        entered: bool,
        speed: f32,
    },
    /// `Player::burn`: `PlayerBurnImage` flames for this long.
    Burn {
        actor: u64,
        seconds: f32,
    },
    /// `Player::teleportEffect` and `Vehicle::teleportEffect`: a
    /// PlayerTeleportExplosion at the cue position, `scale` times its size.
    /// A player also wears PlayerTeleportImage on its back (`emote` slot).
    Teleport {
        actor: u64,
        scale: f32,
        player: bool,
    },
    /// Emote image above the head (alarm, love, hate, confusion) or sit.
    Emote {
        actor: u64,
        name: String,
    },
    /// Vehicle audio: a trigger key (`vehicle.*`, `player.mount`) or an
    /// original sound profile name. `vehicle` is 0 when a rider mounts a
    /// player rather than a vehicle.
    VehicleSound {
        vehicle: u64,
        sound: String,
    },
    /// Vehicle emitter or image (burning, splash, weapon smoke, fuse);
    /// inactive unmounts a lasting image such as the cannon fuse.
    VehicleEffect {
        vehicle: u64,
        effect: String,
        active: bool,
    },
    /// The engine's explosion operation went off here (package creatures,
    /// scripted blasts). Clients choose the look; `source` names the package.
    Explosion {
        radius: f32,
        source: String,
    },
    /// A package's straight beam from `Cue::position` to `to`, fading out
    /// over `seconds`: a tracer, a laser. With `muzzle`, clients start it
    /// at that player's held muzzle as they draw it.
    Beam {
        to: [f32; 3],
        color: [f32; 4],
        width: f32,
        seconds: f32,
        muzzle: Option<u64>,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub id: u64,
    pub tick: u64,
    pub kind: CueKind,
    pub position: [f32; 3],
}
impl Cue {
    pub fn validate(&self) -> Result<()> {
        let text = |s: &str| s.len() <= 128 && !s.chars().any(char::is_control);
        match &self.kind {
            CueKind::WeaponEffect {
                definition,
                node,
                seconds,
                source,
                image,
                hand,
                direction,
                scale,
            } => ensure!(
                !definition.is_empty()
                    && text(definition)
                    && text(node)
                    && seconds.is_finite()
                    && (0.0..=300.).contains(seconds)
                    && match source {
                        bri_weapons::TargetId::Actor(id) => id.0 > 0,
                        bri_weapons::TargetId::Brick(id) | bri_weapons::TargetId::Vehicle(id) =>
                            *id > 0,
                        bri_weapons::TargetId::Map(_) | bri_weapons::TargetId::Shape(_)
                        | bri_weapons::TargetId::Entity(_) => true,
                    }
                    && image.as_ref().is_none_or(|s| !s.is_empty() && text(s))
                    && image.is_some() == hand.is_some()
                    && hand.is_none_or(|h| h < 2)
                    && direction.is_none_or(|v| {
                        v.iter().all(|x| x.is_finite())
                            && (v.iter().map(|x| x * x).sum::<f32>() - 1.).abs() < 0.001
                    })
                    && scale.is_finite()
                    && (0.01..=100.).contains(scale),
                "Invalid weapon effect cue"
            ),
            CueKind::WeaponAnimation {
                actor,
                thread,
                sequence,
                image_hand,
            } => ensure!(
                *actor > 0
                    && *thread < 16
                    && !sequence.is_empty()
                    && text(sequence)
                    && image_hand.is_none_or(|h| usize::from(h) < bri_weapons::IMAGE_SLOTS),
                "Invalid weapon animation cue"
            ),
            CueKind::WeaponShell { actor, image, hand } => ensure!(
                *actor > 0 && !image.is_empty() && text(image) && *hand < 2,
                "Invalid shell cue"
            ),
            CueKind::Death { actor } => ensure!(*actor > 0, "Invalid player cue"),
            CueKind::Pain { actor, level, .. } => ensure!(
                *actor > 0 && level.is_finite() && (0.0..=1e6).contains(level),
                "Invalid pain cue"
            ),
            CueKind::Water { actor, speed, .. } => ensure!(
                *actor > 0 && speed.is_finite() && (0.0..=10000.0).contains(speed),
                "Invalid water cue"
            ),
            CueKind::Burn { actor, seconds } => ensure!(
                *actor > 0 && seconds.is_finite() && (0.0..=300.).contains(seconds),
                "Invalid burn cue"
            ),
            // A player mount (a rider on a horse player) has no vehicle.
            CueKind::VehicleSound { sound, .. } => ensure!(
                !sound.is_empty() && text(sound),
                "Invalid vehicle sound cue"
            ),
            CueKind::VehicleEffect {
                vehicle, effect, ..
            } => ensure!(
                *vehicle > 0 && !effect.is_empty() && text(effect),
                "Invalid vehicle effect cue"
            ),
            CueKind::Explosion { radius, source } => ensure!(
                radius.is_finite() && (0.0..=64.0).contains(radius) && !source.is_empty() && text(source),
                "Invalid explosion cue"
            ),
            CueKind::Beam {
                to,
                color,
                width,
                seconds,
                muzzle,
            } => ensure!(
                to.iter().all(|c| c.is_finite() && c.abs() <= 1_000_000.0)
                    && color.iter().all(|c| (0.0..=1.0).contains(c))
                    && width.is_finite()
                    && *width > 0.0
                    && *width <= bri_package_runtime::ops::MAX_BEAM_WIDTH
                    && seconds.is_finite()
                    && *seconds > 0.0
                    && *seconds <= bri_package_runtime::ops::MAX_BEAM_SECONDS
                    && muzzle.is_none_or(|m| m > 0),
                "Invalid beam cue"
            ),
            CueKind::Teleport { actor, scale, .. } => ensure!(
                *actor > 0 && scale.is_finite() && (0.01..=100.0).contains(scale),
                "Invalid teleport cue"
            ),
            CueKind::Emote { actor, name } => ensure!(
                *actor > 0 && crate::session::EMOTES.contains(&name.as_str()),
                "Invalid emote cue"
            ),
            CueKind::BrickKill {
                brick,
                definition,
                quarter_turns,
                color_effect,
                shape_effect,
                print,
                origin,
                force,
                radius,
                ..
            } => {
                definition.validate()?;
                if let Some(print) = print {
                    print.validate()?;
                }
                ensure!(
                    *brick > 0
                        && matches!(definition, bri_world::ContentRef::Resolved(_))
                        && *quarter_turns < 4
                        && *color_effect <= 6
                        && *shape_effect <= 2
                        && origin
                            .iter()
                            .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
                        && (0. ..=MAX_BRICK_FORCE).contains(force)
                        && (0. ..=MAX_BRICK_FORCE).contains(radius),
                    "Invalid brick kill cue"
                )
            }
            _ => {}
        }
        if let CueKind::WeaponSound { profile } = &self.kind {
            ensure!(
                !profile.is_empty()
                    && profile.len() <= 128
                    && !profile.chars().any(char::is_control),
                "Invalid weapon sound profile"
            );
        }
        ensure!(
            self.id > 0
                && self
                    .position
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1_000_000.),
            "Invalid presentation cue"
        );
        Ok(())
    }
}
#[derive(Default)]
pub struct Cues {
    cursor: u64,
    pending: VecDeque<Cue>,
    dropped: u64,
}
impl Cues {
    pub fn cursor(&self) -> u64 {
        self.cursor
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn emit(&mut self, tick: u64, kind: CueKind, position: [f32; 3]) {
        // Never make a successfully applied gameplay operation fail over cosmetic capacity.
        let Some(id) = self.cursor.checked_add(1) else {
            self.dropped = self.dropped.saturating_add(1);
            return;
        };
        self.cursor = id;
        let cue = Cue {
            id,
            tick,
            kind,
            position,
        };
        // Clients drop the connection over a cue they reject, so the host
        // must only ever emit cues that pass the client's check.
        debug_assert!(cue.validate().is_ok(), "{:?}: {cue:?}", cue.validate());
        if self.pending.len() == MAX_CUES {
            self.pending.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.pending.push_back(cue);
    }
    pub fn take(&mut self) -> Vec<Cue> {
        self.pending.drain(..).collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weapon_effect_pose_metadata_rejects_partial_or_invalid_identity() {
        let mut cue = Cue {
            id: 1,
            tick: 1,
            position: [0.; 3],
            kind: CueKind::WeaponEffect {
                source: bri_weapons::TargetId::Actor(bri_weapons::ActorId(1)),
                definition: "gunFlashEmitter".into(),
                node: "muzzleNode".into(),
                seconds: 0.05,
                image: Some("v20.image.gunimage".into()),
                hand: Some(1),
                direction: Some([0., 0., -1.]),
                scale: 1.,
            },
        };
        assert!(cue.validate().is_ok());
        if let CueKind::WeaponEffect { hand, .. } = &mut cue.kind {
            *hand = None;
        }
        assert!(cue.validate().is_err());
        if let CueKind::WeaponEffect {
            image,
            direction,
            source,
            ..
        } = &mut cue.kind
        {
            *image = None;
            *direction = Some([0.; 3]);
            *source = bri_weapons::TargetId::Map(0);
        }
        assert!(cue.validate().is_err());
        if let CueKind::WeaponEffect { direction, .. } = &mut cue.kind {
            *direction = None;
        }
        assert!(
            cue.validate().is_ok(),
            "Map zero is a valid collision identity"
        );
        if let CueKind::WeaponEffect { source, .. } = &mut cue.kind {
            *source = bri_weapons::TargetId::Actor(bri_weapons::ActorId(0));
        }
        assert!(cue.validate().is_err());
    }
    /// Max's v0.1.0-alpha report: riding a horse player dropped every
    /// client with "Invalid vehicle cue", because the mount sound of a player
    /// mount names no vehicle.
    #[test]
    fn a_player_mount_sound_needs_no_vehicle_but_an_effect_does() {
        let cue = |kind| Cue {
            id: 1,
            tick: 1,
            position: [0.; 3],
            kind,
        };
        assert!(
            cue(CueKind::VehicleSound {
                vehicle: 0,
                sound: "player.mount".into(),
            })
            .validate()
            .is_ok()
        );
        assert!(
            cue(CueKind::VehicleSound {
                vehicle: 3,
                sound: String::new(),
            })
            .validate()
            .is_err()
        );
        assert!(
            cue(CueKind::VehicleEffect {
                vehicle: 0,
                effect: "burn".into(),
                active: true,
            })
            .validate()
            .is_err()
        );
    }
    #[test]
    fn cosmetic_overflow_is_bounded_observable_and_never_reuses_ids() {
        let mut cues = Cues::default();
        for _ in 0..MAX_CUES + 3 {
            cues.emit(9, CueKind::Jump, [0.; 3]);
        }
        let cursor = cues.cursor();
        let first = cues.take();
        assert_eq!(cues.dropped(), 3);
        assert_eq!(first.len(), MAX_CUES);
        assert_eq!(first[0].id, 4);
        assert_eq!(first.last().unwrap().id, cursor);
        assert!(cues.take().is_empty());
        cues.emit(10, CueKind::Plant, [1.; 3]);
        assert_eq!(cues.take()[0].id, cursor + 1);
    }
}
