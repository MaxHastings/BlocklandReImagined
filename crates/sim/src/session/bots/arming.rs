//! Arming and upgrading: a bot goes for a weapon it can see lying in reach,
//! one on an item brick or one dropped in the world. Empty-handed, any item
//! with an attack arms it, and arming takes over at once. Armed, an item is
//! an upgrade worth going for by how much more it is worth than the best it
//! holds (`hand_combat::item_worth`, the same estimate that ranks its
//! inventory), less the walk there and the risk of turning from a close
//! enemy; the hold rule and surprise's boredom weigh it like any behaviour.
//! Either needs a free slot. Picking it up is the ordinary contact pickup;
//! the bot only walks to it. An item whose pickup a package script decides
//! is the package's business and is left alone. An item it reached without
//! getting, or could not find a way to, is passed over for a while.
use super::*;

/// How far off it looks for an item to arm itself with.
const REACH: f32 = 24.0;
/// How long it stands at an item it does not get before passing it over.
const AT_ITEM: u64 = 120;
/// How long an item it passed over stays passed over.
const SKIP: u64 = 30 * 120;
/// Items it remembers passing over at once.
const PASSED_OVER: usize = 32;
/// An empty hand's arming score: before going after an enemy or an
/// objective that wants one beaten.
pub(super) const ARM: f32 = 0.95;
/// The most an upgrade scores, for a weapon worth far more than its best,
/// lying at its feet with no enemy about.
const UPGRADE: f32 = 0.75;
/// How much of an upgrade's score the walk there costs, at `REACH`.
const TRAVEL: f32 = 0.4;
/// Within this many units an enemy is close: turning to an item costs the
/// most.
const CLOSE: f32 = 8.0;
/// Past this many units an enemy costs an upgrade nothing.
const FAR: f32 = 32.0;
/// What an item an ally went for first is worth to another bot, as a share.
const TAKEN: f32 = 0.5;

/// Where an item to arm with lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    /// On an item brick.
    Brick(bri_world::BrickId),
    /// Dropped in the world.
    Drop(u64),
}

/// The item a bot is going for, and the items it passed over.
#[derive(Clone, Debug, Default)]
pub(super) struct Arming {
    /// The item, and since when it stood at it.
    going: Option<(Source, Option<u64>)>,
    /// Items passed over, until when.
    skipped: super::cooldown::Cooldowns<Source, PASSED_OVER>,
}

impl Arming {
    /// Stop going for an item (it armed, or it no longer wants to).
    pub(super) fn clear(&mut self) {
        self.going = None;
    }
    /// Pass over the item it was going for: it found no way there.
    pub(super) fn pass_over(&mut self, tick: u64) {
        if let Some((source, _)) = self.going.take() {
            self.skipped.give_up(source, tick + SKIP);
        }
    }
}

/// How much an item is worth going for, from 0: `worth` against the best it
/// holds (`best`, 0 empty-handed), `distance` away, with the nearest enemy
/// `enemy` units off if any. Empty-handed it is [`ARM`]; armed, only a
/// strictly better item scores, by its share of extra worth, less the walk
/// and, with an enemy close, the risk of turning from it.
pub(super) fn upgrade_score(worth: f32, best: f32, distance: f32, enemy: Option<f32>) -> f32 {
    if worth <= 0.0 || !worth.is_finite() {
        return 0.0;
    }
    let travel = (distance / REACH).clamp(0.0, 1.0);
    if best <= 0.0 {
        return ARM * (1.0 - 0.1 * travel);
    }
    if worth <= best {
        return 0.0;
    }
    let gain = 1.0 - best / worth;
    let close = enemy.map_or(0.0, |d| ((FAR - d) / (FAR - CLOSE)).clamp(0.0, 1.0));
    let risk = close * (0.5 + 0.5 * travel);
    UPGRADE * gain.sqrt() * (1.0 - TRAVEL * travel) * (1.0 - risk)
}

/// What the best weapon it holds is worth (0 empty-handed), and whether a
/// slot is free.
fn holding(session: &Session, bot: OwnerId, scale: f32) -> (f32, bool) {
    let Some(actor) = session.weapons.actor(ActorId(bot)) else {
        return (0.0, false);
    };
    let best = actor
        .inventory
        .iter()
        .flatten()
        .map(|item| hand_combat::item_worth(session, item, scale))
        .fold(0.0, f32::max);
    (best, actor.inventory.iter().any(Option::is_none))
}

