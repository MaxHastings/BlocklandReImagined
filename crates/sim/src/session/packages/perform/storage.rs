//! What the engine does for the `storage` operations
//! (`bri_package_runtime::ops::storage`).
use super::*;

impl Perform for ops::SetHostData {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SetHostData { key, value } = self;
        session.set_host_data(package, &key, value)
    }
}
