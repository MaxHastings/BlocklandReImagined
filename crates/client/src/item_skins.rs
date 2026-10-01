//! Item skins (`looks.json`, [`ItemSkin`]): an Add-On's shader drawn over
//! every copy of one of its items, so the item looks the same in a hand,
//! dropped, on a spawn brick and in a mirror. The game draws the skins,
//! not the Add-On's code: they are part of the item's look.
//!
//! The skins are one render layer of Add-On shaders, held to an Add-On's
//! GPU budgets ([`LayerSource`]); one that runs far over them stops, and
//! the items draw as their plain models. Two renderers draw the layer:
//! the player's view (the holder's first-person copy, not the mirror's)
//! and mirrors and the environment probe (the copy as others see it).
use crate::items::{ItemAssets, ItemSkin};
use crate::world_items::{ItemIdentity, SkinnedCopy, WorldItems};
use bri_client_sandbox::gpu::{Camera, LayerRenderer, LayerSource};
use bri_client_sandbox::host::{
    Blend, Budgets, Draw, Frame, Layer, Material, Mesh, Space, Stopped,
};
use bri_client_sandbox::shader::Shader;
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The light the skins are drawn in: the scene's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Light {
    /// The direction sunlight travels.
    pub sun_direction: [f32; 3],
    pub sun_color: [f32; 3],
    pub ambient: [f32; 3],
}

/// Every skin's shader and material, and the models drawn with them.
struct SkinLayer {
    shaders: Vec<Shader>,
    layer: Layer,
    budgets: Budgets,
    stopped: Option<Stopped>,
    strikes: u32,
}

impl LayerSource for SkinLayer {
    fn shaders(&self) -> &[Shader] {
        &self.shaders
    }
    fn layer(&self) -> &Layer {
        &self.layer
    }
    fn budgets(&self) -> &Budgets {
        &self.budgets
    }
    fn stop(&mut self, reason: Stopped) -> Stopped {
        self.stopped = Some(reason.clone());
        reason
    }
    fn report_gpu_time(&mut self, ms: f32) -> Result<(), Stopped> {
        if let Some(stopped) = &self.stopped {
            return Err(stopped.clone());
        }
        if ms > self.budgets.gpu_stop_ms {
            return Err(self.stop(Stopped::Gpu(format!(
                "one frame took {ms:.0} ms of graphics time (the limit is {:.0} ms)",
                self.budgets.gpu_stop_ms
            ))));
        }
        if ms > self.budgets.gpu_ms_per_frame {
            self.strikes += 1;
            if self.strikes >= self.budgets.gpu_strikes {
                return Err(self.stop(Stopped::Gpu(format!(
                    "{ms:.1} ms a frame for {} frames in a row (budget {:.1} ms)",
                    self.strikes, self.budgets.gpu_ms_per_frame
                ))));
            }
        } else {
            self.strikes = 0;
        }
        Ok(())
    }
}

pub struct ItemSkins {
    /// The presentation the layer was built from.
    assets: Option<Arc<ItemAssets>>,
    source: SkinLayer,
    /// Each skinned image's skin, material and whether a server sent it.
    materials: BTreeMap<String, (ItemSkin, usize, bool)>,
    /// Each drawn model's mesh in the layer; `None` for one that will not
    /// build.
    meshes: BTreeMap<String, Option<usize>>,
    /// This frame's draws: the player's view, then mirrors'.
    frames: [Frame; 2],
    renderers: Option<[LayerRenderer; 2]>,
    messages: Vec<String>,
}

impl Default for ItemSkins {
    fn default() -> Self {
        Self {
            assets: None,
            source: SkinLayer {
                shaders: Vec::new(),
                layer: Layer::default(),
                budgets: Budgets::default(),
                stopped: None,
                strikes: 0,
            },
            materials: BTreeMap::new(),
            meshes: BTreeMap::new(),
            frames: Default::default(),
            renderers: None,
            messages: Vec::new(),
        }
    }
}

