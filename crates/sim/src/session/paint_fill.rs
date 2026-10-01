//! Flood-fill painting for Add-Ons (`paint_fill`): recolour the bricks of
//! one colour that touch the brick a player clicked, as their spray can
//! would paint each one, in one step of their undo. The Fill Can Add-On
//! is built on it.
use super::*;
use bri_world::authority::trust as level;

/// What a fill did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fill {
    /// Bricks recoloured.
    pub painted: usize,
    /// Bricks of the colour, touching the fill, that it left alone because
    /// their builds do not trust the painter fully.
    pub refused: usize,
}

impl Session {
    /// Paint `start` and every brick of its colour joined to it through
    /// shared faces (`Simulation::touching_region`) in palette colour
    /// `color`, with `owner`'s spray can rules: the minigame's paint rule,
    /// full trust brick by brick (a fill flows around bricks it may not
    /// paint, never through them). More than `limit` bricks is refused
    /// rather than cut short, so a fill never stops half way across a wall.
    pub fn paint_fill(
        &mut self,
        owner: OwnerId,
        start: BrickId,
        color: u8,
        limit: usize,
    ) -> Result<Fill> {
        ensure!(
            (1..=bri_package_runtime::ops::MAX_FILL_BRICKS).contains(&limit),
            "A fill paints 1 to {} bricks",
            bri_package_runtime::ops::MAX_FILL_BRICKS
        );
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
        let first = self
            .simulation
            .state()
            .bricks
            .get(&start)
            .context("That brick is gone")?;
        let (from, group) = (first.color, first.owner);
        if !peer.actor.trusted(group, level::FULL) {
            let name = self.brick_group_name(group);
            anyhow::bail!("{name} does not trust you enough to do that.");
        }
        ensure!(from != color, "Those bricks are already that colour.");
        let actor = &self.peers[&owner].actor;
        let mut refused = 0;
        let region = self.simulation.touching_region(start, limit, |_, brick| {
            if brick.color != from {
                return false;
            }
            let allowed = actor.trusted(brick.owner, level::FULL);
            refused += usize::from(!allowed);
            allowed
        })?;
        let Some(region) = region else {
            anyhow::bail!("More than {limit} bricks of that colour touch here.");
        };
        let world = self.simulation.state();
        let before: Vec<(BrickId, copy_edits::Look)> = region
            .iter()
            .map(|&id| (id, copy_edits::Look::of(&world.bricks[&id])))
            .collect();
        self.simulation.mutate_many(&region, |b| b.color = color)?;
        self.dirty.extend(region.iter().copied());
        self.push_undo(owner, undo::UndoEntry::Looks(before));
        Ok(Fill {
            painted: region.len(),
            refused,
        })
    }
}
