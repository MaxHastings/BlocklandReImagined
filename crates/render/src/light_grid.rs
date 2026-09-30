//! Point lights binned into a world-space grid, so a pixel adds up only the
//! lights whose reach covers its cell instead of every light in the frame.
//! The grid spans the union of the lights' reach, which makes it independent
//! of the view: the player's camera and every mirror's reflected camera read
//! the same grid. A pixel outside it is outside every light.
use glam::Vec3;

use crate::scene::PointLight;

/// Most cells along one axis.
pub const MAX_DIM: u32 = 32;
/// Room for the cell table and the per-cell light lists, in `u32`s.
pub const CAPACITY: usize = (MAX_DIM * MAX_DIM * MAX_DIM) as usize + 4 * 32768;
/// Cells narrower than this cull no better and cost more lists.
const MIN_CELL: f32 = 2.;
/// Low bits of a cell entry holding its light count (up to 256).
pub const COUNT_BITS: u32 = 9;

/// The grid's uniform header and its storage words: the cell table, then
/// each cell's light indices. A cell entry is `offset << COUNT_BITS | count`,
/// `offset` indexing the same word array.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightGrid {
    pub origin: Vec3,
    pub cell: f32,
    pub dims: [u32; 3],
    pub words: Vec<u32>,
}

fn reach(light: &PointLight) -> Option<(Vec3, Vec3)> {
    let [x, y, z, radius] = light.position_radius;
    (radius > 0.).then(|| {
        let p = Vec3::new(x, y, z);
        (p - radius, p + radius)
    })
}

