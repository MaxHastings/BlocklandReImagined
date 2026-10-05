//! Minimal WAV writer for offline evidence renders (never plays audio).

use bri_console::Clamp;
use std::io::{self, Write};

/// Write interleaved f32 samples as 16-bit PCM WAV (clamped, TPDF-free rounding).
pub fn write_pcm16<W: Write>(
    mut w: W,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> io::Result<()> {
    let data_len = u32::try_from(samples.len() * 2)
        .map_err(|_| io::Error::other("render too long for WAV"))?;
    let block_align = channels * 2;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sample_rate.to_le_bytes())?;
    w.write_all(&(sample_rate * u32::from(block_align)).to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        let v = if s.is_finite() {
            (s.clamped(-1.0, 1.0) * 32767.0).round() as i16
        } else {
            0
        };
        w.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

/// Encode to an in-memory WAV file.
pub fn encode_pcm16(sample_rate: u32, channels: u16, samples: &[f32]) -> Vec<u8> {
    let mut v = Vec::with_capacity(44 + samples.len() * 2);
    write_pcm16(&mut v, sample_rate, channels, samples).expect("writing to Vec cannot fail");
    v
}
