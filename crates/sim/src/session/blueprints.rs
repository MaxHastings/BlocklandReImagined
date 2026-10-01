//! Copies of builds held for players, and placing them (see
//! [`crate::blueprint`]). An Add-On decides what to copy and for whom
//! (`copy_build`, `copy_box`); placing is the player's own command and
//! follows the plant rules: minigame building, reach, trust, overlap,
//! support, the brick limit, and the plant rate, which a copy uses up as
//! one plant window of its own.
//!
//! A copy remembers the bricks it was taken from, so an Add-On may cut
//! them away (`cut_copy`) or paint them (`paint_copy`) with the player's
//! own trust, each as one step of their undo.
use super::*;
use crate::blueprint::{Blueprint, MAX_BLUEPRINT_BRICKS, Outline, snap_anchor};
use bri_package_runtime::ops::{CopyRule, CopyTrust, MirrorAxis, StackReach};
use bri_world::authority::trust as level;

/// A copy a player holds: the bricks it was taken from (`cut_copy`,
/// `paint_copy`, `highlight_copy`), the Add-On that took it and how it
/// plants.
pub(super) struct HeldCopy {
    pub sources: Vec<BrickId>,
    pub package: String,
    pub partial: bool,
}

/// What a copy took, or why it took nothing: handed to the Add-On's
/// `on_copy` hook, else told to the player.
pub struct Copied {
    pub selection: crate::simulation::Selection,
    /// `trust`, `public`, `empty` or `invalid`.
    pub error: Option<(&'static str, String)>,
}

/// Whether `rule` lets `actor` copy `brick`.
fn admits(actor: &Actor, rule: CopyRule, brick: &Brick) -> bool {
    if brick.owner == 0 {
        return rule.public;
    }
    if actor.administrator && rule.admin {
        return true;
    }
    let needed = match rule.trust {
        CopyTrust::Build => level::BUILD,
        CopyTrust::Full => level::FULL,
    };
    actor.trust_level(brick.owner) >= needed
}

impl Session {
    /// Copy the stack at `brick` for `owner` as `rule` allows
    /// ([`crate::simulation::Simulation::select_stack`]), replacing any
    /// copy they hold, and send it to them to place with `tool`.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_build(
        &mut self,
        owner: OwnerId,
        brick: BrickId,
        limit: usize,
        reach: StackReach,
        rule: CopyRule,
        tool: &str,
        package: &str,
    ) -> Copied {
        let selected = (|| {
            self.check_copy(limit, tool)?;
            let actor = &self.peers.get(&owner).context("Unknown connection")?.actor;
            let first = self
                .simulation
                .state()
                .bricks
                .get(&brick)
                .context("Unknown brick")?;
            if !admits(actor, rule, first) {
                return Ok(Err(if first.owner == 0 {
                    ("public", "Public bricks cannot be copied.".to_string())
                } else {
                    (
                        "trust",
                        "The brick's owner does not trust you enough to do that.".to_string(),
                    )
                }));
            }
            let reach = crate::simulation::StackReach {
                up: reach.up,
                limited: reach.limited,
            };
            Ok(Ok(self.simulation.select_stack(
                brick,
                reach,
                limit,
                |b| admits(actor, rule, b),
            )?))
        })();
        self.finish_copy(owner, selected, rule, tool, package)
    }

