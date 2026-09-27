use std::fmt;

/// Errors returned by pack loading and the game-facing runtime. None of them
/// panic the audio thread; a failed request simply produces no sound.
#[derive(Debug)]
pub enum AudioError {
    Io {
        path: String,
        message: String,
    },
    Manifest {
        path: String,
        message: String,
    },
    Decode {
        clip: String,
        message: String,
    },
    Integrity {
        clip: String,
        expected: String,
        actual: String,
    },
    /// No sound with this id or vanilla datablock name exists in the pack.
    UnknownSound(String),
    /// The sound exists but cannot play (e.g. its authored file is missing in
    /// the reference installation). The pack diagnostics explain why.
    SoundUnavailable {
        sound: String,
        reason: String,
    },
    /// Resident decoded PCM would exceed the configured memory budget.
    MemoryBudget {
        needed: u64,
        budget: u64,
    },
    /// The command queue to the mixer is full; the request was dropped.
    QueueFull,
    /// No output device is available.
    NoDevice,
    /// The output device failed to open or run.
    Device(String),
    /// The requested output feature is not compiled in.
    Unsupported(&'static str),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::Io { path, message } => write!(f, "I/O error for {path}: {message}"),
            AudioError::Manifest { path, message } => {
                write!(f, "invalid audio pack {path}: {message}")
            }
            AudioError::Decode { clip, message } => write!(f, "cannot decode {clip}: {message}"),
            AudioError::Integrity {
                clip,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "{clip}: sha256 {actual} does not match manifest {expected}"
                )
            }
            AudioError::UnknownSound(s) => write!(f, "unknown sound {s:?}"),
            AudioError::SoundUnavailable { sound, reason } => {
                write!(f, "sound {sound} is unavailable: {reason}")
            }
            AudioError::MemoryBudget { needed, budget } => {
                write!(f, "decoded audio needs {needed} bytes, budget is {budget}")
            }
            AudioError::QueueFull => write!(f, "audio command queue is full"),
            AudioError::NoDevice => write!(f, "no audio output device is available"),
            AudioError::Device(m) => write!(f, "audio device error: {m}"),
            AudioError::Unsupported(m) => write!(f, "unsupported: {m}"),
        }
    }
}

impl std::error::Error for AudioError {}
