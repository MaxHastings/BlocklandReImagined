//! What bots tell the rules about their surroundings: spotting an enemy,
//! hearing gunfire, being shot at, seeing a grenade or an ally die, losing
//! an enemy, running dry, observed objective outcomes and failed headway.
//! The brain decides nothing from them; an Add-On's
//! hooks (`on_bot_spotted` and the others of `content::Behaviour`) say what a
//! bot does about it, such as calling it out. Each is told once per
//! occurrence, to every package that declares it, after the bots have
//! stepped.
use super::*;
use std::collections::BTreeSet;

/// One thing a bot noticed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::session) enum Report {
    Spotted(OwnerId, OwnerId),
    HeardGunfire(OwnerId, OwnerId),
    UnderFire(OwnerId, OwnerId),
    SeesGrenade(OwnerId, u64),
    FriendDied(OwnerId, OwnerId),
    LostEnemy(OwnerId, OwnerId),
    OutOfAmmo(OwnerId),
    ObjectiveAcquired(OwnerId),
    ObjectiveCompleted(OwnerId),
    Stalled(OwnerId),
}

impl Report {
    /// The bot it is about.
    pub(in crate::session) fn bot(self) -> OwnerId {
        match self {
            Self::Spotted(b, _)
            | Self::HeardGunfire(b, _)
            | Self::UnderFire(b, _)
            | Self::SeesGrenade(b, _)
            | Self::FriendDied(b, _)
            | Self::LostEnemy(b, _)
            | Self::OutOfAmmo(b)
            | Self::ObjectiveAcquired(b)
            | Self::ObjectiveCompleted(b)
            | Self::Stalled(b) => b,
        }
    }
    /// The hook that hears it, and its arguments after the bot.
    pub(in crate::session) fn hook(self) -> (&'static str, Option<u64>) {
        match self {
            Self::Spotted(_, e) => ("on_bot_spotted", Some(e)),
            Self::HeardGunfire(_, e) => ("on_bot_heard_gunfire", Some(e)),
            Self::UnderFire(_, e) => ("on_bot_under_fire", Some(e)),
            Self::SeesGrenade(_, g) => ("on_bot_sees_grenade", Some(g)),
            Self::FriendDied(_, f) => ("on_bot_friend_died", Some(f)),
            Self::LostEnemy(_, e) => ("on_bot_lost_enemy", Some(e)),
            Self::OutOfAmmo(_) => ("on_bot_out_of_ammo", None),
            Self::ObjectiveAcquired(_) => ("on_bot_objective_acquired", None),
            Self::ObjectiveCompleted(_) => ("on_bot_objective_completed", None),
            Self::Stalled(_) => ("on_bot_stalled", None),
        }
    }
}

/// What the bots told this tick, and what they have already told.
#[derive(Default)]
pub(super) struct Reports {
    /// Told next, in order.
    pub(super) queue: Vec<Report>,
    /// Allies that died this tick, and where.
    pub(super) deaths: Vec<(OwnerId, Vec3)>,
    /// Hitscan rays that ended this tick: shooter, muzzle, end.
    pub(super) shots: Vec<(OwnerId, Vec3, Vec3)>,
    /// Projectiles each bot has seen.
    seen_grenades: BTreeMap<OwnerId, BTreeSet<u64>>,
}

impl Reports {
    pub(super) fn tell(&mut self, report: Report) {
        if self.queue.len() < MAX_REPORTS {
            self.queue.push(report);
        }
    }
}

/// Reports kept in a tick; the rest are dropped.
const MAX_REPORTS: usize = 512;

