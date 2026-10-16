//! Retained native body inputs only; no actor pose clocks or gameplay origins.
use super::{AnimationAssets, Session};
use crate::archetype::{Archetype, ArchetypeId, Archetypes};
use anyhow::{Context, Result, ensure};
use bri_content::{
    avatar::Rig,
    body::RoleClips,
    body_pose::{BodyFitInput, BodyFits, build_body_fits},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

/// Native provider facts; role/socket policy belongs to the actual archetype.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyPoseBinding {
    pub model: String,
    pub resource: String,
    pub fitted: bool,
}

/// The existing avatar reference is explicit and independent of actor definitions.
/// Without it, reference seat-hip fitting remains absent; no rig is guessed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BodyPoseBindings {
    pub reference_resource: Option<String>,
    pub bodies: Vec<BodyPoseBinding>,
}

/// Read-only native registration receipt, never a live actor pose or wire state.
#[derive(Debug, PartialEq)]
pub struct BodyPoseAssetInfo {
    pub definition: Archetype,
    pub resource: String,
    pub digest: String,
    pub reference_resource: Option<String>,
    pub reference_digest: Option<String>,
    pub fitted: bool,
    pub node_count: usize,
    pub hands: [Option<usize>; 2],
    pub eye: Option<usize>,
    pub roles: RoleClips,
    pub fits: BodyFits,
}

pub(super) struct NativePoseRig {
    pub digest: String,
    pub rig: Arc<Rig>,
}

#[derive(Default)]
pub(super) struct BodyPoseInputs {
    input: BodyPoseBindings,
    rigs: BTreeMap<String, NativePoseRig>,
    bindings: BTreeMap<String, BodyPoseBinding>,
    // Each entry retains the full actual definition, never just model/preserve.
    resolved: BTreeMap<String, Arc<BodyPoseAssetInfo>>,
}
impl BodyPoseInputs {
    pub(super) fn prepare(
        rigs: BTreeMap<String, NativePoseRig>,
        input: BodyPoseBindings,
        archetypes: &Archetypes,
    ) -> Result<Self> {
        ensure!(
            AnimationAssets::within_resource_budget(input.bodies.len(), 0),
            "Body pose binding budget"
        );
        if let Some(reference) = &input.reference_resource {
            ensure!(
                rigs.contains_key(reference),
                "Unknown body pose reference resource"
            );
        }
        let mut bindings = BTreeMap::new();
        for binding in &input.bodies {
            ensure!(
                !binding.model.is_empty()
                    && binding.model.len() <= 256
                    && rigs.contains_key(&binding.resource)
                    && bindings
                        .insert(binding.model.clone(), binding.clone())
                        .is_none(),
                "Invalid/duplicate body pose binding"
            );
        }
        let mut prepared = Self {
            input,
            rigs,
            bindings,
            resolved: BTreeMap::new(),
        };
        for (_, definition) in archetypes.iter() {
            if prepared.bindings.contains_key(&definition.look.model) {
                let info = prepared.resolve(definition)?;
                prepared.resolved.insert(definition.id.clone(), info);
            }
        }
        Ok(prepared)
    }
    // Legacy metadata replacement keeps only still-bound resources. Removing the
    // explicit reference leaves it absent, without choosing another native rig.
    pub(super) fn retained_bindings(&self, assets: &AnimationAssets) -> BodyPoseBindings {
        let has = |resource: &str| assets.assets.iter().any(|a| a.resource == resource);
        BodyPoseBindings {
            reference_resource: self.input.reference_resource.clone().filter(|r| has(r)),
            bodies: self
                .input
                .bodies
                .iter()
                .filter(|b| has(&b.resource))
                .cloned()
                .collect(),
        }
    }
    fn resolve(&self, definition: &Archetype) -> Result<Arc<BodyPoseAssetInfo>> {
        let binding = self.bindings.get(&definition.look.model).with_context(|| {
            format!(
                "{}: native body pose inputs unsupported for {}",
                definition.id, definition.look.model
            )
        })?;
        let native = self
            .rigs
            .get(&binding.resource)
            .context("Missing retained body rig")?;
        let rig = &native.rig;
        let node = |name: &str| {
            rig.shape
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(name))
        };
        let hands = [0, 1].map(|n| {
            definition
                .sockets
                .hand(n)
                .and_then(|s| s.node)
                .and_then(|n| node(&n))
        });
        let eye = definition.sockets.eye().node.as_deref().and_then(node);
        let roles = RoleClips::resolve(&definition.look.animations, |n| rig.sequence(n).is_some());
        let reference = self
            .input
            .reference_resource
            .as_ref()
            .and_then(|r| self.rigs.get(r));
        let reference_seat_hip =
            reference.and_then(|r| bri_content::hold::sit_hip(&r.rig.shape, r.rig.sequence("sit")));
        let fits = build_body_fits(BodyFitInput {
            rig,
            roles: &roles,
            fitted: binding.fitted,
            stand_in: false,
            preserve_hand_pose: definition.sockets.preserve_hand_pose,
            hands,
            // Match Avatar's fitting landmark, independently of the eye socket.
            eye: node("eye"),
            reference_seat_hip,
        });
        Ok(Arc::new(BodyPoseAssetInfo {
            definition: definition.clone(),
            resource: binding.resource.clone(),
            digest: native.digest.clone(),
            reference_resource: self.input.reference_resource.clone(),
            reference_digest: reference.map(|r| r.digest.clone()),
            fitted: binding.fitted,
            node_count: rig.shape.nodes.len(),
            hands,
            eye,
            roles,
            fits,
        }))
    }
    pub(super) fn info(&self, definition: &Archetype) -> Result<Arc<BodyPoseAssetInfo>> {
        if let Some(cached) = self.resolved.get(&definition.id)
            && cached.definition == *definition
        {
            return Ok(cached.clone());
        }
        // A later actual definition never inherits a model-only cached policy.
        // Diagnostics remain &self: changed definitions resolve without mutation.
        self.resolve(definition)
    }
}
impl Session {
    /// Inspect the current actual definition against this immutable native set.
    /// Unsupported/stand-in bodies are explicit; no stock rig is substituted.
    pub fn body_pose_asset_info(&self, archetype: ArchetypeId) -> Result<Arc<BodyPoseAssetInfo>> {
        let definition = self
            .archetypes
            .get(archetype)
            .context("Unknown body pose archetype")?;
        self.body_poses.info(definition)
    }
}
