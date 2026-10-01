//! Solid bricks share one collider per chunk of the world.
//!
//! A collider per brick costs a broad-phase leaf, a rigid collider and its
//! bookkeeping for each of a million bricks: most of a big save's placement
//! time and host memory. Solid bricks (colliding, not water) instead become
//! parts of one compound collider per `CHUNK_SIZE` cube, rebuilt when one of
//! its bricks changes; every chunk a tick touches is rebuilt once, at the
//! next collision refresh. Other bricks (sensors: non-colliding and water)
//! keep a collider each. A chunk's collider is tagged `CHUNK_TAG | serial`;
//! `Chunks::part_brick` maps a compound part back to its brick, for contact
//! and hit identity.
use crate::parking::Parking;
use bri_world::BrickId;
use rapier3d::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// Collider tag of a chunk: this, or'ed with the chunk's serial.
pub const CHUNK_TAG: u128 = 4 << 64;
/// Chunk edge, in world units, on every axis.
pub const CHUNK_SIZE: f32 = 8.0;

fn aabb(b: &Aabb) -> ([f32; 3], [f32; 3]) {
    (b.mins.to_array(), b.maxs.to_array())
}

/// Whether a collider tag is a chunk's.
pub fn is_chunk(tag: u128) -> bool {
    tag >> 64 == CHUNK_TAG >> 64
}

pub type ChunkKey = [i32; 3];

/// The chunk a brick at `position` (its centre) belongs to.
pub fn chunk_of(position: [f32; 3]) -> ChunkKey {
    position.map(|p| (p / CHUNK_SIZE).floor() as i32)
}

struct Chunk {
    serial: u64,
    members: BTreeSet<BrickId>,
    handle: Option<ColliderHandle>,
    /// The brick of each compound part, in part order.
    parts: Vec<BrickId>,
}

#[derive(Default)]
pub struct Chunks {
    chunks: BTreeMap<ChunkKey, Chunk>,
    by_serial: BTreeMap<u64, ChunkKey>,
    dirty: BTreeSet<ChunkKey>,
    next_serial: u64,
    /// Boxes of collision rebuilt since `take_changed`, while someone reads
    /// them (`track_changes`): bots' walk grid forgets what lies inside.
    changed: Option<Vec<([f32; 3], [f32; 3])>>,
    /// Bricks already in the chunks marked dirty since `take_rebuilt`: what
    /// the next `flush` builds again besides the bricks changed.
    rebuilt: usize,
}

