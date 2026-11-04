//! One composed pose for authored body channels, geometry and grip nodes.
use super::*;
use bri_content::animation::{Layer, Pose};
use std::collections::BTreeSet;

pub(super) struct BodyRig {
    pub shape: Shape,
    steering: BTreeMap<String, Animation>,
    look: Option<Animation>,
    pub parts: Vec<BodyPart>,
}

pub(super) struct BodyPart {
    pub on: String,
    pub variants: BTreeMap<String, [Option<String>; 2]>,
    pub(super) object_state: Option<String>,
    pub node: Option<usize>,
    pub inverse: Mat4,
    pub skinned: bool,
    objects: BTreeSet<usize>,
}

impl BodyPart {
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.on).chain(self.variants.values().flatten().flatten())
    }
    pub fn key(&self, d: &Definition, info: &VehicleInfo, frame: &VehicleFrame) -> Option<&str> {
        let Some(name) = &self.object_state else {
            return Some(&self.on);
        };
        let Some(binding) = d
            .visuals
            .iter()
            .find(|v| v.objects.iter().any(|n| n.eq_ignore_ascii_case(name)))
        else {
            return Some(&self.on);
        };
        let driver = d
            .control_seat()
            .and_then(|s| info.occupants.get(s))
            .is_some_and(Option::is_some);
        let active = binding.when.evaluate(|signal| match signal {
            "switch" => info.lights,
            "occupied" => driver,
            "jump" => frame.driver_jump_held,
            "brake" => frame.driver_braking,
            "reverse" => frame.driver_reversing,
            "destroyed" => info.destroyed,
            _ => false, // Adapter admission rejects unsupported signals.
        });
        self.variants
            .get(&d.id)
            .and_then(|keys| keys[usize::from(active)].as_deref())
    }
}

impl BodyRig {
    pub fn load(
        model: &str,
        shape: &Shape,
        look: Option<&Animation>,
        pack: &Pack,
        images: &[SceneImage],
        insert: &mut impl FnMut(String, &Pose, &[&SceneImage]) -> Result<()>,
    ) -> Result<Self> {
        const MAX_APPEARANCE_STATES: usize = 256;
        let states = pack
            .definitions
            .iter()
            .filter(|d| d.model == model)
            .flat_map(|d| &d.visuals)
            .map(|v| v.objects.len() * 2)
            .sum::<usize>();
        ensure!(
            states <= MAX_APPEARANCE_STATES,
            "Model {model} exceeds the {MAX_APPEARANCE_STATES} prebuilt appearance-state budget"
        );
        let rest = bri_content::animation::sample(shape, None, 0.)?;
        let refs: Vec<_> = images.iter().collect();
        // Keep the complete source model available for debris and other consumers.
        insert(model.to_string(), &rest, &refs)?;
        let mut steering = BTreeMap::new();
        let mut bound_objects = BTreeSet::new();
        for d in pack.definitions.iter().filter(|d| d.model == model) {
            for thread in &d.threads {
                ensure!(
                    shape
                        .animations
                        .iter()
                        .any(|a| a.name.eq_ignore_ascii_case(&thread.sequence)),
                    "{}: animation thread {} is missing",
                    d.id,
                    thread.sequence
                );
            }
            if let Some(name) = &d.steering_sequence {
                let clip = shape
                    .animations
                    .iter()
                    .find(|a| a.name.eq_ignore_ascii_case(name))
                    .with_context(|| format!("{}: steering sequence {name} is missing", d.id))?;
                let mut clip = clip.clone();
                clip.looping = false;
                steering.insert(name.to_ascii_lowercase(), clip);
            }
            for name in d.visuals.iter().flat_map(|v| &v.objects) {
                let object = shape
                    .objects
                    .iter()
                    .position(|o| o.name.eq_ignore_ascii_case(name))
                    .with_context(|| format!("{}: visual object {name} is missing", d.id))?;
                bound_objects.insert(object);
            }
        }
        // A rigid object is split only when a declared channel can move one
        // of its ancestors. Inspect every authored key, not a few sample times.
        let mut changed = BTreeSet::new();
        for clip in shape
            .animations
            .iter()
            .filter(|a| {
                steering.contains_key(&a.name.to_ascii_lowercase())
                    || pack
                        .definitions
                        .iter()
                        .filter(|d| d.model == model)
                        .any(|d| {
                            d.threads
                                .iter()
                                .any(|t| t.sequence.eq_ignore_ascii_case(&a.name))
                                || d.turret_acceleration.as_ref().is_some_and(|o| {
                                    o.clips.iter().any(|n| n.eq_ignore_ascii_case(&a.name))
                                })
                        })
            })
            .chain(look)
        {
            for track in &clip.nodes {
                if let Some((i, node)) = shape
                    .nodes
                    .iter()
                    .enumerate()
                    .find(|(_, n)| n.name.eq_ignore_ascii_case(&track.node))
                    && (track.rotations.iter().any(|r| *r != node.rotation)
                        || track.translations.iter().any(|t| *t != node.translation)
                        || track.scales.iter().any(|s| *s != [1.; 3])
                        || !track.scale_rotations.is_empty())
                {
                    changed.insert(i);
                }
            }
        }
        let animated = |node: usize| {
            let mut at = Some(node);
            while let Some(i) = at {
                if changed.contains(&i) {
                    return true;
                }
                at = shape.nodes[i].parent;
            }
            false
        };
        // Group rigid objects by node; keep each object_state separate. Skinned geometry
        // and animated object channels use the established dynamic mesh path.
        let mut groups = BTreeMap::<_, BTreeSet<usize>>::new();
        for (i, object) in shape.objects.iter().enumerate() {
            let dynamic = object.meshes.iter().any(|m| {
                shape
                    .meshes
                    .get(*m)
                    .and_then(Option::as_ref)
                    .is_some_and(|m| m.skin.is_some())
            }) || shape
                .animations
                .iter()
                .chain(look)
                .any(|a| a.objects.iter().any(|o| o.object == i));
            let object_state = bound_objects
                .contains(&i)
                .then(|| object.name.to_ascii_lowercase());
            let node = if dynamic {
                None
            } else {
                object.node.filter(|n| animated(*n))
            };
            groups
                .entry((node, object_state, dynamic))
                .or_default()
                .insert(i);
        }
        let mut rig = Self {
            shape: shape.clone(),
            steering,
            look: look.cloned(),
            parts: vec![],
        };
        for ((node, object_state, skinned), objects) in groups {
            let on = format!("{model}#body{}", rig.parts.len());
            let mut part = BodyPart {
                on,
                variants: BTreeMap::new(),
                object_state,
                node,
                skinned,
                objects,
                inverse: node.map_or(Mat4::IDENTITY, |n| rest.nodes[n].inverse()),
            };
            let pose = rig.part_pose(&rest, &part);
            insert(part.on.clone(), &pose, &refs)?;
            if let Some(name) = &part.object_state {
                for d in pack.definitions.iter().filter(|d| d.model == model) {
                    let Some(binding) = d
                        .visuals
                        .iter()
                        .find(|v| v.objects.iter().any(|n| n.eq_ignore_ascii_case(name)))
                    else {
                        continue;
                    };
                    let mut keys = [None, None];
                    for (state, appearance) in [&binding.off, &binding.on].into_iter().enumerate() {
                        if !appearance.visible {
                            continue;
                        }
                        let key = format!("{model}#visual-{}-{name}-{state}", d.id);
                        let mut pictures = images.to_vec();
                        if let Some(color) = appearance.color {
                            for i in &part.objects {
                                for mesh in shape.objects[*i]
                                    .meshes
                                    .iter()
                                    .filter_map(|m| shape.meshes.get(*m).and_then(Option::as_ref))
                                {
                                    for primitive in &mesh.primitives {
                                        if let Some(image) = primitive
                                            .material
                                            .and_then(|slot| pictures.get_mut(slot))
                                        {
                                            *image = SceneImage {
                                                label: "object appearance".into(),
                                                width: 1,
                                                height: 1,
                                                rgba: color.to_vec(),
                                                srgb: false,
                                            };
                                        }
                                    }
                                }
                            }
                        }
                        insert(key.clone(), &pose, &pictures.iter().collect::<Vec<_>>())?;
                        keys[state] = Some(key);
                    }
                    part.variants.insert(d.id.clone(), keys);
                }
            }
            rig.parts.push(part);
        }
        Ok(rig)
    }

