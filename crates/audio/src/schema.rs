//! Versioned native audio pack schema (`manifest.json`).
//!
//! The pack is produced offline by `bri-audio-import`; the runtime only reads
//! this schema and the original clip bytes stored beside it. Nothing here knows
//! about Torque scripts, archives or DSO files.

use serde::{Deserialize, Serialize};

/// Value of [`PackManifest::schema`].
pub const PACK_SCHEMA: &str = "bri.audio-pack";
/// Current [`PackManifest::schema_version`]. Readers reject other versions.
pub const PACK_SCHEMA_VERSION: u32 = 1;

/// Top-level pack manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackManifest {
    pub schema: String,
    pub schema_version: u32,
    /// Directory name of the pack, e.g. `audio-pack-001`.
    pub pack_id: String,
    pub generator: Generator,
    pub source: SourceSummary,
    /// Stock client audio preferences from the reference `base/client/defaults.cs`.
    pub defaults: MixDefaults,
    /// Vanilla audio channel ("type") numbers and their meaning.
    pub channels: Vec<ChannelInfo>,
    pub clips: Vec<ClipEntry>,
    pub descriptions: Vec<DescriptionEntry>,
    pub sounds: Vec<SoundEntry>,
    /// Vanilla gameplay/UI trigger -> native sound id bindings.
    pub triggers: Vec<TriggerEntry>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Generator {
    pub name: String,
    pub version: String,
    /// Arguments used for this run (paths are as given, not resolved).
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceSummary {
    /// Human label of the designated reference installation.
    pub reference_label: String,
    /// SHA-256 of the engine executable (engine-bound profile evidence).
    pub executable_sha256: Option<String>,
    /// Number of source audio files discovered (loose + archive members).
    pub audio_files_found: usize,
    pub wav_files_found: usize,
    pub ogg_files_found: usize,
    /// Script files scanned as text evidence (never executed).
    pub scripts_scanned: usize,
}

/// Stock client audio preferences. Channel volumes are indexed by vanilla type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MixDefaults {
    pub master_volume: f32,
    /// `$pref::Audio::channelVolume1..8`; index 0 is unused by the stock UI.
    pub channel_volumes: [f32; 9],
    pub play_music: bool,
    pub menu_sounds: bool,
    pub plant_error_sound: bool,
    pub play_brick_move_sound: bool,
    pub play_brick_plant_sound: bool,
    pub evidence: Vec<Evidence>,
}

impl Default for MixDefaults {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            channel_volumes: [1.0; 9],
            play_music: true,
            menu_sounds: true,
            plant_error_sound: false,
            play_brick_move_sound: true,
            play_brick_plant_sound: true,
            evidence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelInfo {
    pub channel: u8,
    pub name: String,
    pub bus: Bus,
    /// Stock options slider controlling this channel, if any.
    pub ui_control: Option<String>,
}

/// Native mixing bus. Vanilla had master + numbered channels; `Music` is an
/// additional native bus multiplied on top for music-category sounds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Bus {
    /// Vanilla `$GuiAudioType` (1): menus, notes, HUD, title music.
    Interface,
    /// Vanilla `$SimAudioType` (2): world sounds.
    Effects,
    /// Vanilla `$MessageAudioType` (3).
    Message,
    /// Music-brick loops and title music.
    Music,
    /// Other numbered channels (0, 4..8) only used by the stock options test tone.
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ClipFormat {
    Wav,
    OggVorbis,
}

/// One unique original audio file (by content hash).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipEntry {
    /// Stable id, e.g. `v20/clip/base/data/sound/jump.wav`.
    pub id: String,
    /// Pack-relative path of the preserved original bytes.
    pub file: String,
    pub sha256: String,
    pub bytes: u64,
    pub format: ClipFormat,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: Option<u16>,
    pub frames: u64,
    pub duration_seconds: f64,
    /// Peak absolute decoded sample value (0..=1).
    pub peak: f32,
    /// RMS of all decoded samples.
    pub rms: f32,
    /// Runtime should stream (incrementally decode) instead of preloading PCM.
    pub stream: bool,
    /// Every place in the reference installation holding these exact bytes.
    pub sources: Vec<ClipSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipSource {
    /// Torque virtual path, e.g. `Add-Ons/Weapon_Gun/gunShot1.wav`.
    pub virtual_path: String,
    /// `loose` or `zip:Add-Ons/Weapon_Gun.zip`.
    pub container: String,
    pub archive_sha256: Option<String>,
    /// Owning package: `base` or the add-on directory name.
    pub package: String,
}

/// Authored `AudioDescription` values (defaults applied where unset).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DescriptionEntry {
    /// e.g. `v20/audio-description/audioclose3d`.
    pub id: String,
    pub name: String,
    pub volume: f32,
    pub is_looping: bool,
    pub is_streaming: bool,
    pub is_3d: bool,
    pub reference_distance: f32,
    pub max_distance: f32,
    pub channel: u8,
    pub cone_inside_angle: f32,
    pub cone_outside_angle: f32,
    pub cone_outside_volume: f32,
    pub environment_level: f32,
    /// Fields explicitly authored (others are engine defaults).
    pub authored_fields: Vec<String>,
    pub defined_at: Evidence,
}

