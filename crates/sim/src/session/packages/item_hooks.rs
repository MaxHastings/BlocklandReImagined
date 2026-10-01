//! Add-On rules hearing about their own items and shots: `on_pickup`,
//! `on_drop` and `on_projectile_hit`, and the items they put in the world
//! (`drop_item`) or take back (`take_item`).
//!
//! A package hears only about content in its own namespace or one it
//! depends on, so a rule never sees another Add-On's guns.
use super::*;
use bri_weapons::{ActorId, ProjectileContact, TargetId};

/// Projectile contacts waiting for `on_projectile_hit`, oldest first.
const MAX_PENDING_HITS: usize = 256;
/// Dropped items carrying what `on_drop` kept with them.
const MAX_DROP_DATA: usize = 1024;
/// Items one package may have lying in the world from `drop_item`.
const MAX_PACKAGE_DROPS: usize = 64;

/// What `on_pickup` decided for an item a player touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::session) enum Pickup {
    /// The usual pickup, if they have room.
    Take,
    /// Leave it where it is.
    Leave,
    /// The rules used it up: it goes, and nobody gets it.
    UseUp,
}

/// Add-On state a session keeps for these hooks.
#[derive(Default)]
pub(in crate::session) struct ItemHooks {
    hits: VecDeque<ProjectileContact>,
    /// What `on_drop` returned, by drop id.
    drop_data: BTreeMap<u64, serde_json::Value>,
    /// The package that put each `drop_item` pickup in the world.
    package_drops: BTreeMap<u64, String>,
}

/// Whether `package` may speak for `id` (`namespace:kind/name`): its own
/// content or a dependency's.
pub(in crate::session) fn owns(catalog: &Catalog, package: &str, id: &str) -> bool {
    let namespace = id.split(':').next().unwrap_or_default();
    namespace == package
        || catalog
            .packages
            .get(package)
            .is_some_and(|p| p.manifest.dependencies.contains_key(namespace))
}

impl Session {
    /// Packages declaring a hook that own `id`, in catalog order.
    fn hooked(
        &self,
        declares: fn(&bri_package_runtime::content::Behaviour) -> bool,
        id: &str,
    ) -> Vec<String> {
        let Some(host) = self.packages.as_ref() else {
            return Vec::new();
        };
        host.catalog
            .behaviours()
            .filter(|(package, b)| declares(b) && owns(&host.catalog, package, id))
            .map(|(package, _)| package.clone())
            .collect()
    }

    /// `on_pickup(player, item, info)` as a living player touches `item`,
    /// lying as world drop `drop` or on spawn brick `spawner`. The first
    /// package to answer `false` or `"take"` decides.
    pub(in crate::session) fn package_pickup(
        &mut self,
        owner: OwnerId,
        item: &str,
        drop: Option<u64>,
        spawner: Option<BrickId>,
    ) -> Pickup {
        let hooks = self.hooked(|b| b.on_pickup, item);
        if hooks.is_empty() {
            return Pickup::Take;
        }
        let data = drop
            .and_then(|d| self.packages.as_ref()?.item_hooks.drop_data.get(&d))
            .map_or(Dynamic::UNIT, |v| {
                bri_package_runtime::rhai::serde::to_dynamic(v).unwrap_or(Dynamic::UNIT)
            });
        let mut info = bri_package_runtime::rhai::Map::new();
        let id = |v: Option<u64>| v.map_or(Dynamic::UNIT, |v| Dynamic::from_int(v as i64));
        info.insert("drop".into(), id(drop));
        info.insert("spawner".into(), id(spawner));
        info.insert("data".into(), data);
        for package in hooks {
            let answer = self.run_package(
                &package,
                "on_pickup",
                vec![
                    Dynamic::from_int(owner as i64),
                    item.into(),
                    Dynamic::from_map(info.clone()),
                ],
                Budget::Command,
                Some(owner),
                None,
                None,
            );
            self.charge_work(&package);
            let Ok(answer) = answer else {
                continue;
            };
            if answer.as_bool() == Ok(false) {
                return Pickup::Leave;
            }
            if answer
                .clone()
                .into_string()
                .is_ok_and(|s| s.eq_ignore_ascii_case("take"))
            {
                return Pickup::UseUp;
            }
            if !(answer.is_unit() || answer.as_bool() == Ok(true)) {
                self.hook_warning(
                    &package,
                    format!(
                        "on_pickup must return (), true, false or \"take\", not {}",
                        answer.type_name()
                    ),
                );
            }
        }
        Pickup::Take
    }

    /// `on_drop(player, item, slot)` as `owner` drops `item` from `slot` as
    /// world drop `drop`; what the hook returns is kept with the drop.
    pub(in crate::session) fn package_drop(
        &mut self,
        owner: OwnerId,
        item: &str,
        slot: usize,
        drop: u64,
    ) {
        for package in self.hooked(|b| b.on_drop, item) {
            let answer = self.run_package(
                &package,
                "on_drop",
                vec![
                    Dynamic::from_int(owner as i64),
                    item.into(),
                    Dynamic::from_int(slot as i64),
                ],
                Budget::Command,
                Some(owner),
                None,
                None,
            );
            self.charge_work(&package);
            let Ok(answer) = answer else {
                continue;
            };
            if answer.is_unit() {
                continue;
            }
            let kept = bri_package_runtime::rhai::serde::from_dynamic::<serde_json::Value>(&answer)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .and_then(|v| state::check_value(&v).map(|()| v));
            let host = self.packages.as_mut().expect("hooked packages run");
            match kept {
                Ok(value) if host.item_hooks.drop_data.len() < MAX_DROP_DATA => {
                    host.item_hooks.drop_data.insert(drop, value);
                }
                Ok(_) => self.hook_warning(&package, "too many dropped items carry data".into()),
                Err(error) => {
                    self.hook_warning(&package, format!("on_drop kept nothing: {error:#}"));
                }
            }
        }
    }

