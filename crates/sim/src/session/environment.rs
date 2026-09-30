//! The live environment (sun, light, fog, sky, day/night) over the map's
//! own: set whole by an admin's Environment window, or setting by setting
//! by Add-Ons (`set_environment`, the `environment` capability). The host
//! only keeps and replicates the settings; each client resolves them over
//! the map it loaded (`bri_content::atmosphere::resolve`), so the host
//! needs no scene data. They last until the map changes (a new session).
use super::*;
use bri_content::atmosphere::Settings;

impl Session {
    /// The environment settings now; unset ones keep the map's own.
    pub fn environment(&self) -> Settings {
        self.environment.clone()
    }
    /// Replace the settings. A day/night cycle that changed is anchored at
    /// this tick, so its time of day is the one just set; an unchanged one
    /// keeps running.
    pub(super) fn set_environment(&mut self, mut settings: Settings) -> Result<()> {
        settings.validate()?;
        if let Some(cycle) = &mut settings.day_cycle {
            if self.environment.day_cycle != Some(*cycle) {
                cycle.anchor_tick = self.simulation.state().tick;
            }
        }
        self.environment = settings;
        Ok(())
    }
}
