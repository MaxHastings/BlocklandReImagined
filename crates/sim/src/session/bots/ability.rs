//! A kind's own attacks besides its hands: an ability its Add-On defines
//! and harm on touch (`BotKind::ability`, `BotKind::contact`).
//!
//! The brain owns when: it walks into the ability's range of an enemy as
//! it would to strike (the ability is a body attack with that reach),
//! waits out its reaction, then winds the ability up. The wind-up is data:
//! an action its model plays, an image it wears, a sound repeated at it,
//! called off when the enemy gets away. Then the Add-On's
//! `on_bot_ability` hooks say what happens; nothing here knows whether
//! that is a blast, a spit or a heal (`docs/architecture/bots.md`,
//! Attacks).
use super::*;
use crate::bot_kind::{BotAbility, BotContact};

/// The animation thread an ability's action plays on.
const ABILITY_THREAD: u8 = MELEE_THREAD;

/// An ability being wound up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Fuse {
    /// Who it is aimed at.
    pub target: Option<OwnerId>,
    /// When the hooks run.
    pub ends: u64,
    /// When its sound plays next.
    pub next_sound: u64,
}

/// A bot's ability and touch state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct State {
    pub fuse: Option<Fuse>,
    /// Not again before this tick.
    pub next: u64,
    /// No harm on touch before this tick.
    pub next_touch: u64,
}

/// What the ability asks of the bot's feet this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Winding {
    /// It is winding up.
    pub on: bool,
    /// Its kind's ability stands still to wind up.
    pub stand: bool,
}

impl Session {
    /// Starts, keeps on, calls off or finishes `bot`'s ability against
    /// `enemy` (in sight, or remembered when the ability does not need
    /// sight): started once its reaction has passed with an enemy within
    /// the ability's range, while it fights or chases.
    pub(super) fn bot_ability(
        &mut self,
        bot: OwnerId,
        enemy: Option<(OwnerId, Vec3)>,
        seen: bool,
        fighting: bool,
        feet: Vec3,
        tick: u64,
    ) -> Result<Winding> {
        let kind = self.bots.brains[&bot].kind.clone();
        let Some(ability) = &kind.ability else {
            return Ok(Winding::default());
        };
        let state = self.bots.brains[&bot].ability;
        if let Some(fuse) = state.fuse {
            // Called off when its enemy dies, leaves or gets away.
            let away = fuse.target.and_then(|t| {
                self.peers
                    .get(&t)
                    .filter(|p| p.combat.alive)
                    .map(|p| flat(Vec3::from(p.player.state().feet) - feet).length())
            });
            if fuse.target.is_some() && away.is_none_or(|d| d > ability.cancel_range) {
                self.bot_ability_off(bot, ability, tick, false);
                return Ok(Winding::default());
            }
            if tick >= fuse.ends {
                self.bot_ability_off(bot, ability, tick, true);
                self.package_bot_ability(bot, &kind.id, &ability.name, fuse.target);
                return Ok(Winding::default());
            }
            if tick >= fuse.next_sound {
                self.bot_ability_sound(bot, ability, feet, tick);
                if let Some(brain) = self.bots.brains.get_mut(&bot)
                    && let Some(f) = brain.ability.fuse.as_mut()
                {
                    f.next_sound = tick + ticks(ability.sound_seconds).max(1);
                }
            }
            return Ok(Winding {
                on: true,
                stand: ability.stand,
            });
        }
        let brain = &self.bots.brains[&bot];
        let reacted = tick >= brain.seen_since + ticks(brain.kind.reaction_seconds);
        let Some((target, at)) = enemy.filter(|_| {
            fighting && tick >= state.next && (seen && reacted || !seen && !ability.sight)
        }) else {
            return Ok(Winding::default());
        };
        if flat(at - feet).length() > ability.range || !self.can_damage_player(bot, target, false) {
            return Ok(Winding::default());
        }
        if let Some(action) = &ability.action {
            self.play_thread(tick, bot, ABILITY_THREAD, action);
        }
        if let Some(image) = &ability.emote {
            self.emote_cue(
                tick,
                crate::presentation::CueKind::Emote {
                    actor: bot,
                    name: image.clone(),
                },
                feet.to_array(),
            );
        }
        self.bot_ability_sound(bot, ability, feet, tick);
        let ends = tick + ticks(ability.fuse_seconds);
        self.bots.brains.get_mut(&bot).unwrap().ability.fuse = Some(Fuse {
            target: Some(target),
            ends,
            next_sound: tick + ticks(ability.sound_seconds).max(1),
        });
        // No wind-up: it goes off now.
        if ability.fuse_seconds <= 0.0 {
            return self.bot_ability(bot, enemy, seen, fighting, feet, tick);
        }
        Ok(Winding {
            on: true,
            stand: ability.stand,
        })
    }

