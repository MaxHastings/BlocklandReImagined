//! The net graph (v20's `NetGraphGui`, Ctrl+N) and the performance overlay
//! (not in v20, F3). This holds their history and modes only: the host
//! samples and pushes [`NetSample`]s and [`FrameSample`]s, and
//! `screens::perf` draws. Both are empty and cost nothing while hidden.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// `GuiGraphCtrl`'s `MaxDataPoints`: one sample per pixel of the 200-pixel
/// graph (`allClientGuis.gui` `NetGraph` extent "200 200").
pub const NET_GRAPH_POINTS: usize = 200;
/// `NetGraph::updateStats` reschedules itself every 32 ms.
pub const NET_GRAPH_PERIOD_MS: u64 = 32;
/// The plots in `NetGraph::updateStats` order, which is also their colours'
/// order (`GuiGraphCtrl`'s six plot colours and the `NetGraph*Profile`s).
pub const NET_PLOTS: usize = 6;

/// One `NetGraph::updateStats` sample.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct NetSample {
    /// Milliseconds since the previous sample (32 when on time).
    pub interval_ms: f32,
    /// `ServerConnection.getGhostsActive()`: replicated objects this client
    /// knows (players, vehicles and Add-On entities).
    pub ghosts_active: f32,
    /// `$Stats::netGhostUpdates`: replica updates applied since the last
    /// sample (world deltas and pose datagrams).
    pub ghost_updates: f32,
    /// `$Stats::netBitsSent` / `netBitsReceived`: bits on the wire since the
    /// last sample, UDP payload and QUIC headers included.
    pub bits_sent: f32,
    pub bits_received: f32,
    /// `ServerConnection.getPing()`, milliseconds.
    pub latency_ms: f32,
    /// `ServerConnection.getPacketLoss()`: percent of the packets sent in
    /// the last few seconds that were lost.
    pub packet_loss: f32,
    /// Not graphed in v20: packets since the last sample, each way.
    pub packets_sent: f32,
    pub packets_received: f32,
}

impl NetSample {
    /// The six graphed values, in plot order.
    pub fn plots(&self) -> [f32; NET_PLOTS] {
        [
            self.ghosts_active,
            self.ghost_updates,
            self.bits_sent,
            self.bits_received,
            self.latency_ms,
            self.packet_loss,
        ]
    }
    /// Scale a per-sample count to a per-second rate.
    pub fn per_second(&self, count: f32) -> f32 {
        if self.interval_ms > 0.0 {
            count * 1000.0 / self.interval_ms
        } else {
            0.0
        }
    }
}

/// `NetGraph` (`GuiGraphCtrl`): six plots of the last 200 samples.
#[derive(Debug, Clone, Default)]
pub struct NetGraph {
    /// Newest first, as `GuiGraphCtrl::addDatum` keeps them.
    samples: VecDeque<NetSample>,
}

impl NetGraph {
    pub fn add(&mut self, sample: NetSample) {
        if self.samples.len() == NET_GRAPH_POINTS {
            self.samples.pop_back();
        }
        self.samples.push_front(sample);
    }
    pub fn latest(&self) -> Option<&NetSample> {
        self.samples.front()
    }
    /// Newest first.
    pub fn samples(&self) -> impl Iterator<Item = &NetSample> {
        self.samples.iter()
    }
    /// One plot's values, newest first.
    pub fn plot(&self, plot: usize) -> impl Iterator<Item = f32> + '_ {
        self.samples.iter().map(move |s| s.plots()[plot])
    }
    /// The value a plot's top edge stands for. `GuiGraphCtrl` scales each
    /// plot to its own largest value; `NetGraph.matchScale(2, 3)` gives bits
    /// sent and received one shared scale so they compare.
    pub fn scale(&self, plot: usize) -> f32 {
        let max = |p: usize| self.plot(p).fold(0.0_f32, f32::max);
        match plot {
            2 | 3 => max(2).max(max(3)),
            _ => max(plot),
        }
    }
}