impl Chunks {
    /// Make a solid brick part of its chunk (built at the next `flush`).
    pub fn insert(&mut self, id: BrickId, position: [f32; 3]) {
        let key = chunk_of(position);
        let serial = &mut self.next_serial;
        let chunk = self.chunks.entry(key).or_insert_with(|| {
            *serial += 1;
            Chunk {
                serial: *serial,
                members: BTreeSet::new(),
                handle: None,
                parts: Vec::new(),
            }
        });
        self.by_serial.insert(chunk.serial, key);
        let had = chunk.members.len();
        if chunk.members.insert(id) && self.dirty.insert(key) {
            self.rebuilt += had;
        }
    }
    /// Take a brick out of its chunk; false when it was not a part.
    pub fn remove(&mut self, id: BrickId, position: [f32; 3]) -> bool {
        let key = chunk_of(position);
        let Some(chunk) = self.chunks.get_mut(&key) else {
            return false;
        };
        let removed = chunk.members.remove(&id);
        if removed && self.dirty.insert(key) {
            self.rebuilt += chunk.members.len();
        }
        removed
    }
    /// Mark the chunk of the brick at `position` for rebuilding, ahead of
    /// a change to it (a slice of bricks removed together).
    pub fn mark(&mut self, id: BrickId, position: [f32; 3]) {
        let key = chunk_of(position);
        if let Some(chunk) = self.chunks.get(&key)
            && chunk.members.contains(&id)
            && self.dirty.insert(key)
        {
            self.rebuilt += chunk.members.len() - 1;
        }
    }
    /// Bricks the chunks marked since the last call hold besides the ones
    /// changed: what rebuilding them costs on top of the change itself.
    pub fn take_rebuilt(&mut self) -> usize {
        std::mem::take(&mut self.rebuilt)
    }
    /// Whether a brick is a part of its chunk.
    pub fn contains(&self, id: BrickId, position: [f32; 3]) -> bool {
        self.chunks
            .get(&chunk_of(position))
            .is_some_and(|chunk| chunk.members.contains(&id))
    }
    /// The brick a part of a chunk's collider belongs to.
    pub fn part_brick(&self, tag: u128, part: usize) -> Option<BrickId> {
        if !is_chunk(tag) {
            return None;
        }
        let key = self.by_serial.get(&(tag as u64))?;
        self.chunks.get(key)?.parts.get(part).copied()
    }
    /// Rebuild every chunk changed since the last flush, each once.
    /// `shape_of` gives a brick's collision shape and world pose. An emptied
    /// chunk's collider leaves through `parking`.
    pub fn flush(
        &mut self,
        physics: &mut PhysicsWorld,
        parking: &mut Parking,
        shape_of: impl Fn(BrickId) -> Option<(Pose, SharedShape)>,
    ) {
        let mut retired = Vec::new();
        // The shapes rebuilt chunks had, freed off the tick.
        let mut old = Vec::new();
        self.rebuilt = 0;
        for key in std::mem::take(&mut self.dirty) {
            let Some(chunk) = self.chunks.get_mut(&key) else {
                continue;
            };
            let mut parts = Vec::new();
            let mut owners = Vec::new();
            for &id in &chunk.members {
                let Some((pose, shape)) = shape_of(id) else {
                    continue;
                };
                match shape.as_compound() {
                    Some(compound) => {
                        for (part_pose, part) in compound.shapes() {
                            parts.push((pose * *part_pose, part.clone()));
                            owners.push(id);
                        }
                    }
                    None => {
                        parts.push((pose, shape.clone()));
                        owners.push(id);
                    }
                }
            }
            if parts.is_empty() {
                if let (Some(changed), Some(handle)) = (&mut self.changed, chunk.handle) {
                    changed.push(aabb(&physics.colliders[handle].compute_aabb()));
                }
                if let Some(handle) = chunk.handle.take() {
                    retired.push(handle);
                }
                let serial = chunk.serial;
                self.chunks.remove(&key);
                self.by_serial.remove(&serial);
                continue;
            }
            let shape = SharedShape::compound(parts);
            if let Some(changed) = &mut self.changed {
                if let Some(handle) = chunk.handle {
                    changed.push(aabb(&physics.colliders[handle].compute_aabb()));
                }
                changed.push(aabb(&shape.compute_aabb(&Pose::IDENTITY)));
            }
            match chunk.handle {
                Some(handle) => {
                    let collider = &mut physics.colliders[handle];
                    old.push(collider.shared_shape().clone());
                    collider.set_shape(shape);
                }
                None => {
                    chunk.handle = Some(physics.insert_collider(
                        ColliderBuilder::new(shape).user_data(CHUNK_TAG | u128::from(chunk.serial)),
                        None,
                    ))
                }
            }
            chunk.parts = owners;
        }
        if !retired.is_empty() {
            parking.remove(physics, &retired);
        }
        if !old.is_empty() {
            crate::drop_later::drop_later(old);
        }
    }
    /// Start or stop recording the boxes of rebuilt chunks.
    pub fn track_changes(&mut self, track: bool) {
        match (track, self.changed.is_some()) {
            (true, false) => self.changed = Some(Vec::new()),
            (false, true) => self.changed = None,
            _ => {}
        }
    }
    /// Record a changed box from outside the chunks (map collision).
    pub fn note_changed(&mut self, b: &Aabb) {
        if let Some(changed) = &mut self.changed {
            changed.push(aabb(b));
        }
    }
    /// Boxes of collision rebuilt since the last take (old and new shapes).
    pub fn take_changed(&mut self) -> Vec<([f32; 3], [f32; 3])> {
        self.changed.as_mut().map(std::mem::take).unwrap_or_default()
    }
    /// Chunks waiting for `flush`.
    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }
    /// Chunk colliders in the world.
    pub fn len(&self) -> usize {
        self.chunks.values().filter(|c| c.handle.is_some()).count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl bri_motor::torque::PartTags for Chunks {
    fn part_tag(&self, collider: u128, part: usize) -> Option<u128> {
        self.part_brick(collider, part).map(u128::from)
    }
}
