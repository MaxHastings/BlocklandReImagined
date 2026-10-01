//! What the engine does for the `build` operations
//! (`bri_package_runtime::ops::build`).
use super::*;

impl Perform for ops::CopyBuild {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::CopyBuild {
            player,
            brick,
            limit,
            reach,
            rule,
            tool,
            hold,
        } = self;
        // A copy is taken with the trust of the player who asked.
        ensure!(
            caller == Some(player),
            "A build is copied only for the player whose command asked"
        );
        // The player or the Add-On hears why a copy failed;
        // nothing went wrong with the Add-On.
        session.start_select(
            player,
            package,
            super::blueprints::SelectWhat::Stack { brick, reach },
            (limit as usize, rule, &tool, hold),
        );
        Ok(())
    }
}
impl Perform for ops::CopyBox {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::CopyBox {
            player,
            min,
            max,
            limited,
            limit,
            rule,
            tool,
            hold,
        } = self;
        ensure!(
            caller == Some(player),
            "A build is copied only for the player whose command asked"
        );
        session.start_select(
            player,
            package,
            super::blueprints::SelectWhat::Box {
                area: (min, max),
                limited,
            },
            (limit as usize, rule, &tool, hold),
        );
        Ok(())
    }
}
impl Perform for ops::SaveCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::SaveCopy {
            player,
            name,
            overwrite,
        } = self;
        ensure!(
            caller == Some(player),
            "A copy is saved only for the player whose command asked"
        );
        session.save_copy(player, name, overwrite, package);
        Ok(())
    }
}
impl Perform for ops::ListCopies {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::ListCopies { player, filter } = self;
        ensure!(
            caller == Some(player),
            "Saved copies are listed only for the player whose command asked"
        );
        session.list_copies(player, filter, package);
        Ok(())
    }
}
impl Perform for ops::PlantWait {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::PlantWait { player, seconds } = self;
        session.plant_wait(player, seconds)
    }
}
impl Perform for ops::CancelCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::CancelCopy { player } = self;
        // An administrator may stop anyone's (`/ClearDups`).
        let admin = caller
            .and_then(|c| session.peers.get(&c))
            .is_some_and(|p| p.actor.administrator);
        ensure!(
            caller == Some(player) || admin,
            "Copy work is cancelled only for the player whose command asked"
        );
        session.cancel_copy(player);
        Ok(())
    }
}
impl Perform for ops::PivotCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::PivotCopy { player, whole } = self;
        ensure!(
            caller == Some(player),
            "A copy's pivot is set only for the player whose command asked"
        );
        session.pivot_copy(player, whole)
    }
}
impl Perform for ops::PlantAs {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::PlantAs {
            player,
            target,
            admin,
        } = self;
        ensure!(
            caller == Some(player),
            "Copies are planted as another only for the player whose command asked"
        );
        let outcome = session.plant_as(player, &target, admin);
        session.report_copy(package, player, outcome);
        Ok(())
    }
}
impl Perform for ops::LoadCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::LoadCopy {
            player,
            name,
            limit,
            tool,
            partial,
            whole,
        } = self;
        ensure!(
            caller == Some(player),
            "A copy is loaded only for the player whose command asked"
        );
        ensure!(
            session.weapons.contains_item(&tool),
            "The copy's tool {tool} is not an item on this server"
        );
        session.load_copy(player, name, limit as usize, tool, partial, whole, package);
        Ok(())
    }
}
impl Perform for ops::HighlightCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::HighlightCopy {
            player,
            color,
            seconds,
        } = self;
        ensure!(
            caller == Some(player),
            "A copy is lit only for the player whose command asked"
        );
        // No copy to light is not the Add-On's fault.
        let _ = session.highlight_copy(player, color, seconds);
        Ok(())
    }
}
impl Perform for ops::MirrorCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::MirrorCopy { player, axis } = self;
        ensure!(
            caller == Some(player),
            "A copy is mirrored only for the player whose command asked"
        );
        if let Err(error) = session.mirror_copy(player, axis) {
            session.center_print(player, format!("{error:#}"));
        }
        Ok(())
    }
}
impl Perform for ops::MirrorGhost {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::MirrorGhost {
            player,
            axis,
            asymmetric,
        } = self;
        ensure!(
            caller == Some(player),
            "A ghost brick is mirrored only for the player whose command asked"
        );
        if let Err(error) = session.mirror_ghost(player, axis, &asymmetric) {
            session.center_print(player, format!("{error:#}"));
        }
        Ok(())
    }
}
impl Perform for ops::MoveCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::MoveCopy {
            player,
            point,
            normal,
        } = self;
        ensure!(
            caller == Some(player),
            "A copy is moved only for the player whose command asked"
        );
        if let Err(error) = session.move_copy(player, point, normal) {
            session.center_print(player, format!("{error:#}"));
        }
        Ok(())
    }
}
impl Perform for ops::DropCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::DropCopy { player } = self;
        // An administrator may put away anyone's (`/ClearDups`).
        let admin = caller
            .and_then(|c| session.peers.get(&c))
            .is_some_and(|p| p.actor.administrator);
        ensure!(
            caller == Some(player) || admin,
            "A copy is put away only for the player whose command asked"
        );
        session.drop_copy(player);
        Ok(())
    }
}
impl Perform for ops::ShowCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::ShowCopy { player } = self;
        ensure!(
            caller == Some(player),
            "A copy is shown only for the player whose command asked"
        );
        if let Err(error) = session.show_copy(player) {
            session.center_print(player, format!("{error:#}"));
        }
        Ok(())
    }
}
impl Perform for ops::HideCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::HideCopy { player } = self;
        ensure!(
            caller == Some(player),
            "A copy is hidden only for the player whose command asked"
        );
        session.hide_copy(player);
        Ok(())
    }
}
impl Perform for ops::ShiftCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::ShiftCopy {
            player,
            offset,
            super_shift,
        } = self;
        ensure!(
            caller == Some(player),
            "A copy is moved only for the player whose command asked"
        );
        let _ = session.shift_copy(player, offset, super_shift);
        Ok(())
    }
}
impl Perform for ops::RotateCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::RotateCopy { player, direction } = self;
        ensure!(
            caller == Some(player),
            "A copy is turned only for the player whose command asked"
        );
        let _ = session.rotate_copy(player, direction);
        Ok(())
    }
}
impl Perform for ops::PlantCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::PlantCopy { player, float } = self;
        ensure!(
            caller == Some(player),
            "A copy is planted only for the player whose command asked"
        );
        let _ = session.plant_copy(player, float);
        Ok(())
    }
}
impl Perform for ops::FloatCopy {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::FloatCopy {
            player,
            float,
            admin_only,
        } = self;
        ensure!(
            caller == Some(player),
            "A copy floats only for the player whose command asked"
        );
        let _ = session.float_copy(player, float, admin_only);
        Ok(())
    }
}
impl Perform for ops::TakePaint {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::TakePaint { player, take } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.notify(player, Notice::TakePaint(take));
        Ok(())
    }
}
