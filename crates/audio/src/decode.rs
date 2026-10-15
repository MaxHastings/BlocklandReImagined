//! Clip decoding (WAV PCM and Ogg Vorbis) through Symphonia.
//!
//! Decoding happens on the loading thread for preloaded clips. Streaming clips
//! keep their original compressed bytes in memory and decode incrementally on
//! the mixer thread (see [`StreamDecoder`]).

use std::io::Cursor;
use std::sync::Arc;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

/// Fully decoded interleaved PCM.
#[derive(Debug, Clone, PartialEq)]
pub struct PcmClip {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved samples in -1..=1.
    pub samples: Box<[f32]>,
}

impl PcmClip {
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    pub fn resident_bytes(&self) -> u64 {
        (self.samples.len() * std::mem::size_of::<f32>()) as u64
    }

    pub fn duration_seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate.max(1))
    }
}

/// Format facts reported while decoding.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DecodeReport {
    pub bits_per_sample: Option<u16>,
    pub container_frames: Option<u64>,
    pub packets: u64,
    /// Packets rejected by the codec (skipped). Stock clips should have zero.
    pub corrupt_packets: u64,
    /// Samples that were NaN/inf (replaced with 0). Stock clips should have zero.
    pub non_finite_samples: u64,
}

struct Opened {
    format: Box<dyn FormatReader + 'static>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    frames: Option<u64>,
    bits: Option<u16>,
}

fn open_reader(bytes: Arc<[u8]>, extension: Option<&str>) -> Result<Opened, String> {
    let bytes = pcm_wave_header(&bytes).map_or(bytes, Arc::from);
    let mss = MediaSourceStream::new(
        Box::new(Cursor::new(bytes)),
        MediaSourceStreamOptions::default(),
    );
    let mut hint = Hint::new();
    if let Some(ext) = extension {
        hint.with_extension(ext);
    }
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| format!("unrecognised container: {e}"))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| "no audio track".to_string())?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| "track has no audio codec parameters".to_string())?
        .clone();
    let track_id = track.id;
    let frames = track.num_frames;
    let bits = params
        .bits_per_coded_sample
        .or(params.bits_per_sample)
        .map(|b| b as u16);
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| format!("unsupported codec: {e}"))?;
    Ok(Opened {
        format,
        decoder,
        track_id,
        frames,
        bits,
    })
}

/// Some PCM exporters write WAVEFORMATEX's zero extension (and optional
/// zero padding) in `fmt `, which the demuxer accepts only in its 16-byte
/// form. Normalize that header in memory, preserving every sample and other
/// chunk. Non-PCM codecs and meaningful extensions belong to the decoder.
fn pcm_wave_header(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12usize;
    let mut found = None;
    while at < bytes.len() {
        let header = bytes.get(at..at.checked_add(8)?)?;
        let size = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        let start = at + 8;
        let end = start.checked_add(size)?;
        let data = bytes.get(start..end)?;
        if &header[..4] == b"fmt " {
            if found.is_some()
                || !(18..=64).contains(&size)
                || size & 1 != 0
                || data[..2] != [1, 0]
                || data[16..].iter().any(|v| *v != 0)
            {
                return None;
            }
            found = Some((at, end, size - 16));
        }
        at = end.checked_add(size & 1)?;
    }
    if at != bytes.len() {
        return None;
    }
    let (start, end, removed) = found?;
    let declared = (u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize).checked_add(8)?;
    // A legacy exporter may have counted the minimal PCM header in RIFF's
    // length while writing the extended one. Accept only that exact deficit.
    if declared != bytes.len() && declared.checked_add(removed)? != bytes.len() {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() - removed);
    out.extend_from_slice(&bytes[..start + 4]);
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&bytes[start + 8..start + 24]);
    out.extend_from_slice(&bytes[end..]);
    let size = u32::try_from(out.len() - 8).ok()?;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Some(out)
}

