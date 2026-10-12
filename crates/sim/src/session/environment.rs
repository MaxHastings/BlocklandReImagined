//! The live environment (sun, light, fog, sky, day/night) over the map's
//! own: set whole by an admin's Environment window, or setting by setting
//! by Add-Ons (`set_environment`, the `environment` capability). The host
//! keeps and replicates the settings; each client resolves them over the
//! map it loaded (`bri_content::atmosphere::resolve`). Only the water has a
//! body: the host lays it over a flat map's plate for swimmers and lava
//! (`Simulation::apply_environment_water`). They last until the map
//! changes (a new session).
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
        if let Some(cycle) = &mut settings.day_cycle
            && self.environment.day_cycle != Some(*cycle)
        {
            cycle.anchor_tick = self.simulation.state().tick;
        }
        self.environment = settings;
        // The water is the one setting with a body: swimmers feel it.
        self.simulation.apply_environment_water(&self.environment);
        Ok(())
    }
}
