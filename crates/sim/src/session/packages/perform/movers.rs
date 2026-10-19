//! What the engine does for the `movers` operations
//! (`bri_package_runtime::ops::movers`).
use super::*;

impl Perform for ops::MoverGoto {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        // Refused when it is already heading elsewhere: the script reads
        // `movers()` first and tries again once the mover has arrived.
        session.mover_goto(&self.id, self.key, self.seconds)?;
        Ok(())
    }
}

impl Perform for ops::MoverStop {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        session.mover_stop(&self.id)
    }
}
