//! Server-issued transient presentation. This is not a gameplay command channel.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
pub const MAX_CUES: usize = 4096;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CueKind {
    Jump,
    Plant,
    Break,
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
    /// `Player::playPain` after more than 10 damage.
    Pain {
        actor: u64,
    },
    /// `Armor::onDisabled`: death cry and death animation.
    Death {
        actor: u64,
    },
    /// `spawnProjectile` burst where a player (re)spawns.
    Spawn {
        actor: u64,
    },
    /// Emote image above the head (alarm, love, hate, confusion) or sit.
    Emote {
        actor: u64,
        name: String,
    },
    /// Vehicle audio: a trigger key (`vehicle.*`, `player.mount`) or an
    /// original sound profile name.
    VehicleSound {
        vehicle: u64,
        sound: String,
    },
    /// Vehicle emitter (burning, splash).
    VehicleEffect {
        vehicle: u64,
        effect: String,
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
                        bri_weapons::TargetId::Map(_) => true,
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
                    && image_hand.is_none_or(|h| h < 2),
                "Invalid weapon animation cue"
            ),
            CueKind::WeaponShell { actor, image, hand } => ensure!(
                *actor > 0 && !image.is_empty() && text(image) && *hand < 2,
                "Invalid shell cue"
            ),
            CueKind::Pain { actor } | CueKind::Death { actor } | CueKind::Spawn { actor } => {
                ensure!(*actor > 0, "Invalid player cue")
            }
            CueKind::VehicleSound { vehicle, sound: name }
            | CueKind::VehicleEffect {
                vehicle,
                effect: name,
            } => ensure!(
                *vehicle > 0 && !name.is_empty() && text(name),
                "Invalid vehicle cue"
            ),
            CueKind::Emote { actor, name } => ensure!(
                *actor > 0 && crate::session::EMOTES.contains(&name.as_str()),
                "Invalid emote cue"
            ),
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
        if self.pending.len() == MAX_CUES {
            self.pending.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.pending.push_back(Cue {
            id,
            tick,
            kind,
            position,
        });
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
