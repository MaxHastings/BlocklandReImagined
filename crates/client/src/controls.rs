//! Client intentions and view angles; authoritative simulation owns positions.
use bri_console::Clamp;
use bri_sim::{
    player::{MAX_FREELOOK, MoveInput, PlayerState},
    session::ControlObject,
};
use bri_ui::api::{GameAction, HeldControl};
use bri_world::OwnerId;
use std::{
    collections::{BTreeMap, BTreeSet},
    f32::consts::{FRAC_PI_2, PI},
};

/// `HumanController::ViewShake`: the look moves along a direction at `rate`
/// radians a second; at `max` it turns back at half the speed and stops
/// when it is down to `min`, leaving the aim off by that much.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewShake {
    pitch: f32,
    yaw: f32,
    offset: f32,
    min: f32,
    max: f32,
    rate: f32,
}
impl ViewShake {
    /// Step by `seconds`; the pitch and yaw to add, or `None` once it stops.
    pub fn advance(&mut self, seconds: f32) -> Option<(f32, f32)> {
        if self.rate == 0.0 {
            return None;
        }
        let delta = seconds * self.rate;
        self.offset += delta;
        let turn = (delta * self.pitch, delta * self.yaw);
        if self.rate > 0.0 {
            if self.offset >= self.max {
                self.rate *= -0.5;
            }
        } else if self.offset <= self.min {
            self.rate = 0.0;
        }
        Some(turn)
    }
}

/// A rule's camera shake (`camera_shake`): the view alone jittered, up to
/// `translation` units and `rotation` radians, fading over `seconds`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraShake {
    translation: f32,
    rotation: f32,
    seconds: f32,
    hold: f32,
    left: f32,
}
impl CameraShake {
    /// The share of the shake showing: `1 - (1 - left / seconds)^hold`.
    fn share(&self) -> f32 {
        1.0 - (1.0 - (self.left / self.seconds).clamped(0.0, 1.0)).powf(self.hold)
    }
}
/// The camera shakes' jitter this frame, in the view's own frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraJolt {
    /// Right, up.
    pub offset: [f32; 2],
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
}

