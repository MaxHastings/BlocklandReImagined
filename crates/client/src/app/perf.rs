//! Lag and frame performance sampling.
use super::*;

/// Performance: frame stats and log, network sampling, lag watch and quality.
pub(super) struct Perf {
    /// Connection samples for the net graph and the expanded overlay.
    pub(super) net_sampler: crate::perf::NetSampler,
    /// Whether a joined host has gone quiet, for the lag icon.
    pub(super) lag_watch: bri_net::lag::LagWatch,
    /// When the performance overlay's slower figures are next refreshed.
    pub(super) perf_stats_due: std::time::Instant,
    pub(super) frame_stats: crate::console::FrameStats,
    /// Minute-by-minute frame times for the session log (player sessions).
    pub(super) frame_log: Option<crate::quality::FrameLog>,
    /// Pick a graphics quality from the GPU if the player never has.
    pub(super) auto_quality: bool,
    /// The frame cap the platform was last given (startup, then saves), so
    /// a save that leaves it alone sends no window command.
    pub(super) frame_limit: Option<u32>,
}

impl App {
    /// v20's lag icon (`GameConnection::setLagIcon`): shown while a joined
    /// host has sent nothing for `$Pref::Net::LagThreshold` ms. Never for the
    /// game this process hosts, which v20 skips as a "local" connection.
    pub(super) fn update_lag(&mut self) {
        let joined = self
            .net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| Some((a.id, a.worker.probes.get()?)))
            .filter(|(_, p)| p.host.is_none());
        let Some((id, probes)) = joined else {
            if self.perf.lag_watch.lagging() {
                self.ui.apply(UiUpdate::Lagging(false));
            }
            self.perf.lag_watch.reset();
            return;
        };
        let default = bri_net::lag::DEFAULT_LAG_THRESHOLD.as_millis() as i64;
        let threshold = self
            .ui
            .core
            .prefs
            .i64_or("$Pref::Net::LagThreshold", default)
            .clamp(1, 60_000);
        self.perf
            .lag_watch
            .set_threshold(Duration::from_millis(threshold as u64));
        let received = probes.link.received();
        if let Some(lagging) = self
            .perf
            .lag_watch
            .observe(std::time::Instant::now(), received)
        {
            self.ui.apply_session(id, UiUpdate::Lagging(lagging));
        }
    }
    /// Ask this game's own host for its bots' readout, or stop asking.
    fn want_bot_why(&self, wanted: bool) {
        let host = self
            .net
            .attempt
            .as_ref()
            .and_then(|a| a.worker.probes.get())
            .and_then(|p| p.host.clone());
        if let Some(host) = host {
            let mut held = host.lock().unwrap_or_else(|e| e.into_inner());
            if held.bots_wanted != wanted {
                held.bots_wanted = wanted;
            }
        }
    }
    /// Feed the net graph and performance overlay while they show; nothing
    /// is sampled while both are hidden.
    pub(super) fn update_perf(&mut self) {
        let wants_net = self.ui.core.net_graph.is_some() || self.ui.core.perf.wants_net();
        let wants_stats = self.ui.core.perf.visible();
        if !wants_stats {
            self.want_bot_why(false);
        }
        if !wants_net && !wants_stats {
            self.perf.net_sampler.reset();
            return;
        }
        let now = std::time::Instant::now();
        let probes = self
            .net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.worker.probes.get())
            .cloned();
        let ghosts = self
            .network_view()
            .map_or(0, |v| v.poses.len() + v.vehicles.len() + v.entities.len());
        match probes.as_ref().filter(|_| wants_net) {
            Some(p) => {
                if let Some(sample) = self.perf.net_sampler.sample(now, &p.link, ghosts) {
                    self.ui.apply(UiUpdate::NetSample(sample));
                }
            }
            None => self.perf.net_sampler.reset(),
        }
        if !wants_stats || now < self.perf.perf_stats_due {
            return;
        }
        self.perf.perf_stats_due = now + Duration::from_millis(500);
        let memory = crate::perf::process_memory();
        let view = self.network_view();
        let mut bots = Vec::new();
        let server = probes.as_ref().and_then(|p| p.host.as_ref()).map(|host| {
            let mut held = host.lock().unwrap_or_else(|e| e.into_inner());
            // The host refreshes its bots' readout while this shows.
            held.bots_wanted = true;
            let p = held.clone();
            drop(held);
            bots = p.bots.clone();
            bri_ui::models::perf::ServerStats {
                ticks_per_second: p.ticks_per_second,
                tick_ms_mean: p.tick_ms_mean,
                tick_ms_max: p.tick_ms_max,
                script_ms: p.script_ms,
            }
        });
        // The bot the player looks at, read from the host's own readout.
        let bot = view.and_then(|v| {
            let me = v.poses.get(&v.owner)?.player.clone();
            let eye = glam::Vec3::from(me.feet) + glam::Vec3::Y * 2.0 * me.scale;
            let looked = bri_ui::models::perf::looked_at(
                eye.to_array(),
                me.forward().to_array(),
                bots.iter().filter_map(|(id, _)| {
                    let p = &v.poses.get(id)?.player;
                    Some((
                        *id,
                        (glam::Vec3::from(p.feet) + glam::Vec3::Y * 1.2 * p.scale).to_array(),
                    ))
                }),
            )?;
            let (_, why) = bots.iter().find(|(id, _)| *id == looked)?;
            let name = v.names.get(&looked).cloned().unwrap_or_default();
            Some((name, why.clone()))
        });
        let stats = bri_ui::models::perf::PerfStats {
            bricks: view.map(|v| v.world.bricks.len()),
            players: view.map(|v| v.names.len()),
            vehicles: view.map(|v| v.vehicles.len()),
            entities: view.map(|v| v.entities.len()),
            memory_bytes: memory.map(|m| m.0),
            private_bytes: memory.map(|m| m.1),
            remote_server: probes.as_ref().is_some_and(|p| p.host.is_none()),
            server,
            gpu: self.gpu.gpu_name.clone(),
            gpu_passes: self
                .gpu
                .gpu_passes
                .iter()
                .map(|(pass, ms)| ((*pass).to_string(), *ms))
                .collect(),
            bot,
        };
        self.ui.apply(UiUpdate::PerfStats(stats));
    }
}
