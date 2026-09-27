//! Game-facing typed values exchanged with the mixer.

use std::sync::Arc;

use crate::bank::SoundAsset;
use crate::spatial::{Listener, Vec3};

/// Handle of one playing (or requested) sound instance. Handles are allocated
/// on the game thread, so `play` never waits for the audio thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SoundHandle(pub(crate) u64);

impl SoundHandle {
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// Game-assigned identity of an object sounds can be attached to (player,
/// vehicle, projectile, brick emitter...). Use the world's stable entity id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityKey(pub u64);

/// Where a sound plays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Non-positional, like vanilla `alxPlay(profile)` / `client.play2D()`.
    /// 3D descriptions still play 2D here, exactly as vanilla does.
    Listener,
    /// Fixed world position, like `ServerPlay3D(profile, position)`.
    World(Vec3),
    /// Follows an entity (`ShapeBase::playAudio`, projectile loops, emitters).
    /// Move it with `update_entity`; `despawn` stops everything attached.
    Attached { entity: EntityKey, position: Vec3 },
}

/// Volume controls. Vanilla master and numbered channels plus the native music bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeControl {
    /// `$pref::Audio::masterVolume` (stock default 0.9 in the reference install).
    Master,
    /// Vanilla channel 1, "Shell" slider (`$pref::Audio::channelVolume1`).
    Interface,
    /// Vanilla channel 2, "Sim" slider (`$pref::Audio::channelVolume2`).
    Effects,
    /// Vanilla channel 3 (no stock slider).
    Message,
    /// Native music bus, multiplied on top of the sound's vanilla channel.
    Music,
    /// Any vanilla channel 0..=8.
    Channel(u8),
}

#[derive(Debug)]
pub(crate) enum Command {
    Play {
        handle: SoundHandle,
        asset: Arc<SoundAsset>,
        placement: Placement,
        gain_scale: f32,
    },
    Stop {
        handle: SoundHandle,
        fade_frames: u32,
    },
    StopEntity {
        entity: EntityKey,
        fade_frames: u32,
    },
    StopAll {
        fade_frames: u32,
    },
    UpdateEntity {
        entity: EntityKey,
        position: Vec3,
    },
    SetSourcePosition {
        handle: SoundHandle,
        position: Vec3,
    },
    SetListener(Listener),
    SetMaster(f32),
    SetChannel {
        channel: u8,
        gain: f32,
    },
    SetMusicGain(f32),
    SetMusicEnabled(bool),
}

/// Why a voice was not started or was removed early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CullReason {
    /// A one-shot's initial gain (excluding master) was at or below the
    /// minimum start gain (vanilla `MIN_GAIN` = 0.05).
    BelowMinimumGain,
    /// All real voices were busy with louder sounds.
    VoicePressure,
    /// The virtual (inaudible, tracked) voice list was full.
    VirtualLimit,
    /// `max_instances_per_sound` reached.
    InstanceLimit,
    /// Music is disabled and the request was a one-shot music sound.
    MusicDisabled,
    /// The stream failed to decode.
    StreamError,
}

/// Notifications from the mixer, drained with `AudioRuntime::drain_events`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioEvent {
    /// Voice started audibly (real) or tracked (virtual / suspended music).
    Started { handle: SoundHandle, real: bool },
    /// One-shot reached its end.
    Finished { handle: SoundHandle },
    /// Stopped by request, entity despawn or `stop_all`.
    Stopped { handle: SoundHandle },
    /// Rejected or removed; the handle is no longer valid.
    Culled {
        handle: SoundHandle,
        reason: CullReason,
    },
    /// Looping voice lost its real voice but keeps its state.
    Virtualized { handle: SoundHandle },
    /// Virtual looping voice became audible again.
    Revived { handle: SoundHandle },
}
