//! Running well on the player's PC without asking: the first run picks a
//! graphics quality from the GPU, and every session logs its frame times so
//! a player on a weak PC can send numbers, not "it's laggy".
use crate::graphics::{ANTI_ALIASING, BRICK_SHADOWS, LIGHTING, REFLECTIONS};
use std::time::Duration;

/// Native pref: the quality the first run picked (and that it ran).
pub const AUTO_QUALITY: &str = "$pref::Video::AutoQuality";
const SHADOW_QUALITY: &str = "$pref::ShadowQuality";
const ANISOTROPY: &str = "$pref::OpenGL::anisotropy";
const PRECIPITATION: &str = "$pref::precipitationOn";
/// The options a quality choice sets. The values match Options' Graphics
/// Quality presets, so Options shows the chosen name, not Custom.
const QUALITY_PREFS: [&str; 7] = [
    SHADOW_QUALITY,
    ANTI_ALIASING,
    BRICK_SHADOWS,
    ANISOTROPY,
    PRECIPITATION,
    REFLECTIONS,
    LIGHTING,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Low,
    Medium,
    High,
}

impl Quality {
    pub fn name(self) -> &'static str {
        match self {
            Quality::Low => "Low",
            Quality::Medium => "Medium",
            Quality::High => "High",
        }
    }
    /// Preference values, as Options' presets write them.
    pub fn prefs(self) -> Vec<(String, String)> {
        let (shadows, aa, anisotropy, rain, reflections, lighting) = match self {
            // Shadow Quality 4 (Minimum) is shadows off; Classic lighting.
            Quality::Low => ("4", "0", "0", "0", "0", "0"),
            Quality::Medium => ("2", "1", "0.2", "1", "1", "2"),
            // The renderer's defaults.
            Quality::High => ("0", "1", "0.466667", "1", "2", "2"),
        };
        [
            (SHADOW_QUALITY, shadows),
            (ANTI_ALIASING, aa),
            (BRICK_SHADOWS, "0"),
            (ANISOTROPY, anisotropy),
            (PRECIPITATION, rain),
            (REFLECTIONS, reflections),
            (LIGHTING, lighting),
            (AUTO_QUALITY, self.name()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }
}

/// A quality for this GPU and screen: software rendering gets Low, a GPU
/// built into the processor gets Medium (Low above 1080p, where it fills
/// more than twice the pixels), and a graphics card gets High.
pub fn pick(device: wgpu::DeviceType, screen: Option<(u32, u32)>) -> Quality {
    let big = screen.is_some_and(|(w, h)| u64::from(w) * u64::from(h) > 1920 * 1080);
    match device {
        wgpu::DeviceType::Cpu => Quality::Low,
        wgpu::DeviceType::IntegratedGpu if big => Quality::Low,
        wgpu::DeviceType::IntegratedGpu => Quality::Medium,
        _ => Quality::High,
    }
}

/// True until a quality has been picked or the player chose any of the
/// options a quality sets.
pub fn first_run(prefs: &std::collections::BTreeMap<String, String>) -> bool {
    !prefs.keys().any(|k| {
        k.eq_ignore_ascii_case(AUTO_QUALITY)
            || QUALITY_PREFS.iter().any(|q| k.eq_ignore_ascii_case(q))
    })
}

/// Frame times over a reporting period, written to the session log.
#[derive(Debug, Default)]
pub struct FrameLog {
    frames: Vec<f32>,
    total: Duration,
}

/// How often a summary line is written.
pub const PERIOD: Duration = Duration::from_secs(60);

impl FrameLog {
    /// Record one frame. Returns the summary line when a period is complete.
    pub fn frame(&mut self, elapsed: Duration) -> Option<String> {
        // Suspensions and debugger pauses are not frames.
        if elapsed > Duration::from_secs(2) {
            return None;
        }
        self.frames.push(elapsed.as_secs_f32() * 1000.0);
        self.total += elapsed;
        if self.total < PERIOD {
            return None;
        }
        let line = summarize(&mut self.frames, self.total);
        self.frames.clear();
        self.total = Duration::ZERO;
        line
    }
}

fn summarize(frames: &mut [f32], total: Duration) -> Option<String> {
    if frames.is_empty() {
        return None;
    }
    frames.sort_by(f32::total_cmp);
    let n = frames.len();
    let average = total.as_secs_f32() * 1000.0 / n as f32;
    // The frame 99% of frames beat: what a player feels as stutter.
    let slow = frames[(n * 99 / 100).min(n - 1)];
    let median = frames[n / 2];
    let worst = frames[n - 1];
    let over = frames.iter().filter(|&&ms| ms > 33.4).count();
    Some(format!(
        "Frame times over {:.0} s: {n} frames, {:.0} fps average, median {median:.1} ms, 1% slowest {slow:.1} ms, worst {worst:.1} ms, {over} frames under 30 fps",
        total.as_secs_f32(),
        1000.0 / average,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn the_gpu_picks_the_quality() {
        use wgpu::DeviceType::*;
        assert_eq!(pick(Cpu, None), Quality::Low);
        assert_eq!(pick(IntegratedGpu, Some((1920, 1080))), Quality::Medium);
        assert_eq!(pick(IntegratedGpu, Some((2560, 1440))), Quality::Low);
        assert_eq!(pick(DiscreteGpu, Some((3840, 2160))), Quality::High);
        assert_eq!(pick(Other, None), Quality::High);
    }

    #[test]
    fn a_quality_is_picked_once_and_never_over_the_players_choice() {
        let mut prefs = BTreeMap::new();
        assert!(first_run(&prefs));
        prefs.insert("$Pref::ShadowQuality".into(), "1".into());
        assert!(!first_run(&prefs), "the player set shadows");
        let mut prefs: BTreeMap<_, _> = Quality::Medium.prefs().into_iter().collect();
        assert!(!first_run(&prefs));
        prefs.retain(|k, _| k == AUTO_QUALITY);
        assert!(!first_run(&prefs), "picked before");
    }

    #[test]
    fn quality_values_drive_the_renderer() {
        let graphics = |q: Quality| {
            let mut settings = bri_ui::api::Settings::default();
            settings.prefs.extend(q.prefs());
            crate::graphics::Graphics::from_settings(&settings)
        };
        let low = graphics(Quality::Low);
        assert_eq!((low.samples, low.shadows), (1, None));
        assert_eq!(low.lighting, 0, "Low is Classic lighting");
        assert_eq!(low.filtering.anisotropy, 1);
        let high = graphics(Quality::High);
        let defaults = crate::graphics::Graphics::from_settings(&Default::default());
        assert_eq!(high, defaults, "High is the renderer's defaults");
        let medium = graphics(Quality::Medium);
        assert_eq!(medium.samples, 4);
        assert_eq!(medium.shadows, crate::graphics::shadow_settings(2));
    }

    #[test]
    fn frame_times_are_summarized_each_period() {
        let mut log = FrameLog::default();
        let mut line = None;
        for i in 0..3700 {
            let ms = if i % 100 == 0 { 50 } else { 16 };
            if let Some(l) = log.frame(Duration::from_millis(ms)) {
                line = Some(l);
                break;
            }
        }
        let line = line.expect("a minute of frames");
        assert!(line.contains("median 16.0 ms"), "{line}");
        assert!(line.contains("worst 50.0 ms"), "{line}");
        assert!(line.contains("frames under 30 fps"), "{line}");
        assert!(
            log.frame(Duration::from_secs(5)).is_none(),
            "pauses are skipped"
        );
    }
}
