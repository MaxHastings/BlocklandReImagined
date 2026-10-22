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
    /// The package whose call this is.
    package: &'a str,
    /// The Tutorial's moving target shapes, gathered on the first ray.
    shapes: OnceCell<Vec<crate::weapon_query::ShapeTarget>>,
}
impl<'a> ScriptWorld<'a> {
    pub(super) fn new(session: &'a Session, package: &'a str) -> Self {
        Self {
            session,
            package,
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
            ally: &never,
            catch: &never_catch,
            responses: &session.events.projectile_responses,
            truncated_targets: 0,
            shapes,
            rewound: None,
        };
        let from = Vec3::from(from);
        let direction = Vec3::from(direction).normalize_or_zero();
        let filter = Filter {
            projectile_age_ticks: None,
            // No player has id 0, so `None` passes through nobody.
            source: bri_weapons::ActorId(ignore.unwrap_or(0)),
            players: true,
            world_only: false,
        };
        // On through portals, as players and shots see.
        let (hit, leg) = session
            .simulation
            .passages()
            .cast(from, direction, range, |leg| {
                Ok::<_, std::convert::Infallible>(query.sweep(
                    leg.from,
                    leg.from + leg.direction * leg.length,
                    filter,
                ))
            })
            .ok()??;
        let target = match hit.target {
            TargetId::Actor(actor) => RayTarget::Object(ObjectRef::Player(actor.0)),
            TargetId::Vehicle(vehicle) => RayTarget::Object(ObjectRef::Vehicle(vehicle)),
            TargetId::Entity(entity) => RayTarget::Object(ObjectRef::Entity(entity)),
            TargetId::Brick(brick) => RayTarget::Brick(brick),
            // A mover's collision is the map's to a shot; to a script it is
            // the mover.
            TargetId::Map(_) => session
                .mover_at(hit.position)
                .map_or(RayTarget::Map, |id| RayTarget::Mover(id.to_owned())),
            TargetId::Shape(_) => RayTarget::Map,
        };
        let region = match hit.target {
            TargetId::Actor(actor) => session.region_of(actor.0, hit.position),
            _ => None,
        };
        Some(RayHit {
            target,
            position: hit.position.to_array(),
            normal: hit.normal.to_array(),
            distance: leg.start + hit.fraction * leg.length,
            region,
        })
    }
    fn enabled(&self, id: &str) -> bool {
        self.session
            .packages
            .as_ref()
            .is_some_and(|host| host.catalog.enabled(id))
    }
    fn lan(&self) -> bool {
        self.session.lan_host
    }
    fn addon_state(&self, package: &str) -> Option<bri_package_runtime::state::Namespace> {
        self.session
            .packages
            .as_ref()
            .and_then(|host| host.store.namespace(package).cloned())
    }
    fn addon_vars(&self, package: &str) -> std::sync::Arc<bri_package_runtime::script::EntityVars> {
        let shared = self
            .session
            .packages
            .as_ref()
            .and_then(|h| h.view.as_ref())
            .and_then(|v| v.vars.get(package).cloned());
        shared.unwrap_or_else(|| std::sync::Arc::new(self.session.package_vars(package)))
    }
    fn hit_region(&self, player: u64, point: [f32; 3]) -> Option<&'static str> {
        self.session.region_of(player, Vec3::from(point))
    }
    fn can_damage(&self, by: u64, target: ObjectRef) -> bool {
        let session = self.session;
        match target {
            ObjectRef::Player(player) => session.can_damage_player(by, player, false),
            ObjectRef::Vehicle(vehicle) => session.damage_policy().vehicle(
                by,
                session
                    .vehicle_owner_and_mass(vehicle)
                    .map(|(owner, _)| owner),
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
    fn bricks_of(&self, kind: &str, limit: usize) -> Vec<bri_package_runtime::script::BrickView> {
        self.session
            .simulation
            .bricks_of(kind)
            .take(limit)
            .filter_map(|id| self.session.brick_view(id))
            .collect()
    }
    fn brick(&self, brick: u64) -> Option<bri_package_runtime::script::BrickView> {
        self.session.brick_view(brick)
    }
    fn brick_field(&self, brick: u64, key: &str) -> Option<serde_json::Value> {
        self.session.brick_field(self.package, brick, key)
    }
    fn avatar_choices(&self) -> BTreeMap<String, Vec<String>> {
        let Some(pack) = self.session.avatar_catalog.as_ref() else {
            return BTreeMap::new();
        };
        let mut out = pack.parts.clone();
        out.insert("face".into(), pack.faces.clone());
        out.insert("decal".into(), pack.decals.clone());
        for (hat, accents) in &pack.accents_allowed {
            out.insert(format!("accents.{hat}"), accents.clone());
        }
        out
    }
    fn palette(&self) -> Vec<[f32; 4]> {
        self.session.simulation.state().palette.clone()
    }
    fn movers(&self) -> Vec<bri_package_runtime::script::MoverInfo> {
        self.session
            .mover_views()
            .into_iter()
            .map(|m| bri_package_runtime::script::MoverInfo {
                id: m.id,
                key: m.key,
                from: m.from,
                to: m.to,
                moving: m.moving,
                blocked: m.blocked,
                progress: m.progress,
                keys: m.keys,
                position: m.position,
                rest: m.rest,
                params: m.params,
            })
            .collect()
    }
    fn volumes_at(&self, at: [f32; 3]) -> Vec<bri_package_runtime::script::VolumeInfo> {
        self.session
            .volumes_at(at)
            .into_iter()
            .map(|v| bri_package_runtime::script::VolumeInfo {
                id: v.id.clone(),
                priority: v.priority,
                properties: v.properties.clone(),
            })
            .collect()
    }
    fn movers_near(&self, player: u64, reach: f32) -> Vec<String> {
        self.session.movers_near_player(player, reach)
    }
    fn drops(&self) -> Vec<bri_package_runtime::script::DropView> {
        self.session.package_drop_views(self.package)
    }
    fn setting(
        &self,
        game: Option<u64>,
        team: Option<u64>,
        key: &str,
    ) -> Result<bri_package::setting::SettingValue, String> {
        self.session.setting_value(self.package, game, team, key)
    }
    fn setting_text(&self, game: u64, team: Option<u64>, key: &str) -> Result<String, String> {
        self.session.setting_text(self.package, game, team, key)
    }
    fn setting_info(&self, key: &str) -> Option<serde_json::Value> {
        self.session.setting_info(self.package, key)
    }
    fn minigame_snapshot(&self, game: u64) -> Option<serde_json::Value> {
        self.session.minigame_snapshot(bri_minigames::GameId(game))
    }
    fn as_package(&self, package: &str, f: &mut dyn FnMut(Option<&dyn World>)) {
        f(Some(&ScriptWorld::new(self.session, package)));
    }
    fn host_data(&self, key: &str) -> Option<serde_json::Value> {
        self.session.host_data(self.package, key).cloned()
    }
    fn data_lines(&self, id: &str) -> Option<Vec<String>> {
        let host = self.session.packages.as_ref()?;
        let lines = host.catalog.packages.get(self.package)?.data_lines(id)?;
        Some(lines.into_iter().map(str::to_owned).collect())
    }
    fn pref(&self, name: &str) -> Option<bri_package::setting::SettingValue> {
        self.session.pref_value(name)
    }
    fn brick_box(&self, brick: u64) -> Option<([f32; 3], [f32; 3])> {
        let (min, max) = self.session.simulation.brick_box(brick)?;
        Some((min.to_array(), max.to_array()))
    }
    fn bricks_in(&self, min: [f32; 3], max: [f32; 3], limit: usize) -> Vec<u64> {
        let mut found = self
            .session
            .simulation
            .bricks_in_box(Vec3::from(min), Vec3::from(max));
        found.truncate(limit);
        found
    }
    fn can_edit(&self, caller: Option<u64>, brick: u64) -> bool {
        self.session
            .simulation
            .state()
            .bricks
            .get(&brick)
            .is_some_and(|b| self.session.rule_may_edit(caller, b.owner))
    }
    fn trust_level(&self, actor: u64, builder: u64) -> u8 {
        self.session
            .peers
            .get(&actor)
            .map_or(0, |p| p.actor.trust_level(builder))
    }
    fn may_copy(&self, player: u64, brick: u64, rule: bri_package_runtime::ops::CopyRule) -> bool {
        let sim = &self.session.simulation;
        match (
            self.session.peers.get(&player),
            sim.state().bricks.get(&brick),
        ) {
            (Some(peer), Some(b)) => blueprints::admits(&peer.actor, rule, sim, brick, b),
            _ => false,
        }
    }
    fn can_plant(&self, kind: &str, position: [f32; 3], turns: u8) -> bool {
        self.session
            .planted_brick(kind, position, turns, 0, 0)
            .is_some_and(|b| self.session.simulation.fits_clear_of_bodies(&b))
    }
}