    /// Copy every brick wholly inside the box from `min` to `max` (world
    /// units, grown out to the grid) that `rule` lets `owner` take, as
    /// [`Self::copy_build`] does.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_box(
        &mut self,
        owner: OwnerId,
        min: [f32; 3],
        max: [f32; 3],
        limit: usize,
        rule: CopyRule,
        tool: &str,
        package: &str,
    ) -> Copied {
        let selected = (|| {
            self.check_copy(limit, tool)?;
            let area = grid_box(min, max)?;
            let actor = &self.peers.get(&owner).context("Unknown connection")?.actor;
            let selection = self
                .simulation
                .select_box(area, limit, |b| admits(actor, rule, b));
            Ok(if selection.bricks.is_empty() && selection.refused > 0 {
                Err((
                    "trust",
                    "The bricks in that box belong to builds that do not trust you enough."
                        .to_string(),
                ))
            } else if selection.bricks.is_empty() {
                Err((
                    "empty",
                    "There are no bricks wholly inside that box.".to_string(),
                ))
            } else {
                Ok(selection)
            })
        })();
        self.finish_copy(owner, selected, rule, tool, package)
    }

    fn check_copy(&self, limit: usize, tool: &str) -> Result<()> {
        ensure!(
            (1..=MAX_BLUEPRINT_BRICKS).contains(&limit),
            "A copy holds 1 to {MAX_BLUEPRINT_BRICKS} bricks"
        );
        ensure!(
            self.weapons.contains_item(tool),
            "The copy's tool {tool} is not an item on this server"
        );
        Ok(())
    }

    /// Give `owner` the copy a selection made, or say why there is none.
    fn finish_copy(
        &mut self,
        owner: OwnerId,
        selected: Result<std::result::Result<crate::simulation::Selection, (&'static str, String)>>,
        rule: CopyRule,
        tool: &str,
        package: &str,
    ) -> Copied {
        let failed = |error| Copied {
            selection: Default::default(),
            error: Some(error),
        };
        let selection = match selected {
            Ok(Ok(selection)) => selection,
            Ok(Err(error)) => return failed(error),
            Err(error) => return failed(("invalid", format!("{error:#}"))),
        };
        let held = HeldCopy {
            sources: selection.bricks.clone(),
            package: package.into(),
            partial: rule.partial,
        };
        match self.hold_copy(owner, held, tool) {
            Ok(()) => Copied {
                selection,
                error: None,
            },
            Err(error) => failed(("invalid", format!("{error:#}"))),
        }
    }

    /// Give `owner` a copy of `held.sources` to place with `tool`, each
    /// brick as it is under any highlight.
    fn hold_copy(&mut self, owner: OwnerId, held: HeldCopy, tool: &str) -> Result<()> {
        let world = self.simulation.state();
        let bricks: Vec<Brick> = held
            .sources
            .iter()
            .map(|id| self.unlit(*id, &world.bricks[id]))
            .collect();
        let blueprint = Blueprint::capture(tool, &bricks, &self.simulation.definitions)?;
        self.hold_blueprint(owner, blueprint, held);
        Ok(())
    }

    /// Give `owner` `blueprint` to place, replacing any copy they hold.
    pub(super) fn hold_blueprint(&mut self, owner: OwnerId, blueprint: Blueprint, held: HeldCopy) {
        self.notify(owner, Notice::Blueprint(Some(Box::new(blueprint.clone()))));
        self.blueprints.insert(owner, blueprint);
        self.copies.insert(owner, held);
    }

    /// Light the bricks `owner`'s copy was taken from in the palette
    /// colour nearest `rgba`, glowing, for `seconds`.
    pub fn highlight_copy(&mut self, owner: OwnerId, rgba: [f32; 4], seconds: f32) -> Result<()> {
        let ids = self.copy_originals(owner)?;
        let color = self.closest_paint(rgba);
        self.light_bricks(&ids, color, super::highlight::GLOW, seconds)
    }

    /// Mirror the copy `owner` holds, as they see and plant it. The copy
    /// itself does not change: the mirror is part of where they put it,
    /// like its turn, and travels with `PlaceBlueprint`.
    pub fn mirror_copy(&mut self, owner: OwnerId, axis: MirrorAxis) -> Result<()> {
        ensure!(
            self.blueprints.contains_key(&owner),
            "Copy a build before mirroring it"
        );
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        let across_z = match axis {
            MirrorAxis::X => false,
            MirrorAxis::Z => true,
            // Facing north or south, left and right lie along x.
            MirrorAxis::View => {
                let facing = crate::ghost::cardinal(peer.player.state().forward());
                facing.x.abs() > facing.z.abs()
            }
        };
        self.notify(owner, Notice::MirrorCopy { across_z });
        Ok(())
    }

    /// The bricks `owner`'s copy was taken from that still stand.
    fn copy_originals(&self, owner: OwnerId) -> Result<Vec<BrickId>> {
        let sources = &self
            .copies
            .get(&owner)
            .context("Copy a build first")?
            .sources;
        let world = self.simulation.state();
        let standing: Vec<BrickId> = sources
            .iter()
            .copied()
            .filter(|id| world.bricks.contains_key(id))
            .collect();
        ensure!(
            !standing.is_empty(),
            "The bricks this copy was taken from are gone"
        );
        Ok(standing)
    }

    /// Every brick in `ids` is one `owner` may change with full trust (the
    /// hammer's and the spray can's), else how many are not.
    fn ensure_full_trust(&self, owner: OwnerId, ids: &[BrickId]) -> Result<()> {
        let actor = &self.peers.get(&owner).context("Unknown connection")?.actor;
        let world = self.simulation.state();
        let refused = ids
            .iter()
            .filter(|&&id| !actor.trusted(world.bricks[&id].owner, level::FULL))
            .count();
        ensure!(
            refused == 0,
            "{refused} of these bricks belong to builds that do not trust you enough."
        );
        Ok(())
    }

    /// Remove the bricks `owner`'s copy was taken from, all or none, as
    /// one undo step that puts them back exactly as they were. The copy
    /// stays in hand, so planting it elsewhere moves the build.
    pub fn cut_copy(&mut self, owner: OwnerId) -> Result<usize> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &self.minigames,
            bri_minigames::BuildAction::Build,
        )?;
        let ids = self.copy_originals(owner)?;
        self.ensure_full_trust(owner, &ids)?;
        let world = self.simulation.state();
        let removed: Vec<(BrickId, Brick)> = ids
            .iter()
            .map(|id| (*id, self.unlit(*id, &world.bricks[id])))
            .collect();
        let middle = removed
            .iter()
            .fold(Vec3::ZERO, |sum, (_, b)| sum + Vec3::from(b.position))
            / removed.len() as f32;
        // Checked above; the engine removes them in one pass.
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        self.simulation.remove_many(&engine, &ids)?;
        for &id in &ids {
            self.dirty.insert(id);
            self.events.respawns.remove(&id);
            self.close_inspections(id);
        }
        self.push_undo(owner, undo::UndoEntry::Cut(removed));
        let tick = self.simulation.state().tick;
        self.cues
            .emit(tick, crate::presentation::CueKind::Plant, middle.to_array());
        Ok(ids.len())
    }

    /// Paint the bricks `owner`'s copy was taken from in `color`, all or
    /// none, as one undo step.
    pub fn paint_copy(&mut self, owner: OwnerId, color: u8) -> Result<usize> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &self.minigames,
            bri_minigames::BuildAction::Paint,
        )?;
        ensure!(
            usize::from(color) < self.simulation.state().palette.len(),
            "That colour is not in this server's palette"
        );
        let ids = self.copy_originals(owner)?;
        self.ensure_full_trust(owner, &ids)?;
        let world = self.simulation.state();
        let before: Vec<(BrickId, u8)> = ids
            .iter()
            .map(|id| (*id, self.unlit(*id, &world.bricks[id]).color))
            .filter(|(_, old)| *old != color)
            .collect();
        let changed: Vec<BrickId> = before.iter().map(|(id, _)| *id).collect();
        if !changed.is_empty() {
            self.simulation.mutate_many(&changed, |b| b.color = color)?;
            self.dirty.extend(changed.iter().copied());
            self.push_undo(owner, undo::UndoEntry::Colors(before));
        }
        Ok(ids.len())
    }

    /// Outline a box for `owner` while `tool` is in their hand, or take
    /// it away.
    pub fn show_box(
        &mut self,
        owner: OwnerId,
        area: Option<([f32; 3], [f32; 3])>,
        tool: &str,
    ) -> Result<()> {
        ensure!(self.peers.contains_key(&owner), "No such player");
        let outline = match area {
            Some((min, max)) => {
                let area = grid_box(min, max)?;
                let corner = |cells: [i32; 3]| {
                    std::array::from_fn(|a| cells[a] as f32 * crate::grid::CELL[a])
                };
                Some(Outline {
                    tool: tool.into(),
                    min: corner(area.min),
                    max: corner(area.max()),
                })
            }
            None => None,
        };
        self.notify(owner, Notice::SelectionBox(outline.map(Box::new)));
        Ok(())
    }

    /// The copy `owner` holds, if any.
    pub fn blueprint(&self, owner: OwnerId) -> Option<&Blueprint> {
        self.blueprints.get(&owner)
    }

    /// Place the copy `owner` holds with its pivot at `position` (snapped
    /// to the nearest stud corner and plate), turned `quarter_turns`. All
    /// of it is planted, or none of it, unless its Add-On asked for each
    /// brick that fits ([`HeldCopy::partial`]); one undo entry.
    pub(super) fn place_blueprint(
        &mut self,
        owner: OwnerId,
        position: [f32; 3],
        quarter_turns: u8,
        mirrored: bool,
    ) -> Result<Reply> {
        ensure!(
            quarter_turns < 4
                && position
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1_000_000.0),
            "Invalid copy placement"
        );
        let blueprint = self
            .blueprints
            .get(&owner)
            .context("Select a build to copy first")?;
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &self.minigames,
            bri_minigames::BuildAction::Build,
        )?;
        let anchor = snap_anchor(position);
        let bricks = if mirrored {
            let (definitions, mirrors) = (&self.simulation.definitions, &mut self.mirrors);
            let (image, _) = blueprint.mirrored(|id| mirrors.image(definitions, id));
            image.placed(anchor, quarter_turns)
        } else {
            blueprint.placed(anchor, quarter_turns)
        };
        // The server's brick limit, then the plant rate: a copy needs a
        // plant window with room left and uses the rest of it.
        let settings = &self.admin.settings;
        if self.simulation.state().bricks.len() + bricks.len() > settings.brick_limit as usize
            || (!peer.actor.administrator && peer.plants >= settings.bricks_per_second)
        {
            return Err(crate::simulation::PlantFailure::Limit.into());
        }
        // TooFarDistance, from the feet to the copy's nearest part.
        let size = blueprint.turned_size(quarter_turns);
        let half = Vec3::new(size[0] as f32 * 0.25, 0.0, size[2] as f32 * 0.25);
        let middle = Vec3::from(anchor) + Vec3::new(0.0, size[1] as f32 * 0.1, 0.0);
        let feet = Vec3::from(peer.player.state().feet);
        if feet.distance(middle)
            > settings.too_far_distance.clamp(0.0, 100.0) + half.length() + size[1] as f32 * 0.1
        {
            return Err(crate::simulation::PlantFailure::TooFar.into());
        }
        let actor = peer.actor.clone();
        let rate = settings.bricks_per_second;
        let total = bricks.len();
        let (package, partial) = self
            .copies
            .get(&owner)
            .map_or((None, false), |c| (Some(c.package.clone()), c.partial));
        let planted = if partial {
            match self.simulation.plant_each(&actor, bricks) {
                (ids, _) if !ids.is_empty() => Ok(ids),
                (_, error) => Err(error.unwrap_or_else(|| anyhow::anyhow!("Nothing to plant"))),
            }
        } else {
            self.simulation.plant_group(&actor, bricks)
        };
        if let Some(package) = package {
            let count = planted.as_ref().map_or(0, Vec::len);
            self.report_place(&package, owner, count, total, planted.as_ref().err());
        }
        let ids = planted?;
        if let Some(peer) = self.peers.get_mut(&owner) {
            peer.plants = peer.plants.max(rate);
        }
        for &id in &ids {
            self.special_planted(owner, id)?;
            self.dirty.insert(id);
        }
        self.push_undo(owner, undo::UndoEntry::Group(ids.clone()));
        let tick = self.simulation.state().tick;
        self.cues
            .emit(tick, crate::presentation::CueKind::Plant, anchor);
        self.play_thread_three(tick, owner, "plant");
        Ok(Reply::Planted(ids[0]))
    }

    /// Put `item` in `owner`'s first free tool slot, unless they carry it
    /// already, and optionally take it in hand.
    ///
    /// Taking it in hand with every slot full puts the tool in hand (else
    /// the last) down on the ground to make room, where it can be picked up
    /// again: v20's `/duplicator` mounted its image without a slot, so it
    /// came out whatever the player carried.
    pub fn give_tool(&mut self, owner: OwnerId, item: &str, equip: bool) -> Result<()> {
        let actor = self
            .weapons
            .actor(bri_weapons::ActorId(owner))
            .context("Unknown connection")?;
        let held = actor
            .inventory
            .iter()
            .position(|held| held.as_deref() == Some(item));
        let full = actor.inventory.iter().all(Option::is_some);
        let room = actor.selected.unwrap_or(actor.inventory.len().saturating_sub(1));
        let direction = actor.frame.direction;
        let slot = match held {
            Some(slot) => slot,
            None => {
                if equip && full {
                    self.drop_tool(owner, room, direction)?;
                }
                self.give_item(owner, item)?
            }
        };
        if equip {
            self.equip_tool(owner, Some(slot))?;
        }
        Ok(())
    }

    /// An Add-On tool's `onFire`: run its command for the holder, aimed
    /// where the swing looks. A refusal (no such Add-On running, a
    /// cooldown) is only logged: the swing already played.
    pub(super) fn addon_tool_fire(&mut self, owner: OwnerId, command: &str) {
        self.addon_tool_command(owner, command, Vec::new());
    }

    /// [`Self::addon_tool_fire`] with arguments (a key's).
    pub(super) fn addon_tool_command(
        &mut self,
        owner: OwnerId,
        command: &str,
        args: Vec<super::packages::PackageArg>,
    ) {
        let Some(direction) = self
            .weapons
            .actor(bri_weapons::ActorId(owner))
            .map(|a| a.frame.direction.normalize_or_zero())
            .filter(|d| *d != Vec3::ZERO)
        else {
            return;
        };
        let Some((package, command)) = command.split_once(':') else {
            return;
        };
        let request = PackageCommand {
            package: package.into(),
            command: command.into(),
            args,
        };
        if let Err(error) = self.run_command(owner, request, direction, true) {
            if self.notices.len() == 64 {
                self.notices.pop_front();
            }
            self.notices
                .push_back(format!("Add-On tool {package}:{command}: {error:#}"));
        }
    }

    pub(super) fn forget_blueprint(&mut self, owner: OwnerId) {
        self.blueprints.remove(&owner);
        self.copies.remove(&owner);
    }
}

/// The grid cells covering the box from `min` to `max` (world units),
/// grown out to whole studs and plates.
fn grid_box(min: [f32; 3], max: [f32; 3]) -> Result<crate::grid::Bounds> {
    let span = bri_package_runtime::ops::MAX_BOX_SPAN;
    ensure!(
        (0..3).all(|a| min[a].is_finite()
            && max[a].is_finite()
            && min[a].abs() <= 1_000_000.0
            && max[a].abs() <= 1_000_000.0
            && max[a] >= min[a]
            && max[a] - min[a] <= span),
        "A box is at most {span} units on a side"
    );
    // A thousandth of slack, so a corner on a grid line stays on it.
    let low: [i32; 3] =
        std::array::from_fn(|a| ((min[a] / crate::grid::CELL[a]) + 0.001).floor() as i32);
    let high: [i32; 3] =
        std::array::from_fn(|a| ((max[a] / crate::grid::CELL[a]) - 0.001).ceil() as i32);
    Ok(crate::grid::Bounds {
        min: low,
        size: std::array::from_fn(|a| (high[a] - low[a]).max(1)),
    })
}