/// The distance from `p` to the segment `a` to `b`.
fn distance_to_segment(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let length = ab.length_squared();
    if length <= f32::EPSILON {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / length).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

impl Bots {
    /// An ally died where bots may see it.
    pub(in crate::session) fn note_death(&mut self, victim: OwnerId, at: Vec3) {
        if !self.brains.is_empty() && self.reports.deaths.len() < MAX_REPORTS {
            self.reports.deaths.push((victim, at));
        }
    }
    /// A hitscan ray from `from` ended at `to`.
    pub(in crate::session) fn note_shot(&mut self, shooter: OwnerId, from: Vec3, to: Vec3) {
        if !self.brains.is_empty() && self.reports.shots.len() < MAX_REPORTS {
            self.reports.shots.push((shooter, from, to));
        }
    }
}

impl Session {
    /// What the bots perceive beyond their targets: grenades in view, allies
    /// dying in view, shots passing close. Then tell the rules.
    pub(super) fn tell_reports(&mut self, tick: u64) {
        self.perceive_grenades();
        self.perceive_deaths();
        self.perceive_shots(tick);
        for report in std::mem::take(&mut self.bots.reports.queue) {
            if !self.bot_gone(report.bot()) {
                self.package_bot_report(report);
            }
        }
    }
    /// The bots that are awake and could see: owner, eye, kind sight.
    fn watchers(&self) -> Vec<(OwnerId, Vec3, f32)> {
        self.bots
            .brains
            .iter()
            .filter(|(_, b)| !b.resting)
            .filter_map(|(bot, b)| {
                let p = self.peers.get(bot).filter(|p| p.combat.alive)?;
                Some((*bot, p.player.eye(), self.bot_sight_reach(&b.kind)))
            })
            .collect()
    }
    /// Each live projectile that explodes, once for each bot that sees it.
    fn perceive_grenades(&mut self) {
        let live: Vec<(u64, Vec3, OwnerId)> = self
            .weapons
            .projectiles()
            .filter(|p| {
                self.weapons
                    .pack
                    .projectiles
                    .get(&p.definition)
                    .is_some_and(|d| d.explosion.damage > 0.0)
            })
            .map(|p| (p.id, p.position, p.source.0))
            .collect();
        let alive: BTreeSet<u64> = live.iter().map(|g| g.0).collect();
        for seen in self.bots.reports.seen_grenades.values_mut() {
            seen.retain(|id| alive.contains(id));
        }
        if live.is_empty() {
            return;
        }
        for (bot, eye, reach) in self.watchers() {
            for &(id, at, source) in &live {
                if source == bot
                    || eye.distance(at) > reach
                    || self
                        .bots
                        .reports
                        .seen_grenades
                        .get(&bot)
                        .is_some_and(|s| s.contains(&id))
                    || self
                        .bot_sees(bot, None, eye, at, reach, SightUrgency::Ordinary)
                        .is_none()
                {
                    continue;
                }
                self.bots
                    .reports
                    .seen_grenades
                    .entry(bot)
                    .or_default()
                    .insert(id);
                self.bots.reports.tell(Report::SeesGrenade(bot, id));
            }
        }
    }
    /// Allies dying in view of a bot.
    fn perceive_deaths(&mut self) {
        let deaths = std::mem::take(&mut self.bots.reports.deaths);
        if deaths.is_empty() {
            return;
        }
        for (bot, eye, reach) in self.watchers() {
            for &(victim, at) in &deaths {
                if victim == bot || !self.bot_allies(bot, victim) {
                    continue;
                }
                let chest = at + Vec3::Y * 1.0;
                if eye.distance(chest) <= reach
                    && self
                        .bot_sees(bot, None, eye, chest, reach, SightUrgency::Ordinary)
                        .is_some()
                {
                    self.bots.reports.tell(Report::FriendDied(bot, victim));
                }
            }
        }
    }
    /// Shots of an enemy that passed a bot without hitting it.
    fn perceive_shots(&mut self, tick: u64) {
        let shots = std::mem::take(&mut self.bots.reports.shots);
        if shots.is_empty() {
            return;
        }
        for (bot, brain) in &self.bots.brains {
            let range = brain.kind.under_fire_range;
            let Some(p) = self
                .peers
                .get(bot)
                .filter(|p| p.combat.alive && range > 0.0)
            else {
                continue;
            };
            let state = p.player.state();
            let body = (Vec3::from(state.feet) + p.player.eye()) * 0.5;
            let hit_now = self
                .bots
                .hurt
                .get(bot)
                .is_some_and(|k| k.observed + 1 >= tick);
            for &(shooter, from, to) in &shots {
                if shooter == *bot
                    || !self.bot_enemy(*bot, &brain.kind, shooter)
                    || (hit_now
                        && self
                            .bots
                            .hurt
                            .get(bot)
                            .is_some_and(|k| k.subject == shooter))
                    || distance_to_segment(body, from, to) > range
                {
                    continue;
                }
                self.bots.reports.tell(Report::UnderFire(*bot, shooter));
            }
        }
    }
}
