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
    /// Preserved unresolved imported records, never substituted with another effect.
    pub warnings: BTreeSet<String>,
    /// Cosmetic sources outside the current nearest-source budget; retried each frame.
    pub deferred: usize,
}
impl WorldEffects {
    pub fn new(pack: Arc<EffectsPack>, limits: EffectsLimits) -> Result<Self> {
        Ok(Self {
            world: EffectsWorld::new(pack, limits, 0x425249)?,
            limits,
            source: None,
            attachments: BTreeMap::new(),
            active: BTreeMap::new(),
            warnings: BTreeSet::new(),
            deferred: 0,
        })
    }
    pub fn clear(&mut self) {
        self.world.teardown();
        self.source = None;
        self.attachments.clear();
        self.active.clear();
        self.warnings.clear();
        self.deferred = 0;
    }
    pub fn attachment_count(&self) -> usize {
        self.attachments.len()
    }
    pub fn sync(
        &mut self,
        source: Arc<PublicWorld>,
        meshes: &BTreeMap<String, bri_content::brick::Brick>,
    ) -> Result<()> {
        if self
            .source
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &source))
        {
            return Ok(());
        }
        let mut attachments = BTreeMap::new();
        let mut warnings = BTreeSet::new();
        for (&id, brick) in &source.bricks {
            if brick
                .emitter
                .as_ref()
                .and_then(|e| e.asset.as_ref())
                .is_none()
                && !brick.light.as_ref().is_some_and(|l| l.enabled)
            {
                continue;
            }
            let ContentRef::Resolved(definition) = &brick.definition else {
                warnings.insert(format!("Unresolved effect brick {id}"));
                continue;
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
                        self.world.pack(),
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
                    warnings.insert(format!("Unresolved emitter on brick {id}: {reference:?}"));
                }
            }
            if let Some(light) = &brick.light
                && light.enabled
            {
                if let ContentRef::Resolved(asset) = &light.asset {
                    anyhow::ensure!(
                        self.world
                            .pack()
                            .library
                            .lights
                            .iter()
                            .any(|l| &l.id == asset),
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
                    warnings.insert(format!("Unresolved light on brick {id}: {:?}", light.asset));
                }
            }
        }
        self.attachments = attachments;
        self.warnings = warnings;
        self.source = Some(source);
        Ok(())
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
            if selected.len() >= self.limits.sources || (key.1 && lights >= self.limits.lights) {
                continue;
            }
            selected.insert(key);
            if key.1 {
                lights += 1;
            }
        }
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
                options.flare_visibility = if visible(key.0, eye, attachment.transform.position)? {
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
        self.world.advance(dt, wind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires native effects pack, never opens a window or audio device"]
    fn replicated_effect_lifecycle_late_join_and_capacity() -> Result<()> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = EffectsPack::load(root.join("content/effects-runtime-pack-001"))?;
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
        brick.emitter = Some(bri_world::Emitter {
            asset: Some(ContentRef::Resolved("v20/emitter/playerjetemitter".into())),
            direction: 0,
        });
        brick.light = Some(bri_world::Light {
            asset: ContentRef::Resolved("v20/light/redlight".into()),
            enabled: true,
        });
        let mut replica = PublicWorld {
            name: "test".into(),
            map_id: "native".into(),
            palette: vec![[0.2, 0.4, 0.8, 1.]],
            bricks: BTreeMap::from([(7, brick)]),
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
