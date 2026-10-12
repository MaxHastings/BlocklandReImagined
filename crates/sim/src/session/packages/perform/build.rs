//! What the engine does for the `build` operations
//! (`bri_package_runtime::ops::build`).
use super::*;

impl Perform for ops::CopyBuild {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A build is copied only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
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
        asked_by(
            cx,
            player,
            "A build is copied only for the player whose command asked",
        )?;
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
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A build is copied only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
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
        asked_by(
            cx,
            player,
            "A build is copied only for the player whose command asked",
        )?;
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
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy is saved only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SaveCopy {
            player,
            name,
            overwrite,
        } = self;
        asked_by(
            cx,
            player,
            "A copy is saved only for the player whose command asked",
        )?;
        session.save_copy(player, name, overwrite, package);
        Ok(())
    }
}
impl Perform for ops::ListCopies {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "Saved copies are listed only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::ListCopies { player, filter } = self;
        asked_by(
            cx,
            player,
            "Saved copies are listed only for the player whose command asked",
        )?;
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
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by_or_admin(
            session,
            cx,
            self.player,
            "Copy work is cancelled only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::CancelCopy { player } = self;
        // An administrator may stop anyone's (`/ClearDups`).
        asked_by_or_admin(
            session,
            cx,
            player,
            "Copy work is cancelled only for the player whose command asked",
        )?;
        session.cancel_copy(player);
        Ok(())
    }
}
impl Perform for ops::PivotCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy's pivot is set only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::PivotCopy { player, whole } = self;
        asked_by(
            cx,
            player,
            "A copy's pivot is set only for the player whose command asked",
        )?;
        session.pivot_copy(player, whole)
    }
}
impl Perform for ops::PlantAs {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "Copies are planted as another only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::PlantAs {
            player,
            target,
            admin,
        } = self;
        asked_by(
            cx,
            player,
            "Copies are planted as another only for the player whose command asked",
        )?;
        let outcome = session.plant_as(player, &target, admin);
        session.report_copy(package, player, outcome);
        Ok(())
    }
}
impl Perform for ops::LoadCopy {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy is loaded only for the player whose command asked",
        )?;
        ensure!(
            session.weapons.contains_item(&self.tool),
            "The copy's tool {} is not an item on this server",
            self.tool
        );
        Ok(())
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::LoadCopy {
            player,
            name,
            limit,
            tool,
            partial,
            whole,
        } = self;
        asked_by(
            cx,
            player,
            "A copy is loaded only for the player whose command asked",
        )?;
        ensure!(
            session.weapons.contains_item(&tool),
            "The copy's tool {tool} is not an item on this server"
        );
        session.load_copy(player, name, limit as usize, tool, partial, whole, package);
        Ok(())
    }
}
impl Perform for ops::HighlightCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy is lit only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::HighlightCopy {
            player,
            color,
            seconds,
        } = self;
        asked_by(
            cx,
            player,
            "A copy is lit only for the player whose command asked",
        )?;
        // No copy to light is not the Add-On's fault.
        let _ = session.highlight_copy(player, color, seconds);
        Ok(())
    }
}
impl Perform for ops::MirrorCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy is mirrored only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::MirrorCopy { player, axis } = self;
        asked_by(
            cx,
            player,
            "A copy is mirrored only for the player whose command asked",
        )?;
        if let Err(error) = session.mirror_copy(player, axis) {
            session.center_print(player, format!("{error:#}"));
        }
        Ok(())
    }
}
impl Perform for ops::MirrorGhost {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A ghost brick is mirrored only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::MirrorGhost {
            player,
            axis,
            asymmetric,
        } = self;
        asked_by(
            cx,
            player,
            "A ghost brick is mirrored only for the player whose command asked",
        )?;
        if let Err(error) = session.mirror_ghost(player, axis, &asymmetric) {
            session.center_print(player, format!("{error:#}"));
        }
        Ok(())
    }
}
impl Perform for ops::MoveCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy is moved only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::MoveCopy {
            player,
            point,
            normal,
        } = self;
        asked_by(
            cx,
            player,
            "A copy is moved only for the player whose command asked",
        )?;
        if let Err(error) = session.move_copy(player, point, normal) {
            session.center_print(player, format!("{error:#}"));
        }
        Ok(())
    }
}
impl Perform for ops::DropCopy {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by_or_admin(
            session,
            cx,
            self.player,
            "A copy is put away only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::DropCopy { player } = self;
        // An administrator may put away anyone's (`/ClearDups`).
        asked_by_or_admin(
            session,
            cx,
            player,
            "A copy is put away only for the player whose command asked",
        )?;
        session.drop_copy(player);
        Ok(())
    }
}
impl Perform for ops::ShowCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy is shown only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::ShowCopy { player } = self;
        asked_by(
            cx,
            player,
            "A copy is shown only for the player whose command asked",
        )?;
        if let Err(error) = session.show_copy(player) {
            session.center_print(player, format!("{error:#}"));
        }
        Ok(())
    }
}
impl Perform for ops::HideCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy is hidden only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::HideCopy { player } = self;
        asked_by(
            cx,
            player,
            "A copy is hidden only for the player whose command asked",
        )?;
        session.hide_copy(player);
        Ok(())
    }
}
impl Perform for ops::ShiftCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy is moved only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::ShiftCopy {
            player,
            offset,
            super_shift,
        } = self;
        asked_by(
            cx,
            player,
            "A copy is moved only for the player whose command asked",
        )?;
        let _ = session.shift_copy(player, offset, super_shift);
        Ok(())
    }
}
impl Perform for ops::RotateCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy is turned only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::RotateCopy { player, direction } = self;
        asked_by(
            cx,
            player,
            "A copy is turned only for the player whose command asked",
        )?;
        let _ = session.rotate_copy(player, direction);
        Ok(())
    }
}
impl Perform for ops::PlantCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy is planted only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::PlantCopy { player, float } = self;
        asked_by(
            cx,
            player,
            "A copy is planted only for the player whose command asked",
        )?;
        let _ = session.plant_copy(player, float);
        Ok(())
    }
}
impl Perform for ops::FloatCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy floats only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::FloatCopy {
            player,
            float,
            admin_only,
        } = self;
        asked_by(
            cx,
            player,
            "A copy floats only for the player whose command asked",
        )?;
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
