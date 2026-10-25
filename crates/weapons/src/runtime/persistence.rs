use super::*;
/// Weapon save schema. Alpha saves of any other version do not load.
pub const SAVE_SCHEMA: u32 = 4;
/// Complete fixed-tick state, independent of render/physics handles and legacy files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponsSave {
    pub schema_version: u32,
    pub pack_id: String,
    pub tick: u64,
    pub next_id: u64,
    /// Last allocated item, mount or animation producer identity, including
    /// instances no longer retained by an actor. Never reconstruct from live state.
    pub animation_serial: u64,
    pub actors: Vec<(ActorId, Actor)>,
    pub projectiles: Vec<Projectile>,
    pub drops: Vec<Drop>,
    /// [`WeaponsWorld::fuses`], by projectile id.
    #[serde(default)]
    pub fuses: Vec<(u64, u32)>,
}
impl WeaponsWorld {
    pub fn save(&self) -> WeaponsSave {
        WeaponsSave {
            schema_version: SAVE_SCHEMA,
            pack_id: self.pack.id.clone(),
            tick: self.tick,
            next_id: self.next_id,
            animation_serial: self.animation_serial,
            actors: self.actors.iter().map(|(id, a)| (*id, a.clone())).collect(),
            projectiles: self.projectiles.values().cloned().collect(),
            drops: self
                .drops
                .values()
                .filter(|d| !self.pinned.contains(&d.id))
                .cloned()
                .collect(),
            fuses: self.fuses.iter().map(|(id, f)| (*id, *f)).collect(),
        }
    }
    pub fn restore(pack: Pack, bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 16 * 1024 * 1024, "Weapon save byte budget");
        let save: WeaponsSave = serde_json::from_slice(bytes)?;
        ensure!(
            save.schema_version == SAVE_SCHEMA && save.pack_id == pack.id,
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
        w.animation_serial = save.animation_serial;
        let valid_generation = |generation: u64| generation > 0 && generation <= w.animation_serial;
        // None is only produced for an occupied instance after exhaustion.
        let valid_stamp =
            |stamp: Option<u64>| stamp.map_or(w.animation_serial == u64::MAX, valid_generation);
        let mut live_generations = BTreeSet::new();
        for (id, a) in save.actors {
            a.frame.validate()?;
            ensure!((1..=16).contains(&a.inventory.len()), "Save inventory size");
            ensure!(
                a.animation_items.len() == a.inventory.len()
                    && a.animation_items
                        .iter()
                        .zip(&a.inventory)
                        .all(|(stamp, item)| {
                            if item.is_some() {
                                valid_stamp(*stamp)
                            } else {
                                stamp.is_none()
                            }
                        })
                    && a.images
                        .iter()
                        .flatten()
                        .chain(a.emote.iter())
                        .all(|image| valid_stamp(image.animation_generation)),
                "Save animation instance identity"
            );
            for generation in a.animation_items.iter().copied().flatten().chain(
                a.images
                    .iter()
                    .flatten()
                    .chain(a.emote.iter())
                    .filter_map(|image| image.animation_generation),
            ) {
                ensure!(
                    valid_generation(generation) && live_generations.insert(generation),
                    "Save animation allocator/duplicate instance"
                );
            }
            ensure!(
                a.cues
                    .iter()
                    .filter_map(|cue| cue.animation_binding.as_ref())
                    .all(|binding| {
                        binding
                            .item
                            .as_ref()
                            .is_none_or(|item| valid_generation(item.generation))
                            && binding
                                .image
                                .as_ref()
                                .is_none_or(|image| valid_generation(image.generation))
                    }),
                "Save deferred animation identity"
            );
            for item in a.inventory.iter().flatten() {
                ensure!(w.contains_item(item), "Save unknown item");
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
                    && (d.expires == DROP_LINGERS
                        && w.pack.items.get(&d.item).is_some_and(|i| i.lingers)
                        || d.expires >= w.tick && d.expires - w.tick <= MAX_DROP_TICKS)
                    && d.pickup_after <= w.tick.saturating_add(DROP_PICKUP_DELAY_TICKS),
                "Save drop input"
            );
            ensure!(w.drops.insert(d.id, d).is_none(), "Duplicate saved drop");
        }
        ensure!(save.fuses.len() <= MAX_PROJECTILES, "Save object budget");
        for (id, fuse) in save.fuses {
            if w.projectiles.contains_key(&id) {
                w.fuses.insert(id, fuse);
            }
        }
        Ok(w)
    }
}
