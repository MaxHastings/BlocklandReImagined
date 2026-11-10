//! A full-screen image a rule shows one player (`screen_image`): still or
//! animated frames cycled client-side at `fps`, held for `seconds`, then
//! faded out by the client over [`CLEAR_FADE`] (or cleared early with an
//! empty cue). The rules tick (~12 Hz) is too slow to drive animation by
//! re-issuing the op, so the client advances the frames itself. Used for
//! v20's death boards and similar full-screen beats (a jump-scare face on a
//! black screen).
use serde::{Deserialize, Serialize};

/// Native image index shared by authoring checks and the client loader.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImagePack {
    pub schema_version: u32,
    pub images: Vec<ImageEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageEntry {
    pub key: String,
    pub file: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}
impl ImagePack {
    pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
    pub const MAX_SET_PIXELS: u64 = 64 * 1024 * 1024;
    pub fn pixels(&self) -> u64 {
        self.images.iter().fold(0_u64, |n, e| {
            n.saturating_add(u64::from(e.width) * u64::from(e.height))
        })
    }
    pub fn validate(&self, owner: &str) -> anyhow::Result<()> {
        use anyhow::ensure;
        ensure!(self.schema_version == 1, "UI image index schema must be 1");
        ensure!(
            self.images.len() <= MAX_FRAMES && self.pixels() <= Self::MAX_PIXELS,
            "UI image index exceeds 128 images or 16 million pixels"
        );
        let prefix = format!("{owner}:image/");
        let mut keys = std::collections::BTreeSet::new();
        for e in &self.images {
            ensure!(
                e.key.starts_with(&prefix)
                    && e.key.len() > prefix.len()
                    && e.key.len() <= 128
                    && e.key[prefix.len()..].bytes().all(|b| b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || b"_-./".contains(&b)),
                "UI image key must be namespaced under {prefix}"
            );
            ensure!(keys.insert(&e.key), "Duplicate UI image key {}", e.key);
            ensure!(
                (1..=1024).contains(&e.width) && (1..=1024).contains(&e.height),
                "UI image dimensions must be 1..1024"
            );
            ensure!(
                !e.file.is_empty() && e.file.len() <= 160,
                "Invalid UI image file name"
            );
            ensure!(
                e.sha256.len() == 64
                    && e.sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "Invalid UI image SHA-256"
            );
        }
        Ok(())
    }
}
impl ImageEntry {
    pub const MAX_BYTES: u64 = 16 * 1024 * 1024;
    pub fn verify(&self, bytes: &[u8]) -> anyhow::Result<()> {
        use anyhow::ensure;
        use sha2::Digest;
        ensure!(
            bytes.len() as u64 <= Self::MAX_BYTES,
            "UI image exceeds byte budget"
        );
        ensure!(
            format!("{:x}", sha2::Sha256::digest(bytes)) == self.sha256,
            "UI image checksum mismatch: {}",
            self.key
        );
        let dimensions = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()?
            .into_dimensions()?;
        ensure!(
            dimensions == (self.width, self.height),
            "UI image dimensions do not match index: {}",
            self.key
        );
        Ok(())
    }
}

/// Most frames one cue may name (a 60-frame board is typical).
pub const MAX_FRAMES: usize = 128;
/// Frames per second the client cycles them at.
pub const FPS: std::ops::RangeInclusive<f32> = 1.0..=60.0;
/// How long it stays on screen, seconds (before the fade-out).
pub const SECONDS: std::ops::RangeInclusive<f32> = 0.1..=60.0;
/// Seconds the client fades a board out after it has run its `seconds` — or
/// after an empty cue clears it early, fading from wherever it is showing.
pub const CLEAR_FADE: f32 = 0.35;

fn default_aspect() -> f32 {
    1.0
}

/// A compact run of numbered frames instead of naming every key: frame `i`
/// is `{prefix}{i}`, for `count` of them (even one — a still's key is
/// `{prefix}0`). A 60-frame board then sends its prefix and count, not sixty
/// keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sequence {
    /// The key prefix; frame `i` is `{prefix}{i}`.
    pub prefix: String,
    /// How many frames (1 = a still).
    pub count: usize,
}

/// A full-screen image cue (rules' `screen_image`). Its frames come either
/// from `images` or from `sequence`; with one of them empty the other names
/// them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenImageCue {
    /// UI-pack keys of the frames, in order (one for a still); empty when
    /// `sequence` names them. The client draws them centred and scaled to fit
    /// the screen, preserving `aspect`.
    #[serde(default)]
    pub images: Vec<String>,
    /// Numbered frames `{prefix}{i}` instead of naming every key; cheaper to
    /// send for long boards.
    #[serde(default)]
    pub sequence: Option<Sequence>,
    /// Frames per second. Ignored with one frame.
    pub fps: f32,
    /// Seconds on screen before the client fades it out over [`CLEAR_FADE`].
    pub seconds: f32,
    /// A frame's width over its height (1 = square); fit preserves this ratio.
    #[serde(default = "default_aspect")]
    pub aspect: f32,
    /// Fraction of the viewport's smaller side occupied by the longest image side.
    #[serde(default = "default_aspect")]
    pub scale: f32,
    /// Optional full-screen backing color, independent of the image's coverage.
    #[serde(default)]
    pub background: Option<[f32; 4]>,
}

