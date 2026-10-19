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

/// A see-through `color` (RGBA 0..1) drawn over a player's body, just
/// outside it, fading over `seconds`: a shield taking a hit, a power-up's
/// glow, a cloak shimmering. Every player sees it. Presentation only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyFlash {
    pub player: u64,
    pub color: [f32; 4],
    pub seconds: f32,
}
impl ScriptOp for BodyFlash {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "body_flash";
    fn bounded(&self) -> bool {
        self.color
            .iter()
            .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
            && self.seconds.is_finite()
            && (0.05..=60.0).contains(&self.seconds)
    }
}

/// Show one of an Add-On's weapons-pack shells (`effects.shells`) over a
/// player's body at `strength` (0 to 1), fading to nothing over `seconds`: a
/// shield taking a hit. A new call restarts it. Every player sees it.
/// Presentation only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyShell {
    pub player: u64,
    pub shell: String,
    pub strength: f32,
    pub seconds: f32,
}
impl ScriptOp for BodyShell {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "body_shell";
    fn bounded(&self) -> bool {
        !self.shell.is_empty()
            && self.shell.len() <= 160
            && !self.shell.chars().any(char::is_control)
            && self.strength.is_finite()
            && (0.0..=1.0).contains(&self.strength)
            && self.seconds.is_finite()
            && (0.05..=60.0).contains(&self.seconds)
    }
}

/// Spawn one of an Add-On's effects (a weapons pack's `effects` explosion)
/// at a point, facing out along `direction` (a unit vector, a surface
/// normal), `scale` times its size: a hit spark, a shield ripple, a puff of
/// dust. Presentation only; every player near sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectAt {
    pub effect: String,
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub scale: f32,
}
impl ScriptOp for EffectAt {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "effect_at";
    fn bounded(&self) -> bool {
        let EffectAt {
            effect,
            position,
            direction,
            scale,
        } = self;
        !effect.is_empty()
            && effect.len() <= 128
            && !effect.chars().any(char::is_control)
            && finite(position)
            && position.iter().all(|c| c.abs() <= 1_000_000.0)
            && finite(direction)
            && (direction.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 0.001
            && scale.is_finite()
            && (0.01..=100.0).contains(scale)
    }
}

/// The sound a map teleporter plays at the ears of each player it sends
/// through (`None` for silence, the default). Applies to the map being
/// played; a new map starts with none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetTeleportSound {
    pub profile: Option<String>,
}
impl ScriptOp for SetTeleportSound {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "set_teleport_sound";
    fn bounded(&self) -> bool {
        self.profile.as_ref().is_none_or(|p| {
            !p.is_empty() && p.len() <= 128 && !p.chars().any(char::is_control)
        })
    }
}

/// Play a music track (a sound an Add-On marks `music`, or one of the
/// game's music loops) at a player's ears on the music bus, or stop it
/// (`track` none), fading over `fade` seconds. For one player, or for
/// everyone (`player` none), joiners included until it is stopped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Music {
    pub player: Option<u64>,
    pub track: Option<String>,
    pub looping: bool,
    pub fade: f32,
}
impl ScriptOp for Music {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "play_music";
    fn bounded(&self) -> bool {
        let Music { track, fade, .. } = self;
        track.as_ref().is_none_or(|t| {
            !t.is_empty() && t.len() <= 128 && !t.chars().any(char::is_control)
        }) && fade.is_finite()
            && (0.0..=MAX_MUSIC_FADE).contains(fade)
    }
}
/// The longest fade in or out `play_music` and `stop_music` take, in seconds.
pub const MAX_MUSIC_FADE: f32 = 30.0;

/// Play a looping ambient bed (wind, a canyon's hum, a generator) under a
/// key, on the effects bus apart from music: at the player's ears, or
/// placed in the world at `at`, heard out to `range` units. Another play
/// under the same key replaces it; `track` none stops it. For one player,
/// or for everyone (`player` none), joiners included until stopped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ambient {
    pub player: Option<u64>,
    pub key: String,
    pub track: Option<String>,
    pub volume: f32,
    pub fade: f32,
    pub at: Option<[f32; 3]>,
    pub range: f32,
}
impl ScriptOp for Ambient {
    const CAPABILITY: &str = "effects";
    const NAME: &str = "play_ambient";
    fn bounded(&self) -> bool {
        let Ambient {
            key,
            track,
            volume,
            fade,
            at,
            range,
            ..
        } = self;
        let name = |t: &str| !t.is_empty() && t.len() <= 128 && !t.chars().any(char::is_control);
        name(key)
            && key.len() <= 32
            && track.as_deref().is_none_or(name)
            && (0.0..=1.0).contains(volume)
            && (0.0..=MAX_MUSIC_FADE).contains(fade)
            && at.is_none_or(|p| p.iter().all(|v| v.is_finite() && v.abs() <= 1.0e6))
            && (1.0..=MAX_AMBIENT_RANGE).contains(range)
    }
}
/// The farthest a placed ambient sound carries, in units.
pub const MAX_AMBIENT_RANGE: f32 = 2000.0;
/// Most ambient keys playing for everyone at once.
pub const MAX_AMBIENT: usize = 16;

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
