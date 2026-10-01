//! Flood-fill painting for Add-Ons (`paint_fill`): recolour, or give a
//! colour or shape effect to, the bricks of one colour joined to the brick
//! a player hit, as their spray cans would paint each one, in one step of
//! their undo. Vehicles are painted too (`paint_vehicle`). Imported Fill
//! Can Add-Ons are ported onto them.
use super::*;
use bri_package_runtime::ops::{FillPaint, MAX_FILL_BRICKS, VehiclePaint};
use bri_vehicles::VehicleId;
use bri_world::authority::trust as level;

/// What a fill did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fill {
    /// Bricks painted.
    pub painted: usize,
    /// Bricks of the colour, joined to the fill, that it left alone because
    /// their builds do not trust the painter fully.
    pub refused: usize,
    /// The fill reached its limit and stopped there.
    pub stopped: bool,
}

/// How far a fill spreads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillRules {
    /// Most bricks it paints.
    pub limit: usize,
    /// Join bricks whose boxes overlap a brick's box grown this much
    /// (sideways, vertical), as v20's `containerBoxSearch` did, instead of
    /// through shared faces.
    pub reach: Option<[f32; 2]>,
    /// More bricks than `limit`: paint the first `limit` and stop, as
    /// v20's Fill Can did, instead of refusing the fill.
    pub stop_at_limit: bool,
}

impl Session {
    /// Paint `start` and every brick of its colour joined to it with
    /// `paint`, with `owner`'s spray can rules: the minigame's paint rule,
    /// full trust brick by brick (a fill flows around bricks it may not
    /// paint, never through them). A colour fill of bricks already that
    /// colour does nothing.
    pub fn paint_fill(
        &mut self,
        owner: OwnerId,
        start: BrickId,
        paint: FillPaint,
        rules: FillRules,
    ) -> Result<Fill> {
        let limit = rules.limit;
        ensure!(
            (1..=MAX_FILL_BRICKS).contains(&limit),
            "A fill paints 1 to {MAX_FILL_BRICKS} bricks"
        );
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &self.minigames,
            bri_minigames::BuildAction::Paint,
        )?;
        match paint {
            FillPaint::Color(color) => ensure!(
                usize::from(color) < self.simulation.state().palette.len(),
                "That colour is not in this server's palette"
            ),
            FillPaint::ColorEffect(fx) => ensure!(fx <= 6, "There is no colour effect {fx}"),
            FillPaint::ShapeEffect(fx) => ensure!(fx <= 2, "There is no shape effect {fx}"),
        }
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
        if paint == FillPaint::Color(from) {
            return Ok(Fill::default());
        }
        let actor = &self.peers[&owner].actor;
        let mut refused = 0;
        let (region, more) =
            self.simulation
                .fill_region(start, limit, rules.reach, |_, brick| {
                    if brick.color != from {
                        return false;
                    }
                    let allowed = actor.trusted(brick.owner, level::FULL);
                    refused += usize::from(!allowed);
                    allowed
                })?;
        if more && !rules.stop_at_limit {
            anyhow::bail!("More than {limit} bricks of that colour touch here.");
        }
        let bricks = &self.simulation.state().bricks;
        let before: Vec<(BrickId, u8)> = region
            .iter()
            .map(|id| {
                let b = &bricks[id];
                let old = match paint {
                    FillPaint::Color(_) => b.color,
                    FillPaint::ColorEffect(_) => b.color_effect,
                    FillPaint::ShapeEffect(_) => b.shape_effect,
                };
                (*id, old)
            })
            .collect();
        self.simulation.mutate_many(&region, |b| match paint {
            FillPaint::Color(c) => b.color = c,
            FillPaint::ColorEffect(fx) => b.color_effect = fx,
            FillPaint::ShapeEffect(fx) => b.shape_effect = fx,
        })?;
        self.dirty.extend(region.iter().copied());
        self.push_undo(owner, undo::UndoEntry::Fill(paint, before));
        Ok(Fill {
            painted: region.len(),
            refused,
            stopped: rules.stop_at_limit && region.len() == limit,
        })
    }

    /// Paint `vehicle` as `owner`: the minigame's paint rule, and full
    /// trust from the build of the brick that spawned it (or from its
    /// owner, for a vehicle no brick spawned). A palette colour on a
    /// vehicle its spawn brick recolours paints the brick too, as
    /// `fxDTSBrick::colorVehicle` takes the brick's colour; anything else
    /// colours the vehicle alone until it respawns. Returns the colour it
    /// took; one it already had leaves no undo step.
    pub fn paint_vehicle(
        &mut self,
        owner: OwnerId,
        vehicle: u64,
        paint: VehiclePaint,
    ) -> Result<[f32; 4]> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &self.minigames,
            bri_minigames::BuildAction::Paint,
        )?;
        let id = VehicleId(vehicle);
        let (vehicle_owner, _) = self
            .vehicles
            .world
            .as_ref()
            .and_then(|w| w.owner_of(id))
            .context("That vehicle is gone")?;
        let state = self.simulation.state();
        let spawn = self
            .vehicle_spawn_brick(id)
            .and_then(|b| Some((b, state.bricks.get(&b)?)));
        let group = spawn.map_or(vehicle_owner.0, |(_, b)| b.owner);
        if !peer.actor.trusted(group, level::FULL) {
            let name = self.brick_group_name(group);
            anyhow::bail!("{name} does not trust you enough to do that.");
        }
        let (color, brick) = match paint {
            VehiclePaint::Color(c) => {
                let &[r, g, b, _] = state
                    .palette
                    .get(usize::from(c))
                    .context("That colour is not in this server's palette")?;
                let recolors = spawn.filter(|(_, b)| b.vehicle.as_ref().is_some_and(|v| v.recolor));
                ([r, g, b, 1.0], recolors.map(|(id, b)| (id, b.color, c)))
            }
            VehiclePaint::Rgb([r, g, b]) => ([r, g, b, 1.0], None),
        };
        let before = self.vehicles.colors.get(&id).copied().flatten();
        let repaint = brick.filter(|(_, from, to)| from != to);
        if before == Some(color) && repaint.is_none() {
            return Ok(color);
        }
        if let Some((brick, _, to)) = repaint {
            self.simulation.mutate(brick, |b| b.color = to)?;
            self.dirty.insert(brick);
        }
        self.vehicles.colors.insert(id, Some(color));
        self.push_undo(
            owner,
            undo::UndoEntry::Vehicle {
                vehicle: id,
                color,
                before,
                brick: repaint,
            },
        );
        Ok(color)
    }
}