/// The item in sight most worth going for, where it lies and its score.
/// One an ally went for first counts for less, so it is left to them while
/// another is in sight (`team` overlap).
fn best_item(
    session: &Session,
    bot: OwnerId,
    feet: Vec3,
    enemy: Option<f32>,
    armed: bool,
    tick: u64,
) -> Option<(Source, Vec3, f32)> {
    let peer = session.peers.get(&bot)?;
    let eye = peer.player.eye();
    let scale = peer.player.state().scale;
    let (best, free) = holding(session, bot, scale);
    if !free || armed && best <= 0.0 {
        return None;
    }
    let best = if armed { best } else { 0.0 };
    let skipped = &session.bots.brains.get(&bot)?.arming.skipped;
    let passed = |source: Source| skipped.cooling(&source, tick);
    let statics = session
        .item_spawners
        .items
        .iter()
        .filter(|(_, item)| tick >= item.available_at)
        .map(|(id, item)| {
            (
                Source::Brick(*id),
                item.item.as_str(),
                Vec3::from(item.position),
            )
        });
    let drops = session
        .weapons
        .drops()
        .filter(|d| session.weapons.pickup_ready(ActorId(bot), d.id))
        .map(|d| (Source::Drop(d.id), d.item.as_str(), d.position));
    statics
        .chain(drops)
        .filter(|(source, _, at)| feet.distance(*at) <= REACH && !passed(*source))
        .filter(|(_, item, _)| {
            hand_combat::item_attacks(session, item, scale) && !session.pickup_scripted(item)
        })
        .filter(|(source, _, at)| {
            let subject = match *source {
                Source::Brick(id) => super::SightSubject::Brick(id),
                Source::Drop(id) => super::SightSubject::Drop(id),
            };
            let urgency = super::SightUrgency::Ordinary;
            (session.bot_sees(bot, Some(subject), eye, *at, REACH, urgency)).is_some()
        })
        .map(|(source, item, at)| {
            let worth = hand_combat::item_worth(session, item, scale);
            (
                source,
                at,
                upgrade_score(worth, best, feet.distance(at), enemy),
            )
        })
        .filter(|(_, _, score)| *score > 0.0)
        .map(|(source, at, score)| {
            let taken = session.team_place_crowd(bot, Behaviour::Arm, at) > 0.0;
            (source, at, if taken { score * TAKEN } else { score })
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
}

/// Where `bot` goes to arm itself or upgrade, if anywhere, and how much it
/// wants to (the Arm behaviour's score). `armed` is whether it has an
/// attack already; `enemy` how far the nearest enemy it knows of is. It
/// passes over an item it stood at without getting.
pub(super) fn arm_point(
    session: &mut Session,
    bot: OwnerId,
    feet: Vec3,
    enemy: Option<f32>,
    armed: bool,
    tick: u64,
) -> Option<(Vec3, f32)> {
    let going = session.bots.brains.get(&bot)?.arming.going;
    let found = best_item(session, bot, feet, enemy, armed, tick);
    let brain = session.bots.brains.get_mut(&bot)?;
    let arming = &mut brain.arming;
    arming.skipped.prune(tick, |_| true);
    let Some((source, at, score)) = found else {
        arming.going = None;
        return None;
    };
    let standing = match going {
        Some((s, standing)) if s == source => standing,
        _ => None,
    };
    let standing = if flat(at - feet).length() < 1.0 {
        Some(standing.unwrap_or(tick))
    } else {
        None
    };
    // It reached it and did not get it: the pickup is not its to make.
    if standing.is_some_and(|at| tick >= at + AT_ITEM) {
        arming.going = None;
        arming.skipped.give_up(source, tick + SKIP);
        return None;
    }
    arming.going = Some((source, standing));
    Some((at, score))
}

#[cfg(test)]
mod tests {
    use super::super::behaviour::{self, Behaviour, Situation};
    use super::*;

    /// What a bot in `s` picks after long holding `current`, by the hold
    /// rule.
    fn picks(current: Behaviour, s: &Situation) -> Behaviour {
        behaviour::choose(current, s, |_| 1.0)
    }

    #[test]
    fn a_better_weapon_in_reach_beats_keeping_the_current_one() {
        // No enemy close: it wanders or searches, and goes for the better
        // weapon over keeping on.
        for (worth, best, distance, current) in [
            (40.0, 10.0, 4.0, Behaviour::Search),
            (400.0, 10.0, 20.0, Behaviour::Search),
            (40.0, 20.0, 20.0, Behaviour::Wander),
            (25.0, 20.0, 2.0, Behaviour::Wander),
        ] {
            let s = Situation {
                arm: upgrade_score(worth, best, distance, None),
                remembers: current == Behaviour::Search,
                ..Default::default()
            };
            assert_eq!(
                picks(current, &s),
                Behaviour::Arm,
                "{worth} {best} {distance}"
            );
        }
    }

    #[test]
    fn an_equal_or_worse_weapon_is_never_worth_it() {
        for (worth, best) in [(10.0, 10.0), (5.0, 10.0), (0.0, 10.0)] {
            for distance in [0.0, 5.0, 24.0] {
                for enemy in [None, Some(40.0), Some(4.0)] {
                    assert_eq!(upgrade_score(worth, best, distance, enemy), 0.0);
                }
            }
        }
    }

    #[test]
    fn a_close_enemy_in_a_fight_outweighs_a_far_upgrade() {
        for worth in [20.0, 40.0, 400.0] {
            for enemy in [3.0, 6.0, 8.0] {
                let s = Situation {
                    arm: upgrade_score(worth, 10.0, 20.0, Some(enemy)),
                    enemy: Some((enemy, 0.0)),
                    far: 20.0,
                    ..Default::default()
                };
                assert_eq!(
                    picks(Behaviour::Fight, &s),
                    Behaviour::Fight,
                    "{worth} {enemy}"
                );
                assert_eq!(
                    picks(Behaviour::Chase, &s),
                    Behaviour::Fight,
                    "{worth} {enemy}"
                );
            }
        }
    }

    #[test]
    fn an_empty_hand_arms_first() {
        let s = Situation {
            arm: upgrade_score(5.0, 0.0, 20.0, Some(5.0)),
            enemy: Some((5.0, 0.0)),
            far: 20.0,
            ..Default::default()
        };
        assert_eq!(picks(Behaviour::Chase, &s), Behaviour::Arm);
    }
}
