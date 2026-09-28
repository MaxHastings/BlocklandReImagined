//! Key bindings: stock defaults, lookup, remapping and brick key repeat.
//!
//! Commands keep their original script names (`moveforward`, `plantBrick`,
//! `escapeMenu.toggle();`) because they are the join key between the stock
//! default binds, the Options remap list and saved settings. The UI turns
//! them into typed [`crate::api::GameAction`]s / UI behaviour in `ui.rs`.

use crate::api::{BindEntry, BindInput};
use crate::input::{Chord, Key, Modifiers};
use crate::schema::{BindAtom, DefaultBind, UiData};
use std::collections::BTreeMap;

/// Mouse types (`defaultControlsGui`): 0 one button, 1 two button,
/// 2 two button + wheel (default), 3 tilt wheel (hidden option).
pub const DEFAULT_MOUSE: u8 = 2;
/// Keyboard types: 0 standard (numpad, default), 1 laptop.
pub const DEFAULT_KEYBOARD: u8 = 0;

/// The platform the default binds are evaluated for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    Linux,
}

fn holds(atom: &BindAtom, mouse: u8, keyboard: u8, platform: Platform) -> bool {
    match atom {
        BindAtom::Mouse(m) => *m == mouse,
        BindAtom::Keyboard(k) => *k == keyboard,
        // Linux has no v20 client; it uses the Windows branches (documented
        // product choice: same keys as the reference platform).
        BindAtom::Windows => platform != Platform::MacOs,
        BindAtom::DebugBuild => false,
    }
}

fn applies(b: &DefaultBind, mouse: u8, keyboard: u8, platform: Platform) -> bool {
    b.when
        .iter()
        .all(|(a, pol)| holds(a, mouse, keyboard, platform) == *pol)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemapOutcome {
    /// Bound (or already bound to this command).
    Bound,
    /// The input is bound to another remappable command; ask
    /// "… is already bound to …! Do you want to undo this mapping?"
    Conflict { other: String },
    /// The input is bound to a command outside the remap list.
    NotRemappable { other: String },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BindMap {
    pub entries: Vec<BindEntry>,
}

impl BindMap {
    /// `defaultControlsGui::apply` for the chosen hardware (c:15311–15488).
    pub fn defaults(data: &UiData, mouse: u8, keyboard: u8, platform: Platform) -> BindMap {
        let mut m = BindMap::default();
        for b in &data.default_binds {
            if !applies(b, mouse, keyboard, platform) {
                continue;
            }
            if let Some(input) = BindInput::parse(b.device, &b.key) {
                m.bind(input, &b.command);
            }
        }
        m
    }

    /// `GlobalActionMap` binds (console, fullscreen, help).
    pub fn globals(data: &UiData, platform: Platform) -> BindMap {
        let mut m = BindMap::default();
        for b in &data.global_binds {
            if applies(b, DEFAULT_MOUSE, DEFAULT_KEYBOARD, platform)
                && let Some(input) = BindInput::parse(b.device, &b.key)
            {
                m.bind(input, &b.command);
            }
        }
        m
    }

    /// `ActionMap::bind`: an input maps to one command (rebinding replaces).
    pub fn bind(&mut self, input: BindInput, command: &str) {
        self.entries.retain(|e| e.input != input);
        self.entries.push(BindEntry {
            command: command.to_string(),
            input,
        });
    }

    pub fn unbind_input(&mut self, input: &BindInput) {
        self.entries.retain(|e| &e.input != input);
    }

    pub fn unbind_command(&mut self, command: &str) {
        self.entries
            .retain(|e| !e.command.eq_ignore_ascii_case(command));
    }

    pub fn command_for(&self, input: &BindInput) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| &e.input == input)
            .map(|e| e.command.as_str())
    }

    /// Key lookup with Torque's modifier fallback: an exact chord first, then
    /// the bare key (so Shift held for crouch does not block `w`). Modifier
    /// keys themselves match without modifiers.
    pub fn command_for_key(&self, key: Key, mods: Modifiers) -> Option<&str> {
        if key.is_modifier() {
            return self.command_for(&BindInput::Key(Chord::plain(key)));
        }
        self.command_for(&BindInput::Key(Chord { mods, key }))
            .or_else(|| self.command_for(&BindInput::Key(Chord::plain(key))))
    }

    /// First binding of a command (`moveMap.getBinding`).
    pub fn binding_of(&self, command: &str) -> Option<BindInput> {
        self.entries
            .iter()
            .find(|e| e.command.eq_ignore_ascii_case(command))
            .map(|e| e.input)
    }

    /// Options → Controls key text: the Torque action text upper-cased, like
    /// `buildFullMapString` (`NUMPAD8`, `CTRL Z`, `MOUSE2`).
    pub fn display(&self, command: &str) -> String {
        match self.binding_of(command) {
            Some(BindInput::Key(c)) => c.torque().to_ascii_uppercase(),
            Some(BindInput::Mouse(b)) => {
                let n = match b {
                    crate::input::MouseButton::Left => 1,
                    crate::input::MouseButton::Right => 2,
                    crate::input::MouseButton::Middle => 3,
                    crate::input::MouseButton::Back => 4,
                    crate::input::MouseButton::Forward => 5,
                };
                format!("MOUSE{n}")
            }
            Some(BindInput::Wheel) => "ZAXIS".into(),
            Some(BindInput::MouseX) => "XAXIS".into(),
            Some(BindInput::MouseY) => "YAXIS".into(),
            None => String::new(),
        }
    }

    /// `OptRemapInputCtrl::onInputEvent` (c:3460). `remappable` is the remap
    /// command list. On `Bound` the command's previous binding is replaced
    /// (see README "Product choices": v20 left the old key bound too).
    pub fn remap(
        &mut self,
        command: &str,
        input: BindInput,
        remappable: &[String],
    ) -> RemapOutcome {
        match self.command_for(&input).map(str::to_string) {
            Some(prev) if prev.eq_ignore_ascii_case(command) => RemapOutcome::Bound,
            Some(prev) => {
                if remappable.iter().any(|c| c.eq_ignore_ascii_case(&prev)) {
                    RemapOutcome::Conflict { other: prev }
                } else {
                    RemapOutcome::NotRemappable { other: prev }
                }
            }
            None => {
                self.force_remap(command, input);
                RemapOutcome::Bound
            }
        }
    }

    /// "Yes" on the conflict prompt (`redoMapping`): the input moves to `command`.
    pub fn force_remap(&mut self, command: &str, input: BindInput) {
        self.unbind_command(command);
        self.bind(input, command);
    }
}

