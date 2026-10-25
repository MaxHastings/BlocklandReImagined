//! Footfalls and landings for bodies that bring their own (`sounds.footsteps`,
//! `sounds.land` on an archetype). Each client plays them for every body it
//! draws, from the poses it already has, so they cost nothing on the wire:
//! a step each stride of ground covered while grounded, and a landing when
//! a body that was falling fast touches down. v20's Blockhead has neither.
use bri_sim::archetype::{ArchetypeId, Archetypes, Sounds, StepEvent};
use bri_sim::player::PlayerState;
use glam::Vec3;
use std::collections::BTreeMap;

/// Slower than this on the ground is standing still: no steps.
const MIN_STEP_SPEED: f32 = 0.5;

type OwnerId = u64;

/// What a footfall needs to know about a body drawn this frame.
#[derive(Clone, Copy, Debug)]
pub struct Walk {
    pub owner: OwnerId,
    pub archetype: ArchetypeId,
    pub feet: [f32; 3],
    pub velocity: [f32; 3],
    pub grounded: bool,
    pub scale: f32,
}
impl From<&PlayerState> for Walk {
    fn from(p: &PlayerState) -> Self {
        Self {
            owner: p.owner,
            archetype: p.archetype,
            feet: p.feet,
            velocity: p.velocity,
            grounded: p.grounded,
            scale: p.scale,
        }
    }
}

/// The stretch of its base clip a body played this frame: `from` to `to` in
/// seconds, not wrapped, so a looping clip that came round runs past
/// `duration`. A clip played backward has `to` below `from`; one that just
/// started (`restart`) also covers its first instant.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipSpan {
    /// The clip's lower-case name.
    pub clip: String,
    pub from: f32,
    pub to: f32,
    pub duration: f32,
    pub looping: bool,
    pub restart: bool,
}
impl ClipSpan {
    /// Whether a moment `at` seconds into the clip went by in this span.
    fn passes(&self, at: f32) -> bool {
        let (lo, hi) = (self.from.min(self.to), self.from.max(self.to));
        let forward = self.to >= self.from;
        let hit = |t: f32| {
            if self.restart {
                t >= lo && t <= hi
            } else if forward {
                t > lo && t <= hi
            } else {
                t >= lo && t < hi
            }
        };
        if !(self.looping && self.duration > 0.0) {
            return hit(at);
        }
        // Every lap the span reaches into, a few at most.
        let first = (lo / self.duration).floor() as i64;
        let last = (hi / self.duration).floor() as i64;
        (first..=last.min(first + 3)).any(|lap| hit(at + lap as f32 * self.duration))
    }
}

#[derive(Default)]
struct Walker {
    /// Ground covered since the last footfall.
    covered: f32,
    /// The variant played last, so two steps in a row differ.
    last: usize,
    grounded: bool,
    /// The fastest it fell since it last stood.
    falling: f32,
}

#[derive(Default)]
pub struct Footsteps {
    walkers: BTreeMap<OwnerId, Walker>,
    /// Picks variants; any sequence that is not the same twice will do.
    turn: u32,
}

