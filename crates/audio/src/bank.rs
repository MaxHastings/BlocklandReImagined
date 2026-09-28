//! Loading a native audio pack into playable, shareable assets.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::decode::{PcmClip, decode_all};
use crate::error::AudioError;
use crate::schema::{Bus, ClipEntry, ClipFormat, MixDefaults, PackManifest, Playback, SoundEntry};

/// Decoded or streamable audio data for one clip.
#[derive(Debug, Clone)]
pub enum ClipData {
    /// Preloaded PCM shared by every voice using it.
    Pcm(Arc<PcmClip>),
    /// Compressed original bytes decoded incrementally while playing.
    Stream(Arc<StreamingClip>),
}

#[derive(Debug)]
pub struct StreamingClip {
    pub bytes: Arc<[u8]>,
    pub extension: &'static str,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
}

impl ClipData {
    pub fn sample_rate(&self) -> u32 {
        match self {
            ClipData::Pcm(p) => p.sample_rate,
            ClipData::Stream(s) => s.sample_rate,
        }
    }
    pub fn channels(&self) -> u16 {
        match self {
            ClipData::Pcm(p) => p.channels,
            ClipData::Stream(s) => s.channels,
        }
    }
}

/// Everything the mixer needs to play one sound; resolved on the game side so
/// the audio thread never performs lookups, file I/O or preloading.
#[derive(Debug)]
pub struct SoundAsset {
    pub id: Arc<str>,
    pub name: Arc<str>,
    pub playback: Playback,
    pub data: ClipData,
}

/// Most bytes one clip from outside the bank may have (an Add-On's sound).
pub const MAX_DECODED_CLIP_BYTES: usize = 8 * 1024 * 1024;

impl SoundAsset {
    /// A world sound from a WAV or Ogg Vorbis clip that is not in the bank
    /// (an Add-On's own), fully decoded: heard at full volume within
    /// `reference_distance` and fading out by `max_distance`, on the
    /// effects channel.
    pub fn decoded(
        id: &str,
        bytes: &[u8],
        extension: &str,
        reference_distance: f32,
        max_distance: f32,
    ) -> Result<Self, String> {
        if bytes.len() > MAX_DECODED_CLIP_BYTES {
            return Err(format!(
                "{} bytes is more than a sound may have ({MAX_DECODED_CLIP_BYTES})",
                bytes.len()
            ));
        }
        let extension = match extension.to_ascii_lowercase().as_str() {
            "wav" => "wav",
            "ogg" => "ogg",
            other => return Err(format!("`.{other}` is not a WAV or Ogg Vorbis sound")),
        };
        let (clip, _) = crate::decode::decode_all(bytes, Some(extension))?;
        if clip.frames() == 0 || clip.duration_seconds() > 30.0 {
            return Err("a sound must last between a moment and 30 seconds".into());
        }
        Ok(Self {
            id: Arc::from(id),
            name: Arc::from(id),
            playback: crate::schema::Playback {
                gain: 1.0,
                pitch: 1.0,
                looping: false,
                spatial: Some(crate::schema::Spatial {
                    reference_distance,
                    max_distance,
                }),
                channel: 2,
                bus: crate::schema::Bus::Effects,
            },
            data: ClipData::Pcm(Arc::new(clip)),
        })
    }
}

/// Options for [`SoundBank::load`].
#[derive(Debug, Clone)]
pub struct BankOptions {
    /// Recompute SHA-256 of each clip file and compare with the manifest.
    pub verify_hashes: bool,
    /// Upper bound for resident decoded PCM (bytes). Loading fails cleanly if
    /// preloaded clips would exceed it.
    pub max_resident_bytes: u64,
    /// Honour the manifest's `stream` flag (music). If false everything is preloaded.
    pub allow_streaming: bool,
}

impl Default for BankOptions {
    fn default() -> Self {
        Self {
            verify_hashes: true,
            max_resident_bytes: 64 * 1024 * 1024,
            allow_streaming: true,
        }
    }
}

