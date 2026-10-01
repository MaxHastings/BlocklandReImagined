//! Map lights changed at run time by Add-Ons (`set_map_lights`, the
//! `lighting` capability): every recovered map light within a sphere
//! takes a colour and brightness (0 switches it off). The host keeps the
//! rules and replicates them; each client applies them to the lights it
//! recovered from the map's baked lighting, so the host needs no lighting
//! data. The rules last until the map changes (a new session).
use super::*;
use bri_package_runtime::ops::{MAX_LIGHT_RADIUS, MAX_LIGHT_TINT};

/// Most rules a server keeps: a rule replaces an earlier one over the same
/// sphere, so this bounds distinct places, not calls.
pub const MAX_MAP_LIGHT_RULES: usize = 256;

/// Lights within `radius` of `position` shine at `tint` times their
/// recovered colour; later rules win where spheres overlap.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MapLightRule {
    pub position: [f32; 3],
    pub radius: f32,
    pub tint: [f32; 3],
}
impl MapLightRule {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.position.iter().all(|x| x.is_finite() && x.abs() <= 1_000_000.0)
                && self.radius.is_finite()
                && (0.0..=MAX_LIGHT_RADIUS).contains(&self.radius)
                && self.tint.iter().all(|t| t.is_finite() && (0.0..=MAX_LIGHT_TINT).contains(t)),
            "Invalid map light rule"
        );
        Ok(())
    }
    /// The tint light `at` takes from `rules` (1 when none covers it).
    pub fn tint_at(rules: &[Self], at: Vec3) -> Vec3 {
        rules
            .iter()
            .rev()
            .find(|r| Vec3::from(r.position).distance(at) <= r.radius)
            .map_or(Vec3::ONE, |r| Vec3::from(r.tint))
    }
}

impl Session {
    /// The Add-On map light rules now, oldest first.
    pub fn map_light_rules(&self) -> Vec<MapLightRule> {
        self.map_lights.clone()
    }
    pub(super) fn set_map_lights(&mut self, rule: MapLightRule) -> Result<()> {
        rule.validate()?;
        self.map_lights
            .retain(|r| r.position != rule.position || r.radius != rule.radius);
        ensure!(
            self.map_lights.len() < MAX_MAP_LIGHT_RULES,
            "Too many map light changes: reuse a place and radius, or cover several lights with one sphere"
        );
        self.map_lights.push(rule);
        Ok(())
    }
}
