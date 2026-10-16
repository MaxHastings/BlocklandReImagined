//! Host-only native playback state. Rendering and package policy are consumers.
use super::*;
use bri_content::{
    avatar::Rig,
    body::{Role, RoleClips},
    shape::Shape,
};
use bri_weapons::{ActorId, AnimationBinding, AnimationOwner, AnimationProvenance};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_ASSETS: usize = 256;
const MAX_ASSET_BYTES: usize = 64 << 20;
const MAX_TOTAL_BYTES: usize = 256 << 20;
const MAX_ACTIVE: usize = 128;
const MAX_RECORDS: usize = 512;
const MAX_RECEIPTS: usize = 256;
const MAX_NONCE_OWNERS: usize = 4096;
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationAsset {
    pub resource: String,
    pub digest: String,
    pub rig: bool,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationImageAsset {
    pub model: String,
    pub resource: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationAssets {
    pub assets: Vec<AnimationAsset>,
    pub images: BTreeMap<String, AnimationImageAsset>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationClip {
    pub resource: String,
    pub digest: String,
    pub sequence: String,
    pub duration: f64,
    pub looping: bool,
}
type ClipIndex = BTreeMap<(String, String), AnimationClip>;
impl AnimationAssets {
    /// Lets bounded loaders retain supported resources without making optional
    /// missing clock metadata a failure of legacy hosting.
    pub fn within_resource_budget(count: usize, bytes: usize) -> bool {
        count <= MAX_ASSETS && bytes <= MAX_TOTAL_BYTES
    }
    fn index(&self) -> Result<ClipIndex> {
        ensure!(
            self.assets.len() <= MAX_ASSETS && self.images.len() <= 4096,
            "Animation catalog bounds"
        );
        let mut total = 0usize;
        let mut resources = BTreeSet::new();
        let mut clips = BTreeMap::new();
        for asset in &self.assets {
            total = total
                .checked_add(asset.bytes.len())
                .context("Animation catalog size overflow")?;
            ensure!(
                !asset.resource.is_empty()
                    && asset.resource.len() <= 256
                    && resources.insert(asset.resource.clone())
                    && asset.bytes.len() <= MAX_ASSET_BYTES
                    && total <= MAX_TOTAL_BYTES,
                "Animation resource bounds/duplicate"
            );
            ensure!(
                bri_package::environment::hash_bytes(&asset.bytes) == asset.digest,
                "Animation resource digest mismatch"
            );
            let sequences = if asset.rig {
                let rig: Rig = serde_json::from_slice(&asset.bytes)?;
                rig.validate()?;
                rig.sequences
            } else {
                let shape: Shape = serde_json::from_slice(&asset.bytes)?;
                shape.validate()?;
                let mut sequences = BTreeMap::new();
                for animation in shape.animations {
                    let name = animation.name.to_ascii_lowercase();
                    ensure!(
                        !name.is_empty() && sequences.insert(name, animation).is_none(),
                        "Ambiguous animation alias"
                    );
                }
                sequences
            };
            ensure!(sequences.len() <= 256, "Animation sequence count");
            for (name, animation) in sequences {
                animation.validate()?;
                ensure!(
                    !name.is_empty() && name.len() <= 128,
                    "Animation sequence name"
                );
                clips.insert(
                    (asset.resource.clone(), name.clone()),
                    AnimationClip {
                        resource: asset.resource.clone(),
                        digest: asset.digest.clone(),
                        sequence: name,
                        duration: f64::from(animation.duration),
                        looping: animation.looping,
                    },
                );
            }
        }
        for (image, binding) in &self.images {
            ensure!(
                !image.is_empty()
                    && image.len() <= 256
                    && !binding.model.is_empty()
                    && resources.contains(&binding.resource),
                "Invalid animation image binding"
            );
        }
        Ok(clips)
    }
    pub fn validate(&self) -> Result<()> {
        self.index().map(|_| ())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AnimationTarget {
    Pawn(u8),
    Weapon(usize),
    Image { hand: u8, emote: bool },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationStart {
    pub namespace: String,
    pub nonce: u64,
    pub target: AnimationTarget,
    pub sequence: String,
    pub binding: AnimationBinding,
    pub rate: f64,
    pub tween: f64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackId {
    pub session: u64,
    pub owner: OwnerId,
    pub serial: u64,
    pub epochs: AnimationOwner,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveAnimation {
    pub id: PlaybackId,
    pub request: AnimationStart,
    pub clip: AnimationClip,
    pub started_tick: u64,
    pub anchor_tick: u64,
    pub phase: f64,
    pub rate: f64,
}
impl ActiveAnimation {
    fn phase_at(&self, tick: u64) -> f64 {
        self.phase
            + tick.saturating_sub(self.anchor_tick) as f64 * self.rate
                / bri_world::TICKS_PER_SECOND as f64
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationOutcome {
    Started,
    Natural,
    Interrupted,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationRecord {
    pub playback: ActiveAnimation,
    pub outcome: AnimationOutcome,
    pub tick: u64,
    pub phase: f64,
    pub reason: String,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnimationView {
    pub tick: u64,
    pub revision: u64,
    pub active: Vec<ActiveAnimation>,
}
pub struct PreparedAnimationStart {
    revision: u64,
    catalog: u64,
    owner: OwnerId,
    request: AnimationStart,
    clip: AnimationClip,
    duplicate: Option<PlaybackId>,
}
struct Receipt {
    owner: OwnerId,
    request: AnimationStart,
    id: PlaybackId,
    clip: AnimationClip,
}
pub(super) struct AnimationClocks {
    session: Option<u64>,
    serial: u64,
    epoch: u64,
    revision: u64,
    catalog: u64,
    assets: AnimationAssets,
    clips: ClipIndex,
    owners: BTreeMap<OwnerId, AnimationOwner>,
    active: BTreeMap<(OwnerId, AnimationTarget), ActiveAnimation>,
    records: VecDeque<AnimationRecord>,
    receipts: VecDeque<Receipt>,
    watermarks: BTreeMap<(OwnerId, u64, String), u64>,
}
impl Default for AnimationClocks {
    fn default() -> Self {
        Self {
            session: NEXT_SESSION
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .ok(),
            serial: 0,
            epoch: 0,
            revision: 0,
            catalog: 0,
            assets: Default::default(),
            clips: Default::default(),
            owners: Default::default(),
            active: Default::default(),
            records: Default::default(),
            receipts: Default::default(),
            watermarks: Default::default(),
        }
    }
}
impl AnimationClocks {
    fn finish(
        &mut self,
        key: &(OwnerId, AnimationTarget),
        outcome: AnimationOutcome,
        tick: u64,
        reason: &str,
    ) {
        if let Some(playback) = self.active.remove(key) {
            let phase = if playback.clip.looping {
                playback.phase_at(tick)
            } else {
                playback.phase_at(tick).min(playback.clip.duration)
            };
            self.records.push_back(AnimationRecord {
                playback,
                outcome,
                tick,
                phase,
                reason: reason.into(),
            });
            self.revision += 1;
        }
    }
    fn cancel_owner(&mut self, owner: OwnerId, tick: u64, reason: &str) {
        let keys: Vec<_> = self
            .active
            .keys()
            .filter(|(id, _)| *id == owner)
            .cloned()
            .collect();
        for key in keys {
            self.finish(&key, AnimationOutcome::Cancelled, tick, reason);
        }
    }
    fn new_owner(&mut self, owner: OwnerId, life: bool, tick: u64) -> Result<AnimationOwner> {
        let epoch = self
            .epoch
            .checked_add(1)
            .context("Animation epoch exhausted")?;
        let previous_life = if life {
            epoch
        } else {
            self.owners
                .get(&owner)
                .context("Missing animation life")?
                .life
        };
        self.cancel_owner(
            owner,
            tick,
            if life {
                "life replaced"
            } else {
                "body replaced"
            },
        );
        self.epoch = epoch;
        let epochs = AnimationOwner {
            life: previous_life,
            body: epoch,
        };
        self.owners.insert(owner, epochs);
        if life {
            self.watermarks.retain(|(id, _, _), _| *id != owner);
            self.receipts.retain(|r| r.owner != owner);
        }
        Ok(epochs)
    }
    fn prepare(
        &self,
        owner: OwnerId,
        request: AnimationStart,
        clip: AnimationClip,
    ) -> Result<PreparedAnimationStart> {
        ensure!(self.session.is_some(), "Animation identity exhausted");
        ensure!(
            !request.namespace.is_empty()
                && request.namespace.len() <= 128
                && request
                    .namespace
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-.:/".contains(&c))
                && request.nonce > 0
                && request.sequence.len() <= 128
                && request.rate.is_finite()
                && (0.0..=1000.0).contains(&request.rate)
                && request.tween == 0.0,
            "Unsupported animation options"
        );
        let epochs = self.owners.get(&owner).context("Missing animation owner")?;
        ensure!(
            request.binding.owner.as_ref() == Some(epochs),
            "Stale animation owner"
        );
        let nonce_key = (owner, epochs.life, request.namespace.clone());
        if let Some(receipt) = self.receipts.iter().find(|r| {
            r.owner == owner
                && r.request.namespace == request.namespace
                && r.request.nonce == request.nonce
        }) {
            ensure!(
                receipt.request == request && receipt.clip == clip,
                "Animation nonce reused with different request"
            );
            return Ok(PreparedAnimationStart {
                revision: self.revision,
                catalog: self.catalog,
                owner,
                request,
                clip,
                duplicate: Some(receipt.id.clone()),
            });
        }
        ensure!(
            self.watermarks
                .get(&nonce_key)
                .is_none_or(|last| request.nonce > *last),
            "Stale animation nonce"
        );
        ensure!(
            self.watermarks.contains_key(&nonce_key) || self.watermarks.len() < MAX_NONCE_OWNERS,
            "Animation nonce owner budget"
        );
        let key = (owner, request.target.clone());
        ensure!(
            self.serial < u64::MAX
                && self
                    .revision
                    .checked_add(self.active.len() as u64 + 3)
                    .is_some(),
            "Animation identity exhausted"
        );
        ensure!(
            self.active.contains_key(&key) || self.active.len() < MAX_ACTIVE,
            "Animation active budget"
        );
        // Every active entry owns one future terminal record. Replacement uses
        // that reservation; new Started plus its terminal need two more slots.
        ensure!(
            self.records.len() + self.active.len() + 2 <= MAX_RECORDS,
            "Animation result budget"
        );
        Ok(PreparedAnimationStart {
            revision: self.revision,
            catalog: self.catalog,
            owner,
            request,
            clip,
            duplicate: None,
        })
    }
    fn commit(&mut self, prepared: PreparedAnimationStart, tick: u64) -> Result<PlaybackId> {
        ensure!(
            prepared.revision == self.revision && prepared.catalog == self.catalog,
            "Stale animation preparation"
        );
        if let Some(id) = prepared.duplicate {
            return Ok(id);
        }
        let key = (prepared.owner, prepared.request.target.clone());
        self.finish(&key, AnimationOutcome::Interrupted, tick, "replaced");
        self.serial += 1;
        let epochs = prepared
            .request
            .binding
            .owner
            .context("Missing prepared animation owner")?;
        let id = PlaybackId {
            session: self.session.context("Animation session exhausted")?,
            owner: prepared.owner,
            serial: self.serial,
            epochs,
        };
        let playback = ActiveAnimation {
            id: id.clone(),
            request: prepared.request.clone(),
            clip: prepared.clip,
            started_tick: tick,
            anchor_tick: tick,
            phase: 0.0,
            rate: prepared.request.rate,
        };
        self.records.push_back(AnimationRecord {
            playback: playback.clone(),
            outcome: AnimationOutcome::Started,
            tick,
            phase: 0.0,
            reason: String::new(),
        });
        self.watermarks.insert(
            (
                prepared.owner,
                epochs.life,
                prepared.request.namespace.clone(),
            ),
            prepared.request.nonce,
        );
        self.receipts.push_back(Receipt {
            owner: prepared.owner,
            request: prepared.request,
            id: id.clone(),
            clip: playback.clip.clone(),
        });
        if self.receipts.len() > MAX_RECEIPTS {
            self.receipts.pop_front();
        }
        if playback.clip.duration == 0.0 && !playback.clip.looping {
            self.records.push_back(AnimationRecord {
                playback,
                outcome: AnimationOutcome::Natural,
                tick,
                phase: 0.0,
                reason: String::new(),
            });
        } else {
            self.active.insert(key, playback);
        }
        self.revision += 1;
        Ok(id)
    }
}

impl Session {
    pub(super) fn forget_animation_owner(&mut self, owner: OwnerId) {
        self.animations
            .cancel_owner(owner, self.simulation.state().tick, "owner removed");
        self.animations.owners.remove(&owner);
        self.animations
            .watermarks
            .retain(|(id, _, _), _| *id != owner);
        self.animations.receipts.retain(|r| r.owner != owner);
    }
    pub fn set_animation_assets(&mut self, assets: AnimationAssets) -> Result<()> {
        let clips = assets.index()?;
        ensure!(
            self.animations
                .revision
                .checked_add(self.animations.active.len() as u64 + 1)
                .is_some(),
            "Animation revision exhausted"
        );
        for (image, binding) in &assets.images {
            ensure!(
                self.weapons
                    .pack
                    .images
                    .get(image)
                    .is_some_and(|i| i.model == binding.model),
                "Animation catalog/image mismatch"
            );
        }
        let changed_bodies: Vec<_> = self
            .peers
            .iter()
            .filter(|(_, peer)| {
                let model = &self
                    .archetypes
                    .resolve(peer.player.state().archetype)
                    .look
                    .model;
                self.animations
                    .assets
                    .assets
                    .iter()
                    .find(|a| &a.resource == model)
                    .map(|a| &a.digest)
                    != assets
                        .assets
                        .iter()
                        .find(|a| &a.resource == model)
                        .map(|a| &a.digest)
            })
            .map(|(owner, _)| *owner)
            .collect();
        ensure!(
            self.animations
                .epoch
                .checked_add(changed_bodies.len() as u64)
                .is_some()
                && changed_bodies
                    .iter()
                    .all(|owner| self.animation_owner(*owner).is_some()
                        && self.weapons.actor(ActorId(*owner)).is_some()),
            "Animation replacement epoch admission"
        );
        // Atomic rejection keeps legitimate old clocks usable and retains IDs.
        ensure!(
            self.animations.active.values().all(|a| clips
                .get(&(a.clip.resource.clone(), a.clip.sequence.clone()))
                == Some(&a.clip)
                && match a.request.target {
                    AnimationTarget::Pawn(_) => true,
                    _ => a.request.binding.image.as_ref().is_some_and(|image| assets
                        .images
                        .get(&image.image)
                        .is_some_and(|binding| binding.resource == a.clip.resource)),
                }),
            "Animation replacement invalidates active metadata"
        );
        self.animations.catalog = self
            .animations
            .catalog
            .checked_add(1)
            .context("Animation catalog exhausted")?;
        self.animations.assets = assets;
        self.animations.clips = clips;
        self.animations.revision += 1;
        for owner in changed_bodies {
            self.new_animation_body(owner)?;
        }
        Ok(())
    }
    pub fn animation_owner(&self, owner: OwnerId) -> Option<AnimationOwner> {
        self.animations.owners.get(&owner).copied()
    }
    pub(super) fn new_animation_life(&mut self, owner: OwnerId) -> Result<()> {
        let epochs = self
            .animations
            .new_owner(owner, true, self.simulation.state().tick)?;
        if self.weapons.actor(ActorId(owner)).is_some() {
            self.weapons.set_animation_owner(ActorId(owner), epochs)?;
        }
        Ok(())
    }
    pub(super) fn new_animation_body(&mut self, owner: OwnerId) -> Result<()> {
        let epochs = self
            .animations
            .new_owner(owner, false, self.simulation.state().tick)?;
        self.weapons.set_animation_owner(ActorId(owner), epochs)
    }
    fn animation_binding_valid(
        &self,
        owner: OwnerId,
        binding: &AnimationBinding,
        target: &AnimationTarget,
        admission: bool,
    ) -> bool {
        let inventory_clock = matches!(target, AnimationTarget::Weapon(_)) && !admission;
        self.peers.get(&owner).is_some_and(|p| p.combat.alive)
            && binding.owner == self.animation_owner(owner)
            && binding.item.as_ref().is_none_or(|i| {
                self.weapons.animation_item(ActorId(owner), i.slot).as_ref() == Some(i)
                    && (inventory_clock
                        || self
                            .weapons
                            .actor(ActorId(owner))
                            .is_some_and(|a| a.selected == Some(i.slot)))
            })
            && (inventory_clock
                || binding.image.as_ref().is_none_or(|i| {
                    self.weapons
                        .animation_image(ActorId(owner), i.hand, i.emote)
                        .as_ref()
                        == Some(i)
                }))
    }
    fn animation_clip(&self, owner: OwnerId, request: &AnimationStart) -> Result<AnimationClip> {
        ensure!(
            self.animation_binding_valid(owner, &request.binding, &request.target, true),
            "Stale animation binding"
        );
        let (resource, sequence) = match request.target {
            AnimationTarget::Pawn(channel) => {
                ensure!(channel < 32, "Unsupported pawn channel");
                let peer = self.peers.get(&owner).context("Missing animation peer")?;
                let look = &self.archetypes.resolve(peer.player.state().archetype).look;
                let resource = look.model.clone();
                let roles = RoleClips::resolve(&look.animations, |name| {
                    self.animations
                        .clips
                        .contains_key(&(resource.clone(), name.to_ascii_lowercase()))
                });
                let sequence = Role::from_blockhead_clip(&request.sequence)
                    .and_then(|role| roles.clip(role).map(|(name, _)| name.to_owned()))
                    .unwrap_or_else(|| request.sequence.clone());
                (resource, sequence)
            }
            AnimationTarget::Weapon(slot) => {
                ensure!(
                    request
                        .binding
                        .item
                        .as_ref()
                        .is_some_and(|i| i.slot == slot),
                    "Missing weapon instance"
                );
                let image = request
                    .binding
                    .image
                    .as_ref()
                    .context("Weapon has no actual image")?;
                let asset = self
                    .animations
                    .assets
                    .images
                    .get(&image.image)
                    .context("Unsupported weapon animation metadata")?;
                ensure!(
                    self.weapons
                        .pack
                        .images
                        .get(&image.image)
                        .is_some_and(|i| i.model == asset.model),
                    "Weapon animation model changed"
                );
                (asset.resource.clone(), request.sequence.clone())
            }
            AnimationTarget::Image { hand, emote } => {
                let image = request
                    .binding
                    .image
                    .as_ref()
                    .filter(|i| i.hand == hand && i.emote == emote)
                    .context("Missing image incarnation")?;
                let asset = self
                    .animations
                    .assets
                    .images
                    .get(&image.image)
                    .context("Unsupported image animation metadata")?;
                ensure!(
                    self.weapons
                        .pack
                        .images
                        .get(&image.image)
                        .is_some_and(|i| i.model == asset.model),
                    "Image animation model changed"
                );
                (asset.resource.clone(), request.sequence.clone())
            }
        };
        self.animations
            .clips
            .get(&(resource, sequence.to_ascii_lowercase()))
            .cloned()
            .context("Unsupported native animation clip")
    }
    pub fn prepare_animation_start(
        &self,
        owner: OwnerId,
        request: AnimationStart,
    ) -> Result<PreparedAnimationStart> {
        let clip = self.animation_clip(owner, &request)?;
        self.animations.prepare(owner, request, clip)
    }
    pub fn commit_animation_start(
        &mut self,
        prepared: PreparedAnimationStart,
    ) -> Result<PlaybackId> {
        ensure!(
            self.animation_clip(prepared.owner, &prepared.request)? == prepared.clip,
            "Changed animation binding"
        );
        self.animations
            .commit(prepared, self.simulation.state().tick)
    }
    pub fn animation_view(&self) -> AnimationView {
        AnimationView {
            tick: self.simulation.state().tick,
            revision: self.animations.revision,
            active: self.animations.active.values().cloned().collect(),
        }
    }
    pub fn take_animation_records(&mut self) -> Vec<AnimationRecord> {
        self.animations.records.drain(..).collect()
    }
    pub fn cancel_animation(&mut self, id: &PlaybackId) -> Result<()> {
        let key = self
            .animations
            .active
            .iter()
            .find(|(_, a)| &a.id == id)
            .map(|(key, _)| key.clone())
            .context("Stale animation id")?;
        self.animations.finish(
            &key,
            AnimationOutcome::Cancelled,
            self.simulation.state().tick,
            "host cancelled",
        );
        Ok(())
    }
    pub fn set_animation_rate(&mut self, id: &PlaybackId, rate: f64) -> Result<()> {
        ensure!(
            rate.is_finite() && (0.0..=1000.0).contains(&rate),
            "Unsupported animation rate"
        );
        ensure!(
            self.animations
                .revision
                .checked_add(self.animations.active.len() as u64 + 1)
                .is_some(),
            "Animation revision exhausted"
        );
        let active = self
            .animations
            .active
            .values_mut()
            .find(|a| &a.id == id)
            .context("Stale animation id")?;
        let tick = self.simulation.state().tick;
        active.phase = active.phase_at(tick);
        active.anchor_tick = tick;
        active.rate = rate;
        self.animations.revision += 1;
        Ok(())
    }
    pub(super) fn reconcile_animation_bindings(&mut self) {
        let stale: Vec<_> = self
            .animations
            .active
            .iter()
            .filter(|((owner, _), a)| {
                !self.animation_binding_valid(*owner, &a.request.binding, &a.request.target, false)
            })
            .map(|(k, _)| k.clone())
            .collect();
        let tick = self.simulation.state().tick;
        for key in stale {
            self.animations.finish(
                &key,
                AnimationOutcome::Cancelled,
                tick,
                "binding invalidated",
            );
        }
    }
    pub(super) fn advance_animations(&mut self) {
        self.reconcile_animation_bindings();
        let tick = self.simulation.state().tick;
        let ended: Vec<_> = self
            .animations
            .active
            .iter()
            .filter(|(_, a)| !a.clip.looping && a.phase_at(tick) >= a.clip.duration)
            .map(|(key, _)| key.clone())
            .collect();
        for key in ended {
            self.animations
                .finish(&key, AnimationOutcome::Natural, tick, "");
        }
    }
    pub(super) fn native_animation(
        &mut self,
        owner: OwnerId,
        thread: u8,
        sequence: &str,
        image_hand: Option<u8>,
        provenance: Option<AnimationProvenance>,
    ) {
        self.reconcile_animation_bindings();
        let target = image_hand.map_or(AnimationTarget::Pawn(thread), |hand| {
            AnimationTarget::Image { hand, emote: false }
        });
        let (namespace, nonce, binding) = if let Some(p) = provenance {
            ("native-weapon", p.serial, p.binding)
        } else {
            let Some(epochs) = self.animation_owner(owner) else {
                return;
            };
            let Some(nonce) = self.animations.serial.checked_add(1) else {
                return;
            };
            (
                "native-body",
                nonce,
                AnimationBinding {
                    owner: Some(epochs),
                    ..Default::default()
                },
            )
        };
        if !self.animation_binding_valid(owner, &binding, &target, true) {
            let count = self
                .weapon_gaps
                .entry("animation.stale_producer".into())
                .or_default();
            *count = count.saturating_add(1);
            return;
        }
        if sequence.eq_ignore_ascii_case("root") && image_hand.is_none() {
            self.animations.finish(
                &(owner, target),
                AnimationOutcome::Cancelled,
                self.simulation.state().tick,
                "legacy root",
            );
            return;
        }
        let request = AnimationStart {
            namespace: namespace.into(),
            nonce,
            target,
            sequence: sequence.into(),
            binding,
            rate: 1.0,
            tween: 0.0,
        };
        let result = self
            .prepare_animation_start(owner, request)
            .and_then(|p| self.commit_animation_start(p));
        if result.is_err() {
            let count = self
                .weapon_gaps
                .entry("animation.unsupported".into())
                .or_default();
            *count = count.saturating_add(1);
        }
    }
}
