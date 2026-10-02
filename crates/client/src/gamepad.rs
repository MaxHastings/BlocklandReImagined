//! Gamepad play (not in v20, which was keyboard and mouse only). A pad drives
//! the same bound commands the keyboard does, so every rule that applies to a
//! key press applies to a button press:
//!
//! | Pad | Command |
//! |---|---|
//! | Left stick | move (forward, back, strafe) |
//! | Right stick | look |
//! | A / Cross | jump |
//! | B / Circle | crouch |
//! | X / Square | walk |
//! | Right trigger | fire |
//! | Left trigger | jet |
//! | Start | Escape menu |
//!
//! Only while playing: in menus the pad releases everything it holds.
use bri_ui::{api::GameAction, ui::Ui};
use std::collections::BTreeSet;

/// Stick travel ignored around the centre.
const DEAD_ZONE: f32 = 0.25;
/// Stick travel that counts as a movement key press.
const MOVE_THRESHOLD: f32 = 0.5;
/// Look speed at full right-stick travel, in the keyboard-turn units
/// (`KeyboardTurnSpeed` 0.5 turns this fast).
const LOOK_RATE: f32 = 0.5 / 0.032;

/// The pad's input this frame, independent of the device library.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PadState {
    pub left: [f32; 2],
    pub right: [f32; 2],
    pub south: bool,
    pub east: bool,
    pub west: bool,
    pub right_trigger: bool,
    pub left_trigger: bool,
    pub start: bool,
}

/// The held commands `pad` asks for.
pub fn held_commands(pad: &PadState) -> BTreeSet<&'static str> {
    let mut held = BTreeSet::new();
    let [x, y] = pad.left;
    for (on, command) in [
        (y > MOVE_THRESHOLD, "moveforward"),
        (y < -MOVE_THRESHOLD, "movebackward"),
        (x < -MOVE_THRESHOLD, "moveleft"),
        (x > MOVE_THRESHOLD, "moveright"),
        (pad.south, "jump"),
        (pad.east, "crouch"),
        (pad.west, "walk"),
        (pad.right_trigger, "mouseFire"),
        (pad.left_trigger, "jet"),
    ] {
        if on {
            held.insert(command);
        }
    }
    held
}

/// Look movement (yaw, pitch) for `dt_ms` of right-stick travel; pushing
/// up looks up (pitch follows mouse Y, which grows downwards).
pub fn look(pad: &PadState, dt_ms: u64) -> Option<(f32, f32)> {
    let axis = |v: f32| {
        if v.abs() < DEAD_ZONE {
            0.0
        } else {
            v.signum() * (v.abs() - DEAD_ZONE) / (1.0 - DEAD_ZONE)
        }
    };
    let (x, y) = (axis(pad.right[0]), axis(pad.right[1]));
    if x == 0.0 && y == 0.0 {
        return None;
    }
    let step = LOOK_RATE * dt_ms.min(100) as f32 / 1000.0;
    Some((x * step, -y * step))
}

pub struct Gamepads {
    gilrs: Option<gilrs::Gilrs>,
    held: BTreeSet<&'static str>,
    start: bool,
}

impl Gamepads {
    pub fn new() -> Self {
        let gilrs = match gilrs::Gilrs::new() {
            Ok(g) => Some(g),
            Err(error) => {
                bri_console::warn(format!("Gamepads unavailable: {error}"));
                None
            }
        };
        Self {
            gilrs,
            held: BTreeSet::new(),
            start: false,
        }
    }

    fn state(&mut self) -> Option<PadState> {
        use gilrs::{Axis, Button};
        let gilrs = self.gilrs.as_mut()?;
        while gilrs.next_event().is_some() {}
        let (_, pad) = gilrs.gamepads().find(|(_, p)| p.is_connected())?;
        Some(PadState {
            left: [pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY)],
            right: [pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)],
            south: pad.is_pressed(Button::South),
            east: pad.is_pressed(Button::East),
            west: pad.is_pressed(Button::West),
            right_trigger: pad.is_pressed(Button::RightTrigger2),
            left_trigger: pad.is_pressed(Button::LeftTrigger2),
            start: pad.is_pressed(Button::Start),
        })
    }

    /// Apply the pad to `ui` for this frame. `playing` is false in menus,
    /// dialogs and when the window is in the background.
    pub fn poll(&mut self, ui: &mut Ui, dt_ms: u64, focused: bool) {
        let pad = if focused { self.state() } else { None };
        let pad = pad.unwrap_or_default();
        let start = pad.start && !self.start;
        self.start = pad.start;
        if start && ui.core.in_game() {
            ui.core.run_command("escapeMenu.toggle();", true);
        }
        let playing =
            focused && ui.core.in_game() && ui.top_id() == bri_ui::screens::ScreenId::Play;
        let want = if playing {
            held_commands(&pad)
        } else {
            BTreeSet::new()
        };
        for command in self.held.difference(&want) {
            ui.core.run_command(command, false);
        }
        for command in want.difference(&self.held) {
            ui.core.run_command(command, true);
        }
        self.held = want;
        if playing && let Some((yaw, pitch)) = look(&pad, dt_ms) {
            ui.core.game(GameAction::Look { yaw, pitch });
        }
    }
}

impl Default for Gamepads {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticks_and_buttons_map_to_the_keyboard_commands() {
        let idle = PadState::default();
        assert!(held_commands(&idle).is_empty());
        assert_eq!(look(&idle, 16), None);
        let pad = PadState {
            left: [0.9, 0.8],
            right: [1.0, 0.1],
            south: true,
            right_trigger: true,
            ..Default::default()
        };
        let held: Vec<_> = held_commands(&pad).into_iter().collect();
        assert_eq!(held, ["jump", "mouseFire", "moveforward", "moveright"]);
        let (yaw, pitch) = look(&pad, 32).unwrap();
        assert!((yaw - 0.5).abs() < 1e-4, "{yaw}");
        assert_eq!(pitch, 0.0, "inside the dead zone");
        let up = PadState {
            right: [0.0, 1.0],
            ..Default::default()
        };
        assert!(look(&up, 32).unwrap().1 < 0.0, "pushing up looks up");
    }
}
