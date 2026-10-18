//! Where a gunner's shot leaves the barrel. v20 poses turret and cannon
//! barrels with a `look` clip driven by the head pitch
//! (`Player::updateLookAnimation`), then spawns the shell at the posed node:
//! the Tank Turret's `getSlotTransform(1)` (its `mount1` node) and the Pirate
//! Cannon's `getEyeTransform()` (its `eye` node). Both nodes sit on the barrel,
//! so the shot starts at the barrel's mouth at every pitch.
use crate::schema::{Asset, Definition, Family, Pack};
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
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
    (0.5 - pitch / std::f32::consts::PI).clamped(0.0, 1.0)
}

/// The `look` clip made for this shape: every channel names one of its nodes.
pub fn look_clip<'a>(clips: &'a [Animation], shape: &'a Shape) -> Option<&'a Animation> {
    shape.animations.iter().chain(clips).find(|clip| {
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

/// Where `pitch` falls along a three-key aim clip ordered highest, level,
/// lowest whose end keys sit at `up` and `down` radians: 0 at `up`, 0.5
/// level, 1 at `down`, each half sampled evenly (Halo's aiming screens, with
/// different `up_pitch_per_frame` and `down_pitch_per_frame`).
pub fn aim_phase(down: f32, up: f32, pitch: f32) -> f32 {
    if pitch >= 0. {
        0.5 * (1. - pitch / up).clamped(0., 1.)
    } else {
        0.5 + 0.5 * (pitch / down).clamped(0., 1.)
    }
}

impl Definition {
    /// Shared look phase for rendered geometry, effect sockets and host shots.
    pub fn look_phase(&self, pitch: f32) -> f32 {
        match self.weapon.as_ref().and_then(|w| w.look_animation_pitch) {
            Some([down, up]) => aim_phase(down, up, pitch),
            None => look_phase(pitch),
        }
    }
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
        let pitch = aim[1].clamped(self.look_pitch[0], self.look_pitch[1]);
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
            let f = self.look_phase(pitch) * (track.len() - 1) as f32;
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

impl Definition {
    /// Where seat `seat`'s rider holds on with `hand` (0 right, 1 left),
    /// body-local and unscaled, for a gunner's `[yaw, pitch]` aim. A
    /// gunner's holds move as the gun is drawn: tipped about its `pivot`
    /// when its model has a `look` clip, then turned with the turret about
    /// its mount (as the seat is). A gun drawn fixed to the body keeps its
    /// holds where they are, whatever the shots do.
    pub fn hand_hold(&self, seat: usize, hand: usize, aim: [f32; 2]) -> Option<Vec3> {
        let s = self.seats.get(seat)?;
        let at = Vec3::from((*s.hands.each().get(hand)?)?);
        let Some(weapon) = self
            .weapon
            .as_ref()
            .filter(|_| self.weapon_seat() == Some(seat))
        else {
            return Some(at);
        };
        let pitch = aim[1].clamped(self.look_pitch[0], self.look_pitch[1]);
        if weapon.look_muzzle.len() < 2
            && !self.is_actor()
            && let Some(mount) = &self.attachment_mount
        {
            let about = Vec3::from(mount.position);
            let basis = Quat::from_array(mount.rotation);
            let local = basis.inverse() * (at - about);
            return Some(
                about
                    + basis * Quat::from_rotation_y(aim[0]) * Quat::from_rotation_x(pitch) * local,
            );
        }
        let at = if weapon.look_muzzle.len() >= 2 {
            let pitch = aim[1].clamped(self.look_pitch[0], self.look_pitch[1]);
            let pivot = Vec3::from(weapon.pivot);
            let basis = self
                .attachment_mount
                .as_ref()
                .map_or(Quat::IDENTITY, |m| Quat::from_array(m.rotation));
            pivot + basis * Quat::from_rotation_x(pitch) * basis.inverse() * (at - pivot)
        } else {
            at
        };
        Some(match &self.attachment_mount {
            Some(mount) if !self.is_actor() => {
                let about = Vec3::from(mount.position);
                let basis = Quat::from_array(mount.rotation);
                about + basis * Quat::from_rotation_y(aim[0]) * basis.inverse() * (at - about)
            }
            _ => at,
        })
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
    #[test]
    fn authored_look_endpoints_share_the_neutral_sample() {
        let mut d = crate::testing::tank();
        let limits = [-0.2, 0.7];
        d.weapon.as_mut().unwrap().look_animation_pitch = Some(limits);
        for (pitch, expected) in [
            (-1., 1.),
            (-0.2, 1.),
            (-0.1, 0.75),
            (0., 0.5),
            (0.35, 0.25),
            (0.7, 0.),
            (1., 0.),
        ] {
            assert!((d.look_phase(pitch) - expected).abs() < 1e-6);
        }
        d.weapon.as_mut().unwrap().look_animation_pitch = None;
        for pitch in [-1., 0., 0.7] {
            assert_eq!(d.look_phase(pitch), look_phase(pitch));
        }
    }
    fn pack() -> (Pack, std::path::PathBuf) {
        let root = bri_package::testing::pack_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
            "vehicles",
        );
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

    /// A gunner's hands stay on the drawn gun: turned with the turret
    /// about its mount, tipped with either the whole attachment or its barrel
    /// look clip; other seats' holds stay put.
    #[test]
    fn a_gunners_hand_holds_turn_and_tip_with_the_drawn_gun() {
        let mut tank = crate::testing::tank();
        let grip = [0.3, 2.4, 1.0];
        tank.seats[2].hands.right = Some(grip);
        tank.seats[0].hands.left = Some([0.2, 1.2, -1.5]);
        let quarter = std::f32::consts::FRAC_PI_2;
        let turned = tank.hand_hold(2, 0, [quarter, 0.3]).unwrap();
        // A model without a look clip pitches the whole attachment, so its
        // hand targets pitch too, before yaw, about that attachment's mount.
        let expected = Vec3::new(0., 1.6, 0.)
            + Quat::from_rotation_y(quarter)
                * Quat::from_rotation_x(0.3)
                * (Vec3::from(grip) - Vec3::new(0., 1.6, 0.));
        assert!(turned.abs_diff_eq(expected, 1e-5), "{turned}");
        assert_eq!(tank.hand_hold(2, 1, [quarter, 0.3]), None, "left free");
        assert_eq!(
            tank.hand_hold(0, 1, [quarter, 0.3]),
            Some(Vec3::new(0.2, 1.2, -1.5)),
            "the driver's do not turn"
        );
        // With a look clip the gun tips: the grip, behind the pivot at
        // (0, 2.4, 0), drops as the gun looks up.
        tank.weapon.as_mut().unwrap().look_muzzle = vec![[0.0; 3]; 2];
        let tipped = tank.hand_hold(2, 0, [0.0, 0.3]).unwrap();
        assert!(tipped.y < 2.4 && (tipped.distance(Vec3::new(0.3, 2.4, 0.0)) - 1.0).abs() < 1e-5);
    }
}