    /// A world drop is gone (picked up, used up, popped): forget what was
    /// kept with it.
    pub(in crate::session) fn forget_drop(&mut self, drop: u64) {
        if let Some(host) = self.packages.as_mut() {
            host.item_hooks.drop_data.remove(&drop);
            host.item_hooks.package_drops.remove(&drop);
        }
    }

    /// A projectile struck something: packages owning it hear of it next
    /// tick.
    pub(in crate::session) fn package_hit(&mut self, impact: &ProjectileContact) {
        if self.hooked(|b| b.on_projectile_hit, &impact.definition).is_empty() {
            return;
        }
        let host = self.packages.as_mut().expect("hooked packages run");
        if host.item_hooks.hits.len() == MAX_PENDING_HITS {
            note(
                host,
                Diagnostic::warning(
                    "hook.dropped",
                    "Too many projectile hits in one tick for on_projectile_hit",
                ),
            );
            return;
        }
        host.item_hooks.hits.push_back(impact.clone());
    }

    /// `on_projectile_hit(hit)` for each hit since the last tick, in order.
    /// Hits the hooks' own shots cause are delivered next tick.
    pub(super) fn deliver_hits(&mut self) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        let hits = std::mem::take(&mut host.item_hooks.hits);
        for hit in hits {
            let map = self.hit_map(&hit);
            for package in self.hooked(|b| b.on_projectile_hit, &hit.definition) {
                let _ = self.run_package(
                    &package,
                    "on_projectile_hit",
                    vec![map.clone()],
                    Budget::Command,
                    None,
                    None,
                    None,
                );
                self.charge_work(&package);
            }
        }
    }

    fn hit_map(&self, hit: &ProjectileContact) -> Dynamic {
        let int = |v: u64| Dynamic::from_int(v as i64);
        let (kind, id, object) = match hit.target {
            TargetId::Actor(a) => ("player", int(a.0), Some(ObjectRef::Player(a.0))),
            TargetId::Vehicle(v) => ("vehicle", int(v), Some(ObjectRef::Vehicle(v))),
            TargetId::Entity(e) => ("entity", int(e), Some(ObjectRef::Entity(e))),
            TargetId::Brick(b) => ("brick", int(b), None),
            TargetId::Map(_) | TargetId::Shape(_) => ("map", Dynamic::UNIT, None),
        };
        let by = Some(hit.source.0).filter(|o| self.peers.contains_key(o));
        let mut m = bri_package_runtime::rhai::Map::new();
        m.insert("projectile".into(), hit.definition.clone().into());
        m.insert("by".into(), by.map_or(Dynamic::UNIT, int));
        m.insert("kind".into(), kind.into());
        m.insert("id".into(), id);
        m.insert(
            "ref".into(),
            object.map_or(Dynamic::UNIT, |o| o.to_string().into()),
        );
        for (keys, v) in [
            (["x", "y", "z"], hit.position),
            (["nx", "ny", "nz"], hit.normal),
            (["vx", "vy", "vz"], hit.velocity),
        ] {
            for (key, value) in keys.into_iter().zip(v.to_array()) {
                m.insert(key.into(), Dynamic::from_float(f64::from(value)));
            }
        }
        Dynamic::from_map(m)
    }

    /// `take_item(player, item)`.
    pub(super) fn package_take_item(&mut self, player: OwnerId, item: &str) -> Result<()> {
        ensure!(self.peers.contains_key(&player), "No such player");
        let was = self.weapons.actor(ActorId(player)).and_then(|a| a.selected);
        let taken = self.weapons.take_item(ActorId(player), item)?;
        if taken.is_some() && taken == was {
            self.peers.get_mut(&player).expect("checked").inspection = None;
        }
        Ok(())
    }

    /// `drop_item(item, x, y, z, vx, vy, vz)`: a pickup anyone may take at
    /// once, popping after ten seconds.
    pub(super) fn package_drop_item(
        &mut self,
        package: &str,
        item: &str,
        position: [f32; 3],
        velocity: [f32; 3],
        data: Option<serde_json::Value>,
    ) -> Result<()> {
        ensure!(self.weapons.contains_item(item), "`{item}` is not an item of this server");
        let host = self.packages.as_ref().context("No packages are enabled")?;
        let lying = host
            .item_hooks
            .package_drops
            .values()
            .filter(|p| *p == package)
            .count();
        ensure!(
            lying < MAX_PACKAGE_DROPS,
            "`{package}` already has {MAX_PACKAGE_DROPS} items lying in the world"
        );
        let drop = self
            .weapons
            .spawn_drop(item, Vec3::from(position), Vec3::from(velocity))?;
        self.packages
            .as_mut()
            .expect("checked")
            .item_hooks
            .package_drops
            .insert(drop, package.to_string());
        if let Some(value) = data {
            let host = self.packages.as_mut().expect("checked");
            if host.item_hooks.drop_data.len() < MAX_DROP_DATA {
                host.item_hooks.drop_data.insert(drop, value);
            } else {
                self.hook_warning(package, "too many dropped items carry data".into());
            }
        }
        Ok(())
    }

    fn hook_warning(&mut self, package: &str, message: String) {
        if let Some(host) = self.packages.as_mut() {
            note(
                host,
                Diagnostic::warning("hook.answer", message).at(package.to_string()),
            );
        }
    }
}