    pub fn part_pose(&self, pose: &Pose, part: &BodyPart) -> Pose {
        Pose {
            nodes: pose.nodes.clone(),
            visibility: pose
                .visibility
                .iter()
                .enumerate()
                .map(|(i, v)| if part.objects.contains(&i) { *v } else { 0. })
                .collect(),
            frames: pose.frames.clone(),
            material_frames: pose.material_frames.clone(),
        }
    }

    pub fn sample(
        &self,
        d: &Definition,
        frame: &VehicleFrame,
        seconds: f64,
        bob: Option<Bob<'_>>,
    ) -> Result<Pose> {
        let mut layers = vec![];
        for slot in 0..4 {
            if let Some(thread) = d
                .threads
                .iter()
                .find(|t| t.slot == slot && t.matches(frame.velocity.length()))
            {
                let clip = self
                    .shape
                    .animations
                    .iter()
                    .find(|a| a.name.eq_ignore_ascii_case(&thread.sequence))
                    .with_context(|| {
                        format!("{}: animation thread {} is missing", d.id, thread.sequence)
                    })?;
                let time = seconds * f64::from(thread.rate);
                let time = if clip.looping && clip.duration > 0. {
                    time.rem_euclid(f64::from(clip.duration))
                } else {
                    time
                };
                layers.push(Layer {
                    animation: clip,
                    time: time as f32,
                    weight: 1.,
                });
            }
        }
        if d.weapon.is_some()
            && d.attachment_model.is_none()
            && let Some(clip) = &self.look
        {
            layers.push(Layer {
                animation: clip,
                time: d.look_phase(frame.turret_aim[1]) * clip.duration,
                weight: 1.,
            });
        }
        if let Some(name) = &d.steering_sequence {
            let clip = self
                .steering
                .get(&name.to_ascii_lowercase())
                .context("Missing steering clip")?;
            layers.push(Layer {
                animation: clip,
                time: (1. - frame.steering.clamped(-1., 1.)) * 0.5 * clip.duration,
                weight: 1.,
            });
        }
        if let Some((overlays, values)) = bob {
            for (name, value) in overlays.clips.iter().zip(values) {
                if let Some(clip) = self
                    .shape
                    .animations
                    .iter()
                    .find(|a| a.additive && a.name.eq_ignore_ascii_case(name))
                {
                    layers.push(Layer {
                        animation: clip,
                        time: value.clamped(0., 1.) * clip.duration,
                        weight: 1.,
                    });
                }
            }
        }
        bri_content::animation::sample_layers(&self.shape, &layers)
    }
}

#[cfg(test)]
mod tests;
