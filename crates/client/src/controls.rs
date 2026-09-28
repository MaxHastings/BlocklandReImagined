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
    /// `$pref::Input::MouseInvert` (already applied to look input) and
    /// `$Pref::Input::VehicleMouseInvert`, which replaces it while driving a
    /// mouse-steered vehicle without free look (`pitch()` in v20).
    mouse_invert: bool,
    /// `VehicleMouseInvert` turned off (v20 defaults it on).
    vehicle_mouse_plain: bool,
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
    /// Orbit a package entity the player drives (`ControlObject::Entity`):
    /// the moves go to the entity, steered by this camera's yaw.
    Drive(u64),
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
        } else if self.held(HeldControl::FreeLook) {
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
            self.yaw = wrap(self.yaw + yaw);
            self.pitch = wrap_half(self.pitch + pitch);
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
            ControlObject::Entity(entity) => ObserverMode::Drive(entity),
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
            ObserverMode::Orbit(_) | ObserverMode::Drive(_) => None,
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
            None => (self.yaw, self.pitch),
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
            self.zoom_fov = Some(zoom.clamp(ZOOM_FOV_RANGE.0, ZOOM_FOV_RANGE.1));
        }
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
    fn target_fov(&self) -> f32 {
        if self.held(HeldControl::Zoom) {
            self.zoom_fov.unwrap_or(10.0)
        } else {
            self.normal_fov.unwrap_or(90.0)
        }
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
        self.camera_pos = if self.third_person {
            (self.camera_pos + step).min(1.0)
        } else {
            (self.camera_pos - step).max(0.0)
        };
    }
    /// How far the camera is out toward third person, 0 to 1.
    pub fn camera_pos(&self) -> f32 {
        self.camera_pos
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
            archetype: Default::default(),
            scale: 1.0,
            energy: 100.0,
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
