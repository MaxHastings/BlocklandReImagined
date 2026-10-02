//! Console commands the client app runs (`UiAction::Console`): diagnostics
//! that need the renderer, network or content state rather than the UI.
use crate::app::App;
use bri_console::{CommandInfo, Output, Registry, Store};
use std::collections::VecDeque;
use std::time::Duration;

/// Recent frame times for `stats`.
#[derive(Debug, Default)]
pub struct FrameStats {
    frames: VecDeque<Duration>,
}

impl FrameStats {
    const WINDOW: usize = 240;
    pub fn push(&mut self, elapsed: Duration) {
        if self.frames.len() == Self::WINDOW {
            self.frames.pop_front();
        }
        self.frames.push_back(elapsed);
    }
    /// (frames per second, average ms, worst ms) over the window.
    pub fn summary(&self) -> Option<(f64, f64, f64)> {
        let total: Duration = self.frames.iter().sum();
        let worst = self.frames.iter().max()?;
        let avg = total.as_secs_f64() / self.frames.len() as f64;
        (avg > 0.0).then(|| (1.0 / avg, avg * 1000.0, worst.as_secs_f64() * 1000.0))
    }
}

/// App commands share the UI's prefs.
impl Store for App {
    fn get(&self, key: &str) -> Option<String> {
        self.ui.core.get(key)
    }
    fn set(&mut self, key: &str, value: &str) {
        self.ui.core.set(key, value);
    }
    fn keys(&self) -> Vec<String> {
        self.ui.core.keys()
    }
}

pub fn registry() -> Registry<App> {
    let mut r: Registry<App> = Registry::new();
    r.command(
        "stats",
        "",
        "Frame rate, network and world statistics.",
        |app, _, out| {
            stats(app, out);
            Ok(())
        },
    );
    r.command(
        "version",
        "",
        "Build and content package identity.",
        |app, _, out| {
            version(app, out);
            Ok(())
        },
    );
    r
}

/// What the UI lists and forwards to [`registry`].
pub fn commands() -> Vec<CommandInfo> {
    registry()
        .commands()
        .filter(|c| !matches!(c.name.as_str(), "help" | "cvars"))
        .cloned()
        .collect()
}

fn stats(app: &App, out: &mut Output) {
    match app.frame_stats().summary() {
        Some((fps, avg, worst)) => out.echo(format!(
            "Frame: {fps:.0} fps, {avg:.1} ms average, {worst:.1} ms worst (last 240 frames)"
        )),
        None => out.echo("Frame: no frames yet"),
    }
    match app.network_view() {
        Some(view) => {
            out.echo(format!(
                "Network: {} ms ping, {} player(s), tick {}",
                view.rtt_ms,
                view.names.len(),
                view.tick
            ));
            out.echo(format!(
                "World: {} ({}), {} brick(s), {} vehicle(s)",
                view.world.name,
                view.world.map_id,
                view.world.bricks.len(),
                view.vehicles.len()
            ));
        }
        None => out.echo("Network: not in a game"),
    }
    let audio = app.audio_stats();
    out.echo(format!(
        "Audio: {} real, {} virtual voice(s)",
        audio.real_voices, audio.virtual_voices
    ));
    let (sources, particles) = {
        let (_, sources, particles, _) = app.effect_counts();
        (sources, particles)
    };
    out.echo(format!(
        "Effects: {sources} source(s), {particles} particle(s)"
    ));
}

fn version(app: &App, out: &mut Output) {
    out.echo(format!(
        "Blockland ReImagined {} ({})",
        crate::updates::version(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    ));
    out.echo(format!("Protocol {}", bri_net::protocol::VERSION));
    let p = &app.content.paths;
    let packages = [
        &p.map_bundle,
        &p.brick_catalog,
        &p.geometry,
        &p.worlds,
        &p.ui_pack,
        &p.audio,
        &p.vehicles,
        &p.weapons,
        &p.events,
    ];
    let names: Vec<String> = packages
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    out.echo(format!("Content: {}", names.join(", ")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_stats_summarise_the_window() {
        let mut s = FrameStats::default();
        assert_eq!(s.summary(), None);
        for ms in [10, 10, 40] {
            s.push(Duration::from_millis(ms));
        }
        let (fps, avg, worst) = s.summary().unwrap();
        assert!(
            (avg - 20.0).abs() < 1e-9 && (fps - 50.0).abs() < 1e-9 && (worst - 40.0).abs() < 1e-9
        );
        for _ in 0..FrameStats::WINDOW {
            s.push(Duration::from_millis(5));
        }
        assert!((s.summary().unwrap().2 - 5.0).abs() < 1e-9);
    }

    #[test]
    fn host_commands_exclude_the_ui_builtins() {
        let names: Vec<_> = commands().into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["stats", "version"]);
    }
}
