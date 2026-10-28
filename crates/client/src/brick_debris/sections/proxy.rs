//! Merge actual member boxes only across completely shared rectangular faces.
//! The union preserves holes; a sparse assembly never becomes a solid cube.
use super::*;
use bri_sim::grid::{Bounds, CELL};
const PARTS_PER_CHUNK: usize = 64;

pub(super) fn shape(
    pieces: &[SectionPiece],
    members: &[usize],
    center: Vec3,
) -> (Option<SharedShape>, usize) {
    let mut boxes: Vec<_> = members
        .iter()
        .map(|&i| partition::bounds(&pieces[i]))
        .collect();
    // Two passes let a row join a plane and a plane join a solid box. Each
    // merge is exact, with sorting rather than a quadratic all-pairs scan.
    for _ in 0..2 {
        for axis in 0..3 {
            let other: Vec<_> = (0..3).filter(|&a| a != axis).collect();
            let key = |b: &Bounds| {
                [
                    b.min[other[0]],
                    b.size[other[0]],
                    b.min[other[1]],
                    b.size[other[1]],
                ]
            };
            boxes.sort_unstable_by_key(|b| (key(b), b.min[axis]));
            let mut merged: Vec<Bounds> = Vec::with_capacity(boxes.len());
            for b in boxes {
                if let Some(last) = merged.last_mut()
                    && key(last) == key(&b)
                    && last.max()[axis] >= b.min[axis]
                {
                    last.size[axis] = last.max()[axis].max(b.max()[axis]) - last.min[axis];
                    continue;
                }
                merged.push(b);
            }
            boxes = merged;
        }
    }
    let count = boxes.len();
    if count > PARTS_PER_CHUNK {
        return (None, count);
    }
    let parts = boxes
        .into_iter()
        .map(|b| {
            let low = Vec3::from_array(std::array::from_fn(|a| b.min[a] as f32 * CELL[a]));
            let half = Vec3::from_array(std::array::from_fn(|a| b.size[a] as f32 * CELL[a] * 0.5));
            let at = low + half - center;
            let h = (half * BODY_SHRINK).max(Vec3::splat(0.01));
            (
                Pose::translation(at.x, at.y, at.z),
                SharedShape::cuboid(h.x, h.y, h.z),
            )
        })
        .collect();
    (Some(SharedShape::compound(parts)), count)
}
