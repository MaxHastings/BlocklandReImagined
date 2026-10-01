//! Operations behind the `environment` capability.
use super::*;

/// Change the live environment (sun, light, fog, sky, day/night) for
/// every player until the map changes: `changes` sets what it sets,
/// then each of `unset` (names from `bri_content::atmosphere::KEYS`)
/// goes back to the map's own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetEnvironment {
    pub changes: Box<bri_content::atmosphere::Settings>,
    pub unset: Vec<String>,
}
impl ScriptOp for SetEnvironment {
    const CAPABILITY: &str = "environment";
    const NAME: &str = "set_environment";
    fn bounded(&self) -> bool {
        let SetEnvironment { changes, unset } = self;
        changes.validate().is_ok()
            && unset.len() <= bri_content::atmosphere::KEYS.len()
            && unset
                .iter()
                .all(|k| bri_content::atmosphere::KEYS.contains(&k.as_str()))
    }
}