impl ItemSkins {
    /// The layer for `assets`' skins: a shader per distinct skin shader
    /// and a material per skinned image.
    fn build(assets: Arc<ItemAssets>) -> Self {
        let mut skins = Self::default();
        let mut compiled: Vec<&crate::items::SkinShader> = Vec::new();
        for (image, (skin, shader)) in assets.skins() {
            let index = match compiled.iter().position(|s| *s == shader) {
                Some(index) => index,
                None => match bri_client_sandbox::shader::compile(&shader.name, &shader.source) {
                    Ok(module) => {
                        skins.source.shaders.push(module);
                        compiled.push(shader);
                        compiled.len() - 1
                    }
                    // Checked when the item was read; kept out if not.
                    Err(error) => {
                        skins.messages.push(format!("{}: {error}", shader.name));
                        continue;
                    }
                },
            };
            skins.materials.insert(
                image.clone(),
                (
                    skin.clone(),
                    skins.source.layer.materials.len(),
                    shader.downloaded,
                ),
            );
            skins.source.layer.materials.push(Material {
                shader: index,
                params: [[0.; 4]; 4],
                blend: Blend::Opaque,
                space: Space::World,
            });
        }
        skins.assets = Some(assets);
        skins
    }

    /// This frame's draws for `copies`, in `light`, with each model's mesh
    /// from `mesh`. A server's skins are drawn only when it is `trusted`.
    fn frame(
        &mut self,
        copies: &[SkinnedCopy],
        trusted: bool,
        light: Light,
        mut mesh: impl FnMut(&str) -> Option<Arc<Mesh>>,
    ) {
        for frame in &mut self.frames {
            frame.draws.clear();
            frame.triangles = 0;
        }
        if self.source.stopped.is_some() {
            return;
        }
        let budgets = &self.source.budgets;
        for copy in copies {
            let Some((skin, material, _)) = self
                .materials
                .get(&copy.image)
                .filter(|(_, _, downloaded)| trusted || !downloaded)
            else {
                continue;
            };
            let layer = &mut self.source.layer;
            let index = *self.meshes.entry(copy.model.clone()).or_insert_with(|| {
                let built = mesh(&copy.model)?;
                (layer.meshes.len() < budgets.meshes
                    && built.vertices.len() <= budgets.mesh_vertices)
                    .then(|| {
                        layer.meshes.push(Mesh::clone(&built));
                        layer.meshes.len() - 1
                    })
            });
            let Some(index) = index else {
                continue;
            };
            let draw = Draw {
                mesh: index,
                material: *material,
                model: copy.transform.to_cols_array(),
                params: Some(params(skin, copy, light)),
            };
            let triangles = (layer.meshes[index].indices.len() / 3) as u64;
            for (frame, seen) in self
                .frames
                .iter_mut()
                .zip([!copy.reflected, !copy.first_person])
            {
                if seen && frame.draws.len() < budgets.draws_per_frame {
                    frame.draws.push(draw);
                    frame.triangles += triangles;
                }
            }
        }
    }

    /// Build this frame's draws from what `items` drew and upload them.
    /// Renderers are built lazily for the pass's formats.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        samples: u32,
        items: &mut WorldItems,
        trusted: bool,
        light: Light,
        view_projection: Mat4,
        eye: Vec3,
        size: [u32; 2],
        time: f32,
    ) {
        if !self
            .assets
            .as_ref()
            .is_some_and(|a| Arc::ptr_eq(a, items.assets()))
        {
            let messages = std::mem::take(&mut self.messages);
            *self = Self::build(items.assets().clone());
            self.messages.splice(0..0, messages);
        }
        if self.materials.is_empty() {
            return;
        }
        let copies = items.skinned().to_vec();
        self.frame(&copies, trusted, light, |model| items.addon_mesh(model));
        if self.source.stopped.is_some() {
            return;
        }
        let renderers = self.renderers.get_or_insert_with(|| {
            [0, 1].map(|_| {
                LayerRenderer::new(
                    device,
                    queue,
                    None,
                    color,
                    Some(depth),
                    samples,
                    Budgets::default().draws_per_frame,
                )
            })
        });
        let camera = Camera {
            view_proj: view_projection,
            position: eye,
            size,
            normal_fov: 90.,
        };
        for (renderer, frame) in renderers.iter_mut().zip(&self.frames) {
            if let Err(reason) =
                renderer.prepare(device, queue, &mut self.source, frame, camera, [time, 0.])
            {
                self.messages
                    .push(format!("Item skins stopped, so items draw plain: {reason}"));
                for frame in &mut self.frames {
                    frame.draws.clear();
                }
                return;
            }
        }
    }

    /// The skins from another view (a mirror's or the environment
    /// probe's), after [`Self::prepare`].
    pub fn prepare_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: usize,
        view_projection: Mat4,
        eye: Vec3,
    ) {
        if let Some([_, mirrors]) = &mut self.renderers {
            mirrors.prepare_view(device, queue, view, view_projection, eye);
        }
    }

    /// Draw the skins into the player's view.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some([player, _]) = &self.renderers {
            player.draw(pass, &self.frames[0], &self.source.layer);
        }
    }

    /// Draw the skins from a view [`Self::prepare_view`] prepared.
    pub fn render_view(&self, pass: &mut wgpu::RenderPass<'_>, view: usize) {
        if let Some([_, mirrors]) = &self.renderers {
            mirrors.draw_view(pass, &self.frames[1], &self.source.layer, view);
        }
    }

    /// After the pass `render` drew into ends, before it is submitted.
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        if let Some([player, _]) = &self.renderers {
            player.resolve(encoder);
        }
    }

    /// The device went away or the pass changed shape: rebuild renderers.
    pub fn gpu_stopped(&mut self) {
        self.renderers = None;
    }

    /// Lines for the player since the last call.
    pub fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }
}

