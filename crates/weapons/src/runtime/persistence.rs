use super::*;
/// Complete fixed-tick state, independent of render/physics handles and legacy files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponsSave {
    pub schema_version: u32,
    pub pack_id: String,
    pub tick: u64,
    pub next_id: u64,
    pub actors: Vec<(ActorId, Actor)>,
    pub projectiles: Vec<Projectile>,
    pub drops: Vec<Drop>,
}
impl WeaponsWorld {
    pub fn save(&self) -> WeaponsSave {
        WeaponsSave {
            schema_version: 3,
            pack_id: self.pack.id.clone(),
            tick: self.tick,
            next_id: self.next_id,
            actors: self.actors.iter().map(|(id, a)| (*id, a.clone())).collect(),
            projectiles: self.projectiles.values().cloned().collect(),
            drops: self.drops.values().cloned().collect(),
        }
    }
    pub fn restore(pack: Pack, bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 16 * 1024 * 1024, "Weapon save byte budget");
        let save: WeaponsSave = serde_json::from_slice(bytes)?;
        ensure!(
            matches!(save.schema_version, 1..=3) && save.pack_id == pack.id,
            "Save schema/pack mismatch"
        );
        ensure!(
            save.actors.len() <= MAX_ACTORS
                && save.projectiles.len() <= MAX_PROJECTILES
                && save.drops.len() <= MAX_DROPS,
            "Save object budget"
        );
        ensure!(
            save.tick < u64::MAX - 1_000_000
                && save.next_id < u64::MAX - 1_000_000
                && save.next_id > 0,
            "Save clock/identity overflow"
        );
        let mut w = Self::new(pack)?;
        w.tick = save.tick;
        w.next_id = save.next_id;
        for (id, a) in save.actors {
            a.frame.validate()?;
            ensure!((1..=16).contains(&a.inventory.len()), "Save inventory size");
            let mut items = std::collections::BTreeSet::new();
            for item in a.inventory.iter().flatten() {
                ensure!(w.contains_item(item), "Save unknown item");
                ensure!(items.insert(item), "Duplicate saved inventory item");
            }
            if let Some(slot) = a.selected {
                ensure!(
                    a.inventory.get(slot).is_some_and(Option::is_some),
                    "Save selected slot"
                );
            }
            for (hand, e) in a.images.iter().enumerate() {
                if let Some(e) = e {
                    let image = w.pack.images.get(&e.image).context("Save unknown image")?;
                    ensure!(
                        e.hand as usize == hand
                            && e.state < image.states.len()
                            && e.remaining <= 36000,
                        "Save image state"
                    );
                }
            }
            ensure!(w.actors.insert(id, a).is_none(), "Duplicate saved actor");
        }
        for p in save.projectiles {
            let definition = w
                .pack
                .projectiles
                .get(&p.definition)
                .context("Save unknown projectile")?;
            ensure!(
                p.id > 0 && p.id < w.next_id && p.age < definition.lifetime_ticks,
                "Save projectile identity/age"
            );
            ensure!(
                p.position.is_finite()
                    && p.origin.is_finite()
                    && p.velocity.is_finite()
                    && p.position.abs().max_element() < 1e7
                    && p.velocity.length() <= 10000.0
                    && (0.01..=100.0).contains(&p.scale),
                "Save projectile input"
            );
            ensure!(
                w.projectiles.insert(p.id, p).is_none(),
                "Duplicate saved projectile"
            );
        }
        for d in save.drops {
            ensure!(
                d.id > 0
                    && d.id < w.next_id
                    && !w.projectiles.contains_key(&d.id)
                    && w.contains_item(&d.item),
                "Save drop identity/item"
            );
            ensure!(
                d.position.is_finite()
                    && d.rotation.is_finite()
                    && (d.rotation.length_squared() - 1.).abs() < 0.001
                    && (0.01..=100.).contains(&d.scale)
                    && d.velocity.is_finite()
                    && d.position.abs().max_element() < 1e7
                    && d.velocity.length() <= 10000.0
                    && d.expires >= w.tick
                    && d.expires - w.tick <= if save.schema_version < 3 { 7200 } else { 1200 }
                    && d.pickup_after
                        <= w.tick
                            .saturating_add(if save.schema_version < 3 { 120 } else { 58 }),
                "Save drop input"
            );
            ensure!(w.drops.insert(d.id, d).is_none(), "Duplicate saved drop");
        }
        Ok(w)
    }
}
