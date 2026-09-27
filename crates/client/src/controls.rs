//! Client intentions and view angles; authoritative simulation owns positions.
use bri_sim::player::MoveInput;
use bri_ui::api::{GameAction, HeldControl};
use std::{
    collections::BTreeSet,
    f32::consts::{FRAC_PI_2, PI},
};

#[derive(Default)]
pub struct Controls {
    held: BTreeSet<HeldControl>,
    pub yaw: f32,
    pub pitch: f32,
    free_yaw: f32,
    free_pitch: f32,
    pub third_person: bool,
    zoom_fov: Option<f32>,
    /// Admin observer camera (`dropCameraAtPlayer`): flies with the movement
    /// keys while the player stands still.
    pub free_camera: Option<glam::Vec3>,
}
fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}
impl Controls {
    pub fn held(&self, c: HeldControl) -> bool {
        self.held.contains(&c)
    }
    pub fn release(&mut self) {
        self.held.clear();
        self.free_yaw = 0.0;
        self.free_pitch = 0.0;
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
                    self.free_pitch = 0.0;
                }
            }
            GameAction::Look { yaw, pitch } => {
                if !yaw.is_finite() || !pitch.is_finite() {
                    return true;
                }
                let scale = self.fov(90.0) / 90.0;
                // OS mouse Y increases downwards; simulation pitch increases up.
                self.look(yaw * scale, -pitch * scale);
            }
            GameAction::SetZoomFov { fov } if fov.is_finite() => {
                self.zoom_fov = Some(fov.clamp(5.0, 85.0))
            }
            GameAction::ToggleFirstPerson { .. } => self.third_person = !self.third_person,
            _ => return false,
        }
        true
    }
    fn look(&mut self, yaw: f32, pitch: f32) {
        if self.held(HeldControl::FreeLook) {
            self.free_yaw = wrap(self.free_yaw + yaw);
            self.free_pitch =
                (self.free_pitch + pitch).clamp(-FRAC_PI_2 - self.pitch, FRAC_PI_2 - self.pitch);
        } else {
            self.yaw = wrap(self.yaw + yaw);
            self.pitch = (self.pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
        }
    }
    fn axis(&self, positive: HeldControl, negative: HeldControl) -> f32 {
        u8::from(self.held(positive)) as f32 - u8::from(self.held(negative)) as f32
    }
    /// Fly the observer camera; returns true while it is active.
    pub fn fly(&mut self, seconds: f32) -> bool {
        let Some(mut position) = self.free_camera else {
            return false;
        };
        if !seconds.is_finite() {
            return true;
        }
        let (yaw, pitch) = (self.yaw, self.pitch);
        let forward = glam::Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos());
        let right = glam::Vec3::new(yaw.cos(), 0.0, yaw.sin());
        let up = u8::from(self.held.contains(&HeldControl::Jump)) as f32
            - u8::from(self.held.contains(&HeldControl::Crouch)) as f32;
        let speed = if self.held.contains(&HeldControl::Walk) { 8.0 } else { 30.0 };
        let direction = forward * self.axis(HeldControl::Forward, HeldControl::Backward)
            + right * self.axis(HeldControl::Right, HeldControl::Left)
            + glam::Vec3::Y * up;
        position += direction.normalize_or_zero() * speed * seconds.clamp(0.0, 0.1);
        self.free_camera = Some(position);
        true
    }
    pub fn movement(&self) -> MoveInput {
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
            jump: self.held(HeldControl::Jump),
            crouch: self.held(HeldControl::Crouch),
            jet: self.held(HeldControl::Jet),
        }
    }
    pub fn view_angles(&self) -> (f32, f32) {
        (wrap(self.yaw + self.free_yaw), self.pitch + self.free_pitch)
    }
    pub fn fov(&self, normal: f32) -> f32 {
        if self.held(HeldControl::Zoom) {
            self.zoom_fov.unwrap_or(45.0)
        } else if normal.is_finite() {
            normal.clamp(5.0, 140.0)
        } else {
            90.0
        }
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
        held(&mut c, HeldControl::FreeLook, false);
        assert_eq!(c.view_angles(), (c.yaw, c.pitch));
        held(&mut c, HeldControl::Zoom, true);
        assert_eq!(c.fov(90.0), 45.0);
        held(&mut c, HeldControl::Zoom, false);
        assert_eq!(c.fov(90.0), 90.0);
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
