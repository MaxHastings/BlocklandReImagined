//! Where a gunner's shot leaves the barrel. v20 poses turret and cannon
//! barrels with a `look` clip driven by the head pitch
//! (`Player::updateLookAnimation`), then spawns the shell at the posed node:
//! the Tank Turret's `getSlotTransform(1)` (its `mount1` node) and the Pirate
//! Cannon's `getEyeTransform()` (its `eye` node). Both nodes sit on the barrel,
//! so the shot starts at the barrel's mouth at every pitch.
use crate::schema::{Asset, Definition, Family, Pack};
use anyhow::{Context, Result, ensure};
use bri_content::shape::{Animation, ClipSet, Shape};
use glam::{Quat, Vec3};
use sha2::{Digest, Sha256};
use std::path::{Component, Path};

/// Muzzle points sampled across the look range; linear between them.
pub const MUZZLE_SAMPLES: usize = 65;

/// Look clip phase for a pitch, 0 at straight up (the clip's start) to 1 at
/// straight down. v20's `Player::updateLookAnimation` (0x5A53B0) sets the
/// thread to `(mHead.x + pi/2) / pi` whatever the datablock's look angles,
/// so the barrel always points where the gunner looks.
pub fn look_phase(pitch: f32) -> f32 {
    (0.5 - pitch / std::f32::consts::PI).clamp(0.0, 1.0)
}

/// The `look` clip made for this shape: every channel names one of its nodes.
pub fn look_clip<'a>(clips: &'a [Animation], shape: &Shape) -> Option<&'a Animation> {
    clips.iter().find(|clip| {
        clip.name == "look"
            && !clip.nodes.is_empty()
            && clip.nodes.iter().all(|channel| {
                shape
                    .nodes
                    .iter()
                    .any(|n| n.name.eq_ignore_ascii_case(&channel.node))
            })
    })
}

/// Every `look` clip in the pack.
pub fn look_clips(pack: &Pack, root: &Path) -> Result<Vec<Animation>> {
    let mut out = Vec::new();
    for asset in pack.assets.iter().filter(|a| {
        a.kind == "animation" && a.virtual_path.to_ascii_lowercase().ends_with("look.dsq")
    }) {
        let clips: ClipSet = serde_json::from_slice(&read(root, asset, 8 << 20)?)?;
        out.extend(clips.animations.into_iter().filter(|a| a.name == "look"));
    }
    Ok(out)
}

fn read(root: &Path, asset: &Asset, limit: u64) -> Result<Vec<u8>> {
    let path = Path::new(&asset.path);
    ensure!(
        !path.is_absolute() && path.components().all(|c| matches!(c, Component::Normal(_))),
        "unsafe vehicle asset path"
    );
    let path = crate::asset_root(root, asset).join(path);
    ensure!(
        std::fs::metadata(&path)?.len() < limit,
        "vehicle asset too large"
    );
    let bytes = std::fs::read(&path)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == asset.sha256,
        "vehicle asset hash mismatch: {}",
        asset.path
    );
    Ok(bytes)
}

impl Definition {
    /// The barrel node v20 fires from.
    pub fn muzzle_node(&self) -> &'static str {
        if self.family == Family::Cannon {
            "eye"
        } else {
            "mount1"
        }
    }
    /// Body-local, unscaled shot origin and direction for a gunner's
    /// `[yaw, pitch]` aim. The direction is `getMuzzleVector`: the turret's
    /// facing tipped by the head pitch.
    pub fn muzzle(&self, aim: [f32; 2]) -> Option<(Vec3, Vec3)> {
        let weapon = self.weapon.as_ref()?;
        let pitch = aim[1].clamp(self.look_pitch[0], self.look_pitch[1]);
        let yaw = if self.is_actor() { 0.0 } else { aim[0] };
        let (mount, mount_rotation) = self
            .attachment_mount
            .as_ref()
            .map_or((Vec3::ZERO, Quat::IDENTITY), |t| {
                (Vec3::from(t.position), Quat::from_array(t.rotation))
            });
        let turn = mount_rotation * Quat::from_rotation_y(yaw);
        let direction = turn * Quat::from_rotation_x(pitch) * Vec3::NEG_Z;
        let track = &weapon.look_muzzle;
        let origin = if track.len() >= 2 {
            let f = look_phase(pitch) * (track.len() - 1) as f32;
            let i = (f.floor() as usize).min(track.len() - 2);
            let local = Vec3::from(track[i]).lerp(Vec3::from(track[i + 1]), f - i as f32);
            mount + turn * local
        } else {
            // No look clip: turn the rest muzzle about the authored pivot.
            let aiming = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
            let pivot = Vec3::from(weapon.pivot);
            pivot + aiming * (Vec3::from(weapon.muzzle.position) - pivot)
        };
        Some((origin, direction))
    }
}