/// How much of the performance overlay shows. Its key cycles through them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerfMode {
    #[default]
    Off,
    /// A small corner readout: frame rate, frame time and a short graph.
    Compact,
    /// Everything: the frame graph, CPU/GPU split, server, network, world,
    /// memory and per-Add-On script time.
    Expanded,
}

impl PerfMode {
    pub fn next(self) -> PerfMode {
        match self {
            PerfMode::Off => PerfMode::Compact,
            PerfMode::Compact => PerfMode::Expanded,
            PerfMode::Expanded => PerfMode::Off,
        }
    }
}

/// Frames the overlay keeps: its graph is one pixel per frame.
pub const FRAME_HISTORY: usize = 240;

/// One presented frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameSample {
    /// Time since the previous frame was presented.
    pub frame_ms: f32,
    /// The main thread's work: game update, UI and recording draw commands.
    pub cpu_ms: f32,
    /// Waiting for the swapchain and presenting (VSync and a busy GPU show
    /// up here).
    pub wait_ms: f32,
    /// GPU time for the frame's commands, where the GPU has timestamps. It
    /// comes back a few frames late, so this is the latest measured.
    pub gpu_ms: Option<f32>,
}

/// The host's simulation, when this game hosts (`bri_net::server::ServerPerf`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerStats {
    pub ticks_per_second: f32,
    pub tick_ms_mean: f32,
    pub tick_ms_max: f32,
    /// Script time per Add-On package, per simulation step, busiest first.
    pub script_ms: Vec<(String, f32)>,
}

/// Slower-changing figures the host refreshes a few times a second.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PerfStats {
    /// Bricks, players, vehicles and Add-On entities in the world, in game.
    pub bricks: Option<usize>,
    pub players: Option<usize>,
    pub vehicles: Option<usize>,
    pub entities: Option<usize>,
    /// The process's working set and committed private memory.
    pub memory_bytes: Option<u64>,
    pub private_bytes: Option<u64>,
    /// None when another computer hosts: its tick time stays on that host.
    pub server: Option<ServerStats>,
    /// Whether this game is connected to another computer's server.
    pub remote_server: bool,
    pub gpu: String,
    /// GPU ms per world pass (sun shadows, lamp shadows, mirrors, world,
    /// effects), in frame order, where the GPU has timestamps.
    #[serde(default)]
    pub gpu_passes: Vec<(String, f32)>,
}

/// Frame-time figures over the recent history.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameSummary {
    /// Frames per second over about the last second.
    pub fps: f32,
    pub frame_ms: f32,
    /// The slowest frame in the history.
    pub worst_ms: f32,
    pub cpu_ms: f32,
    pub wait_ms: f32,
    pub gpu_ms: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct PerfOverlay {
    pub mode: PerfMode,
    /// Newest first.
    frames: VecDeque<FrameSample>,
    pub stats: PerfStats,
    /// The latest net sample, sampled while the overlay is expanded.
    pub net: Option<NetSample>,
}

