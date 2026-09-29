//! Brick deaths: v20 `killBrick` (hammer, Destructo Wand), `fakeKillBrick`
//! and brick explosions.
//!
//! The server only changes the brick and announces the death with a
//! `BrickKill` cue. Its `BrickDeath` tells clients which of v20's two looks
//! to draw: a killed brick falls through the world, a blasted one is
//! thrown as a physics body (`transmitBrickExplosion`). Either is short-lived
//! and cosmetic; it never affects play.
use super::*;
use crate::presentation::BrickDeath;

/// `killBrick` has no blast of its own. Clients throw a killed brick their
/// own way (see `BrickDeath::Kill`); the cue still carries a small pop up
/// from below the brick for anything that reads the blast fields.
const KILL_POP_FORCE: f32 = 12.0;

/// Where debris is thrown from, as v20 `transmitBrickExplosion(center, force,
/// radius, ...)`. Clients push each brick away from `origin`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrickBlast {
    pub origin: Vec3,
    pub force: f32,
    /// Falloff distance; direct hits and events use v20's tiny radii.
    pub radius: f32,
}
impl BrickBlast {
    /// A `killBrick` pop: straight up from just below the brick's center.
    fn pop(center: Vec3) -> Self {
        Self {
            origin: center - Vec3::Y,
            force: KILL_POP_FORCE,
            radius: 0.,
        }
    }
    /// `fakeKillBrick(vector, time)`: thrown along `velocity`.
    pub fn fake_kill(center: Vec3, velocity: Vec3) -> Self {
        let (origin, force) = bri_events::semantics::fake_kill_origin(center, velocity);
        Self {
            origin,
            force,
            radius: 1.,
        }
    }
}

impl Session {
    /// `killBrick`: remove the brick for good; clients draw it falling
    /// through the world. The hammer, both wands and undo come through here.
    /// Like v20, every brick left with no path to the ground dies with it
    /// (the chain kill); the hammer never gets here with such a brick.
    pub(super) fn kill_brick(&mut self, actor: &Actor, brick: BrickId) -> Result<()> {
        let stranded = self.simulation.stranded_by(brick)?;
        self.kill_one_brick(actor, brick, None)?;
        // The engine kills these, whoever owns them.
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        // Each brick's look is captured, then all are removed in one pass:
        // a collapse costs one collision refresh, not one per brick.
        let mut fallen = Vec::new();
        let mut cues = Vec::new();
        for id in stranded {
            let Some(b) = self.simulation.state().bricks.get(&id) else {
                continue;
            };
            if self.simulation.definitions.get(b)?.indestructible {
                continue;
            }
            cues.push(self.brick_kill_cue(id, None)?);
            fallen.push(id);
        }
        self.simulation.remove_many(&engine, &fallen)?;
        for (id, cue) in fallen.into_iter().zip(cues) {
            self.dirty.insert(id);
            self.events.respawns.remove(&id);
            self.emit_brick_kill(cue);
        }
        Ok(())
    }
    /// Remove one brick, without the chain kill. With no `blast` it dies
    /// like `killBrick`; a blast throws it as debris. Package voxel worlds
    /// remove their own voxels one at a time and record each.
    pub(super) fn kill_one_brick(
        &mut self,
        actor: &Actor,
        brick: BrickId,
        blast: Option<BrickBlast>,
    ) -> Result<()> {
        let cue = self.brick_kill_cue(brick, blast)?;
        self.simulation.remove(actor, brick)?;
        self.dirty.insert(brick);
        self.events.respawns.remove(&brick);
        self.emit_brick_kill(cue);
        Ok(())
    }
    /// `fakeKillBrick` and brick explosions: hide the brick, make it
    /// intangible, throw its debris and bring it back after `respawn_ticks`
    /// (which fires `onRespawn`).
    pub(super) fn fake_kill_brick(
        &mut self,
        brick: BrickId,
        blast: BrickBlast,
        respawn_ticks: u64,
    ) -> Result<()> {
        self.fake_kill_bricks(&[(brick, blast)], respawn_ticks)
    }
    /// `fake_kill_brick` for every brick a blast knocks out, with one
    /// collision refresh for them all.
    pub(super) fn fake_kill_bricks(
        &mut self,
        kills: &[(BrickId, BrickBlast)],
        respawn_ticks: u64,
    ) -> Result<()> {
        let cues = kills
            .iter()
            .map(|(brick, blast)| self.brick_kill_cue(*brick, Some(*blast)))
            .collect::<Result<Vec<_>>>()?;
        let bricks: Vec<BrickId> = kills.iter().map(|(brick, _)| *brick).collect();
        self.simulation.mutate_many(&bricks, |b| {
            b.visible = false;
            b.raycast = false;
            b.colliding = false;
        })?;
        let tick = self.simulation.state().tick;
        for (brick, cue) in bricks.into_iter().zip(cues) {
            self.dirty.insert(brick);
            self.events
                .respawns
                .insert(brick, tick + respawn_ticks.max(1));
            self.emit_brick_kill(cue);
        }
        Ok(())
    }
    /// Capture the brick's look before it changes, so clients can draw the
    /// debris even when the brick is already gone from their world. No
    /// `blast` is a `killBrick`.
    fn brick_kill_cue(
        &self,
        brick: BrickId,
        blast: Option<BrickBlast>,
    ) -> Result<(crate::presentation::CueKind, [f32; 3])> {
        let b = self
            .simulation
            .state()
            .bricks
            .get(&brick)
            .context("Unknown brick")?;
        let (death, blast) = match blast {
            Some(blast) => (BrickDeath::Blast, blast),
            None => (BrickDeath::Kill, BrickBlast::pop(Vec3::from(b.position))),
        };
        ensure!(
            blast.origin.is_finite() && blast.force.is_finite() && blast.radius.is_finite(),
            "Invalid brick blast"
        );
        Ok((
            crate::presentation::CueKind::BrickKill {
                brick,
                death,
                definition: b.definition.clone(),
                quarter_turns: b.quarter_turns,
                color: b.color,
                color_effect: b.color_effect,
                shape_effect: b.shape_effect,
                print: b.print.clone(),
                origin: blast.origin.to_array(),
                force: blast.force.clamp(0., crate::presentation::MAX_BRICK_FORCE),
                radius: blast.radius.clamp(0., crate::presentation::MAX_BRICK_FORCE),
            },
            b.position,
        ))
    }
    fn emit_brick_kill(&mut self, (kind, position): (crate::presentation::CueKind, [f32; 3])) {
        let tick = self.simulation.state().tick;
        self.cues.emit(tick, kind, position);
    }
}
