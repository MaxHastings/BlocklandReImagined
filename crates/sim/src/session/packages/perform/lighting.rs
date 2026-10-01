//! What the engine does for the `lighting` operations
//! (`bri_package_runtime::ops::lighting`).
use super::*;

impl Perform for ops::SetMapLights {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetMapLights {
            position,
            radius,
            tint,
        } = self;
        session.set_map_lights(MapLightRule {
            position,
            radius,
            tint,
        })
    }
}
