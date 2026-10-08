//! Clunk: what a viewer reads as a bot's systems overriding each other,
//! from the controls it pressed each tick (`BotThought::input`) and what
//! its act stage says moved it (`BotThought::acted`). Read only.
//!
//! - a twitch: the walk turns round (over 135 degrees) after holding its
//!   way for less than 0.4 s;
//! - a stutter: it stops for less than 0.25 s between two walks;
//! - a head snap: the look swings one way faster than 90 degrees a second
//!   and back within 0.3 s;
//! - a hand-over: the walk or the look passes to another owner, counted
//!   by pair (`route>stance`), and how long the owner it took over from
//!   had held it.
use glam::Vec3;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

const TPS: f32 = 120.0;

#[derive(Default)]
pub struct Clunk {
    /// The way it last walked and since when.
    way: Option<(Vec3, u64)>,
    /// When it stopped, after walking.
    stopped: Option<u64>,
    /// The last look swing: its sign and when it began.
    swing: Option<(f32, u64)>,
    last_yaw: Option<f32>,
    walker: Option<(String, u64)>,
    looker: Option<(String, u64)>,
    jump_was: bool,
    crouch_was: bool,
    /// Recent twitches, stutters and snaps, for a jitter moment.
    recent: VecDeque<(u64, &'static str, String)>,
    reported: u64,
    pub ticks: u64,
    pub twitches: u64,
    pub stutters: u64,
    pub snaps: u64,
    pub jumps: u64,
    pub crouches: u64,
}

/// Everything summed over every bot, with the pairs.
#[derive(Default)]
pub struct Totals {
    pub ticks: u64,
    pub twitches: u64,
    pub stutters: u64,
    pub snaps: u64,
    pub jumps: u64,
    pub crouches: u64,
    pub walk_pairs: BTreeMap<String, (u64, u64)>,
    pub look_pairs: BTreeMap<String, (u64, u64)>,
    pub twitch_by: BTreeMap<String, u64>,
    pub stutter_by: BTreeMap<String, u64>,
    pub snap_by: BTreeMap<String, u64>,
}

fn part<'a>(acted: &'a str, key: &str) -> &'a str {
    acted
        .split([',', ';'])
        .map(str::trim)
        .find_map(|p| p.strip_prefix(key))
        .unwrap_or("?")
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

