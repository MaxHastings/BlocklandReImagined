//! Footfalls and landings for bodies that bring their own (`sounds.footsteps`,
//! `sounds.land` on an archetype). Each client plays them for every body it
//! draws, from the poses it already has, so they cost nothing on the wire:
//! a step each stride of ground covered while grounded, and a landing when
//! a body that was falling fast touches down. v20's Blockhead has neither.
use bri_sim::archetype::{ArchetypeId, Archetypes, FootContact, Sounds, StepEvent};
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
    /// The posture it holds: `stand`, `crouch` or `prone`.
    pub posture: &'static str,
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
            posture: if p.posture.prone {
                "prone"
            } else if p.crouched {
                "crouch"
            } else {
                "stand"
            },
        }
    }
}

pub use bri_sim::gait::ClipSpan;

/// One footfall to play: its sound, where, and what it leaves on the
/// surface it landed on ([`Sounds::contacts`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Footfall {
    pub sound: String,
    pub at: [f32; 3],
    pub contact: Option<FootContact>,
    /// Times the sound's own volume, and the distance it is heard out to (0
    /// keeps its own).
    pub volume: f32,
    pub radius: f32,
}
impl Footfall {
    /// A sound at `at` as authored, leaving nothing.
    fn plain(sound: &str, at: [f32; 3]) -> Self {
        Self {
            sound: sound.to_owned(),
            at,
            contact: None,
            volume: 1.0,
            radius: 0.0,
        }
    }
    fn on(sounds: &Sounds, sound: &str, at: [f32; 3], surface: &str) -> Self {
        Self {
            sound: sound.to_owned(),
            at,
            contact: sounds.contact_on(surface).cloned(),
            volume: 1.0,
            radius: 0.0,
        }
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
    /// Whether the last frame's jump take-off was already heard.
    airborne: bool,
    /// The posture it held last frame, once seen.
    posture: Option<&'static str>,
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
    ) -> Vec<Footfall> {
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
            // Another sound source in place of the surface's footfall.
            if let Some(sound) = &event.sound {
                out.push(Footfall {
                    volume: event.volume.unwrap_or(1.0),
                    radius: event.radius.unwrap_or(0.0),
                    ..Footfall::plain(sound, position)
                });
                continue;
            }
            let kind = surface(position);
            let walker = self.walkers.entry(owner).or_default();
            let steps = sounds.footsteps_for(walker.posture.unwrap_or("stand"), &kind);
            if steps.is_empty() {
                continue;
            }
            self.turn = self.turn.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let mut pick = (self.turn >> 16) as usize % steps.len();
            if steps.len() > 1 && pick == walker.last {
                pick = (pick + 1) % steps.len();
            }
            walker.last = pick;
            // The mark's own loudness and reach, as a sound source of its own has.
            out.push(Footfall {
                volume: event.volume.unwrap_or(1.0),
                radius: event.radius.unwrap_or(0.0),
                ..Footfall::on(sounds, &steps[pick], position, &kind)
            });
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
    ) -> Vec<Footfall> {
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
            // A change of posture sounds where the body is.
            if walker.posture.is_some_and(|last| last != player.posture)
                && let Some(sound) = sounds.postures.get(player.posture)
            {
                out.push(Footfall::plain(sound, player.feet));
            }
            walker.posture = Some(player.posture);
            // A landing: on the ground now, falling fast a moment ago.
            if player.grounded && !walker.grounded {
                if walker.falling >= Sounds::LAND_SPEED {
                    if let Some(land) = sounds.land.as_ref() {
                        if !land.is_empty() {
                            out.push(Footfall::plain(land, player.feet));
                        }
                    } else if sounds.land_on_surface {
                        // AA's PlayLanded: the floor material's foot-contact sound.
                        let kind = surface(player.feet);
                        let steps = sounds.footsteps_for(player.posture, &kind);
                        if !steps.is_empty() {
                            self.turn = self.turn.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                            let pick = (self.turn >> 16) as usize % steps.len();
                            walker.last = pick;
                            out.push(Footfall::on(sounds, &steps[pick], player.feet, &kind));
                        }
                    }
                }
                // Feet down: the next step is a whole stride away.
                walker.covered = 0.0;
                walker.airborne = false;
            }
            // A jump's take-off: off the ground rising, heard once.
            if !player.grounded && walker.grounded && !walker.airborne {
                walker.airborne = true;
                if sounds.takeoff_on_surface && velocity.y > 0.0 {
                    let kind = surface(player.feet);
                    let steps = sounds.footsteps_for(player.posture, &kind);
                    if !steps.is_empty() {
                        self.turn = self.turn.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                        let pick = (self.turn >> 16) as usize % steps.len();
                        walker.last = pick;
                        out.push(Footfall::on(sounds, &steps[pick], player.feet, &kind));
                    }
                }
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
                || (sounds.footsteps.is_empty()
                    && sounds.surfaces.is_empty()
                    && sounds.posture_steps.is_empty())
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
                let kind = surface(player.feet);
                let steps = sounds.footsteps_for(player.posture, &kind);
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
                out.push(Footfall::on(sounds, &steps[pick], player.feet, &kind));
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
            posture: "stand",
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
                    ..Default::default()
                },
                StepEvent {
                    seconds: Some(0.5),
                    frame: None,
                    foot: None,
                    ..Default::default()
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
        assert!(
            heard.windows(2).all(|w| w[0].sound != w[1].sound),
            "{heard:?}"
        );
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
        assert_eq!(heard[0].sound, "test:metal_step");
        // A body with no landing option stays silent.
        let mut quiet = Footsteps::default();
        let plain = player(walker, [0.0, -9.0, 0.0], false);
        let down = player(walker, [0.0, 0.0, 0.0], true);
        quiet.update(1.0 / 60.0, [plain], &archetypes, |_| "metal".into());
        let heard = quiet.update(1.0 / 60.0, [down], &archetypes, |_| "metal".into());
        assert_eq!(
            heard
                .iter()
                .filter(|h| h.sound == "test:metal_step")
                .count(),
            0
        );
    }

    #[test]
    fn a_jump_sounds_its_take_off_once_and_a_footfall_leaves_its_marks() {
        let (mut archetypes, walker) = walker_body();
        let mut body = archetypes.resolve(walker).clone();
        body.id = "test:archetype/jumper".into();
        body.sounds.land = None;
        body.sounds.land_on_surface = true;
        body.sounds.takeoff_on_surface = true;
        body.sounds
            .surfaces
            .insert("sand".into(), vec!["test:sand_step".into()]);
        body.sounds.contacts.insert(
            "sand".into(),
            FootContact {
                effect: Some("test:dust".into()),
                decal: None,
            },
        );
        let jumper = archetypes.add(body).expect("valid body");
        let mut steps = Footsteps::default();
        let standing = player(jumper, [0.0, 0.0, 0.0], true);
        let rising = player(jumper, [0.0, 6.0, 0.0], false);
        steps.update(1.0 / 60.0, [standing], &archetypes, |_| "sand".into());
        let heard = steps.update(1.0 / 60.0, [rising], &archetypes, |_| "sand".into());
        assert_eq!(heard.len(), 1, "{heard:?}");
        assert_eq!(heard[0].sound, "test:sand_step");
        let contact = heard[0].contact.as_ref().expect("sand leaves dust");
        assert_eq!(contact.effect.as_deref(), Some("test:dust"));
        // Still rising: not heard again.
        assert!(
            steps
                .update(1.0 / 60.0, [rising], &archetypes, |_| "sand".into())
                .is_empty()
        );
        // Stone leaves nothing.
        let mut stone = Footsteps::default();
        stone.update(1.0 / 60.0, [standing], &archetypes, |_| "stone".into());
        let heard = stone.update(1.0 / 60.0, [rising], &archetypes, |_| "stone".into());
        assert!(heard.iter().all(|h| h.contact.is_none()), "{heard:?}");
    }

    #[test]
    fn a_mark_sets_the_loudness_of_a_surface_step_and_a_prone_body_crawls() {
        let (mut archetypes, walker) = walker_body();
        let mut body = archetypes.resolve(walker).clone();
        body.id = "test:archetype/crawler".into();
        body.sounds.fps = Some(30.0);
        body.sounds.steps.insert(
            "run".into(),
            vec![StepEvent {
                seconds: Some(0.5),
                volume: Some(0.3),
                radius: Some(90.0),
                ..Default::default()
            }],
        );
        body.sounds.posture_steps.insert(
            "prone".into(),
            bri_sim::archetype::PostureSteps {
                footsteps: vec!["test:crawl".into()],
                surfaces: [("metal".to_string(), vec!["test:crawl_metal".to_string()])].into(),
            },
        );
        let crawler = archetypes.add(body).expect("valid body");
        let sounds = &archetypes.resolve(crawler).sounds;
        let mut steps = Footsteps::default();
        let standing = steps.on_clip(7, sounds, &span(0.4, 0.6), |_| [0.0; 3], |_| "dirt".into());
        assert_eq!(standing.len(), 1);
        assert_ne!(standing[0].sound, "test:crawl");
        assert_eq!((standing[0].volume, standing[0].radius), (0.3, 90.0));
        // Once the body is seen lying down, its marks play the crawl's sounds.
        let mut prone = player(crawler, [0.0; 3], true);
        prone.posture = "prone";
        steps.update(1.0 / 60.0, [prone], &archetypes, |_| "dirt".into());
        let on_dirt = steps.on_clip(7, sounds, &span(0.4, 0.6), |_| [0.0; 3], |_| "dirt".into());
        assert_eq!(on_dirt[0].sound, "test:crawl");
        let on_metal = steps.on_clip(7, sounds, &span(0.4, 0.6), |_| [0.0; 3], |_| "metal".into());
        assert_eq!(on_metal[0].sound, "test:crawl_metal");
    }

    #[test]
    fn a_mark_can_name_another_sound_and_a_posture_change_sounds_once() {
        let (mut archetypes, walker) = walker_body();
        let mut body = archetypes.resolve(walker).clone();
        body.id = "test:archetype/climber".into();
        body.sounds.fps = Some(30.0);
        body.sounds.steps.insert(
            "run".into(),
            vec![StepEvent {
                seconds: Some(0.5),
                sound: Some("test:climb".into()),
                volume: Some(0.9),
                radius: Some(150.0),
                ..Default::default()
            }],
        );
        body.sounds
            .postures
            .insert("crouch".into(), "test:down".into());
        let climber = archetypes.add(body).expect("valid body");
        let mut steps = Footsteps::default();
        let sounds = &archetypes.resolve(climber).sounds;
        let heard = steps.on_clip(7, sounds, &span(0.4, 0.6), |_| [0.0; 3], |_| "dirt".into());
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].sound, "test:climb");
        assert_eq!((heard[0].volume, heard[0].radius), (0.9, 150.0));
        assert!(heard[0].contact.is_none());
        let mut standing = player(climber, [0.0; 3], true);
        assert!(
            steps
                .update(1.0 / 60.0, [standing], &archetypes, |_| "dirt".into())
                .is_empty()
        );
        standing.posture = "crouch";
        let heard = steps.update(1.0 / 60.0, [standing], &archetypes, |_| "dirt".into());
        assert_eq!(heard.len(), 1, "{heard:?}");
        assert_eq!(heard[0].sound, "test:down");
        assert!(
            steps
                .update(1.0 / 60.0, [standing], &archetypes, |_| "dirt".into())
                .is_empty()
        );
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
        let names: Vec<_> = heard.iter().map(|h| h.sound.as_str()).collect();
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
        assert_eq!(heard.len(), 1);
        assert_eq!(
            (heard[0].sound.as_str(), heard[0].at),
            ("test:land", landed.feet)
        );
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