impl LightGrid {
    pub fn build(lights: &[PointLight]) -> Self {
        let reaches: Vec<_> = lights.iter().map(reach).collect();
        let Some((min, max)) = reaches
            .iter()
            .flatten()
            .copied()
            .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
        else {
            return Self::default();
        };
        let extent = max - min;
        // Finest cells that fit the table, coarsened until every light's
        // list fits too.
        let mut cell = (extent.max_element() / MAX_DIM as f32).max(MIN_CELL);
        loop {
            let dims = extent
                .to_array()
                .map(|e| ((e / cell).ceil() as u32).clamp(1, MAX_DIM));
            let span = |lo: Vec3, hi: Vec3| {
                let a = ((lo - min) / cell).floor().as_uvec3();
                let b = ((hi - min) / cell).floor().as_uvec3();
                let top = glam::UVec3::from(dims) - 1;
                (a.min(top), b.min(top))
            };
            let cells = (dims[0] * dims[1] * dims[2]) as usize;
            let listed: usize = reaches
                .iter()
                .flatten()
                .map(|&(lo, hi)| {
                    let (a, b) = span(lo, hi);
                    ((b - a + 1).element_product()) as usize
                })
                .sum();
            if cells + listed > CAPACITY {
                cell *= 1.25;
                continue;
            }
            let index = |x: u32, y: u32, z: u32| (x + dims[0] * (y + dims[1] * z)) as usize;
            let mut counts = vec![0u32; cells];
            for &(lo, hi) in reaches.iter().flatten() {
                let (a, b) = span(lo, hi);
                for z in a.z..=b.z {
                    for y in a.y..=b.y {
                        for x in a.x..=b.x {
                            counts[index(x, y, z)] += 1;
                        }
                    }
                }
            }
            let mut words = vec![0u32; cells + listed];
            let mut next = cells as u32;
            let mut fill = vec![0u32; cells];
            for (i, &count) in counts.iter().enumerate() {
                words[i] = next << COUNT_BITS | count;
                fill[i] = next;
                next += count;
            }
            for (light, reach) in reaches.iter().enumerate() {
                let Some((lo, hi)) = *reach else { continue };
                let (a, b) = span(lo, hi);
                for z in a.z..=b.z {
                    for y in a.y..=b.y {
                        for x in a.x..=b.x {
                            let c = index(x, y, z);
                            words[fill[c] as usize] = light as u32;
                            fill[c] += 1;
                        }
                    }
                }
            }
            return Self {
                origin: min,
                cell,
                dims,
                words,
            };
        }
    }
    /// The longest cell list.
    pub fn most_per_cell(&self) -> u32 {
        let cells = (self.dims[0] * self.dims[1] * self.dims[2]) as usize;
        self.words
            .iter()
            .take(cells)
            .map(|w| w & ((1 << COUNT_BITS) - 1))
            .max()
            .unwrap_or(0)
    }
    /// The lights listed for the cell holding `point` (for tests).
    pub fn lights_at(&self, point: Vec3) -> &[u32] {
        let c = ((point - self.origin) / self.cell).floor();
        if self.words.is_empty()
            || c.min_element() < 0.
            || c.cmpge(glam::UVec3::from(self.dims).as_vec3()).any()
        {
            return &[];
        }
        let c = c.as_uvec3();
        let entry = self.words[(c.x + self.dims[0] * (c.y + self.dims[1] * c.z)) as usize];
        let offset = (entry >> COUNT_BITS) as usize;
        &self.words[offset..offset + (entry & ((1 << COUNT_BITS) - 1)) as usize]
    }
    /// The shader's header: origin and inverse cell size, then dimensions.
    pub fn header(&self, count: usize) -> [u32; 12] {
        let o = self.origin;
        let inverse = if self.words.is_empty() {
            0.
        } else {
            1. / self.cell
        };
        [
            count as u32,
            0,
            0,
            0,
            o.x.to_bits(),
            o.y.to_bits(),
            o.z.to_bits(),
            inverse.to_bits(),
            self.dims[0],
            self.dims[1],
            self.dims[2],
            u32::from(!self.words.is_empty()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(x: f32, y: f32, z: f32, radius: f32) -> PointLight {
        PointLight {
            position_radius: [x, y, z, radius],
            color: [1.; 4],
        }
    }

    #[test]
    fn a_point_lists_every_light_that_reaches_it_and_few_that_do_not() {
        let lights: Vec<_> = (0..256)
            .map(|i| {
                light(
                    (i % 16) as f32 * 20.,
                    (i / 64) as f32 * 5.,
                    (i / 16 % 4) as f32 * 30.,
                    8.,
                )
            })
            .collect();
        let grid = LightGrid::build(&lights);
        assert!(grid.words.len() <= CAPACITY);
        let mut listed = 0;
        let mut checked = 0;
        for step in 0..4000 {
            let p = Vec3::new(
                (step * 37 % 330) as f32 - 10.,
                (step * 13 % 30) as f32 - 5.,
                (step * 53 % 110) as f32 - 10.,
            );
            let near = grid.lights_at(p);
            for (i, l) in lights.iter().enumerate() {
                let [x, y, z, r] = l.position_radius;
                if Vec3::new(x, y, z).distance(p) < r {
                    assert!(near.contains(&(i as u32)), "light {i} missing at {p}");
                }
            }
            listed += near.len();
            checked += 1;
        }
        // Each point pays for a handful of lights, not all 256.
        assert!(
            listed / checked < 16,
            "{} lights per point",
            listed / checked
        );
    }

    #[test]
    fn lights_spread_far_apart_still_fit_the_buffer() {
        let lights: Vec<_> = (0..256)
            .map(|i| light(i as f32 * 400., 0., (i % 7) as f32 * 900., 60.))
            .collect();
        let grid = LightGrid::build(&lights);
        assert!(grid.words.len() <= CAPACITY);
        // Cells this coarse still list only the few lights sharing one.
        let near = grid.lights_at(Vec3::new(400., 0., 900.));
        assert!(near.contains(&1) && near.len() <= 8, "{near:?}");
        assert!(LightGrid::build(&[]).words.is_empty());
        assert!(LightGrid::build(&[light(0., 0., 0., 0.)]).words.is_empty());
    }
}
