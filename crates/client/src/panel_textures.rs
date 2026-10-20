//! The pictures an Add-On's panels draw (`show_panel` backgrounds, images and
//! buttons): each is an asset of the Add-On that showed the panel, decoded
//! when a panel names it and uploaded for the UI once per GPU.
use bri_package_runtime::Catalog;
use bri_render::scene::SceneImage;
use bri_ui::api::PanelPicture;
use std::collections::BTreeMap;

/// Texture keys of panel pictures (`bri_ui::pack::TexKey::External`).
const KEY_BASE: u64 = 0x5041_0000;
/// Most pictures panels draw in a session.
const MAX_TEXTURES: usize = 256;

#[derive(Default)]
pub struct PanelTextures {
    /// Add-On and texture id to its key and size.
    keys: BTreeMap<(String, String), PanelPicture>,
    images: BTreeMap<u64, SceneImage>,
    /// Keys not yet uploaded to the current GPU.
    pending: Vec<u64>,
}

impl PanelTextures {
    /// The picture `texture` names among `package`'s assets, read the first
    /// time. None when the Add-On has no such image (it is reported once, and
    /// the panel draws without it).
    pub fn picture(
        &mut self,
        catalog: Option<&Catalog>,
        package: &str,
        texture: &str,
    ) -> Option<PanelPicture> {
        let name = (package.to_owned(), texture.to_owned());
        if let Some(picture) = self.keys.get(&name) {
            return Some(*picture);
        }
        let decoded = catalog
            .and_then(|c| c.packages.get(package))
            .and_then(|p| p.assets.iter().find(|a| a.id == texture))
            .and_then(|a| image::load_from_memory(&a.bytes).ok())
            .map(|i| i.to_rgba8());
        let Some(rgba) = decoded.filter(|_| self.keys.len() < MAX_TEXTURES) else {
            crate::add_on_health::report(bri_package::health::Problem::new(
                package,
                bri_package::health::Kind::Presentation,
                texture,
                "its panel picture could not be read; the panel is drawn without it",
            ));
            return None;
        };
        let key = KEY_BASE + self.keys.len() as u64;
        let (width, height) = rgba.dimensions();
        let picture = PanelPicture {
            key,
            size: [width, height],
        };
        self.keys.insert(name, picture);
        self.images.insert(
            key,
            SceneImage {
                label: texture.to_owned(),
                width,
                height,
                rgba: rgba.into_raw(),
                srgb: false,
            },
        );
        self.pending.push(key);
        Some(picture)
    }

    pub fn gpu_stopped(&mut self) {
        self.pending = self.images.keys().copied().collect();
    }

    /// Upload the pictures read since the last call.
    pub fn register_textures(&mut self, frame: &mut crate::platform::RenderContext<'_>) {
        for key in std::mem::take(&mut self.pending) {
            let Some(image) = self.images.get(&key) else {
                continue;
            };
            let size = wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            };
            let texture = frame.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Add-On panel picture"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            frame.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &image.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * image.width),
                    rows_per_image: Some(image.height),
                },
                size,
            );
            frame.ui_renderer.set_external(
                key,
                texture.create_view(&Default::default()),
                (image.width, image.height),
            );
        }
    }
}
