//! Vehicle presentation: native vehicle models drawn as GPU instances at
//! interpolated authoritative transforms, with wheels, steering, suspension
//! and turrets, plus seat transforms for riders and the driving camera.
use crate::items::native_shape_scene;
use anyhow::{Context, Result, ensure};
use bri_content::shape::Shape;
use bri_render::scene::{GpuInstances, GpuScene, SceneImage, SceneRenderer, SceneTransform};
use bri_sim::session::{VehicleInfo, VehiclePose};
use bri_vehicles::{Definition, Pack, schema::Wheel};
use glam::{Mat4, Quat, Vec3};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
};

const HISTORY: usize = 16;
/// Render other vehicles this many server ticks behind the newest pose.
const INTERPOLATION_TICKS: f64 = 9.0;
const TICK_RATE: f64 = 120.0;

struct Model {
    data: bri_render::scene::SceneData,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transforms: Vec<SceneTransform>,
}

pub struct VehicleAssets {
    pack: Pack,
    models: BTreeMap<String, Model>,
}

impl VehicleAssets {
    /// Load every declared model with its folder-local material textures.
    pub fn load(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let pack = Pack::load(root.join("vehicles.json"))?;
        let mut textures: BTreeMap<String, SceneImage> = BTreeMap::new();
        for asset in pack.assets.iter().filter(|a| a.kind == "texture") {
            let bytes = crate::items::checked_read(&root, &asset.path, &asset.sha256, 16 << 20)?;
            let image = image::load_from_memory(&bytes)?.to_rgba8();
            textures.insert(
                asset.virtual_path.to_ascii_lowercase(),
                SceneImage {
                    label: asset.virtual_path.clone(),
                    width: image.width(),
                    height: image.height(),
                    rgba: image.into_raw(),
                    srgb: false,
                },
            );
        }
        let blank = textures
            .iter()
            .find(|(path, _)| path.ends_with("/blank.png"))
            .map(|(_, image)| image.clone())
            .context("Vehicle pack has no blank paint texture")?;
        let mut models = BTreeMap::new();
        for asset in pack.assets.iter().filter(|a| a.kind == "model") {
            let bytes = crate::items::checked_read(&root, &asset.path, &asset.sha256, 32 << 20)?;
            let shape: Shape = serde_json::from_slice(&bytes)?;
            shape.validate()?;
            let folder = asset
                .virtual_path
                .rsplit_once('/')
                .map_or(String::new(), |(dir, _)| dir.to_ascii_lowercase());
            // DTS materials name their texture; resolve beside the model, then
            // anywhere in the pack, then the blank paint surface.
            let images: Vec<SceneImage> = shape
                .materials
                .iter()
                .map(|m| {
                    let name = m.name.to_ascii_lowercase();
                    textures
                        .get(&format!("{folder}/{name}.png"))
                        .or_else(|| {
                            textures
                                .iter()
                                .find(|(path, _)| path.ends_with(&format!("/{name}.png")))
                                .map(|(_, image)| image)
                        })
                        .cloned()
                        .unwrap_or_else(|| blank.clone())
                })
                .collect();
            let refs: Vec<&SceneImage> = images.iter().collect();
            let pose = bri_content::animation::sample(&shape, None, 0.0)?;
            let data = native_shape_scene(
                &asset.path,
                &shape,
                &refs,
                [1.0; 4],
                Mat4::IDENTITY,
                &pose,
            )?;
            models.insert(
                asset.path.clone(),
                Model {
                    data,
                    gpu: None,
                    instances: None,
                    transforms: Vec::new(),
                },
            );
        }
        for d in &pack.definitions {
            ensure!(
                models.contains_key(&d.model),
                "Vehicle {} model is missing",
                d.id
            );
        }
        Ok(Self { pack, models })
    }
    pub fn definition(&self, id: &str) -> Option<&Definition> {
        self.pack.definitions.iter().find(|d| d.id == id)
    }
}

/// Chassis-local tire transform: the hub drops by the suspension extension,
/// steering is positive to the right (clockwise from above), forward travel
/// spins the tire's top toward -Z, and the authored tire is turned axle-out.
pub fn wheel_transform(wheel: &Wheel, suspension: f32, spin: f32, steering: f32) -> Mat4 {
    Mat4::from_translation(Vec3::from(wheel.position) - Vec3::Y * suspension)
        * Mat4::from_rotation_y(-steering * wheel.steering)
        * Mat4::from_rotation_x(-spin)
        * Mat4::from_quat(Quat::from_array(wheel.model_rotation))
}

fn to_transform(position: Vec3, rotation: Quat) -> Mat4 {
    Mat4::from_rotation_translation(rotation, position)
}

/// Interpolated per-vehicle state for this frame.
#[derive(Clone, Debug)]
pub struct VehicleFrame {
    pub position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub steering: f32,
    pub wheel_suspension: Vec<f32>,
    pub wheel_rotation: Vec<f32>,
    pub turret_aim: [f32; 2],
}

#[derive(Default)]
pub struct ClientVehicles {
    history: BTreeMap<u64, VecDeque<VehiclePose>>,
    frames: BTreeMap<u64, VehicleFrame>,
}

