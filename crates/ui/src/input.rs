//! Platform-neutral input events and Torque key naming.
//!
//! The host (winit or a test) translates its events into [`InputEvent`]s. Key
//! names follow Torque's ActionMap spelling so the stock default binds from the
//! UI pack (`"ctrl z"`, `"shift-ctrl p"`, `"numpadenter"`) parse directly.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Key {
    /// `a`..=`z` (always lower case).
    Letter(char),
    /// Top-row digits 0..=9.
    Digit(u8),
    /// Numpad digits 0..=9.
    Numpad(u8),
    /// F1..=F12.
    F(u8),
    Escape,
    Return,
    NumpadEnter,
    Tab,
    Space,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    LShift,
    RShift,
    LControl,
    RControl,
    LAlt,
    RAlt,
    /// macOS option keys (`lopt`/`ropt`). Hosts on macOS report these instead
    /// of `LAlt`/`RAlt`; on other platforms they never occur.
    LOpt,
    ROpt,
    Tilde,
    Minus,
    Equals,
    LBracket,
    RBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Comma,
    Period,
    Slash,
    NumpadAdd,
    NumpadMinus,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    CapsLock,
    PrintScreen,
    Pause,
}

impl Key {
    /// Canonical Torque ActionMap name.
    pub fn torque_name(&self) -> String {
        match self {
            Key::Letter(c) => c.to_string(),
            Key::Digit(d) => d.to_string(),
            Key::Numpad(d) => format!("numpad{d}"),
            Key::F(n) => format!("f{n}"),
            other => NAMED
                .iter()
                .find(|(_, k)| k == other)
                .map(|(n, _)| (*n).to_string())
                .unwrap_or_default(),
        }
    }

    /// Parse a Torque key name (case-insensitive). Single punctuation
    /// characters used by stock binds are accepted too: `/`, `;`, and `+`
    /// (stock v20 binds `"+"` to the numpad plus key on Windows; see
    /// docs/research/ui-ux/02 §"Brick movement").
    pub fn from_torque(name: &str) -> Option<Key> {
        let n = name.to_ascii_lowercase();
        let mut chars = n.chars();
        if let (Some(c), None) = (chars.next(), chars.clone().next()) {
            return match c {
                'a'..='z' => Some(Key::Letter(c)),
                '0'..='9' => Some(Key::Digit(c as u8 - b'0')),
                '/' => Some(Key::Slash),
                ';' => Some(Key::Semicolon),
                ',' => Some(Key::Comma),
                '.' => Some(Key::Period),
                '-' => Some(Key::Minus),
                '=' => Some(Key::Equals),
                '[' => Some(Key::LBracket),
                ']' => Some(Key::RBracket),
                '\\' => Some(Key::Backslash),
                '\'' => Some(Key::Apostrophe),
                '`' | '~' => Some(Key::Tilde),
                '+' => Some(Key::NumpadAdd),
                '*' => Some(Key::NumpadMultiply),
                _ => None,
            };
        }
        if let Some(d) = n.strip_prefix("numpad").and_then(|d| d.parse::<u8>().ok()) {
            return (d <= 9).then_some(Key::Numpad(d));
        }
        if let Some(f) = n.strip_prefix('f').and_then(|d| d.parse::<u8>().ok()) {
            return (1..=24).contains(&f).then_some(Key::F(f));
        }
        if n == "enter" {
            // Torque accelerator spelling used by v20 dialogs.
            return Some(Key::Return);
        }
        NAMED.iter().find(|(s, _)| *s == n).map(|(_, k)| *k)
    }

    pub fn is_modifier(&self) -> bool {
        matches!(
            self,
            Key::LShift
                | Key::RShift
                | Key::LControl
                | Key::RControl
                | Key::LAlt
                | Key::RAlt
                | Key::LOpt
                | Key::ROpt
        )
    }

