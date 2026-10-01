//! Immutable native item names/icons for the HUD. No gameplay authority.
use anyhow::{Result, ensure};
use bri_render::scene::SceneImage;
use bri_ui::api::{IconRef, ToolInfo};
use std::collections::BTreeMap;

const ICON_BASE: u64 = 0x4954_0000;
/// Scope overlays' texture keys (`bri_weapons::Zoom::overlay`).
const OVERLAY_BASE: u64 = 0x5343_0000;
pub struct ItemUi {
    catalog: BTreeMap<String, ToolInfo>,
    /// HUD icons and scope overlays, by texture key.
    icons: BTreeMap<u64, SceneImage>,
    /// Each image with a scope overlay: its texture key and aspect ratio
    /// (width over height).
    overlays: BTreeMap<String, (u64, f32)>,
    /// Icons still being drawn from their models, shown when they are.
    drawing: Vec<(u64, crate::items::DrawnIcon)>,
    uploaded: bool,
}
impl ItemUi {
    /// One HUD row per weapon item in `names`, the game's item list. The
    /// rows come from that list, never from the presentation, so the two
    /// cannot disagree: an item without art of its own shows its first
    /// letter in white, as v20's `handleItemPickup` does.
    pub fn new(
        assets: &crate::items::ItemAssets,
        names: &[(String, String)],
        ui_pack: &bri_ui::pack::Pack,
    ) -> Result<Self> {
        ensure!(names.len() <= 1024, "Item HUD catalog budget exceeded");
        let mut catalog = BTreeMap::new();
        let mut icons = BTreeMap::new();
        let mut drawing = Vec::new();
        // Native stable-ID ordering makes resource IDs independent of display sorting.
        let ordered: BTreeMap<_, _> = names.iter().cloned().collect();
        ensure!(ordered.len() == names.len(), "Duplicate HUD item ID");
        for (index, (id, name)) in ordered.into_iter().enumerate() {
            let item = assets.presentation.items.get(&id);
            let image = item.and_then(|_| assets.icon(&id).ok().flatten());
            let key = ICON_BASE + index as u64;
            let pending = item
                .and_then(|_| assets.drawn_icon(&id))
                .filter(|slot| slot.get().is_none());
            if let Some(slot) = pending.clone() {
                drawing.push((key, slot));
            }
            let icon = if let Some(image) = image {
                icons.insert(key, image.clone());
                IconRef::External(key)
            } else if pending.is_some() {
                // Nothing to show until it is drawn.
                icons.insert(key, SceneImage { label: id.clone(), width: 1, height: 1, rgba: vec![0; 4], srgb: false });
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
            let tint = item
                .map_or([1.; 4], |item| item.tint)
                .map(|c| (c.clamp(0., 1.) * 255.).round() as u8);
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
        // Scope overlays by image, keyed in the stable order of their
        // image ids; the same picture shared by two images is uploaded once.
        let mut overlays = BTreeMap::new();
        let mut by_texture = BTreeMap::new();
        for (image, presented) in &assets.presentation.images {
            let Some(texture) = presented.overlay.as_ref() else {
                continue;
            };
            let Some(picture) = assets.texture(texture) else {
                continue;
            };
            let next = OVERLAY_BASE + by_texture.len() as u64;
            let key = *by_texture.entry(texture.clone()).or_insert(next);
            icons.entry(key).or_insert_with(|| picture.clone());
            overlays.insert(
                image.clone(),
                (key, picture.width as f32 / picture.height as f32),
            );
        }
        Ok(Self {
            catalog,
            icons,
            overlays,
            drawing,
            uploaded: false,
        })
    }
    /// The scope overlay drawn while aiming `image`, if it has one: its
    /// texture key and aspect ratio.
    pub fn scope_overlay(&self, image: &str) -> Option<(u64, f32)> {
        self.overlays.get(image).copied()
    }
    pub fn catalog(&self) -> BTreeMap<String, ToolInfo> {
        self.catalog.clone()
    }
    pub fn gpu_stopped(&mut self) {
        self.uploaded = false;
    }
    /// Take the icons drawn since the last call; true when there were any.
    pub fn take_drawn(&mut self) -> bool {
        let before = self.drawing.len();
        let icons = &mut self.icons;
        self.drawing.retain(|(key, slot)| match slot.get() {
            Some(image) => {
                icons.insert(*key, image.clone());
                false
            }
            None => true,
        });
        let changed = self.drawing.len() != before;
        if changed {
            self.uploaded = false;
        }
        changed
    }
    pub fn register_icons(&mut self, frame: &mut crate::platform::RenderContext<'_>) {
        if !self.drawing.is_empty() {
            self.take_drawn();
        }
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
    /// An icon still being drawn replaces the slot's stand-in once it is,
    /// and the icons are uploaded again.
    #[test]
    fn a_drawn_icon_replaces_its_stand_in() {
        let blank = SceneImage { label: "x".into(), width: 1, height: 1, rgba: vec![0; 4], srgb: false };
        let slot = crate::items::DrawnIcon::default();
        let mut ui = ItemUi {
            catalog: BTreeMap::new(),
            icons: BTreeMap::from([(ICON_BASE, blank)]),
            drawing: vec![(ICON_BASE, slot.clone())],
            overlays: BTreeMap::new(),
            uploaded: true,
        };
        assert!(!ui.take_drawn(), "not drawn yet");
        assert!(ui.uploaded);
        let drawn = SceneImage { label: "x".into(), width: 2, height: 1, rgba: vec![9; 8], srgb: false };
        slot.set(drawn.clone()).unwrap();
        assert!(ui.take_drawn());
        assert_eq!(ui.icons[&ICON_BASE].rgba, drawn.rgba);
        assert!(!ui.uploaded && ui.drawing.is_empty());
        assert!(!ui.take_drawn(), "only once");
    }
    use crate::items::fixture::Items;
    use std::rc::Rc;

    /// The item packs, the weapons' item list and a UI pack with the
    /// letter prints: made up, or the generated v20 content.
    struct Hud {
        items: Items,
        ui: Rc<bri_ui::pack::Pack>,
    }
    impl Hud {
        fn synthetic() -> Result<Self> {
            let mut data = bri_ui::schema::UiPack::default();
            for letter in 'a'..='z' {
                bri_ui::testing::add_image(
                    &mut data,
                    &format!("add-ons/print_letters_default/icons/{letter}"),
                    16,
                    16,
                );
            }
            Ok(Self {
                items: Items::synthetic()?,
                ui: bri_ui::testing::pack(data),
            })
        }
        fn content() -> Result<Self> {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
            Ok(Self {
                items: Items::content()?,
                ui: Rc::new(bri_ui::pack::Pack::load(&root.join("ui-pack-004"))?),
            })
        }
        fn weapons(&self) -> Result<bri_net::content_identity::WeaponContent> {
            bri_net::content_identity::WeaponContent::load(&self.items.weapons)
        }
    }
    crate::testing::synthetic_and_content!(
        Hud: all_native_item_names_icons_and_source_tints,
        add_on_weapons_without_presentation_reuse_stock_icons,
        original_hud_icons_upload_and_reregister_after_gpu_reset,
    );
    /// The letter print shown for an item without an icon.
    fn letter(name: &str) -> IconRef {
        let first = name.chars().next().map(|c| c.to_lowercase().to_string());
        IconRef::Pack(format!(
            "add-ons/print_letters_default/icons/{}",
            first.unwrap_or_default()
        ))
    }
    /// The real catalog: 21 items, 17 with icons, the rest balls showing
    /// the B print.
    #[test]
    #[ignore = "requires generated v20 content"]
    fn original_item_hud_catalog() -> Result<()> {
        let fx = Hud::content()?;
        let weapons = fx.weapons()?;
        let ui = ItemUi::new(&fx.items.assets()?, &weapons.item_choices, &fx.ui)?;
        assert_eq!(ui.catalog.len(), 21);
        assert_eq!(ui.icons.len(), 17);
        for (id, info) in &ui.catalog {
            if !matches!(info.icon, IconRef::External(_)) {
                assert!(id.contains("ballitem"));
                assert_eq!(
                    info.icon,
                    IconRef::Pack("add-ons/print_letters_default/icons/b".into())
                );
            }
        }
        Ok(())
    }
    fn all_native_item_names_icons_and_source_tints(fx: &Hud) -> Result<()> {
        let weapons = fx.weapons()?;
        let assets = fx.items.assets()?;
        let pack = &fx.ui;
        let ui = ItemUi::new(&assets, &weapons.item_choices, pack)?;
        assert_eq!(ui.catalog.len(), weapons.item_choices.len());
        let with_icons = weapons
            .item_choices
            .iter()
            .filter(|(id, _)| assets.icon(id).is_ok_and(|i| i.is_some()))
            .count();
        assert!(
            with_icons > 0 && with_icons < ui.catalog.len(),
            "icons and letters"
        );
        assert_eq!(ui.icons.len(), with_icons);
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
                assert_eq!(info.icon, letter(&name));
            }
        }
        Ok(())
    }
    fn add_on_weapons_without_presentation_reuse_stock_icons(fx: &Hud) -> Result<()> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
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
        let base: Vec<String> = fx
            .weapons()?
            .item_choices
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let weapons =
            bri_net::content_identity::WeaponContent::load_with(&fx.items.weapons, &extras)?;
        let assets = crate::items::ItemAssets::load_with(
            &fx.items.presentation,
            &fx.items.weapons,
            &extras,
        )?;
        let ui = ItemUi::new(&assets, &weapons.item_choices, &fx.ui)?;
        // The two Add-Ons each add one item (unless the base game has it).
        let added = [
            "duplicator-tool:weapon/duplicator",
            "sample-bubble-blaster:weapon/bubble_blaster",
        ];
        let new = added
            .iter()
            .filter(|id| !base.iter().any(|b| b == *id))
            .count();
        assert_eq!(ui.catalog.len(), base.len() + new);
        let IconRef::External(gun) = ui.catalog["sample-bubble-blaster:weapon/bubble_blaster"].icon
        else {
            panic!("the Bubble Blaster should show the gun icon");
        };
        assert_eq!(
            ui.icons[&gun].rgba,
            assets.icon("v20.weapon.gunitem")?.unwrap().rgba
        );
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
    fn original_hud_icons_upload_and_reregister_after_gpu_reset(fx: &Hud) -> Result<()> {
        let weapons = fx.weapons()?;
        let assets = fx.items.assets()?;
        let pack = &*fx.ui;
        let mut icons = ItemUi::new(&assets, &weapons.item_choices, pack)?;
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
            // Five icons a row, 32 pixels each.
            let height = icons.catalog.len().div_ceil(5) * 32;
            let mut draw =
                bri_ui::draw::DrawList::new(bri_ui::geom::Rect::new(0, 0, 160, height as i32));
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
            let pixels = gpu.render_rgba(
                &mut renderer,
                pack,
                &draw,
                (160, height as u32),
                1.,
                [0.; 4],
            )?;
            assert_eq!(renderer.missing_textures().count(), 0);
            for index in 0..icons.catalog.len() {
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