/// Loaded pack. Cheap to share (`Arc<SoundBank>`), immutable after load.
#[derive(Debug)]
pub struct SoundBank {
    manifest: PackManifest,
    ready: HashMap<String, Arc<SoundAsset>>,
    unavailable: HashMap<String, String>,
    /// lower-case datablock name / id -> canonical id
    aliases: HashMap<String, String>,
    resident_bytes: u64,
    streamed_clips: usize,
}

impl SoundBank {
    /// Load `manifest.json` and clip files from a pack directory.
    pub fn load(pack_dir: impl AsRef<Path>, options: &BankOptions) -> Result<Self, AudioError> {
        let dir = pack_dir.as_ref().to_path_buf();
        let manifest_path = dir.join("manifest.json");
        let text = std::fs::read_to_string(&manifest_path).map_err(|e| AudioError::Io {
            path: manifest_path.display().to_string(),
            message: e.to_string(),
        })?;
        let manifest: PackManifest =
            serde_json::from_str(&text).map_err(|e| AudioError::Manifest {
                path: manifest_path.display().to_string(),
                message: e.to_string(),
            })?;
        Self::from_manifest(manifest, options, |clip| {
            let p: PathBuf = dir.join(&clip.file);
            std::fs::read(&p).map_err(|e| AudioError::Io {
                path: p.display().to_string(),
                message: e.to_string(),
            })
        })
    }

    /// Build a bank from a manifest and a byte loader (used by tests/tools).
    pub fn from_manifest(
        manifest: PackManifest,
        options: &BankOptions,
        mut read_clip: impl FnMut(&ClipEntry) -> Result<Vec<u8>, AudioError>,
    ) -> Result<Self, AudioError> {
        manifest
            .validate()
            .map_err(|message| AudioError::Manifest {
                path: manifest.pack_id.clone(),
                message,
            })?;

        // Only clips referenced by a ready sound are loaded.
        let mut needed: HashMap<&str, &ClipEntry> = HashMap::new();
        let clips: HashMap<&str, &ClipEntry> =
            manifest.clips.iter().map(|c| (c.id.as_str(), c)).collect();
        for s in manifest.sounds.iter().filter(|s| s.is_ready()) {
            if let Some(c) = s.clip.as_deref().and_then(|id| clips.get(id)) {
                needed.insert(c.id.as_str(), c);
            }
        }
        let mut loaded: HashMap<String, ClipData> = HashMap::new();
        let mut resident = 0u64;
        let mut streamed = 0usize;
        let mut ids: Vec<&str> = needed.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let clip = needed[id];
            let bytes = read_clip(clip)?;
            if options.verify_hashes {
                let actual = hex(&Sha256::digest(&bytes));
                if actual != clip.sha256 {
                    return Err(AudioError::Integrity {
                        clip: clip.id.clone(),
                        expected: clip.sha256.clone(),
                        actual,
                    });
                }
            }
            let ext = match clip.format {
                ClipFormat::Wav => "wav",
                ClipFormat::OggVorbis => "ogg",
            };
            let data = if clip.stream && options.allow_streaming {
                streamed += 1;
                resident += bytes.len() as u64;
                ClipData::Stream(Arc::new(StreamingClip {
                    bytes: Arc::from(bytes.into_boxed_slice()),
                    extension: ext,
                    sample_rate: clip.sample_rate,
                    channels: clip.channels,
                    frames: clip.frames,
                }))
            } else {
                let (pcm, _) =
                    decode_all(&bytes, Some(ext)).map_err(|message| AudioError::Decode {
                        clip: clip.id.clone(),
                        message,
                    })?;
                if pcm.sample_rate != clip.sample_rate || pcm.channels != clip.channels {
                    return Err(AudioError::Decode {
                        clip: clip.id.clone(),
                        message: format!(
                            "decoded {} Hz/{} ch, manifest says {} Hz/{} ch",
                            pcm.sample_rate, pcm.channels, clip.sample_rate, clip.channels
                        ),
                    });
                }
                resident += pcm.resident_bytes();
                ClipData::Pcm(Arc::new(pcm))
            };
            if resident > options.max_resident_bytes {
                return Err(AudioError::MemoryBudget {
                    needed: resident,
                    budget: options.max_resident_bytes,
                });
            }
            loaded.insert(clip.id.clone(), data);
        }

