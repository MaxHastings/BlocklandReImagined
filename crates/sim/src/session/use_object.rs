//! The Use key on things that are not bricks: a dropped item or an Add-On
//! creature the eye points at, within the same reach as a brick. The engine
//! gives them no meaning; a rule's `on_use_object` decides (a medic picking
//! up a casualty, a soldier searching a body, a crate that opens).
use super::*;
use bri_package_runtime::Dynamic;
use bri_package_runtime::rhai::Map;
use bri_package_runtime::script::{RayTarget, World};
use bri_weapons::ItemBounds;

impl Session {
    /// What the Use ray meets first of the world's loose objects within
    /// `reach` of `eye`, nearer than `limit` if given: `#{ kind, id, ref,
    /// distance }` for a `drop` or an `entity`, with a drop's `item` too.
    fn use_object(
        &self,
        owner: OwnerId,
        eye: Vec3,
        direction: Vec3,
        reach: f32,
        limit: Option<f32>,
    ) -> Option<Map> {
        let direction = direction.normalize_or_zero();
        if direction == Vec3::ZERO {
            return None;
        }
        let reach = limit.map_or(reach, |l| reach.min(l));
        let mut best: Option<(f32, Map)> = None;
        for drop in self.weapons.drops() {
            let bounds = self
                .item_spawners
                .bounds
                .get(&drop.item)
                .copied()
                .unwrap_or(ItemBounds::FALLBACK);
            let scaled = ItemBounds {
                min: (Vec3::from(bounds.min) * drop.scale).to_array(),
                max: (Vec3::from(bounds.max) * drop.scale).to_array(),
            }
            .transformed(drop.position, drop.rotation);
            let Some(distance) = ray_box(eye, direction, scaled.min.into(), scaled.max.into())
            else {
                continue;
            };
            if distance <= reach && best.as_ref().is_none_or(|(d, _)| distance < *d) {
                let mut map = Map::new();
                map.insert("kind".into(), "drop".into());
                map.insert("id".into(), Dynamic::from_int(drop.id as i64));
                map.insert("ref".into(), format!("drop:{}", drop.id).into());
                map.insert("item".into(), drop.item.clone().into());
                best = Some((distance, map));
            }
        }
        let world = script_world::ScriptWorld::new(self, "");
        if let Some(hit) = world.raycast(eye.to_array(), direction.to_array(), reach, Some(owner))
            && let RayTarget::Object(object @ bri_package_runtime::ops::ObjectRef::Entity(_)) =
                hit.target
            && best.as_ref().is_none_or(|(d, _)| hit.distance < *d)
        {
            let mut map = Map::new();
            map.insert("kind".into(), "entity".into());
            map.insert("id".into(), Dynamic::from_int(object.id() as i64));
            map.insert("ref".into(), object.to_string().into());
            best = Some((hit.distance, map));
        }
        let (distance, mut map) = best?;
        map.insert("distance".into(), Dynamic::from_float(f64::from(distance)));
        Some(map)
    }

    /// Offer the Use click to `on_use_object` when it points at a loose
    /// object nearer than any brick it would activate. True when a rule
    /// took it.
    pub(super) fn package_use_object(
        &mut self,
        owner: OwnerId,
        eye: Vec3,
        direction: Vec3,
        reach: f32,
        brick_distance: Option<f32>,
    ) -> bool {
        let declared = self
            .packages
            .as_ref()
            .is_some_and(|host| host.catalog.behaviours().any(|(_, b)| b.on_use_object));
        if !declared {
            return false;
        }
        let Some(target) = self.use_object(owner, eye, direction, reach, brick_distance) else {
            return false;
        };
        self.package_take(
            owner,
            |b| b.on_use_object,
            "on_use_object",
            vec![Dynamic::from_int(owner as i64), Dynamic::from_map(target)],
        )
    }
}

/// Where a ray from `origin` along the unit `direction` first meets the box
/// `min..max`, as a distance; None when it misses or the box is behind.
fn ray_box(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inverse = direction.recip();
    let (a, b) = ((min - origin) * inverse, (max - origin) * inverse);
    let near = a.min(b).max_element();
    let far = a.max(b).min_element();
    (near <= far && far >= 0.0).then_some(near.max(0.0))
}