impl ClientVehicles {
    pub fn clear(&mut self) {
        self.history.clear();
        self.frames.clear();
    }
    pub fn frame(&self, id: u64) -> Option<&VehicleFrame> {
        self.frames.get(&id)
    }
    /// Record replicated poses and compute this frame's transforms. The
    /// vehicle the local player drives is shown at its newest pose, lightly
    /// extrapolated, so steering feels immediate; others are interpolated.
    pub fn update(
        &mut self,
        infos: &BTreeMap<u64, VehicleInfo>,
        poses: &BTreeMap<u64, VehiclePose>,
        server_tick: Option<f64>,
        driven: Option<u64>,
    ) {
        self.history.retain(|id, _| infos.contains_key(id));
        for (id, pose) in poses {
            let history = self.history.entry(*id).or_default();
            if history.back().is_none_or(|last| last.tick < pose.tick) {
                if history.len() == HISTORY {
                    history.pop_front();
                }
                history.push_back(pose.clone());
            }
        }
        self.frames.clear();
        for (id, history) in &self.history {
            let Some(newest) = history.back() else {
                continue;
            };
            let frame = match server_tick {
                Some(now) if Some(*id) != driven => {
                    sample(history, now - INTERPOLATION_TICKS)
                }
                Some(now) => {
                    let ahead = ((now - newest.tick as f64).clamp(0.0, 6.0) / TICK_RATE) as f32;
                    let mut frame = frame_of(newest);
                    frame.position += frame.velocity * ahead;
                    frame
                }
                None => frame_of(newest),
            };
            self.frames.insert(*id, frame);
        }
    }
    /// World transform of a seat node (players' feet ride there).
    pub fn seat(
        &self,
        assets: &VehicleAssets,
        info: &VehicleInfo,
        seat: usize,
    ) -> Option<(Vec3, f32)> {
        let frame = self.frames.get(&info.id)?;
        let d = assets.definition(&info.definition)?;
        let s = d.seats.get(seat)?;
        let mut local = to_transform(
            Vec3::from(s.transform.position),
            Quat::from_array(s.transform.rotation),
        );
        if seat == 2 && d.attachment_mount.is_some() {
            let pivot = d
                .attachment_mount
                .as_ref()
                .map_or(Vec3::ZERO, |t| Vec3::from(t.position));
            local = Mat4::from_translation(pivot)
                * Mat4::from_rotation_y(frame.turret_aim[0])
                * Mat4::from_translation(-pivot)
                * local;
        }
        let world = to_transform(frame.position, frame.rotation) * local;
        let (_, rotation, position) = world.to_scale_rotation_translation();
        let forward = rotation * Vec3::NEG_Z;
        Some((position, forward.x.atan2(-forward.z)))
    }
    /// Refresh per-model instance lists (chassis, wheels, turrets).
    pub fn prepare(
        &mut self,
        assets: &mut VehicleAssets,
        infos: &BTreeMap<u64, VehicleInfo>,
        palette: &[[f32; 4]],
    ) {
        for model in assets.models.values_mut() {
            model.transforms.clear();
        }
        for (id, frame) in &self.frames {
            let Some(info) = infos.get(id) else { continue };
            let Some(d) = assets
                .pack
                .definitions
                .iter()
                .find(|d| d.id == info.definition)
                .cloned()
            else {
                continue;
            };
            let tint = info
                .color
                .and_then(|c| palette.get(usize::from(c)))
                .map_or([1.0; 4], |c| [c[0], c[1], c[2], 1.0]);
            let body = to_transform(frame.position, frame.rotation);
            let mut push = |model: &str, transform: Mat4, tint: [f32; 4]| {
                if let Some(m) = assets.models.get_mut(model)
                    && transform.is_finite()
                {
                    m.transforms.push(SceneTransform { transform, tint });
                }
            };
            push(&d.model, body, tint);
            for (i, wheel) in d.wheels.iter().enumerate() {
                let suspension = frame
                    .wheel_suspension
                    .get(i)
                    .copied()
                    .unwrap_or(wheel.rest_length);
                let spin = frame.wheel_rotation.get(i).copied().unwrap_or(0.0);
                let local = wheel_transform(wheel, suspension, spin, frame.steering);
                push(&wheel.model, body * local, [1.0; 4]);
            }
            if let (Some(model), Some(mount)) = (&d.attachment_model, &d.attachment_mount) {
                let local = to_transform(
                    Vec3::from(mount.position),
                    Quat::from_array(mount.rotation),
                ) * Mat4::from_rotation_y(frame.turret_aim[0]);
                push(model, body * local, tint);
            }
        }
    }
    pub fn upload(
        assets: &mut VehicleAssets,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        for model in assets.models.values_mut() {
            if model.transforms.is_empty() {
                if let Some(instances) = &mut model.instances {
                    instances.update(queue, &[])?;
                }
                continue;
            }
            if model.gpu.is_none() {
                model.gpu = Some(renderer.upload(device, queue, &model.data)?);
            }
            if model
                .instances
                .as_ref()
                .is_none_or(|i| i.capacity() < model.transforms.len())
            {
                model.instances = Some(GpuInstances::new(
                    device,
                    model.transforms.len().next_power_of_two().max(4),
                )?);
            }
            model
                .instances
                .as_mut()
                .unwrap()
                .update(queue, &model.transforms)?;
        }
        Ok(())
    }
    pub fn draws(assets: &VehicleAssets) -> Vec<(&GpuScene, &GpuInstances)> {
        assets
            .models
            .values()
            .filter(|m| !m.transforms.is_empty())
            .filter_map(|m| Some((m.gpu.as_ref()?, m.instances.as_ref()?)))
            .collect()
    }
    pub fn gpu_stopped(assets: &mut VehicleAssets) {
        for model in assets.models.values_mut() {
            model.gpu = None;
            model.instances = None;
        }
    }
}