impl ScreenImageCue {
    /// How many frames this cue shows.
    pub fn frame_count(&self) -> usize {
        self.sequence
            .as_ref()
            .map(|s| s.count)
            .unwrap_or(self.images.len())
    }
    /// Whether every number and key is usable.
    pub fn bounded(&self) -> bool {
        (1..=MAX_FRAMES).contains(&self.frame_count())
            && (self.sequence.is_none() || self.images.is_empty())
            && match &self.sequence {
                Some(s) => {
                    !s.prefix.is_empty()
                        && s.prefix.len() <= 96
                        && !s.prefix.chars().any(char::is_control)
                }
                None => self
                    .images
                    .iter()
                    .all(|k| !k.is_empty() && k.len() <= 128 && !k.chars().any(char::is_control)),
            }
            && self.fps.is_finite()
            && FPS.contains(&self.fps)
            && self.seconds.is_finite()
            && SECONDS.contains(&self.seconds)
            && self.aspect.is_finite()
            && (0.1..=10.0).contains(&self.aspect)
            && self.scale.is_finite()
            && (0.01..=1.0).contains(&self.scale)
            && self
                .background
                .is_none_or(|c| c.iter().all(|v| (0.0..=1.0).contains(v)))
    }

    /// The frame key to show `t` seconds in: cycled at `fps`, wrapped around
    /// the set, its one key for a still. Past `seconds` — while the client
    /// fades it out — the last frame holds instead of wrapping back to the
    /// first (a pass that ends exactly on the hold point keeps its final
    /// frame).
    pub fn frame_key_at(&self, t: f32) -> Option<String> {
        let n = self.frame_count();
        if n == 0 {
            return None;
        }
        if n == 1 {
            return match &self.sequence {
                Some(s) => Some(format!("{}{}", s.prefix, 0)),
                None => self.images.first().cloned(),
            };
        }
        let t = t.max(0.0).min(self.seconds);
        let i0 = (t * self.fps).floor() as usize;
        // A whole pass ending exactly on the hold point keeps its last frame.
        let i = if i0 > 0
            && i0.is_multiple_of(n)
            && t >= self.seconds - 1e-6
            && (self.seconds * self.fps).fract().abs() < 1e-6
        {
            n - 1
        } else {
            i0 % n
        };
        match &self.sequence {
            Some(s) => Some(format!("{}{}", s.prefix, i)),
            None => self.images.get(i).cloned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_index_checks_names_budgets_and_encoded_dimensions() {
        use sha2::Digest;
        for format in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
            let mut encoded = std::io::Cursor::new(Vec::new());
            image::DynamicImage::new_rgb8(2, 3)
                .write_to(&mut encoded, format)
                .unwrap();
            let bytes = encoded.into_inner();
            let entry = ImageEntry {
                key: "sample:image/frame".into(),
                file: "frame.image".into(),
                sha256: format!("{:x}", sha2::Sha256::digest(&bytes)),
                width: 2,
                height: 3,
            };
            let mut pack = ImagePack {
                schema_version: 1,
                images: vec![entry],
            };
            pack.validate("sample").unwrap();
            pack.images[0].verify(&bytes).unwrap();
            assert!(pack.validate("other").is_err());
            pack.images[0].width = 3;
            assert!(pack.images[0].verify(&bytes).is_err());
            pack.images[0].width = 2;
            let mut corrupt = bytes.clone();
            corrupt[0] ^= 1;
            assert!(pack.images[0].verify(&corrupt).is_err());
            pack.images.push(pack.images[0].clone());
            assert!(pack.validate("sample").is_err());
            pack.images = (0..17)
                .map(|i| ImageEntry {
                    key: format!("sample:image/{i}"),
                    width: 1024,
                    height: 1024,
                    ..pack.images[0].clone()
                })
                .collect();
            assert!(pack.validate("sample").is_err());
        }
    }

    #[test]
    fn a_board_is_bounded_and_cycles_its_frames() {
        let cue = ScreenImageCue {
            images: (0..60)
                .map(|i| format!("add-ons/support_render/ui/render_{i}"))
                .collect(),
            sequence: None,
            fps: 15.0,
            seconds: 4.0,
            aspect: 1.0,
            scale: 1.0,
            background: None,
        };
        assert!(cue.bounded());
        assert_eq!(
            cue.frame_key_at(0.0).as_deref(),
            Some("add-ons/support_render/ui/render_0")
        );
        assert_eq!(
            cue.frame_key_at(1.0 / 15.0).as_deref(),
            Some("add-ons/support_render/ui/render_1")
        );
        // One full pass is 60/15 = 4 s; past the end it holds its last frame
        // for the client's fade-out, instead of wrapping back to the start.
        assert_eq!(
            cue.frame_key_at(4.0 + 2.0 / 15.0).as_deref(),
            Some("add-ons/support_render/ui/render_59")
        );
    }

    #[test]
    fn a_board_playing_several_passes_wraps_mid_run() {
        let cue = ScreenImageCue {
            images: (0..30)
                .map(|i| format!("add-ons/support_render/ui/render_{i}"))
                .collect(),
            sequence: None,
            fps: 15.0,
            seconds: 4.0, // two full passes
            aspect: 1.0,
            scale: 1.0,
            background: None,
        };
        assert!(cue.bounded());
        // Into the second pass it wraps normally...
        assert_eq!(
            cue.frame_key_at(2.0 + 2.0 / 15.0).as_deref(),
            Some("add-ons/support_render/ui/render_2")
        );
        // ...and holds its last frame past the end.
        assert_eq!(
            cue.frame_key_at(4.0 + 1.0 / 15.0).as_deref(),
            Some("add-ons/support_render/ui/render_29")
        );
    }

    #[test]
    fn a_sequence_names_numbered_frames() {
        let cue = ScreenImageCue {
            images: Vec::new(),
            sequence: Some(Sequence {
                prefix: "add-ons/support_render/ui/render_".into(),
                count: 60,
            }),
            fps: 30.0,
            seconds: 2.0,
            aspect: 1.0,
            scale: 1.0,
            background: None,
        };
        assert!(cue.bounded());
        assert_eq!(cue.frame_count(), 60);
        assert_eq!(
            cue.frame_key_at(0.0).as_deref(),
            Some("add-ons/support_render/ui/render_0")
        );
        assert_eq!(
            cue.frame_key_at(59.0 / 30.0).as_deref(),
            Some("add-ons/support_render/ui/render_59")
        );
    }

    #[test]
    fn the_last_frame_holds_past_the_end() {
        // A pass that ends exactly on `seconds` holds its final frame while
        // the client fades out, instead of wrapping to the first.
        let cue = ScreenImageCue {
            images: (0..60)
                .map(|i| format!("add-ons/support_render/ui/render_{i}"))
                .collect(),
            sequence: None,
            fps: 30.0,
            seconds: 2.0,
            aspect: 1.0,
            scale: 1.0,
            background: None,
        };
        assert_eq!(
            cue.frame_key_at(2.0).as_deref(),
            Some("add-ons/support_render/ui/render_59")
        );
        // Into the fade-out it still holds frame 59...
        assert_eq!(
            cue.frame_key_at(2.3).as_deref(),
            Some("add-ons/support_render/ui/render_59")
        );
        // ...while an earlier wrap point (t < seconds) wraps normally.
        let long = ScreenImageCue {
            images: vec!["a".into(), "b".into()],
            sequence: None,
            fps: 2.0,
            seconds: 4.0,
            aspect: 1.0,
            scale: 1.0,
            background: None,
        };
        assert_eq!(long.frame_key_at(1.0).as_deref(), Some("a"));
    }

    #[test]
    fn a_cue_outside_its_limits_is_not_bounded() {
        let mut cue = ScreenImageCue {
            images: vec!["a".into()],
            sequence: None,
            fps: 0.0, // below the floor
            seconds: 4.0,
            aspect: 1.0,
            scale: 1.0,
            background: None,
        };
        assert!(!cue.bounded());
        cue.fps = 30.0;
        cue.images.clear(); // no frames at all
        assert!(!cue.bounded());
    }

    #[test]
    fn a_sequence_without_a_prefix_is_not_bounded() {
        let cue = ScreenImageCue {
            images: Vec::new(),
            sequence: Some(Sequence {
                prefix: String::new(),
                count: 60,
            }),
            fps: 30.0,
            seconds: 2.0,
            aspect: 1.0,
            scale: 1.0,
            background: None,
        };
        assert!(!cue.bounded());
    }

    #[test]
    fn a_still_ignits_frame_zero() {
        let cue = ScreenImageCue {
            images: vec!["a".into()],
            sequence: None,
            fps: 1.0,
            seconds: 1.0,
            aspect: 1.0,
            scale: 1.0,
            background: None,
        };
        assert!(cue.bounded());
        assert_eq!(cue.frame_key_at(99.0).as_deref(), Some("a"));
    }
}
