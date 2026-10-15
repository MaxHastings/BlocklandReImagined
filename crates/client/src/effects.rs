//! Cosmetic attachments derive exclusively from the server's replicated world.
//! No gameplay or particle state is accepted from a remote client.
use anyhow::{Context, Result};
use bri_fx_runtime::*;
use bri_net::protocol::PublicWorld;
use bri_world::{BrickId, ContentRef};
use glam::Vec3;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone)]
struct Attachment {
    asset: String,
    transform: SourceTransform,
    options: SourceOptions,
}

struct Active {
    asset: String,
    handle: EffectHandle,
}
/// Stable brick identity and resource kind; light and emitter never share a handle.
type Key = (BrickId, bool);
pub struct WorldEffects {
    pub world: EffectsWorld,
    limits: EffectsLimits,
    source: Option<Arc<PublicWorld>>,
    attachments: BTreeMap<Key, Attachment>,
    active: BTreeMap<Key, Active>,
    /// Preserved unresolved imported records, never substituted with
    /// another effect, by brick so a changed brick clears its own.
    warnings: BTreeMap<BrickId, Vec<String>>,
    /// Cosmetic sources outside the current nearest-source budget; retried each frame.
    pub deferred: usize,
    /// Bricks [`Self::sync`] has looked at.
    visited: u64,
    /// The game's own effects, which a map's emitters are added to.
    base: Arc<EffectsPack>,
    /// The map's own emitters (`map_effects.json`), always running.
    map_sources: Vec<(String, SourceTransform)>,
    map_active: Vec<Option<EffectHandle>>,
}

/// A map's own emitters, from `map_effects.json` beside its bundle.
pub fn load_map_effects(
    map_root: &std::path::Path,
) -> Result<Option<bri_content::effects::MapEffects>> {
    let path = map_root.join("map_effects.json");
    if !path.is_file() {
        return Ok(None);
    }
    anyhow::ensure!(
        std::fs::metadata(&path)?.len() <= 4 << 20,
        "map_effects.json is over 4 MiB"
    );
    let effects: bri_content::effects::MapEffects =
        serde_json::from_slice(&std::fs::read(&path)?).context("map_effects.json")?;
    effects.validate()?;
    Ok(Some(effects))
}

