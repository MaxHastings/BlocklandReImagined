//! Optional package-owned response for local brick presentation, not gameplay.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub profile: String,
    /// World units, using the ordinary mixer and sound-carry system.
    pub radius: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub id: String,
    pub snap: Option<Response>,
    pub impact: Option<Response>,
    pub landing: Option<Response>,
    /// Contact impulse divided by effective mass, in units/second.
    pub min_speed: f32,
    pub full_speed: f32,
    pub cooldown: f32,
    /// Smaller sections use the single-contact response on landing.
    pub landing_min_members: usize,
    pub pitch: [f32; 2],
}
impl Policy {
    pub fn responses(&self) -> impl Iterator<Item = &Response> {
        self.snap.iter().chain(&self.impact).chain(&self.landing)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            crate::add_on_of(&self.id).is_some()
                && self.id.len() <= 256
                && !self.id.chars().any(char::is_control)
                && self.responses().all(|r| {
                    !r.profile.is_empty()
                        && r.profile.len() <= 256
                        && !r.profile.chars().any(char::is_control)
                        && r.radius.is_finite()
                        && (1.0..=100.0).contains(&r.radius)
                })
                && self.min_speed.is_finite()
                && (0.5..=20.0).contains(&self.min_speed)
                && self.full_speed.is_finite()
                && self.full_speed >= self.min_speed
                && self.full_speed <= 100.0
                && self.cooldown.is_finite()
                && (0.08..=2.0).contains(&self.cooldown)
                && self
                    .pitch
                    .iter()
                    .all(|p| p.is_finite() && (0.8..=1.25).contains(p))
                && self.pitch[0] <= self.pitch[1]
                && (2..=1024).contains(&self.landing_min_members),
            "Invalid brick_debris_audio: bounded profiles/radii, positive speed/cooldown and pitch range"
        );
        Ok(())
    }
}
