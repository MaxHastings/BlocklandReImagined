//! The live world package scripts ask during a call (`raycast`,
//! `can_damage`, `brick_box`), read through the same weapon sweep and damage policy the
//! engine's own weapons use.
use super::*;
use bri_package_runtime::ops::ObjectRef;
use bri_package_runtime::script::{RayHit, RayTarget, World};
use bri_weapons::{Filter, Query, TargetId};
use std::cell::OnceCell;

pub(super) struct ScriptWorld<'a> {
    pub(super) session: &'a Session,
    /// The Tutorial's moving target shapes, gathered on the first ray.
    shapes: OnceCell<Vec<crate::weapon_query::ShapeTarget>>,
}
impl<'a> ScriptWorld<'a> {
    pub(super) fn new(session: &'a Session) -> Self {
        Self {
            session,
            shapes: OnceCell::new(),
        }
    }
}
impl World for ScriptWorld<'_> {
    fn raycast(
        &self,
        from: [f32; 3],
        direction: [f32; 3],
        range: f32,
        ignore: Option<u64>,
    ) -> Option<RayHit> {
        let session = self.session;
        let shapes = self.shapes.get_or_init(|| session.tutorial_shape_targets());
        let never = |_: bri_weapons::ActorId, _: TargetId| false;
        let never_catch = |_: bri_weapons::ActorId, _: bri_weapons::ActorId| false;
        let mut query = crate::weapon_query::WeaponQuery {
            simulation: &session.simulation,
            affect: &never,
            affect_radius: &never,
            catch: &never_catch,
            responses: &session.events.projectile_responses,
            truncated_targets: 0,
            shapes,
        };
        let from = Vec3::from(from);
        let direction = Vec3::from(direction);
        let hit = query.sweep(
            from,
            from + direction * range,
            Filter {
                projectile_age_ticks: None,
                // No player has id 0, so `None` passes through nobody.
                source: bri_weapons::ActorId(ignore.unwrap_or(0)),
                players: true,
                world_only: false,
            },
        )?;
        let target = match hit.target {
            TargetId::Actor(actor) => RayTarget::Object(ObjectRef::Player(actor.0)),
            TargetId::Vehicle(vehicle) => RayTarget::Object(ObjectRef::Vehicle(vehicle)),
            TargetId::Entity(entity) => RayTarget::Object(ObjectRef::Entity(entity)),
            TargetId::Brick(brick) => RayTarget::Brick(brick),
            TargetId::Map(_) | TargetId::Shape(_) => RayTarget::Map,
        };
        Some(RayHit {
            target,
            position: hit.position.to_array(),
            normal: hit.normal.to_array(),
            distance: hit.fraction * range,
        })
    }
    fn can_damage(&self, by: u64, target: ObjectRef) -> bool {
        let session = self.session;
        match target {
            ObjectRef::Player(player) => session.can_damage_player(by, player, false),
            ObjectRef::Vehicle(vehicle) => session.damage_policy().vehicle(
                by,
                session.vehicle_owner_and_mass(vehicle).map(|(owner, _)| owner),
            ),
            // As shots: a player never hurts the creature they drive.
            ObjectRef::Entity(entity) => {
                session
                    .packages
                    .as_ref()
                    .is_some_and(|h| h.entities.contains_key(&entity))
                    && session
                        .peers
                        .get(&by)
                        .is_none_or(|p| p.control != ControlObject::Entity(entity))
            }
        }
    }
    fn voxel(&self, brick: u64) -> Option<([i64; 3], String)> {
        self.session.package_voxel(brick)
    }
    fn can_place_voxel(&self, position: [i64; 3]) -> bool {
        self.session.voxel_fits(position)
    }
    fn brick_box(&self, brick: u64) -> Option<([f32; 3], [f32; 3])> {
        let (min, max) = self.session.simulation.brick_box(brick)?;
        Some((min.to_array(), max.to_array()))
    }
}
