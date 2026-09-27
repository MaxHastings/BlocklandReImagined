use crate::{
    definitions::{Definition, Definitions},
    grid::{self, Bounds, Index},
};
use anyhow::{Context, Result, ensure};
use bri_world::{
    Brick, BrickId, Input, World,
    authority::{Actor, Authority, Edit},
};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::BTreeMap;
// Brick IDs occupy u64; zero remains available for untagged dynamic bodies.
const MAP_TAG: u128 = u128::MAX;
pub struct Builder<'a> {
    pub actor: &'a Actor,
    pub position: Vec3,
    pub reach: f32,
}
#[derive(Debug, Clone)]
pub struct Hit {
    pub brick: Option<BrickId>,
    pub position: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}
pub struct Simulation {
    authority: Authority,
    pub definitions: Definitions,
    pub physics: PhysicsWorld,
    pub waters: Vec<bri_content::water::Water>,
    index: Index,
    handles: BTreeMap<BrickId, ColliderHandle>,
}
fn pose(brick: &Brick) -> Pose {
    Pose::from_parts(
        Vector::from_array(brick.position),
        Rotation::from_scaled_axis(
            Vector::Y * (-f32::from(brick.quarter_turns) * std::f32::consts::FRAC_PI_2),
        ),
    )
}
fn collider(brick: &Brick, definition: &Definition, id: BrickId) -> ColliderBuilder {
    ColliderBuilder::new(definition.shape.clone())
        .position(pose(brick))
        .sensor(!brick.colliding)
        .user_data(u128::from(id))
}
fn may_build_on(actor: &Actor, brick: &Brick) -> bool {
    actor.administrator || brick.owner == 0 || brick.owner == actor.owner
}
impl Simulation {
    pub fn new(world: World, definitions: Definitions, map: Vec<ColliderBuilder>) -> Result<Self> {
        world.validate()?;
        let mut physics = bri_physics::new_world();
        for c in map {
            physics.insert_collider(c.user_data(MAP_TAG), None);
        }
        let mut index = Index::default();
        let mut handles = BTreeMap::new();
        for (id, brick) in &world.bricks {
            let definition = definitions.get(brick)?;
            let bounds = Bounds::new(brick, &definition.mesh)?;
            index.insert(*id, bounds);
            handles.insert(
                *id,
                physics.insert_collider(collider(brick, definition, *id), None),
            );
        }
        physics.detect_collisions(&(), &());
        Ok(Self {
            authority: Authority::new(world)?,
            definitions,
            physics,
            waters: Vec::new(),
            index,
            handles,
        })
    }
    pub fn state(&self) -> &World {
        self.authority.state()
    }
    pub fn load_build(
        &mut self,
        actor: &Actor,
        plan: bri_world::build::LoadPlan,
    ) -> Result<Vec<BrickId>> {
        ensure!(
            actor.administrator,
            "Only the host/administrator may load builds"
        );
        // Preflight the whole build, then publish state/index/colliders together.
        // Existing players, bodies, bricks and scheduled events stay alive.
        let mut prepared = Vec::new();
        for (id, brick) in plan.bricks() {
            let definition = self.definitions.get(brick)?;
            prepared.push((
                *id,
                Bounds::new(brick, &definition.mesh)?,
                collider(brick, definition, *id),
            ));
        }
        let ids = self.authority.load_build(actor, plan)?;
        for (id, bounds, collider) in prepared {
            self.index.insert(id, bounds);
            self.handles
                .insert(id, self.physics.insert_collider(collider, None));
        }
        self.physics.detect_collisions(&(), &());
        Ok(ids)
    }
    pub fn plant(&mut self, builder: &Builder<'_>, brick: Brick) -> Result<BrickId> {
        let definition = self.definitions.get(&brick)?;
        let bounds = Bounds::new(&brick, &definition.mesh)?;
        let defs = &self.definitions;
        let index = &self.index;
        let physics = &self.physics;
        let id = self.authority.plant(builder.actor, brick, |world, brick| {
            validate_placement(world, defs, index, physics, builder, brick)
        })?;
        let brick = &self.authority.state().bricks[&id];
        self.handles.insert(
            id,
            self.physics
                .insert_collider(collider(brick, definition, id), None),
        );
        self.index.insert(id, bounds);
        self.physics.detect_collisions(&(), &());
        Ok(id)
    }
    pub fn edit(&mut self, actor: &Actor, id: BrickId, edit: Edit) -> Result<()> {
        self.authority.edit(actor, id, edit)?;
        self.sync_flags(id);
        self.physics.detect_collisions(&(), &());
        Ok(())
    }
    pub fn remove(&mut self, actor: &Actor, id: BrickId) -> Result<()> {
        let brick = self.state().bricks.get(&id).context("Unknown brick")?;
        ensure!(
            !self.definitions.get(brick)?.indestructible || actor.administrator,
            "Brick is indestructible"
        );
        self.authority.remove(actor, id)?;
        self.index.remove(id);
        if let Some(handle) = self.handles.remove(&id) {
            self.physics.remove_collider(handle);
        }
        self.physics.detect_collisions(&(), &());
        Ok(())
    }
    fn sync_flags(&mut self, id: BrickId) {
        if let Some(handle) = self.handles.get(&id) {
            self.physics.colliders[*handle]
                .set_sensor(!self.authority.state().bricks[&id].colliding);
        }
    }
    pub fn step(&mut self) -> Result<Vec<BrickId>> {
        let changed = self.authority.step()?;
        for id in &changed {
            self.sync_flags(*id);
        }
        self.physics.step();
        Ok(changed)
    }
    /// Eye and direction are from the server's player state, not packet positions.
    pub fn activate(&mut self, eye: Vec3, direction: Vec3) -> Result<Option<BrickId>> {
        let Some(hit) = self.target(eye, direction, 5.0)? else {
            return Ok(None);
        };
        let Some(id) = hit.brick else { return Ok(None) };
        self.authority.trigger(id, Input::Activate)?;
        Ok(Some(id))
    }
    /// Call on contact entry from the authoritative player collision controller.
    pub fn touch(&mut self, id: BrickId) -> Result<usize> {
        self.authority.trigger(id, Input::Touch)
    }
    pub fn target(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Result<Option<Hit>> {
        self.target_filtered(origin, direction, max_distance, false)
    }
    /// Stock editing tools use FxBrickAlwaysObjectType, including bricks whose
    /// ordinary raycasting flag is disabled. Map geometry still obstructs tools.
    pub fn target_bricks_always(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Result<Option<Hit>> {
        self.target_filtered(origin, direction, max_distance, true)
    }
    fn target_filtered(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        all_bricks: bool,
    ) -> Result<Option<Hit>> {
        ensure!(
            origin.is_finite()
                && origin.abs().max_element() <= 1_000_000.0
                && direction.is_finite()
                && direction.length_squared().is_finite()
                && direction.length_squared() > 0.1
                && max_distance.is_finite()
                && max_distance > 0.0
                && max_distance <= 150.0,
            "Invalid targeting ray"
        );
        let direction = direction.normalize();
        let ray = Ray::new(
            Vector::from_array(origin.to_array()),
            Vector::from_array(direction.to_array()),
        );
        let filter = |_: ColliderHandle, c: &Collider| c.user_data == MAP_TAG;
        let mut nearest = self
            .physics
            .query_pipeline_with_filter(QueryFilter::default().predicate(&filter))
            .cast_ray_and_get_normal(&ray, max_distance, true)
            .map(|(_, hit)| Hit {
                brick: None,
                position: origin + direction * hit.time_of_impact,
                normal: Vec3::from(hit.normal.to_array()),
                distance: hit.time_of_impact,
            });
        let end = origin + direction * max_distance;
        let low = origin.min(end) - Vec3::splat(0.01);
        let high = origin.max(end) + Vec3::splat(0.01);
        let min = std::array::from_fn(|a| (low[a] / grid::CELL[a]).floor() as i32);
        let max: [i32; 3] = std::array::from_fn(|a| (high[a] / grid::CELL[a]).ceil() as i32);
        let bounds = Bounds {
            min,
            size: std::array::from_fn(|a| (max[a] - min[a]).max(1)),
        };
        for id in self.index.query(bounds) {
            let brick = &self.state().bricks[&id];
            if !all_bricks && !brick.raycast {
                continue;
            }
            let d = self.definitions.get(brick)?;
            let inverse = brick.transform().inverse();
            let o = inverse.transform_point3(origin);
            let dir = inverse.transform_vector3(direction);
            if let Some((distance, normal)) = bri_physics::content::raycast(
                &d.collision,
                Vector::from_array(o.to_array()),
                Vector::from_array(dir.to_array()),
                max_distance,
            ) && nearest.as_ref().is_none_or(|hit| distance < hit.distance)
            {
                nearest = Some(Hit {
                    brick: Some(id),
                    position: origin + direction * distance,
                    normal: brick
                        .transform()
                        .transform_vector3(Vec3::from(normal.to_array())),
                    distance,
                });
            }
        }
        Ok(nearest)
    }
}
fn validate_placement(
    world: &World,
    defs: &Definitions,
    index: &Index,
    physics: &PhysicsWorld,
    builder: &Builder<'_>,
    brick: &Brick,
) -> Result<()> {
    ensure!(
        builder.position.is_finite()
            && builder.reach.is_finite()
            && (0.0..=100.0).contains(&builder.reach),
        "Invalid builder position/reach"
    );
    let definition = defs.get(brick)?;
    ensure!(
        !definition.requires_behavior_adapter,
        "This brick requires a native behavior adapter"
    );
    let bounds = Bounds::new(brick, &definition.mesh)?;
    let radius = *definition.mesh.footprint_studs.iter().max().unwrap() as f32 * 0.25;
    ensure!(
        builder.position.distance(Vec3::from(brick.position)) <= builder.reach + radius,
        "Brick is too far away"
    );
    let mut supported = false;
    for id in index.query(bounds.expanded(1)) {
        let existing = &world.bricks[&id];
        let other = defs.get(existing)?;
        let ob = index.bounds(id);
        ensure!(
            !grid::overlaps(
                (brick, &definition.mesh, bounds),
                (existing, &other.mesh, ob)
            ),
            "Brick overlaps authored occupied cells"
        );
        if grid::connected(
            (brick, &definition.mesh, bounds),
            (existing, &other.mesh, ob),
        ) {
            ensure!(
                may_build_on(builder.actor, existing),
                "No build permission on supporting brick"
            );
            supported = true;
        }
    }
    let placement = pose(brick);
    // Some authored hulls extend below their logical build grid (the stock pine
    // tree by 0.014544 units). Preserve that hull for physics, but allow precisely
    // its below-grid extent at an upward-facing map surface. Do not inflate a
    // general penetration tolerance or apply this allowance to moving entities.
    let local_bottom = definition.shape.compute_local_aabb().mins.y;
    let authored_below_grid =
        (-(definition.mesh.height_plates as f32) * 0.1 - local_bottom).max(0.0);
    let aabb = definition.shape.compute_aabb(&placement);
    let query = physics.query_pipeline();
    for (_, obstacle) in query.intersect_aabb_conservative(aabb).filter(|(_, c)| {
        c.user_data == MAP_TAG
            || (!c.is_sensor() && c.parent().is_some_and(|p| !physics.bodies[p].is_fixed()))
    }) {
        if let Some(contact) = rapier3d::parry::query::contact(
            &placement,
            definition.shape.as_ref(),
            obstacle.position(),
            obstacle.shape(),
            0.0,
        )
        .map_err(|_| anyhow::anyhow!("Unsupported obstacle/brick collision pair"))?
        {
            let allowance = if obstacle.user_data == MAP_TAG && contact.normal2.y > 0.7 {
                authored_below_grid
            } else {
                0.0
            };
            ensure!(
                contact.dist >= -0.002 - allowance,
                "Brick is embedded in map geometry or an entity (penetration {})",
                -contact.dist
            );
        }
    }
    if !supported {
        let map_filter = |_: ColliderHandle, c: &Collider| c.user_data == MAP_TAG;
        let query =
            physics.query_pipeline_with_filter(QueryFilter::default().predicate(&map_filter));
        'support: for z in bounds.min[2]..bounds.max()[2] {
            for x in bounds.min[0]..bounds.max()[0] {
                for y in bounds.min[1]..bounds.max()[1] {
                    if !b"bd".contains(&bounds.cell(
                        [x, y, z],
                        brick.quarter_turns,
                        &definition.mesh,
                    )) {
                        continue;
                    }
                    let origin = Vector::new(
                        (x as f32 + 0.5) * 0.5,
                        y as f32 * 0.2 + 0.002,
                        (z as f32 + 0.5) * 0.5,
                    );
                    if query
                        .cast_ray_and_get_normal(&Ray::new(origin, -Vector::Y), 0.205, true)
                        .is_some_and(|(_, h)| h.normal.y > 0.5)
                    {
                        supported = true;
                        break 'support;
                    }
                }
            }
        }
    }
    ensure!(
        supported,
        "Brick is floating: no stud connection or map support"
    );
    Ok(())
}
