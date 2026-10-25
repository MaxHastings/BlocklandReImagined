//! What a bot's rules want of it (`bot_intent`): keep to a spot, walk a
//! round, stay by a leader, or run. The rules say what; the shared brain
//! carries it out with everything it already does (routes, jumps, seats,
//! fights), so an Add-On never ships a brain loop of its own
//! (`docs/architecture/bots.md`, Intents).
//!
//! An intent moves the bot's anchor, the place its stroll, its walk home
//! and its chase leash are measured from: a guard's spot, the patrol point
//! it is heading for, the leader's feet. Fleeing is a behaviour of the
//! brain (`Behaviour::Flee`) that the kind's `flee_below` or a `flee`
//! intent switches on.
use super::*;
use bri_package_runtime::ops::BotIntent;

/// How near a patrol point counts as reached.
const PATROL_REACHED: f32 = 1.5;
/// How far a fleeing bot heads off each time it picks where to.
const FLEE_LEG: f32 = 12.0;
/// How long a flee leg's point is kept before it picks again.
const FLEE_LEG_TICKS: u64 = 2 * 120;

/// A bot's intent and how far along it is.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Held {
    pub intent: BotIntent,
    /// The patrol point it heads for.
    pub next: usize,
    /// A flee intent ends at this tick.
    pub until: u64,
}
impl Held {
    /// Whether it anchors the bot somewhere (all but a flee).
    pub fn anchors(&self) -> bool {
        !matches!(self.intent, BotIntent::Flee { .. })
    }
}

impl Brain {
    /// How far from its anchor it strolls.
    pub(super) fn stroll_radius(&self) -> f32 {
        match self.intent.as_ref().map(|h| &h.intent) {
            // A guard keeps close to its post.
            Some(BotIntent::Guard { radius, .. }) => (radius * 0.5).min(self.kind.wander_radius),
            Some(BotIntent::Follow { distance, .. }) => *distance,
            _ => self.kind.wander_radius,
        }
    }
    /// How far from its anchor it follows a fight on foot.
    pub(super) fn chase_radius(&self) -> f32 {
        match self.intent.as_ref().map(|h| &h.intent) {
            Some(BotIntent::Guard { radius, .. } | BotIntent::Patrol { radius, .. }) => *radius,
            _ => self.kind.chase_radius,
        }
    }
    /// The patrol point it walks to next while strolling, if it patrols.
    pub(super) fn patrol_point(&mut self, feet: Vec3) -> Option<Vec3> {
        let held = self.intent.as_mut()?;
        let BotIntent::Patrol { points, .. } = &held.intent else {
            return None;
        };
        let mut at = Vec3::from(points[held.next % points.len()]);
        if at.distance(feet) <= PATROL_REACHED {
            held.next = (held.next + 1) % points.len();
            at = Vec3::from(points[held.next]);
        }
        Some(at)
    }
}

impl Brain {
    /// Where a bot running from `from` heads: a leg straight away from it,
    /// kept a while, picked again once there or once the leg is old.
    pub(super) fn flee(&mut self, feet: Vec3, from: Vec3, tick: u64) {
        let brain = self;
        let away = flat(feet - from).try_normalize().unwrap_or_else(|| {
            let angle = brain.random() * std::f32::consts::TAU;
            Vec3::new(angle.sin(), 0.0, angle.cos())
        });
        let keep = match brain.goal {
            Some(Goal::Flee(at)) => {
                tick < brain.next_wander
                    && flat(at - feet).length() > PATROL_REACHED
                    && !brain.settled
                    // Still away from what it runs from.
                    && flat(at - from).length() > flat(feet - from).length()
            }
            _ => false,
        };
        if !keep {
            brain.set_goal(Some(Goal::Flee(feet + away * FLEE_LEG)));
            brain.next_wander = tick + FLEE_LEG_TICKS;
        }
    }
}

impl Session {
    /// Give `bot` an intent from `package`'s rules, or none.
    pub(in crate::session) fn set_bot_intent(
        &mut self,
        package: &str,
        bot: OwnerId,
        intent: Option<BotIntent>,
    ) -> Result<()> {
        self.check_rest_rules_bot(package, bot)?;
        if let Some(BotIntent::Follow { leader, .. }) = &intent {
            ensure!(
                *leader != bot && self.peers.contains_key(leader),
                "No player {leader} to follow"
            );
        }
        let tick = self.simulation.state().tick;
        let brain = self.bots.brains.get_mut(&bot).context("No such bot")?;
        brain.intent = intent.map(|intent| Held {
            until: match &intent {
                BotIntent::Flee { seconds, .. } => tick + ticks(*seconds),
                _ => u64::MAX,
            },
            intent,
            next: 0,
        });
        // Its stroll starts over from the new anchor.
        if matches!(brain.goal, Some(Goal::Wander(_))) {
            brain.set_goal(None);
        }
        brain.next_wander = tick;
        if brain.intent.is_none() && brain.brick.is_none() {
            brain.rehome = true;
        }
        Ok(())
    }

    /// Moves `bot`'s anchor where its intent has it this tick, and lets a
    /// flee that has run its time, or a follow whose leader left, go.
    pub(super) fn bot_follow_intent(&mut self, bot: OwnerId, tick: u64) {
        let leader_feet = match self.bots.brains[&bot].intent.as_ref().map(|h| &h.intent) {
            Some(BotIntent::Follow { leader, .. }) => Some(
                self.peers
                    .get(leader)
                    .filter(|p| p.combat.alive)
                    .map(|p| Vec3::from(p.player.state().feet)),
            ),
            _ => None,
        };
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let Some(held) = brain.intent.as_ref() else {
            return;
        };
        let anchor = match &held.intent {
            BotIntent::Guard { at, .. } => Some(Vec3::from(*at)),
            BotIntent::Patrol { points, .. } => Some(Vec3::from(points[held.next % points.len()])),
            // A leader gone for good ends it; one dead a moment keeps the
            // last place.
            BotIntent::Follow { leader, .. } => match leader_feet.flatten() {
                Some(feet) => Some(feet),
                None if self.peers.contains_key(leader) => None,
                None => {
                    brain.intent = None;
                    brain.rehome = brain.brick.is_none();
                    return;
                }
            },
            BotIntent::Flee { .. } => {
                if tick >= held.until {
                    brain.intent = None;
                }
                None
            }
        };
        if let Some(at) = anchor {
            brain.home = at;
            brain.leash = at;
        }
    }

    /// Whether `bot` wants to run now (`Behaviour::Flee`): hurt to its
    /// kind's `flee_below` or less, or a flee intent, and the place it runs
    /// from: the one the intent names, else the enemy it sees or knows of.
    pub(super) fn bot_flee_from(
        &self,
        bot: OwnerId,
        seen: Option<Vec3>,
        known: Option<Knowledge>,
    ) -> Option<Vec3> {
        let brain = &self.bots.brains[&bot];
        let peer = self.peers.get(&bot)?;
        let intent = match brain.intent.as_ref().map(|h| &h.intent) {
            Some(BotIntent::Flee { from, .. }) => Some(*from),
            _ => None,
        };
        let hurt = brain.kind.flee_below > 0.0
            && peer.combat.health <= self.max_health(bot) * brain.kind.flee_below;
        if !hurt && intent.is_none() {
            return None;
        }
        let named = intent.flatten().and_then(|from| {
            self.peers
                .get(&from)
                .filter(|p| p.combat.alive)
                .map(|p| Vec3::from(p.player.state().feet))
        });
        named.or(seen).or(known.map(|k| k.at))
    }
}