impl Clunk {
    /// One tick of one living, unmounted bot. Returns a jitter moment's
    /// text when four or more twitches, stutters or snaps fell in the last
    /// two seconds.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        tick: u64,
        input: &bri_sim::player::MoveInput,
        acted: &str,
        behaviour: &str,
        totals: &mut Totals,
    ) -> Option<String> {
        self.ticks += 1;
        totals.ticks += 1;
        let acted = acted.strip_prefix("Acts: ").unwrap_or(acted);
        let walker = part(acted, "walk ").to_string();
        let looker = part(acted, "look ").to_string();
        let tag = |w: &str| format!("{behaviour}/{w}");
        let (s, c) = input.yaw.sin_cos();
        let fwd = Vec3::new(s, 0.0, -c);
        let right = Vec3::new(c, 0.0, s);
        let walk = fwd * input.forward + right * input.right;
        let mut events: Vec<(&'static str, String)> = vec![];
        if walk.length() > 0.2 {
            let dir = walk.normalize();
            if let Some(t) = self.stopped.take()
                && tick - t < (0.25 * TPS) as u64
            {
                self.stutters += 1;
                totals.stutters += 1;
                *totals.stutter_by.entry(tag(&walker)).or_default() += 1;
                events.push(("stutter", walker.clone()));
            }
            match self.way {
                Some((way, since)) if way.dot(dir) < -0.7 => {
                    if tick - since < (0.4 * TPS) as u64 {
                        self.twitches += 1;
                        totals.twitches += 1;
                        *totals.twitch_by.entry(tag(&walker)).or_default() += 1;
                        events.push(("twitch", walker.clone()));
                    }
                    self.way = Some((dir, tick));
                }
                Some((way, _)) if way.dot(dir) > 0.7 => {}
                _ => self.way = Some((dir, tick)),
            }
        } else if self.way.is_some() && self.stopped.is_none() {
            self.stopped = Some(tick);
        }
        if let Some(last) = self.last_yaw {
            let rate = wrap(input.yaw - last) * TPS;
            if rate.abs() > 90f32.to_radians() {
                let sign = rate.signum();
                match self.swing {
                    Some((was, since)) if was != sign => {
                        if tick - since < (0.3 * TPS) as u64 {
                            self.snaps += 1;
                            totals.snaps += 1;
                            *totals.snap_by.entry(tag(&looker)).or_default() += 1;
                            events.push(("snap", looker.clone()));
                        }
                        self.swing = Some((sign, tick));
                    }
                    Some(_) => {}
                    None => self.swing = Some((sign, tick)),
                }
            }
        }
        self.last_yaw = Some(input.yaw);
        for (owner, now, pairs) in [
            (&mut self.walker, &walker, &mut totals.walk_pairs),
            (&mut self.looker, &looker, &mut totals.look_pairs),
        ] {
            match owner {
                Some((was, since)) if was != now => {
                    let e = pairs.entry(format!("{was}>{now}")).or_default();
                    e.0 += 1;
                    e.1 += tick - *since;
                    *owner = Some((now.clone(), tick));
                }
                Some(_) => {}
                None => *owner = Some((now.clone(), tick)),
            }
        }
        if input.jump && !self.jump_was {
            self.jumps += 1;
            totals.jumps += 1;
        }
        if input.crouch != self.crouch_was {
            self.crouches += 1;
            totals.crouches += 1;
        }
        self.jump_was = input.jump;
        self.crouch_was = input.crouch;
        for (k, w) in events {
            self.recent
                .push_back((tick, k, format!("{behaviour} {k} by {w}")));
        }
        while self
            .recent
            .front()
            .is_some_and(|(t, _, _)| tick - t > 2 * TPS as u64)
        {
            self.recent.pop_front();
        }
        if self.recent.len() >= 4 && tick > self.reported + 5 * TPS as u64 {
            self.reported = tick;
            let what: Vec<_> = self.recent.iter().map(|(_, _, w)| w.as_str()).collect();
            return Some(format!("{}; {acted}", what.join(", ")));
        }
        None
    }

    pub fn reset(&mut self) {
        let keep = (
            self.ticks,
            self.twitches,
            self.stutters,
            self.snaps,
            self.jumps,
            self.crouches,
            self.reported,
        );
        *self = Self::default();
        (
            self.ticks,
            self.twitches,
            self.stutters,
            self.snaps,
            self.jumps,
            self.crouches,
            self.reported,
        ) = keep;
    }

    pub fn per_minute(&self) -> Value {
        let m = (self.ticks as f32 / TPS / 60.0).max(1e-3);
        json!({
            "twitches": self.twitches as f32 / m,
            "stutters": self.stutters as f32 / m,
            "snaps": self.snaps as f32 / m,
            "jumps": self.jumps as f32 / m,
            "crouch_toggles": self.crouches as f32 / m,
        })
    }
}

impl Totals {
    pub fn json(&self) -> Value {
        let m = (self.ticks as f32 / TPS / 60.0).max(1e-3);
        let pairs = |p: &BTreeMap<String, (u64, u64)>| {
            let mut v: Vec<_> = p.iter().collect();
            v.sort_by(|a, b| b.1.0.cmp(&a.1.0));
            v.into_iter()
                .take(20)
                .map(|(k, (n, held))| {
                    json!({"pair": k, "per_bot_minute": *n as f32 / m,
                        "mean_held_s": *held as f32 / *n as f32 / TPS})
                })
                .collect::<Vec<_>>()
        };
        let by = |p: &BTreeMap<String, u64>| {
            let mut v: Vec<_> = p.iter().collect();
            v.sort_by(|a, b| b.1.cmp(a.1));
            v.into_iter()
                .take(12)
                .map(|(k, n)| json!([k, *n as f32 / m]))
                .collect::<Vec<_>>()
        };
        json!({
            "bot_minutes": m,
            "per_bot_minute": {
                "twitches": self.twitches as f32 / m,
                "stutters": self.stutters as f32 / m,
                "snaps": self.snaps as f32 / m,
                "jumps": self.jumps as f32 / m,
                "crouch_toggles": self.crouches as f32 / m,
                "walk_handovers": self.walk_pairs.values().map(|v| v.0).sum::<u64>() as f32 / m,
                "look_handovers": self.look_pairs.values().map(|v| v.0).sum::<u64>() as f32 / m,
            },
            "twitch_by": by(&self.twitch_by),
            "stutter_by": by(&self.stutter_by),
            "snap_by": by(&self.snap_by),
            "walk_pairs": pairs(&self.walk_pairs),
            "look_pairs": pairs(&self.look_pairs),
        })
    }
}