/// A copy's skin parameters (`ItemSkin`'s contract): colour and energy,
/// sun direction and seed, sun colour, ambient light.
fn params(skin: &ItemSkin, copy: &SkinnedCopy, light: Light) -> [[f32; 4]; 4] {
    let [r, g, b] = skin.color;
    let [x, y, z] = light.sun_direction;
    // The same copy keeps the same seed: its holder's, or its own.
    let seed = match copy.identity {
        ItemIdentity::Mounted(id, _)
        | ItemIdentity::Reflected(id, _)
        | ItemIdentity::Static(id)
        | ItemIdentity::Drop(id)
        | ItemIdentity::Projectile(id)
        | ItemIdentity::Loose(id) => (id % 4096) as f32,
    };
    let rgb = |c: [f32; 3]| [c[0], c[1], c[2], 0.];
    [
        [r, g, b, if copy.energized { 1. } else { 0. }],
        [x, y, z, seed],
        rgb(light.sun_color),
        rgb(light.ambient),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(identity: ItemIdentity, first_person: bool, reflected: bool) -> SkinnedCopy {
        SkinnedCopy {
            identity,
            image: "gun".into(),
            model: "gun.dts".into(),
            transform: Mat4::from_translation(Vec3::new(1., 2., 3.)),
            energized: false,
            first_person,
            reflected,
        }
    }

    fn skins(skin: ItemSkin) -> ItemSkins {
        let mut skins = ItemSkins::default();
        skins
            .source
            .shaders
            .push(bri_client_sandbox::shader::compile("skin.wgsl", SHADER).expect("shader"));
        skins.materials.insert("gun".into(), (skin, 0, false));
        skins.source.layer.materials.push(Material {
            shader: 0,
            params: [[0.; 4]; 4],
            blend: Blend::Opaque,
            space: Space::World,
        });
        skins
    }

    const SHADER: &str = "
@vertex fn vs_main(v: BriVertex) -> @builtin(position) vec4<f32> {
    return bri_frame.view_proj * bri_draw.model * vec4<f32>(v.position, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4<f32> { return bri_draw.params[0]; }
";

    fn mesh() -> Arc<Mesh> {
        Arc::new(Mesh {
            vertices: vec![
                bri_client_sandbox::host::Vertex {
                    position: [0.; 3],
                    normal: [0., 0., 1.],
                    uv: [0.; 2],
                };
                3
            ],
            indices: vec![0, 1, 2],
        })
    }

    #[test]
    fn each_copy_is_skinned_in_the_views_that_see_it() {
        let skin = ItemSkin {
            shader: "skin.wgsl".into(),
            color: [0.3, 0.95, 1.],
            energy_states: vec!["Grab".into()],
        };
        let mut skins = skins(skin);
        let mut built = 0;
        let copies = [
            copy(ItemIdentity::Static(7), false, false),
            copy(ItemIdentity::Drop(8), false, false),
            copy(ItemIdentity::Mounted(2, 0), true, false),
            SkinnedCopy {
                transform: Mat4::from_translation(Vec3::new(9., 9., 9.)),
                ..copy(ItemIdentity::Reflected(2, 0), false, true)
            },
            copy(ItemIdentity::Mounted(3, 0), false, false),
        ];
        skins.frame(&copies, true, Light::default(), |_| {
            built += 1;
            Some(mesh())
        });
        assert_eq!(built, 1, "a model's mesh is built once");
        let seeds = |frame: &Frame| -> Vec<f32> {
            frame
                .draws
                .iter()
                .map(|d| d.params.unwrap()[1][3])
                .collect()
        };
        // The player sees their own first-person copy, not the mirror's.
        assert_eq!(seeds(&skins.frames[0]), [7., 8., 2., 3.]);
        // Mirrors see it as others do.
        assert_eq!(seeds(&skins.frames[1]), [7., 8., 2., 3.]);
        let mirrored = |frame: &Frame| {
            frame
                .draws
                .iter()
                .filter(|d| d.model == copies[3].transform.to_cols_array())
                .count()
        };
        assert_eq!(
            (mirrored(&skins.frames[0]), mirrored(&skins.frames[1])),
            (0, 1)
        );
        assert_eq!(skins.frames[0].triangles, 4);
        // Unknown images draw plain.
        let other = SkinnedCopy {
            image: "hammer".into(),
            ..copy(ItemIdentity::Drop(1), false, false)
        };
        skins.frame(&[other], true, Light::default(), |_| Some(mesh()));
        assert!(skins.frames.iter().all(|f| f.draws.is_empty()));
    }

    /// A skin is WGSL, run under the same trust as an Add-On's code: one a
    /// server sent draws only when the player trusts that server.
    #[test]
    fn a_servers_skin_draws_only_when_it_is_trusted() {
        let skin = ItemSkin {
            shader: "skin.wgsl".into(),
            color: [1.; 3],
            energy_states: Vec::new(),
        };
        let mut skins = skins(skin);
        skins.materials.get_mut("gun").unwrap().2 = true;
        let copies = [copy(ItemIdentity::Drop(1), false, false)];
        skins.frame(&copies, false, Light::default(), |_| Some(mesh()));
        assert!(skins.frames.iter().all(|f| f.draws.is_empty()));
        skins.frame(&copies, true, Light::default(), |_| Some(mesh()));
        assert!(skins.frames.iter().all(|f| f.draws.len() == 1));
    }

    #[test]
    fn a_skin_lights_up_in_its_energy_states() {
        let skin = ItemSkin {
            shader: "skin.wgsl".into(),
            color: [0.3, 0.95, 1.],
            energy_states: vec!["Grab".into()],
        };
        let light = Light {
            sun_direction: [0., -1., 0.],
            sun_color: [1., 0.9, 0.8],
            ambient: [0.2, 0.2, 0.3],
        };
        let mut held = copy(ItemIdentity::Mounted(5, 0), false, false);
        held.energized = true;
        assert_eq!(
            params(&skin, &held, light),
            [
                [0.3, 0.95, 1., 1.],
                [0., -1., 0., 5.],
                [1., 0.9, 0.8, 0.],
                [0.2, 0.2, 0.3, 0.]
            ]
        );
        let dropped = copy(ItemIdentity::Drop(5), false, false);
        assert_eq!(params(&skin, &dropped, light)[0], [0.3, 0.95, 1., 0.]);
    }

    #[test]
    fn a_stopped_skin_layer_draws_nothing() {
        let skin = ItemSkin {
            shader: "skin.wgsl".into(),
            color: [1.; 3],
            energy_states: Vec::new(),
        };
        let mut skins = skins(skin);
        for _ in 0..skins.source.budgets.gpu_strikes {
            let _ = skins
                .source
                .report_gpu_time(skins.source.budgets.gpu_ms_per_frame + 1.);
        }
        assert!(skins.source.stopped.is_some());
        skins.frame(
            &[copy(ItemIdentity::Drop(1), false, false)],
            true,
            Light::default(),
            |_| Some(mesh()),
        );
        assert!(skins.frames.iter().all(|f| f.draws.is_empty()));
    }
}