/// `base` with a map's particles and emitters added. A particle drawing a
/// texture the game lacks draws the cloud, as v20 did.
fn with_map_effects(
    base: &EffectsPack,
    effects: &bri_content::effects::MapEffects,
) -> Result<Arc<EffectsPack>> {
    let mut library = base.library.clone();
    for p in &effects.particles {
        if library.particles.iter().any(|q| q.id == p.id) {
            continue;
        }
        let mut p = p.clone();
        if !library.textures.contains_key(&p.texture) {
            p.texture = crate::weapon_effects::MISSING_PARTICLE_TEXTURE.into();
        }
        library.particles.push(p);
    }
    for e in &effects.emitters {
        if !library.emitters.iter().any(|x| x.id == e.id) {
            library.emitters.push(bri_content::effects::Emitter {
                name: String::new(),
                ..e.clone()
            });
        }
    }
    let textures = base
        .textures
        .iter()
        .map(|t| bri_fx_runtime::pack::TextureImage {
            id: t.id.clone(),
            width: t.width,
            height: t.height,
            rgba: t.rgba.clone(),
        })
        .collect();
    EffectsPack::from_parts(library, base.manifest.clone(), textures)
}
impl WorldEffects {
    pub fn new(pack: Arc<EffectsPack>, limits: EffectsLimits) -> Result<Self> {
        Ok(Self {
            world: EffectsWorld::new(pack.clone(), limits, 0x425249)?,
            limits,
            source: None,
            attachments: BTreeMap::new(),
            active: BTreeMap::new(),
            warnings: BTreeMap::new(),
            deferred: 0,
            visited: 0,
            base: pack,
            map_sources: Vec::new(),
            map_active: Vec::new(),
        })
    }
    /// The next map's own emitters (none for most maps): the effects start
    /// over with them added to the game's.
    pub fn set_map_effects(
        &mut self,
        effects: Option<&bri_content::effects::MapEffects>,
    ) -> Result<()> {
        self.clear();
        let pack = match effects {
            Some(effects) => with_map_effects(&self.base, effects)?,
            None => self.base.clone(),
        };
        if !Arc::ptr_eq(self.world.pack(), &pack) {
            self.world = EffectsWorld::new(pack, self.limits, 0x425249)?;
        }
        self.map_sources = effects
            .into_iter()
            .flat_map(|e| &e.sources)
            .map(|s| {
                let rotation = glam::Quat::from_array(s.rotation).normalize();
                (
                    s.emitter.clone(),
                    SourceTransform {
                        position: Vec3::from(s.position),
                        rotation,
                        velocity: rotation * Vec3::Y * s.velocity,
                    },
                )
            })
            .collect();
        self.map_active = vec![None; self.map_sources.len()];
        Ok(())
    }
    pub fn clear(&mut self) {
        self.world.teardown();
        self.map_active.iter_mut().for_each(|a| *a = None);
        self.source = None;
        self.attachments.clear();
        self.active.clear();
        self.warnings.clear();
        self.deferred = 0;
    }
    pub fn attachment_count(&self) -> usize {
        self.attachments.len()
    }
    /// Follow the replicated world: the attachments of the bricks that
    /// changed since the last world it saw (an imbl diff, which skips the
    /// structure the two share), or of every brick when there is no last
    /// world or its palette or map changed.
    pub fn sync(
        &mut self,
        source: Arc<PublicWorld>,
        meshes: &BTreeMap<String, bri_content::brick::Brick>,
    ) -> Result<()> {
        let old = match &self.source {
            Some(old) if Arc::ptr_eq(old, &source) => return Ok(()),
            Some(old) if old.palette == source.palette && old.map_id == source.map_id => {
                Some(old.clone())
            }
            _ => None,
        };
        let followed = match old {
            Some(old) => self.follow(&old, &source, meshes),
            None => self.rebuild(&source, meshes),
        };
        // A world that failed part way is rebuilt whole next time.
        self.source = followed.is_ok().then_some(source);
        followed
    }
    fn follow(
        &mut self,
        old: &PublicWorld,
        new: &PublicWorld,
        meshes: &BTreeMap<String, bri_content::brick::Brick>,
    ) -> Result<()> {
        for change in old.bricks.diff(&new.bricks) {
            let (id, brick) = match change {
                imbl::ordmap::DiffItem::Remove(id, _) => (*id, None),
                imbl::ordmap::DiffItem::Add(id, brick)
                | imbl::ordmap::DiffItem::Update {
                    new: (id, brick), ..
                } => (*id, Some(brick)),
            };
            self.visited += 1;
            self.attachments.remove(&(id, false));
            self.attachments.remove(&(id, true));
            self.warnings.remove(&id);
            if let Some(brick) = brick {
                let pack = self.world.pack().clone();
                attach(
                    &pack,
                    new,
                    meshes,
                    id,
                    brick,
                    &mut self.attachments,
                    &mut self.warnings,
                )?;
            }
        }
        Ok(())
    }
    fn rebuild(
        &mut self,
        source: &PublicWorld,
        meshes: &BTreeMap<String, bri_content::brick::Brick>,
    ) -> Result<()> {
        let pack = self.world.pack().clone();
        let mut attachments = BTreeMap::new();
        let mut warnings = BTreeMap::new();
        for (&id, brick) in &source.bricks {
            self.visited += 1;
            attach(
                &pack,
                source,
                meshes,
                id,
                brick,
                &mut attachments,
                &mut warnings,
            )?;
        }
        self.attachments = attachments;
        self.warnings = warnings;
        Ok(())
    }
    /// Bricks [`Self::sync`] has looked at, for the work-count tests.
    pub fn visited(&self) -> u64 {
        self.visited
    }
    /// Preserved unresolved imported records, never substituted with
    /// another effect.
    pub fn warnings(&self) -> impl Iterator<Item = &String> {
        self.warnings.values().flatten()
    }
    /// Reconcile nearest sources without resetting unchanged animation/emission clocks.
    /// Visibility callback ignores the emitting brick, but must test intervening geometry.
    pub fn advance(
        &mut self,
        dt: f32,
        eye: Vec3,
        wind: Vec3,
        mut visible: impl FnMut(BrickId, Vec3, Vec3) -> Result<bool>,
    ) -> Result<()> {
        // Within budget every attachment runs; only past it do the nearest
        // win, which needs them in distance order.
        let lights_wanted = self.attachments.keys().filter(|k| k.1).count();
        let selected: BTreeSet<Key> = if self.attachments.len() <= self.limits.sources
            && lights_wanted <= self.limits.lights
        {
            self.attachments.keys().copied().collect()
        } else {
            let mut order: Vec<_> = self.attachments.iter().collect();
            order.sort_by(|(ka, a), (kb, b)| {
                a.transform
                    .position
                    .distance_squared(eye)
                    .total_cmp(&b.transform.position.distance_squared(eye))
                    .then_with(|| ka.cmp(kb))
            });
            let mut selected = BTreeSet::new();
            let mut lights = 0;
            for (&key, _) in order {
                if selected.len() >= self.limits.sources || (key.1 && lights >= self.limits.lights)
                {
                    continue;
                }
                selected.insert(key);
                if key.1 {
                    lights += 1;
                }
            }
            selected
        };
        self.deferred = self.attachments.len() - selected.len();
        let removed: Vec<_> = self
            .active
            .iter()
            .filter(|(k, a)| {
                !selected.contains(k) || self.attachments.get(k).is_none_or(|b| b.asset != a.asset)
            })
            .map(|(k, _)| *k)
            .collect();
        for key in removed {
            let old = self.active.remove(&key).unwrap();
            self.world.stop(
                old.handle,
                if key.1 {
                    StopMode::Immediate
                } else {
                    StopMode::Drain
                },
            );
        }
        for key in selected {
            let attachment = &self.attachments[&key];
            let mut options = attachment.options.clone();
            if key.1 {
                // Flares only show nearer than FLARE_MAX_DISTANCE; farther
                // lights skip the line-of-sight test, like player lights.
                let at = attachment.transform.position;
                options.flare_visibility =
                    if eye.distance(at) < FLARE_MAX_DISTANCE && visible(key.0, eye, at)? {
                        1.
                    } else {
                        0.
                    };
            }
            if let Some(active) = self.active.get(&key) {
                // Finite authored emitters end naturally; do not restart every frame.
                if self.world.is_active(active.handle) {
                    self.world
                        .update_source(active.handle, attachment.transform)?;
                    self.world.update_options(active.handle, options)?;
                }
            } else {
                let handle = if key.1 {
                    self.world
                        .start_light(&attachment.asset, attachment.transform, options)?
                } else {
                    self.world
                        .start_emitter(&attachment.asset, attachment.transform, options)?
                };
                self.active.insert(
                    key,
                    Active {
                        asset: attachment.asset.clone(),
                        handle,
                    },
                );
            }
        }
        // The map's own emitters run all the time.
        for ((asset, transform), active) in self.map_sources.iter().zip(&mut self.map_active) {
            if active.is_none_or(|h| !self.world.is_active(h)) {
                *active = Some(self.world.start_emitter(
                    asset,
                    *transform,
                    SourceOptions::default(),
                )?);
            }
        }
        self.world.advance(dt, wind)
    }
}