    /// Human label for the key-binding list (Options → Controls).
    pub fn label(&self) -> String {
        match self {
            Key::Letter(c) => c.to_ascii_uppercase().to_string(),
            Key::Digit(d) => d.to_string(),
            Key::Numpad(d) => format!("Numpad {d}"),
            Key::F(n) => format!("F{n}"),
            Key::NumpadEnter => "Numpad Enter".into(),
            Key::NumpadAdd => "Numpad +".into(),
            Key::NumpadMinus => "Numpad -".into(),
            Key::NumpadMultiply => "Numpad *".into(),
            Key::NumpadDivide => "Numpad /".into(),
            Key::NumpadDecimal => "Numpad .".into(),
            Key::Return => "Enter".into(),
            Key::Escape => "Escape".into(),
            Key::Space => "Space".into(),
            Key::Tab => "Tab".into(),
            Key::Tilde => "~".into(),
            Key::Minus => "-".into(),
            Key::Equals => "=".into(),
            Key::LBracket => "[".into(),
            Key::RBracket => "]".into(),
            Key::Backslash => "\\".into(),
            Key::Semicolon => ";".into(),
            Key::Apostrophe => "'".into(),
            Key::Comma => ",".into(),
            Key::Period => ".".into(),
            Key::Slash => "/".into(),
            Key::LShift => "Left Shift".into(),
            Key::RShift => "Right Shift".into(),
            Key::LControl => "Left Ctrl".into(),
            Key::RControl => "Right Ctrl".into(),
            Key::LAlt => "Left Alt".into(),
            Key::RAlt => "Right Alt".into(),
            Key::LOpt => "Left Option".into(),
            Key::ROpt => "Right Option".into(),
            Key::PageUp => "Page Up".into(),
            Key::PageDown => "Page Down".into(),
            other => {
                let n = other.torque_name();
                let mut c = n.chars();
                c.next()
                    .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            }
        }
    }
}

const NAMED: &[(&str, Key)] = &[
    ("escape", Key::Escape),
    ("return", Key::Return),
    ("numpadenter", Key::NumpadEnter),
    ("tab", Key::Tab),
    ("space", Key::Space),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("insert", Key::Insert),
    ("home", Key::Home),
    ("end", Key::End),
    ("pageup", Key::PageUp),
    ("pagedown", Key::PageDown),
    ("up", Key::Up),
    ("down", Key::Down),
    ("left", Key::Left),
    ("right", Key::Right),
    ("lshift", Key::LShift),
    ("rshift", Key::RShift),
    ("lcontrol", Key::LControl),
    ("rcontrol", Key::RControl),
    ("lalt", Key::LAlt),
    ("ralt", Key::RAlt),
    ("lopt", Key::LOpt),
    ("ropt", Key::ROpt),
    ("tilde", Key::Tilde),
    ("minus", Key::Minus),
    ("equals", Key::Equals),
    ("lbracket", Key::LBracket),
    ("rbracket", Key::RBracket),
    ("backslash", Key::Backslash),
    ("semicolon", Key::Semicolon),
    ("apostrophe", Key::Apostrophe),
    ("comma", Key::Comma),
    ("period", Key::Period),
    ("slash", Key::Slash),
    ("numpadadd", Key::NumpadAdd),
    ("numpadminus", Key::NumpadMinus),
    ("numpadmult", Key::NumpadMultiply),
    ("numpaddivide", Key::NumpadDivide),
    ("decimal", Key::NumpadDecimal),
    ("capslock", Key::CapsLock),
    ("printscreen", Key::PrintScreen),
    ("pause", Key::Pause),
];

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.torque_name())
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// macOS command key (Torque `cmd`).
    pub cmd: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        cmd: false,
    };
    pub fn is_empty(&self) -> bool {
        !(self.shift || self.ctrl || self.alt || self.cmd)
    }
    /// Torque bind prefix, e.g. `"shift-ctrl "` (empty when no modifiers).
    pub fn prefix(&self) -> String {
        let mut parts = Vec::new();
        if self.shift {
            parts.push("shift");
        }
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.cmd {
            parts.push("cmd");
        }
        if parts.is_empty() {
            String::new()
        } else {
            parts.join("-") + " "
        }
    }
    pub fn label_prefix(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s += "Ctrl+";
        }
        if self.cmd {
            s += "Cmd+";
        }
        if self.alt {
            s += "Alt+";
        }
        if self.shift {
            s += "Shift+";
        }
        s
    }
}

