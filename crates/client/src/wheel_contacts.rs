//! What a vehicle's sliding tyres throw up, by the surface each stands on
//! ([`bri_vehicles::schema::WheelContacts`]).
//!
//! Each client works it out for every vehicle it draws from the poses it
//! already has, so it costs nothing on the wire: a wheel in contact whose
//! tyre slips (across its axle, or along it as the wheel spins faster or
//! slower than the ground passes) at least `min_slip` makes the effect its
//! surface names every `interval`, and its sound every `sound_interval`.
use crate::vehicles::VehicleFrame;
use bri_vehicles::Definition;
use glam::{Quat, Vec3};
use std::collections::BTreeMap;

/// One effect a wheel makes this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactBurst {
    pub vehicle: u64,
    pub wheel: usize,
    /// The effect to burst, when this wheel's effect is due.
    pub effect: Option<String>,
    /// The sound to play, when this wheel's sound is due.
    pub sound: Option<String>,
    /// Where the tyre meets the ground.
    pub position: Vec3,
}

#[derive(Default)]
struct WheelState {
    /// The wheel's spin angle last frame, for how fast it turns.
    rotation: Option<f32>,
    /// Seconds until the next effect and the next sound.
    effect_in: f32,
    sound_in: f32,
}

#[derive(Default)]
pub struct WheelContactEffects {
    wheels: BTreeMap<(u64, usize), WheelState>,
}

/// How fast a tyre slips against the ground, units per second: the slide
/// across its axle together with the difference between the speed its
/// spin would carry it and the speed it moves along.
pub fn slip_speed(d: &Definition, frame: &VehicleFrame, wheel: usize, spin: Option<f32>) -> f32 {
    let w = &d.wheels[wheel];
    let turn = frame.rotation * Quat::from_rotation_y(-w.steer_angle(frame.steering));
    let across = frame.velocity.dot(turn * Vec3::X);
    let along = frame.velocity.dot(turn * Vec3::NEG_Z);
    let rolling = spin.map_or(along.abs(), |rate| rate.abs() * w.radius);
    across.hypot(rolling - along.abs())
}