/// Decode an entire clip to interleaved f32 PCM.
pub fn decode_all(
    bytes: &[u8],
    extension: Option<&str>,
) -> Result<(PcmClip, DecodeReport), String> {
    let data: Arc<[u8]> = Arc::from(bytes);
    let Opened {
        mut format,
        mut decoder,
        track_id,
        frames,
        bits,
    } = open_reader(data, extension)?;
    let mut report = DecodeReport {
        bits_per_sample: bits,
        container_frames: frames,
        ..Default::default()
    };
    let mut samples: Vec<f32> = Vec::with_capacity(frames.unwrap_or(0) as usize);
    let mut chunk: Vec<f32> = Vec::new();
    let mut spec: Option<(u32, u16)> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("container read failed: {e}")),
        };
        if packet.track_id != track_id {
            continue;
        }
        report.packets += 1;
        match decoder.decode(&packet) {
            Ok(buf) => {
                let s = buf.spec();
                let this = (s.rate(), s.channels().count() as u16);
                match spec {
                    None => spec = Some(this),
                    Some(prev) if prev != this => {
                        return Err(format!("format changed mid-stream {prev:?} -> {this:?}"));
                    }
                    _ => {}
                }
                chunk.resize(buf.samples_interleaved(), 0.0);
                buf.copy_to_slice_interleaved(&mut chunk);
                for v in &chunk {
                    if v.is_finite() {
                        samples.push(*v);
                    } else {
                        report.non_finite_samples += 1;
                        samples.push(0.0);
                    }
                }
            }
            Err(SymError::DecodeError(_)) => report.corrupt_packets += 1,
            Err(e) => return Err(format!("decode failed: {e}")),
        }
    }
    let (sample_rate, channels) = spec.ok_or_else(|| "no decodable audio".to_string())?;
    if channels == 0 || sample_rate == 0 {
        return Err("empty audio format".into());
    }
    Ok((
        PcmClip {
            sample_rate,
            channels,
            samples: samples.into_boxed_slice(),
        },
        report,
    ))
}

/// Incremental decoder for streamed clips. Owns the compressed bytes (shared),
/// so restarting a loop never touches the filesystem.
pub struct StreamDecoder {
    bytes: Arc<[u8]>,
    extension: Option<&'static str>,
    format: Box<dyn FormatReader + 'static>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    pub sample_rate: u32,
    pub channels: u16,
    scratch: Vec<f32>,
    finished: bool,
}

impl std::fmt::Debug for StreamDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamDecoder")
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .field("finished", &self.finished)
            .finish()
    }
}

impl StreamDecoder {
    pub fn open(
        bytes: Arc<[u8]>,
        extension: Option<&'static str>,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Self, String> {
        let Opened {
            format,
            decoder,
            track_id,
            ..
        } = open_reader(bytes.clone(), extension)?;
        Ok(Self {
            bytes,
            extension,
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            scratch: Vec::new(),
            finished: false,
        })
    }

    /// Append the next decoded packet's interleaved frames to `out`.
    /// Returns `Ok(false)` at end of stream.
    pub fn next_chunk(&mut self, out: &mut Vec<f32>) -> Result<bool, String> {
        if self.finished {
            return Ok(false);
        }
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.finished = true;
                    return Ok(false);
                }
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.finished = true;
                    return Ok(false);
                }
                Err(e) => return Err(e.to_string()),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(buf) => {
                    let ch = buf.spec().channels().count() as u16;
                    if ch != self.channels || buf.spec().rate() != self.sample_rate {
                        return Err("stream format differs from manifest".into());
                    }
                    self.scratch.resize(buf.samples_interleaved(), 0.0);
                    buf.copy_to_slice_interleaved(&mut self.scratch);
                    out.extend(
                        self.scratch
                            .iter()
                            .map(|v| if v.is_finite() { *v } else { 0.0 }),
                    );
                    return Ok(true);
                }
                Err(SymError::DecodeError(_)) => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    /// Rewind to the start (used for looping streams).
    pub fn restart(&mut self) -> Result<(), String> {
        let Opened {
            format,
            decoder,
            track_id,
            ..
        } = open_reader(self.bytes.clone(), self.extension)?;
        self.format = format;
        self.decoder = decoder;
        self.track_id = track_id;
        self.finished = false;
        Ok(())
    }
}

/// Peak and RMS over interleaved samples.
pub fn peak_rms(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let mut peak = 0f32;
    let mut sum = 0f64;
    for &s in samples {
        peak = peak.max(s.abs());
        sum += f64::from(s) * f64::from(s);
    }
    (peak, (sum / samples.len() as f64).sqrt() as f32)
}