/// A key chord in bind form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Chord {
    pub mods: Modifiers,
    pub key: Key,
}

impl Chord {
    pub const fn plain(key: Key) -> Chord {
        Chord {
            mods: Modifiers::NONE,
            key,
        }
    }
    /// Parse Torque bind text: `"ctrl z"`, `"shift-ctrl p"`, `"alt numpad8"`, `"w"`.
    pub fn parse(s: &str) -> Option<Chord> {
        let s = s.trim();
        let (mods_s, key_s) = match s.rsplit_once(' ') {
            Some((m, k)) => (m, k),
            None => ("", s),
        };
        let mut mods = Modifiers::NONE;
        for m in mods_s.split(['-', ' ']).filter(|m| !m.is_empty()) {
            match m.to_ascii_lowercase().as_str() {
                "shift" => mods.shift = true,
                "ctrl" => mods.ctrl = true,
                "alt" | "opt" => mods.alt = true,
                "cmd" => mods.cmd = true,
                _ => return None,
            }
        }
        Some(Chord {
            mods,
            key: Key::from_torque(key_s)?,
        })
    }
    pub fn torque(&self) -> String {
        self.mods.prefix() + &self.key.torque_name()
    }
    pub fn label(&self) -> String {
        self.mods.label_prefix() + &self.key.label()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    pub fn torque_name(&self) -> &'static str {
        match self {
            MouseButton::Left => "button0",
            MouseButton::Right => "button1",
            MouseButton::Middle => "button2",
        }
    }
}

/// Host input, in physical window pixels (the UI divides by its scale).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    KeyDown {
        key: Key,
        mods: Modifiers,
        repeat: bool,
    },
    KeyUp {
        key: Key,
        mods: Modifiers,
    },
    /// Text input (already composed by the host).
    Char(char),
    MouseMove {
        x: f32,
        y: f32,
    },
    /// Raw relative motion while the cursor is hidden (mouse look).
    MouseDelta {
        dx: f32,
        dy: f32,
    },
    MouseDown {
        button: MouseButton,
        x: f32,
        y: f32,
    },
    MouseUp {
        button: MouseButton,
        x: f32,
        y: f32,
    },
    /// Wheel notches, positive = away from the user (up).
    Wheel {
        delta: f32,
    },
    /// Window lost focus: release everything held.
    FocusLost,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stock_bind_spellings() {
        assert_eq!(Chord::parse("w"), Some(Chord::plain(Key::Letter('w'))));
        let c = Chord::parse("shift-ctrl p").unwrap();
        assert!(c.mods.shift && c.mods.ctrl && !c.mods.alt);
        assert_eq!(c.key, Key::Letter('p'));
        assert_eq!(Chord::parse("ctrl E").unwrap().key, Key::Letter('e'));
        assert_eq!(Chord::parse("alt numpad8").unwrap().key, Key::Numpad(8));
        assert_eq!(Chord::parse("+").unwrap().key, Key::NumpadAdd);
        assert_eq!(Chord::parse("F9").unwrap().key, Key::F(9));
        assert_eq!(Chord::parse("numpadenter").unwrap().key, Key::NumpadEnter);
        assert_eq!(Chord::parse("ctrl comma").unwrap().key, Key::Comma);
        assert_eq!(Chord::parse("bogus q"), None);
        for k in [
            Key::Letter('q'),
            Key::Numpad(3),
            Key::F(11),
            Key::PageDown,
            Key::LBracket,
            Key::NumpadAdd,
        ] {
            assert_eq!(Key::from_torque(&k.torque_name()), Some(k));
        }
        assert_eq!(
            Chord::parse("shift-ctrl p").unwrap().torque(),
            "shift-ctrl p"
        );
        assert_eq!(Chord::parse("ctrl z").unwrap().label(), "Ctrl+Z");
    }
}
