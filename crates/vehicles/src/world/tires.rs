//! Torque's `WheeledVehicle` wheels, which blocklandv20.exe keeps from stock
//! Torque (`extendWheels`, `updateForces` 0x574490): each wheel is a ray
//! from its hub down the spring and tyre, a spring pushing up by how far it
//! is compressed, and a tyre that is itself a spring sideways and lengthways.
//! The tyre stretches as the ground slides under it and pulls the vehicle
//! with that stretch, inside a friction circle of its load. So a tyre gives
//! a little before it grips and slides once it is pushed past its grip:
//! v20's Tank pivots almost on the spot at full lock because its rear
//! tyres, steering the other way, slide round instead of holding a line.
use super::*;
use bri_console::Clamp;

/// One wheel as `WheeledVehicle::Wheel` keeps it between ticks.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WheelState {
    /// Spring extension: 0 fully compressed, 1 fully extended.
    pub extension: f32,
    /// On the ground at the last update (`surface.contact`).
    pub contact: bool,
    /// Rolled angle, radians, forward positive (`apos`).
    pub rotation: f32,
    pub tire: TireState,
}
impl Default for WheelState {
    fn default() -> Self {
        Self {
            extension: 1.,
            contact: false,
            rotation: 0.,
            tire: TireState::default(),
        }
    }
}
/// A tyre's motion state, which the next tick's forces start from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TireState {
    /// `avel`: the wheel's spin, rad/s, rolling forward positive.
    pub spin: f32,
    /// `Dx`, `Dy`: how far the tyre is stretched sideways and lengthways.
    pub lateral: f32,
    pub longitudinal: f32,
    /// Past its grip at the last update: kinetic friction holds it.
    pub slipping: bool,
}
impl TireState {
    pub fn is_finite(&self) -> bool {
        self.spin.is_finite() && self.lateral.is_finite() && self.longitudinal.is_finite()
    }
}
/// What the driver asks of the wheels this tick.
pub(super) struct Drive {
    /// `mSteering.x`, radians up to `maxSteeringAngle`, right positive.
    pub steering: f32,
    pub throttle: f32,
    pub braking: bool,
    pub jetting: bool,
}
/// The wheel opposite each one (`WheeledVehicleData::preload`): level with
/// it front to back on the other side, for the anti-sway spring.
pub(super) fn opposites(d: &Definition) -> Vec<Option<usize>> {
    d.wheels
        .iter()
        .enumerate()
        .map(|(i, w)| {
            d.wheels.iter().enumerate().position(|(j, o)| {
                j != i
                    && (o.position[2] - w.position[2]).abs() < 0.1
                    && (o.position[0] + w.position[0]).abs() < 0.1
            })
        })
        .collect()
}
struct Surface {
    normal: Vec3,
    point: Vec3,
}
/// `WheeledVehicle::extendWheels` then the wheel half of `updateForces`:
/// finds each wheel's ground, adds the spring and tyre forces to the body
/// and advances the wheels' spin and tyre stretch by `dt`.
pub(super) fn update(
    world: &mut PhysicsWorld,
    body: RigidBodyHandle,
    d: &Definition,
    scale: f32,
    wheels: &mut [WheelState],
    drive: Drive,
    dt: f32,
) {
    let (origin, rotation) = {
        let b = &world.bodies[body];
        (b.translation(), *b.rotation())
    };
    let right = rotation * Vec3::X;
    let forward = rotation * -Vec3::Z;
    let up = rotation * Vec3::Y;
    let mount = |w: &Wheel| origin + rotation * (Vec3::from_array(w.position) * scale);
    // The ray runs from the hub's mount down the whole spring and tyre; a
    // hit within the tyre's radius of the mount compresses it fully.
    let surfaces: Vec<Option<Surface>> = {
        let q = world.query_pipeline_with_filter(
            QueryFilter::default()
                .exclude_rigid_body(body)
                .exclude_sensors(),
        );
        d.wheels
            .iter()
            .zip(wheels.iter_mut())
            .map(|(w, s)| {
                let (length, radius) = (w.rest_length * scale, w.radius * scale);
                let start = mount(w);
                let hit = q.cast_ray_and_get_normal(&Ray::new(start, -up), length + radius, true);
                s.contact = hit.is_some();
                s.extension = hit.map_or(1., |(_, hit)| {
                    ((hit.time_of_impact - radius) / length).clamped(0., 1.)
                });
                hit.map(|(_, hit)| Surface {
                    normal: if hit.normal.length_squared() > 0.5 {
                        hit.normal
                    } else {
                        up
                    },
                    point: start - up * hit.time_of_impact,
                })
            })
            .collect()
    };
    let opposite = opposites(d);
    let extensions: Vec<Option<f32>> = wheels
        .iter()
        .map(|s| s.contact.then_some(s.extension))
        .collect();
    let b = &mut world.bodies[body];
    let mass = d.mass;
    let momentum = mass / wheels.len().max(1) as f32;
    // Engine and brakes: the brake, else the throttle (doubled while
    // jetting forward), else the engine brake.
    let (engine, brake) = if drive.braking {
        (0., d.brake_force / momentum * dt)
    } else if drive.throttle != 0. {
        let doubled = if drive.throttle > 0. && drive.jetting {
            2.
        } else {
            1.
        };
        (d.engine_force * drive.throttle * doubled, 0.)
    } else {
        (0., d.engine_brake / momentum * dt)
    };
    // blocklandv20.exe squares the steering (0x5746ea); each wheel's axle
    // turns by its own share of it (the Tank's rear, -0.8, against the front).
    let (sin, cos) = (-(drive.steering * drive.steering.abs())).sin_cos();
    let com = b.center_of_mass();
    let inverse_inertia = b.mass_properties().effective_world_inv_inertia;
    for (i, ((w, s), surface)) in d
        .wheels
        .iter()
        .zip(wheels.iter_mut())
        .zip(&surfaces)
        .enumerate()
    {
        let (length, radius) = (w.rest_length * scale, w.radius * scale);
        let t = &w.tire;
        let tire = &mut s.tire;
        let mut fy = 0.;
        if let Some(surface) = surface {
            let at = mount(w);
            let velocity = b.velocity_at_point(at);
            let spring = w.spring * (1. - s.extension);
            if s.extension == 0. {
                // Bottomed out: an impulse along world up stops the mount
                // sinking into the ground, applied as a force.
                let n = -velocity.y;
                if n >= 0. {
                    let r = at - com;
                    let zero = 1. / (1. / mass + (inverse_inertia * r.cross(Vec3::Y)).cross(r).y);
                    b.add_force(Vec3::Y * (n * (1. + d.restitution) * zero), true);
                }
            }
            // The damper only resists compression.
            let damping = (w.damping * -up.dot(velocity) / length).max(0.);
            let anti_sway = opposite[i]
                .and_then(|o| extensions[o])
                .map_or(0., |other| ((other - s.extension) * w.anti_sway).max(0.));
            let load = spring + damping + anti_sway;
            b.add_force_at_point(up * load, at, true);
            // The tyre's own axes along the ground.
            let axle = right * cos + forward * (sin * w.steering);
            let along = surface.normal.cross(axle).normalize_or_zero();
            let across = along.cross(surface.normal).normalize_or_zero();
            let ground = b.velocity_at_point(surface.point);
            let (xv, yv) = (across.dot(ground), along.dot(ground));
            let dy = (tire.spin * radius - yv)
                - t.longitudinal_relaxation * tire.spin.abs() * tire.longitudinal;
            tire.longitudinal += dy * dt;
            fy = t.longitudinal_force * tire.longitudinal + t.longitudinal_damping * dy;
            let dx = xv - t.lateral_relaxation * tire.spin.abs() * tire.lateral;
            tire.lateral += dx * dt;
            let mut fx = -(t.lateral_force * tire.lateral + t.lateral_damping * dx);
            let grip = load.max(0.)
                * if tire.slipping {
                    t.kinetic_friction
                } else {
                    t.static_friction
                };
            let (limit, pull) = (grip * grip, fx * fx + fy * fy);
            if pull > limit {
                let k = (limit / pull).sqrt();
                fx *= k;
                fy *= k;
                tire.lateral *= k;
                tire.longitudinal *= k;
                tire.slipping = true;
            } else {
                tire.slipping = false;
            }
            // Tyre forces act at the hub.
            let hub = at - up * (length * s.extension);
            b.add_force_at_point(across * fx + along * fy, hub, true);
        } else {
            // In the air the stretch relaxes as the wheel spins.
            tire.slipping = true;
            tire.longitudinal -=
                t.longitudinal_relaxation * tire.spin.abs() * tire.longitudinal * dt;
            tire.lateral -= t.lateral_relaxation * tire.spin.abs() * tire.lateral * dt;
        }
        // The engine turns powered wheels, less as they near maxWheelSpeed;
        // the tyre's pull turns every wheel back.
        let scale_torque = if w.powered {
            let max = d.max_speed / radius;
            if tire.spin.abs() > max {
                0.
            } else {
                1. - tire.spin.abs() / max
            }
        } else {
            0.
        };
        tire.spin += (scale_torque * engine - fy * radius) / momentum * dt;
        // Brakes after, so a braked wheel comes to a full stop.
        if brake > tire.spin.abs() {
            tire.spin = 0.;
        } else {
            tire.spin -= brake * tire.spin.signum();
        }
        s.rotation = (s.rotation + tire.spin * dt).rem_euclid(std::f32::consts::TAU);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{
        Controls, Motion, Occupant, OccupantId, OwnerId, Spawn, VehicleId, VehiclesWorld,
    };

    /// Torque's `WheeledVehicle::updateForces` on flat ground in two
    /// dimensions, each wheel carrying a quarter of the weight: the tyre
    /// springs, friction circle, wheel spin and v20's drag, integrated at
    /// the datablock's `integration = 4` (8 ms). No suspension or load
    /// transfer, so it is a check on the full model, not a copy of it.
    /// Returns the circle's radius (speed over yaw rate) after `seconds`
    /// at full throttle with the steering held at `steer`.
    fn torque_circle(d: &Definition, steer: f32, seconds: f32) -> f32 {
        let dt = 0.032 / 4.;
        let n = d.wheels.len();
        let momentum = d.mass / n as f32;
        let load = d.mass * VEHICLE_GRAVITY / n as f32;
        let [bx, _, bz] = d.inertia_box;
        let inertia = d.mass / 12. * (bx * bx + bz * bz);
        let (mut position, mut heading) = (glam::Vec2::ZERO, 0f32);
        let (mut velocity, mut yaw) = (glam::Vec2::ZERO, 0f32);
        let mut tires = vec![TireState::default(); n];
        let (sin, cos) = (-(steer * steer.abs())).sin_cos();
        let mut t = 0.;
        while t < seconds {
            // x right, y forward; heading turns left positive.
            let right = glam::Vec2::new(heading.cos(), -heading.sin());
            let forward = glam::Vec2::new(heading.sin(), heading.cos());
            let (mut force, mut torque) = (glam::Vec2::ZERO, 0.);
            for (w, tire) in d.wheels.iter().zip(&mut tires) {
                let t = &w.tire;
                let axle = (right * cos + forward * (sin * w.steering)).normalize();
                let (across, along) = (axle, glam::Vec2::new(-axle.y, axle.x));
                let r = right * w.position[0] + forward * -w.position[2];
                let at = velocity + glam::Vec2::new(-yaw * r.y, yaw * r.x);
                let (xv, yv) = (across.dot(at), along.dot(at));
                let dy = (tire.spin * w.radius - yv)
                    - t.longitudinal_relaxation * tire.spin.abs() * tire.longitudinal;
                tire.longitudinal += dy * dt;
                let mut fy = t.longitudinal_force * tire.longitudinal + t.longitudinal_damping * dy;
                let dx = xv - t.lateral_relaxation * tire.spin.abs() * tire.lateral;
                tire.lateral += dx * dt;
                let mut fx = -(t.lateral_force * tire.lateral + t.lateral_damping * dx);
                let mu = if tire.slipping {
                    t.kinetic_friction
                } else {
                    t.static_friction
                };
                let (limit, pull) = ((load * mu).powi(2), fx * fx + fy * fy);
                tire.slipping = pull > limit;
                if tire.slipping {
                    let k = (limit / pull).sqrt();
                    fx *= k;
                    fy *= k;
                    tire.lateral *= k;
                    tire.longitudinal *= k;
                }
                let f = across * fx + along * fy;
                force += f;
                torque += r.perp_dot(f);
                let max = d.max_speed / w.radius;
                let engine = if w.powered && tire.spin.abs() <= max {
                    (1. - tire.spin.abs() / max) * d.engine_force
                } else {
                    0.
                };
                tire.spin += (engine - fy * w.radius) / momentum * dt;
            }
            velocity += (force - velocity * d.drag) / d.mass * dt;
            yaw += (torque / inertia - yaw * (d.angular_drag + d.drag)) * dt;
            position += velocity * dt;
            heading += yaw * dt;
            t += dt;
        }
        velocity.length() / yaw.abs()
    }

    /// Ours, in the full simulation: a driver holds the mouse steering at
    /// `steer` and full throttle on flat ground.
    fn driven(pack: Pack, id: &str) -> (VehiclesWorld, PhysicsWorld) {
        let mut v = VehiclesWorld::new(pack).unwrap();
        let mut w = bri_physics::new_world();
        w.insert(
            RigidBodyBuilder::fixed().translation(Vec3::new(0., -0.5, 0.)),
            ColliderBuilder::cuboid(2000., 0.5, 2000.),
        );
        v.spawn(
            &mut w,
            Spawn {
                scale: 1.,
                id: VehicleId(1),
                owner: OwnerId(10),
                definition: id.into(),
                transform: Transform {
                    position: [0., 1.5, 0.],
                    ..Default::default()
                },
                spawn_id: None,
                respawn_ticks: None,
            },
        )
        .unwrap();
        w.detect_collisions(&(), &());
        let seat = v.snapshot(&w).vehicles[0].seats[0].transform.position;
        let rider = Occupant {
            id: OccupantId(20),
            owner: OwnerId(10),
            body: [1.25, 2.65],
        };
        v.mount(&w, VehicleId(1), 0, rider, seat).unwrap();
        (v, w)
    }
    fn drive(v: &mut VehiclesWorld, w: &mut PhysicsWorld, c: Controls) {
        v.set_controls(OwnerId(10), OccupantId(20), c).unwrap();
        v.pre_step(w, &[]).unwrap();
        w.step();
        v.post_step(w).unwrap();
    }
    /// Full throttle, the mouse turned by `steer` on the first step.
    fn turning(step: usize, steer: f32) -> Controls {
        Controls {
            throttle: 1.,
            strafe_steering_off: true,
            auto_return_off: true,
            look_delta: [if step == 0 { steer } else { 0. }, 0.],
            ..Default::default()
        }
    }
    fn our_circle(pack: Pack, id: &str, steer: f32, seconds: f32) -> f32 {
        let (mut v, mut w) = driven(pack, id);
        for step in 0..(seconds / FIXED_DT).round() as usize {
            drive(&mut v, &mut w, turning(step, steer));
        }
        let s = &v.snapshot(&w).vehicles[0];
        Vec3::from(s.velocity).length() / s.angular_velocity[1].abs()
    }

    fn check(pack: Pack, id: &str) {
        let d = pack
            .definitions
            .iter()
            .find(|d| d.id == id)
            .unwrap()
            .clone();
        for steer in [0.25, 0.5, d.max_steering] {
            let torque = torque_circle(&d, steer, 6.);
            let ours = our_circle(pack.clone(), id, steer, 6.);
            assert!(
                (ours - torque).abs() < torque * 0.2,
                "steering {steer}: ours circles in {ours}, Torque's tyres in {torque}"
            );
        }
    }

    /// At full lock a tank's rear tyres, steering against the front, slide
    /// round and it pivots almost on the spot; at part lock it holds a wide
    /// circle. Rapier's near-rigid wheels circled far wider at full lock
    /// than Torque's tyres do.
    #[test]
    fn a_tank_turns_as_torques_tyres_do() {
        check(crate::testing::pack(), crate::testing::TANK);
    }

    /// A driving client resets its vehicle to the host's pose and replays
    /// its moves: with the wheels' spin and tyre stretch in the pose, the
    /// replay lands exactly where the host does, slipping tyres included.
    #[test]
    fn a_replay_from_the_hosts_pose_matches_the_host() {
        let tank = crate::testing::TANK;
        let full_lock = crate::testing::definition(tank).max_steering;
        // Slick tyres, so the replay has slipping wheels to get right.
        let slick = || {
            crate::testing::pack_with(|d| {
                for wheel in &mut d.wheels {
                    wheel.tire.static_friction = 0.3;
                    wheel.tire.kinetic_friction = 0.2;
                }
            })
        };
        let (mut host, mut hw) = driven(slick(), tank);
        let (mut client, mut cw) = driven(slick(), tank);
        for step in 0..240 {
            drive(&mut host, &mut hw, turning(step, full_lock));
        }
        let s = host.snapshot(&hw).vehicles.remove(0);
        assert!(s.wheel_tire.iter().any(|t| t.slipping), "full lock slides");
        let motion = Motion {
            passage_frame: Default::default(),
            transform: s.transform.clone(),
            velocity: s.velocity,
            angular_velocity: s.angular_velocity,
            mouse_steering: s.mouse_steering,
            steering: s.steering,
            steering_quiet: s.steering_quiet,
            wheel_suspension: s.wheel_suspension.clone(),
            wheel_rotation: s.wheel_rotation.clone(),
            wheel_contact: s.wheel_contact.clone(),
            wheel_tire: s.wheel_tire.clone(),
            actor: None,
        };
        client
            .restore_motion(&mut cw, VehicleId(1), &motion)
            .unwrap();
        for step in 1..121 {
            drive(&mut host, &mut hw, turning(step, 0.));
            drive(&mut client, &mut cw, turning(step, 0.));
        }
        let (a, b) = (
            &host.snapshot(&hw).vehicles[0],
            &client.snapshot(&cw).vehicles[0],
        );
        let apart = Vec3::from(a.transform.position).distance(Vec3::from(b.transform.position));
        assert!(apart < 1e-3, "the replay ends {apart} from the host");
        assert_eq!(a.wheel_tire.len(), 4);
        for (a, b) in a.wheel_tire.iter().zip(&b.wheel_tire) {
            assert!((a.spin - b.spin).abs() < 1e-2, "{a:?} {b:?}");
        }
    }

    /// The same for v20's own Tank, whose part-lock circles were right once
    /// its steering was squared and whose full lock circled 12 on
    /// Rapier's wheels.
    #[test]
    #[ignore = "requires generated v20 content"]
    fn the_v20_tank_turns_as_torques_tyres_do() {
        let pack = Pack::load(
            bri_package::testing::pack_dir(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
                "vehicles",
            )
            .join("vehicles.json"),
        )
        .unwrap();
        check(pack, "v20.vehicle.tankvehicle");
    }
}