/// Resolved parameters the runtime needs to play a sound.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Playback {
    /// Description `volume` (linear, before channel/master gain).
    pub gain: f32,
    /// Vanilla pitch; stock descriptions never set it, so 1.0.
    pub pitch: f32,
    pub looping: bool,
    /// `None` for 2D (non-positional) sounds.
    pub spatial: Option<Spatial>,
    pub channel: u8,
    pub bus: Bus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Spatial {
    /// Full volume inside this distance (world units).
    pub reference_distance: f32,
    /// Silent at and beyond this distance (world units).
    pub max_distance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SoundStatus {
    /// Clip resolved and decodable.
    Ready,
    /// The authored file does not exist in the reference installation.
    MissingClip {
        requested: String,
        candidates: Vec<String>,
    },
    /// The description could not be resolved.
    MissingDescription { requested: String },
    /// The file exists but could not be decoded (see diagnostics).
    Undecodable { file: String },
    /// Vanilla would reject/remove this profile at load (e.g. stereo music).
    RejectedByVanilla { reason: String },
}

/// A playable sound: one vanilla `AudioProfile` (static or rule-generated).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoundEntry {
    /// Stable id, e.g. `v20/sound/jumpsound` or `v20/music/after_school_special`.
    pub id: String,
    /// Original datablock name, case preserved.
    pub name: String,
    pub clip: Option<String>,
    pub description: Option<String>,
    pub playback: Playback,
    pub preload: bool,
    pub ui_name: Option<String>,
    /// Coarse family used by the coverage table (ui, building, player, ...).
    pub family: String,
    /// `base` or add-on directory name.
    pub package: String,
    /// Package is enabled in the stock default add-on list (base is always true).
    pub default_enabled: bool,
    /// Source layer: `v20-base`, `launcher-patch` or `add-on`.
    pub layer: String,
    /// Menus/lists this profile appears in: `event:Sound`, `wrench:Music`, ...
    pub lists: Vec<String>,
    pub status: SoundStatus,
    /// Effective definition (last one in the assumed load order).
    pub defined_at: Evidence,
}

impl SoundEntry {
    pub fn is_ready(&self) -> bool {
        matches!(self.status, SoundStatus::Ready) && self.clip.is_some()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlacementKind {
    /// Non-positional (listener-relative 2D).
    Listener,
    /// Fixed world position at trigger time.
    World,
    /// Follows a game object until stopped or the object despawns.
    Attached,
}

/// Maps a vanilla trigger to a native sound id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerEntry {
    /// Stable key, e.g. `player.jump` or `datablock:PlayerStandardArmor.JumpSound`.
    pub key: String,
    /// Short human description of when vanilla plays it.
    pub label: String,
    pub sound: String,
    pub placement: PlacementKind,
    /// `datablock-field`, `script-call`, `engine` or `rule`.
    pub source: String,
    pub package: String,
    pub evidence: Vec<Evidence>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Evidence {
    /// Evidence file, e.g. `.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs`
    /// or `Add-Ons/Weapon_Gun.zip!server.cs`.
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub evidence: Vec<Evidence>,
}

impl PackManifest {
    /// Validate schema identity and internal references.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != PACK_SCHEMA {
            return Err(format!("unexpected schema {:?}", self.schema));
        }
        if self.schema_version != PACK_SCHEMA_VERSION {
            return Err(format!(
                "unsupported schema_version {} (expected {PACK_SCHEMA_VERSION})",
                self.schema_version
            ));
        }
        let mut clip_ids = std::collections::HashSet::new();
        for c in &self.clips {
            if !clip_ids.insert(c.id.as_str()) {
                return Err(format!("duplicate clip id {}", c.id));
            }
            if c.file.contains("..") || c.file.starts_with('/') || c.file.contains('\\') {
                return Err(format!("clip {} has unsafe file path {:?}", c.id, c.file));
            }
            if c.channels == 0 || c.sample_rate == 0 {
                return Err(format!("clip {} has an empty format", c.id));
            }
        }
        let mut desc_ids = std::collections::HashSet::new();
        for d in &self.descriptions {
            if !desc_ids.insert(d.id.as_str()) {
                return Err(format!("duplicate description id {}", d.id));
            }
        }
        let mut sound_ids = std::collections::HashSet::new();
        for s in &self.sounds {
            if !sound_ids.insert(s.id.as_str()) {
                return Err(format!("duplicate sound id {}", s.id));
            }
            if let Some(clip) = &s.clip
                && !clip_ids.contains(clip.as_str())
            {
                return Err(format!("sound {} references unknown clip {clip}", s.id));
            }
            if let Some(d) = &s.description
                && !desc_ids.contains(d.as_str())
            {
                return Err(format!("sound {} references unknown description {d}", s.id));
            }
            let p = &s.playback;
            if !(p.gain.is_finite() && p.gain >= 0.0 && p.pitch.is_finite() && p.pitch > 0.0) {
                return Err(format!("sound {} has invalid gain/pitch", s.id));
            }
            if let Some(sp) = p.spatial
                && !(sp.reference_distance.is_finite()
                    && sp.max_distance.is_finite()
                    && sp.reference_distance >= 0.0
                    && sp.max_distance >= 0.0)
            {
                return Err(format!("sound {} has invalid distances", s.id));
            }
            if usize::from(p.channel) >= self.defaults.channel_volumes.len() {
                return Err(format!("sound {} uses channel {}", s.id, p.channel));
            }
        }
        for t in &self.triggers {
            if !sound_ids.contains(t.sound.as_str()) {
                return Err(format!(
                    "trigger {} references unknown sound {}",
                    t.key, t.sound
                ));
            }
        }
        Ok(())
    }
}