impl Footsteps {
    /// The footfalls the clip a body played this frame marks: for each moment
    /// of `span` that went by, one step sound where `foot` says (`feet` for a
    /// marker without a foot), on the surface `surface` names there. Bodies
    /// whose clips mark none make no sound here.
    pub fn on_clip(
        &mut self,
        owner: OwnerId,
        sounds: &Sounds,
        span: &ClipSpan,
        mut foot: impl FnMut(&StepEvent) -> [f32; 3],
        mut surface: impl FnMut([f32; 3]) -> String,
    ) -> Vec<(String, [f32; 3])> {
        let mut out = Vec::new();
        let Some(events) = sounds.steps.get(&span.clip) else {
            return out;
        };
        for event in events {
            let Some(at) = sounds.step_time(event) else {
                continue;
            };
            if !span.passes(at) {
                continue;
            }
            let position = foot(event);
            let steps = if sounds.surfaces.is_empty() {
                &sounds.footsteps[..]
            } else {
                sounds.footsteps_on(&surface(position))
            };
            if steps.is_empty() {
                continue;
            }
            let walker = self.walkers.entry(owner).or_default();
            self.turn = self.turn.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let mut pick = (self.turn >> 16) as usize % steps.len();
            if steps.len() > 1 && pick == walker.last {
                pick = (pick + 1) % steps.len();
            }
            walker.last = pick;
            out.push((steps[pick].clone(), position));
        }
        out
    }
    /// Advance `seconds` for the bodies drawn now (`players`, without those
    /// in a seat) and return the sounds to play and where. `surface` names
    /// the surface kind under a footfall (`bri_content::surface`).
    pub fn update(
        &mut self,
        seconds: f32,
        players: impl IntoIterator<Item = Walk>,
        archetypes: &Archetypes,
        mut surface: impl FnMut([f32; 3]) -> String,
    ) -> Vec<(String, [f32; 3])> {
        let mut out = Vec::new();
        let mut seen = Vec::new();
        if !(seconds.is_finite() && seconds > 0.0) {
            return out;
        }
        let seconds = seconds.min(0.25);
        for player in players {
            seen.push(player.owner);
            let archetype = archetypes.resolve(player.archetype);
            let sounds = &archetype.sounds;
            let walker = self.walkers.entry(player.owner).or_default();
            let velocity = Vec3::from(player.velocity);
            if !velocity.is_finite() {
                continue;
            }
            // A landing: on the ground now, falling fast a moment ago.
            if player.grounded && !walker.grounded {
                if walker.falling >= Sounds::LAND_SPEED {
                    if let Some(land) = sounds.land.as_ref() {
                        if !land.is_empty() {
                            out.push((land.clone(), player.feet));
                        }
                    } else if sounds.land_on_surface {
                        // AA's PlayLanded: the floor material's foot-contact sound.
                        let steps = if sounds.surfaces.is_empty() {
                            &sounds.footsteps[..]
                        } else {
                            sounds.footsteps_on(&surface(player.feet))
                        };
                        if !steps.is_empty() {
                            self.turn = self.turn.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                            let pick = (self.turn >> 16) as usize % steps.len();
                            walker.last = pick;
                            out.push((steps[pick].clone(), player.feet));
                        }
                    }
                }
                // Feet down: the next step is a whole stride away.
                walker.covered = 0.0;
            }
            walker.falling = if player.grounded {
                0.0
            } else {
                walker.falling.max(-velocity.y)
            };
            walker.grounded = player.grounded;
            // A body whose clips mark their footfalls steps on those instead
            // ([`Self::on_clip`]).
            if !sounds.steps.is_empty()
                || (sounds.footsteps.is_empty() && sounds.surfaces.is_empty())
                || !player.grounded
            {
                continue;
            }
            let speed = Vec3::new(velocity.x, 0.0, velocity.z).length();
            if speed < MIN_STEP_SPEED {
                continue;
            }
            walker.covered += speed * seconds;
            let stride = sounds.stride_for(archetype.movement.stand_height, player.scale.max(0.01));
            if walker.covered >= stride {
                walker.covered %= stride;
                let steps = if sounds.surfaces.is_empty() {
                    &sounds.footsteps[..]
                } else {
                    sounds.footsteps_on(&surface(player.feet))
                };
                let count = steps.len();
                if count == 0 {
                    continue;
                }
                self.turn = self.turn.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let mut pick = (self.turn >> 16) as usize % count;
                if count > 1 && pick == walker.last {
                    pick = (pick + 1) % count;
                }
                walker.last = pick;
                out.push((steps[pick].clone(), player.feet));
            }
        }
        self.walkers.retain(|owner, _| seen.contains(owner));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walker_body() -> (Archetypes, ArchetypeId) {
        let mut archetypes = Archetypes::default();
        let mut body = archetypes.resolve(ArchetypeId(0)).clone();
        body.id = "test:archetype/walker".into();
        body.sounds.footsteps = vec!["test:step_1".into(), "test:step_2".into()];
        body.sounds.stride = Some(3.0);
        body.sounds.land = Some("test:land".into());
        let id = archetypes.add(body).expect("valid body");
        (archetypes, id)
    }

    fn player(archetype: ArchetypeId, velocity: [f32; 3], grounded: bool) -> Walk {
        Walk {
            owner: 7,
            archetype,
            feet: [1.0, 2.0, 3.0],
            velocity,
            grounded,
            scale: 1.0,
        }
    }

    fn marked_body() -> (Archetypes, ArchetypeId) {
        let (mut archetypes, walker) = walker_body();
        let mut body = archetypes.resolve(walker).clone();
        body.id = "test:archetype/marked".into();
        body.sounds.fps = Some(30.0);
        body.sounds.steps.insert(
            "run".into(),
            vec![
                StepEvent {
                    seconds: None,
                    frame: Some(3),
                    foot: Some("foot_l".into()),
                },
                StepEvent {
                    seconds: Some(0.5),
                    frame: None,
                    foot: None,
                },
            ],
        );
        let id = archetypes.add(body).expect("valid body");
        (archetypes, id)
    }
    fn span(from: f32, to: f32) -> ClipSpan {
        ClipSpan {
            clip: "run".into(),
            from,
            to,
            duration: 0.8,
            looping: true,
            restart: false,
        }
    }
    fn heard(
        steps: &mut Footsteps,
        archetypes: &Archetypes,
        id: ArchetypeId,
        s: &ClipSpan,
    ) -> usize {
        let sounds = &archetypes.resolve(id).sounds;
        steps
            .on_clip(7, sounds, s, |_| [0.0; 3], |_| "default".into())
            .len()
    }

    #[test]
    fn a_clip_marks_its_footfalls_and_the_timer_stays_quiet() {
        let (archetypes, id) = marked_body();
        let mut steps = Footsteps::default();
        // Frame 3 is 0.1 s in, the second mark 0.5 s.
        assert_eq!(heard(&mut steps, &archetypes, id, &span(0.0, 0.05)), 0);
        assert_eq!(heard(&mut steps, &archetypes, id, &span(0.05, 0.15)), 1);
        assert_eq!(heard(&mut steps, &archetypes, id, &span(0.15, 0.55)), 1);
        // Round the loop: 0.75 to 0.95 holds the lap's 0.1 (0.9).
        assert_eq!(heard(&mut steps, &archetypes, id, &span(0.75, 0.95)), 1);
        // Backward through both marks.
        assert_eq!(heard(&mut steps, &archetypes, id, &span(0.6, 0.05)), 2);
        // A clip that marks none is silent, and a running body's timer is off.
        let mut other = span(0.0, 0.7);
        other.clip = "idle".into();
        assert_eq!(heard(&mut steps, &archetypes, id, &other), 0);
        let runner = player(id, [9.0, 0.0, 0.0], true);
        let mut timer = 0;
        for _ in 0..120 {
            timer += steps
                .update(1.0 / 60.0, [runner], &archetypes, |_| "default".into())
                .len();
        }
        assert_eq!(timer, 0);
    }

    #[test]
    fn a_clip_that_just_started_covers_its_first_instant() {
        let (archetypes, id) = marked_body();
        let mut steps = Footsteps::default();
        let mut start = span(0.0, 0.0);
        start.restart = true;
        let mut sounds = archetypes.resolve(id).sounds.clone();
        sounds.steps.get_mut("run").unwrap()[1].seconds = Some(0.0);
        let n = steps
            .on_clip(7, &sounds, &start, |_| [0.0; 3], |_| "default".into())
            .len();
        assert_eq!(n, 1);
    }

    #[test]
    fn a_running_body_steps_each_stride_alternating_its_sounds() {
        let (archetypes, id) = walker_body();
        let mut steps = Footsteps::default();
        let runner = player(id, [9.0, 0.0, 0.0], true);
        let mut heard = Vec::new();
        // Two seconds at 9 units a second: 18 units, six 3-unit strides.
        for _ in 0..120 {
            heard.extend(steps.update(1.0 / 60.0, [runner], &archetypes, |_| "default".into()));
        }
        assert!((5..=6).contains(&heard.len()), "{heard:?}");
        assert!(heard.windows(2).all(|w| w[0].0 != w[1].0), "{heard:?}");
    }

    #[test]
    fn a_landing_can_sound_like_a_footfall_on_the_surface_underfoot() {
        let (mut archetypes, walker) = walker_body();
        let mut body = archetypes.resolve(walker).clone();
        body.id = "test:archetype/lander".into();
        body.sounds.land = None;
        body.sounds.land_on_surface = true;
        body.sounds
            .surfaces
            .insert("metal".into(), vec!["test:metal_step".into()]);
        let lander = archetypes.add(body).expect("valid body");
        let mut steps = Footsteps::default();
        let falling = player(lander, [0.0, -9.0, 0.0], false);
        let landed = player(lander, [0.0, 0.0, 0.0], true);
        steps.update(1.0 / 60.0, [falling], &archetypes, |_| "metal".into());
        let heard = steps.update(1.0 / 60.0, [landed], &archetypes, |_| "metal".into());
        assert_eq!(heard.len(), 1, "{heard:?}");
        assert_eq!(heard[0].0, "test:metal_step");
        // A body with no landing option stays silent.
        let mut quiet = Footsteps::default();
        let plain = player(walker, [0.0, -9.0, 0.0], false);
        let down = player(walker, [0.0, 0.0, 0.0], true);
        quiet.update(1.0 / 60.0, [plain], &archetypes, |_| "metal".into());
        let heard = quiet.update(1.0 / 60.0, [down], &archetypes, |_| "metal".into());
        assert_eq!(heard.iter().filter(|h| h.0 == "test:metal_step").count(), 0);
    }

    #[test]
    fn the_surface_underfoot_picks_its_own_steps() {
        let (mut archetypes, walker) = walker_body();
        let mut body = archetypes.resolve(walker).clone();
        body.id = "test:archetype/soldier".into();
        body.sounds.surfaces = [("metal".to_string(), vec!["test:clang".to_string()])].into();
        let id = archetypes.add(body).expect("valid body");
        let mut steps = Footsteps::default();
        let runner = player(id, [9.0, 0.0, 0.0], true);
        let mut heard = Vec::new();
        for i in 0..120 {
            let kind = if i < 60 { "metal" } else { "dirt" };
            heard.extend(steps.update(1.0 / 60.0, [runner], &archetypes, |_| kind.into()));
        }
        let names: Vec<_> = heard.iter().map(|h| h.0.as_str()).collect();
        assert!(names.contains(&"test:clang"), "{names:?}");
        assert!(
            names.iter().any(|n| n.starts_with("test:step")),
            "{names:?}"
        );
    }

    #[test]
    fn standing_or_falling_makes_no_steps_and_a_hard_landing_is_heard_once() {
        let (archetypes, id) = walker_body();
        let mut steps = Footsteps::default();
        let still = player(id, [0.1, 0.0, 0.0], true);
        assert!(
            steps
                .update(1.0, [still], &archetypes, |_| "default".into())
                .is_empty()
        );
        let falling = player(id, [9.0, -12.0, 0.0], false);
        for _ in 0..30 {
            assert!(
                steps
                    .update(1.0 / 60.0, [falling], &archetypes, |_| "default".into())
                    .is_empty()
            );
        }
        let landed = player(id, [0.0, 0.0, 0.0], true);
        let heard = steps.update(1.0 / 60.0, [landed], &archetypes, |_| "default".into());
        assert_eq!(heard, vec![("test:land".to_string(), landed.feet)]);
        assert!(
            steps
                .update(1.0 / 60.0, [landed], &archetypes, |_| "default".into())
                .is_empty()
        );
    }

    #[test]
    fn the_blockhead_makes_no_sound_walking() {
        let archetypes = Archetypes::default();
        let mut steps = Footsteps::default();
        let blockhead = player(ArchetypeId(0), [7.0, 0.0, 0.0], true);
        for _ in 0..120 {
            assert!(
                steps
                    .update(1.0 / 60.0, [blockhead], &archetypes, |_| "default".into())
                    .is_empty()
            );
        }
    }
}