#[derive(Clone, Default)]
pub struct Controls {
    /// Rules' camera shakes running, and the jitter they give this frame.
    camera_shakes: Vec<CameraShake>,
    camera_jolt: CameraJolt,
    jolt_state: u32,
    /// The corpse camera holds still on the body (a mini-game's
    /// `death_view_ms` after death): the mouse does not turn it.
    pub death_view: bool,
    held: BTreeSet<HeldControl>,
    pub yaw: f32,
    pub pitch: f32,
    /// Held free look turns the head, not the body (`mHead.z`).
    free_yaw: f32,
    pub third_person: bool,
    /// [`Self::set_first_person_only`].
    first_person_only: bool,
    /// `$pref::Player::defaultFov`; `None` is v20's 90.
    normal_fov: Option<f32>,
    /// The host's `setControlCameraFov` (an Add-On's rules), in place of
    /// `normal_fov` until the host hands it back.
    server_fov: Option<f32>,
    /// A rule's view shake in progress ([`Self::start_view_shake`]).
    view_shake: Option<ViewShake>,
    /// Seconds of breathing since the game began ([`Self::advance_breath`]).
    breath_clock: f32,
    /// Whether the breath cycle was past zero (breathing out) when last seen
    /// while aiming.
    breath_out: Option<bool>,
    /// The body's archetype is `thirdPersonOnly`.
    third_person_only: bool,
    /// Target zoom FOV (`$Pref::player::CurrentFOV`); the wheel steps it.
    zoom_fov: Option<f32>,
    /// The FOV shown (`$cameraFov`), ramping toward the normal or zoom FOV.
    /// `None` until the first frame, which starts at its target.
    fov_shown: Option<f32>,
    /// `GameConnection::mCameraPos`: 0 in first person, 1 fully out in
    /// third person; slides between them when the view is toggled.
    camera_pos: f32,
    /// The last toggle's `$pref::Input::FastFirstThirdPerson`.
    fast_view: bool,
    /// The admin camera in control, if any. The body's `yaw`/`pitch` stay
    /// where they were left while it is active.
    observer: Option<Observer>,
    /// Driving a mouse-steered vehicle: the view follows the vehicle and
    /// the mouse only steers. `yaw`/`pitch` then carry the raw mouse turn,
    /// pitch wrapping every half turn, for the server's steering deltas.
    vehicle_view: Option<(f32, f32)>,
    /// Seated: the body sits fixed on its mount (`Player::processTick`
    /// takes the mount transform), so the view faces the seat and the
    /// mouse only tilts it; free look still turns the head.
    seat_yaw: Option<f32>,
    /// The frame the first-person view rides in this frame, if any.
    ride: Option<Ride>,
    /// `mHead.x` relative to a vehicle seat while riding one, up positive.
    head_pitch: f32,
    /// A mouse driver's head returning on v20's 32 ms ticks, which
    /// `head_pitch` shows between.
    driver_head: HeadTicks,
    /// The heading of a vehicle that steers toward its driver's look, as
    /// last seen: the head turns back by each turn the vehicle makes, so
    /// the view holds its direction in the world and the vehicle turns to
    /// meet it ([`Self::hold_world_look`]).
    look_heading: Option<f32>,
    /// Driving a vehicle whose camera orbits (`VehicleCamera::orbit`): the
    /// head's pitch stays where the camera can follow it, between minus the
    /// highest elevation and minus the lowest (radians, up positive).
    orbit_pitch: Option<(f32, f32)>,
    /// Where an orbiting camera's trailing pivot is (`CameraOrbit::lag`),
    /// with the vehicle's place it trails, both this frame.
    orbit_trail: Option<(glam::Vec3, glam::Vec3)>,
    /// The nose pitch of such a vehicle that flies, as last seen: the head
    /// tips back by each pitch the nose makes, likewise.
    look_nose: Option<f32>,
    /// A passenger's body turn on the seat (`mRot.z` relative to the mount).
    body_turn: f32,
    /// Mounted on anything (a vehicle, a player-type mount, another player).
    mounted: bool,
    /// `$pref::Input::MouseInvert` (already applied to look input) and
    /// `$Pref::Input::VehicleMouseInvert`, which replaces it while driving a
    /// mouse-steered vehicle without free look (`pitch()` in v20).
    mouse_invert: bool,
    /// `VehicleMouseInvert` turned off (stock v20 ships it on).
    vehicle_mouse_plain: bool,
    /// The held weapon's aim (`Image::zoom`), while one is held.
    aim: Option<bri_weapons::Zoom>,
    /// Eligibility is separate from the retained profile's input ownership.
    aim_state_allowed: bool,
    /// Look speed's multiple from the held image's aim assist this frame;
    /// None for full speed. Set by the frame drawn, which holds the
    /// controls only to read them.
    assist_slowdown: std::cell::Cell<Option<f32>>,
    /// The aim's wheel step (`Zoom::levels`), kept while it is held.
    aim_level: usize,
    /// The aim's sway (`Zoom::sway`), eased in while aiming and out after.
    sway: SwayState,
    /// The roll an opening in a floor or ceiling turned the view by, and
    /// how it turned the body's eye and camera pivot about its middle
    /// (upside down, for a floor onto a floor) past the turn of its
    /// heading, both easing back upright (`crate::portal_view`).
    portal_ease: Option<PortalEase>,
    /// The active body's camera (`Look::camera`) and its bob and weapon
    /// trail. The stock profile leaves every view untouched.
    body_camera: BodyCamera,
}
/// The active body's camera profile and how it is moving.
#[derive(Clone, Debug, Default)]
struct BodyCamera {
    profile: bri_content::camera::CameraProfile,
    motion: bri_content::camera::CameraMotion,
    /// The look's angles at the last advance, to measure each turn.
    last_look: Option<(f32, f32)>,
    /// Scale of the body, which the offsets follow.
    scale: f32,
}
/// The roll and tilt an opening left on the view, and how far they have
/// eased out.
#[derive(Clone, Copy, Debug)]
struct PortalEase {
    roll: f32,
    tilt: glam::Quat,
    seconds: f32,
}
impl PortalEase {
    /// The share left: all of it at first, none after
    /// [`PORTAL_EASE_SECONDS`], gently at both ends.
    fn left(&self) -> f32 {
        let t = (self.seconds / PORTAL_EASE_SECONDS).clamped(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
    }
    fn roll(&self) -> f32 {
        self.roll * self.left()
    }
    fn tilt(&self) -> glam::Quat {
        glam::Quat::IDENTITY.slerp(self.tilt, self.left())
    }
}
/// Where a swaying aim is on its figure of eight. The drift is added to
/// the look itself, so the shot goes where the scope shows and the host
/// hears nothing new: it is part of the player's own aim.
#[derive(Clone, Copy, Debug, Default)]
struct SwayState {
    /// The sway last aimed with, kept to ease out after the aim ends.
    def: Option<bri_weapons::Sway>,
    /// Rounds of the figure of eight so far.
    phase: f64,
    /// How much of the drift is applied, 0 to the stance's multiple.
    gain: f32,
    /// The drift already added to the look.
    applied: (f32, f32),
}
/// Seconds a sway takes to ease fully in or out.
const SWAY_EASE_SECONDS: f32 = 0.6;
/// A mouse driver's `mHead.x` returning after Free Look as v20 runs it:
/// in first person each 32 ms tick halves it (blocklandv20.exe 0x5aeb0b),
/// and the view shows it between the last two ticks
/// (`Player::interpolateTick`) rather than halving continuously.
#[derive(Clone, Copy, Debug, Default)]
struct HeadTicks {
    from: f32,
    to: f32,
    /// Seconds into the tick.
    phase: f32,
}
impl HeadTicks {
    fn at(pitch: f32) -> Self {
        Self {
            from: pitch,
            to: pitch,
            ..Default::default()
        }
    }
    /// Run the ticks `seconds` brings and return the pitch shown.
    fn advance(&mut self, seconds: f32, halve: bool) -> f32 {
        self.phase += seconds;
        while self.phase >= HEAD_RETURN_TICK {
            self.phase -= HEAD_RETURN_TICK;
            self.from = self.to;
            if halve {
                self.to *= 0.5;
            }
        }
        self.from + (self.to - self.from) * (self.phase / HEAD_RETURN_TICK)
    }
}
/// What a rider's first-person view turns with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ride {
    /// A vehicle seat's world rotation (a `VehicleObjectType` mount). v20's
    /// first-person view is the rider's transform, which is the seat's,
    /// turned by the head (`Player::getRenderEyeTransform`,
    /// blocklandv20.exe 0x5aafa0), so it rolls and pitches with the
    /// vehicle. How the mouse moves the head depends on the seat.
    Seat(glam::Quat, SeatLook),
    /// The rotation of the hull a gunner's turret sits on: the view is the
    /// hull's, turned by the aim relative to it and pitched by the look,
    /// which never springs back (the turret is a Player, not a vehicle).
    Hull(glam::Quat),
}
/// How the mouse moves a vehicle rider's head, from `Player::processTick`
/// (blocklandv20.exe 0x5b2c81), which splits the move only for a player
/// with a control object (+0x864, set by `Armor::onMount` for the driver;
/// `setControlObject(%obj)` on a passenger stores none, as in Torque), and
/// `Player::updateMove` (0x5ae972).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatLook {
    /// No control object: the whole move reaches the player. The mouse
    /// pitches the head freely (never returned) and turns the whole body on
    /// the seat: `updateMove` adds the turn to `mRot.z` (0x5aeacd) and a
    /// mounted player's transform is the mount node's times `rotZ(mRot.z)`
    /// (`Player::setPosition` 0x5a6bc0; Torque player.cpp `setPosition`).
    /// Free Look turns only the head (`mMount.object` set at 0x5aea73), and
    /// that turn eases back after.
    Passenger,
    /// A strafe-steered vehicle's driver (Jeep, Tank): 0x5b2d7a treats the
    /// move as free looking whenever the vehicle steers by the strafe keys,
    /// so the mouse turns and pitches the head without Free Look, and
    /// nothing returns.
    StrafeDriver,
    /// A mouse-steered vehicle's driver (Stunt Plane, Flying Wheeled Jeep,
    /// Magic Carpet, skis, and the Jeep and Tank with strafe steering off):
    /// the mouse steers; Free Look moves the head, which springs back after
    /// (0x5aeae3, a control object of `VehicleObjectType`; see
    /// [`Controls::advance_head`] for third person).
    MouseDriver,
}
/// `Player::updateMove` halves a returning head every 32 ms tick.
const HEAD_RETURN_TICK: f32 = 0.032;
/// The client's half of a replicated camera [`ControlObject`]: look and move
/// keys steer it instead of the body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Observer {
    pub mode: ObserverMode,
    pub yaw: f32,
    pub pitch: f32,
    /// How far out an orbit sits: 8 for the spy and corpse cameras
    /// (`Observer::setMode("Corpse")`'s `setOrbitMode(..., 0, 8, 8)`), an
    /// Add-On's own for its orbit, which the wheel zooms within `zoom`.
    pub distance: f32,
    /// The nearest and farthest the wheel takes an Add-On's orbit; equal
    /// for the fixed cameras.
    pub zoom: (f32, f32),
    /// The control object this camera follows, so a zoom lasts until the
    /// host hands over another.
    pub from: ControlObject,
}
/// The spy and corpse cameras' orbit distance.
pub const CORPSE_ORBIT_DISTANCE: f32 = 8.0;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObserverMode {
    /// `Observer` fly mode, flown locally from `dropCameraAtPlayer`.
    Free(glam::Vec3),
    /// `Corpse` orbit mode around a spied player, or around one's own body
    /// after death, or an Add-On's orbit around another player.
    Orbit(OwnerId),
    /// Orbit a package entity the player drives (`ControlObject::Entity`):
    /// the moves go to the entity, steered by this camera's yaw.
    Drive(u64),
    /// A rule's path camera (`ControlObject::Path`), at this point of its
    /// path; [`Controls::fly_path`] moves and turns it each frame.
    Path(glam::Vec3),
    /// A rule's orbit around a point (`ControlObject::Point`), this far out;
    /// [`Controls::orbit_point`] keeps it on the replicated point.
    Point(glam::Vec3, f32),
}
/// `setFov` only sets a target; each frame `$cameraFov` moves toward it by
/// elapsed ms / zoomSpeed * 90 degrees (blocklandv20.exe 0x58ee10).
/// init.cs passes `$pref::Player::zoomSpeed` (0) to `setZoomSpeed`, which
/// clamps it to 200 ms, so every FOV change is a linear 450 degrees/s ramp.
const ZOOM_DEGREES_PER_SECOND: f32 = 90.0 / 0.2;
/// `$cameraSpeed` from `toggleFirstPerson`: 5 camera positions per second,
/// or 1000 with `$pref::Input::FastFirstThirdPerson`.
const CAMERA_SPEED: (f32, f32) = (5.0, 1000.0);
/// v20's wheel-zoom limits (`toggleZoomFOV`'s 5 and 85).
const ZOOM_FOV_RANGE: (f32, f32) = (5.0, 85.0);
/// `$Camera::movementSpeed`, which v20's scripts set to 40 units per second.
const CAMERA_MOVEMENT_SPEED: f32 = 40.0;
/// Observer cameras stop just short of straight up or down.
const OBSERVER_PITCH: f32 = FRAC_PI_2 - 0.01;
/// How long the roll and tilt an opening left take to ease out.
const PORTAL_EASE_SECONDS: f32 = 0.5;
fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}
fn wrap_half(a: f32) -> f32 {
    (a + FRAC_PI_2).rem_euclid(PI) - FRAC_PI_2
}
/// The yaw and pitch of a view looking along `forward` with `up` up.
/// Looking straight up or down, the yaw is the one whose level basis
/// (`App::view_basis`) has `up` for its up.
pub fn angles(forward: glam::Vec3, up: glam::Vec3) -> (f32, f32) {
    let pitch = forward.y.clamped(-1.0, 1.0).asin();
    let yaw = if forward.x * forward.x + forward.z * forward.z > 1e-8 {
        forward.x.atan2(-forward.z)
    } else if forward.y > 0.0 {
        (-up.x).atan2(up.z)
    } else {
        up.x.atan2(-up.z)
    };
    (yaw, pitch)
}
/// The roll of a view rotation about its forward axis, relative to the
/// level basis of its yaw and pitch: the turn about the camera's own +Z
/// after its yaw and pitch, so positive tips the top of the view left.
pub fn roll(view: glam::Quat) -> f32 {
    let (forward, up) = (view * glam::Vec3::NEG_Z, view * glam::Vec3::Y);
    let (yaw, _) = angles(forward, up);
    let right = glam::Vec3::new(yaw.cos(), 0.0, yaw.sin());
    let level_up = right.cross(forward);
    (-up.dot(right)).atan2(up.dot(level_up))
}
/// The camera's frame in the world for a view at `eye` with these angles
/// ([`angles`] and [`roll`] taken apart again): -Z forward, +Y up.
pub fn view_frame(eye: glam::Vec3, yaw: f32, pitch: f32, roll: f32) -> Option<glam::Mat4> {
    let rotation = glam::Quat::from_rotation_y(-yaw)
        * glam::Quat::from_rotation_x(pitch)
        * glam::Quat::from_rotation_z(roll);
    let frame = glam::Mat4::from_rotation_translation(rotation, eye);
    frame.is_finite().then_some(frame)
}
fn valid(q: glam::Quat) -> Option<glam::Quat> {
    (q.is_finite() && q.length_squared() > 0.5).then(|| q.normalize())
}
impl Controls {
    pub fn held(&self, c: HeldControl) -> bool {
        self.held.contains(&c)
    }
    /// Let go of `c` as if its key came up (an Add-On took it over).
    pub fn let_go(&mut self, c: HeldControl) {
        self.held.remove(&c);
    }
    pub fn release(&mut self) {
        self.held.clear();
        self.free_yaw = 0.0;
    }
    /// Returns true only for locally handled control actions.
    pub fn action(&mut self, action: &GameAction) -> bool {
        match *action {
            GameAction::Held { control, down } => {
                if down {
                    self.held.insert(control);
                } else {
                    self.held.remove(&control);
                }
                // Letting go, the head eases back (`advance_head`).
            }
            GameAction::Look { yaw, pitch } => {
                if !yaw.is_finite() || !pitch.is_finite() {
                    return true;
                }
                // `getMouseAdjustAmount`: sensitivity × `$cameraFov` / 90,
                // then the aim's own `sensitivity` while aiming.
                // An aim assist slows it while the aim is on a body.
                let scale = self.fov() / 90.0
                    * self.aim_sensitivity()
                    * self.assist_slowdown.get().unwrap_or(1.0);
                // OS mouse Y increases downwards; simulation pitch increases up.
                self.look(yaw * scale, -pitch * scale);
            }
            GameAction::SetZoomFov { fov } if fov.is_finite() => {
                self.zoom_fov = Some(fov.clamped(ZOOM_FOV_RANGE.0, ZOOM_FOV_RANGE.1))
            }
            GameAction::ToggleFirstPerson { fast } => {
                self.third_person = !self.third_person;
                self.fast_view = fast;
            }
            _ => return false,
        }
        true
    }
    fn look(&mut self, yaw: f32, pitch: f32) {
        if let Some(observer) = &mut self.observer {
            if self.death_view && observer.from == ControlObject::Corpse {
                return;
            }
            observer.yaw = wrap(observer.yaw + yaw);
            observer.pitch = (observer.pitch + pitch).clamped(-OBSERVER_PITCH, OBSERVER_PITCH);
        } else if let Some(look) = self.seat_look()
            && (look == SeatLook::StrafeDriver || self.held(HeldControl::FreeLook))
        {
            // Free looking hands the rider the whole turn and the vehicle
            // none (`Player::processTick` 0x5b2df7 zeroes the vehicle's yaw
            // and pitch), and `pitch()` goes back to Invert Mouse.
            self.free_yaw = (self.free_yaw + yaw).clamped(-MAX_FREELOOK, MAX_FREELOOK);
            let (low, high) = self
                .orbit_pitch
                .filter(|_| self.driver_head_yaw().is_some())
                .unwrap_or((-FRAC_PI_2, FRAC_PI_2));
            self.head_pitch = (self.head_pitch + pitch).clamped(low, high);
            self.driver_head = HeadTicks::at(self.head_pitch);
        } else if self.free_looking() {
            // Only the turn is free; pitch still tilts the body's look
            // (`Player::updateMove` always adds pitch to `mHead.x`).
            self.free_yaw = (self.free_yaw + yaw).clamped(-MAX_FREELOOK, MAX_FREELOOK);
            self.pitch = (self.pitch + pitch).clamped(-FRAC_PI_2, FRAC_PI_2);
        } else if self.vehicle_view.is_some() {
            // v20's `pitch()`: Vehicle Mouse Invert replaces Invert Mouse
            // (which the UI already applied). The steering is Torque's
            // `move->pitch`, positive with the mouse up when inverted (the
            // default), which dips the nose; so the look is kept as it came
            // when the two prefs agree with that, and flipped otherwise.
            let pitch = if self.mouse_invert == self.vehicle_mouse_plain {
                pitch
            } else {
                -pitch
            };
            // Only the vehicle steers by the move; the head stays on the
            // seat, so the view turns with the vehicle as drawn and never
            // ahead of it (Torque's `Player::processTick` hands the rider
            // a null move unless free looking). blocklandv20.exe also
            // copies the move's pitch to the rider (0x5b2cd4), which tipped
            // the first-person view one tick ahead of the nose each mouse
            // move: Max, v0.1.7, "my camera moves first and then the plane
            // takes a second to catch up". Left out on purpose; see
            // `docs/audits/vehicles-v20-checklist.md`.
            self.yaw = wrap(self.yaw + yaw);
            self.pitch = wrap_half(self.pitch + pitch);
        } else if self.seat_look() == Some(SeatLook::Passenger) {
            self.body_turn = wrap(self.body_turn + yaw);
            self.head_pitch = (self.head_pitch + pitch).clamped(-FRAC_PI_2, FRAC_PI_2);
        } else if self.seated() {
            self.head_pitch = (self.head_pitch + pitch).clamped(-FRAC_PI_2, FRAC_PI_2);
        } else if self.seat_yaw.is_some() {
            self.pitch = (self.pitch + pitch).clamped(-FRAC_PI_2, FRAC_PI_2);
        } else {
            self.yaw = wrap(self.yaw + yaw);
            self.pitch = (self.pitch + pitch).clamped(-FRAC_PI_2, FRAC_PI_2);
        }
    }
    /// Follow a mouse-steered vehicle's heading and pitch, or stop. Leaving
    /// faces the body where the vehicle was heading.
    pub fn set_vehicle_view(&mut self, view: Option<(f32, f32)>) {
        if view.is_none()
            && let Some((yaw, _)) = self.vehicle_view
        {
            self.yaw = wrap(yaw);
            self.pitch = 0.0;
        }
        self.vehicle_view = view;
    }
    pub fn set_invert_prefs(&mut self, mouse: bool, vehicle: bool) {
        self.mouse_invert = mouse;
        self.vehicle_mouse_plain = !vehicle;
    }
    /// Face the seat, or stop. Leaving keeps facing where the seat faced.
    pub fn set_seat_yaw(&mut self, yaw: Option<f32>) {
        if let Some(yaw) = yaw.filter(|y| y.is_finite()) {
            self.yaw = wrap(yaw);
        }
        self.seat_yaw = yaw.filter(|y| y.is_finite());
    }
    /// The frame the first-person view rides this frame, or `None`.
    /// Boarding a vehicle seat starts with the head facing it.
    pub fn set_ride(&mut self, ride: Option<Ride>) {
        let ride = ride.and_then(|r| match r {
            Ride::Seat(q, look) => valid(q).map(|q| Ride::Seat(q, look)),
            Ride::Hull(q) => valid(q).map(Ride::Hull),
        });
        let seat = |r: &Option<Ride>| match r {
            Some(Ride::Seat(_, look)) => Some(*look),
            _ => None,
        };
        if seat(&ride) != seat(&self.ride) {
            // Stepping off keeps the head's pitch (`mHead.x` is the body's).
            if self.seated() && ride.is_none() {
                self.pitch = self.head_pitch;
            }
            self.head_pitch = 0.0;
            self.driver_head = HeadTicks::default();
            self.body_turn = 0.0;
            self.free_yaw = 0.0;
            self.look_heading = None;
            self.look_nose = None;
        }
        self.ride = ride;
    }
    /// Whether the player sits on anything; free look then works in first
    /// person too.
    pub fn set_mounted(&mut self, mounted: bool) {
        self.mounted = mounted;
    }
    fn seated(&self) -> bool {
        self.seat_look().is_some()
    }
    /// Free Look turns the head only while mounted or in third person
    /// (`Player::updateMove` 0x5aea5f: `mMount.object` set, or not
    /// `isFirstPerson`); on foot in first person the mouse turns the body.
    fn free_looking(&self) -> bool {
        self.held(HeldControl::FreeLook) && (self.mounted || self.camera_pos != 0.0)
    }
    /// What the rider is doing, for control profile layers: (driving, steering
    /// with the mouse). None when not riding anything.
    pub fn ride_context(&self) -> Option<(bool, bool)> {
        match self.ride {
            Some(Ride::Seat(_, SeatLook::Passenger)) => Some((false, false)),
            Some(Ride::Seat(_, SeatLook::StrafeDriver)) => Some((true, false)),
            Some(Ride::Seat(_, SeatLook::MouseDriver)) => Some((true, true)),
            Some(Ride::Hull(_)) => Some((false, false)),
            None => None,
        }
    }
    fn seat_look(&self) -> Option<SeatLook> {
        match self.ride {
            Some(Ride::Seat(_, look)) => Some(look),
            _ => None,
        }
    }
    /// Ease a seated head back as v20 does, halving it every 32 ms tick
    /// (`Player::updateMove`, blocklandv20.exe 0x5aeae3): without Free Look
    /// a passenger's turn returns, and so do a mouse driver's turn and
    /// pitch, in first and third person alike. A strafe driver's head never
    /// returns.
    ///
    /// 0x5aeaf6 skips both halvings for a vehicle's driver when the
    /// controlling connection is not `isFirstPerson` (0x526d60, `mCameraPos`
    /// at +0x364). That field only moves in `getControlCameraTransform`
    /// (0x592790), which follows the first-person global (0x77026c) of the
    /// process it runs in, so the authoritative server's copy never tracked
    /// a joined client's view, and its halved head reached the client in
    /// the control-object update. Max, v0.2.10: letting go of Z in a Jeep
    /// left the chase camera stuck where he looked, "def not how old v20
    /// felt".
    pub fn advance_head(&mut self, seconds: f32) {
        let Some(look) = self.seat_look() else {
            // Off a vehicle seat the head's turn halves every tick unless
            // it is free looking (the else branch of 0x5aea5f).
            if seconds.is_finite() && !self.free_looking() {
                self.free_yaw *= 0.5f32.powf(seconds.clamped(0.0, 1.0) / HEAD_RETURN_TICK);
            }
            return;
        };
        if !seconds.is_finite() || look == SeatLook::StrafeDriver {
            return;
        }
        if self.held(HeldControl::FreeLook) {
            // Free look moves the head directly; the ticks pick up from it.
            self.driver_head = HeadTicks::at(self.head_pitch);
            return;
        }
        let seconds = seconds.clamped(0.0, 1.0);
        let keep = 0.5f32.powf(seconds / HEAD_RETURN_TICK);
        match look {
            SeatLook::Passenger => self.free_yaw *= keep,
            SeatLook::MouseDriver => {
                self.free_yaw *= keep;
                self.head_pitch = self.driver_head.advance(seconds, true);
            }
            _ => {}
        }
    }
    /// Driving a vehicle that steers toward the look (`toward_look`), at
    /// `heading`: the head turns back by however far the vehicle turned
    /// since the last frame, holding the view's direction in the world.
    /// `None` drives anything else.
    /// A flying one also passes its `nose` pitch, which the head's pitch
    /// holds against the same way.
    pub fn hold_world_look(&mut self, heading: Option<f32>, nose: Option<f32>) {
        let heading = heading.filter(|h| h.is_finite());
        let nose = nose.filter(|n| n.is_finite() && heading.is_some());
        let driving = self.seat_look() == Some(SeatLook::StrafeDriver);
        if let (Some(now), Some(before)) = (heading, self.look_heading)
            && driving
        {
            self.free_yaw =
                (self.free_yaw - wrap(now - before)).clamped(-MAX_FREELOOK, MAX_FREELOOK);
        }
        if let (Some(now), Some(before)) = (nose, self.look_nose)
            && driving
        {
            self.head_pitch = (self.head_pitch - (now - before)).clamped(-FRAC_PI_2, FRAC_PI_2);
            self.driver_head = HeadTicks::at(self.head_pitch);
        }
        self.look_heading = heading;
        self.look_nose = nose;
    }
    /// A passenger's body turn on the seat, radians (right positive); 0 in
    /// any other seat.
    pub fn passenger_turn(&self) -> f32 {
        if self.seat_look() == Some(SeatLook::Passenger) {
            self.body_turn
        } else {
            0.0
        }
    }
    /// The head's turn on a vehicle's driver, which v20's chase camera
    /// swings round by (`Vehicle::getCameraTransform` 0x56cc10 reads the
    /// rider's `mHead`); `None` off a driver's seat.
    pub fn driver_head_yaw(&self) -> Option<f32> {
        matches!(
            self.seat_look(),
            Some(SeatLook::StrafeDriver | SeatLook::MouseDriver)
        )
        .then_some(self.free_yaw)
    }
    /// The driver's head (turn, pitch) on the vehicle, which an orbiting
    /// camera swings round by; `None` off a driver's seat.
    pub fn driver_head(&self) -> Option<(f32, f32)> {
        self.driver_head_yaw().map(|yaw| (yaw, self.head_pitch))
    }
    /// The orbit range (lowest and highest elevation, radians) of the
    /// camera of the vehicle this player drives, or `None`: the head's
    /// pitch then stays where the camera can follow it.
    pub fn set_driver_orbit(&mut self, range: Option<(f32, f32)>) {
        self.orbit_pitch = range
            .filter(|(low, high)| low.is_finite() && high.is_finite() && low <= high)
            .map(|(low, high)| (-high, -low));
        if let Some((low, high)) = self.orbit_pitch
            && self.driver_head_yaw().is_some()
        {
            self.head_pitch = self.head_pitch.clamped(low, high);
        }
    }
    /// Trail the orbiting camera's pivot behind the driven vehicle at
    /// `position` by `lag` seconds over the last `seconds`; `None` off an
    /// orbiting driver's seat. A jump of over 20 units (a respawn, a
    /// teleport) catches up at once.
    pub fn trail_orbit(&mut self, position: Option<glam::Vec3>, lag: f32, seconds: f32) {
        let Some(position) = position.filter(|p| p.is_finite()) else {
            self.orbit_trail = None;
            return;
        };
        let trailed = match self.orbit_trail {
            Some((trailed, _))
                if lag > 0.0
                    && seconds.is_finite()
                    && trailed.distance_squared(position) < 20.0 * 20.0 =>
            {
                let keep = (-seconds.clamped(0.0, 0.25) / lag).exp();
                position + (trailed - position) * keep
            }
            _ => position,
        };
        self.orbit_trail = Some((trailed, position));
    }
    /// How far the orbiting camera's pivot trails the vehicle this frame.
    pub fn orbit_lag(&self) -> glam::Vec3 {
        self.orbit_trail
            .map_or(glam::Vec3::ZERO, |(trailed, position)| trailed - position)
    }
    /// The first-person view's rotation while riding (looking down -Z, up
    /// +Y): the ride's frame turned by the head.
    pub fn ride_view(&self) -> Option<glam::Quat> {
        use glam::{Quat, Vec3};
        Some(match self.ride? {
            Ride::Seat(seat, _) => {
                seat * Quat::from_rotation_y(-(self.body_turn + self.free_yaw))
                    * Quat::from_rotation_x(self.head_pitch)
            }
            Ride::Hull(hull) => {
                let (heading, _) = angles(hull * Vec3::NEG_Z, hull * Vec3::Y);
                hull * Quat::from_rotation_y(-wrap(self.yaw + self.free_yaw - heading))
                    * Quat::from_rotation_x(self.pitch.clamped(-FRAC_PI_2, FRAC_PI_2))
            }
        })
    }
    /// Turn the look by a rule's `kick_view`: `yaw` to the right and `pitch`
    /// up, in radians, and leave it turned. A corpse camera that holds still,
    /// a spectator's orbit and a rider's seat look are not the holder's aim
    /// and stay as they are.
    pub fn kick_view(&mut self, yaw: f32, pitch: f32) {
        if !(yaw.is_finite() && pitch.is_finite())
            || self.death_view
            || self.observer.is_some()
            || self.seat_look().is_some()
        {
            return;
        }
        self.yaw = wrap(self.yaw + yaw);
        self.pitch = (self.pitch + pitch).clamped(-FRAC_PI_2, FRAC_PI_2);
    }
    /// Take control of a turret looking along it: the look becomes the
    /// barrel's (`yaw`, `pitch`), as v20's control object hands the camera
    /// the turret's own rotation.
    pub fn take_turret(&mut self, yaw: f32, pitch: f32) {
        if yaw.is_finite() && pitch.is_finite() {
            self.yaw = wrap(yaw);
            self.pitch = pitch.clamped(-FRAC_PI_2, FRAC_PI_2);
            self.free_yaw = 0.0;
        }
    }
    /// Turn the look the whole way an opening's carry turned the body: it
    /// sees the same view from the far side, its pitch and any roll
    /// included. The body stays upright, so its eye and camera pivot end up
    /// the other side of its middle from where the carry takes them through
    /// a floor or ceiling: [`Self::portal_tilt`] puts them there. The roll
    /// and tilt then ease out (see [`Self::ease_roll`]).
    pub fn carry_look(&mut self, carry: &glam::Affine3A) {
        let look = (
            wrap(self.yaw + self.free_yaw),
            self.pitch,
            self.portal_roll(),
        );
        let (yaw, pitch, roll) = crate::portal_view::carried_look(look, carry);
        let turn = glam::Quat::from_mat3a(&carry.matrix3).normalize();
        let before = self.yaw;
        self.yaw = wrap(yaw - self.free_yaw);
        self.pitch = pitch.clamped(-FRAC_PI_2, FRAC_PI_2);
        // The eye's offset turns with the heading (`ahead` of the yaw): the
        // tilt is the rest of the carry's turn.
        let tilt = turn * self.portal_tilt() * glam::Quat::from_rotation_y(self.yaw - before);
        let tilt = if tilt.is_finite() {
            tilt.normalize()
        } else {
            glam::Quat::IDENTITY
        };
        let roll = wrap(roll);
        self.portal_ease = (roll.abs() > 1e-4 || tilt.angle_between(glam::Quat::IDENTITY) > 1e-4)
            .then_some(PortalEase {
                roll,
                tilt,
                seconds: 0.0,
            });
    }
    /// The view's roll left by openings, added to the camera's.
    pub fn portal_roll(&self) -> f32 {
        self.portal_ease.map_or(0.0, |e| e.roll())
    }
    /// The turn about the body's middle its eye and camera pivot are shown
    /// with, left by openings.
    pub fn portal_tilt(&self) -> glam::Quat {
        self.portal_ease.map_or(glam::Quat::IDENTITY, |e| e.tilt())
    }
    /// Ease the roll and tilt an opening left back upright, as Portal does.
    pub fn ease_roll(&mut self, seconds: f32) {
        if let Some(ease) = &mut self.portal_ease
            && seconds.is_finite()
        {
            ease.seconds += seconds.clamped(0.0, 0.25);
            if ease.seconds >= PORTAL_EASE_SECONDS {
                self.portal_ease = None;
            }
        }
    }
    /// Turn the view with the vehicle it rides.
    pub fn carry_yaw(&mut self, turn: f32) {
        if turn.is_finite() {
            self.yaw = wrap(self.yaw + turn);
        }
    }
    fn axis(&self, positive: HeldControl, negative: HeldControl) -> f32 {
        u8::from(self.held(positive)) as f32 - u8::from(self.held(negative)) as f32
    }
    /// Follow the server's control object for player `owner`. A newly
    /// granted camera starts at `eye`, looking where the player was looking.
    pub fn follow(&mut self, control: ControlObject, owner: OwnerId, eye: Option<glam::Vec3>) {
        let mode = match control {
            ControlObject::Player => {
                // Back from a camera: fire held on the camera was never
                // passed to the body, so its release may not reach here
                // either. This runs every frame, so only on that change:
                // on foot, the trigger held stays held.
                if self.observer.take().is_some() {
                    self.held.remove(&HeldControl::Fire);
                }
                return;
            }
            ControlObject::Camera | ControlObject::Observer => match self.observer {
                Some(Observer {
                    mode: ObserverMode::Free(_),
                    ..
                }) => return,
                _ => match eye {
                    Some(eye) => ObserverMode::Free(eye),
                    None => return,
                },
            },
            ControlObject::Spy(target) => ObserverMode::Orbit(target),
            ControlObject::Orbit { target, .. } => ObserverMode::Orbit(target),
            ControlObject::Corpse => ObserverMode::Orbit(owner),
            ControlObject::Entity(entity) => ObserverMode::Drive(entity),
            ControlObject::Path => match self.observer {
                Some(Observer {
                    mode: ObserverMode::Path(_),
                    ..
                }) => return,
                _ => ObserverMode::Path(eye.unwrap_or_default()),
            },
            ControlObject::Point => match self.observer {
                Some(Observer {
                    mode: ObserverMode::Point(..),
                    ..
                }) => return,
                _ => ObserverMode::Point(eye.unwrap_or_default(), 8.0),
            },
        };
        let (distance, zoom) = match control {
            ControlObject::Orbit {
                min, max, distance, ..
            } => (f32::from(distance), (f32::from(min), f32::from(max))),
            _ => (
                CORPSE_ORBIT_DISTANCE,
                (CORPSE_ORBIT_DISTANCE, CORPSE_ORBIT_DISTANCE),
            ),
        };
        if let Some(observer) = &mut self.observer {
            observer.mode = mode;
            if observer.from != control {
                observer.from = control;
                observer.distance = distance;
                observer.zoom = zoom;
            }
        } else {
            let (yaw, pitch) = self.view_angles();
            self.free_yaw = 0.0;
            self.observer = Some(Observer {
                mode,
                yaw,
                pitch: pitch.clamped(-OBSERVER_PITCH, OBSERVER_PITCH),
                distance,
                zoom,
                from: control,
            });
        }
    }
    pub fn observer(&self) -> Option<Observer> {
        self.observer
    }
    /// An orbit the wheel can zoom: an Add-On's, with room between its
    /// nearest and farthest.
    pub fn orbit_zooms(&self) -> bool {
        self.observer.is_some_and(|o| o.zoom.0 < o.zoom.1)
    }
    /// The wheel on a zooming orbit: a unit a notch, rolled forward
    /// (positive) closer, within its nearest and farthest.
    pub fn zoom_orbit(&mut self, notches: i32) {
        if let Some(o) = &mut self.observer
            && o.zoom.0 < o.zoom.1
        {
            o.distance = (o.distance - notches as f32).clamped(o.zoom.0, o.zoom.1);
        }
    }
    /// `dropCameraAtPlayer` again while flying: back to the player's eye.
    pub fn redrop_camera(&mut self, eye: glam::Vec3) {
        if let Some(observer) = &mut self.observer
            && let ObserverMode::Free(position) = &mut observer.mode
        {
            *position = eye;
        }
    }
    pub fn clear_observer(&mut self) {
        self.observer = None;
    }
    pub fn free_camera(&self) -> Option<glam::Vec3> {
        match self.observer?.mode {
            ObserverMode::Free(position) => Some(position),
            ObserverMode::Orbit(_)
            | ObserverMode::Drive(_)
            | ObserverMode::Path(_)
            | ObserverMode::Point(..) => None,
        }
    }
    /// The orbited player's presented eye, or the driven entity's head,
    /// which the orbit camera circles.
    pub fn orbit_focus(
        &self,
        presented: &BTreeMap<OwnerId, PlayerState>,
        seated: &dyn Fn(OwnerId) -> bool,
        archetypes: &bri_sim::archetype::Archetypes,
        entities: &BTreeMap<u64, bri_sim::session::EntityInfo>,
    ) -> Option<glam::Vec3> {
        match self.observer?.mode {
            ObserverMode::Orbit(target) => presented
                .get(&target)
                .map(|p| archetypes.posed_eye(p, seated(target))),
            ObserverMode::Drive(entity) => entities
                .get(&entity)
                .map(|e| glam::Vec3::from(e.position) + glam::Vec3::Y * 1.5),
            ObserverMode::Point(at, _) => Some(at),
            ObserverMode::Free(_) | ObserverMode::Path(_) => None,
        }
    }
    /// How far the orbit camera sits from its focus: a rule's point camera
    /// says, as an Add-On's orbit does (zoomed by the wheel);
    /// `Observer::setMode("Corpse")` orbits 8 units out.
    pub fn orbit_distance(&self) -> f32 {
        match self.observer {
            Some(Observer {
                mode: ObserverMode::Point(_, distance),
                ..
            }) => distance,
            Some(observer) => observer.distance,
            None => CORPSE_ORBIT_DISTANCE,
        }
    }
    /// Keep a rule's point camera on its replicated point.
    pub fn orbit_point(&mut self, point: bri_sim::session::OrbitPoint) {
        if let Some(observer) = &mut self.observer
            && let ObserverMode::Point(at, distance) = &mut observer.mode
        {
            *at = glam::Vec3::from(point.at);
            *distance = point.distance;
        }
    }
    /// Put the path camera where its path is now: the look keys do not turn
    /// it, as a `PathCamera` in control ignores the mouse.
    pub fn fly_path(&mut self, view: bri_sim::session::CameraView) {
        if let Some(observer) = &mut self.observer
            && let ObserverMode::Path(position) = &mut observer.mode
        {
            *position = view.eye();
            observer.yaw = view.yaw;
            observer.pitch = view.pitch;
        }
    }
    /// Fly the free camera with the movement keys, through any opening of
    /// `passages` it flies into as a body goes (turned with it, level).
    pub fn fly(&mut self, seconds: f32, passages: &bri_content::passage::Passages) {
        let Some(Observer {
            mode: ObserverMode::Free(mut position),
            yaw,
            pitch,
            ..
        }) = self.observer
        else {
            return;
        };
        if !seconds.is_finite() {
            return;
        }
        let forward = glam::Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        let right = glam::Vec3::new(yaw.cos(), 0.0, yaw.sin());
        // v20 binds no `moveup`/`movedown`, so the camera only flies along
        // its view and strafe axes; each axis is scaled on its own, so a
        // diagonal is faster, as in Torque.
        let walk = if self.held(HeldControl::Walk) {
            0.4
        } else {
            1.0
        };
        let direction = (forward * self.axis(HeldControl::Forward, HeldControl::Backward)
            + right * self.axis(HeldControl::Right, HeldControl::Left))
            * walk;
        let from = position;
        position += direction * self.fly_speed() * seconds.clamped(0.0, 0.1);
        let (position, carry) = match passages.is_empty() {
            true => (position, None),
            false => passages.travel(from, position),
        };
        if let Some(observer) = &mut self.observer {
            observer.mode = ObserverMode::Free(position);
            if let Some(carry) = carry {
                let (yaw, pitch, _) =
                    crate::portal_view::carried_look((observer.yaw, observer.pitch, 0.0), &carry);
                observer.yaw = yaw;
                observer.pitch = pitch.clamped(-OBSERVER_PITCH, OBSERVER_PITCH);
            }
        }
    }
    /// Free-camera speed in units per second (`Camera::processTick` fly
    /// mode, blocklandv20.exe 0x588514): `$Camera::movementSpeed` doubled
    /// while fire (trigger 0) is held, else quartered while crouch
    /// (trigger 3) is held. Trigger 1 would halve it, but v20 binds
    /// `altTrigger` to nothing; right click is jet, which the camera ignores.
    pub fn fly_speed(&self) -> f32 {
        let scale = if self.held(HeldControl::Fire) {
            2.0
        } else if self.held(HeldControl::Crouch) {
            0.25
        } else {
            1.0
        };
        CAMERA_MOVEMENT_SPEED * scale
    }
    /// The body's move: the held controls, unless a camera has control, when
    /// the body stands still with the aim it was left with.
    pub fn movement(&self) -> MoveInput {
        let (yaw, pitch) = match self.observer {
            // A driven entity takes the held controls, steered by the camera.
            Some(Observer {
                mode: ObserverMode::Drive(_),
                yaw,
                pitch,
                ..
            }) => (yaw, pitch),
            Some(_) => {
                return MoveInput {
                    yaw: self.yaw,
                    pitch: self.pitch,
                    ..Default::default()
                };
            }
            // A seated rider's body looks along the seat tilted by the
            // head's pitch, and `head_yaw` carries the free-look turn; a
            // mouse driver's yaw and pitch carry the steering.
            None => match self.ride {
                // A passenger sends the body's turn relative to the seat,
                // which the host adds to the seat's heading (`mRot.z`).
                Some(Ride::Seat(_, SeatLook::Passenger)) => (self.body_turn, self.head_pitch),
                Some(Ride::Seat(seat, _)) if self.vehicle_view.is_none() => {
                    let look = seat * glam::Quat::from_rotation_x(self.head_pitch);
                    angles(look * glam::Vec3::NEG_Z, look * glam::Vec3::Y)
                }
                _ => (self.yaw, self.pitch),
            },
        };
        let walk = if self.held(HeldControl::Walk) {
            0.4
        } else {
            1.0
        };
        MoveInput {
            seen: 0,
            forward: self.axis(HeldControl::Forward, HeldControl::Backward) * walk,
            right: self.axis(HeldControl::Right, HeldControl::Left) * walk,
            yaw,
            pitch,
            head_yaw: self.free_yaw,
            jump: self.held(HeldControl::Jump),
            crouch: self.held(HeldControl::Crouch),
            // A scope on the right mouse button may take jet for itself
            // (`Zoom::jets`).
            jet: self.held(HeldControl::Jet)
                && self.aim.as_ref().is_none_or(|a| !a.on_jet || a.jets),
            // A movement controller's own move button (SM64's B); items
            // still fire from their trigger messages.
            attack: self.held(HeldControl::Fire),
        }
    }
    /// The pitch the body's look pose (the arms' `look` thread) shows: a
    /// seated rider's head relative to the seat, which a mouse driver's
    /// steering never moves; otherwise the body's own look.
    pub fn body_pitch(&self) -> f32 {
        if self.seated() {
            self.head_pitch
        } else {
            self.pitch
        }
    }
    /// The body's head and eye direction, including held free-look.
    pub fn view_angles(&self) -> (f32, f32) {
        if let Some(view) = self.ride_view() {
            return angles(view * glam::Vec3::NEG_Z, view * glam::Vec3::Y);
        }
        match self.vehicle_view {
            Some((yaw, pitch)) => (wrap(yaw + self.free_yaw), pitch),
            None => (wrap(self.yaw + self.free_yaw), self.pitch),
        }
    }
    /// The head's free-look turn while Free Look is held (`$mvFreeLook`).
    pub fn free_look(&self) -> Option<f32> {
        self.held(HeldControl::FreeLook).then_some(self.free_yaw)
    }
    /// The look of a rider controlling a player-type mount (horse, rowboat,
    /// cannon): the mount's drawn heading turned by the head, as
    /// `Player::getCameraTransform` (blocklandv20.exe 0x5ab7d0) builds it
    /// from the control object's render transform. The mouse turns the
    /// mount, so the view turns with the mount as drawn, never ahead of it.
    pub fn mount_look(&self, mount: glam::Quat) -> (f32, f32) {
        let forward = mount * glam::Vec3::NEG_Z;
        (
            wrap(forward.x.atan2(-forward.z) + self.free_yaw),
            self.pitch,
        )
    }
    /// Where the rendered camera looks: the observer's own angles while a
    /// camera has control.
    pub fn camera_angles(&self) -> (f32, f32) {
        self.observer
            .map_or_else(|| self.view_angles(), |o| (o.yaw, o.pitch))
    }
    /// The saved FOV prefs: the normal FOV, and the zoom FOV to start from
    /// until the wheel changes it.
    pub fn set_fov_prefs(&mut self, normal: f32, zoom: f32) {
        self.normal_fov = normal.is_finite().then(|| normal.clamped(5.0, 140.0));
        if self.zoom_fov.is_none() && zoom.is_finite() {
            self.zoom_fov = Some(zoom.clamped(ZOOM_FOV_RANGE.0, ZOOM_FOV_RANGE.1));
        }
    }
    /// The field of view the host sets, or `None` for the player's own. It
    /// glides like any other FOV change.
    /// A rule shakes the view alone (`camera_shake`); up to 8 run at once.
    pub fn start_camera_shake(&mut self, translation: f32, rotation: f32, seconds: f32, hold: f32) {
        if [translation, rotation, seconds, hold]
            .iter()
            .all(|v| v.is_finite())
            && seconds > 0.0
            && self.camera_shakes.len() < 8
        {
            self.camera_shakes.push(CameraShake {
                translation: translation.clamped(0.0, 4.0),
                rotation: rotation.clamped(0.0, 0.5),
                seconds: seconds.clamped(0.05, 10.0),
                hold: hold.clamped(1.0, 8.0),
                left: seconds.clamped(0.05, 10.0),
            });
        }
    }
    /// Run the camera shakes for `seconds` and settle this frame's jitter.
    pub fn advance_camera_shakes(&mut self, seconds: f32) {
        let seconds = if seconds.is_finite() {
            seconds.clamped(0.0, 0.25)
        } else {
            0.0
        };
        for shake in &mut self.camera_shakes {
            shake.left -= seconds;
        }
        let mut jolt = CameraJolt::default();
        let unit = |state: &mut u32| {
            // xorshift32 to -1..1: the jitter is cosmetic, any stream will do.
            *state ^= *state << 13;
            *state ^= *state >> 17;
            *state ^= *state << 5;
            *state as f32 / u32::MAX as f32 * 2.0 - 1.0
        };
        if self.jolt_state == 0 {
            self.jolt_state = 0x9e37_79b9;
        }
        for shake in &self.camera_shakes {
            let share = shake.share();
            if share <= 0.0 {
                continue;
            }
            let (t, r) = (shake.translation * share, shake.rotation * share);
            jolt.offset[0] += unit(&mut self.jolt_state) * t;
            jolt.offset[1] += unit(&mut self.jolt_state) * t;
            jolt.yaw += unit(&mut self.jolt_state) * r;
            jolt.pitch += unit(&mut self.jolt_state) * r;
            jolt.roll += unit(&mut self.jolt_state) * r;
        }
        self.camera_shakes.retain(|s| s.left > 0.0);
        self.camera_jolt = jolt;
    }
    /// This frame's camera jitter, to move and turn the view by; none while
    /// no shake runs.
    pub fn camera_jolt(&self) -> CameraJolt {
        self.camera_jolt
    }
    /// A rule kicks the aim (`view_shake`): see [`ViewShake`]. A new kick
    /// replaces one still running.
    pub fn start_view_shake(&mut self, direction: f32, min: f32, max: f32, rate: f32) {
        if [direction, min, max, rate].iter().all(|v| v.is_finite()) {
            self.view_shake = Some(ViewShake {
                pitch: direction.cos(),
                yaw: direction.sin(),
                offset: 0.0,
                min,
                max,
                rate,
            });
        }
    }
    /// Run the kick for `seconds`, turning the real look.
    pub fn advance_view_shake(&mut self, seconds: f32) {
        let Some(shake) = self.view_shake.as_mut() else {
            return;
        };
        if !seconds.is_finite() || self.death_view {
            return;
        }
        match shake.advance(seconds.min(0.1)) {
            Some((pitch, yaw)) => {
                self.pitch = (self.pitch + pitch).clamped(-FRAC_PI_2, FRAC_PI_2);
                self.yaw += yaw;
            }
            None => self.view_shake = None,
        }
    }
    /// Aiming with a [`bri_weapons::Breath`]: the sound to play when the
    /// breath turns (out as the cycle goes negative, in as it comes back),
    /// as `AGP_Pawn::BreatheCycle` plays `BreathOutSound` and
    /// `BreathInSound` where its cycle crosses zero. The clock runs
    /// whether or not the player aims.
    pub fn advance_breath(&mut self, seconds: f32) -> Option<String> {
        if !seconds.is_finite() {
            return None;
        }
        self.breath_clock = (self.breath_clock + seconds.min(0.25)) % 3600.0;
        let Some(breath) = self.aim_while_aiming().and_then(|a| a.breath.clone()) else {
            self.breath_out = None;
            return None;
        };
        let out = (self.breath_clock * std::f32::consts::TAU / breath.period).sin() < 0.0;
        let turned = self.breath_out.is_some_and(|before| before != out);
        self.breath_out = Some(out);
        turned.then_some(if out { breath.exhale } else { breath.inhale })
    }
    pub fn set_server_fov(&mut self, fov: Option<f32>) {
        self.server_fov = fov.filter(|f| f.is_finite()).map(|f| {
            f.clamped(
                *bri_package_runtime::ops::FOV_RANGE.start(),
                *bri_package_runtime::ops::FOV_RANGE.end(),
            )
        });
    }
    /// Ramp the shown FOV toward the zoom FOV while Zoom is held, else the
    /// normal FOV. Wheel steps and the options slider ride the same ramp.
    pub fn advance_zoom(&mut self, seconds: f32) {
        if !seconds.is_finite() {
            return;
        }
        let target = self.target_fov();
        let shown = self.fov_shown.get_or_insert(target);
        let step = ZOOM_DEGREES_PER_SECOND * seconds.clamped(0.0, 0.25);
        *shown += (target - *shown).clamped(-step, step);
    }
    /// The held weapon's aim, or `None` when it has none: holding Zoom (or
    /// Jet, when the aim is `on_jet`) then aims at its FOV in place of the
    /// wheel's zoom.
    /// Look speed's multiple from an aim assist (`AimAssist::slowdown`),
    /// 0.1 to 1.
    pub fn set_assist_slowdown(&self, slowdown: f32) {
        self.assist_slowdown.set(
            slowdown
                .is_finite()
                .then(|| slowdown.clamped(0.1, 1.0))
                .filter(|s| *s < 1.0),
        );
    }
    /// A profile without current-state information. Restricted profiles fail
    /// closed; use `set_aim_in_state` when the accepted mounted state is known.
    pub fn set_aim(&mut self, aim: Option<bri_weapons::Zoom>) {
        self.set_aim_in_state(aim, None);
    }
    pub fn set_aim_in_state(&mut self, aim: Option<bri_weapons::Zoom>, state: Option<&str>) {
        let aim = aim.filter(|a| a.validate().is_ok());
        self.aim_state_allowed = aim.as_ref().is_none_or(|a| a.allows_state(state));
        if aim != self.aim {
            // Another weapon (or none) starts from its widest step.
            self.aim_level = 0;
            self.aim = aim;
        }
    }
    /// Aiming down the held weapon's sights.
    pub fn aiming(&self) -> bool {
        self.aim.as_ref().is_some_and(|a| {
            self.aim_state_allowed
                && self.observer.is_none()
                && (self.held(HeldControl::Zoom) || (a.on_jet && self.held(HeldControl::Jet)))
        })
    }
    /// Which of the aim's zoom steps is showing while aiming: 1 for the
    /// aim's own field of view, 2 and up for each of its wheel `levels`; 0
    /// when not aiming (a HUD marker for the scope's level).
    pub fn zoom_level(&self) -> u32 {
        self.aim_while_aiming()
            .map_or(0, |_| self.aim_level as u32 + 1)
    }
    fn aim_while_aiming(&self) -> Option<&bri_weapons::Zoom> {
        self.aim.as_ref().filter(|_| self.aiming())
    }
    /// Aiming hides the crosshair when the aim says so.
    pub fn aim_hides_crosshair(&self) -> bool {
        self.aim_while_aiming().is_some_and(|a| !a.crosshair)
    }
    /// Look speed's multiple from the aim (`Zoom::sensitivity`), 1 when
    /// not aiming.
    fn aim_sensitivity(&self) -> f32 {
        self.aim_while_aiming().map_or(1.0, |a| a.sensitivity)
    }
    /// The mouse wheel steps the aim's magnification (`Zoom::levels`)
    /// rather than the tools.
    pub fn aim_takes_wheel(&self) -> bool {
        self.aim_while_aiming()
            .is_some_and(|a| !a.levels.is_empty())
    }
    /// Step the aim's magnification by `notches` (positive rolled forward,
    /// zooming in); false when the wheel is not the aim's.
    pub fn aim_wheel(&mut self, notches: i32) -> bool {
        let Some(levels) = self.aim_while_aiming().map(|a| a.levels.len()) else {
            return false;
        };
        if levels == 0 {
            return false;
        }
        self.aim_level =
            (self.aim_level as i64 + i64::from(notches)).clamp(0, levels as i64) as usize;
        true
    }
    /// The scope picture to draw over the screen (`Zoom::overlay`): while
    /// aiming from the eye, once the camera is all the way in.
    pub fn scope_overlay(&self) -> Option<&str> {
        self.aim_while_aiming()
            .and_then(|a| a.overlay.as_deref())
            .filter(|_| self.at_eye())
    }
    /// Third person as the view shows it: aiming a `first_person` aim
    /// looks from the eye whatever the toggle says.
    pub fn third_person_view(&self) -> bool {
        !self.first_person_only
            && (self.third_person_only
                || (self.third_person && !self.aim_while_aiming().is_some_and(|a| a.first_person)))
    }
    /// The player's type keeps the view first person (`firstPersonOnly`);
    /// the toggle is kept for when it lifts.
    pub fn set_first_person_only(&mut self, on: bool) {
        self.first_person_only = on;
    }
    /// `thirdPersonOnly`: while the body's archetype says so, the camera
    /// stays out behind it whatever the view toggle says.
    pub fn set_third_person_only(&mut self, only: bool) {
        self.third_person_only = only;
    }
    /// Move a swaying aim along its figure of eight (`Zoom::sway`), easing
    /// in while aiming on foot and out after. Crouching steadies it and
    /// moving shakes it, as the sway says. Deterministic in the frame
    /// times it is given.
    pub fn advance_sway(&mut self, seconds: f32) {
        if !seconds.is_finite() {
            return;
        }
        let seconds = seconds.clamped(0.0, 0.25);
        let on_foot = self.observer.is_none()
            && !self.mounted
            && self.vehicle_view.is_none()
            && self.seat_yaw.is_none();
        let aimed = self
            .aim_while_aiming()
            .and_then(|a| a.sway)
            .filter(|_| on_foot);
        if let Some(def) = aimed {
            self.sway.def = Some(def);
        }
        let Some(def) = self.sway.def else {
            return;
        };
        let target = match aimed {
            None => 0.0,
            Some(_) if self.held(HeldControl::Crouch) => def.crouched,
            Some(_) => {
                let moving = [
                    HeldControl::Forward,
                    HeldControl::Backward,
                    HeldControl::Left,
                    HeldControl::Right,
                    HeldControl::Jump,
                    HeldControl::Jet,
                ]
                .into_iter()
                .any(|c| {
                    self.held(c)
                        && !(c == HeldControl::Jet && self.aim.as_ref().is_some_and(|a| a.on_jet))
                });
                if moving { def.moving } else { 1.0 }
            }
        };
        let step = seconds / SWAY_EASE_SECONDS;
        self.sway.gain += (target - self.sway.gain).clamped(-step, step);
        self.sway.phase = (self.sway.phase + f64::from(seconds / def.seconds)).fract();
        let (yaw, pitch) = def.offset(self.sway.phase);
        let now = (yaw * self.sway.gain, pitch * self.sway.gain);
        let (dy, dp) = (now.0 - self.sway.applied.0, now.1 - self.sway.applied.1);
        self.sway.applied = now;
        if on_foot {
            self.yaw = wrap(self.yaw + dy);
            self.pitch = (self.pitch + dp).clamped(-FRAC_PI_2, FRAC_PI_2);
        }
        if self.sway.gain == 0.0 {
            self.sway = SwayState::default();
        }
    }
    /// The active body's camera (`Look::camera`) at its `scale`. A
    /// different profile starts from rest, so switching bodies hands the
    /// camera over cleanly: the old body's bob and trail never carry over.
    pub fn set_body_camera(&mut self, profile: &bri_content::camera::CameraProfile, scale: f32) {
        if self.body_camera.profile != *profile {
            self.body_camera = BodyCamera {
                profile: profile.clone(),
                ..Default::default()
            };
        }
        self.body_camera.scale = if scale.is_finite() { scale } else { 1.0 };
    }
    /// The body owns the view: on foot, not seated, riding or watching
    /// from a camera. A vehicle's camera owns it in a seat, and aiming
    /// owns the FOV while it runs.
    fn body_owns_view(&self) -> bool {
        self.observer.is_none()
            && !self.mounted
            && self.vehicle_view.is_none()
            && self.seat_yaw.is_none()
    }
    /// Move the body camera's bob and weapon trail on by `seconds`, the body
    /// moving at `ground_speed` (horizontal units a second), on the
    /// ground or not. At rest whenever the body does not own the view.
    pub fn advance_body_camera(&mut self, seconds: f32, ground_speed: f32, grounded: bool) {
        let look = self.view_angles();
        if !self.body_owns_view() || self.body_camera.profile.is_default() {
            self.body_camera.motion = Default::default();
            self.body_camera.last_look = Some(look);
            return;
        }
        let turned = self
            .body_camera
            .last_look
            .map_or((0.0, 0.0), |(yaw, pitch)| {
                (wrap(look.0 - yaw), look.1 - pitch)
            });
        self.body_camera.last_look = Some(look);
        let scale = self.body_camera.scale;
        self.body_camera.motion.advance(
            &self.body_camera.profile,
            seconds,
            ground_speed / scale.max(0.01),
            grounded,
            turned,
        );
    }
    /// The body struck the ground going `speed` units a second downward:
    /// the profile's landing dip, when it has one.
    pub fn land_body_camera(&mut self, speed: f32) {
        if !self.body_owns_view() {
            return;
        }
        let scale = self.body_camera.scale.max(0.01);
        let profile = self.body_camera.profile.clone();
        self.body_camera.motion.land(&profile, speed / scale);
    }
    /// Where the body moves the first-person camera from its eye, in the
    /// world, for a look heading `yaw`: the profile's `eye_offset` plus
    /// the bob. Zero for the stock camera and whenever the body does not
    /// own the view.
    pub fn body_eye_offset(&self, yaw: f32) -> glam::Vec3 {
        let b = &self.body_camera;
        if !self.body_owns_view() || b.profile.is_default() {
            return glam::Vec3::ZERO;
        }
        let (side, dip) = b.motion.bob(&b.profile);
        let [right, up, forward] = b.profile.eye_offset;
        let heading = glam::Vec3::new(yaw.sin(), 0.0, -yaw.cos());
        let across = glam::Vec3::new(yaw.cos(), 0.0, yaw.sin());
        let offset = across * (right + side) + glam::Vec3::Y * (up + dip) + heading * forward;
        offset * b.scale
    }
    /// The held weapon's sway and bob in first person, in the view's frame
    /// (x right, y up, looking down -z), applied to the view frame the
    /// weapon is drawn in. `None` when there is none to apply.
    pub fn view_model_sway(&self) -> Option<glam::Mat4> {
        let b = &self.body_camera;
        b.profile.sway.as_ref()?;
        if !self.body_owns_view() || b.motion.at_rest() {
            return None;
        }
        let (side, dip) = b.motion.weapon_bob(&b.profile);
        let (yaw, pitch) = b.motion.trail;
        let m = glam::Mat4::from_rotation_translation(
            glam::Quat::from_rotation_y(-yaw) * glam::Quat::from_rotation_x(pitch),
            glam::Vec3::new(side, dip, 0.0) * b.scale,
        );
        m.is_finite().then_some(m)
    }
    /// The view model's sway for Add-On code (`view`): trail yaw and pitch
    /// (radians), then its bob offset in view space.
    pub fn view_model_record(&self) -> [f32; 5] {
        let b = &self.body_camera;
        match (&b.profile.sway, self.body_owns_view()) {
            (Some(_), true) => {
                let (side, dip) = b.motion.weapon_bob(&b.profile);
                let k = b.scale;
                [b.motion.trail.0, b.motion.trail.1, side * k, dip * k, 0.0]
            }
            _ => [0.0; 5],
        }
    }
    /// The on-foot third-person pivot's shift from the body's profile, in
    /// the world, for a look heading `yaw`.
    pub fn body_pivot_offset(&self, yaw: f32) -> glam::Vec3 {
        let b = &self.body_camera;
        match &b.profile.third_person {
            Some(t) if self.body_owns_view() => {
                let across = glam::Vec3::new(yaw.cos(), 0.0, yaw.sin());
                (across * t.side + glam::Vec3::Y * t.height) * b.scale
            }
            _ => glam::Vec3::ZERO,
        }
    }
    fn target_fov(&self) -> f32 {
        if let Some(aim) = self.aim_while_aiming() {
            let unzoomed = self.server_fov.unwrap_or_else(|| {
                self.body_camera
                    .profile
                    .fov_for(self.normal_fov.unwrap_or(90.0))
            });
            aim.level_fov_from(self.aim_level, unzoomed)
                .clamped(ZOOM_FOV_RANGE.0, ZOOM_FOV_RANGE.1)
        } else if self.held(HeldControl::Zoom) && (self.aim.is_none() || self.aim_state_allowed) {
            self.zoom_fov.unwrap_or(10.0)
        } else {
            // The body's FOV, at the ratio the player's own setting keeps.
            self.server_fov.unwrap_or_else(|| {
                self.body_camera
                    .profile
                    .fov_for(self.normal_fov.unwrap_or(90.0))
            })
        }
    }
    /// The player's normal horizontal FOV in degrees, zoom aside.
    pub fn normal_fov(&self) -> f32 {
        self.normal_fov.unwrap_or(90.0)
    }
    /// The current horizontal FOV in degrees (Torque's `$cameraFov`). Look
    /// sensitivity follows it through the transition.
    pub fn fov(&self) -> f32 {
        self.fov_shown.unwrap_or_else(|| self.target_fov())
    }
    /// Slide the camera out to third person or in to first person
    /// (`GameConnection::getControlCameraTransform`).
    pub fn advance_view(&mut self, seconds: f32) {
        if !seconds.is_finite() {
            return;
        }
        let speed = if self.fast_view {
            CAMERA_SPEED.1
        } else {
            CAMERA_SPEED.0
        };
        let step = speed * seconds.clamped(0.0, 0.25);
        self.camera_pos = if self.third_person_view() {
            (self.camera_pos + step).min(1.0)
        } else {
            (self.camera_pos - step).max(0.0)
        };
    }
    /// How far the camera is out toward third person, 0 to 1.
    pub fn camera_pos(&self) -> f32 {
        self.camera_pos
    }
    /// Whether the view draws as first person: Torque's
    /// `GameConnection::isFirstPerson` is `mCameraPos == 0`, so the own body,
    /// its third-person images and the crosshair switch only once the
    /// camera has slid all the way into the eye, and switch back the moment
    /// it starts sliding out. The toggle alone ([`Self::third_person_view`])
    /// only says which way the camera is heading.
    pub fn at_eye(&self) -> bool {
        self.camera_pos == 0.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn held(c: &mut Controls, key: HeldControl, down: bool) {
        c.action(&GameAction::Held { control: key, down });
    }
    /// Max, v0.1.4: turning a horse in third person, the camera swung
    /// round before the horse did. The mouse steers the horse, which is
    /// drawn from its predicted ticks; the view follows that drawn horse,
    /// turned only by the head's free look.
    #[test]
    fn a_mount_rider_looks_along_the_drawn_mount() {
        let mut c = Controls::default();
        c.set_mounted(true);
        c.action(&GameAction::Look {
            yaw: 0.4,
            pitch: -0.2,
        });
        // The move carries the turn to the horse at once...
        assert!((c.movement().yaw - 0.4).abs() < 1e-6);
        // ...and the view waits for the horse as drawn.
        let drawn = glam::Quat::from_rotation_y(-0.1);
        let (yaw, pitch) = c.mount_look(drawn);
        assert!((yaw - 0.1).abs() < 1e-6, "{yaw}");
        assert!((pitch - c.pitch).abs() < 1e-6);
        // Free look turns the head on the drawn horse.
        held(&mut c, HeldControl::FreeLook, true);
        c.action(&GameAction::Look {
            yaw: 0.25,
            pitch: 0.0,
        });
        let (yaw, _) = c.mount_look(drawn);
        assert!((yaw - 0.35).abs() < 1e-6, "{yaw}");
    }
    #[test]
    fn opposing_controls_walk_and_release() {
        let mut c = Controls::default();
        held(&mut c, HeldControl::Forward, true);
        held(&mut c, HeldControl::Backward, true);
        assert_eq!(c.movement().forward, 0.0);
        held(&mut c, HeldControl::Backward, false);
        held(&mut c, HeldControl::Walk, true);
        assert_eq!(c.movement().forward, 0.4);
        held(&mut c, HeldControl::Jet, true);
        c.release();
        assert_eq!(c.movement(), MoveInput::default());
    }
    #[test]
    fn freelook_never_changes_body_facing_and_zoom_is_held() {
        // On foot in first person v20 has no free look (`updateMove`
        // 0x5aea5f): Z held, the mouse turns the body as usual.
        let mut c = Controls::default();
        held(&mut c, HeldControl::FreeLook, true);
        c.action(&GameAction::Look {
            yaw: 0.3,
            pitch: 0.0,
        });
        assert!((c.movement().yaw - 0.3).abs() < 1e-6);
        assert_eq!(c.movement().head_yaw, 0.0);
        held(&mut c, HeldControl::FreeLook, false);
        // In third person it turns the head.
        let mut c = Controls::default();
        c.action(&GameAction::ToggleFirstPerson { fast: true });
        c.advance_view(0.01);
        c.action(&GameAction::Look {
            yaw: 0.5,
            pitch: 0.1,
        });
        held(&mut c, HeldControl::FreeLook, true);
        c.action(&GameAction::Look {
            yaw: 1.0,
            pitch: 0.3,
        });
        assert_eq!(c.movement().yaw, 0.5);
        assert_ne!(c.view_angles().0, c.yaw);
        // The head carries the free turn to everyone; pitch stays the body's.
        assert_eq!(c.movement().head_yaw, 1.0);
        assert!((c.movement().pitch + 0.4).abs() < 1e-6);
        for _ in 0..10 {
            c.action(&GameAction::Look {
                yaw: 1.0,
                pitch: 0.0,
            });
        }
        assert_eq!(c.movement().head_yaw, MAX_FREELOOK);
        held(&mut c, HeldControl::FreeLook, false);
        // Let go, the head eases back, halving every 32 ms tick.
        c.advance_head(0.032);
        assert!((c.movement().head_yaw - MAX_FREELOOK / 2.0).abs() < 1e-5);
        c.advance_head(1.0);
        assert!(c.movement().head_yaw.abs() < 1e-4);
        c.set_fov_prefs(90.0, 45.0);
        c.advance_zoom(0.0);
        held(&mut c, HeldControl::Zoom, true);
        assert_eq!(c.fov(), 90.0, "zoom ramps in rather than snapping");
        // v20's linear 450 degrees/s: 45 degrees takes 0.1 s.
        c.advance_zoom(0.05);
        assert!((c.fov() - 67.5).abs() < 1e-3, "{}", c.fov());
        c.advance_zoom(0.05);
        assert_eq!(c.fov(), 45.0);
        held(&mut c, HeldControl::Zoom, false);
        for _ in 0..4 {
            c.advance_zoom(0.25);
        }
        assert_eq!(c.fov(), 90.0);
    }
    fn shooter_camera() -> bri_content::camera::CameraProfile {
        serde_json::from_value(serde_json::json!({
            "fov": 70,
            "eye_offset": [0, 0.1, 0.05],
            "bob": { "height": 0.06, "side": 0.03, "stride": 2.4 },
            "sway": { "lag": 0.5, "max_degrees": 4, "settle": 8, "bob": 0.5 },
            "third_person": { "side": 0.7, "height": 0.3 }
        }))
        .unwrap()
    }
    /// Walk forward turning right for `frames`, then return the view.
    fn walk(c: &mut Controls, frames: u32) {
        for _ in 0..frames {
            c.yaw = wrap(c.yaw + 0.01);
            c.advance_body_camera(1.0 / 60.0, 6.0, true);
            c.advance_zoom(1.0 / 60.0);
        }
    }
    #[test]
    fn the_stock_body_camera_changes_nothing() {
        let mut c = Controls::default();
        c.set_fov_prefs(100.0, 45.0);
        let before = (c.fov(), c.body_eye_offset(0.3), c.body_pivot_offset(0.3));
        c.set_body_camera(&Default::default(), 1.0);
        walk(&mut c, 60);
        assert_eq!(c.fov(), 100.0);
        assert_eq!(c.body_eye_offset(0.3), glam::Vec3::ZERO);
        assert_eq!(c.body_pivot_offset(0.3), glam::Vec3::ZERO);
        assert_eq!(before.1, glam::Vec3::ZERO);
        assert!(c.view_model_sway().is_none());
        assert_eq!(c.view_model_record(), [0.0; 5]);
    }
    #[test]
    fn a_body_camera_owns_the_view_and_hands_it_back_on_a_switch() {
        let mut c = Controls::default();
        c.set_fov_prefs(90.0, 45.0);
        c.set_body_camera(&shooter_camera(), 1.0);
        walk(&mut c, 120);
        assert!((c.fov() - 70.0).abs() < 1e-4, "the body's FOV: {}", c.fov());
        let offset = c.body_eye_offset(c.yaw);
        assert!(offset.y > 0.0 && offset.length() > 0.05, "{offset:?}");
        assert!(c.view_model_sway().is_some(), "the weapon trails the turn");
        assert!(c.body_pivot_offset(0.0).x > 0.6, "over the right shoulder");
        // Seated, the vehicle's camera owns the view: no bob, no sway.
        c.set_mounted(true);
        walk(&mut c, 1);
        assert_eq!(c.body_eye_offset(c.yaw), glam::Vec3::ZERO);
        assert!(c.view_model_sway().is_none());
        c.set_mounted(false);
        // Back to the Blockhead: everything stock again, the FOV gliding home.
        c.set_body_camera(&Default::default(), 1.0);
        walk(&mut c, 120);
        assert_eq!(c.fov(), 90.0);
        assert_eq!(c.body_eye_offset(c.yaw), glam::Vec3::ZERO);
        assert!(c.view_model_sway().is_none());
        // Aiming still owns the FOV over the body's.
        c.set_body_camera(&shooter_camera(), 1.0);
        c.set_server_fov(Some(50.0));
        walk(&mut c, 120);
        assert_eq!(c.fov(), 50.0, "a rule's set_fov wins over the body");
    }
    #[test]
    fn host_fov_replaces_the_normal_fov_until_handed_back() {
        let mut c = Controls::default();
        c.set_fov_prefs(100.0, 45.0);
        c.advance_zoom(0.0);
        c.set_server_fov(Some(30.0));
        c.advance_zoom(10.0);
        assert_eq!(c.fov(), 30.0);
        c.set_server_fov(Some(f32::NAN));
        c.advance_zoom(10.0);
        assert_eq!(c.fov(), 100.0, "a bad value hands the view back");
        c.set_server_fov(Some(1.0));
        c.advance_zoom(10.0);
        assert_eq!(c.fov(), 5.0, "clamped to the camera's range");
        c.set_server_fov(None);
        c.advance_zoom(10.0);
        assert_eq!(c.fov(), 100.0);
    }
    #[test]
    fn fov_prefs_set_normal_and_zoom_and_wheel_steps_glide() {
        let mut c = Controls::default();
        assert_eq!(c.fov(), 90.0, "v20 default FOV");
        c.set_fov_prefs(110.0, 10.0);
        assert_eq!(c.fov(), 110.0);
        held(&mut c, HeldControl::Zoom, true);
        for _ in 0..120 {
            c.advance_zoom(1.0 / 60.0);
        }
        assert_eq!(c.fov(), 10.0, "v20's saved zoom FOV, not a fixed 45");
        c.action(&GameAction::SetZoomFov { fov: 15.0 });
        c.set_fov_prefs(110.0, 10.0);
        c.advance_zoom(0.005);
        let gliding = c.fov();
        assert!((gliding - 12.25).abs() < 1e-3, "{gliding}");
        for _ in 0..120 {
            c.advance_zoom(1.0 / 60.0);
        }
        assert_eq!(c.fov(), 15.0);
        // Mouse look slows with the zoomed FOV, as v20's `$cameraFov / 90`.
        let before = c.yaw;
        c.action(&GameAction::Look {
            yaw: 0.9,
            pitch: 0.0,
        });
        assert!((c.yaw - before - 0.15).abs() < 1e-5);
    }
    #[test]
    fn view_toggle_slides_the_camera_in_a_fifth_of_a_second() {
        let mut c = Controls::default();
        c.action(&GameAction::ToggleFirstPerson { fast: false });
        c.advance_view(0.1);
        assert!((c.camera_pos() - 0.5).abs() < 1e-6);
        c.advance_view(0.2);
        assert_eq!(c.camera_pos(), 1.0);
        c.action(&GameAction::ToggleFirstPerson { fast: true });
        c.advance_view(0.001);
        assert_eq!(c.camera_pos(), 0.0, "FastFirstThirdPerson snaps");
    }
    #[test]
    fn the_view_draws_first_person_only_with_the_camera_in_the_eye() {
        let mut c = Controls::default();
        assert!(c.at_eye());
        // Out to third person: the body shows from the first frame.
        c.action(&GameAction::ToggleFirstPerson { fast: false });
        assert!(c.third_person_view());
        assert!(c.at_eye(), "the toggle alone moves nothing");
        c.advance_view(0.001);
        assert!(!c.at_eye(), "the body shows as the camera starts out");
        c.advance_view(0.5);
        assert_eq!(c.camera_pos(), 1.0);
        // Back in: the body stays until the camera reaches the eye.
        c.action(&GameAction::ToggleFirstPerson { fast: false });
        assert!(!c.third_person_view());
        for _ in 0..11 {
            c.advance_view(1.0 / 60.0);
            assert!(!c.at_eye(), "still sliding in at {}", c.camera_pos());
        }
        c.advance_view(2.0 / 60.0);
        assert!(c.at_eye(), "in the eye once the fifth of a second is up");
    }
    #[test]
    fn a_first_person_only_player_type_holds_the_view_in_the_eye() {
        let mut c = Controls::default();
        c.action(&GameAction::ToggleFirstPerson { fast: false });
        c.advance_view(0.5);
        assert_eq!(c.camera_pos(), 1.0);
        // The Light MG gunner's LMGArmor (`firstPersonOnly`).
        c.set_first_person_only(true);
        assert!(!c.third_person_view());
        c.advance_view(0.5);
        assert!(c.at_eye());
        // Lifted, the toggle the player left in third person takes over.
        c.set_first_person_only(false);
        assert!(c.third_person_view());
    }
    #[test]
    fn camera_control_leaves_the_body_still_and_unturned() {
        let mut c = Controls::default();
        c.action(&GameAction::Look {
            yaw: 0.4,
            pitch: 0.1,
        });
        let body = c.movement();
        c.follow(
            ControlObject::Camera,
            1,
            Some(glam::Vec3::new(0.0, 2.0, 0.0)),
        );
        held(&mut c, HeldControl::Forward, true);
        c.action(&GameAction::Look {
            yaw: 1.2,
            pitch: -0.5,
        });
        let input = c.movement();
        assert_eq!(
            input,
            MoveInput {
                yaw: body.yaw,
                pitch: body.pitch,
                ..Default::default()
            }
        );
        assert_eq!(c.view_angles(), (body.yaw, body.pitch));
        assert_ne!(c.camera_angles().0, body.yaw);
        c.fly(0.05, &Default::default());
        let flown = c.free_camera().unwrap();
        assert!(flown.y > 2.0, "looking up flies the camera up");
        // A repeated grant keeps the camera where it was flown.
        c.follow(ControlObject::Camera, 1, Some(glam::Vec3::ZERO));
        assert_eq!(c.free_camera(), Some(flown));
        c.follow(ControlObject::Player, 1, None);
        assert_eq!(c.movement().forward, 1.0);
        assert_eq!(c.movement().yaw, body.yaw);
    }
    /// A free camera flown into a portal comes out of its partner, turned
    /// as a body is.
    #[test]
    fn a_free_camera_flies_through_a_portal() {
        use bri_content::passage::{Passage, Passages};
        // In through z = 0 going -z, out at x = 10 turned a quarter.
        let carry = glam::Affine3A::from_translation(glam::Vec3::X * 10.0)
            * glam::Affine3A::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        let passages = Passages {
            list: vec![Passage {
                brick: 1,
                centre: glam::Vec3::Y,
                normal: glam::Vec3::Z,
                u: glam::Vec3::X,
                v: glam::Vec3::Y,
                half: glam::Vec2::new(1.0, 1.0),
                carry,
            }],
            closed: vec![],
        };
        let mut c = Controls::default();
        c.follow(
            ControlObject::Camera,
            1,
            Some(glam::Vec3::new(0.0, 1.0, 0.5)),
        );
        held(&mut c, HeldControl::Forward, true);
        let before = c.observer().unwrap();
        let forward = glam::Vec3::new(before.yaw.sin(), 0.0, -before.yaw.cos());
        c.fly(0.05, &passages);
        let at = c.free_camera().unwrap();
        let moved = glam::Vec3::new(0.0, 1.0, 0.5) + forward * CAMERA_MOVEMENT_SPEED * 0.05;
        assert!(at.abs_diff_eq(carry.transform_point3(moved), 1e-3), "{at}");
        let after = c.observer().unwrap();
        let turned = glam::Vec3::new(after.yaw.sin(), 0.0, -after.yaw.cos());
        assert!(
            turned.abs_diff_eq(carry.transform_vector3(forward), 1e-4),
            "{turned}"
        );
    }
    /// v20's fly mode: 40 units/s, doubled while fire is held, quartered
    /// while crouching, walk scaling each axis by 0.4, no vertical keys.
    #[test]
    fn free_camera_flies_at_v20_speeds() {
        let flown = |keys: &[HeldControl]| {
            let mut c = Controls::default();
            c.follow(ControlObject::Camera, 1, Some(glam::Vec3::ZERO));
            for &key in keys {
                held(&mut c, key, true);
            }
            c.fly(0.1, &Default::default());
            c.free_camera().unwrap()
        };
        let close = |a: glam::Vec3, b: glam::Vec3| (a - b).length() < 1e-4;
        let ahead = |d: f32| glam::Vec3::new(0.0, 0.0, -d);
        use HeldControl::*;
        assert!(close(flown(&[Forward]), ahead(4.0)));
        assert!(close(flown(&[Forward, Fire]), ahead(8.0)));
        assert!(close(flown(&[Forward, Crouch]), ahead(1.0)));
        // Fire wins over crouch.
        assert!(close(flown(&[Forward, Fire, Crouch]), ahead(8.0)));
        assert!(close(flown(&[Forward, Walk]), ahead(1.6)));
        assert!(close(flown(&[Forward, Walk, Fire]), ahead(3.2)));
        // Each axis moves at full speed, so a diagonal is faster.
        assert!(close(
            flown(&[Forward, Right]),
            glam::Vec3::new(4.0, 0.0, -4.0)
        ));
        // Jump, jet and fire alone do not move it.
        assert_eq!(flown(&[Jump, Jet, Fire]), glam::Vec3::ZERO);
    }
    #[test]
    fn leaving_the_camera_forgets_a_held_fire() {
        let mut c = Controls::default();
        c.follow(ControlObject::Camera, 1, Some(glam::Vec3::ZERO));
        held(&mut c, HeldControl::Fire, true);
        assert_eq!(c.fly_speed(), 80.0);
        c.follow(ControlObject::Player, 1, None);
        c.follow(ControlObject::Camera, 1, Some(glam::Vec3::ZERO));
        assert_eq!(c.fly_speed(), 40.0);
    }
    #[test]
    fn spy_orbits_with_the_mouse_and_never_flies() {
        let mut c = Controls::default();
        c.follow(ControlObject::Spy(7), 1, None);
        held(&mut c, HeldControl::Forward, true);
        c.fly(0.05, &Default::default());
        assert_eq!(c.free_camera(), None);
        c.action(&GameAction::Look {
            yaw: 0.3,
            pitch: 0.0,
        });
        assert_eq!(c.observer().unwrap().mode, ObserverMode::Orbit(7));
        assert!((c.camera_angles().0 - 0.3).abs() < 1e-6);
        assert_eq!(c.movement().forward, 0.0);
        assert_eq!(c.movement().yaw, 0.0);
    }
    /// An Add-On's orbit circles its target at the Add-On's distance, and
    /// the wheel zooms it a unit a notch within its range, for as long as
    /// the host keeps that orbit; the spy and corpse cameras keep v20's 8.
    #[test]
    fn an_add_on_orbit_sits_at_its_own_distance() {
        let mut c = Controls::default();
        let orbit = ControlObject::Orbit {
            target: 7,
            min: 5,
            max: 10,
            distance: 5,
            body: bri_sim::session::OrbitBody::Acts,
        };
        c.follow(orbit, 1, None);
        let observer = c.observer().unwrap();
        assert_eq!(observer.mode, ObserverMode::Orbit(7));
        assert_eq!(observer.distance, 5.0);
        assert!(c.orbit_zooms());
        assert_eq!(c.movement().forward, 0.0);
        c.zoom_orbit(-3);
        c.follow(orbit, 1, None);
        assert_eq!(c.observer().unwrap().distance, 8.0, "kept every frame");
        c.zoom_orbit(-9);
        assert_eq!(c.observer().unwrap().distance, 10.0, "no farther than max");
        c.zoom_orbit(20);
        assert_eq!(c.observer().unwrap().distance, 5.0, "no nearer than min");
        c.follow(ControlObject::Spy(7), 1, None);
        assert_eq!(c.observer().unwrap().distance, CORPSE_ORBIT_DISTANCE);
        assert!(!c.orbit_zooms());
        c.zoom_orbit(3);
        assert_eq!(c.observer().unwrap().distance, CORPSE_ORBIT_DISTANCE);
        c.follow(ControlObject::Player, 1, None);
        assert_eq!(c.observer(), None);
    }
    #[test]
    fn rules_cameras_fly_freely_or_circle_their_point() {
        let mut c = Controls::default();
        // A spectator's free camera flies like the admin's.
        c.follow(ControlObject::Observer, 1, Some(glam::Vec3::ZERO));
        held(&mut c, HeldControl::Forward, true);
        c.fly(0.05, &Default::default());
        assert!(c.free_camera().is_some_and(|p| p != glam::Vec3::ZERO));
        held(&mut c, HeldControl::Forward, false);
        // A point camera circles the replicated point at its distance,
        // turned by the mouse, and never flies.
        c.follow(ControlObject::Point, 1, Some(glam::Vec3::ZERO));
        c.orbit_point(bri_sim::session::OrbitPoint {
            at: [3.0, 1.0, 4.0],
            distance: 4.5,
            object: None,
        });
        assert_eq!(
            c.orbit_focus(
                &BTreeMap::new(),
                &|_| false,
                &Default::default(),
                &Default::default()
            ),
            Some(glam::Vec3::new(3.0, 1.0, 4.0))
        );
        assert_eq!(c.orbit_distance(), 4.5);
        c.action(&GameAction::Look {
            yaw: 0.3,
            pitch: 0.0,
        });
        assert!((c.camera_angles().0 - 0.3).abs() < 1e-6);
        assert_eq!(c.free_camera(), None);
    }
    #[test]
    fn spy_orbit_follows_its_target() {
        let mut c = Controls::default();
        let body = |owner, x: f32| PlayerState {
            owner,
            feet: [x, 0.0, 0.0],
            velocity: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            grounded: true,
            crouched: false,
            jetting: false,
            jump: Default::default(),
            archetype: Default::default(),
            scale: 1.0,
            energy: 100.0,
            speed_scale: 1.0,
            tick: Default::default(),
            tether: None,
            controller: None,
            pass_group: 0,
            posture: Default::default(),
        };
        let mut presented = BTreeMap::from([(1, body(1, 0.0)), (7, body(7, 5.0))]);
        assert_eq!(
            c.orbit_focus(
                &presented,
                &|_| false,
                &Default::default(),
                &Default::default()
            ),
            None
        );
        c.follow(ControlObject::Spy(7), 1, None);
        let first = c
            .orbit_focus(
                &presented,
                &|_| false,
                &Default::default(),
                &Default::default(),
            )
            .unwrap();
        assert_eq!(first.x, 5.0);
        presented.insert(7, body(7, 12.0));
        assert_eq!(
            c.orbit_focus(
                &presented,
                &|_| false,
                &Default::default(),
                &Default::default()
            )
            .unwrap()
            .x,
            12.0
        );
        assert!(first.y > 1.0, "orbits the eye, not the feet");
        let archetypes = bri_sim::archetype::Archetypes::default();
        let seated = c
            .orbit_focus(
                &presented,
                &|owner| owner == 7,
                &archetypes,
                &Default::default(),
            )
            .unwrap();
        assert_eq!(seated, archetypes.posed_eye(&presented[&7], true));
        assert!(
            seated.y < first.y,
            "a seated body is orbited at its sit eye"
        );
    }
    #[test]
    fn driving_an_entity_sends_the_held_controls_steered_by_the_camera() {
        let mut c = Controls::default();
        c.follow(ControlObject::Entity(9), 1, None);
        assert_eq!(c.observer().unwrap().mode, ObserverMode::Drive(9));
        held(&mut c, HeldControl::Forward, true);
        c.action(&GameAction::Look {
            yaw: 0.6,
            pitch: 0.0,
        });
        let input = c.movement();
        assert_eq!(input.forward, 1.0);
        assert_eq!(input.yaw, c.observer().unwrap().yaw);
        let kart = bri_sim::session::EntityInfo {
            id: 9,
            kind: "kart:entity/kart".into(),
            model: "kart:model/kart".into(),
            position: [3.0, 0.0, 4.0],
            yaw: 0.0,
            scale: 1.0,
            label: String::new(),
        };
        let focus = c
            .orbit_focus(
                &BTreeMap::new(),
                &|_| false,
                &Default::default(),
                &BTreeMap::from([(9, kart)]),
            )
            .unwrap();
        assert_eq!((focus.x, focus.z), (3.0, 4.0));
        c.follow(ControlObject::Player, 1, None);
        assert_eq!(c.observer(), None);
    }
    #[test]
    fn death_orbits_the_corpse_without_turning_it() {
        let mut c = Controls::default();
        c.action(&GameAction::Look {
            yaw: 0.7,
            pitch: 0.0,
        });
        let body = c.movement().yaw;
        c.follow(ControlObject::Corpse, 1, None);
        assert_eq!(c.observer().unwrap().mode, ObserverMode::Orbit(1));
        c.action(&GameAction::Look {
            yaw: 2.0,
            pitch: 0.0,
        });
        assert_eq!(c.movement().yaw, body);
        assert_eq!(c.view_angles().0, body);
        c.follow(ControlObject::Player, 1, None);
        assert_eq!(c.observer(), None);
    }
    #[test]
    fn invalid_and_large_look_stays_valid() {
        let mut c = Controls::default();
        c.action(&GameAction::Look {
            yaw: f32::NAN,
            pitch: 0.0,
        });
        c.action(&GameAction::Look {
            yaw: 100.0,
            pitch: -100.0,
        });
        c.movement().validate().unwrap();
    }
    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }
    /// A plane looping and rolling: its seat's rotation.
    fn banked(pitch: f32, roll: f32) -> glam::Quat {
        glam::Quat::from_rotation_y(-0.7)
            * glam::Quat::from_rotation_x(pitch)
            * glam::Quat::from_rotation_z(roll)
    }
    #[test]
    fn a_seated_first_person_view_rolls_and_pitches_with_the_seat() {
        let mut c = Controls::default();
        for (pitch, roll) in [(0.0, 0.0), (1.2, 0.0), (0.0, 0.8), (2.6, -0.5)] {
            let seat = banked(pitch, roll);
            c.set_ride(Some(Ride::Seat(seat, SeatLook::Passenger)));
            let view = c.ride_view().unwrap();
            assert!(view.angle_between(seat) < 1e-4, "the head faces the seat");
            // The drawn view: its yaw, pitch and roll rebuild the seat.
            let (yaw, look) = c.view_angles();
            let frame = super::view_frame(glam::Vec3::ZERO, yaw, look, super::roll(view)).unwrap();
            let rebuilt = glam::Quat::from_mat4(&frame);
            assert!(
                rebuilt.angle_between(seat) < 1e-3,
                "pitch {pitch} roll {roll}: {rebuilt} vs {seat}"
            );
        }
        // A seat banked right tips the view's top right: a negative roll.
        c.set_ride(Some(Ride::Seat(
            glam::Quat::from_rotation_z(-0.5),
            SeatLook::Passenger,
        )));
        assert!(close(super::roll(c.ride_view().unwrap()), -0.5));
    }
    fn seated(look: SeatLook) -> Controls {
        let mut c = Controls::default();
        c.set_ride(Some(Ride::Seat(glam::Quat::IDENTITY, look)));
        c
    }
    fn mouse(c: &mut Controls, yaw: f32, up: f32) {
        c.action(&GameAction::Look { yaw, pitch: -up });
    }
    fn third_person(c: &mut Controls) {
        c.action(&GameAction::ToggleFirstPerson { fast: true });
        c.advance_view(0.01);
    }
    /// A passenger has no control object, so the whole move reaches the
    /// head: the mouse pitches it and it stays; only Free Look turns it, and
    /// that turn eases back after, in first and third person alike.
    #[test]
    fn a_passenger_turns_on_the_seat_and_looks_up_and_down_freely() {
        let mut c = seated(SeatLook::Passenger);
        mouse(&mut c, 0.5, 0.4);
        // The mouse turns the whole body (`mRot.z`), sent relative to the seat.
        assert!(close(c.view_angles().0, 0.5), "{:?}", c.view_angles());
        assert!(close(c.passenger_turn(), 0.5));
        assert!(close(c.movement().yaw, 0.5));
        assert!(close(c.movement().head_yaw, 0.0));
        assert!(close(c.view_angles().1, 0.4));
        c.advance_head(1.0);
        assert!(
            close(c.view_angles().1, 0.4),
            "the pitch never springs back"
        );
        assert!(close(c.passenger_turn(), 0.5), "nor does the body turn");
        // A full turn round the seat.
        for _ in 0..8 {
            mouse(&mut c, 1.0, 0.0);
        }
        assert!(close(c.passenger_turn(), wrap(8.5)));
        mouse(&mut c, -8.0, 0.0);
        assert!(close(c.passenger_turn(), 0.5));
        held(&mut c, HeldControl::FreeLook, true);
        mouse(&mut c, 5.0, 0.2);
        assert!(close(c.movement().head_yaw, MAX_FREELOOK));
        assert!(close(c.view_angles().1, 0.6));
        c.advance_head(1.0);
        assert!(
            close(c.movement().head_yaw, MAX_FREELOOK),
            "held while Free Look is"
        );
        held(&mut c, HeldControl::FreeLook, false);
        c.advance_head(0.032);
        assert!(
            close(c.movement().head_yaw, MAX_FREELOOK / 2.0),
            "halved a tick"
        );
        third_person(&mut c);
        c.advance_head(0.032);
        assert!(close(c.movement().head_yaw, MAX_FREELOOK / 4.0));
        assert!(close(c.view_angles().1, 0.6));
        assert!(
            close(c.passenger_turn(), 0.5),
            "Free Look turned only the head"
        );
        assert_eq!(c.driver_head_yaw(), None);
    }
    /// The Jeep's or Tank's driver: the strafe keys steer, so the move goes
    /// down the free-look path and the mouse turns and pitches the head
    /// without Free Look; nothing returns, and the chase camera swings with
    /// the turn.
    #[test]
    fn a_strafe_driver_looks_round_with_the_mouse_and_it_stays() {
        let mut c = seated(SeatLook::StrafeDriver);
        mouse(&mut c, 0.7, -0.3);
        assert!(close(c.movement().head_yaw, 0.7));
        assert!(close(c.view_angles().1, -0.3));
        c.advance_head(1.0);
        assert!(close(c.movement().head_yaw, 0.7));
        assert!(close(c.view_angles().1, -0.3));
        assert_eq!(c.driver_head_yaw(), Some(c.movement().head_yaw));
    }
    /// A mouse driver's mouse steers and leaves the head alone; Free Look
    /// moves the head, which springs back after in either view.
    #[test]
    fn a_mouse_driver_steers_and_free_look_springs_back() {
        let mut c = seated(SeatLook::MouseDriver);
        c.set_invert_prefs(false, false);
        c.set_vehicle_view(Some((0.0, 0.0)));
        let before = c.view_angles();
        mouse(&mut c, 0.3, 0.2);
        assert_ne!(c.movement().pitch, 0.0, "the mouse steers");
        assert!(close(c.view_angles().0, before.0), "the turn only steers");
        assert!(close(c.view_angles().1, before.1), "and so does the pitch");
        held(&mut c, HeldControl::FreeLook, true);
        mouse(&mut c, 0.6, 0.4);
        held(&mut c, HeldControl::FreeLook, false);
        c.advance_head(0.064);
        assert!(close(c.movement().head_yaw, 0.15));
        assert!(close(c.view_angles().1, 0.2), "{:?}", c.view_angles());
        // Max, v0.2.10: in a Jeep's chase camera, letting go of Z left the
        // view where he had looked.
        third_person(&mut c);
        held(&mut c, HeldControl::FreeLook, true);
        mouse(&mut c, 1.0, 0.2);
        let looked = c.driver_head_yaw().unwrap();
        assert!(close(looked, 1.15));
        c.advance_head(1.0);
        assert!(close(c.driver_head_yaw().unwrap(), looked), "held with Z");
        held(&mut c, HeldControl::FreeLook, false);
        c.advance_head(0.032);
        assert!(close(c.movement().head_yaw, looked / 2.0), "halved a tick");
        c.advance_head(1.0);
        assert!(c.driver_head_yaw().unwrap().abs() < 1e-3, "back ahead");
        assert!(c.body_pitch().abs() < 1e-3, "pitch too");
    }
    /// Max, v0.1.7: pulling the Stunt Plane up in first person, the view
    /// looked up first and the plane caught up after; in third person the
    /// pilot's body nodded with the mouse. The head took each move's pitch
    /// ahead of the plane. Now the mouse only steers: the view stays on the
    /// seat, in first and third person, however the mouse flicks.
    #[test]
    fn a_mouse_drivers_view_never_leads_the_vehicle() {
        for third in [false, true] {
            let mut c = seated(SeatLook::MouseDriver);
            c.set_invert_prefs(false, false);
            c.set_vehicle_view(Some((0.0, 0.0)));
            if third {
                third_person(&mut c);
            }
            let frame = 1.0 / 144.0;
            for i in 0..144 {
                if i % 7 == 0 {
                    mouse(&mut c, 0.05, if i < 72 { 0.3 } else { -0.2 });
                }
                c.advance_head(frame);
                let view = c.ride_view().unwrap();
                assert!(
                    view.angle_between(glam::Quat::IDENTITY) < 1e-6,
                    "{third} {i}"
                );
                assert_eq!(c.body_pitch(), 0.0, "the pilot never nods");
            }
            assert_ne!(c.movement().pitch, 0.0, "yet it steered");
        }
    }
    #[test]
    fn a_seated_driver_aims_where_the_tilted_view_looks() {
        let mut c = Controls::default();
        let seat = banked(0.9, 0.3);
        c.set_ride(Some(Ride::Seat(seat, SeatLook::StrafeDriver)));
        let input = c.movement();
        let forward = seat * glam::Vec3::NEG_Z;
        assert!(close(input.yaw, forward.x.atan2(-forward.z)));
        assert!(close(input.pitch, forward.y.asin()));
        // The head's turn travels as `head_yaw`; the body stays on the seat.
        c.action(&GameAction::Look {
            yaw: 0.8,
            pitch: 0.0,
        });
        let looking = c.movement();
        assert!(close(looking.yaw, input.yaw) && close(looking.head_yaw, 0.8));
        // Stepping off keeps the head's pitch for the body.
        c.action(&GameAction::Look {
            yaw: 0.0,
            pitch: -0.3,
        });
        c.set_ride(None);
        assert!(close(c.pitch, 0.3));
    }
    #[test]
    fn free_look_in_a_mouse_steered_vehicle_leaves_the_steering_alone() {
        let mut c = Controls::default();
        c.set_ride(Some(Ride::Seat(
            glam::Quat::IDENTITY,
            SeatLook::MouseDriver,
        )));
        c.set_vehicle_view(Some((0.0, 0.0)));
        c.action(&GameAction::Look {
            yaw: 0.2,
            pitch: 0.1,
        });
        let steering = c.movement();
        held(&mut c, HeldControl::FreeLook, true);
        c.action(&GameAction::Look {
            yaw: 0.6,
            pitch: -0.4,
        });
        let input = c.movement();
        assert_eq!((input.yaw, input.pitch), (steering.yaw, steering.pitch));
        assert!(close(input.head_yaw, 0.6));
        // The head looks round the cockpit instead: right and up.
        let (yaw, pitch) = c.view_angles();
        assert!(yaw > 0.5 && pitch > 0.4, "{yaw} {pitch}");
    }
    #[test]
    fn a_gunner_view_rides_the_hull_and_aims_relative_to_it() {
        let mut c = Controls::default();
        // The hull climbs a ramp facing -Z; the gunner looks 0.4 right.
        let hull = glam::Quat::from_rotation_x(0.3);
        c.set_ride(Some(Ride::Hull(hull)));
        c.action(&GameAction::Look {
            yaw: 0.4,
            pitch: 0.0,
        });
        let view = c.ride_view().unwrap();
        let expected = hull * glam::Quat::from_rotation_y(-0.4);
        assert!(view.angle_between(expected) < 1e-4);
        // Never springs back: the turret is a player.
        c.advance_head(1.0);
        assert!(c.ride_view().unwrap().angle_between(expected) < 1e-4);
    }
    #[test]
    fn vehicle_mouse_invert_replaces_invert_mouse_while_mouse_steering() {
        let steer = |mouse: bool, vehicle: bool| {
            let mut c = Controls::default();
            c.set_invert_prefs(mouse, vehicle);
            c.set_vehicle_view(Some((0.0, 0.0)));
            // The mouse moves up (OS y shrinks). The UI has already
            // applied Invert Mouse to the raw pitch.
            let raw = 0.1;
            let input = if mouse { -raw } else { raw };
            c.action(&GameAction::Look {
                yaw: 0.0,
                pitch: -input,
            });
            c.pitch
        };
        // v20 defaults: mouse up steers positive (`$mvPitch -= ...` with
        // the invert on), which the vehicle turns nose down, whatever
        // Invert Mouse says.
        assert!(steer(false, true) > 0.0);
        assert_eq!(steer(false, true), steer(true, true));
        assert_eq!(steer(false, false), -steer(false, true));
        assert_eq!(steer(true, false), steer(false, false));
    }
    fn scope() -> bri_weapons::Zoom {
        serde_json::from_value(serde_json::json!({
            "fov": 22, "on_jet": true, "jets": false, "crosshair": false,
            "first_person": true, "levels": [10], "sensitivity": 0.5,
            "overlay": "scope/scope",
            "sway": {"degrees": 1.0, "seconds": 4.0, "crouched": 0.25, "moving": 2.0}
        }))
        .unwrap()
    }
    fn settle(c: &mut Controls) {
        for _ in 0..120 {
            c.advance_zoom(1.0 / 60.0);
            c.advance_view(1.0 / 60.0);
        }
    }
    #[test]
    fn a_magnified_aim_zooms_by_tangent_from_the_players_own_view() {
        let mut zoom = scope();
        zoom.magnification = Some(2.0);
        for (normal, expected) in [(90.0_f32, 53.130_1), (70.0, 38.590_7)] {
            let mut c = Controls::default();
            c.set_fov_prefs(normal, 10.0);
            held(&mut c, HeldControl::Jet, true);
            c.set_aim_in_state(Some(zoom.clone()), Some("Ready"));
            settle(&mut c);
            assert!(
                (c.fov() - expected).abs() < 0.01,
                "{normal}: {} not {expected}",
                c.fov()
            );
        }
        // The wheel's further levels keep their own fixed fields of view.
        let mut c = Controls::default();
        held(&mut c, HeldControl::Jet, true);
        c.set_aim_in_state(Some(zoom), Some("Ready"));
        assert_eq!(c.zoom_level(), 1, "the scope's own step");
        assert!(c.aim_wheel(1));
        assert_eq!(c.zoom_level(), 2, "the wheel's step");
        settle(&mut c);
        assert!((c.fov() - 10.0).abs() < 0.01);
    }
    fn restricted_scope() -> bri_weapons::Zoom {
        let mut profile = scope();
        profile.allowed_states = Some(vec!["Ready".into(), "Fire".into(), "WaitRelease".into()]);
        profile
    }
    #[test]
    fn restricted_scope_gates_visuals_but_retains_right_button_ownership() {
        let mut c = Controls {
            third_person: true,
            ..Default::default()
        };
        held(&mut c, HeldControl::Jet, true);
        c.set_aim_in_state(Some(restricted_scope()), Some("Ready"));
        settle(&mut c);
        assert!(c.aiming() && !c.third_person_view());
        assert!(c.aim_wheel(1));
        for state in ["Lower", "Hold", "Raise", "Reload", "Draw", "unknown"] {
            c.set_aim_in_state(Some(restricted_scope()), Some(state));
            settle(&mut c);
            assert!(!c.aiming() && c.scope_overlay().is_none());
            assert!(!c.aim_hides_crosshair() && !c.aim_takes_wheel() && !c.aim_wheel(1));
            assert_eq!(c.zoom_level(), 0, "no level while barred: {state}");
            assert_eq!(c.aim_sensitivity(), 1.0);
            assert_eq!(c.fov(), 90.0);
            assert!(c.third_person_view());
            assert!(!c.movement().jet, "barred scope still owns RMB: {state}");
            assert_eq!(c.aim_level, 1, "temporary gating keeps wheel magnification");
        }
        for state in ["READY", "Fire", "WaitRelease"] {
            c.set_aim_in_state(Some(restricted_scope()), Some(state));
            settle(&mut c);
            assert!(c.aiming() && c.scope_overlay().is_some());
            assert_eq!(c.fov(), 10.0, "held intent resumes the retained wheel step");
            assert!(!c.movement().jet);
        }
        let mut jetting = restricted_scope();
        jetting.jets = true;
        c.set_aim_in_state(Some(jetting), Some("Hold"));
        assert!(c.movement().jet && !c.aiming());
    }
    #[test]
    fn restricted_scope_without_current_state_fails_closed_for_old_setter() {
        let mut c = Controls::default();
        held(&mut c, HeldControl::Jet, true);
        c.set_aim(Some(restricted_scope()));
        settle(&mut c);
        assert!(!c.aiming() && !c.movement().jet && c.scope_overlay().is_none());
        assert_eq!(c.fov(), 90.0);
        c.set_aim_in_state(Some(restricted_scope()), None);
        assert!(!c.aiming());
        let mut barred = scope();
        barred.allowed_states = Some(vec![]);
        c.set_aim_in_state(Some(barred), Some("Ready"));
        assert!(!c.aiming());
    }
    #[test]
    fn barred_keyboard_zoom_cannot_fall_back_to_generic_zoom() {
        let mut c = Controls::default();
        held(&mut c, HeldControl::Zoom, true);
        c.set_aim_in_state(Some(restricted_scope()), Some("Hold"));
        settle(&mut c);
        assert_eq!(c.fov(), 90.0);
        c.set_aim_in_state(Some(restricted_scope()), Some("Ready"));
        settle(&mut c);
        assert_eq!(c.fov(), 22.0);
        c.set_aim(None);
        settle(&mut c);
        assert_eq!(c.fov(), 10.0, "no-profile generic zoom is unchanged");
        c.set_aim(Some(scope()));
        settle(&mut c);
        assert_eq!(
            c.fov(),
            22.0,
            "omitted eligibility is legacy even without state"
        );
    }
    #[test]
    fn barred_scope_eases_existing_sway_out_and_back_in() {
        let mut c = Controls::default();
        held(&mut c, HeldControl::Jet, true);
        c.set_aim_in_state(Some(restricted_scope()), Some("Ready"));
        swayed(&mut c, 120);
        assert!(c.sway.gain > 0.0);
        c.set_aim_in_state(Some(restricted_scope()), Some("Hold"));
        swayed(&mut c, 120);
        assert_eq!(c.sway.gain, 0.0);
        assert!(c.sway.applied.0.abs() < 1e-6 && c.sway.applied.1.abs() < 1e-6);
        c.set_aim_in_state(Some(restricted_scope()), Some("Ready"));
        swayed(&mut c, 120);
        assert!(c.sway.gain > 0.0);
    }
    /// The mouse wheel steps a scope's magnification while it is aimed,
    /// and the tools otherwise; the step is kept while the weapon is held
    /// and forgotten with it.
    #[test]
    fn a_scope_zooms_in_steps_on_the_wheel() {
        let mut c = Controls::default();
        c.set_aim(Some(scope()));
        assert!(
            !c.aim_takes_wheel() && !c.aim_wheel(1),
            "not aiming: the tools"
        );
        held(&mut c, HeldControl::Jet, true);
        settle(&mut c);
        assert_eq!(c.fov(), 22.0);
        assert!(c.aim_takes_wheel());
        assert!(c.aim_wheel(1));
        settle(&mut c);
        assert_eq!(c.fov(), 10.0);
        assert!(c.aim_wheel(3), "further in stays at the last step");
        settle(&mut c);
        assert_eq!(c.fov(), 10.0);
        held(&mut c, HeldControl::Jet, false);
        settle(&mut c);
        assert_eq!(c.fov(), 90.0);
        held(&mut c, HeldControl::Jet, true);
        settle(&mut c);
        assert_eq!(c.fov(), 10.0, "the step is kept while it is held");
        assert!(c.aim_wheel(-1));
        settle(&mut c);
        assert_eq!(c.fov(), 22.0);
        c.aim_wheel(1);
        c.set_aim(None);
        c.set_aim(Some(scope()));
        settle(&mut c);
        assert_eq!(c.fov(), 22.0, "put away and drawn again, it starts wide");
    }
    /// Looking through the scope turns at its `sensitivity` on top of the
    /// field of view's own slowing, and its jet only aims.
    #[test]
    fn a_scope_sets_look_speed_and_keeps_jet() {
        let mut c = Controls::default();
        c.set_aim(Some(scope()));
        held(&mut c, HeldControl::Jet, true);
        settle(&mut c);
        assert!(!c.movement().jet, "the right mouse button only aims");
        assert_eq!(c.scope_overlay(), Some("scope/scope"));
        assert!(c.aim_hides_crosshair());
        let before = c.yaw;
        c.action(&GameAction::Look {
            yaw: 0.1,
            pitch: 0.0,
        });
        let turned = c.yaw - before;
        let expected = 0.1 * 22.0 / 90.0 * 0.5;
        assert!((turned - expected).abs() < 1e-6, "{turned} {expected}");
        // A scope that lets its holder jet, and no scope at all, jet.
        let mut jetting = scope();
        jetting.jets = true;
        c.set_aim(Some(jetting));
        assert!(c.movement().jet);
        c.set_aim(None);
        assert!(c.movement().jet);
        assert_eq!(c.scope_overlay(), None);
    }
    /// An aim assist on a body slows look by its `slowdown`, and look is
    /// whole again once the aim leaves it.
    #[test]
    fn an_aim_assist_slows_look_while_on_a_body() {
        let c = &mut Controls::default();
        let turn = |c: &mut Controls| {
            let before = c.yaw;
            c.action(&GameAction::Look {
                yaw: 0.1,
                pitch: 0.0,
            });
            c.yaw - before
        };
        let whole = turn(c);
        c.set_assist_slowdown(0.4);
        assert!((turn(c) - whole * 0.4).abs() < 1e-6);
        c.set_assist_slowdown(1.0);
        assert!((turn(c) - whole).abs() < 1e-6);
    }
    /// A third-person player aiming a first-person scope sees its picture
    /// only once the camera is in the eye.
    #[test]
    fn the_scope_picture_waits_for_the_eye() {
        let mut c = Controls {
            third_person: true,
            ..Default::default()
        };
        settle(&mut c);
        c.set_aim(Some(scope()));
        held(&mut c, HeldControl::Jet, true);
        assert_eq!(c.scope_overlay(), None);
        settle(&mut c);
        assert_eq!(c.scope_overlay(), Some("scope/scope"));
    }
    fn swayed(c: &mut Controls, frames: usize) -> Vec<(f32, f32)> {
        (0..frames)
            .map(|_| {
                c.advance_sway(1.0 / 60.0);
                (c.yaw, c.pitch)
            })
            .collect()
    }
    /// The aim drifts along its figure of eight while aimed, the same on
    /// every run, steadier crouched, and comes back to where the mouse left
    /// it once the aim ends.
    #[test]
    fn a_scope_sways_and_settles() {
        let mut c = Controls::default();
        c.set_aim(Some(scope()));
        held(&mut c, HeldControl::Jet, true);
        let path = swayed(&mut c, 600);
        let widest = path.iter().map(|(y, _)| y.abs()).fold(0.0, f32::max);
        assert!(
            (widest - 1f32.to_radians()).abs() < 0.02f32.to_radians(),
            "{} degrees",
            widest.to_degrees()
        );
        let mut again = Controls::default();
        again.set_aim(Some(scope()));
        held(&mut again, HeldControl::Jet, true);
        assert_eq!(swayed(&mut again, 600), path, "deterministic");
        held(&mut c, HeldControl::Crouch, true);
        let crouched = swayed(&mut c, 600)[300..]
            .iter()
            .map(|(y, _)| y.abs())
            .fold(0.0, f32::max);
        assert!(
            (crouched - 0.25f32.to_radians()).abs() < 0.02f32.to_radians(),
            "{} degrees crouched",
            crouched.to_degrees()
        );
        held(&mut c, HeldControl::Crouch, false);
        held(&mut c, HeldControl::Jet, false);
        swayed(&mut c, 120);
        assert!(
            c.yaw.abs() < 1e-5 && c.pitch.abs() < 1e-5,
            "{} {}",
            c.yaw,
            c.pitch
        );
        // The mouse still turns freely under it.
        held(&mut c, HeldControl::Jet, true);
        let turned = 0.4 * c.fov() / 90.0 * 0.5; // the view and the scope's look speed
        c.action(&GameAction::Look {
            yaw: 0.4,
            pitch: 0.0,
        });
        swayed(&mut c, 97);
        held(&mut c, HeldControl::Jet, false);
        swayed(&mut c, 120);
        assert!((c.yaw - turned).abs() < 1e-5, "{} {}", c.yaw, turned);
    }
    /// Seated, driving or observing, the sway leaves the view alone.
    #[test]
    fn a_seated_scope_does_not_sway() {
        let mut c = Controls::default();
        c.set_mounted(true);
        c.set_aim(Some(scope()));
        held(&mut c, HeldControl::Jet, true);
        swayed(&mut c, 300);
        assert_eq!((c.yaw, c.pitch), (0.0, 0.0));
    }
}