/// The emitter and light a brick carries, if any, into `attachments`, and
/// what of them cannot be resolved into `warnings`.
fn attach(
    pack: &EffectsPack,
    source: &PublicWorld,
    meshes: &BTreeMap<String, bri_content::brick::Brick>,
    id: BrickId,
    brick: &bri_world::Brick,
    attachments: &mut BTreeMap<Key, Attachment>,
    warnings: &mut BTreeMap<BrickId, Vec<String>>,
) -> Result<()> {
    if brick
        .emitter
        .as_ref()
        .and_then(|e| e.asset.as_ref())
        .is_none()
        && !brick.light.as_ref().is_some_and(|l| l.enabled)
    {
        return Ok(());
    }
    let mut warn = |text: String| warnings.entry(id).or_default().push(text);
    let ContentRef::Resolved(definition) = &brick.definition else {
        warn(format!("Unresolved effect brick {id}"));
        return Ok(());
    };
    let mesh = meshes
        .get(definition)
        .context("Effect brick mesh missing")?;
    let stud_size = [
        mesh.footprint_studs[0],
        mesh.footprint_studs[1],
        mesh.height_plates,
    ];
    let mut size = Vec3::new(
        stud_size[0] as f32 * 0.5,
        stud_size[2] as f32 * 0.2,
        stud_size[1] as f32 * 0.5,
    );
    if brick.quarter_turns % 2 == 1 {
        std::mem::swap(&mut size.x, &mut size.z);
    }
    let center = Vec3::from(brick.position);
    if let Some(emitter) = &brick.emitter
        && let Some(reference) = &emitter.asset
    {
        if let ContentRef::Resolved(asset) = reference {
            let (transform, options) = brick_source(
                pack,
                asset,
                &BrickAttachment {
                    center,
                    world_size: size,
                    stud_size,
                    direction: emitter.direction,
                    paint: *source
                        .palette
                        .get(brick.color as usize)
                        .context("Effect paint index outside palette")?,
                    // Fake kill has no replicated native state yet. Visibility is deliberately independent.
                    fake_dead: false,
                },
            )?;
            attachments.insert(
                (id, false),
                Attachment {
                    asset: asset.clone(),
                    transform,
                    options,
                },
            );
        } else {
            warn(format!("Unresolved emitter on brick {id}: {reference:?}"));
        }
    }
    if let Some(light) = &brick.light
        && light.enabled
    {
        if let ContentRef::Resolved(asset) = &light.asset {
            anyhow::ensure!(
                pack.library.lights.iter().any(|l| &l.id == asset),
                "Unknown brick light {asset}"
            );
            attachments.insert(
                (id, true),
                Attachment {
                    asset: asset.clone(),
                    transform: SourceTransform {
                        position: center,
                        ..Default::default()
                    },
                    options: SourceOptions {
                        flare_visibility: 0.,
                        ..Default::default()
                    },
                },
            );
        } else {
            warn(format!("Unresolved light on brick {id}: {:?}", light.asset));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// An effects pack and an emitter and a light it has: made up
    /// (`bri_fx_runtime::testing`), or the converted v20 pack.
    struct Effects {
        pack: Arc<EffectsPack>,
        emitter: String,
        light: String,
    }
    impl Effects {
        fn synthetic() -> Result<Self> {
            Ok(Self {
                pack: bri_fx_runtime::testing::pack(|_| {}),
                emitter: "emitter".into(),
                light: "light".into(),
            })
        }
        fn content() -> Result<Self> {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            Ok(Self {
                pack: EffectsPack::load(bri_package::testing::pack_dir(
                    &root.join("content"),
                    "effects_runtime",
                ))?,
                emitter: "v20/emitter/playerjetemitter".into(),
                light: "v20/light/redlight".into(),
            })
        }
    }
    crate::testing::synthetic_and_content!(
        Effects: replicated_effect_lifecycle_late_join_and_capacity
    );

    /// A map's own emitter (an imported map's waterfall) is added to the
    /// game's effects and runs where the map puts it; the next map without
    /// one goes back to the game's own pack.
    #[test]
    fn a_maps_own_emitters_run_where_it_places_them() -> Result<()> {
        let base = bri_fx_runtime::testing::pack(|_| {});
        let mut particle = base.library.particles[0].clone();
        particle.id = "creek:particle/fall".into();
        // A texture the game lacks draws the cloud, as v20 did.
        particle.texture = "add-ons/map_creek/waterfall".into();
        let mut emitter = base.library.emitters[0].clone();
        emitter.id = "creek:emitter/fall".into();
        emitter.particles = vec![particle.id.clone()];
        let map = bri_content::effects::MapEffects {
            schema_version: 1,
            particles: vec![particle],
            emitters: vec![emitter],
            sources: vec![bri_content::effects::MapEmitter {
                node: 3,
                emitter: "creek:emitter/fall".into(),
                position: [10.0, 20.0, 30.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                velocity: 1.0,
            }],
        };
        let mut library = base.library.clone();
        library.textures.insert(
            crate::weapon_effects::MISSING_PARTICLE_TEXTURE.into(),
            "cloud.png".into(),
        );
        let image = |id: &str| bri_fx_runtime::pack::TextureImage {
            id: id.into(),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        };
        let base = EffectsPack::from_parts(
            library,
            bri_fx_runtime::testing::manifest(),
            vec![
                image(bri_fx_runtime::testing::TEXTURE),
                image(crate::weapon_effects::MISSING_PARTICLE_TEXTURE),
            ],
        )?;
        let mut effects = WorldEffects::new(base.clone(), Default::default())?;
        effects.set_map_effects(Some(&map))?;
        for _ in 0..30 {
            effects.advance(1.0 / 30.0, Vec3::ZERO, Vec3::ZERO, |_, _, _| Ok(true))?;
        }
        assert!(effects.world.particle_count() > 0, "the waterfall runs");
        let drawn = effects.world.pack();
        let fall = &drawn.library.particles[drawn.library.particles.len() - 1];
        assert_eq!(
            fall.texture,
            crate::weapon_effects::MISSING_PARTICLE_TEXTURE
        );
        effects.set_map_effects(None)?;
        assert!(Arc::ptr_eq(effects.world.pack(), &base));
        assert_eq!(effects.world.particle_count(), 0);
        Ok(())
    }
    fn plate() -> BTreeMap<String, bri_content::brick::Brick> {
        BTreeMap::from([(
            "plate".into(),
            bri_content::brick::Brick {
                schema_version: 1,
                id: "plate".into(),
                footprint_studs: [2, 1],
                height_plates: 1,
                attachment_rows: vec!["bb".into()],
                collision_boxes: vec![],
                needs_external_collision: false,
                coverage: None,
                quads: vec![],
            },
        )])
    }
    /// What each attachment is, comparable.
    fn attached(effects: &WorldEffects) -> Vec<(Key, String, [f32; 3], SourceOptions)> {
        effects
            .attachments
            .iter()
            .map(|(k, a)| {
                (
                    *k,
                    a.asset.clone(),
                    a.transform.position.to_array(),
                    a.options.clone(),
                )
            })
            .collect()
    }
    #[test]
    fn a_world_update_looks_only_at_the_bricks_it_changed() -> Result<()> {
        let fx = Effects::synthetic()?;
        let meshes = plate();
        let brick = |i: u64| {
            let mut b = bri_world::Brick::new(
                ContentRef::Resolved("plate".into()),
                [i as f32, 0.1, 0.25],
                0,
            );
            if i.is_multiple_of(10) {
                b.emitter = Some(Box::new(bri_world::Emitter {
                    asset: Some(ContentRef::Resolved(fx.emitter.clone())),
                    direction: 0,
                }));
            }
            if i.is_multiple_of(15) {
                b.light = Some(Box::new(bri_world::Light {
                    asset: ContentRef::Resolved(fx.light.clone()),
                    enabled: true,
                }));
            }
            b
        };
        let mut replica = PublicWorld {
            name: "test".into(),
            map_id: "native".into(),
            palette: vec![[0.2, 0.4, 0.8, 1.], [0.9, 0.1, 0.1, 1.]],
            bricks: (0..10_000).map(|i| (i, brick(i))).collect(),
        };
        let mut effects = WorldEffects::new(fx.pack.clone(), Default::default())?;
        effects.sync(Arc::new(replica.clone()), &meshes)?;
        assert_eq!(effects.visited(), 10_000);
        assert_eq!(effects.attachment_count(), 1000 + 667);
        // Repaint an emitter, remove a light, add an emitter, and turn a
        // plain brick into an unresolved one: four bricks, four looks.
        let mut painted = replica.bricks[&20].clone();
        painted.color = 1;
        replica.bricks.insert(20, painted);
        replica.bricks.remove(&45);
        replica.bricks.insert(20_000, brick(20_000));
        let mut unresolved = brick(30);
        unresolved.definition = ContentRef::unresolved("brick", "gone");
        replica.bricks.insert(31, unresolved);
        effects.sync(Arc::new(replica.clone()), &meshes)?;
        assert_eq!(effects.visited(), 10_004);
        // The same attachments and warnings a whole rebuild finds.
        let mut whole = WorldEffects::new(fx.pack.clone(), Default::default())?;
        whole.sync(Arc::new(replica.clone()), &meshes)?;
        assert_eq!(attached(&effects), attached(&whole));
        assert_eq!(
            effects.warnings().collect::<Vec<_>>(),
            whole.warnings().collect::<Vec<_>>()
        );
        assert_eq!(effects.warnings().count(), 1);
        // A changed brick clears its own warning.
        replica.bricks.insert(31, brick(31));
        effects.sync(Arc::new(replica.clone()), &meshes)?;
        assert_eq!(effects.warnings().count(), 0);
        // A new palette repaints every emitter: the whole world again.
        replica.palette[0] = [1.; 4];
        let before = effects.visited();
        effects.sync(Arc::new(replica), &meshes)?;
        assert_eq!(effects.visited() - before, 10_000);
        Ok(())
    }
    /// The cost of following one changed brick in a 200,000-brick world
    /// with 488 emitters and 151 lights (the Bedroom stack of the v0.2.10
    /// waste audit), against the whole walk every change used to cost.
    ///
    ///   cargo test --release -p bri-client --lib one_change_in_a_big_world -- --ignored --nocapture
    #[test]
    #[ignore = "timing benchmark; run in release with --nocapture"]
    fn one_change_in_a_big_world() -> Result<()> {
        let fx = Effects::synthetic()?;
        let meshes = plate();
        let mut replica = PublicWorld {
            name: "test".into(),
            map_id: "native".into(),
            palette: vec![[0.2, 0.4, 0.8, 1.], [0.9, 0.1, 0.1, 1.]],
            bricks: (0..200_000u64)
                .map(|i| {
                    let mut b = bri_world::Brick::new(
                        ContentRef::Resolved("plate".into()),
                        [(i % 500) as f32, (i / 500) as f32 * 0.2, 0.25],
                        0,
                    );
                    if i % 410 == 0 {
                        b.emitter = Some(Box::new(bri_world::Emitter {
                            asset: Some(ContentRef::Resolved(fx.emitter.clone())),
                            direction: 0,
                        }));
                    }
                    if i % 1325 == 7 {
                        b.light = Some(Box::new(bri_world::Light {
                            asset: ContentRef::Resolved(fx.light.clone()),
                            enabled: true,
                        }));
                    }
                    (i, b)
                })
                .collect(),
        };
        let mut effects = WorldEffects::new(fx.pack.clone(), Default::default())?;
        effects.sync(Arc::new(replica.clone()), &meshes)?;
        let (mut follow, mut whole) = (vec![], vec![]);
        for round in 0..50u64 {
            let id = round * 3_989 % 200_000;
            let mut changed = replica.bricks[&id].clone();
            changed.color ^= 1;
            replica.bricks.insert(id, changed);
            let world = Arc::new(replica.clone());
            let start = std::time::Instant::now();
            effects.sync(world.clone(), &meshes)?;
            follow.push(start.elapsed().as_secs_f64() * 1e3);
            let start = std::time::Instant::now();
            effects.rebuild(&world, &meshes)?;
            whole.push(start.elapsed().as_secs_f64() * 1e3);
        }
        follow.sort_by(f64::total_cmp);
        whole.sort_by(f64::total_cmp);
        println!(
            "one changed brick of 200,000 ({} attachments): followed p50 {:.4} ms max {:.4} ms; whole walk p50 {:.3} ms max {:.3} ms",
            effects.attachment_count(),
            follow[25],
            follow[49],
            whole[25],
            whole[49]
        );
        Ok(())
    }
    fn replicated_effect_lifecycle_late_join_and_capacity(fx: &Effects) -> Result<()> {
        let pack = fx.pack.clone();
        let mesh = bri_content::brick::Brick {
            schema_version: 1,
            id: "plate".into(),
            footprint_studs: [2, 1],
            height_plates: 1,
            attachment_rows: vec!["bb".into()],
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![],
        };
        let meshes = BTreeMap::from([("plate".into(), mesh)]);
        let mut brick =
            bri_world::Brick::new(ContentRef::Resolved("plate".into()), [0.5, 1.1, 0.25], 1);
        brick.visible = false;
        brick.emitter = Some(Box::new(bri_world::Emitter {
            asset: Some(ContentRef::Resolved(fx.emitter.clone())),
            direction: 0,
        }));
        brick.light = Some(Box::new(bri_world::Light {
            asset: ContentRef::Resolved(fx.light.clone()),
            enabled: true,
        }));
        let mut replica = PublicWorld {
            name: "test".into(),
            map_id: "native".into(),
            palette: vec![[0.2, 0.4, 0.8, 1.]],
            bricks: bri_world::Bricks::unit(7, brick),
        };
        let mut effects = WorldEffects::new(
            pack.clone(),
            EffectsLimits {
                sources: 2,
                lights: 1,
                ..Default::default()
            },
        )?;
        effects.sync(Arc::new(replica.clone()), &meshes)?;
        effects.advance(0.1, Vec3::ZERO, Vec3::ZERO, |_, _, _| Ok(true))?;
        assert_eq!(effects.world.source_count(), 2);
        assert!(
            effects.world.particle_count() > 0,
            "Invisible bricks retain intentional emitters"
        );
        let initial = effects.active[&(7, false)].handle;
        replica.name = "unrelated network update".into();
        effects.sync(Arc::new(replica.clone()), &meshes)?;
        effects.advance(0.1, Vec3::ZERO, Vec3::ZERO, |_, _, _| Ok(false))?;
        assert_eq!(
            initial,
            effects.active[&(7, false)].handle,
            "Unrelated deltas restarted emitter"
        );
        let mut late = WorldEffects::new(pack, Default::default())?;
        late.sync(Arc::new(replica.clone()), &meshes)?;
        late.advance(0.1, Vec3::ZERO, Vec3::ZERO, |_, _, _| Ok(true))?;
        assert_eq!(late.world.source_count(), 2);
        assert!(late.world.particle_count() > 0);
        let mut distant = replica.bricks[&7].clone();
        distant.position[0] = 100.;
        replica.bricks.insert(8, distant);
        effects.sync(Arc::new(replica.clone()), &meshes)?;
        effects.advance(0.1, Vec3::ZERO, Vec3::ZERO, |_, _, _| Ok(true))?;
        assert_eq!(effects.deferred, 2);
        assert!(effects.active.contains_key(&(7, true)));
        effects.advance(0.1, Vec3::X * 100., Vec3::ZERO, |_, _, _| Ok(true))?;
        assert!(
            effects.active.contains_key(&(8, true)) && !effects.active.contains_key(&(7, true))
        );
        replica.bricks.clear();
        effects.sync(Arc::new(replica), &meshes)?;
        effects.advance(0., Vec3::ZERO, Vec3::ZERO, |_, _, _| Ok(true))?;
        assert_eq!(effects.world.source_count(), 0);
        assert!(
            effects.world.particle_count() > 0,
            "Deleted emitter particles should drain"
        );
        effects.clear();
        assert_eq!(effects.world.particle_count(), 0);
        assert_eq!(effects.attachment_count(), 0);
        assert_eq!(effects.deferred, 0);
        Ok(())
    }
}
