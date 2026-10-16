//! Host-only native playback state. Rendering and package policy are consumers.
use super::*;
use bri_content::{
    avatar::Rig,
    body::{Role, RoleClips},
    shape::Shape,
};
use bri_weapons::{ActorId, AnimationBinding, AnimationImage, AnimationOwner, AnimationProvenance};
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
struct ValidatedAnimationAssets {
    clips: ClipIndex,
    rigs: BTreeMap<String, super::body_pose::NativePoseRig>,
}
impl AnimationAssets {
    /// Lets bounded loaders retain supported resources without making optional
    /// missing clock metadata a failure of legacy hosting.
    pub fn within_resource_budget(count: usize, bytes: usize) -> bool {
        count <= MAX_ASSETS && bytes <= MAX_TOTAL_BYTES
    }
    fn index(&self) -> Result<ValidatedAnimationAssets> {
        ensure!(
            self.assets.len() <= MAX_ASSETS && self.images.len() <= 4096,
            "Animation catalog bounds"
        );
        let mut total = 0usize;
        let mut resources = BTreeSet::new();
        let mut clips = BTreeMap::new();
        let mut rigs = BTreeMap::new();
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
            let rig = if asset.rig {
                let rig: Rig = serde_json::from_slice(&asset.bytes)?;
                rig.validate()?;
                rig
            } else {
                let shape: Shape = serde_json::from_slice(&asset.bytes)?;
                shape.validate()?;
                let mut sequences = BTreeMap::new();
                for animation in &shape.animations {
                    let name = animation.name.to_ascii_lowercase();
                    ensure!(
                        !name.is_empty() && sequences.insert(name, animation.clone()).is_none(),
                        "Ambiguous animation alias"
                    );
                }
                // Shape validation admits bind-only bodies. Full-avatar Rig
                // validation separately requires nonempty sequence metadata.
                Rig {
                    schema_version: 1,
                    id: asset.resource.clone(),
                    shape,
                    sequences,
                    sources: vec![],
                    omissions: vec![],
                }
            };
            let sequences = &rig.sequences;
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
                        sequence: name.clone(),
                        duration: f64::from(animation.duration),
                        looping: animation.looping,
                    },
                );
            }
            rigs.insert(
                asset.resource.clone(),
                super::body_pose::NativePoseRig {
                    digest: asset.digest.clone(),
                    rig: Arc::new(rig),
                },
            );
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
        Ok(ValidatedAnimationAssets { clips, rigs })
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
/// An unchanged playback anchor together with clock phase at the snapshot tick.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedActiveAnimation {
    pub playback: ActiveAnimation,
    /// Elapsed native seconds, before the sampler's loop or final-key clamp.
    pub evaluated_phase: f64,
}
/// Read-only active playback state evaluated at the current Session tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvaluatedAnimationView {
    pub tick: u64,
    /// Mutation revision, not a cache key for phases: time can advance alone.
    pub revision: u64,
    pub active: Vec<EvaluatedActiveAnimation>,
}
/// Trusted-host opt-in only; existing producers use NoHold.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AnimationPoseRetention {
    #[default]
    NoHold,
    HoldNativeFinal,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationPoseApplicability {
    pub body: Arc<super::body_pose::BodyPoseAssetInfo>,
    /// Ordinary slots, including worn 2/3; the separate emote store is excluded.
    pub images: [Option<AnimationImage>; bri_weapons::IMAGE_SLOTS],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPoseState {
    Playing,
    HeldFinal,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationPose {
    pub playback: ActiveAnimation,
    pub phase: f64,
    pub state: AnimationPoseState,
    pub applicability: AnimationPoseApplicability,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimationPoseView {
    pub tick: u64,
    /// Clock mutations only; current applicability must also be observed.
    pub revision: u64,
    pub poses: Vec<AnimationPose>,
}
fn pose_image_identity(
    hand: u8,
    occupied: Option<&str>,
    identity: Option<AnimationImage>,
) -> Result<Option<AnimationImage>> {
    match (occupied, identity) {
        (None, None) => Ok(None),
        (Some(image), Some(identity))
            if identity.hand == hand
                && !identity.emote
                && identity.image == image
                && identity.generation > 0 =>
        {
            Ok(Some(identity))
        }
        _ => anyhow::bail!("Unsupported native pose mount identity"),
    }
}
pub struct PreparedAnimationStart {
    revision: u64,
    catalog: u64,
    owner: OwnerId,
    request: AnimationStart,
    clip: AnimationClip,
    duplicate: Option<PlaybackId>,
    retention: AnimationPoseRetention,
    applicability: Option<AnimationPoseApplicability>,
}
struct Receipt {
    owner: OwnerId,
    request: AnimationStart,
    id: PlaybackId,
    clip: AnimationClip,
    retention: AnimationPoseRetention,
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
    pose_contexts: BTreeMap<(OwnerId, AnimationTarget), AnimationPoseApplicability>,
    held: BTreeMap<(OwnerId, AnimationTarget), AnimationPose>,
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
            pose_contexts: Default::default(),
            held: Default::default(),
            records: Default::default(),
            receipts: Default::default(),
            watermarks: Default::default(),
        }
    }
}
impl AnimationClocks {
    fn occupancy(&self) -> usize {
        self.active.len() + self.held.len()
    }
    // Every occupied key can be cleared. An opted-in active entry additionally
    // needs one revision for Natural -> held, before its eventual clear.
    fn revision_reserve(&self) -> u64 {
        (self.occupancy() + self.pose_contexts.len()) as u64
    }
    fn check_revision(&self, extra: u64) -> Result<()> {
        ensure!(
            self.revision
                .checked_add(self.revision_reserve())
                .and_then(|n| n.checked_add(extra))
                .is_some(),
            "Animation revision exhausted"
        );
        Ok(())
    }
    fn bump_revision(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("reserved animation revision");
        debug_assert!(self.revision.checked_add(self.revision_reserve()).is_some());
    }
    fn start_budget(&self, key: &(OwnerId, AnimationTarget)) -> Result<()> {
        ensure!(self.serial < u64::MAX, "Animation identity exhausted");
        self.check_revision(3)?;
        ensure!(
            self.active.contains_key(key)
                || self.held.contains_key(key)
                || self.occupancy() < MAX_ACTIVE,
            "Animation active/held occupancy budget"
        );
        ensure!(
            self.records.len() + self.active.len() + 2 <= MAX_RECORDS,
            "Animation result budget"
        );
        Ok(())
    }
    fn finish(
        &mut self,
        key: &(OwnerId, AnimationTarget),
        outcome: AnimationOutcome,
        tick: u64,
        reason: &str,
    ) {
        if let Some(playback) = self.active.remove(key) {
            let applicability = self.pose_contexts.remove(key);
            let phase = if playback.clip.looping {
                playback.phase_at(tick)
            } else {
                playback.phase_at(tick).min(playback.clip.duration)
            };
            if outcome == AnimationOutcome::Natural
                && let Some(applicability) = applicability
            {
                self.held.insert(
                    key.clone(),
                    AnimationPose {
                        playback: playback.clone(),
                        phase,
                        state: AnimationPoseState::HeldFinal,
                        applicability,
                    },
                );
            }
            self.records.push_back(AnimationRecord {
                playback,
                outcome,
                tick,
                phase,
                reason: reason.into(),
            });
            self.bump_revision();
        } else if outcome != AnimationOutcome::Natural && self.held.remove(key).is_some() {
            self.bump_revision();
        }
    }
    fn cancel_owner(&mut self, owner: OwnerId, tick: u64, reason: &str) {
        let keys: Vec<_> = self
            .active
            .keys()
            .chain(self.held.keys())
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
        retention: AnimationPoseRetention,
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
                receipt.request == request
                    && receipt.clip == clip
                    && receipt.retention == retention,
                "Animation nonce reused with different request"
            );
            return Ok(PreparedAnimationStart {
                revision: self.revision,
                catalog: self.catalog,
                owner,
                request,
                clip,
                duplicate: Some(receipt.id.clone()),
                retention,
                applicability: None,
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
        // Every active entry owns one future terminal record. Replacement uses
        // that reservation; new Started plus its terminal need two more slots.
        self.start_budget(&key)?;
        Ok(PreparedAnimationStart {
            revision: self.revision,
            catalog: self.catalog,
            owner,
            request,
            clip,
            duplicate: None,
            retention,
            applicability: None,
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
        self.start_budget(&key)?;
        let epochs = prepared
            .request
            .binding
            .owner
            .context("Missing prepared animation owner")?;
        let session = self.session.context("Animation session exhausted")?;
        ensure!(
            (prepared.retention == AnimationPoseRetention::HoldNativeFinal)
                == prepared.applicability.is_some(),
            "Missing prepared pose applicability"
        );
        self.finish(&key, AnimationOutcome::Interrupted, tick, "replaced");
        self.serial += 1;
        let id = PlaybackId {
            session,
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
            retention: prepared.retention,
        });
        if self.receipts.len() > MAX_RECEIPTS {
            self.receipts.pop_front();
        }
        if playback.clip.duration == 0.0 && !playback.clip.looping {
            if let Some(applicability) = prepared.applicability {
                self.held.insert(
                    key,
                    AnimationPose {
                        playback: playback.clone(),
                        phase: 0.0,
                        state: AnimationPoseState::HeldFinal,
                        applicability,
                    },
                );
            }
            self.records.push_back(AnimationRecord {
                playback,
                outcome: AnimationOutcome::Natural,
                tick,
                phase: 0.0,
                reason: String::new(),
            });
        } else {
            if let Some(applicability) = prepared.applicability {
                self.pose_contexts.insert(key.clone(), applicability);
            }
            self.active.insert(key, playback);
        }
        self.bump_revision();
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
        let bindings = self.body_poses.retained_bindings(&assets);
        self.set_body_pose_assets(assets, bindings)
    }
    /// One verified catalog transaction for clocks and retained native body inputs.
    /// Failed preparation leaves every existing generation and entry intact.
    pub fn set_body_pose_assets(
        &mut self,
        assets: AnimationAssets,
        bindings: BodyPoseBindings,
    ) -> Result<()> {
        let validated = assets.index()?;
        let body_poses =
            super::body_pose::BodyPoseInputs::prepare(validated.rigs, bindings, &self.archetypes)?;
        let clips = validated.clips;
        self.animations.check_revision(1)?;
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
        ensure!(
            self.animations.held.values().all(|pose| clips.get(&(
                pose.playback.clip.resource.clone(),
                pose.playback.clip.sequence.clone()
            )) == Some(&pose.playback.clip))
                && self
                    .animations
                    .pose_contexts
                    .iter()
                    .map(|(key, context)| (key.0, context))
                    .chain(
                        self.animations
                            .held
                            .iter()
                            .map(|(key, pose)| (key.0, &pose.applicability))
                    )
                    .all(
                        |(owner, context)| self.peers.get(&owner).is_some_and(|peer| {
                            self.archetypes
                                .get(peer.player.state().archetype)
                                .and_then(|definition| body_poses.info(definition).ok())
                                .is_some_and(|incoming| incoming == context.body)
                        })
                    ),
            "Animation replacement invalidates pose metadata"
        );
        self.animations.catalog = self
            .animations
            .catalog
            .checked_add(1)
            .context("Animation catalog exhausted")?;
        self.animations.assets = assets;
        self.body_poses = body_poses;
        self.animations.clips = clips;
        self.animations.bump_revision();
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
    /// Atomically admit a finite native weapon clock and optional current-weapon
    /// writes. Trusted host only: package authorization/state publication and
    /// source completion policy are separate responsibilities.
    ///
    /// Admission requires the selected ordinary right-hand instance. Afterward
    /// the existing Weapon clock retains its inventory-custody lifetime. No
    /// callback runs here; image-state entry remains deferred to weapon advance.
    pub fn start_weapon_animation_with_patch(
        &mut self,
        owner: OwnerId,
        request: AnimationStart,
        expected_rounds: Option<u32>,
        rounds: Option<u32>,
        image_state: Option<&str>,
    ) -> Result<PlaybackId> {
        let slot = match &request.target {
            AnimationTarget::Weapon(slot) => *slot,
            _ => anyhow::bail!("Weapon patch requires a weapon target"),
        };
        ensure!(
            request
                .binding
                .item
                .as_ref()
                .is_some_and(|item| item.slot == slot)
                && request
                    .binding
                    .image
                    .as_ref()
                    .is_some_and(|image| image.hand == 0 && !image.emote)
                && request.rate == 1.0
                && request.tween == 0.0,
            "Unsupported weapon patch animation options"
        );
        ensure!(
            !self
                .animations
                .active
                .contains_key(&(owner, request.target.clone())),
            "Weapon animation target is occupied"
        );
        let clip = self.animation_clip(owner, &request)?;
        ensure!(!clip.looping, "Weapon patch requires a finite native clip");
        let tick = self.simulation.state().tick;
        let (animations, weapons) = (&mut self.animations, &mut self.weapons);
        let patch = weapons.prepare_weapon_patch(
            ActorId(owner),
            &request.binding,
            expected_rounds,
            rounds,
            image_state,
        )?;
        let prepared = animations.prepare(owner, request, clip, AnimationPoseRetention::NoHold)?;
        ensure!(
            prepared.duplicate.is_none(),
            "Duplicate weapon patch animation action"
        );
        let id = animations.commit(prepared, tick)?;
        patch.commit();
        Ok(id)
    }

    pub fn prepare_animation_start(
        &self,
        owner: OwnerId,
        request: AnimationStart,
    ) -> Result<PreparedAnimationStart> {
        self.prepare_animation_start_with_pose(owner, request, AnimationPoseRetention::NoHold)
    }
    /// Trusted-host opt-in; existing serialized requests/producers remain NoHold.
    pub fn prepare_animation_start_with_pose(
        &self,
        owner: OwnerId,
        request: AnimationStart,
        retention: AnimationPoseRetention,
    ) -> Result<PreparedAnimationStart> {
        ensure!(
            retention == AnimationPoseRetention::NoHold
                || request.target == AnimationTarget::Pawn(2),
            "Unsupported pose retention target"
        );
        let clip = self.animation_clip(owner, &request)?;
        let mut prepared = self.animations.prepare(owner, request, clip, retention)?;
        if retention == AnimationPoseRetention::HoldNativeFinal && prepared.duplicate.is_none() {
            prepared.applicability =
                Some(self.animation_pose_applicability(owner, &prepared.clip)?);
        }
        Ok(prepared)
    }
    fn animation_pose_applicability(
        &self,
        owner: OwnerId,
        clip: &AnimationClip,
    ) -> Result<AnimationPoseApplicability> {
        let peer = self.peers.get(&owner).context("Missing pose owner")?;
        let body = self.body_pose_asset_info(peer.player.state().archetype)?;
        ensure!(
            body.resource == clip.resource && body.digest == clip.digest,
            "Pose clip does not belong to actual body inputs"
        );
        let mut images = std::array::from_fn(|_| None);
        for (hand, image) in images.iter_mut().enumerate() {
            *image = pose_image_identity(
                hand as u8,
                self.weapons.image_id(ActorId(owner), hand as u8),
                self.weapons
                    .animation_image(ActorId(owner), hand as u8, false),
            )?;
        }
        Ok(AnimationPoseApplicability { body, images })
    }
    fn animation_pose_valid(
        &self,
        playback: &ActiveAnimation,
        context: &AnimationPoseApplicability,
    ) -> bool {
        self.animation_binding_valid(
            playback.id.owner,
            &playback.request.binding,
            &playback.request.target,
            false,
        ) && self
            .animation_pose_applicability(playback.id.owner, &playback.clip)
            .is_ok_and(|current| current == *context)
    }
    pub fn commit_animation_start(
        &mut self,
        prepared: PreparedAnimationStart,
    ) -> Result<PlaybackId> {
        ensure!(
            self.animation_clip(prepared.owner, &prepared.request)? == prepared.clip,
            "Changed animation binding"
        );
        if let Some(context) = &prepared.applicability {
            ensure!(
                self.animation_pose_applicability(prepared.owner, &prepared.clip)
                    .is_ok_and(|current| current == *context),
                "Changed pose applicability"
            );
        }
        self.animations
            .commit(prepared, self.simulation.state().tick)
    }
    /// Opted-in state only. Applicability is checked now, independently of the
    /// clock revision; querying never clears entries or consumes result records.
    pub fn animation_pose_view(&self) -> AnimationPoseView {
        let tick = self.simulation.state().tick;
        let mut poses: Vec<_> = self
            .animations
            .pose_contexts
            .iter()
            .filter_map(|(key, applicability)| {
                let playback = self.animations.active.get(key)?;
                self.animation_pose_valid(playback, applicability)
                    .then(|| AnimationPose {
                        playback: playback.clone(),
                        phase: playback.phase_at(tick),
                        state: AnimationPoseState::Playing,
                        applicability: applicability.clone(),
                    })
            })
            .chain(
                self.animations
                    .held
                    .values()
                    .filter(|pose| self.animation_pose_valid(&pose.playback, &pose.applicability))
                    .cloned(),
            )
            .collect();
        poses.sort_by(|a, b| {
            (a.playback.id.owner, &a.playback.request.target)
                .cmp(&(b.playback.id.owner, &b.playback.request.target))
        });
        AnimationPoseView {
            tick,
            revision: self.animations.revision,
            poses,
        }
    }
    /// Clear an actual opted-in active or held ID, never a replacement's pose.
    /// A held clear has no terminal outcome; cancel_animation stays active-only.
    pub fn clear_animation_pose(&mut self, id: &PlaybackId) -> Result<()> {
        let key = (id.owner, AnimationTarget::Pawn(2));
        ensure!(
            self.animations.pose_contexts.contains_key(&key)
                && self
                    .animations
                    .active
                    .get(&key)
                    .is_some_and(|a| &a.id == id)
                || self
                    .animations
                    .held
                    .get(&key)
                    .is_some_and(|pose| &pose.playback.id == id),
            "Stale retained pose id"
        );
        self.animations.finish(
            &key,
            AnimationOutcome::Cancelled,
            self.simulation.state().tick,
            "host cleared pose",
        );
        Ok(())
    }
    pub fn animation_view(&self) -> AnimationView {
        AnimationView {
            tick: self.simulation.state().tick,
            revision: self.animations.revision,
            active: self.animations.active.values().cloned().collect(),
        }
    }
    /// Evaluates existing clocks without changing anchors or consuming records.
    /// Completed playbacks are absent; this query does not retain final poses.
    pub fn evaluated_animation_view(&self) -> EvaluatedAnimationView {
        let tick = self.simulation.state().tick;
        EvaluatedAnimationView {
            tick,
            revision: self.animations.revision,
            active: self
                .animations
                .active
                .values()
                .map(|playback| EvaluatedActiveAnimation {
                    playback: playback.clone(),
                    evaluated_phase: playback.phase_at(tick),
                })
                .collect(),
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
        self.animations.check_revision(1)?;
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
        self.animations.bump_revision();
        Ok(())
    }
    pub(super) fn reconcile_animation_bindings(&mut self) {
        let stale: Vec<_> = self
            .animations
            .active
            .iter()
            .filter(|((owner, _), a)| {
                !self.animation_binding_valid(*owner, &a.request.binding, &a.request.target, false)
                    || self
                        .animations
                        .pose_contexts
                        .get(&(*owner, a.request.target.clone()))
                        .is_some_and(|context| !self.animation_pose_valid(a, context))
            })
            .map(|(k, _)| k.clone())
            .chain(
                self.animations
                    .held
                    .iter()
                    .filter(|(_, pose)| {
                        !self.animation_pose_valid(&pose.playback, &pose.applicability)
                    })
                    .map(|(key, _)| key.clone()),
            )
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

#[cfg(test)]
mod retention_tests {
    use super::*;

    fn native_session() -> Result<(Session, AnimationAssets, BodyPoseBindings, OwnerId)> {
        let rig = bri_content::testing::avatar::rig();
        let bytes = serde_json::to_vec(&rig)?;
        let assets = AnimationAssets {
            assets: vec![AnimationAsset {
                resource: "v20.shape.m".into(),
                digest: bri_package::environment::hash_bytes(&bytes),
                rig: true,
                bytes,
            }],
            images: Default::default(),
        };
        let bindings = BodyPoseBindings {
            reference_resource: None,
            bodies: vec![BodyPoseBinding {
                model: "v20.shape.m".into(),
                resource: "v20.shape.m".into(),
                fitted: false,
            }],
        };
        let mut session = Session::new(Simulation::new(
            World::new("Revision reserve".into(), "test".into(), vec![[1.; 4]]),
            crate::definitions::Definitions::default(),
            vec![],
        )?);
        session.set_body_pose_assets(assets.clone(), bindings.clone())?;
        let owner = session.join("Ledger owner".into(), Vec3::ZERO, true)?;
        Ok((session, assets, bindings, owner))
    }
    fn request(session: &Session, owner: OwnerId, nonce: u64, channel: u8) -> AnimationStart {
        AnimationStart {
            namespace: "revision-reserve".into(),
            nonce,
            target: AnimationTarget::Pawn(channel),
            sequence: "activate2".into(),
            binding: AnimationBinding {
                owner: session.animation_owner(owner),
                ..Default::default()
            },
            rate: 1.,
            tween: 0.,
        }
    }
    fn hold(session: &mut Session, owner: OwnerId) -> Result<PlaybackId> {
        let ticket = session.prepare_animation_start_with_pose(
            owner,
            request(session, owner, 1, 2),
            AnimationPoseRetention::HoldNativeFinal,
        )?;
        session.commit_animation_start(ticket)
    }

    #[test]
    fn revision_reserve_covers_future_natural_then_held_clear_and_rejects_other_mutations()
    -> Result<()> {
        let (mut session, assets, bindings, owner) = native_session()?;
        let id = hold(&mut session, owner)?;
        assert_eq!(session.animations.revision_reserve(), 2);
        session.animations.revision = u64::MAX - 2;
        let before = session.animation_view();
        assert!(session.set_animation_rate(&id, 2.).is_err());
        assert!(
            session
                .prepare_animation_start(owner, request(&session, owner, 2, 3))
                .is_err()
        );
        assert!(session.set_body_pose_assets(assets, bindings).is_err());
        assert_eq!(session.animation_view(), before);
        session.animations.finish(
            &(owner, AnimationTarget::Pawn(2)),
            AnimationOutcome::Natural,
            u64::MAX,
            "",
        );
        assert_eq!(session.animations.revision, u64::MAX - 1);
        assert_eq!(session.animations.revision_reserve(), 1);
        assert_eq!(
            session.animation_pose_view().poses[0].state,
            AnimationPoseState::HeldFinal
        );
        session.clear_animation_pose(&id)?;
        assert_eq!(session.animations.revision, u64::MAX);
        assert_eq!(session.animations.revision_reserve(), 0);
        assert_eq!(
            session
                .take_animation_records()
                .iter()
                .map(|r| r.outcome)
                .collect::<Vec<_>>(),
            [AnimationOutcome::Started, AnimationOutcome::Natural]
        );
        Ok(())
    }

    #[test]
    fn bulk_body_owner_cleanup_consumes_reserved_headroom_without_held_terminals() -> Result<()> {
        let (mut session, assets, bindings, owner) = native_session()?;
        hold(&mut session, owner)?;
        let other = session.join("Other ledger owner".into(), Vec3::X * 4., true)?;
        hold(&mut session, other)?;
        session.animations.finish(
            &(other, AnimationTarget::Pawn(2)),
            AnimationOutcome::Natural,
            u64::MAX,
            "",
        );
        let ticket = session.prepare_animation_start(owner, request(&session, owner, 2, 3))?;
        session.commit_animation_start(ticket)?;
        assert_eq!(session.animations.revision_reserve(), 4);
        session.animations.revision = u64::MAX - 4;
        session.new_animation_body(owner)?;
        session.forget_animation_owner(other);
        assert_eq!(session.animations.occupancy(), 0);
        assert_eq!(session.animations.revision_reserve(), 0);
        assert_eq!(session.animations.revision, u64::MAX - 1);
        session.set_body_pose_assets(assets, bindings)?;
        assert_eq!(session.animations.revision, u64::MAX);
        let records = session.take_animation_records();
        assert_eq!(
            records
                .iter()
                .filter(|r| r.outcome == AnimationOutcome::Cancelled)
                .count(),
            2
        );
        assert_eq!(
            records
                .iter()
                .filter(|r| r.outcome == AnimationOutcome::Natural)
                .count(),
            1
        );
        Ok(())
    }

    #[test]
    fn occupied_mount_without_identity_is_unsupported_and_emote_three_is_distinct() {
        // The unavailable-generation branch cannot be reached through ordinary
        // Session controls without exhausting the runtime's private u64 serial.
        // Exercise the exact validator used by its actual slot reads, no hook.
        assert!(pose_image_identity(1, Some("test:image/left"), None).is_err());
        assert_eq!(pose_image_identity(1, None, None).unwrap(), None);
        let ordinary = AnimationImage {
            hand: 3,
            generation: 1,
            image: "test:image/worn".into(),
            emote: false,
        };
        assert_eq!(
            pose_image_identity(3, Some(&ordinary.image), Some(ordinary.clone())).unwrap(),
            Some(ordinary.clone())
        );
        let mut emote = ordinary.clone();
        emote.emote = true;
        assert!(pose_image_identity(3, Some(&ordinary.image), Some(emote)).is_err());
    }
}
