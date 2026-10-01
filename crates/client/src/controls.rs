//! Client intentions and view angles; authoritative simulation owns positions.
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

#[derive(Clone, Default)]
pub struct Controls {
    held: BTreeSet<HeldControl>,
    pub yaw: f32,
    pub pitch: f32,
    /// Held free look turns the head, not the body (`mHead.z`).
    free_yaw: f32,
    pub third_person: bool,
    /// `$pref::Player::defaultFov`; `None` is v20's 90.
    normal_fov: Option<f32>,
    /// The host's `setControlCameraFov` (an Add-On's rules), in place of
    /// `normal_fov` until the host hands it back.
    server_fov: Option<f32>,
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
    /// The roll an opening in a floor or ceiling turned the view by, and
    /// how it turned the body's eye and camera pivot about its middle
    /// (upside down, for a floor onto a floor) past the turn of its
    /// heading, both easing back upright (`crate::portal_view`).
    portal_ease: Option<PortalEase>,
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
        let t = (self.seconds / PORTAL_EASE_SECONDS).clamp(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
    }
    fn roll(&self) -> f32 {
        self.roll * self.left()
    }
    fn tilt(&self) -> glam::Quat {
        glam::Quat::IDENTITY.slerp(self.tilt, self.left())
    }
}
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
    /// Magic Carpet, skis): the mouse steers; Free Look moves the head,
    /// which springs back after in first person (0x5aeae3, a control object
    /// of `VehicleObjectType`) and stays put in third.
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
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObserverMode {
    /// `Observer` fly mode, flown locally from `dropCameraAtPlayer`.
    Free(glam::Vec3),
    /// `Corpse` orbit mode around a spied player, or around one's own body
    /// after death.
    Orbit(OwnerId),
    /// Orbit a package entity the player drives (`ControlObject::Entity`):
    /// the moves go to the entity, steered by this camera's yaw.
    Drive(u64),
    /// A rule's path camera (`ControlObject::Path`), at this point of its
    /// path; [`Controls::fly_path`] moves and turns it each frame.
    Path(glam::Vec3),
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
    let pitch = forward.y.clamp(-1.0, 1.0).asin();
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
                // `getMouseAdjustAmount`: sensitivity × `$cameraFov` / 90.
                let scale = self.fov() / 90.0;
                // OS mouse Y increases downwards; simulation pitch increases up.
                self.look(yaw * scale, -pitch * scale);
            }
            GameAction::SetZoomFov { fov } if fov.is_finite() => {
                self.zoom_fov = Some(fov.clamp(ZOOM_FOV_RANGE.0, ZOOM_FOV_RANGE.1))
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
            observer.yaw = wrap(observer.yaw + yaw);
            observer.pitch = (observer.pitch + pitch).clamp(-OBSERVER_PITCH, OBSERVER_PITCH);
        } else if let Some(look) = self.seat_look()
            && (look == SeatLook::StrafeDriver || self.held(HeldControl::FreeLook))
        {
            // Free looking hands the rider the whole turn and the vehicle
            // none (`Player::processTick` 0x5b2df7 zeroes the vehicle's yaw
            // and pitch), and `pitch()` goes back to Invert Mouse.
            self.free_yaw = (self.free_yaw + yaw).clamp(-MAX_FREELOOK, MAX_FREELOOK);
            self.head_pitch = (self.head_pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
            self.driver_head = HeadTicks::at(self.head_pitch);
        } else if self.free_looking() {
            // Only the turn is free; pitch still tilts the body's look
            // (`Player::updateMove` always adds pitch to `mHead.x`).
            self.free_yaw = (self.free_yaw + yaw).clamp(-MAX_FREELOOK, MAX_FREELOOK);
            self.pitch = (self.pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
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
            self.head_pitch = (self.head_pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
        } else if self.seated() {
            self.head_pitch = (self.head_pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
        } else if self.seat_yaw.is_some() {
            self.pitch = (self.pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
        } else {
            self.yaw = wrap(self.yaw + yaw);
            self.pitch = (self.pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
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
    fn seat_look(&self) -> Option<SeatLook> {
        match self.ride {
            Some(Ride::Seat(_, look)) => Some(look),
            _ => None,
        }
    }
    /// Ease a seated head back as v20 does, halving it every 32 ms tick
    /// (`Player::updateMove`, blocklandv20.exe 0x5aeae3): without Free Look
    /// a passenger's turn returns; a mouse driver's turn and pitch return in
    /// first person only (`isFirstPerson`: the camera fully in; a vehicle's
    /// driver in third person skips both halvings). A strafe driver's head
    /// never returns.
    pub fn advance_head(&mut self, seconds: f32) {
        let Some(look) = self.seat_look() else {
            // Off a vehicle seat the head's turn halves every tick unless
            // it is free looking (the else branch of 0x5aea5f).
            if seconds.is_finite() && !self.free_looking() {
                self.free_yaw *= 0.5f32.powf(seconds.clamp(0.0, 1.0) / HEAD_RETURN_TICK);
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
        let seconds = seconds.clamp(0.0, 1.0);
        let keep = 0.5f32.powf(seconds / HEAD_RETURN_TICK);
        match look {
            SeatLook::Passenger => self.free_yaw *= keep,
            SeatLook::MouseDriver => {
                let first_person = self.camera_pos == 0.0;
                if first_person {
                    self.free_yaw *= keep;
                }
                self.head_pitch = self.driver_head.advance(seconds, first_person);
            }
            _ => {}
        }
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
                    * Quat::from_rotation_x(self.pitch.clamp(-FRAC_PI_2, FRAC_PI_2))
            }
        })
    }
    /// Turn the look the whole way an opening's carry turned the body: it
    /// sees the same view from the far side, its pitch and any roll
    /// included. The body stays upright, so its eye and camera pivot end up
    /// the other side of its middle from where the carry takes them through
    /// a floor or ceiling: [`Self::portal_tilt`] puts them there. The roll
    /// and tilt then ease out (see [`Self::ease_roll`]).
    pub fn carry_look(&mut self, carry: &glam::Affine3A) {
        let look = (wrap(self.yaw + self.free_yaw), self.pitch, self.portal_roll());
        let (yaw, pitch, roll) = crate::portal_view::carried_look(look, carry);
        let turn = glam::Quat::from_mat3a(&carry.matrix3).normalize();
        let before = self.yaw;
        self.yaw = wrap(yaw - self.free_yaw);
        self.pitch = pitch.clamp(-FRAC_PI_2, FRAC_PI_2);
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
            ease.seconds += seconds.clamp(0.0, 0.25);
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
            ControlObject::Camera => match self.observer {
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
            ControlObject::Corpse => ObserverMode::Orbit(owner),
            ControlObject::Entity(entity) => ObserverMode::Drive(entity),
            ControlObject::Path => match self.observer {
                Some(Observer {
                    mode: ObserverMode::Path(_),
                    ..
                }) => return,
                _ => ObserverMode::Path(eye.unwrap_or_default()),
            },
        };
        if let Some(observer) = &mut self.observer {
            observer.mode = mode;
        } else {
            let (yaw, pitch) = self.view_angles();
            self.free_yaw = 0.0;
            self.observer = Some(Observer {
                mode,
                yaw,
                pitch: pitch.clamp(-OBSERVER_PITCH, OBSERVER_PITCH),
            });
        }
    }
    pub fn observer(&self) -> Option<Observer> {
        self.observer
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
            ObserverMode::Orbit(_) | ObserverMode::Drive(_) | ObserverMode::Path(_) => None,
        }
    }
    /// The orbited player's presented eye, or the driven entity's head,
    /// which the orbit camera circles.
    pub fn orbit_focus(
        &self,
        presented: &BTreeMap<OwnerId, PlayerState>,
        archetypes: &bri_sim::archetype::Archetypes,
        entities: &BTreeMap<u64, bri_sim::session::EntityInfo>,
    ) -> Option<glam::Vec3> {
        match self.observer?.mode {
            ObserverMode::Orbit(target) => presented.get(&target).map(|p| archetypes.eye(p)),
            ObserverMode::Drive(entity) => entities
                .get(&entity)
                .map(|e| glam::Vec3::from(e.position) + glam::Vec3::Y * 1.5),
            ObserverMode::Free(_) | ObserverMode::Path(_) => None,
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
    /// Fly the free camera with the movement keys.
    pub fn fly(&mut self, seconds: f32) {
        let Some(Observer {
            mode: ObserverMode::Free(mut position),
            yaw,
            pitch,
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
        position += direction * self.fly_speed() * seconds.clamp(0.0, 0.1);
        if let Some(observer) = &mut self.observer {
            observer.mode = ObserverMode::Free(position);
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
            forward: self.axis(HeldControl::Forward, HeldControl::Backward) * walk,
            right: self.axis(HeldControl::Right, HeldControl::Left) * walk,
            yaw,
            pitch,
            head_yaw: self.free_yaw,
            jump: self.held(HeldControl::Jump),
            crouch: self.held(HeldControl::Crouch),
            jet: self.held(HeldControl::Jet),
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
        (wrap(forward.x.atan2(-forward.z) + self.free_yaw), self.pitch)
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
        self.normal_fov = normal.is_finite().then(|| normal.clamp(5.0, 140.0));
        if self.zoom_fov.is_none() && zoom.is_finite() {
            self.zoom_fov = Some(zoom.clamp(ZOOM_FOV_RANGE.0, ZOOM_FOV_RANGE.1));
        }
    }
    /// The field of view the host sets, or `None` for the player's own. It
    /// glides like any other FOV change.
    pub fn set_server_fov(&mut self, fov: Option<f32>) {
        self.server_fov = fov
            .filter(|f| f.is_finite())
            .map(|f| f.clamp(*bri_package_runtime::ops::FOV_RANGE.start(), *bri_package_runtime::ops::FOV_RANGE.end()));
    }
    /// Ramp the shown FOV toward the zoom FOV while Zoom is held, else the
    /// normal FOV. Wheel steps and the options slider ride the same ramp.
    pub fn advance_zoom(&mut self, seconds: f32) {
        if !seconds.is_finite() {
            return;
        }
        let target = self.target_fov();
        let shown = self.fov_shown.get_or_insert(target);
        let step = ZOOM_DEGREES_PER_SECOND * seconds.clamp(0.0, 0.25);
        *shown += (target - *shown).clamp(-step, step);
    }
    /// The held weapon's aim, or `None` when it has none: holding Zoom (or
    /// Jet, when the aim is `on_jet`) then aims at its FOV in place of the
    /// wheel's zoom.
    pub fn set_aim(&mut self, aim: Option<bri_weapons::Zoom>) {
        self.aim = aim.filter(|a| a.fov.is_finite());
    }
    /// Aiming down the held weapon's sights.
    pub fn aiming(&self) -> bool {
        self.aim.is_some_and(|a| {
            self.observer.is_none()
                && (self.held(HeldControl::Zoom) || (a.on_jet && self.held(HeldControl::Jet)))
        })
    }
    /// Aiming hides the crosshair when the aim says so.
    pub fn aim_hides_crosshair(&self) -> bool {
        self.aiming() && self.aim.is_some_and(|a| !a.crosshair)
    }
    /// Third person as the view shows it: aiming a `first_person` aim
    /// looks from the eye whatever the toggle says.
    pub fn third_person_view(&self) -> bool {
        self.third_person_only
            || (self.third_person && !(self.aiming() && self.aim.is_some_and(|a| a.first_person)))
    }
    /// `thirdPersonOnly`: while the body's archetype says so, the camera
    /// stays out behind it whatever the view toggle says.
    pub fn set_third_person_only(&mut self, only: bool) {
        self.third_person_only = only;
    }
    fn target_fov(&self) -> f32 {
        if let Some(aim) = self.aim.filter(|_| self.aiming()) {
            aim.fov.clamp(ZOOM_FOV_RANGE.0, ZOOM_FOV_RANGE.1)
        } else if self.held(HeldControl::Zoom) {
            self.zoom_fov.unwrap_or(10.0)
        } else {
            self.server_fov.or(self.normal_fov).unwrap_or(90.0)
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
        let step = speed * seconds.clamp(0.0, 0.25);
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
        c.fly(0.05);
        let flown = c.free_camera().unwrap();
        assert!(flown.y > 2.0, "looking up flies the camera up");
        // A repeated grant keeps the camera where it was flown.
        c.follow(ControlObject::Camera, 1, Some(glam::Vec3::ZERO));
        assert_eq!(c.free_camera(), Some(flown));
        c.follow(ControlObject::Player, 1, None);
        assert_eq!(c.movement().forward, 1.0);
        assert_eq!(c.movement().yaw, body.yaw);
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
            c.fly(0.1);
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
        c.fly(0.05);
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
            tick: Default::default(),
        };
        let mut presented = BTreeMap::from([(1, body(1, 0.0)), (7, body(7, 5.0))]);
        assert_eq!(
            c.orbit_focus(&presented, &Default::default(), &Default::default()),
            None
        );
        c.follow(ControlObject::Spy(7), 1, None);
        let first = c
            .orbit_focus(&presented, &Default::default(), &Default::default())
            .unwrap();
        assert_eq!(first.x, 5.0);
        presented.insert(7, body(7, 12.0));
        assert_eq!(
            c.orbit_focus(&presented, &Default::default(), &Default::default())
                .unwrap()
                .x,
            12.0
        );
        assert!(first.y > 1.0, "orbits the eye, not the feet");
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
        assert!(close(c.view_angles().1, 0.4), "the pitch never springs back");
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
        assert!(close(c.movement().head_yaw, MAX_FREELOOK), "held while Free Look is");
        held(&mut c, HeldControl::FreeLook, false);
        c.advance_head(0.032);
        assert!(close(c.movement().head_yaw, MAX_FREELOOK / 2.0), "halved a tick");
        third_person(&mut c);
        c.advance_head(0.032);
        assert!(close(c.movement().head_yaw, MAX_FREELOOK / 4.0));
        assert!(close(c.view_angles().1, 0.6));
        assert!(close(c.passenger_turn(), 0.5), "Free Look turned only the head");
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
    /// moves the head, which springs back after in first person only.
    #[test]
    fn a_mouse_driver_steers_and_free_look_springs_back_in_first_person() {
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
        third_person(&mut c);
        held(&mut c, HeldControl::FreeLook, true);
        mouse(&mut c, 0.0, 0.2);
        held(&mut c, HeldControl::FreeLook, false);
        c.advance_head(0.032);
        assert!(close(c.body_pitch(), 0.4), "no pitch return in third person");
        assert!(close(c.movement().head_yaw, 0.15), "nor a turn return");
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
                assert!(view.angle_between(glam::Quat::IDENTITY) < 1e-6, "{third} {i}");
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
        c.set_ride(Some(Ride::Seat(glam::Quat::IDENTITY, SeatLook::MouseDriver)));
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
}