#[cfg(test)]
mod camera_shake_tests {
    use super::*;

    #[test]
    fn a_camera_shake_jitters_the_view_within_its_bounds_and_fades_without_touching_the_aim() {
        let mut c = Controls::default();
        let aim = (c.yaw, c.pitch);
        c.start_camera_shake(0.4, 0.1, 1.0, 1.0);
        let mut peak = 0.0f32;
        let mut frames = 0;
        while !c.camera_shakes.is_empty() && frames < 1000 {
            c.advance_camera_shakes(1.0 / 120.0);
            let j = c.camera_jolt();
            assert!(
                j.offset[0].abs() <= 0.4 && j.offset[1].abs() <= 0.4,
                "{j:?}"
            );
            assert!(
                j.yaw.abs() <= 0.1 && j.pitch.abs() <= 0.1 && j.roll.abs() <= 0.1,
                "{j:?}"
            );
            peak = peak.max(j.offset[0].abs());
            frames += 1;
        }
        assert!(peak > 0.01, "it moves the view");
        assert!(
            c.camera_shakes.is_empty() && frames >= 119,
            "it ends after its second"
        );
        c.advance_camera_shakes(1.0 / 120.0);
        assert_eq!(
            c.camera_jolt(),
            CameraJolt::default(),
            "nothing left once it is over"
        );
        assert_eq!((c.yaw, c.pitch), aim, "the aim is not moved");
    }

