//! Brick deaths: v20 `killBrick` (hammer, Destructo Wand), `fakeKillBrick`
//! and brick explosions.
//!
//! The server only changes the brick and announces the death with a
//! `BrickKill` cue, like v20's `transmitBrickExplosion`. Every client turns
//! the cue into short-lived cosmetic debris; the debris never affects play.
use super::*;

/// `killBrick` has no blast of its own; v20 pops the brick up off its spot.
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
    pub fn pop(center: Vec3) -> Self {
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
    /// `killBrick`: remove the brick for good and throw its debris. The
    /// hammer, both wands and undo come through here. Like v20, every brick
    /// left with no path to the ground dies with it (the chain kill); the
    /// hammer never gets here with such a brick.
    pub(super) fn kill_brick(
        &mut self,
        actor: &Actor,
        brick: BrickId,
        blast: BrickBlast,
    ) -> Result<()> {
        let stranded = self.simulation.stranded_by(brick)?;
        self.kill_brick_with(actor, brick, blast, stranded)
    }
    /// `killBrick` without the chain kill, for package rules that remove
    /// their own bricks (a mined voxel has no map ground to hang from).
    pub(super) fn kill_lone_brick(
        &mut self,
        actor: &Actor,
        brick: BrickId,
        blast: BrickBlast,
    ) -> Result<()> {
        self.kill_brick_with(actor, brick, blast, Vec::new())
    }
    fn kill_brick_with(
        &mut self,
        actor: &Actor,
        brick: BrickId,
        blast: BrickBlast,
        stranded: Vec<BrickId>,
    ) -> Result<()> {
        let cue = self.brick_kill_cue(brick, blast)?;
        self.simulation.remove(actor, brick)?;
        self.dirty.insert(brick);
        self.events.respawns.remove(&brick);
        self.emit_brick_kill(cue);
        // The engine kills these, whoever owns them.
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        for id in stranded {
            let Some(b) = self.simulation.state().bricks.get(&id) else {
                continue;
            };
            if self.simulation.definitions.get(b)?.indestructible {
                continue;
            }
            let cue = self.brick_kill_cue(id, BrickBlast::pop(Vec3::from(b.position)))?;
            self.simulation.remove(&engine, id)?;
            self.dirty.insert(id);
            self.events.respawns.remove(&id);
            self.emit_brick_kill(cue);
        }
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
        let cue = self.brick_kill_cue(brick, blast)?;
        self.simulation.mutate(brick, |b| {
            b.visible = false;
            b.raycast = false;
            b.colliding = false;
        })?;
        self.dirty.insert(brick);
        let tick = self.simulation.state().tick;
        self.events
            .respawns
            .insert(brick, tick + respawn_ticks.max(1));
        self.emit_brick_kill(cue);
        Ok(())
    }
    /// Capture the brick's look before it changes, so clients can draw the
    /// debris even when the brick is already gone from their world.
    fn brick_kill_cue(
        &self,
        brick: BrickId,
        blast: BrickBlast,
    ) -> Result<(crate::presentation::CueKind, [f32; 3])> {
        let b = self
            .simulation
            .state()
            .bricks
            .get(&brick)
            .context("Unknown brick")?;
        ensure!(
            blast.origin.is_finite() && blast.force.is_finite() && blast.radius.is_finite(),
            "Invalid brick blast"
        );
        Ok((
            crate::presentation::CueKind::BrickKill {
                brick,
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
