//! Measurements behind the net graph (Ctrl+N) and the performance overlay
//! (F3): connection samples, the frame's GPU time, process memory and the
//! capture file. Each runs only while its display is showing.
use anyhow::{Context, Result};
use bri_net::client::{LinkProbe, LinkSample};
use bri_ui::models::perf::{FrameSample, NET_GRAPH_PERIOD_MS, NetSample, PerfStats};
use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How far back packet loss looks (`getPacketLoss` is a recent average).
const LOSS_WINDOW: Duration = Duration::from_secs(4);

/// Turns the connection's running counters into `NetGraph::updateStats`
/// samples every 32 ms.
#[derive(Default)]
pub struct NetSampler {
    last: Option<(Instant, LinkSample)>,
    /// (when, packets sent, packets lost) for the loss window.
    history: VecDeque<(Instant, u64, u64)>,
}

impl NetSampler {
    /// Forget the previous connection's counters.
    pub fn reset(&mut self) {
        *self = NetSampler::default();
    }
    /// A sample when one is due. `ghosts` counts the replicated objects.
    pub fn sample(&mut self, now: Instant, probe: &LinkProbe, ghosts: usize) -> Option<NetSample> {
        if let Some((at, _)) = self.last
            && now.duration_since(at) < Duration::from_millis(NET_GRAPH_PERIOD_MS)
        {
            return None;
        }
        self.sample_link(now, probe.sample(), ghosts)
    }
    fn sample_link(&mut self, now: Instant, link: LinkSample, ghosts: usize) -> Option<NetSample> {
        let previous = self.last.replace((now, link));
        self.history
            .push_back((now, link.sent_packets, link.lost_packets));
        while self
            .history
            .front()
            .is_some_and(|(at, ..)| now.duration_since(*at) > LOSS_WINDOW)
        {
            self.history.pop_front();
        }
        let (at, before) = previous?;
        let (sent0, lost0) = self.history.front().map_or((0, 0), |&(_, s, l)| (s, l));
        let sent = link.sent_packets.saturating_sub(sent0);
        let lost = link.lost_packets.saturating_sub(lost0);
        let d = |a: u64, b: u64| a.saturating_sub(b) as f32;
        Some(NetSample {
            interval_ms: now.duration_since(at).as_secs_f32() * 1000.0,
            ghosts_active: ghosts as f32,
            ghost_updates: d(link.updates, before.updates),
            bits_sent: d(link.sent_bytes, before.sent_bytes) * 8.0,
            bits_received: d(link.received_bytes, before.received_bytes) * 8.0,
            latency_ms: link.rtt.as_secs_f32() * 1000.0,
            packet_loss: if sent > 0 {
                (lost as f32 / sent as f32 * 100.0).min(100.0)
            } else {
                0.0
            },
            packets_sent: d(link.sent_packets, before.sent_packets),
            packets_received: d(link.received_packets, before.received_packets),
        })
    }
}

/// The process's working set and private (committed) bytes.
#[cfg(windows)]
pub fn process_memory() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    // SAFETY: the pseudo-handle needs no closing; the struct is sized for
    // the call, which fills it in place.
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters.cb,
        )
    };
    (ok != 0).then_some((counters.WorkingSetSize as u64, counters.PrivateUsage as u64))
}
#[cfg(not(windows))]
pub fn process_memory() -> Option<(u64, u64)> {
    None
}

/// Main-thread timing of one frame, from the platform loop.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameTiming {
    /// Present to present.
    pub frame: Duration,
    /// Update, UI and recording the frame's commands.
    pub cpu: Duration,
    /// Acquiring the swapchain image and presenting.
    pub wait: Duration,
    /// The latest GPU time that has come back.
    pub gpu: Option<Duration>,
}

impl FrameTiming {
    pub fn sample(&self) -> FrameSample {
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        FrameSample {
            frame_ms: ms(self.frame),
            cpu_ms: ms(self.cpu),
            wait_ms: ms(self.wait),
            gpu_ms: self.gpu.map(ms),
        }
    }
}

/// Times a frame's command encoder on the GPU, first thing to last
/// (`bri_render::timing::GpuTimer` with no stretches marked in between).
/// Readbacks are asynchronous, a few frames late.
pub struct GpuFrameTimer(bri_render::timing::GpuTimer);

impl GpuFrameTimer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        bri_render::timing::GpuTimer::new(device, queue).map(Self)
    }
    /// First thing in the frame's encoder. Skips the frame when every
    /// readback is still in flight.
    pub fn begin(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.0.begin(encoder);
    }
    /// Last thing in the frame's encoder, before it is submitted.
    pub fn end(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.0.end(encoder, "frame");
    }
    /// After submitting: start reading submitted slots and take any that
    /// finished. Returns the latest GPU time known.
    pub fn collect(&mut self, device: &wgpu::Device) -> Option<Duration> {
        self.0.collect(device).map(|(whole, _)| *whole)
    }
}

/// What Ctrl+F3 writes: enough to attach to a bug report.
#[derive(Serialize)]
pub struct Capture<'a> {
    pub schema: &'static str,
    pub version: &'a str,
    /// Seconds since the Unix epoch.
    pub saved_at: u64,
    pub summary: bri_ui::models::perf::FrameSummary,
    pub stats: &'a PerfStats,
    /// Newest first.
    pub frames: Vec<FrameSample>,
    /// The net graph's samples when it is showing, newest first.
    pub net: Vec<NetSample>,
    pub resolution: (i32, i32),
}

