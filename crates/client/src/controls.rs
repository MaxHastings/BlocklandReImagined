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

#[derive(Default)]
pub struct Controls {
    held: BTreeSet<HeldControl>,
    pub yaw: f32,
    pub pitch: f32,
    /// Held free look turns the head, not the body (`mHead.z`).
    free_yaw: f32,
    pub third_person: bool,
    /// `$pref::Player::defaultFov`; `None` is v20's 90.
    normal_fov: Option<f32>,
    /// Target zoom FOV (`$Pref::player::CurrentFOV`); the wheel steps it.
    zoom_fov: Option<f32>,
    /// The zoom FOV shown, easing toward `zoom_fov` so wheel steps glide.
    zoom_shown: f32,
    /// Eased zoom progress: 0 at the normal FOV, 1 fully zoomed.
    zoom: f32,
    /// The admin camera in control, if any. The body's `yaw`/`pitch` stay
    /// where they were left while it is active.
    observer: Option<Observer>,
    /// Driving a mouse-steered vehicle: the view follows the vehicle and
    /// the mouse only steers. `yaw`/`pitch` then carry the raw mouse turn,
    /// pitch wrapping every half turn, for the server's steering deltas.
    vehicle_view: Option<(f32, f32)>,
}
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
}
/// Zoom eases toward its target at this exponential rate (95% in 0.3 s),
/// in place of the engine's timed `setFov` transition.
const ZOOM_RATE: f32 = 10.0;
/// v20's wheel-zoom limits (`toggleZoomFOV`'s 5 and 85).
const ZOOM_FOV_RANGE: (f32, f32) = (5.0, 85.0);
/// Observer cameras stop just short of straight up or down.
const OBSERVER_PITCH: f32 = FRAC_PI_2 - 0.01;
fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}
fn wrap_half(a: f32) -> f32 {
    (a + FRAC_PI_2).rem_euclid(PI) - FRAC_PI_2
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
                if control == HeldControl::FreeLook && !down {
                    self.free_yaw = 0.0;
                }
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
            GameAction::ToggleFirstPerson { .. } => self.third_person = !self.third_person,
            _ => return false,
        }
        true
    }
    fn look(&mut self, yaw: f32, pitch: f32) {
        if let Some(observer) = &mut self.observer {
            observer.yaw = wrap(observer.yaw + yaw);
            observer.pitch = (observer.pitch + pitch).clamp(-OBSERVER_PITCH, OBSERVER_PITCH);
        } else if self.held(HeldControl::FreeLook) {
            // Only the turn is free; pitch still tilts the body's look
            // (`Player::updateMove` always adds pitch to `mHead.x`).
            self.free_yaw = (self.free_yaw + yaw).clamp(-MAX_FREELOOK, MAX_FREELOOK);
            self.pitch = (self.pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
        } else if self.vehicle_view.is_some() {
            self.yaw = wrap(self.yaw + yaw);
            self.pitch = wrap_half(self.pitch + pitch);
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
    /// `setLookLimits` while seated: keep the look pitch in `[low, high]`.
    pub fn limit_pitch(&mut self, low: f32, high: f32) {
        if self.vehicle_view.is_none() && low <= high {
            self.pitch = self.pitch.clamp(low, high);
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
                self.observer = None;
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
            ObserverMode::Orbit(_) => None,
        }
    }
    /// The orbited player's presented eye, which the orbit camera circles.
    pub fn orbit_focus(&self, presented: &BTreeMap<OwnerId, PlayerState>) -> Option<glam::Vec3> {
        match self.observer?.mode {
            ObserverMode::Orbit(target) => presented
                .get(&target)
                .map(|p| p.eye(&p.tuning())),
            ObserverMode::Free(_) => None,
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
        let up = u8::from(self.held.contains(&HeldControl::Jump)) as f32
            - u8::from(self.held.contains(&HeldControl::Crouch)) as f32;
        let speed = if self.held.contains(&HeldControl::Walk) {
            8.0
        } else {
            30.0
        };
        let direction = forward * self.axis(HeldControl::Forward, HeldControl::Backward)
            + right * self.axis(HeldControl::Right, HeldControl::Left)
            + glam::Vec3::Y * up;
        position += direction.normalize_or_zero() * speed * seconds.clamp(0.0, 0.1);
        if let Some(observer) = &mut self.observer {
            observer.mode = ObserverMode::Free(position);
        }
    }
    /// The body's move: the held controls, unless a camera has control, when
    /// the body stands still with the aim it was left with.
    pub fn movement(&self) -> MoveInput {
        if self.observer.is_some() {
            return MoveInput {
                yaw: self.yaw,
                pitch: self.pitch,
                ..Default::default()
            };
        }
        let walk = if self.held(HeldControl::Walk) {
            0.4
        } else {
            1.0
        };
        MoveInput {
            forward: self.axis(HeldControl::Forward, HeldControl::Backward) * walk,
            right: self.axis(HeldControl::Right, HeldControl::Left) * walk,
            yaw: self.yaw,
            pitch: self.pitch,
            head_yaw: self.free_yaw,
            jump: self.held(HeldControl::Jump),
            crouch: self.held(HeldControl::Crouch),
            jet: self.held(HeldControl::Jet),
        }
    }
    /// The body's head and eye direction, including held free-look.
    pub fn view_angles(&self) -> (f32, f32) {
        match self.vehicle_view {
            Some((yaw, pitch)) => (wrap(yaw + self.free_yaw), pitch),
            None => (wrap(self.yaw + self.free_yaw), self.pitch),
        }
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
            let zoom = zoom.clamp(ZOOM_FOV_RANGE.0, ZOOM_FOV_RANGE.1);
            self.zoom_fov = Some(zoom);
            self.zoom_shown = zoom;
        }
    }
    /// Ease the zoom toward whether Zoom is held, and the zoom FOV toward
    /// the wheel's choice; frame-rate independent.
    pub fn advance_zoom(&mut self, seconds: f32) {
        if !seconds.is_finite() {
            return;
        }
        let step = 1.0 - (-ZOOM_RATE * seconds.clamp(0.0, 0.25)).exp();
        let ease = |value: &mut f32, target: f32, done: f32| {
            *value += (target - *value) * step;
            if (target - *value).abs() < done {
                *value = target;
            }
        };
        let zooming = f32::from(u8::from(self.held(HeldControl::Zoom)));
        ease(&mut self.zoom, zooming, 1e-3);
        let zoom_fov = self.zoom_fov();
        if self.zoom == 0.0 || self.zoom_shown == 0.0 {
            self.zoom_shown = zoom_fov;
        } else {
            ease(&mut self.zoom_shown, zoom_fov, 1e-2);
        }
    }
    fn zoom_fov(&self) -> f32 {
        self.zoom_fov.unwrap_or(10.0)
    }
    /// The current horizontal FOV in degrees (Torque's `$cameraFov`),
    /// between the normal FOV and the zoom FOV as zoom eases in. Look
    /// sensitivity follows it through the transition.
    pub fn fov(&self) -> f32 {
        let normal = self.normal_fov.unwrap_or(90.0);
        let t = self.zoom * self.zoom * (3.0 - 2.0 * self.zoom);
        normal + (self.zoom_shown - normal) * t
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn held(c: &mut Controls, key: HeldControl, down: bool) {
        c.action(&GameAction::Held { control: key, down });
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
        let mut c = Controls::default();
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
        assert_eq!(c.movement().head_yaw, 0.0);
        assert_eq!(c.view_angles(), (c.yaw, c.pitch));
        c.set_fov_prefs(90.0, 45.0);
        held(&mut c, HeldControl::Zoom, true);
        assert_eq!(c.fov(), 90.0, "zoom eases in rather than snapping");
        c.advance_zoom(0.05);
        let partial = c.fov();
        assert!(partial < 90.0 && partial > 45.0, "{partial}");
        for _ in 0..60 {
            c.advance_zoom(1.0 / 60.0);
        }
        assert_eq!(c.fov(), 45.0);
        held(&mut c, HeldControl::Zoom, false);
        for _ in 0..4 {
            c.advance_zoom(0.25);
        }
        assert_eq!(c.fov(), 90.0);
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
        c.advance_zoom(1.0 / 60.0);
        let gliding = c.fov();
        assert!(gliding > 10.0 && gliding < 15.0, "{gliding}");
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
        held(&mut c, HeldControl::Jump, true);
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
        assert!(flown.y > 2.0, "jump flies the camera up");
        // A repeated grant keeps the camera where it was flown.
        c.follow(ControlObject::Camera, 1, Some(glam::Vec3::ZERO));
        assert_eq!(c.free_camera(), Some(flown));
        c.follow(ControlObject::Player, 1, None);
        assert_eq!(c.movement().forward, 1.0);
        assert_eq!(c.movement().yaw, body.yaw);
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
            datablock: Default::default(),
            scale: 1.0,
            energy: 100.0,
        };
        let mut presented = BTreeMap::from([(1, body(1, 0.0)), (7, body(7, 5.0))]);
        assert_eq!(c.orbit_focus(&presented), None);
        c.follow(ControlObject::Spy(7), 1, None);
        let first = c.orbit_focus(&presented).unwrap();
        assert_eq!(first.x, 5.0);
        presented.insert(7, body(7, 12.0));
        assert_eq!(c.orbit_focus(&presented).unwrap().x, 12.0);
        assert!(first.y > 1.0, "orbits the eye, not the feet");
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
}
