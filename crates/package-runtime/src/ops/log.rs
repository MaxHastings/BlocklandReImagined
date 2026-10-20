//! Operations behind the `log` capability: a rules script writing a named
//! line to the host's game log, or reporting a problem that also shows in
//! the in-game Add-On problems list.
use super::*;

/// Longest a log channel name may be.
pub const MAX_LOG_NAME: usize = 32;

/// A log channel name: 1 to 32 letters, digits, `.`, `_` or `-`.
fn channel(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= MAX_LOG_NAME
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// One line in the game log, headed by the Add-On and `name`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Log {
    pub name: String,
    pub text: String,
}
impl ScriptOp for Log {
    const CAPABILITY: &str = "log";
    const NAME: &str = "log";
    fn bounded(&self) -> bool {
        let Log { name, text } = self;
        channel(name) && chat(text)
    }
}

/// A warning: in the game log like `log`, and in the Add-On problems list
/// (told to admins, once a minute per Add-On and `name`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warn {
    pub name: String,
    pub text: String,
}
impl ScriptOp for Warn {
    const CAPABILITY: &str = "log";
    const NAME: &str = "warn";
    fn bounded(&self) -> bool {
        let Warn { name, text } = self;
        channel(name) && chat(text)
    }
}
