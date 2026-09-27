//! Synthetic, hardware-free test pack (generated in memory; no original assets).
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use bri_audio::bank::{BankOptions, SoundBank};
use bri_audio::schema::*;
use bri_audio::wav::encode_pcm16;
use sha2::{Digest, Sha256};

pub const OUT_RATE: u32 = 48_000;

pub fn sine(rate: u32, channels: u16, seconds: f32, freq: f32, amp: f32) -> Vec<f32> {
    let frames = (rate as f32 * seconds) as usize;
    let mut v = Vec::with_capacity(frames * channels as usize);
    for i in 0..frames {
        let s = (i as f32 / rate as f32 * freq * std::f32::consts::TAU).sin() * amp;
        for c in 0..channels {
            // Right channel silent for stereo clips so channel routing is observable.
            v.push(if c == 0 { s } else { 0.0 });
        }
    }
    v
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

struct ClipSpec {
    id: &'static str,
    rate: u32,
    channels: u16,
    seconds: f32,
    stream: bool,
}

fn ev() -> Evidence {
    Evidence {
        file: "synthetic".into(),
        line: 1,
    }
}

#[allow(clippy::too_many_arguments)]
fn sound(
    id: &str,
    name: &str,
    clip: Option<&str>,
    gain: f32,
    looping: bool,
    spatial: Option<(f32, f32)>,
    channel: u8,
    bus: Bus,
) -> SoundEntry {
    SoundEntry {
        id: id.into(),
        name: name.into(),
        clip: clip.map(Into::into),
        description: None,
        playback: Playback {
            gain,
            pitch: 1.0,
            looping,
            spatial: spatial.map(|(r, m)| Spatial {
                reference_distance: r,
                max_distance: m,
            }),
            channel,
            bus,
        },
        preload: true,
        ui_name: None,
        family: "test".into(),
        package: "base".into(),
        default_enabled: true,
        layer: "v20-base".into(),
        lists: vec![],
        status: if clip.is_some() {
            SoundStatus::Ready
        } else {
            SoundStatus::MissingClip {
                requested: "~/data/sound/missing.wav".into(),
                candidates: vec![],
            }
        },
        defined_at: ev(),
    }
}

/// Returns (manifest, clip bytes by file).
pub fn synthetic() -> (PackManifest, HashMap<String, Vec<u8>>) {
    let specs = [
        ClipSpec {
            id: "short",
            rate: 22_050,
            channels: 1,
            seconds: 0.25,
            stream: false,
        },
        ClipSpec {
            id: "tone",
            rate: 44_100,
            channels: 1,
            seconds: 1.0,
            stream: false,
        },
        ClipSpec {
            id: "stereo",
            rate: 44_100,
            channels: 2,
            seconds: 0.5,
            stream: false,
        },
        ClipSpec {
            id: "streamed",
            rate: 22_050,
            channels: 1,
            seconds: 0.7,
            stream: true,
        },
    ];
    let mut files = HashMap::new();
    let mut clips = Vec::new();
    for s in &specs {
        let pcm = sine(s.rate, s.channels, s.seconds, 440.0, 0.5);
        let bytes = encode_pcm16(s.rate, s.channels, &pcm);
        let file = format!("blobs/{}.wav", s.id);
        clips.push(ClipEntry {
            id: format!("test/clip/{}", s.id),
            file: file.clone(),
            sha256: hex(&Sha256::digest(&bytes)),
            bytes: bytes.len() as u64,
            format: ClipFormat::Wav,
            channels: s.channels,
            sample_rate: s.rate,
            bits_per_sample: Some(16),
            frames: (pcm.len() / s.channels as usize) as u64,
            duration_seconds: s.seconds as f64,
            peak: 0.5,
            rms: 0.35,
            stream: s.stream,
            sources: vec![],
        });
        files.insert(file, bytes);
    }
    let sounds = vec![
        sound(
            "test/sound/ui",
            "UiSound",
            Some("test/clip/short"),
            1.0,
            false,
            None,
            1,
            Bus::Interface,
        ),
        sound(
            "test/sound/boom",
            "BoomSound",
            Some("test/clip/short"),
            1.0,
            false,
            Some((10.0, 60.0)),
            2,
            Bus::Effects,
        ),
        sound(
            "test/sound/loop",
            "LoopSound",
            Some("test/clip/tone"),
            1.0,
            true,
            Some((5.0, 30.0)),
            2,
            Bus::Effects,
        ),
        sound(
            "test/sound/loop2d",
            "Loop2dSound",
            Some("test/clip/tone"),
            1.0,
            true,
            None,
            2,
            Bus::Effects,
        ),
        sound(
            "test/sound/stereo",
            "StereoSound",
            Some("test/clip/stereo"),
            1.0,
            false,
            Some((10.0, 60.0)),
            2,
            Bus::Effects,
        ),
        sound(
            "test/music/loop",
            "musicData_Test",
            Some("test/clip/streamed"),
            1.0,
            true,
            Some((10.0, 30.0)),
            2,
            Bus::Music,
        ),
        sound(
            "test/sound/missing",
            "MissingSound",
            None,
            1.0,
            false,
            None,
            1,
            Bus::Interface,
        ),
    ];
    let mut sounds = sounds;
    sounds[5].ui_name = Some("Test".into());
    let manifest = PackManifest {
        schema: PACK_SCHEMA.into(),
        schema_version: PACK_SCHEMA_VERSION,
        pack_id: "synthetic".into(),
        generator: Generator {
            name: "test".into(),
            version: "0".into(),
            arguments: vec![],
        },
        source: SourceSummary {
            reference_label: "synthetic".into(),
            executable_sha256: None,
            audio_files_found: 4,
            wav_files_found: 4,
            ogg_files_found: 0,
            scripts_scanned: 0,
        },
        defaults: MixDefaults::default(),
        channels: vec![],
        clips,
        descriptions: vec![],
        sounds,
        triggers: vec![TriggerEntry {
            key: "test.boom".into(),
            label: "boom".into(),
            sound: "test/sound/boom".into(),
            placement: PlacementKind::World,
            source: "rule".into(),
            package: "base".into(),
            evidence: vec![ev()],
            note: None,
        }],
        diagnostics: vec![],
    };
    (manifest, files)
}

pub fn bank() -> Arc<SoundBank> {
    let (m, files) = synthetic();
    Arc::new(
        SoundBank::from_manifest(m, &BankOptions::default(), |c| Ok(files[&c.file].clone()))
            .expect("synthetic bank"),
    )
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// RMS of each channel of interleaved stereo.
pub fn rms_lr(samples: &[f32]) -> (f32, f32) {
    let l: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let r: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
    (rms(&l), rms(&r))
}
