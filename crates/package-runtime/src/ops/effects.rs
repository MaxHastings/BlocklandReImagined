//! Operations behind the `effects` capability.
use super::*;

/// Outline a box for one player while `tool` is in their hand (a
/// selection, a zone being marked); `None` takes it away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowBox {
    pub player: u64,
    pub area: Option<([f32; 3], [f32; 3])>,
    pub tool: String,
}
impl ScriptOp for ShowBox {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "hide_box";
    fn bounded(&self) -> bool {
        let ShowBox { area, tool, .. } = self;
        match area {
            Some((min, max)) => item(tool) && span(min, max),
            None => tool.is_empty(),
        }
    }
}

/// Draw the set of boxes named `key` for every player (joiners too),
/// replacing the set drawn under that key; none takes it away. A set
/// with an `owner` goes when that player leaves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowShapes {
    pub owner: Option<u64>,
    pub key: String,
    pub shapes: Vec<WorldShape>,
}
impl ScriptOp for ShowShapes {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "show_shapes";
    fn bounded(&self) -> bool {
        let ShowShapes { key, shapes, .. } = self;
        shape_key(key) && shapes.len() <= MAX_SHAPES && shapes.iter().all(WorldShape::check)
    }
}

/// Play a sound profile (an Add-On weapons pack's `sounds`, or v20's):
/// at `position` for everyone near, or at one player's ears.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sound {
    pub profile: String,
    pub at: SoundAt,
}
impl ScriptOp for Sound {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "play_sound";
    fn bounded(&self) -> bool {
        let Sound { profile, at } = self;
        !profile.is_empty()
            && profile.len() <= 128
            && !profile.chars().any(char::is_control)
            && match at {
                SoundAt::Position(p) => finite(p),
                SoundAt::Player(_) => true,
            }
    }
}

/// A straight beam from `from` to `to` for `seconds`, fading out: a
/// tracer, a laser, a bolt. With `muzzle`, clients start it at that
/// player's held muzzle as they draw it. Presentation only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Beam {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub color: [f32; 4],
    pub width: f32,
    pub seconds: f32,
    pub muzzle: Option<u64>,
}
impl ScriptOp for Beam {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "beam";
    fn bounded(&self) -> bool {
        let Beam {
            from,
            to,
            color,
            width,
            seconds,
            ..
        } = self;
        let span = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        finite(from)
            && finite(to)
            && glam_length(&span) <= MAX_RAY_RANGE
            && color.iter().all(|c| (0.0..=1.0).contains(c))
            && width.is_finite()
            && *width > 0.0
            && *width <= MAX_BEAM_WIDTH
            && seconds.is_finite()
            && *seconds > 0.0
            && *seconds <= MAX_BEAM_SECONDS
    }
}

/// Play an animation on one of a player's four script threads
/// (`playThread`): 0 and 1 the body, 2 the arms with what they hold, 3 a
/// gesture; `root` stops it. `after` seconds later when above 0, as
/// `%player.schedule(ms, "playThread", ...)` did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayThread {
    pub player: u64,
    pub thread: u8,
    pub sequence: String,
    pub after: f32,
}
impl ScriptOp for PlayThread {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "play_thread";
    fn bounded(&self) -> bool {
        let PlayThread {
            thread,
            sequence,
            after,
            ..
        } = self;
        *thread <= 3
            && after.is_finite()
            && (0.0..=MAX_THREAD_DELAY).contains(after)
            && !sequence.is_empty()
            && sequence.len() <= 64
            && sequence
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    }
}