        let mut ready = HashMap::new();
        let mut unavailable = HashMap::new();
        let mut aliases = HashMap::new();
        for s in &manifest.sounds {
            aliases.insert(s.id.to_ascii_lowercase(), s.id.clone());
            // Datablock names are unique case-insensitively in Torque; the first
            // (canonical) entry wins if a generated name ever collided.
            aliases
                .entry(s.name.to_ascii_lowercase())
                .or_insert_with(|| s.id.clone());
            match s
                .clip
                .as_deref()
                .and_then(|c| loaded.get(c))
                .filter(|_| s.is_ready())
            {
                Some(data) => {
                    ready.insert(
                        s.id.clone(),
                        Arc::new(SoundAsset {
                            id: Arc::from(s.id.as_str()),
                            name: Arc::from(s.name.as_str()),
                            playback: s.playback.clone(),
                            data: data.clone(),
                        }),
                    );
                }
                None => {
                    unavailable.insert(s.id.clone(), describe_status(s));
                }
            }
        }
        Ok(Self {
            manifest,
            ready,
            unavailable,
            aliases,
            resident_bytes: resident,
            streamed_clips: streamed,
        })
    }

    /// Resolve by stable id (`v20/sound/jumpsound`) or vanilla datablock name
    /// (`JumpSound`), case-insensitively.
    pub fn resolve(&self, id_or_name: &str) -> Result<&Arc<SoundAsset>, AudioError> {
        let key = id_or_name.to_ascii_lowercase();
        let id = self
            .aliases
            .get(&key)
            .ok_or_else(|| AudioError::UnknownSound(id_or_name.to_string()))?;
        match self.ready.get(id) {
            Some(a) => Ok(a),
            None => Err(AudioError::SoundUnavailable {
                sound: id.clone(),
                reason: self
                    .unavailable
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| "not loaded".into()),
            }),
        }
    }

    pub fn manifest(&self) -> &PackManifest {
        &self.manifest
    }
    pub fn defaults(&self) -> &MixDefaults {
        &self.manifest.defaults
    }
    pub fn ready_count(&self) -> usize {
        self.ready.len()
    }
    pub fn unavailable(&self) -> impl Iterator<Item = (&str, &str)> {
        self.unavailable
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
    }
    /// Decoded PCM plus retained compressed stream bytes.
    pub fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }
    pub fn streamed_clips(&self) -> usize {
        self.streamed_clips
    }
    /// Native sound id bound to a vanilla trigger key, if any.
    pub fn trigger(&self, key: &str) -> Option<&str> {
        self.manifest
            .triggers
            .iter()
            .find(|t| t.key == key)
            .map(|t| t.sound.as_str())
    }
    /// Is this a music-bus sound?
    pub fn is_music(&self, id: &str) -> bool {
        self.ready
            .get(id)
            .is_some_and(|a| a.playback.bus == Bus::Music)
    }
}

fn describe_status(s: &SoundEntry) -> String {
    use crate::schema::SoundStatus::*;
    match &s.status {
        Ready => "clip not loaded".into(),
        MissingClip { requested, .. } => {
            format!("authored file {requested:?} is missing from the reference installation")
        }
        MissingDescription { requested } => format!("description {requested:?} is undefined"),
        Undecodable { file } => format!("{file} exists but cannot be decoded"),
        RejectedByVanilla { reason } => format!("vanilla rejects this profile: {reason}"),
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
