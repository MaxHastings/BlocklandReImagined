//! Lag and frame performance sampling.
use super::*;

impl App {
    /// v20's lag icon (`GameConnection::setLagIcon`): shown while a joined
    /// host has sent nothing for `$Pref::Net::LagThreshold` ms. Never for the
    /// game this process hosts, which v20 skips as a "local" connection.
    pub(super) fn update_lag(&mut self) {
        let joined = self
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| Some((a.id, a.worker.probes.get()?)))
            .filter(|(_, p)| p.host.is_none());
        let Some((id, probes)) = joined else {
            if self.lag_watch.lagging() {
                self.ui.apply(UiUpdate::Lagging(false));
            }
            self.lag_watch.reset();
            return;
        };
        let default = bri_net::lag::DEFAULT_LAG_THRESHOLD.as_millis() as i64;
        let threshold = self
            .ui
            .core
            .prefs
            .i64_or("$Pref::Net::LagThreshold", default)
            .clamp(1, 60_000);
        self.lag_watch.set_threshold(Duration::from_millis(threshold as u64));
        let received = probes.link.received();
        if let Some(lagging) = self.lag_watch.observe(std::time::Instant::now(), received) {
            self.ui.apply_session(id, UiUpdate::Lagging(lagging));
        }
    }
    /// Feed the net graph and performance overlay while they show; nothing
    /// is sampled while both are hidden.
    pub(super) fn update_perf(&mut self) {
        let wants_net = self.ui.core.net_graph.is_some() || self.ui.core.perf.wants_net();
        let wants_stats = self.ui.core.perf.visible();
        if !wants_net && !wants_stats {
            self.net_sampler.reset();
            return;
        }
        let now = std::time::Instant::now();
        let probes = self
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
                if let Some(sample) = self.net_sampler.sample(now, &p.link, ghosts) {
                    self.ui.apply(UiUpdate::NetSample(sample));
                }
            }
            None => self.net_sampler.reset(),
        }
        if !wants_stats || now < self.perf_stats_due {
            return;
        }
        self.perf_stats_due = now + Duration::from_millis(500);
        let memory = crate::perf::process_memory();
        let view = self.network_view();
        let server = probes.as_ref().and_then(|p| p.host.as_ref()).map(|host| {
            let p = host.lock().unwrap_or_else(|e| e.into_inner()).clone();
            bri_ui::models::perf::ServerStats {
                ticks_per_second: p.ticks_per_second,
                tick_ms_mean: p.tick_ms_mean,
                tick_ms_max: p.tick_ms_max,
                script_ms: p.script_ms,
            }
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
            gpu: self.gpu_name.clone(),
            gpu_passes: self
                .gpu_passes
                .iter()
                .map(|(pass, ms)| ((*pass).to_string(), *ms))
                .collect(),
        };
        self.ui.apply(UiUpdate::PerfStats(stats));
    }
}
