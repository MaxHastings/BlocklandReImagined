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
            match self.query.step(&sim.index) {
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
        }
    }
    pub(crate) fn take_support_edits(&mut self) -> (Vec<Bounds>, bool) {
        (
            std::mem::take(&mut self.support_edits),
            std::mem::take(&mut self.support_map_edit),
        )
    }
}
