//! Immutable native item names/icons for the HUD. No gameplay authority.
use anyhow::{Context, Result, ensure};
use bri_render::scene::SceneImage;
use bri_ui::api::{IconRef, ToolInfo};
use std::collections::BTreeMap;

const ICON_BASE: u64 = 0x4954_0000;
pub struct ItemUi {
    catalog: BTreeMap<String, ToolInfo>,
    icons: BTreeMap<u64, SceneImage>,
    uploaded: bool,
}
impl ItemUi {
    pub fn new(
        assets: &crate::items::ItemAssets,
        names: &[(String, String)],
        ui_pack: &bri_ui::pack::Pack,
    ) -> Result<Self> {
        ensure!(
            names.len() <= 1024 && names.len() == assets.presentation.items.len(),
            "Item HUD catalog coverage mismatch"
        );
        let mut catalog = BTreeMap::new();
        let mut icons = BTreeMap::new();
        // Native stable-ID ordering makes resource IDs independent of display sorting.
        let ordered: BTreeMap<_, _> = names.iter().cloned().collect();
        ensure!(ordered.len() == names.len(), "Duplicate HUD item ID");
        for (index, (id, name)) in ordered.into_iter().enumerate() {
            let item = assets
                .presentation
                .items
                .get(&id)
                .context("Missing native HUD item")?;
            let icon = if let Some(image) = assets.icon(&id)? {
                let key = ICON_BASE + index as u64;
                icons.insert(key, image.clone());
                IconRef::External(key)
            } else {
                // Vanilla handleItemPickup falls back to the item's first-letter print.
                let letter = name
                    .chars()
                    .next()
                    .map(|c| c.to_lowercase().to_string())
                    .unwrap_or_default();
                let path = format!("add-ons/print_letters_default/icons/{letter}");
                if ui_pack.data.images.contains_key(&path) {
                    IconRef::Pack(path)
                } else if ui_pack
                    .data
                    .images
                    .contains_key("base/client/ui/brickicons/unknown")
                {
                    IconRef::Pack("base/client/ui/brickicons/unknown".into())
                } else {
                    IconRef::None
                }
            };
            let tint = item.tint.map(|c| (c.clamp(0., 1.) * 255.).round() as u8);
            catalog.insert(
                id.clone(),
                ToolInfo {
                    id,
                    name,
                    icon,
                    tint: Some(tint),
                },
            );
        }
        Ok(Self {
            catalog,
            icons,
            uploaded: false,
        })
    }
    pub fn catalog(&self) -> BTreeMap<String, ToolInfo> {
        self.catalog.clone()
    }
    pub fn gpu_stopped(&mut self) {
        self.uploaded = false;
    }
    pub fn register_icons(&mut self, frame: &mut crate::platform::RenderContext<'_>) {
        if self.uploaded {
            return;
        }
        for (&id, image) in &self.icons {
            let size = wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            };
            let texture = frame.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Original native item HUD icon"),
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
                id,
                texture.create_view(&Default::default()),
                (image.width, image.height),
            );
        }
        self.uploaded = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires native item pack003; model-only, no GPU/window/audio"]
    fn all_native_item_names_icons_and_source_tints() -> Result<()> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let weapons =
            bri_net::content_identity::WeaponContent::load(&root.join("weapons-pack-009"))?;
        let assets = crate::items::ItemAssets::load(
            &root.join("item-presentation-pack-010"),
            &root.join("weapons-pack-009"),
        )?;
        let pack = bri_ui::pack::Pack::load(&root.join("ui-pack-004"))?;
        let ui = ItemUi::new(&assets, &weapons.item_choices, &pack)?;
        assert_eq!(ui.catalog.len(), 21);
        assert_eq!(ui.icons.len(), 17);
        for (id, name) in weapons.item_choices {
            let info = &ui.catalog[&id];
            assert_eq!(info.name, name);
            assert_eq!(
                info.tint,
                Some(
                    assets.presentation.items[&id]
                        .tint
                        .map(|c| (c.clamp(0., 1.) * 255.).round() as u8)
                )
            );
            if let IconRef::External(key) = info.icon {
                assert_eq!(ui.icons[&key].rgba, assets.icon(&id)?.unwrap().rgba);
            } else {
                assert!(assets.icon(&id)?.is_none());
                assert!(id.contains("ballitem"));
                assert_eq!(
                    info.icon,
                    IconRef::Pack("add-ons/print_letters_default/icons/b".into())
                );
            }
        }
        Ok(())
    }
    #[test]
    #[ignore = "requires native item pack003; model-only, no GPU/window/audio"]
    fn add_on_weapons_without_presentation_reuse_stock_icons() -> Result<()> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.join("../../content");
        let extras = vec![
            (
                "addons/duplicator-tool/assets".to_string(),
                manifest.join("../../packages/duplicator/duplicator-tool/assets"),
            ),
            (
                "addons/sample-bubble-blaster/assets".to_string(),
                manifest.join("../../packages/samples/sample-bubble-blaster/assets"),
            ),
        ];
        let weapons = bri_net::content_identity::WeaponContent::load_with(
            &root.join("weapons-pack-009"),
            &extras,
        )?;
        let assets = crate::items::ItemAssets::load_with(
            &root.join("item-presentation-pack-010"),
            &root.join("weapons-pack-009"),
            &extras,
        )?;
        let pack = bri_ui::pack::Pack::load(&root.join("ui-pack-004"))?;
        let ui = ItemUi::new(&assets, &weapons.item_choices, &pack)?;
        assert_eq!(ui.catalog.len(), 23);
        let IconRef::External(gun) = ui.catalog["sample-bubble-blaster:weapon/bubble_blaster"].icon
        else {
            panic!("the Bubble Blaster should show the gun icon");
        };
        assert_eq!(ui.icons[&gun].rgba, assets.icon("v20.weapon.gunitem")?.unwrap().rgba);
        let tool = "duplicator-tool:weapon/duplicator";
        assert_eq!(ui.catalog[tool].name, "Duplicator");
        let IconRef::External(key) = ui.catalog[tool].icon else {
            panic!("the Duplicator should show the wand icon");
        };
        assert_eq!(
            ui.icons[&key].rgba,
            assets.icon("v20.weapon.wanditem")?.unwrap().rgba
        );
        Ok(())
    }
    #[test]
    #[ignore = "native pack003 and bounded offscreen GPU; no window or audio"]
    fn original_hud_icons_upload_and_reregister_after_gpu_reset() -> Result<()> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let weapons =
            bri_net::content_identity::WeaponContent::load(&root.join("weapons-pack-009"))?;
        let assets = crate::items::ItemAssets::load(
            &root.join("item-presentation-pack-010"),
            &root.join("weapons-pack-009"),
        )?;
        let pack = bri_ui::pack::Pack::load(&root.join("ui-pack-004"))?;
        let mut icons = ItemUi::new(&assets, &weapons.item_choices, &pack)?;
        let gpu = bri_ui::gpu::Headless::new()?;
        for _ in 0..2 {
            let mut renderer = bri_ui::gpu::UiRenderer::new(&gpu.device, &gpu.queue);
            let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("HUD icon registration fixture"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = target.create_view(&Default::default());
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            icons.register_icons(&mut crate::platform::RenderContext {
                device: &gpu.device,
                queue: &gpu.queue,
                encoder: &mut encoder,
                target: &view,
                format: wgpu::TextureFormat::Rgba8Unorm,
                size: (1, 1),
                ui_renderer: &mut renderer,
            });
            let mut draw = bri_ui::draw::DrawList::new(bri_ui::geom::Rect::new(0, 0, 160, 160));
            for (index, item) in icons.catalog.values().enumerate() {
                let (key, src) = match &item.icon {
                    IconRef::External(id) => {
                        (bri_ui::pack::TexKey::External(*id), [0., 0., 1., 1.])
                    }
                    IconRef::Pack(path) => {
                        let (width, height) = pack.image_size(path).unwrap();
                        (
                            bri_ui::pack::TexKey::Image(path.clone()),
                            [0., 0., width as f32, height as f32],
                        )
                    }
                    IconRef::None => panic!("Missing native icon or original letter fallback"),
                };
                draw.image(
                    key,
                    src,
                    [
                        ((index % 5) * 32) as f32,
                        ((index / 5) * 32) as f32,
                        32.,
                        32.,
                    ],
                    [255; 4],
                    bri_ui::draw::Filter::Linear,
                );
            }
            let pixels = gpu.render_rgba(&mut renderer, &pack, &draw, (160, 160), 1., [0.; 4])?;
            assert_eq!(renderer.missing_textures().count(), 0);
            for index in 0..21 {
                assert!(
                    (0..32).any(|y| (0..32).any(|x| pixels
                        [(((index / 5 * 32 + y) * 160 + index % 5 * 32 + x) * 4) + 3]
                        > 0)),
                    "Original icon {index} rendered empty"
                );
            }
            icons.gpu_stopped();
        }
        Ok(())
    }
}