    #[test]
    fn broken_numbers_start_nothing_and_a_hold_keeps_the_shake_longer() {
        let mut c = Controls::default();
        c.start_camera_shake(f32::NAN, 0.1, 1.0, 1.0);
        c.start_camera_shake(0.1, 0.1, 0.0, 1.0);
        assert!(c.camera_shakes.is_empty());
        let straight = CameraShake {
            translation: 1.0,
            rotation: 0.0,
            seconds: 1.0,
            hold: 1.0,
            left: 0.5,
        };
        let held = CameraShake {
            hold: 3.0,
            ..straight
        };
        assert!((straight.share() - 0.5).abs() < 1e-6);
        assert!((held.share() - 0.875).abs() < 1e-6);
    }
}

#[cfg(test)]
mod view_shake_tests {
    use super::*;

    #[test]
    fn a_view_shake_goes_out_to_its_peak_comes_back_slower_and_leaves_the_minimum() {
        let mut c = Controls::default();
        c.start_view_shake(0.0, 0.04, 0.3, 4.0);
        let mut peak = 0.0f32;
        let mut steps = 0;
        while c.view_shake.is_some() && steps < 10_000 {
            c.advance_view_shake(1.0 / 120.0);
            peak = peak.max(c.pitch);
            steps += 1;
        }
        assert!(c.view_shake.is_none(), "it ends");
        assert!(
            (0.3..0.4).contains(&peak),
            "out to about the peak, not past a frame's step: {peak}"
        );
        assert!(
            (0.03..0.1).contains(&c.pitch),
            "the aim is left off by about the minimum: {}",
            c.pitch
        );
        assert!(c.yaw.abs() < 1e-6, "direction 0 turns pitch only");
    }

