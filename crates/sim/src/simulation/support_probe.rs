//! Resumable support proof: every footprint ray, bucket candidate and
//! authored attachment cell consumes work. No eager neighbor collection.
use super::*;
use crate::grid::{ConnectionCells, QueryCursor};

pub(crate) struct SupportProbe {
    pub area: Bounds,
    id: BrickId,
    ground_cell: i64,
    pub grounded: bool,
    query: QueryCursor,
    cells: Option<(BrickId, ConnectionCells)>,
    pub neighbors: Vec<BrickId>,
    pub below: bool,
    pub exhausted: bool,
}
impl SupportProbe {
    pub(crate) fn new(sim: &Simulation, id: BrickId) -> Self {
        let bounds = sim.index.bounds(id);
        Self {
            id,
            area: bounds.expanded(1),
            ground_cell: 0,
            grounded: false,
            query: QueryCursor::new(bounds.expanded(1)),
            cells: None,
            neighbors: Vec::new(),
            below: false,
            exhausted: false,
        }
    }
    pub(crate) fn step(
        &mut self,
        sim: &Simulation,
        left: &mut u32,
        live: impl Fn(BrickId) -> bool,
    ) -> Result<bool> {
        let Some(own) = sim.state().bricks.get(&self.id) else {
            return Ok(true);
        };
        let bounds = sim.index.bounds(self.id);
        let mesh = &sim.definitions.get(own)?.mesh;
        let total = i64::from(bounds.size[0]) * i64::from(bounds.size[2]);
        while *left > 0 {
            if self.neighbors.len() > crate::presentation::MAX_SECTION_MEMBERS {
                self.exhausted = true;
                return Ok(true);
            }
            if !self.grounded && self.ground_cell < total {
                if *left < 2 {
                    return Ok(false);
                }
                *left -= 2;
                let cell = self.ground_cell;
                self.ground_cell += 1;
                let small = Bounds {
                    min: [
                        bounds.min[0] + (cell % i64::from(bounds.size[0])) as i32,
                        bounds.min[1],
                        bounds.min[2] + (cell / i64::from(bounds.size[0])) as i32,
                    ],
                    size: [1, bounds.size[1], 1],
                };
                self.grounded = on_ground(&sim.physics, sim.terrain.as_ref(), small);

                continue;
            }
            *left -= 1;
            if let Some((other, cells)) = &mut self.cells {
                if let Some(b) = sim.state().bricks.get(other) {
                    let other_mesh = &sim.definitions.get(b)?.mesh;
                    if let Some((_, _, attached)) =
                        cells.next(own.quarter_turns, mesh, b.quarter_turns, other_mesh)
                    {
                        if attached {
                            self.below |= sim.index.bounds(*other).max()[1] <= bounds.min[1];
                            self.neighbors.push(*other);
                            self.cells = None;
                        }
                        continue;
                    }
                }
                self.cells = None;
                continue;
            }
            // A coarse bucket holds many bricks unrelated to these attachment
            // faces. Batch at most sixteen cheap bounds rejects per credit;
            // actual ground rays, attachment cells and candidates keep their
            // original quota. This remains a bounded cursor, not an eager scan.
            let mut next = Some(None);
            for _ in 0..16 {
                next = self.query.step(&sim.index);
                match next {
                    Some(Some(other)) if other != self.id => {
                        let other_bounds = sim.index.bounds(other);
                        // Connection cells may be recessed inside overlapping
                        // mesh bounds (ramps/interlocking custom bricks).
                        let vertical = bounds.min[1] < other_bounds.max()[1] + 1
                            && other_bounds.min[1] < bounds.max()[1] + 1
                            && [0, 2].into_iter().all(|a| {
                                bounds.min[a] < other_bounds.max()[a]
                                    && other_bounds.min[a] < bounds.max()[a]
                            });
                        let family_face = sim.damage_parent(self.id).is_some()
                            && sim.damage_parent(self.id) == sim.damage_parent(other)
                            && grid::share_face(bounds, other_bounds);
                        if (vertical || family_face) && live(other) {
                            break;
                        }
                        next = Some(None);
                    }
                    None => break,
                    _ => {
                        next = Some(None);
                    }
                }
            }
            match next {
                None => return Ok(true),
                Some(Some(other)) if other != self.id && live(other) => {
                    let other_bounds = sim.index.bounds(other);
                    if sim.damage_parent(self.id).is_some()
                        && sim.damage_parent(self.id) == sim.damage_parent(other)
                        && grid::share_face(bounds, other_bounds)
                    {
                        self.neighbors.push(other);
                        self.below |= other_bounds.max()[1] <= bounds.min[1];
                    } else {
                        self.cells = Some((other, ConnectionCells::new(bounds, other_bounds)));
                    }
                }
                _ => {}
            }
        }
        Ok(false)
    }
}
impl Simulation {
    pub(super) fn note_support_at(&mut self, id: BrickId) {
        if let Some(b) = self.state().bricks.get(&id)
            && let Ok(d) = self.definitions.get(b)
            && let Ok(bounds) = Bounds::new(b, &d.mesh)
        {
            self.support_edits.push(bounds);
            // Conservative bounded invalidation summary. Precise individual
            // edits are useful until a bulk operation exceeds this envelope.
            if self.support_edits.len() > 32 {
                let mut area = self.support_edits[0];
                for b in &self.support_edits[1..] {
                    let min = std::array::from_fn(|i| area.min[i].min(b.min[i]));
                    let max: [i32; 3] = std::array::from_fn(|i| area.max()[i].max(b.max()[i]));
                    area = Bounds {
                        min,
                        size: std::array::from_fn(|i| max[i] - min[i]),
                    };
                }
                self.support_edits.clear();
                self.support_edits.push(area);
            }
        }
    }
    pub(crate) fn take_support_edits(&mut self) -> (Vec<Bounds>, bool) {
        (
            std::mem::take(&mut self.support_edits),
            std::mem::take(&mut self.support_map_edit),
        )
    }
}