impl Pack {
    /// Sample each gunner's muzzle node along its model's `look` clip.
    pub fn attach_muzzle_tracks(&mut self, root: &Path) -> Result<()> {
        if self.definitions.iter().all(|d| d.weapon.is_none()) {
            return Ok(());
        }
        let clips = look_clips(self, root)?;
        for i in 0..self.definitions.len() {
            let d = &self.definitions[i];
            if d.weapon.is_none() {
                continue;
            }
            let model = d.attachment_model.as_ref().unwrap_or(&d.model);
            let asset = self
                .assets
                .iter()
                .find(|a| &a.path == model)
                .with_context(|| format!("Vehicle {} weapon model is missing", d.id))?;
            let shape: Shape = serde_json::from_slice(&read(root, asset, 32 << 20)?)?;
            shape.validate()?;
            let Some(clip) = look_clip(&clips, &shape) else {
                continue;
            };
            let node = shape
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(d.muzzle_node()))
                .with_context(|| format!("Vehicle {} has no {} node", d.id, d.muzzle_node()))?;
            let mut track = Vec::with_capacity(MUZZLE_SAMPLES);
            for step in 0..MUZZLE_SAMPLES {
                let time = clip.duration * step as f32 / (MUZZLE_SAMPLES - 1) as f32;
                let pose = bri_content::animation::sample(&shape, Some(clip), time)?;
                let point = pose.nodes[node].w_axis.truncate();
                ensure!(point.is_finite(), "Vehicle {} muzzle is not finite", d.id);
                track.push(point.to_array());
            }
            if let Some(weapon) = &mut self.definitions[i].weapon {
                weapon.look_muzzle = track;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pack() -> (Pack, std::path::PathBuf) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/vehicles-pack-012");
        (Pack::load(root.join("vehicles.json")).unwrap(), root)
    }
    /// The barrel turns one-for-one with the aim: the hinge-to-muzzle line
    /// keeps a fixed angle to the firing direction across the look range, and
    /// the shot starts at the barrel's mouth, ahead of the hinge.
    #[test]
    #[ignore = "requires the converted native vehicle pack"]
    fn shots_leave_the_posed_barrel_mouth_at_every_pitch() -> Result<()> {
        let (pack, root) = pack();
        let clips = look_clips(&pack, &root)?;
        for id in [
            "v20.vehicle.tankvehicle",
            "v20.vehicle.tankturretplayer",
            "v20.vehicle.cannonturret",
        ] {
            let d = pack.definitions.iter().find(|d| d.id == id).unwrap();
            let weapon = d.weapon.as_ref().unwrap();
            assert_eq!(weapon.look_muzzle.len(), MUZZLE_SAMPLES, "{id}");
            let model = d.attachment_model.as_ref().unwrap_or(&d.model);
            let asset = pack.assets.iter().find(|a| &a.path == model).unwrap();
            let shape: Shape = serde_json::from_slice(&read(&root, asset, 32 << 20)?)?;
            let clip = look_clip(&clips, &shape).unwrap();
            let hinge = shape
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(&clip.nodes[0].node))
                .unwrap();
            let (mount, turn) = d
                .attachment_mount
                .as_ref()
                .map_or((Vec3::ZERO, Quat::IDENTITY), |t| {
                    (Vec3::from(t.position), Quat::from_array(t.rotation))
                });
            let mut offsets = vec![];
            for step in 0..=8 {
                let t = step as f32 / 8.0;
                let pitch = d.look_pitch[1] - t * (d.look_pitch[1] - d.look_pitch[0]);
                let phase = look_phase(pitch);
                let pose =
                    bri_content::animation::sample(&shape, Some(clip), phase * clip.duration)?;
                let hinge = mount + turn * pose.nodes[hinge].w_axis.truncate();
                let (origin, direction) = d.muzzle([0.0, pitch]).unwrap();
                let barrel = origin - hinge;
                let elevation = |v: Vec3| v.y.atan2(-v.z);
                offsets.push(elevation(barrel) - elevation(direction));
                assert!(
                    barrel.dot(direction) > 0.0,
                    "{id}: muzzle behind the hinge at pitch {pitch}"
                );
            }
            let spread = offsets.iter().copied().fold(f32::MIN, f32::max)
                - offsets.iter().copied().fold(f32::MAX, f32::min);
            println!("{id}: barrel-to-aim offsets {offsets:?}");
            assert!(spread < 0.03, "{id}: barrel lags the aim by {spread} rad");
        }
        Ok(())
    }
}