    /// Ends a wind-up, finished or called off: the image comes off and
    /// the cooldown starts.
    fn bot_ability_off(&mut self, bot: OwnerId, ability: &BotAbility, tick: u64, done: bool) {
        if ability.emote.is_some() && ability.fuse_seconds > 0.0 {
            let feet = self.peers[&bot].player.state().feet;
            self.emote_cue(
                tick,
                crate::presentation::CueKind::Emote {
                    actor: bot,
                    name: String::new(),
                },
                feet,
            );
        }
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        brain.ability.fuse = None;
        // Called off, it may try again as soon as it is back in range.
        brain.ability.next = if done {
            tick + ticks(ability.cooldown_seconds)
        } else {
            tick + ticks(brain.kind.reaction_seconds)
        };
    }

    fn bot_ability_sound(&mut self, bot: OwnerId, ability: &BotAbility, feet: Vec3, tick: u64) {
        if let Some(sound) = &ability.sound {
            let _ = bot;
            self.cues.emit(
                tick,
                crate::presentation::CueKind::WeaponSound {
                    profile: sound.clone(),
                },
                (feet + Vec3::Y).to_array(),
            );
        }
    }

    /// Harm on touch: each enemy whose body touches `bot`'s, while its
    /// kind's `contact` hurts and the rules let it (`can_damage_player`).
    pub(super) fn bot_contact(&mut self, bot: OwnerId, tick: u64) -> Result<()> {
        let kind = self.bots.brains[&bot].kind.clone();
        let Some(BotContact {
            damage,
            seconds,
            name,
        }) = &kind.contact
        else {
            return Ok(());
        };
        let (damage, seconds) = (*damage, *seconds);
        if damage <= 0.0 || tick < self.bots.brains[&bot].ability.next_touch {
            return Ok(());
        }
        let Some(me) = self.peers.get(&bot) else {
            return Ok(());
        };
        let (lo, hi) = body_box(me);
        let touched: Vec<OwnerId> = self
            .peers
            .iter()
            .filter(|(o, p)| {
                **o != bot && p.combat.alive && {
                    let (a, b) = body_box(p);
                    a.cmple(hi).all() && lo.cmple(b).all()
                }
            })
            .map(|(o, _)| *o)
            .filter(|o| self.bot_enemy(bot, &kind, *o) && self.can_damage_player(bot, *o, false))
            .collect();
        if touched.is_empty() {
            return Ok(());
        }
        self.bots.brains.get_mut(&bot).unwrap().ability.next_touch = tick + ticks(seconds).max(1);
        for target in touched {
            let at = Vec3::from(self.peers[&target].player.state().feet) + Vec3::Y;
            self.damage_player_at(
                target,
                damage,
                combat::DamageKind::weapon(name.clone(), true),
                Some(bot),
                Some(at),
            )?;
        }
        Ok(())
    }
}

/// How much bodies may be apart and still touch.
const TOUCH: f32 = 0.15;

/// A player's body box, grown by [`TOUCH`].
fn body_box(p: &Peer) -> (Vec3, Vec3) {
    let state = p.player.state();
    let feet = Vec3::from(state.feet);
    let half = p.player.tuning().width * state.scale * 0.5 + TOUCH;
    let height = crate::water::body_height(state, p.player.tuning()) * state.scale;
    (
        feet - Vec3::new(half, TOUCH, half),
        feet + Vec3::new(half, height + TOUCH, half),
    )
}