impl WheelContactEffects {
    /// Advance `seconds` for the vehicles drawn now and return what their
    /// wheels make. `surface` names the surface kind at a ground point.
    pub fn update<'a>(
        &mut self,
        seconds: f32,
        vehicles: impl IntoIterator<Item = (u64, &'a Definition, &'a VehicleFrame)>,
        mut surface: impl FnMut(Vec3) -> String,
    ) -> Vec<ContactBurst> {
        let mut out = Vec::new();
        let mut seen = Vec::new();
        if !(seconds.is_finite() && seconds > 0.0) {
            return out;
        }
        let seconds = seconds.min(0.25);
        for (id, d, frame) in vehicles {
            let Some(contacts) = d.wheel_contacts.as_ref() else {
                continue;
            };
            if !frame.position.is_finite() || !frame.velocity.is_finite() {
                continue;
            }
            for (i, w) in d.wheels.iter().enumerate() {
                let key = (id, i);
                seen.push(key);
                let state = self.wheels.entry(key).or_default();
                let rotation = frame
                    .wheel_rotation
                    .get(i)
                    .copied()
                    .filter(|r| r.is_finite());
                let spin = rotation.zip(state.rotation).map(|(now, before)| {
                    let turn = (now - before + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    turn / seconds
                });
                state.rotation = rotation;
                state.effect_in = (state.effect_in - seconds).max(-contacts.interval);
                state.sound_in = (state.sound_in - seconds).max(-contacts.sound_interval);
                if !frame.wheel_contact.get(i).copied().unwrap_or(false) {
                    continue;
                }
                if slip_speed(d, frame, i, spin) < contacts.min_slip {
                    continue;
                }
                let suspension = frame
                    .wheel_suspension
                    .get(i)
                    .copied()
                    .unwrap_or(w.rest_length);
                let position = frame.position
                    + frame.rotation * (Vec3::from(w.position) - Vec3::Y * (suspension + w.radius));
                let Some(effect) = contacts.effect_on(&surface(position)) else {
                    continue;
                };
                let mut burst = ContactBurst {
                    vehicle: id,
                    wheel: i,
                    effect: None,
                    sound: None,
                    position,
                };
                let mut fire = false;
                if state.effect_in <= 0.0 {
                    state.effect_in += contacts.interval;
                    burst.effect = Some(effect.effect.clone());
                    fire = true;
                }
                if let Some(sound) = effect.sound.as_ref()
                    && state.sound_in <= 0.0
                {
                    state.sound_in += contacts.sound_interval;
                    burst.sound = Some(sound.clone());
                    fire = true;
                }
                if fire {
                    out.push(burst);
                }
            }
        }
        self.wheels.retain(|key, _| seen.contains(key));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_vehicles::schema::{ContactEffect, WheelContacts};

    fn car() -> Definition {
        let mut d = bri_vehicles::testing::car();
        d.wheel_contacts = Some(WheelContacts {
            min_slip: 3.0,
            interval: 0.1,
            sound_interval: 0.5,
            effects: vec![
                ContactEffect {
                    surface: "sand".into(),
                    effect: "t:dust".into(),
                    sound: Some("t:skid".into()),
                },
                ContactEffect {
                    surface: "stone".into(),
                    effect: "t:chips".into(),
                    sound: None,
                },
            ],
        });
        d
    }
    fn frame(d: &Definition, velocity: Vec3, rotation: f32) -> VehicleFrame {
        VehicleFrame {
            position: Vec3::new(0., 1., 0.),
            rotation: Quat::IDENTITY,
            velocity,
            steering: 0.,
            wheel_suspension: vec![0.2; d.wheels.len()],
            wheel_rotation: vec![rotation; d.wheels.len()],
            wheel_contact: vec![true; d.wheels.len()],
            driver_jump_held: false,
            turret_aim: [0.; 2],
        }
    }
    fn run(
        d: &Definition,
        seconds: f32,
        velocity: Vec3,
        spin: f32,
        surface: &str,
    ) -> Vec<ContactBurst> {
        let mut effects = WheelContactEffects::default();
        let mut out = Vec::new();
        let r = d.wheels[0].radius;
        let mut rotation = 0.0f32;
        for _ in 0..(seconds * 60.0) as usize {
            rotation = (rotation + spin / r / 60.0).rem_euclid(std::f32::consts::TAU);
            out.extend(
                effects.update(1.0 / 60.0, [(1, d, &frame(d, velocity, rotation))], |_| {
                    surface.to_string()
                }),
            );
        }
        out
    }

    #[test]
    fn rolling_grips_but_sliding_across_the_axle_throws_up_the_surfaces_effect() {
        let d = car();
        // Rolling at 10 u/s with the wheels turning as fast: no slip.
        assert!(run(&d, 1.0, Vec3::new(0., 0., -10.), 10.0, "sand").is_empty());
        // Sliding sideways at 8.
        let slide = run(&d, 1.0, Vec3::new(8., 0., -10.), 10.0, "sand");
        let effects: Vec<_> = slide.iter().filter_map(|b| b.effect.as_deref()).collect();
        assert!(effects.iter().all(|e| *e == "t:dust"));
        // Four wheels, one effect each 0.1 s: about ten a wheel a second.
        assert!((36..=44).contains(&effects.len()), "{}", effects.len());
        // The squeal comes at its own, slower, rate: two a wheel a second.
        let sounds = slide.iter().filter(|b| b.sound.is_some()).count();
        assert!((4..=12).contains(&sounds), "{sounds}");
    }
    #[test]
    fn a_spinning_wheel_slips_along_the_ground_too() {
        let d = car();
        // Parked, wheels spinning at 10 u/s of tyre speed.
        let burnout = run(&d, 1.0, Vec3::ZERO, 10.0, "stone");
        assert!(
            !burnout.is_empty()
                && burnout
                    .iter()
                    .all(|b| b.effect.as_deref() == Some("t:chips"))
        );
        // A locked wheel dragging along at speed: also slip.
        assert!(!run(&d, 1.0, Vec3::new(0., 0., -10.), 0.0, "stone").is_empty());
    }
    #[test]
    fn the_surface_decides_and_a_row_with_nothing_makes_nothing() {
        let d = car();
        assert!(
            run(&d, 0.5, Vec3::new(8., 0., 0.), 0.0, "stone")
                .iter()
                .all(|b| b.sound.is_none())
        );
        assert!(run(&d, 0.5, Vec3::new(8., 0., 0.), 0.0, "metal").is_empty());
        let mut d = d;
        d.wheel_contacts
            .as_mut()
            .unwrap()
            .effects
            .push(ContactEffect {
                surface: "*".into(),
                effect: "t:any".into(),
                sound: None,
            });
        assert!(
            run(&d, 0.5, Vec3::new(8., 0., 0.), 0.0, "metal")
                .iter()
                .all(|b| b.effect.as_deref() == Some("t:any"))
        );
    }
    #[test]
    fn a_wheel_in_the_air_makes_nothing() {
        let d = car();
        let mut effects = WheelContactEffects::default();
        let mut f = frame(&d, Vec3::new(9., 0., 0.), 0.0);
        f.wheel_contact = vec![false; d.wheels.len()];
        for _ in 0..30 {
            assert!(
                effects
                    .update(1.0 / 60.0, [(1, &d, &f)], |_| "sand".into())
                    .is_empty()
            );
        }
    }
}
