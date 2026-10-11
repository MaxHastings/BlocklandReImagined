//! Running well on the player's PC without asking: the first run picks a
//! graphics quality from the GPU, and every session logs its frame times so
//! a player on a weak PC can send numbers, not "it's laggy".
use crate::graphics::{ANTI_ALIASING, BRICK_SHADOWS, LIGHTING, REFLECTIONS};
use std::fmt::Write as _;
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

/// Frame times over a reporting period, written to the session log, and a
/// line for each long frame naming the work that took it.
///
/// Frames drawn while the window is unfocused or hidden are paced by the
/// background timer (at most 20 a second, see `platform.rs`), so they are
/// counted apart: a minute spent alt-tabbed never reads as a 20 fps GPU.
#[derive(Debug, Default)]
pub struct FrameLog {
    /// Focused frames, in ms.
    frames: Vec<f32>,
    focused: Duration,
    background_frames: usize,
    background: Duration,
    total: Duration,
    /// Main-thread time per top-level span over focused frames.
    work: Vec<(&'static str, Duration)>,
    /// GPU time of each drawn frame read back this period, in ms, and the
    /// summed time of each pass.
    gpu: Vec<f32>,
    passes: Vec<(&'static str, Duration)>,
    long: LongFrames,
}

/// How often a summary line is written.
pub const PERIOD: Duration = Duration::from_secs(60);

/// A focused frame at least this long gets its own log line.
pub const LONG_FRAME: Duration = Duration::from_millis(50);
/// At most this many long-frame lines per period, at least
/// [`LONG_FRAME_GAP`] apart; the rest are counted in the summary.
const LONG_FRAMES_LOGGED: u32 = 20;
const LONG_FRAME_GAP: Duration = Duration::from_secs(1);
/// Frames the "usual" frame time is the median of.
const RECENT: usize = 30;

#[derive(Debug, Default)]
struct LongFrames {
    recent: std::collections::VecDeque<f32>,
    /// Time since the last long-frame line (None before the first).
    since_logged: Option<Duration>,
    logged: u32,
    unlogged: u32,
}

impl LongFrames {
    fn frame(&mut self, record: &crate::frame_trace::FrameRecord) -> Option<String> {
        let ms = record.total.as_secs_f32() * 1000.0;
        if let Some(since) = &mut self.since_logged {
            *since += record.total;
        }
        let usual = {
            let mut recent: Vec<f32> = self.recent.iter().copied().collect();
            recent.sort_by(f32::total_cmp);
            recent.get(recent.len() / 2).copied()
        };
        if !record.background {
            if self.recent.len() == RECENT {
                self.recent.pop_front();
            }
            self.recent.push_back(ms);
        }
        if record.background || record.total < LONG_FRAME {
            return None;
        }
        let spaced = self
            .since_logged
            .is_none_or(|since| since >= LONG_FRAME_GAP);
        if !spaced || self.logged >= LONG_FRAMES_LOGGED {
            self.unlogged += 1;
            return None;
        }
        self.logged += 1;
        self.since_logged = Some(Duration::ZERO);
        let usual = usual.map_or(String::new(), |u| format!(" (usual {u:.1} ms)"));
        Some(format!(
            "Long frame: {ms:.1} ms{usual}, window focused: {}",
            record.describe()
        ))
    }
    /// Long frames this period, and how many of them had no line.
    fn end_period(&mut self) -> (u32, u32) {
        let counts = (self.logged + self.unlogged, self.unlogged);
        self.logged = 0;
        self.unlogged = 0;
        counts
    }
}

impl FrameLog {
    /// Record one frame. Returns the lines to log: a long frame's own line,
    /// and the period's summary when a period is complete.
    pub fn frame(&mut self, record: &crate::frame_trace::FrameRecord) -> Vec<String> {
        let mut lines = Vec::new();
        lines.extend(self.long.frame(record));
        let elapsed = record.total;
        // Suspensions and debugger pauses are not frames.
        if elapsed > Duration::from_secs(2) {
            return lines;
        }
        if record.background {
            self.background_frames += 1;
            self.background += elapsed;
        } else {
            self.frames.push(elapsed.as_secs_f32() * 1000.0);
            self.focused += elapsed;
            for span in record.spans.iter().filter(|s| s.parent.is_none()) {
                add_time(&mut self.work, span.name, span.time);
            }
        }
        self.total += elapsed;
        if self.total < PERIOD {
            return lines;
        }
        lines.extend(self.summary());
        lines
    }
    /// A drawn frame's GPU time as read back (a few frames late), whole
    /// and per pass.
    pub fn gpu(&mut self, whole: Duration, passes: &[(&'static str, Duration)]) {
        self.gpu.push(whole.as_secs_f32() * 1000.0);
        for (pass, time) in passes {
            add_time(&mut self.passes, pass, *time);
        }
    }
    fn summary(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        let (long, unlogged) = self.long.end_period();
        let total = self.total.as_secs_f32();
        let mut first = match summarize(&mut self.frames, self.focused) {
            Some(stats) if self.background_frames == 0 => {
                format!("Frame times over {total:.0} s: {stats}")
            }
            Some(stats) => format!(
                "Frame times over {total:.0} s ({:.0} s focused): {stats}",
                self.focused.as_secs_f32()
            ),
            None => format!("Frame times over {total:.0} s: no focused frames"),
        };
        if self.background_frames > 0 {
            let _ = write!(
                first,
                "; {} more frames over {:.0} s with the window unfocused or hidden, paced at 20 fps on purpose and not counted here",
                self.background_frames,
                self.background.as_secs_f32()
            );
        }
        if long > 0 {
            let _ = write!(
                first,
                "; {long} long frames (over {} ms)",
                LONG_FRAME.as_millis()
            );
            if unlogged > 0 {
                let _ = write!(first, ", {unlogged} without their own line");
            }
        }
        lines.push(first);
        let focused_frames = self.frames.len();
        if focused_frames > 0 && !self.work.is_empty() {
            self.work.sort_by(|a, b| b.1.cmp(&a.1));
            let per = |d: Duration| d.as_secs_f64() * 1000.0 / focused_frames as f64;
            let parts: Vec<String> = self
                .work
                .iter()
                .filter(|(_, d)| per(*d) >= 0.05)
                .map(|(name, d)| format!("{name} {:.2}", per(*d)))
                .collect();
            lines.push(format!(
                "Main thread per focused frame, average ms: {}",
                parts.join(", ")
            ));
        }
        if !self.gpu.is_empty() {
            let timed = self.gpu.len();
            self.gpu.sort_by(f32::total_cmp);
            let median = self.gpu[timed / 2];
            let slow = self.gpu[(timed * 99 / 100).min(timed - 1)];
            let worst = self.gpu[timed - 1];
            let parts: Vec<String> = self
                .passes
                .iter()
                .map(|(name, d)| format!("{name} {:.2}", d.as_secs_f64() * 1000.0 / timed as f64))
                .collect();
            lines.push(format!(
                "GPU time per drawn frame over {total:.0} s: median {median:.1} ms, 1% slowest {slow:.1} ms, worst {worst:.1} ms ({timed} frames timed); average ms per pass: {}",
                parts.join(", ")
            ));
        }
        let long = std::mem::take(&mut self.long);
        *self = FrameLog {
            long,
            ..Default::default()
        };
        lines
    }
}

fn add_time(totals: &mut Vec<(&'static str, Duration)>, name: &'static str, time: Duration) {
    match totals.iter_mut().find(|(n, _)| *n == name) {
        Some((_, total)) => *total += time,
        None => totals.push((name, time)),
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
        "{n} frames, {:.0} fps average, median {median:.1} ms, 1% slowest {slow:.1} ms, worst {worst:.1} ms, {over} frames under 30 fps",
        1000.0 / average,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_trace::FrameRecord;
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

    fn frame(ms: f32, background: bool) -> FrameRecord {
        FrameRecord {
            total: Duration::from_secs_f32(ms / 1000.0),
            background,
            ..Default::default()
        }
    }

    /// Feed frames until a summary comes out; every line logged on the way.
    fn run(log: &mut FrameLog, mut next: impl FnMut(usize) -> FrameRecord) -> Vec<String> {
        let mut lines = Vec::new();
        for i in 0..100_000 {
            let out = log.frame(&next(i));
            let done = out.iter().any(|l| l.starts_with("Frame times"));
            lines.extend(out);
            if done {
                return lines;
            }
        }
        panic!("no summary: {lines:?}");
    }

    fn summary(lines: &[String]) -> &str {
        lines.iter().find(|l| l.starts_with("Frame times")).unwrap()
    }

    #[test]
    fn frame_times_are_summarized_each_period() {
        let mut log = FrameLog::default();
        let lines = run(&mut log, |i| {
            frame(if i % 100 == 0 { 50.0 } else { 16.0 }, false)
        });
        let line = summary(&lines);
        assert!(line.starts_with("Frame times over 60 s: "), "{line}");
        assert!(line.contains("median 16.0 ms"), "{line}");
        assert!(line.contains("worst 50.0 ms"), "{line}");
        assert!(line.contains("frames under 30 fps"), "{line}");
        assert!(!line.contains("unfocused"), "{line}");
        assert!(
            log.frame(&frame(5000.0, true)).is_empty(),
            "pauses are skipped"
        );
    }

    /// v0.2.8 session logs showed whole minutes at a 50.3 ms median, which
    /// read like a 20 fps GPU; they were the background timer while the
    /// window was unfocused. Those frames are reported apart.
    #[test]
    fn background_frames_never_count_as_slow_frames() {
        let mut log = FrameLog::default();
        let lines = run(&mut log, |i| {
            if i < 4900 {
                frame(6.1, false)
            } else {
                frame(50.3, true)
            }
        });
        let line = summary(&lines);
        assert!(
            line.contains("Frame times over 60 s (30 s focused): 4900 frames"),
            "{line}"
        );
        assert!(line.contains("median 6.1 ms"), "{line}");
        assert!(
            line.contains("worst 6.1 ms, 0 frames under 30 fps"),
            "{line}"
        );
        assert!(
            line.contains("more frames over 30 s with the window unfocused or hidden"),
            "{line}"
        );
        assert!(
            !lines.iter().any(|l| l.starts_with("Long frame")),
            "background frames are paced, not long: {lines:?}"
        );
        let mut log = FrameLog::default();
        let lines = run(&mut log, |_| frame(50.3, true));
        assert!(summary(&lines).contains("no focused frames"), "{lines:?}");
    }

    #[test]
    fn a_long_frame_names_the_work_that_took_it() {
        let mut log = FrameLog::default();
        for _ in 0..40 {
            assert!(log.frame(&frame(6.0, false)).is_empty());
        }
        let long = FrameRecord {
            total: Duration::from_millis(390),
            background: false,
            spans: vec![
                crate::frame_trace::SpanTime {
                    name: "draw",
                    parent: None,
                    time: Duration::from_millis(380),
                    count: 1,
                },
                crate::frame_trace::SpanTime {
                    name: "world upload",
                    parent: Some("draw"),
                    time: Duration::from_millis(350),
                    count: 1,
                },
            ],
            notes: vec!["graphics rebuilt".into()],
        };
        let lines = log.frame(&long);
        assert_eq!(
            lines,
            vec![
                "Long frame: 390.0 ms (usual 6.0 ms), window focused: draw 380.0 ms \
                 (world upload 350.0), untraced 10.0 ms; events: graphics rebuilt"
                    .to_string()
            ]
        );
        // A burst of long frames: a line a second at most, the rest counted.
        let mut logged = 1;
        let lines = run(&mut log, |i| frame(if i < 10 { 390.0 } else { 6.0 }, false));
        logged += lines.iter().filter(|l| l.starts_with("Long frame")).count();
        assert_eq!(logged, 4, "{lines:?}");
        assert!(
            summary(&lines).contains("; 11 long frames (over 50 ms), 7 without their own line"),
            "{lines:?}"
        );
    }

    #[test]
    fn gpu_and_main_thread_time_are_averaged_per_frame() {
        let mut log = FrameLog::default();
        for _ in 0..4 {
            log.gpu(
                Duration::from_millis(4),
                &[
                    ("sun shadows", Duration::from_millis(1)),
                    ("world", Duration::from_millis(3)),
                ],
            );
        }
        log.gpu(
            Duration::from_millis(40),
            &[("world", Duration::from_millis(40))],
        );
        let lines = run(&mut log, |_| FrameRecord {
            total: Duration::from_millis(10),
            background: false,
            spans: vec![crate::frame_trace::SpanTime {
                name: "update",
                parent: None,
                time: Duration::from_millis(2),
                count: 1,
            }],
            notes: Vec::new(),
        });
        let main = lines.iter().find(|l| l.starts_with("Main thread")).unwrap();
        assert_eq!(
            main,
            "Main thread per focused frame, average ms: update 2.00"
        );
        let gpu = lines.iter().find(|l| l.starts_with("GPU time")).unwrap();
        assert_eq!(
            gpu,
            "GPU time per drawn frame over 60 s: median 4.0 ms, 1% slowest 40.0 ms, worst 40.0 ms \
             (5 frames timed); average ms per pass: sun shadows 0.80, world 10.40"
        );
        // The next period starts empty.
        let lines = run(&mut log, |_| frame(10.0, false));
        assert!(
            !lines.iter().any(|l| l.starts_with("GPU time")),
            "{lines:?}"
        );
    }
}
