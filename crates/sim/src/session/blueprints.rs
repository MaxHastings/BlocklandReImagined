//! Copies of builds held for players, and placing them (see
//! [`crate::blueprint`]). An Add-On decides what to copy and for whom
//! (`copy_build`); placing is the player's own command and follows the
//! plant rules: minigame building, reach, trust, overlap, support, the
//! brick limit, and the plant rate, which a copy uses up as one plant
//! window of its own.
use super::*;
use crate::blueprint::{Blueprint, MAX_BLUEPRINT_BRICKS, snap_anchor};

impl Session {
    /// Copy the build at `brick` for `owner` (`Simulation::build_from`),
    /// replacing any copy they hold, and send it to them to place with
    /// `tool`. Returns the number of bricks copied.
    pub fn copy_build(
        &mut self,
        owner: OwnerId,
        brick: BrickId,
        limit: usize,
        above_only: bool,
        tool: &str,
    ) -> Result<usize> {
        ensure!(
            (1..=MAX_BLUEPRINT_BRICKS).contains(&limit),
            "A copy holds 1 to {MAX_BLUEPRINT_BRICKS} bricks"
        );
        ensure!(
            self.weapons.contains_item(tool),
            "The copy's tool {tool} is not an item on this server"
        );
        let actor = &self.peers.get(&owner).context("Unknown connection")?.actor;
        let ids = self
            .simulation
            .build_from(actor, brick, limit, above_only)?;
        let world = self.simulation.state();
        let bricks: Vec<Brick> = ids.iter().map(|id| world.bricks[id].clone()).collect();
        let blueprint = Blueprint::capture(tool, &bricks, &self.simulation.definitions)?;
        self.notify(owner, Notice::Blueprint(Some(Box::new(blueprint.clone()))));
        self.blueprints.insert(owner, blueprint);
        Ok(ids.len())
    }

    /// The copy `owner` holds, if any.
    pub fn blueprint(&self, owner: OwnerId) -> Option<&Blueprint> {
        self.blueprints.get(&owner)
    }

    /// Place the copy `owner` holds with its pivot at `position` (snapped
    /// to the nearest stud corner and plate), turned `quarter_turns`. All
    /// of it is planted, or none of it, with one undo entry.
    pub(super) fn place_blueprint(
        &mut self,
        owner: OwnerId,
        position: [f32; 3],
        quarter_turns: u8,
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
        let bricks = blueprint.placed(anchor, quarter_turns);
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
        let ids = self.simulation.plant_group(&actor, bricks)?;
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
            args: Vec::new(),
        };
        if let Err(error) = self.package_command(owner, request, direction) {
            if self.notices.len() == 64 {
                self.notices.pop_front();
            }
            self.notices
                .push_back(format!("Add-On tool {package}:{command}: {error:#}"));
        }
    }

    pub(super) fn forget_blueprint(&mut self, owner: OwnerId) {
        self.blueprints.remove(&owner);
    }
}
