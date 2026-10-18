//! Operations behind the `movers` capability: starting and holding the
//! map's keyed movers (doors) from a package's rules.
use super::*;

/// Highest key a mover may be sent to (`bri_content::mover::MAX_KEYS`).
pub const MAX_MOVER_KEY: u8 = 15;
/// Longest a mover's travel may take, seconds.
pub const MAX_MOVER_SECONDS: f32 = 600.0;

/// Send the mover called `id` to key `key`, taking `seconds`. A mover
/// already on its way to another key, or between keys, is not redirected
/// (turning back the way it came is); the script reads `movers()` first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoverGoto {
    pub id: String,
    pub key: u8,
    pub seconds: f32,
}
impl ScriptOp for MoverGoto {
    const CAPABILITY: &str = "movers";
    const NAME: &str = "mover_goto";
    fn bounded(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= 64
            && self.key <= MAX_MOVER_KEY
            && self.seconds.is_finite()
            && (0.0..=MAX_MOVER_SECONDS).contains(&self.seconds)
    }
}

/// Hold the mover called `id` where it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoverStop {
    pub id: String,
}
impl ScriptOp for MoverStop {
    const CAPABILITY: &str = "movers";
    const NAME: &str = "mover_stop";
    fn bounded(&self) -> bool {
        !self.id.is_empty() && self.id.len() <= 64
    }
}