impl PerfOverlay {
    pub fn visible(&self) -> bool {
        self.mode != PerfMode::Off
    }
    /// Whether the host should sample the connection for this overlay.
    pub fn wants_net(&self) -> bool {
        self.mode == PerfMode::Expanded
    }
    /// Cycle the mode; hiding drops the history so none is kept (or shown
    /// stale) while off.
    pub fn cycle(&mut self) {
        self.mode = self.mode.next();
        if self.mode == PerfMode::Off {
            *self = PerfOverlay::default();
        }
    }
    pub fn push_frame(&mut self, frame: FrameSample) {
        if !self.visible() {
            return;
        }
        if self.frames.len() == FRAME_HISTORY {
            self.frames.pop_back();
        }
        self.frames.push_front(frame);
    }
    /// Newest first.
    pub fn frames(&self) -> impl Iterator<Item = &FrameSample> {
        self.frames.iter()
    }
    /// Averages over the frames of about the last second (at least one),
    /// so the numbers settle instead of flickering every frame.
    pub fn summary(&self) -> FrameSummary {
        let mut n = 0;
        let mut total = 0.0;
        let mut cpu = 0.0;
        let mut wait = 0.0;
        for f in &self.frames {
            n += 1;
            total += f.frame_ms;
            cpu += f.cpu_ms;
            wait += f.wait_ms;
            if total >= 1000.0 {
                break;
            }
        }
        if n == 0 {
            return FrameSummary::default();
        }
        let count = n as f32;
        FrameSummary {
            fps: if total > 0.0 {
                count * 1000.0 / total
            } else {
                0.0
            },
            frame_ms: total / count,
            worst_ms: self.frames.iter().map(|f| f.frame_ms).fold(0.0, f32::max),
            cpu_ms: cpu / count,
            wait_ms: wait / count,
            gpu_ms: self.frames.iter().find_map(|f| f.gpu_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(bits_sent: f32, bits_received: f32, latency: f32) -> NetSample {
        NetSample {
            interval_ms: 32.0,
            bits_sent,
            bits_received,
            latency_ms: latency,
            ..Default::default()
        }
    }

    #[test]
    fn net_graph_keeps_two_hundred_samples_newest_first() {
        let mut g = NetGraph::default();
        for i in 0..250 {
            g.add(sample(0.0, 0.0, i as f32));
        }
        assert_eq!(g.samples().count(), NET_GRAPH_POINTS);
        assert_eq!(g.latest().unwrap().latency_ms, 249.0);
        assert_eq!(g.plot(4).last(), Some(50.0));
    }

    #[test]
    fn bits_sent_and_received_share_a_scale_like_match_scale() {
        let mut g = NetGraph::default();
        g.add(sample(100.0, 4000.0, 30.0));
        g.add(sample(300.0, 1000.0, 80.0));
        assert_eq!(g.scale(2), 4000.0);
        assert_eq!(g.scale(3), 4000.0);
        assert_eq!(g.scale(4), 80.0);
        assert_eq!(g.scale(5), 0.0);
    }

    #[test]
    fn per_second_rates_follow_the_sample_interval() {
        let s = NetSample {
            interval_ms: 32.0,
            packets_received: 4.0,
            ..Default::default()
        };
        assert_eq!(s.per_second(s.packets_received), 125.0);
        assert_eq!(NetSample::default().per_second(10.0), 0.0);
    }

    #[test]
    fn perf_mode_cycles_off_compact_expanded() {
        let mut o = PerfOverlay::default();
        o.push_frame(FrameSample::default());
        assert_eq!(o.frames().count(), 0, "hidden overlay keeps nothing");
        o.cycle();
        assert_eq!(o.mode, PerfMode::Compact);
        o.push_frame(FrameSample::default());
        o.cycle();
        assert_eq!(o.mode, PerfMode::Expanded);
        assert_eq!(o.frames().count(), 1);
        o.cycle();
        assert_eq!(o.mode, PerfMode::Off);
        assert_eq!(o.frames().count(), 0);
    }

    #[test]
    fn summary_averages_about_the_last_second() {
        let mut o = PerfOverlay {
            mode: PerfMode::Compact,
            ..Default::default()
        };
        // An old slow second, then 100 frames of 10 ms.
        for _ in 0..10 {
            o.push_frame(FrameSample {
                frame_ms: 100.0,
                cpu_ms: 50.0,
                ..Default::default()
            });
        }
        for i in 0..100 {
            o.push_frame(FrameSample {
                frame_ms: 10.0,
                cpu_ms: 4.0,
                wait_ms: 5.0,
                gpu_ms: (i == 97).then_some(3.0),
            });
        }
        let s = o.summary();
        assert_eq!(s.fps, 100.0);
        assert_eq!(s.frame_ms, 10.0);
        assert_eq!(s.cpu_ms, 4.0);
        assert_eq!(s.wait_ms, 5.0);
        assert_eq!(s.worst_ms, 100.0);
        assert_eq!(s.gpu_ms, Some(3.0));
        assert_eq!(PerfOverlay::default().summary(), FrameSummary::default());
    }
}
