//! What a bot hears: an ally's alert and distant fighting.
use super::*;

impl Session {
    /// Bots of a warner's side within its reach that have nothing better
    /// to go on remember where the enemy was, and go and look (Bot_Hole's
    /// `hAlertOtherBots`).
    pub(super) fn hear_alerts(&mut self, tick: u64) {
        for alert in std::mem::take(&mut self.bots.alerts) {
            let Some(from) = self
                .peers
                .get(&alert.from)
                .map(|p| Vec3::from(p.player.state().feet))
            else {
                continue;
            };
            let heard: Vec<OwnerId> = self
                .bots
                .brains
                .iter()
                .filter(|(o, b)| {
                    **o != alert.from
                        && !b.resting
                        && b.target.is_none()
                        && b.memory
                            .is_none_or(|old| old.observed < alert.knowledge.observed)
                        && tick < alert.knowledge.expires
                        && self.bot_enemy(**o, &b.kind, alert.knowledge.subject)
                        && self.bot_allies(**o, alert.from)
                        && self.peers.get(o).is_some_and(|p| {
                            p.combat.alive
                                && Vec3::from(p.player.state().feet).distance(from) <= alert.reach
                        })
                })
                .map(|(o, _)| *o)
                .collect();
            for bot in heard {
                let brain = self.bots.brains.get_mut(&bot).unwrap();
                let expires = alert.knowledge.expires.min(
                    alert
                        .knowledge
                        .observed
                        .saturating_add((brain.kind.memory_seconds * 120.0) as u64),
                );
                if tick < expires {
                    // Acted on after a short, seeded reaction (`perception`).
                    let k = Knowledge {
                        expires,
                        ..alert.knowledge
                    };
                    brain.hear(k, bot, tick);
                }
            }
        }
    }
    /// A rules bot with nothing to go on that hears an enemy's weapon
    /// across the map goes to look where the fighting is, as a player
    /// follows the gunfire: it knows the spot only roughly, the farther the
    /// rougher, and acts on it after a moment (`perception`).
    pub(super) fn hear_fighting(&mut self, tick: u64) {
        for (from, at, volume) in std::mem::take(&mut self.bots.noises) {
            let hearing = FIGHT_HEARING * volume.clamp(0.0, 1.0);
            if hearing <= 0.0 {
                continue;
            }
            let heard: Vec<(OwnerId, f32)> = self
                .bots
                .brains
                .iter()
                .filter(|(o, b)| {
                    **o != from
                        && b.brick.is_none()
                        && !b.resting
                        && b.target.is_none()
                        && b.memory.is_none()
                        && self.bot_enemy(**o, &b.kind, from)
                })
                .filter_map(|(o, _)| {
                    let p = self.peers.get(o).filter(|p| p.combat.alive)?;
                    let d = Vec3::from(p.player.state().feet).distance(at);
                    (d <= hearing).then_some((*o, d))
                })
                .collect();
            for (bot, distance) in heard {
                let brain = self.bots.brains.get_mut(&bot).unwrap();
                let angle = brain.random() * std::f32::consts::TAU;
                let off = brain.random() * distance * FIGHT_HEARD_ROUGHLY;
                let k = Knowledge {
                    subject: from,
                    at: at + Vec3::new(angle.sin(), 0.0, angle.cos()) * off,
                    observed: tick,
                    expires: tick + (brain.kind.memory_seconds * 120.0) as u64,
                };
                brain.hear(k, bot, tick);
            }
        }
    }
}
