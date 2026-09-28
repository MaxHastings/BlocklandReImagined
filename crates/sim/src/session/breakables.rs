//! Breakable map shapes: v20 `Glass`-class static shapes (the Bedroom
//! windows and light bulb, the Kitchen fluorescent lights).
//!
//! `Player::updatePos` calls `onImpact` on the server whenever the player's
//! speed into a surface exceeds `minImpactSpeed` (30). `Armor::onImpact` then
//! calls `StaticShape::explode` on a `Glass` shape that is not
//! `indestructable`, and skips falling damage for that impact. `explode`
//! sets the shape destroyed (clients hide it and play its explosion), plays
//! its `explosionSound` and hides it 100 ms later, which removes its
//! collision. Nothing repairs it: it stays broken until the mission reloads.
//! Projectiles, explosions and vehicles never break glass in v20.
use super::*;
use crate::map::Breakable;
use rapier3d::prelude::ColliderHandle;

/// `PlayerStandardArmor.minImpactSpeed`; the engine calls `onImpact` only
/// above it.
const MIN_IMPACT_SPEED: f32 = 30.0;
/// `explode`: `%obj.schedule(100, setHidden, 1)` at 120 Hz.
const HIDE_TICKS: u64 = 12;

#[derive(Default)]
pub(super) struct Breakables {
    shapes: Vec<Breakable>,
    /// Destroyed shapes by scene node; clients hide these.
    broken: BTreeSet<u32>,
    /// Shapes whose collision goes away at a tick.
    hiding: BTreeMap<usize, u64>,
    hidden: BTreeSet<usize>,
}

impl Session {
    /// Install the mission's breakable shapes (from `NativeMap`).
    pub fn set_breakables(&mut self, shapes: Vec<Breakable>) -> Result<()> {
        ensure!(shapes.len() <= 4096, "Too many breakable shapes");
        for shape in &shapes {
            ensure!(
                shape.position.is_finite() && shape.center.is_finite(),
                "Invalid breakable shape placement"
            );
            if !shape.colliders.is_empty() {
                // Validates the range against the map's colliders.
                self.simulation
                    .set_map_colliders(shape.colliders.clone(), true)?;
            }
        }
        self.breakables = Breakables {
            shapes,
            ..Default::default()
        };
        Ok(())
    }
    /// Scene nodes of destroyed shapes, replicated so clients hide them.
    pub fn broken_shapes(&self) -> BTreeSet<u32> {
        self.breakables.broken.clone()
    }
    /// Apply one tick's player collisions. Returns the players whose impact
    /// broke glass: `Armor::onImpact` returns before falling damage for them.
    pub(super) fn smash_breakables(
        &mut self,
        hits: Vec<(OwnerId, Vec<(ColliderHandle, f32)>)>,
    ) -> Result<BTreeSet<OwnerId>> {
        let mut smashers = BTreeSet::new();
        if self.breakables.shapes.is_empty() {
            return Ok(smashers);
        }
        for (owner, hits) in hits {
            for (handle, speed) in hits {
                if speed <= MIN_IMPACT_SPEED {
                    continue;
                }
                let Some(index) = self.simulation.map_collider_index(handle).and_then(|c| {
                    self.breakables
                        .shapes
                        .iter()
                        .position(|s| s.colliders.contains(&c))
                }) else {
                    continue;
                };
                if self.breakables.shapes[index].indestructable {
                    continue;
                }
                self.explode_shape(index);
                smashers.insert(owner);
            }
        }
        Ok(smashers)
    }
    /// `StaticShape::explode`. A second hit before the shape is hidden plays
    /// the sound again, as v20 does; the explosion only plays once.
    fn explode_shape(&mut self, index: usize) {
        let tick = self.simulation.state().tick;
        let shape = &self.breakables.shapes[index];
        let node = shape.node;
        let sound = shape.sound.clone();
        let explosion = shape.explosion.clone();
        let (position, center) = (shape.position.to_array(), shape.center.to_array());
        if self.breakables.broken.insert(node)
            && let Some(definition) = explosion
        {
            self.cues.emit(
                tick,
                crate::presentation::CueKind::WeaponEffect {
                    source: bri_weapons::TargetId::Map(u64::from(node)),
                    definition,
                    node: String::new(),
                    seconds: 0.,
                    image: None,
                    hand: None,
                    direction: None,
                    scale: 1.,
                },
                center,
            );
        }
        if let Some(profile) = sound {
            self.cues.emit(
                tick,
                crate::presentation::CueKind::WeaponSound { profile },
                position,
            );
        }
        if !self.breakables.hidden.contains(&index) {
            self.breakables.hiding.insert(index, tick + HIDE_TICKS);
        }
    }
    /// `setHidden(1)`: the shape stops colliding.
    pub(super) fn step_breakables(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        let due: Vec<usize> = self
            .breakables
            .hiding
            .iter()
            .filter(|(_, at)| **at <= tick)
            .map(|(index, _)| *index)
            .collect();
        for index in due {
            self.breakables.hiding.remove(&index);
            self.breakables.hidden.insert(index);
            let colliders = self.breakables.shapes[index].colliders.clone();
            if !colliders.is_empty() {
                self.simulation.set_map_colliders(colliders, false)?;
            }
        }
        Ok(())
    }
}
