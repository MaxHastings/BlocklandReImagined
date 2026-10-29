//! Resumable shape queries for debris without a resident solver body. Uses the
//! same real map/terrain/brick shapes as the collision mirror, without growing
//! its static cache. Translation-only motion deliberately reduces spin, not
//! contact, when the full solver is busy.
use super::*;
use rapier3d::parry::query::{ShapeCastHit, ShapeCastOptions, cast_shapes};

pub(crate) struct DebrisSweep {
    pub from: Vec3,
    pub delta: Vec3,
    pose: Pose,
    shape: SharedShape,
    query: grid::QueryCursor,
    terrain: Vec<TriMesh>,
    map_done: bool,
    pub generation: u64,
    low: Vec3,
    high: Vec3,
    nearest: Option<(f32, Vec3)>,
}
impl DebrisSweep {
    pub(crate) fn bounds(&self) -> (Vec3, Vec3) {
        (self.low, self.high)
    }
    pub(crate) fn test_shape(&mut self, shape: &SharedShape, pose: &Pose) -> Result<()> {
        if let Some(hit) = cast_shapes(
            pose,
            Vector::ZERO,
            shape.as_ref(),
            &self.pose,
            Vector::from_array(self.delta.to_array()),
            self.shape.as_ref(),
            ShapeCastOptions {
                max_time_of_impact: 1.,
                stop_at_penetration: false,
                ..Default::default()
            },
        )
        .map_err(|_| anyhow::anyhow!("Unsupported debris pair sweep"))?
        {
            let normal = Vec3::from_array((pose.rotation * hit.normal1).to_array());
            self.record(hit, normal);
        }
        Ok(())
    }
    pub(crate) fn changed(&self, building: &Building) -> bool {
        building.query_changed_since(self.generation, self.low, self.high)
    }
    fn record(&mut self, hit: ShapeCastHit, normal: Vec3) {
        if self.nearest.is_none_or(|(t, _)| hit.time_of_impact < t) {
            self.nearest = Some((hit.time_of_impact, normal));
        }
    }
}
impl Building {
    pub(crate) fn debris_sweep(
        &self,
        from: Vec3,
        turn: glam::Quat,
        delta: Vec3,
        shape: SharedShape,
    ) -> Result<DebrisSweep> {
        let pose = Pose::from_parts(
            Vector::from_array(from.to_array()),
            Rotation::from_array(turn.to_array()),
        );
        let aabb = shape.compute_aabb(&pose);
        let low = Vec3::from_array(aabb.mins.to_array()) + delta.min(Vec3::ZERO);
        let high = Vec3::from_array(aabb.maxs.to_array()) + delta.max(Vec3::ZERO);
        Ok(DebrisSweep {
            from,
            delta,
            pose,
            shape,
            query: grid::QueryCursor::new(query_bounds(low, high)),
            terrain: self.terrain_patches(low, high)?,
            map_done: false,
            generation: self.query_generation(),
            low,
            high,
            nearest: None,
        })
    }
    pub(crate) fn step_debris_sweep(
        &self,
        sweep: &mut DebrisSweep,
        work: &mut u32,
        skip: impl Fn(BrickId) -> bool,
    ) -> Result<Option<Option<(f32, Vec3)>>> {
        let cost = crate::local_physics::COLLISION_QUERY_COST;
        let velocity = Vector::from_array(sweep.delta.to_array());
        let options = ShapeCastOptions {
            max_time_of_impact: 1.,
            stop_at_penetration: false,
            ..Default::default()
        };
        while *work >= cost {
            if !sweep.map_done {
                *work -= cost;
                if let Some((_, hit)) = self
                    .map
                    .query_pipeline_with_filter(QueryFilter::default().exclude_sensors())
                    .cast_shape(&sweep.pose, velocity, sweep.shape.as_ref(), options)
                {
                    let normal = Vec3::from_array(hit.normal1.to_array());
                    sweep.record(hit, normal);
                }
                sweep.map_done = true;
                continue;
            }
            if let Some(patch) = sweep.terrain.pop() {
                *work -= cost;
                if let Some(hit) = cast_shapes(
                    &Pose::IDENTITY,
                    Vector::ZERO,
                    &patch,
                    &sweep.pose,
                    velocity,
                    sweep.shape.as_ref(),
                    options,
                )
                .map_err(|_| anyhow::anyhow!("Unsupported debris terrain sweep"))?
                {
                    let normal = Vec3::from_array(hit.normal1.to_array());
                    sweep.record(hit, normal);
                }
                continue;
            }
            *work -= 1;
            match self.section_solid_step(&mut sweep.query) {
                None => return Ok(Some(sweep.nearest)),
                Some(Some((id, shape, pose))) if !skip(id) && !self.bricks[&id].damage_hidden => {
                    *work -= cost - 1;
                    if let Some(hit) = cast_shapes(
                        &pose,
                        Vector::ZERO,
                        shape.as_ref(),
                        &sweep.pose,
                        velocity,
                        sweep.shape.as_ref(),
                        options,
                    )
                    .map_err(|_| anyhow::anyhow!("Unsupported debris brick sweep {id}"))?
                    {
                        let normal = Vec3::from_array((pose.rotation * hit.normal1).to_array());
                        sweep.record(hit, normal);
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }
}