    #[test]
    fn a_sideways_shake_turns_the_yaw() {
        let mut c = Controls::default();
        c.start_view_shake(std::f32::consts::FRAC_PI_2, 0.0, 0.2, 4.0);
        for _ in 0..600 {
            c.advance_view_shake(1.0 / 120.0);
        }
        assert!(
            c.yaw.abs() < 0.02 && c.pitch.abs() < 0.02,
            "back to where it was with min 0"
        );
    }
}

#[cfg(test)]
mod breath_tests {
    use super::*;

    fn scoped() -> Controls {
        let mut c = Controls::default();
        let zoom: bri_weapons::Zoom = serde_json::from_value(serde_json::json!({
            "fov": 40.0,
            "breath": { "exhale": "t:out", "inhale": "t:in", "period": 4.0 }
        }))
        .expect("zoom");
        c.set_aim_in_state(Some(zoom), None);
        c
    }

    #[test]
    fn breath_alternates_each_half_period_only_while_aiming() {
        let mut c = scoped();
        // Not aiming: nothing, however long.
        for _ in 0..600 {
            assert!(c.advance_breath(1.0 / 60.0).is_none());
        }
        c.action(&GameAction::Held {
            control: HeldControl::Zoom,
            down: true,
        });
        let mut heard = vec![];
        for _ in 0..(60 * 10) {
            heard.extend(c.advance_breath(1.0 / 60.0));
        }
        assert!(
            heard.windows(2).all(|w| w[0] != w[1]),
            "out and in alternate: {heard:?}"
        );
        assert!(
            (4..=6).contains(&heard.len()),
            "a turn every two seconds: {heard:?}"
        );
        c.let_go(HeldControl::Zoom);
        for _ in 0..600 {
            assert!(c.advance_breath(1.0 / 60.0).is_none(), "stops with the aim");
        }
    }
}
