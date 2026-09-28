//! Native v20 vehicle data. No legacy reader or script execution is linked here.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
/// 5 adds the chase camera and seated look limits. 6 types the steering and
/// wheeled-flight fields (5 kept them only in `authored`), folds the
/// `FlyingWheeled` family into `Wheeled` and adds animation threads.
/// `Pack::load` still reads 5 and upgrades it.
pub const SCHEMA_VERSION: u32 = 6;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pack {
    pub schema_version: u32,
    pub definitions: Vec<Definition>,
    pub assets: Vec<Asset>,
    pub evidence: Vec<Evidence>,
    pub unresolved: Vec<String>,
    /// Keys are native vehicle ID + :: + authored sequence alias; values are native clips assets.
    pub animation_aliases: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset {
    pub virtual_path: String,
    pub path: String,
    pub sha256: String,
    pub kind: String,
    pub source_sha256: String,
    /// Content-root-relative directory of the package holding `path`, set
    /// when packs from several packages are merged; None is this pack's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    pub sha256: String,
    pub line: usize,
    pub subject: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Family {
    /// Blockland's `WheeledVehicle`; it flies when `wheeled_flight` is set.
    Wheeled,
    /// Torque's `FlyingVehicle`: hovers, and `flight` holds its fields.
    Flying,
    Horse,
    Ball,
    Cannon,
    Rowboat,
    Turret,
    Skis,
    Tumble,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transform {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.; 3],
            rotation: [0., 0., 0., 1.],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Seat {
    pub node: String,
    pub transform: Transform,
    pub pose: String,
    pub controls: bool,
    pub weapon: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wheel {
    /// Hub node: the suspension's fully compressed mount point.
    pub position: [f32; 3],
    /// Torque derives this from the tire shape's bounds, not the datablock field.
    pub radius: f32,
    pub rest_length: f32,
    pub spring: f32,
    pub damping: f32,
    pub friction: f32,
    pub steering: f32,
    pub powered: bool,
    pub model: String,
    /// Turns the tire model, authored with its hub axis along forward, so the
    /// axle lies along X with the tire's outer face pointing away from the chassis.
    pub model_rotation: [f32; 4],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Weapon {
    pub projectile: String,
    pub muzzle: Transform,
    pub pivot: [f32; 3],
    pub cooldown_ticks: u64,
    pub speed: f32,
    pub charge_ticks: u64,
    pub charge_steps: u8,
    pub sound: String,
    pub effect: String,
    /// Weapon-frame muzzle node positions from full up to full down, sampled
    /// from the model's `look` clip at load (see `muzzle`). Empty without one.
    #[serde(skip)]
    pub look_muzzle: Vec<[f32; 3]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnergySettings {
    pub maximum: f32,
    pub minimum_jet: f32,
    pub drain_per_32ms: f32,
    pub recharge_per_32ms: f32,
    pub jet_force: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlightSettings {
    pub hover_height: f32,
    pub create_hover_height: f32,
    pub min_drag: f32,
    pub max_auto_speed: f32,
    pub auto_linear_force: f32,
    pub auto_angular_force: f32,
    pub auto_input_damping: f32,
    pub horizontal_surface_force: f32,
    pub vertical_surface_force: f32,
    pub steering_force: f32,
    pub steering_roll_force: f32,
    pub vertical_thrust_multiple: f32,
}
/// Blockland's flying forces on `WheeledVehicle` (blocklandv20.exe
/// `WheeledVehicle::updateForces` 0x5746a0, fields registered at 0x5703ea).
/// The thrust, lift and turning forces are `Definition::thrust`,
/// `reverse_thrust`, `lift`, `pitch_force`, `yaw_force` and `roll_force`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WheeledFlightSettings {
    /// `maxForwardVel`: thrust only below this speed along the nose; the
    /// surfaces bite fully at `stallSpeed` + this.
    pub max_forward_vel: f32,
    /// `maxReverseVel`: reverse thrust only below this speed.
    pub max_reverse_vel: f32,
    /// `horizontalSurfaceForce`: resists sideways air.
    pub horizontal_surface_force: f32,
    /// `verticalSurfaceForce`: resists air through the roof; what makes a
    /// raised nose climb.
    pub vertical_surface_force: f32,
    /// `stallSpeed`: below it the surfaces and turning forces do nothing.
    pub stall_speed: f32,
    /// `isSled` (datablock +0x378): the surfaces bite only while wheel 0
    /// touches the ground (0x57565f).
    pub sled: bool,
}
/// How a driver's keys and mouse steer a wheeled vehicle
/// (`WheeledVehicle::updateMove` 0x570be0; defaults from its constructor).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SteeringSettings {
    /// `steeringStrafeSteeringRate` (0x5716dc): steering a held strafe key
    /// adds per 32 ms tick, radians.
    pub strafe_rate: f32,
    /// `steeringUseAutoReturn`: a move with no mouse turn returns the
    /// steering toward straight.
    pub auto_return: bool,
    /// `steeringAutoReturnRate`: share returned per tick at full throttle.
    pub auto_return_rate: f32,
    /// `steeringAutoReturnMaxSpeed`: the throttle at which the return is full.
    pub auto_return_max_speed: f32,
}
impl Default for SteeringSettings {
    fn default() -> Self {
        Self {
            strafe_rate: 0.1,
            auto_return: true,
            auto_return_rate: 0.9,
            auto_return_max_speed: 10.,
        }
    }
}
/// An animation the vehicle's model plays on its own, like a spinning
/// propeller (`ShapeBase::playThread(slot, sequence)` from a script such as
/// `onAdd`). Of a slot's threads, the first whose speed range holds the
/// vehicle's speed plays; one with no range always matches.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimationThread {
    /// Torque thread slot, 0 through 3. Threads on one slot replace each other.
    pub slot: u8,
    /// The model's sequence name.
    pub sequence: String,
    /// Playback rate: 1 is as authored, 2 twice as fast, and a negative rate
    /// plays backwards (`setThreadDir(slot, false)`).
    #[serde(default = "one")]
    pub rate: f32,
    /// Plays only at this speed or faster, units per second.
    #[serde(default)]
    pub min_speed: Option<f32>,
    /// Plays only below this speed.
    #[serde(default)]
    pub max_speed: Option<f32>,
}
fn one() -> f32 {
    1.
}
impl AnimationThread {
    pub fn matches(&self, speed: f32) -> bool {
        self.min_speed.is_none_or(|m| speed >= m) && self.max_speed.is_none_or(|m| speed < m)
    }
}
/// Third-person camera while riding (`Vehicle::getCameraTransform`; for
/// PlayerData mounts the player camera fields).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VehicleCamera {
    /// `cameraMaxDist`: distance behind the pivot.
    pub max_dist: f32,
    /// `cameraOffset` (`cameraVerticalOffset` on PlayerData): pivot height.
    pub offset: f32,
    /// `cameraTilt`: the view looks down this many radians.
    pub tilt: f32,
    /// `cameraLag`/`cameraDecay`: stock Torque's trailing camera. Blockland's
    /// `Vehicle::getCameraTransform` (0x56cc10) never reads them.
    pub lag: f32,
    pub decay: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Definition {
    pub id: String,
    pub datablock: String,
    pub name: String,
    pub family: Family,
    pub energy: EnergySettings,
    pub flight: Option<FlightSettings>,
    pub model: String,
    pub seats: Vec<Seat>,
    pub wheels: Vec<Wheel>,
    pub weapon: Option<Weapon>,
    pub attachment_model: Option<String>,
    pub attachment_collision_hulls: Vec<Vec<[f32; 3]>>,
    pub attachment_mount: Option<Transform>,
    pub attachment_fallback_seat: Option<Transform>,
    pub collision_hulls: Vec<Vec<[f32; 3]>>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub mass: f32,
    /// Authored `massCenter`, chassis-local.
    pub mass_center: [f32; 3],
    /// Box whose inertia the rigid body uses: `massBox`, else the shape bounds.
    pub inertia_box: [f32; 3],
    pub density: f32,
    pub drag: f32,
    pub friction: f32,
    pub restitution: f32,
    pub max_damage: f32,
    pub burn_ticks: u64,
    pub invulnerable_ticks: u64,
    pub initial_explosion: Option<String>,
    pub final_explosion: Option<String>,
    pub initial_explosion_offset: f32,
    pub final_explosion_offset: f32,
    pub mount_distance: f32,
    pub engine_force: f32,
    pub engine_brake: f32,
    pub brake_force: f32,
    pub max_speed: f32,
    pub reverse_speed: f32,
    pub max_steering: f32,
    pub thrust: f32,
    pub reverse_thrust: f32,
    pub lift: f32,
    pub yaw_force: f32,
    pub pitch_force: f32,
    pub roll_force: f32,
    pub angular_drag: f32,
    pub jump_speed: f32,
    pub max_side_speed: f32,
    pub run_surface_angle: f32,
    pub impact_threshold: f32,
    pub impact_damage: f32,
    /// `steeringUseStrafeSteering`: the strafe keys steer. Otherwise the
    /// mouse steers and pitches the vehicle (Torque `mSteering`).
    pub strafe_steering: bool,
    #[serde(default)]
    pub steering: SteeringSettings,
    /// Blockland's flying forces for a `Wheeled` or `Skis` vehicle; `None`
    /// is a car.
    #[serde(default)]
    pub wheeled_flight: Option<WheeledFlightSettings>,
    /// Animations the model plays by itself.
    #[serde(default)]
    pub threads: Vec<AnimationThread>,
    /// Actor look pitch range, native up-positive radians, from PlayerData
    /// `maxLookAngle`/`minLookAngle`. Bounds a gunner's barrel.
    pub look_pitch: [f32; 2],
    /// PlayerData `maxUnderwaterForward/Backward/SideSpeed`.
    pub underwater_speeds: [f32; 3],
    pub camera: VehicleCamera,
    /// `setLookLimits(lookUpLimit, lookDownLimit)` while seated, as
    /// [down, up] fractions of the look range from straight down (0) to
    /// straight up (1); [0, 1] is unlimited.
    pub look_limits: [f32; 2],
    pub runover_speed: f32,
    pub runover_damage: f32,
    pub runover_push: f32,
    pub protect_direct: bool,
    pub protect_radius: bool,
    pub protect_burn: bool,
    /// Authored fields retained as metadata only. Runtime reads typed native fields above.
    pub authored: BTreeMap<String, String>,
    pub adaptations: Vec<String>,
}
/// How a seat's occupant controls things. Host input mapping, rider facing
/// and the client camera all follow it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatRole {
    /// Turns freely in the seat and rides along.
    Passenger,
    /// Drives with the strafe keys steering; the mouse looks around.
    StrafeDriver,
    /// Drives with the mouse steering and pitching the vehicle.
    MouseDriver,
    /// Controls a player-type mount (horse, rowboat, cannon, turret): the
    /// mount faces where the rider looks.
    Actor,
    /// Aims an attached turret; the view turns with the hull.
    Gunner,
}
impl Definition {
    /// Families built from PlayerData: they move like players, not rigid bodies.
    pub fn is_actor(&self) -> bool {
        matches!(
            self.family,
            Family::Horse | Family::Rowboat | Family::Cannon | Family::Turret
        )
    }
    pub fn seat_role(&self, seat: usize) -> SeatRole {
        self.seat_role_for(seat, true)
    }
    /// The seat's role for a rider whose `$pref::Input::UseStrafeSteering`
    /// is `strafe_steering`: off, a strafe-steered vehicle's driver steers
    /// with the mouse (`amIStrafeSteering`, blocklandv20.exe 0x4d8010).
    pub fn seat_role_for(&self, seat: usize, strafe_steering: bool) -> SeatRole {
        let Some(s) = self.seats.get(seat) else {
            return SeatRole::Passenger;
        };
        if self.is_actor() && (s.controls || s.weapon) {
            SeatRole::Actor
        } else if s.weapon {
            SeatRole::Gunner
        } else if !s.controls {
            SeatRole::Passenger
        } else if self.strafe_steering && strafe_steering {
            SeatRole::StrafeDriver
        } else {
            SeatRole::MouseDriver
        }
    }
}
impl Pack {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        ensure!(
            std::fs::metadata(path)?.len() < 16 * 1024 * 1024,
            "vehicle pack too large"
        );
        let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        upgrade(&mut value)?;
        let mut pack: Self = serde_json::from_value(value)?;
        pack.validate()?;
        pack.attach_muzzle_tracks(path.parent().unwrap_or(Path::new(".")))?;
        Ok(pack)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA_VERSION,
            "unknown vehicle schema"
        );
        ensure!(self.definitions.len() <= 128, "too many vehicles");
        ensure!(self.assets.len() <= 4096, "too many assets");
        let mut ids = std::collections::BTreeSet::new();
        for d in &self.definitions {
            ensure!(
                ids.insert(&d.id)
                    && (d.id.starts_with("v20.vehicle.")
                        || d.id
                            .split_once(':')
                            .is_some_and(|(_, rest)| rest.starts_with("vehicle/"))),
                "duplicate/invalid vehicle identity"
            );
            ensure!(
                d.mass.is_finite() && d.mass > 0. && d.max_damage.is_finite() && d.max_damage > 0.,
                "invalid vehicle mass/damage"
            );
            ensure!(
                d.seats.len() <= 32 && d.wheels.len() <= 16,
                "too many seats/wheels"
            );
            ensure!(
                !d.collision_hulls.is_empty() && d.collision_hulls.len() <= 32,
                "missing/excessive collision geometry"
            );
            ensure!(
                [
                    d.energy.maximum,
                    d.energy.minimum_jet,
                    d.energy.drain_per_32ms,
                    d.energy.recharge_per_32ms,
                    d.energy.jet_force
                ]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.),
                "invalid energy parameters"
            );
            ensure!(
                d.flight.is_some() == (d.family == Family::Flying),
                "flying controller configuration mismatch"
            );
            if let Some(f) = &d.wheeled_flight {
                ensure!(
                    matches!(d.family, Family::Wheeled | Family::Skis)
                        && [
                            f.max_forward_vel,
                            f.max_reverse_vel,
                            f.horizontal_surface_force,
                            f.vertical_surface_force,
                            f.stall_speed,
                        ]
                        .iter()
                        .all(|v| v.is_finite() && *v >= 0.),
                    "invalid wheeled flight"
                );
            }
            let st = &d.steering;
            ensure!(
                [
                    st.strafe_rate,
                    st.auto_return_rate,
                    st.auto_return_max_speed
                ]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.),
                "invalid steering"
            );
            ensure!(d.threads.len() <= 16, "too many animation threads");
            for t in &d.threads {
                ensure!(
                    t.slot < 4
                        && !t.sequence.trim().is_empty()
                        && t.sequence.len() <= 64
                        && t.rate.is_finite()
                        && t.rate.abs() <= 100.
                        && [t.min_speed, t.max_speed]
                            .iter()
                            .flatten()
                            .all(|v| v.is_finite() && *v >= 0.),
                    "invalid animation thread"
                );
            }
            if let Some(f) = &d.flight {
                ensure!(
                    [
                        f.hover_height,
                        f.create_hover_height,
                        f.min_drag,
                        f.max_auto_speed,
                        f.auto_linear_force,
                        f.auto_angular_force,
                        f.auto_input_damping,
                        f.horizontal_surface_force,
                        f.vertical_surface_force,
                        f.steering_force,
                        f.steering_roll_force,
                        f.vertical_thrust_multiple
                    ]
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.),
                    "invalid flight parameters"
                );
                ensure!(
                    f.auto_input_damping <= 1. && f.min_drag > 0.,
                    "invalid flight damping"
                );
            }
            let scalars = [
                d.density,
                d.drag,
                d.friction,
                d.restitution,
                d.engine_force,
                d.engine_brake,
                d.brake_force,
                d.max_speed,
                d.reverse_speed,
                d.mount_distance,
                d.max_steering,
                d.thrust,
                d.reverse_thrust,
                d.lift,
                d.yaw_force,
                d.pitch_force,
                d.roll_force,
                d.angular_drag,
                d.jump_speed,
                d.max_side_speed,
                d.run_surface_angle,
                d.impact_threshold,
                d.impact_damage,
                d.runover_speed,
                d.runover_damage,
                d.runover_push,
            ];
            ensure!(
                scalars.iter().all(|v| v.is_finite() && *v >= 0.),
                "invalid physics coefficient"
            );
            ensure!(
                d.density > 0. && d.burn_ticks < 120 * 3600 && d.invulnerable_ticks < 120 * 3600,
                "invalid timing/density"
            );
            ensure!(
                d.bounds_min
                    .iter()
                    .chain(d.bounds_max.iter())
                    .all(|v| v.is_finite()),
                "invalid bounds"
            );
            ensure!(
                d.mass_center.iter().all(|v| v.is_finite())
                    && d.inertia_box.iter().all(|v| v.is_finite() && *v > 0.),
                "invalid mass properties"
            );
            ensure!(
                d.look_pitch
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= std::f32::consts::FRAC_PI_2 + 0.001)
                    && d.look_pitch[0] <= d.look_pitch[1]
                    && d.underwater_speeds
                        .iter()
                        .all(|v| v.is_finite() && *v >= 0.),
                "invalid actor look/swim parameters"
            );
            let c = &d.camera;
            ensure!(
                [c.max_dist, c.offset, c.tilt, c.lag, c.decay]
                    .iter()
                    .all(|v| v.is_finite())
                    && (0. ..=100.).contains(&c.max_dist)
                    && c.tilt.abs() <= std::f32::consts::FRAC_PI_2
                    && c.lag >= 0.
                    && c.decay >= 0.
                    && d.look_limits.iter().all(|v| (0. ..=1.).contains(v))
                    && d.look_limits[0] <= d.look_limits[1],
                "invalid camera or look limits"
            );
            for wheel in &d.wheels {
                ensure!(
                    wheel.position.iter().all(|v| v.is_finite())
                        && [wheel.radius, wheel.rest_length, wheel.spring, wheel.damping]
                            .iter()
                            .all(|v| v.is_finite() && *v > 0.)
                        && wheel.friction.is_finite()
                        && wheel.friction >= 0.
                        && wheel.steering.is_finite()
                        && (glam::Quat::from_array(wheel.model_rotation).length() - 1.).abs()
                            < 0.001,
                    "invalid wheel"
                );
            }
            if let Some(w) = &d.weapon {
                ensure!(
                    w.cooldown_ticks > 0
                        && w.speed.is_finite()
                        && w.speed > 0.
                        && w.charge_steps > 0
                        && w.charge_steps <= 64,
                    "invalid weapon"
                );
            }
            ensure!(
                d.wheels.is_empty() || d.max_speed > 0.,
                "wheeled vehicle needs positive max speed"
            );
            ensure!(
                d.attachment_mount.is_none()
                    || (!d.attachment_collision_hulls.is_empty()
                        && d.attachment_fallback_seat.is_some()
                        && d.seats.len() > 2),
                "incomplete attachment"
            );
            ensure!(
                d.attachment_collision_hulls.len() <= 32,
                "too many attachment hulls"
            );
            for hull in d
                .collision_hulls
                .iter()
                .chain(d.attachment_collision_hulls.iter())
            {
                ensure!(
                    hull.len() >= 4
                        && hull.len() <= 65536
                        && hull.iter().flatten().all(|n| n.is_finite()),
                    "invalid hull"
                );
            }
            for seat in &d.seats {
                ensure!(
                    seat.transform
                        .position
                        .iter()
                        .chain(seat.transform.rotation.iter())
                        .all(|x| x.is_finite()),
                    "invalid seat transform"
                );
            }
        }
        Ok(())
    }
    pub fn verify_assets(&self, root: impl AsRef<Path>) -> Result<()> {
        for asset in &self.assets {
            let p = Path::new(&asset.path);
            ensure!(
                !p.is_absolute()
                    && p.components()
                        .all(|c| matches!(c, std::path::Component::Normal(_))),
                "unsafe asset path"
            );
            ensure!(
                std::fs::metadata(root.as_ref().join(p))?.len() < 64 * 1024 * 1024,
                "asset too large"
            );
            let bytes = std::fs::read(root.as_ref().join(p))?;
            ensure!(
                format!("{:x}", Sha256::digest(bytes)) == asset.sha256,
                "asset hash mismatch: {}",
                asset.path
            );
        }
        Ok(())
    }
}
/// Upgrades an older pack to this schema in place. Schema 5 kept the
/// steering and wheeled-flight fields only in `authored` and marked flying
/// wheeled vehicles with a family of their own; the runtime then gave the
/// flying forces to that family and to skis.
fn upgrade(pack: &mut serde_json::Value) -> Result<()> {
    use serde_json::{Value, json};
    if pack.get("schema_version").and_then(Value::as_u64) != Some(5) {
        return Ok(());
    }
    for d in pack
        .get_mut("definitions")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let authored = d.get("authored").cloned().unwrap_or(Value::Null);
        let number = |key: &str, default: f32| {
            authored
                .get(key)
                .and_then(Value::as_str)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(default)
        };
        let flag = |key: &str, default: bool| {
            authored
                .get(key)
                .and_then(Value::as_str)
                .map_or(default, |v| !matches!(v.trim(), "0" | "false" | ""))
        };
        let family = d.get("family").and_then(Value::as_str).unwrap_or_default();
        let flies = matches!(family, "FlyingWheeled" | "Skis");
        if family == "FlyingWheeled" {
            d["family"] = json!("Wheeled");
        }
        d["steering"] = serde_json::to_value(SteeringSettings {
            strafe_rate: number("steeringstrafesteeringrate", 0.1),
            auto_return: flag("steeringuseautoreturn", true),
            auto_return_rate: number("steeringautoreturnrate", 0.9),
            auto_return_max_speed: number("steeringautoreturnmaxspeed", 10.),
        })?;
        d["wheeled_flight"] = if flies {
            serde_json::to_value(WheeledFlightSettings {
                max_forward_vel: number("maxforwardvel", 0.),
                max_reverse_vel: number("maxreversevel", 0.),
                horizontal_surface_force: number("horizontalsurfaceforce", 0.),
                vertical_surface_force: number("verticalsurfaceforce", 0.),
                stall_speed: number("stallspeed", 0.),
                sled: flag("issled", false),
            })?
        } else {
            Value::Null
        };
    }
    pack["schema_version"] = json!(SCHEMA_VERSION);
    Ok(())
}