fn frame_of(pose: &VehiclePose) -> VehicleFrame {
    VehicleFrame {
        position: Vec3::from(pose.position),
        rotation: Quat::from_array(pose.rotation).normalize(),
        velocity: Vec3::from(pose.velocity),
        steering: pose.steering,
        wheel_suspension: pose.wheel_suspension.clone(),
        wheel_rotation: pose.wheel_rotation.clone(),
        turret_aim: pose.turret_aim,
    }
}

fn sample(history: &VecDeque<VehiclePose>, tick: f64) -> VehicleFrame {
    let first = history.front().unwrap();
    if tick <= first.tick as f64 {
        return frame_of(first);
    }
    for pair in history.iter().collect::<Vec<_>>().windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if tick <= b.tick as f64 {
            let t = ((tick - a.tick as f64) / (b.tick - a.tick).max(1) as f64) as f32;
            let (fa, fb) = (frame_of(a), frame_of(b));
            let lerp = |x: &[f32], y: &[f32]| -> Vec<f32> {
                x.iter()
                    .zip(y)
                    .map(|(p, q)| p + (q - p) * t)
                    .collect()
            };
            return VehicleFrame {
                position: fa.position.lerp(fb.position, t),
                rotation: fa.rotation.slerp(fb.rotation, t),
                velocity: fa.velocity.lerp(fb.velocity, t),
                steering: fa.steering + (fb.steering - fa.steering) * t,
                wheel_suspension: lerp(&fa.wheel_suspension, &fb.wheel_suspension),
                // Wheel spin wraps; take the newer value rather than blending.
                wheel_rotation: fb.wheel_rotation,
                turret_aim: [
                    fa.turret_aim[0] + (fb.turret_aim[0] - fa.turret_aim[0]) * t,
                    fa.turret_aim[1] + (fb.turret_aim[1] - fa.turret_aim[1]) * t,
                ],
            };
        }
    }
    let last = history.back().unwrap();
    let mut frame = frame_of(last);
    let ahead = ((tick - last.tick as f64).min(6.0) / TICK_RATE) as f32;
    frame.position += frame.velocity * ahead;
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pose(tick: u64, x: f32) -> VehiclePose {
        VehiclePose {
            id: 1,
            tick,
            position: [x, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
            steering: 0.0,
            wheel_suspension: vec![0.4],
            wheel_rotation: vec![0.0],
            turret_aim: [0.0; 2],
            jetting: false,
        }
    }
    /// v20 tires are authored with the hub axis along Torque +Y (native -Z),
    /// outer face first; mounted, they must roll along the chassis.
    #[test]
    fn tires_mount_axle_out_steer_right_and_roll_forward() {
        let tire = |x: f32| Wheel {
            position: [x, 0.4, -1.9],
            radius: 0.75,
            rest_length: 0.4,
            spring: 6000.0,
            damping: 800.0,
            friction: 5.0,
            steering: 1.0,
            powered: false,
            model: String::new(),
            model_rotation: Quat::from_rotation_y(-x.signum() * std::f32::consts::FRAC_PI_2)
                .to_array(),
        };
        for x in [-1.6, 1.6] {
            let rest = wheel_transform(&tire(x), 0.3, 0.0, 0.0);
            let outer = rest.transform_vector3(Vec3::NEG_Z);
            assert!(
                (outer - Vec3::X * x.signum()).length() < 1e-5,
                "outer face {outer}"
            );
            assert!((rest.transform_point3(Vec3::ZERO) - Vec3::new(x, 0.1, -1.9)).length() < 1e-5);
            let steered = wheel_transform(&tire(x), 0.3, 0.0, 0.5) * rest.inverse();
            let heading = steered.transform_vector3(Vec3::NEG_Z);
            assert!(heading.x > 0.4 && heading.z < 0.0, "steer right {heading}");
            let rolled = wheel_transform(&tire(x), 0.3, 0.2, 0.0).transform_vector3(Vec3::Y);
            assert!(rolled.z < -0.1, "forward spin must carry the top forward");
        }
    }
    #[test]
    fn vehicle_samples_interpolate_between_poses() {
        let history: VecDeque<_> = [pose(10, 0.0), pose(13, 3.0)].into();
        assert!((sample(&history, 11.5).position.x - 1.5).abs() < 1e-5);
        assert_eq!(sample(&history, 0.0).position.x, 0.0);
    }
}
