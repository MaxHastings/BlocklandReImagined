//! Footfalls and landings for bodies that bring their own (`sounds.footsteps`,
//! `sounds.land` on an archetype). Each client plays them for every body it
//! draws, from the poses it already has, so they cost nothing on the wire:
//! a step each stride of ground covered while grounded, and a landing when
//! a body that was falling fast touches down. v20's Blockhead has neither.
use bri_sim::archetype::{ArchetypeId, Archetypes, Sounds};
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
    /// Advance `seconds` for the bodies drawn now (`players`, without those
    /// in a seat) and return the sounds to play and where.
    pub fn update(
        &mut self,
        seconds: f32,
        players: impl IntoIterator<Item = Walk>,
        archetypes: &Archetypes,
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
                if walker.falling >= Sounds::LAND_SPEED
                    && let Some(land) = sounds.land.as_ref().filter(|s| !s.is_empty())
                {
                    out.push((land.clone(), player.feet));
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
            if sounds.footsteps.is_empty() || !player.grounded {
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
                let count = sounds.footsteps.len();
                self.turn = self.turn.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let mut pick = (self.turn >> 16) as usize % count;
                if count > 1 && pick == walker.last {
                    pick = (pick + 1) % count;
                }
                walker.last = pick;
                out.push((sounds.footsteps[pick].clone(), player.feet));
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

    #[test]
    fn a_running_body_steps_each_stride_alternating_its_sounds() {
        let (archetypes, id) = walker_body();
        let mut steps = Footsteps::default();
        let runner = player(id, [9.0, 0.0, 0.0], true);
        let mut heard = Vec::new();
        // Two seconds at 9 units a second: 18 units, six 3-unit strides.
        for _ in 0..120 {
            heard.extend(steps.update(1.0 / 60.0, [runner], &archetypes));
        }
        assert!((5..=6).contains(&heard.len()), "{heard:?}");
        assert!(heard.windows(2).all(|w| w[0].0 != w[1].0), "{heard:?}");
    }

    #[test]
    fn standing_or_falling_makes_no_steps_and_a_hard_landing_is_heard_once() {
        let (archetypes, id) = walker_body();
        let mut steps = Footsteps::default();
        let still = player(id, [0.1, 0.0, 0.0], true);
        assert!(steps.update(1.0, [still], &archetypes).is_empty());
        let falling = player(id, [9.0, -12.0, 0.0], false);
        for _ in 0..30 {
            assert!(steps.update(1.0 / 60.0, [falling], &archetypes).is_empty());
        }
        let landed = player(id, [0.0, 0.0, 0.0], true);
        let heard = steps.update(1.0 / 60.0, [landed], &archetypes);
        assert_eq!(heard, vec![("test:land".to_string(), landed.feet)]);
        assert!(steps.update(1.0 / 60.0, [landed], &archetypes).is_empty());
    }

    #[test]
    fn the_blockhead_makes_no_sound_walking() {
        let archetypes = Archetypes::default();
        let mut steps = Footsteps::default();
        let blockhead = player(ArchetypeId(0), [7.0, 0.0, 0.0], true);
        for _ in 0..120 {
            assert!(
                steps
                    .update(1.0 / 60.0, [blockhead], &archetypes)
                    .is_empty()
            );
        }
    }
}
