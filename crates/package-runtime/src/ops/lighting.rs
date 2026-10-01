//! Operations behind the `lighting` capability.
use super::*;

/// Every map light within `radius` of `position` shines at `tint` times
/// its recovered colour (0 switches it off, 1 is as the map was lit),
/// for every player, until the map changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetMapLights {
    pub position: [f32; 3],
    pub radius: f32,
    pub tint: [f32; 3],
}
impl ScriptOp for SetMapLights {
    const CAPABILITY: &str = "lighting";
    const NAME: &str = "set_map_lights";
    fn bounded(&self) -> bool {
        let SetMapLights {
            position,
            radius,
            tint,
        } = self;
        finite(position)
            && radius.is_finite()
            && (0.0..=MAX_LIGHT_RADIUS).contains(radius)
            && tint
                .iter()
                .all(|t| t.is_finite() && (0.0..=MAX_LIGHT_TINT).contains(t))
    }
}
