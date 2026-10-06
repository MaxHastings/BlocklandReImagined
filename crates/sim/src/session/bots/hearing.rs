//! What a bot knows beyond its eyes: an ally's alert, distant fighting,
//! and who just hurt it.
use super::*;

impl Session {
    /// What it knows of enemies beyond its eyes this tick: what it heard
    /// becomes its memory when it has nothing fresher, a hit this tick names
    /// who hurt it (only roughly where from, out of sight), stale or no
    /// longer hostile memory is dropped, and the threat it answers is the
    /// hurt or the one its objective already faces. The hurt, and the
    /// threat.
    pub(super) fn bot_evidence(
        &mut self,
        bot: OwnerId,
        target: Option<Seen>,
        feet: Vec3,
        tick: u64,
    ) -> (Option<Knowledge>, Option<Knowledge>) {
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        if let Some(k) = brain.perception.heard(tick)
            && brain.target.is_none()
            && brain.memory.is_none_or(|old| old.observed < k.observed)
        {
            brain.memory = Some(k);
        }
        let hurt_by = self.bots.hurt.remove(&bot).filter(|k| {
            tick < k.expires && self.bot_enemy(bot, &self.bots.brains[&bot].kind, k.subject)
        });
        // Out of sight, a hit gives only a rough idea where from.
        let hurt_by = hurt_by.map(|k| match target {
            Some(seen) if seen.owner == k.subject => k,
            _ => Knowledge {
                at: perception::guess(feet, k.at, &mut self.bots.brains.get_mut(&bot).unwrap().rng),
                ..k
            },
        });
        if self.bots.brains[&bot].memory.is_some_and(|k| {
            tick >= k.expires || !self.bot_enemy(bot, &self.bots.brains[&bot].kind, k.subject)
        }) {
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.memory = None;
            brain.evidence_search.clear();
        }
        let threat = hurt_by
            .or(self.bots.brains[&bot].objective_threat)
            .filter(|k| {
                tick < k.expires && self.bot_enemy(bot, &self.bots.brains[&bot].kind, k.subject)
            });
        self.bots.brains.get_mut(&bot).unwrap().objective_threat = threat;
        (hurt_by, threat)
    }
    /// Where a bot stands if it would take in news of `subject` seen at
    /// `observed`: alive, awake, nothing in sight, nothing it remembers as
    /// fresh, and `subject` its enemy. The one test of who listens, to an
    /// ally's alert and to fighting heard across the map alike.
    fn listener(&self, bot: OwnerId, b: &Brain, subject: OwnerId, observed: u64) -> Option<Vec3> {
        let p = self.peers.get(&bot).filter(|p| p.combat.alive)?;
        (bot != subject
            && !b.resting
            && b.target.is_none()
            && b.memory.is_none_or(|old| old.observed < observed)
            && self.bot_enemy(bot, &b.kind, subject))
        .then(|| Vec3::from(p.player.state().feet))
    }
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
            if tick >= alert.knowledge.expires {
                continue;
            }
            let k = alert.knowledge;
            let heard: Vec<OwnerId> = self
                .bots
                .brains
                .iter()
                .filter(|(o, _)| **o != alert.from && self.bot_allies(**o, alert.from))
                .filter(|(o, b)| {
                    self.listener(**o, b, k.subject, k.observed)
                        .is_some_and(|feet| feet.distance(from) <= alert.reach)
                })
                .map(|(o, _)| *o)
                .collect();
            for bot in heard {
                let brain = self.bots.brains.get_mut(&bot).unwrap();
                let expires = k
                    .expires
                    .min(k.observed.saturating_add(ticks(brain.kind.memory_seconds)));
                if tick < expires {
                    // Acted on after a short, seeded reaction (`perception`).
                    brain.hear(Knowledge { expires, ..k }, bot, tick);
                }
            }
        }
    }
    /// An untethered bot (`Brain::tethered`) with nothing to go on that
    /// hears an enemy's weapon across the map goes to look where the
    /// fighting is, as a player follows the gunfire: it knows the spot only
    /// roughly, the farther the rougher, and acts on it after a moment
    /// (`perception`).
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
                .filter(|(_, b)| !b.tethered() && b.memory.is_none())
                .filter_map(|(o, b)| {
                    let d = self.listener(*o, b, from, tick)?.distance(at);
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
                    expires: tick + ticks(brain.kind.memory_seconds),
                };
                brain.hear(k, bot, tick);
            }
        }
    }
}
