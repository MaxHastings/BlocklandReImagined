//! What the engine does for the `world_edit` operations
//! (`bri_package_runtime::ops::world_edit`).
use super::*;

impl Perform for ops::RemoveBrick {
    /// Whether the brick is there and the caller may remove it.
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.package_may_remove_brick(self.brick, cx.caller)?;
        session.package_check_edit_share(cx.package, "destroyed")
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::RemoveBrick { brick } = self;
        session.package_remove_brick(package, brick, None, caller)
    }
}
impl Perform for ops::PlaceBrick {
    /// Whether the package has an edit to spend and the brick fits.
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.package_check_place_brick(cx.package, &self.shape, self.position)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::PlaceBrick {
            shape,
            position,
            color,
        } = self;
        session.package_place_brick(package, &shape, position, color)
    }
}
impl Perform for ops::PlantBrick {
    /// Whether the brick would really be planted. Without `require` a brick
    /// that does not fit is skipped without a report, as v20's `plant()`
    /// was; a script that charges for it asks to hear.
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.package_check_plant_brick(
            cx.package,
            &self.kind,
            self.position,
            self.turns,
            self.color,
            self.owner,
            cx.caller,
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::PlantBrick {
            kind,
            position,
            turns,
            color,
            owner,
        } = self;
        session.package_plant_brick(package, &kind, position, turns, color, owner, caller)
    }
}
impl Perform for ops::PlaceVoxel {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.package_check_place_voxel(cx.package, self.position, &self.material)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::PlaceVoxel { position, material } = self;
        session.package_place_voxel(package, position, &material)
    }
}
impl Perform for ops::SetBlockState {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetBlockState { brick, state } = self;
        let host = session
            .packages
            .as_ref()
            .context("No packages are enabled")?;
        let look = session
            .simulation
            .state()
            .bricks
            .get(&brick)
            .context("No such brick")?
            .look
            .as_ref()
            .context("That brick shows no block")?;
        let block = host
            .catalog
            .block(&look.block)
            .with_context(|| format!("No block {}", look.block))?;
        ensure!(
            state.is_empty() || block.states.contains_key(&state),
            "Block {} has no state `{state}`",
            look.block
        );
        if look.state != state {
            session.simulation.mutate(brick, |b| {
                if let Some(look) = &mut b.look {
                    look.state = state;
                }
            })?;
            session.dirty.insert(brick);
        }
        Ok(())
    }
}
impl Perform for ops::CutCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy's bricks are cut only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::CutCopy { player, each } = self;
        // Bricks go with the trust of the player who asked.
        asked_by(
            cx,
            player,
            "A copy's bricks are cut only for the player whose command asked",
        )?;
        session.start_cut(player, package, each);
        Ok(())
    }
}
impl Perform for ops::PaintCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy's bricks are painted only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::PaintCopy {
            player,
            paint,
            each,
        } = self;
        asked_by(
            cx,
            player,
            "A copy's bricks are painted only for the player whose command asked",
        )?;
        session.start_paint(player, package, paint, each);
        Ok(())
    }
}
impl Perform for ops::WrenchCopy {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "A copy's bricks are wrenched only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::WrenchCopy { player } = self;
        asked_by(
            cx,
            player,
            "A copy's bricks are wrenched only for the player whose command asked",
        )?;
        if let Err(error) = session.open_copy_wrench(player) {
            let outcome = crate::session::copy_store::CopyOutcome::failed(
                "wrench",
                session.blueprints.contains_key(&player),
                error,
            );
            session.report_copy(package, player, outcome);
        }
        Ok(())
    }
}
impl Perform for ops::SuperCut {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "Bricks are cut only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SuperCut { player, min, max } = self;
        asked_by(
            cx,
            player,
            "Bricks are cut only for the player whose command asked",
        )?;
        session.start_super_cut(player, package, (min, max));
        Ok(())
    }
}
impl Perform for ops::FillBox {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "Bricks are filled only for the player whose command asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::FillBox {
            player,
            min,
            max,
            color,
        } = self;
        asked_by(
            cx,
            player,
            "Bricks are filled only for the player whose command asked",
        )?;
        session.start_fill(player, package, (min, max), color);
        Ok(())
    }
}
impl Perform for ops::PaintFill {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "Bricks are filled only for the player whose command or shot asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::PaintFill {
            player,
            brick,
            paint,
            limit,
            reach,
            stop_at_limit,
            limit_message,
            refusal_seconds,
            limit_error,
        } = self;
        asked_by(
            cx,
            player,
            "Bricks are filled only for the player whose command or shot asked",
        )?;
        let rules = super::FillRules {
            limit: limit as usize,
            reach,
            stop_at_limit,
        };
        match session.paint_fill(player, brick, paint, rules) {
            Ok(fill) => {
                if fill.stopped && limit_error {
                    session.notify(
                        player,
                        Notice::PlantError(crate::simulation::PlantFailure::Limit),
                    );
                }
                if let (true, Some((text, seconds))) = (fill.stopped, limit_message) {
                    session.notify(player, Notice::Center { text, seconds });
                }
            }
            Err(error) => session.notify(
                player,
                Notice::Center {
                    text: format!("{error:#}"),
                    seconds: refusal_seconds.unwrap_or(1.0),
                },
            ),
        }
        Ok(())
    }
}
impl Perform for ops::PaintVehicle {
    fn check(&self, _session: &Session, cx: OpCall<'_>) -> Result<()> {
        asked_by(
            cx,
            self.player,
            "Vehicles are painted only for the player whose command or shot asked",
        )
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let ops::PaintVehicle {
            player,
            vehicle,
            paint,
            riders_seconds,
            refusal_seconds,
        } = self;
        asked_by(
            cx,
            player,
            "Vehicles are painted only for the player whose command or shot asked",
        )?;
        match session.paint_vehicle(player, vehicle, paint) {
            Ok(color) => {
                let seconds = riders_seconds.unwrap_or(0.0);
                for rider in session.vehicle_riders(vehicle).filter(|_| seconds > 0.0) {
                    let look = bri_package_runtime::ops::TempLook {
                        color: Some(color),
                        ..Default::default()
                    };
                    session.temp_look(rider, look, seconds);
                }
            }
            Err(error) => session.notify(
                player,
                Notice::Center {
                    text: format!("{error:#}"),
                    seconds: refusal_seconds.unwrap_or(1.0),
                },
            ),
        }
        Ok(())
    }
}
impl Perform for ops::SetBrickItem {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::SetBrickItem { brick, item } = self;
        if let Some(item) = &item {
            let host = session
                .packages
                .as_ref()
                .context("No packages are enabled")?;
            // The base game's items (`v20.weapon.gunitem`) are every
            // package's to hand out, as v20's `setItem` took any datablock.
            let base_game = item.split(['.', ':']).next() == Some(bri_package::id::BASE_NAMESPACE);
            ensure!(
                base_game || item_hooks::owns(&host.catalog, package, item),
                "`{item}` is not an item of `{package}` or an Add-On it depends on"
            );
        }
        session.package_set_brick_item(brick, item, caller)
    }
}
impl Perform for ops::SetBrickColor {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::SetBrickColor { brick, color } = self;
        session.package_set_brick_color(brick, color, caller)
    }
}
impl Perform for ops::SetBrickShown {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::SetBrickShown {
            brick,
            rendering,
            colliding,
            raycasting,
        } = self;
        session.package_set_brick_shown(package, brick, [rendering, colliding, raycasting], caller)
    }
}