/// Brick key repeat (`shiftBrickAway`/`repeatBrickAway`, c:3972–4290): the
/// action fires on key down, repeats after 200 ms and then every 50 ms while
/// held. Each command has a counter; release (or a super-shift toggle)
/// bumps it, which cancels the pending repeat exactly like `$brickAway++`.
#[derive(Debug, Clone, Default)]
pub struct Repeater {
    counters: BTreeMap<String, u32>,
    pending: Vec<(u64, String, u32)>,
    pub first_ms: u64,
    pub repeat_ms: u64,
}

impl Repeater {
    pub fn new(first_ms: u64, repeat_ms: u64) -> Self {
        Repeater {
            first_ms,
            repeat_ms,
            ..Default::default()
        }
    }
    pub fn press(&mut self, command: &str, now: u64) {
        let c = self.counters.entry(command.to_string()).or_default();
        *c = (*c + 1) % 1000;
        self.pending
            .push((now + self.first_ms, command.to_string(), *c));
    }
    pub fn release(&mut self, command: &str) {
        let c = self.counters.entry(command.to_string()).or_default();
        *c = (*c + 1) % 1000;
    }
    pub fn cancel_all(&mut self) {
        let keys: Vec<String> = self.counters.keys().cloned().collect();
        for k in keys {
            self.release(&k);
        }
        self.pending.clear();
    }
    /// Commands whose repeat fires at or before `now` (in order).
    pub fn due(&mut self, now: u64) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(i) = self
            .pending
            .iter()
            .enumerate()
            .filter(|(_, p)| p.0 <= now)
            .min_by_key(|(_, p)| p.0)
            .map(|(i, _)| i)
        {
            let (at, cmd, token) = self.pending.remove(i);
            if self.counters.get(&cmd) == Some(&token) {
                out.push(cmd.clone());
                self.pending.push((at + self.repeat_ms, cmd, token));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::MouseButton;
    use crate::schema::Device;

    fn bind(key: &str, cmd: &str, when: Vec<(BindAtom, bool)>) -> DefaultBind {
        DefaultBind {
            device: Device::Keyboard,
            key: key.into(),
            command: cmd.into(),
            when,
            source_line: 0,
        }
    }

    #[test]
    fn defaults_follow_hardware_conditions() {
        let data = UiData {
            default_binds: vec![
                bind("w", "moveforward", vec![]),
                bind(
                    "numpad8",
                    "shiftBrickAway",
                    vec![(BindAtom::Keyboard(0), true)],
                ),
                bind("i", "shiftBrickAway", vec![(BindAtom::Keyboard(0), false)]),
                bind("lalt", "toggleSuperShift", vec![(BindAtom::Windows, true)]),
                bind("ropt", "toggleSuperShift", vec![(BindAtom::Windows, false)]),
                DefaultBind {
                    device: Device::Mouse,
                    key: "button1".into(),
                    command: "Jet".into(),
                    when: vec![(BindAtom::Mouse(0), false)],
                    source_line: 0,
                },
            ],
            ..Default::default()
        };
        let std = BindMap::defaults(&data, 2, 0, Platform::Windows);
        assert_eq!(
            std.command_for_key(Key::Numpad(8), Modifiers::NONE),
            Some("shiftBrickAway")
        );
        assert_eq!(std.command_for_key(Key::Letter('i'), Modifiers::NONE), None);
        assert_eq!(
            std.command_for_key(
                Key::LAlt,
                Modifiers {
                    alt: true,
                    ..Modifiers::NONE
                }
            ),
            Some("toggleSuperShift")
        );
        assert_eq!(
            std.command_for(&BindInput::Mouse(MouseButton::Right)),
            Some("Jet")
        );
        let laptop = BindMap::defaults(&data, 0, 1, Platform::MacOs);
        assert_eq!(
            laptop.command_for_key(Key::Letter('i'), Modifiers::NONE),
            Some("shiftBrickAway")
        );
        assert_eq!(
            laptop.command_for_key(Key::ROpt, Modifiers::NONE),
            Some("toggleSuperShift")
        );
        assert_eq!(
            laptop.command_for(&BindInput::Mouse(MouseButton::Right)),
            None
        );
        // Shift held (crouch) still walks forward.
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            std.command_for_key(Key::Letter('w'), shift),
            Some("moveforward")
        );
    }

    #[test]
    fn remap_conflicts_and_replacement() {
        let mut m = BindMap::default();
        m.bind(
            BindInput::Key(Chord::plain(Key::Letter('w'))),
            "moveforward",
        );
        m.bind(BindInput::Key(Chord::plain(Key::Letter('b'))), "openBSD");
        m.bind(BindInput::Key(Chord::plain(Key::Tilde)), "toggleConsole");
        let remap = vec!["moveforward".to_string(), "openBSD".to_string()];
        let up = BindInput::Key(Chord::plain(Key::Up));
        assert_eq!(m.remap("moveforward", up, &remap), RemapOutcome::Bound);
        assert_eq!(m.display("moveforward"), "UP");
        assert_eq!(m.command_for_key(Key::Letter('w'), Modifiers::NONE), None);
        let b = BindInput::Key(Chord::plain(Key::Letter('b')));
        assert_eq!(
            m.remap("moveforward", b, &remap),
            RemapOutcome::Conflict {
                other: "openBSD".into()
            }
        );
        m.force_remap("moveforward", b);
        assert_eq!(m.command_for(&b), Some("moveforward"));
        assert_eq!(m.binding_of("openBSD"), None);
        let t = BindInput::Key(Chord::plain(Key::Tilde));
        assert_eq!(
            m.remap("openBSD", t, &remap),
            RemapOutcome::NotRemappable {
                other: "toggleConsole".into()
            }
        );
    }

    #[test]
    fn brick_repeat_timing_and_cancel() {
        let mut r = Repeater::new(200, 50);
        r.press("shiftBrickAway", 0);
        assert!(r.due(199).is_empty());
        assert_eq!(r.due(200), vec!["shiftBrickAway"]);
        assert_eq!(r.due(260).len(), 1); // 250
        assert_eq!(r.due(355).len(), 2); // 300, 350
        r.release("shiftBrickAway");
        assert!(r.due(1000).is_empty());
        // Quick tap: no repeat.
        r.press("plantBrick", 1000);
        r.release("plantBrick");
        assert!(r.due(1300).is_empty());
    }
}
