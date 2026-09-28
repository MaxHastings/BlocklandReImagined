//! Named slots packages fill. A slot is either composable (every package's
//! `add` runs) or exclusive (at most one package may `replace` it). Conflicts
//! come from these declarations, never from load order.
use crate::manifest::SlotMode;

#[derive(Debug, Clone, Copy)]
pub struct SlotSpec {
    pub slot: &'static str,
    /// Modes this slot accepts.
    pub modes: &'static [SlotMode],
    pub kind: &'static str,
    pub meaning: &'static str,
}

pub const SLOTS: &[SlotSpec] = &[SlotSpec {
    slot: "game.mode",
    modes: &[SlotMode::Replace],
    kind: "behaviour",
    meaning: "the server's game mode: at most one package runs the rules of the game",
}];

pub fn find(slot: &str) -> Option<&'static SlotSpec> {
    SLOTS.iter().find(|spec| spec.slot == slot)
}

pub fn names() -> Vec<&'static str> {
    SLOTS.iter().map(|spec| spec.slot).collect()
}
