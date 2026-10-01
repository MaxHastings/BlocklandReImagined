//! What the engine does for the `physics` operations
//! (`bri_package_runtime::ops::physics`).
use super::*;

impl Perform for ops::MountObject {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::MountObject {
            mount,
            rider,
            node,
            can_dismount,
            turn,
        } = self;
        ensure!(
            caller.is_none_or(|c| c == mount),
            "A player mounts others on themselves only by their own command"
        );
        // Carrying someone moves them: the same rules as `hold`.
        ensure!(
            session.may_move(mount, ObjectRef::Player(rider)),
            "Player {mount} may not move {rider} under the minigame and trust rules"
        );
        session.mount_player(mount, rider, node, can_dismount)?;
        session.turn_rider(rider, turn);
        Ok(())
    }
}
impl Perform for ops::UnmountObject {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::UnmountObject { rider } = self;
        // A command's player lets themselves off, or someone they
        // carry or may move.
        ensure!(
            caller.is_none_or(|c| c == rider
                || session
                    .riding_seat(rider)
                    .is_some_and(|(mount, _)| mount == c)
                || session.may_move(c, ObjectRef::Player(rider))),
            "Player {} may not take {rider} off their mount",
            caller.unwrap_or_default()
        );
        session.unmount_object(rider)
    }
}
