//! Hit queries outlive allocation of dynamic bodies. Budgeted source-box queries
//! preserve gaps even when an assembly has no usable compound physics proxy.
use super::*;
use bri_sim::grid::{Bounds, QueryCursor};
type Key = (u64, u64);
type Hit = (u64, Key, usize);
struct Query {
    hit: Hit,
    cursor: QueryCursor,
    from: Vec3,
    direction: Vec3,
    length: f32,
    push: Vec3,
}
#[derive(Default)]
pub(in crate::brick_debris) struct Queries {
    pending: VecDeque<Query>,
    struck: BTreeSet<Hit>,
    queued: BTreeSet<Hit>,
}
impl Queries {
    pub(in crate::brick_debris) fn enqueue(
        &mut self,
        sections: &BTreeMap<Key, Section>,
        world: &PhysicsWorld,
        shots: &Shots,
    ) {
        self.struck.retain(|(_, key, _)| sections.contains_key(key));
        if self.struck.len() > 4096 {
            self.struck.clear();
        }
        let mut work = 1024;
        for &(projectile, from, to, _) in shots.segments() {
            for (&key, section) in sections {
                if section.pieces.is_empty() {
                    continue;
                }
                let count = section.chunks.len().max(1);
                for chunk in 0..count {
                    if work == 0 {
                        return;
                    }
                    work -= 1;
                    if self.pending.len() >= 256 {
                        return;
                    }
                    let assembly = section.chunks.is_empty();
                    if !assembly
                        && section.chunks[chunk].handle.is_some_and(|h| {
                            let (index, generation) = h.0.into_raw_parts();
                            shots.struck(projectile, u64::from(generation) << 32 | u64::from(index))
                        })
                    {
                        continue;
                    }
                    if !assembly
                        && section.chunks[chunk]
                            .handle
                            .is_some_and(|h| world.bodies[h].is_enabled())
                    {
                        continue;
                    }
                    let chunk_id = if assembly { usize::MAX } else { chunk };
                    let hit = (projectile, key, chunk_id);
                    if self.struck.contains(&hit) || self.queued.contains(&hit) {
                        continue;
                    }
                    let (center, turn, rest) = if assembly {
                        let (c, t) = section.pose(section.shown);
                        (c + section.recoil, t, section.origin)
                    } else {
                        let c = &section.chunks[chunk];
                        let (p, t) = c.pose(world);
                        (p, t, section.origin + c.local)
                    };
                    let a = rest + turn.inverse() * (from - center);
                    let b = rest + turn.inverse() * (to - center);
                    let length = a.distance(b);
                    if length < 1e-5 {
                        continue;
                    }
                    let low = a.min(b);
                    let high = a.max(b);
                    let min =
                        std::array::from_fn(|i| (low[i] / bri_sim::grid::CELL[i]).floor() as i32);
                    let max: [i32; 3] = std::array::from_fn(|i| {
                        (high[i] / bri_sim::grid::CELL[i]).ceil() as i32 + 1
                    });
                    self.queued.insert(hit);
                    self.pending.push_back(Query {
                        hit,
                        cursor: QueryCursor::new(Bounds {
                            min,
                            size: std::array::from_fn(|i| (max[i] - min[i]).max(1)),
                        }),
                        from: a,
                        direction: (b - a) / length,
                        length,
                        push: (to - from).normalize_or(Vec3::Y) * 0.35 + Vec3::Y * 0.12,
                    });
                }
            }
        }
    }
    pub(in crate::brick_debris) fn step(
        &mut self,
        sections: &mut BTreeMap<Key, Section>,
        _world: &PhysicsWorld,
        hits: &mut u64,
    ) {
        let mut work = 512;
        while work > 0 {
            let Some(mut query) = self.pending.pop_front() else {
                break;
            };
            self.queued.remove(&query.hit);
            work -= 1;
            let (_, key, chunk) = query.hit;
            let Some(section) = sections.get_mut(&key) else {
                continue;
            };
            if section.pieces.is_empty() {
                continue;
            }
            if self.struck.contains(&query.hit) {
                continue;
            }
            match query.cursor.step(&section.index) {
                None => {}
                Some(Some(id)) => {
                    let i = id as usize;
                    let p = &section.pieces[i];
                    if p.active && (chunk == usize::MAX || section.assignment[i] == Some(chunk)) {
                        let pose = Pose::from_parts(
                            Vector::from_array((section.origin + p.offset).to_array()),
                            Rotation::from_array(p.turn.to_array()),
                        );
                        let ray = Ray::new(
                            Vector::from_array(query.from.to_array()),
                            Vector::from_array(query.direction.to_array()),
                        );
                        let shape = SharedShape::cuboid(p.half.x, p.half.y, p.half.z);
                        if shape.cast_ray(&pose, &ray, query.length, true).is_some() {
                            if chunk == usize::MAX {
                                section.recoil =
                                    (section.recoil + query.push).clamp_length_max(0.7);
                            } else if let Some(c) = section.chunks.get_mut(chunk) {
                                c.velocity = (c.velocity + query.push * 8.).clamp_length_max(12.);
                                c.sweep = None;
                            }
                            self.struck.insert(query.hit);
                            *hits += 1;
                            continue;
                        }
                    }
                    self.queued.insert(query.hit);
                    self.pending.push_back(query);
                }
                Some(None) => {
                    self.queued.insert(query.hit);
                    self.pending.push_back(query);
                }
            }
        }
    }
}