pub const CAPTURE_SCHEMA: &str = "bri-perf-capture/1";

/// Write the overlay's history as `perf-<time>.json` in `dir`.
pub fn save_capture(
    dir: &std::path::Path,
    core: &bri_ui::ui::Core,
    version: &str,
) -> Result<std::path::PathBuf> {
    let saved_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut net: Vec<NetSample> = core
        .net_graph
        .as_ref()
        .map(|g| g.samples().copied().collect())
        .unwrap_or_default();
    if net.is_empty()
        && let Some(latest) = core.perf.net
    {
        net.push(latest);
    }
    let capture = Capture {
        schema: CAPTURE_SCHEMA,
        version,
        saved_at,
        summary: core.perf.summary(),
        stats: &core.perf.stats,
        frames: core.perf.frames().copied().collect(),
        net,
        resolution: core.logical,
    };
    let bytes = serde_json::to_vec_pretty(&capture)?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("perf-{saved_at}.json"));
    bri_files::replace(&path, &bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(sent: u64, received: u64, lost: u64, bytes: u64, updates: u64) -> LinkSample {
        LinkSample {
            rtt: Duration::from_millis(40),
            sent_packets: sent,
            received_packets: received,
            sent_bytes: bytes,
            received_bytes: bytes * 2,
            lost_packets: lost,
            updates,
        }
    }

    #[test]
    fn net_samples_are_differences_between_link_counters() {
        let t0 = Instant::now();
        let mut s = NetSampler::default();
        assert!(
            s.sample_link(t0, link(10, 10, 0, 1000, 5), 3).is_none(),
            "first sample only sets a baseline"
        );
        let n = s
            .sample_link(t0 + Duration::from_millis(32), link(14, 19, 0, 1100, 9), 3)
            .unwrap();
        assert_eq!(n.interval_ms, 32.0);
        assert_eq!(n.ghosts_active, 3.0);
        assert_eq!(n.ghost_updates, 4.0);
        assert_eq!(n.bits_sent, 800.0);
        assert_eq!(n.bits_received, 1600.0);
        assert_eq!(n.packets_sent, 4.0);
        assert_eq!(n.packets_received, 9.0);
        assert_eq!(n.latency_ms, 40.0);
        assert_eq!(n.packet_loss, 0.0);
    }

    #[test]
    fn packet_loss_is_a_percentage_over_the_last_seconds() {
        let t0 = Instant::now();
        let mut s = NetSampler::default();
        s.sample_link(t0, link(100, 0, 0, 0, 0), 0);
        let n = s
            .sample_link(t0 + Duration::from_secs(1), link(200, 0, 5, 0, 0), 0)
            .unwrap();
        assert_eq!(n.packet_loss, 5.0);
        // Old losses leave the window.
        let n = s
            .sample_link(t0 + Duration::from_secs(10), link(300, 0, 5, 0, 0), 0)
            .unwrap();
        assert_eq!(n.packet_loss, 0.0);
    }

    #[test]
    fn process_memory_reads_this_process() {
        if cfg!(windows) {
            let (working, private) = process_memory().unwrap();
            assert!(working > 1024 * 1024 && private > 0);
        }
    }

    #[test]
    fn frame_timing_converts_to_milliseconds() {
        let t = FrameTiming {
            frame: Duration::from_millis(16),
            cpu: Duration::from_millis(5),
            wait: Duration::from_millis(9),
            gpu: Some(Duration::from_micros(4500)),
        };
        let f = t.sample();
        assert_eq!(
            (f.frame_ms, f.cpu_ms, f.wait_ms, f.gpu_ms),
            (16.0, 5.0, 9.0, Some(4.5))
        );
    }
}

/// Startup phases, logged once each against the time the game started, so a
/// slow start on a player's PC shows which step took the time.
pub mod startup {
    use std::{sync::OnceLock, time::Instant};

    static START: OnceLock<Instant> = OnceLock::new();

    /// The moment the game started; call first thing in `main`.
    pub fn begin() {
        START.get_or_init(Instant::now);
    }
    /// Milliseconds since `begin`.
    pub fn elapsed_ms() -> u128 {
        START.get_or_init(Instant::now).elapsed().as_millis()
    }
    /// Log that `phase` finished.
    pub fn mark(phase: &str) {
        bri_console::echo(format!("Startup: {phase} at {} ms", elapsed_ms()));
    }
}

/// Work the app has done since it loaded, for frame probes and soak tests.
/// Counts, unlike times, are the same on every machine: in steady play the
/// costly kinds (whole uploads, model and look builds, passes over the
/// whole world) must stop growing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct WorkCounters {
    /// Brick chunk rebuild jobs started and the chunks they rebuilt.
    pub chunk_jobs: u64,
    pub chunks_rebuilt: u64,
    /// What the current scene renderer uploaded (None before the GPU is
    /// ready). A new renderer (device or map change) starts from zero.
    pub uploads: Option<bri_render::scene::UploadCounts>,
    /// World-item (held, lying, thrown) models built and uploaded whole.
    pub item_model_builds: u64,
    pub item_model_uploads: u64,
    /// Debris looks built and uploaded.
    pub debris_looks_built: u64,
    /// Music bricks: whole-world passes and bricks examined.
    pub music_full_scans: u64,
    pub music_visited: u64,
    /// Hidden-brick outlines.
    pub hidden_outlines: crate::hidden_outlines::HiddenOutlineDiagnostics,
}
