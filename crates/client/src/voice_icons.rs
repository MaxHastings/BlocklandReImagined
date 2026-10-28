//! The voice indicator's two pictures (`bri_ui::api::VoiceIndicator`): a white
//! microphone and a red no-entry sign, both with transparent backgrounds. They
//! ship in the executable, are decoded once, and are uploaded for the UI once
//! per GPU as the external textures the HUD draws them with.
use bri_ui::api::{VOICE_ICON_MICROPHONE, VOICE_ICON_NO_ENTRY};

const MICROPHONE: &[u8] = include_bytes!("../assets/voice/icon_microphone.png");
const NO_ENTRY: &[u8] = include_bytes!("../assets/voice/icon_no.png");

struct Icon {
    key: u64,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

pub struct VoiceIcons {
    icons: Vec<Icon>,
    /// Not yet uploaded to the current GPU.
    pending: bool,
}

impl Default for VoiceIcons {
    fn default() -> Self {
        let icons = [
            (VOICE_ICON_MICROPHONE, MICROPHONE),
            (VOICE_ICON_NO_ENTRY, NO_ENTRY),
        ]
        .into_iter()
        .filter_map(|(key, bytes)| match decode(key, bytes) {
            Ok(icon) => Some(icon),
            Err(error) => {
                bri_console::warn(format!("Voice indicator picture unusable: {error}"));
                None
            }
        })
        .collect();
        Self {
            icons,
            pending: true,
        }
    }
}

fn decode(key: u64, bytes: &[u8]) -> Result<Icon, image::ImageError> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)?.to_rgba8();
    Ok(Icon {
        key,
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

impl VoiceIcons {
    /// The GPU was replaced or stopped: upload again.
    pub fn gpu_stopped(&mut self) {
        self.pending = true;
    }

    /// Upload the pictures if the current GPU has none of them yet.
    pub fn register_textures(&mut self, frame: &mut crate::platform::RenderContext<'_>) {
        if !std::mem::take(&mut self.pending) {
            return;
        }
        for icon in &self.icons {
            let size = wgpu::Extent3d {
                width: icon.width,
                height: icon.height,
                depth_or_array_layers: 1,
            };
            let texture = frame.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Voice indicator picture"),
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
                &icon.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * icon.width),
                    rows_per_image: Some(icon.height),
                },
                size,
            );
            frame.ui_renderer.set_external(
                icon.key,
                texture.create_view(&Default::default()),
                (icon.width, icon.height),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_pictures_decode_to_transparent_square_icons() {
        let icons = VoiceIcons::default();
        assert_eq!(icons.icons.len(), 2);
        for icon in &icons.icons {
            assert_eq!((icon.width, icon.height), (64, 64));
            assert_eq!(icon.rgba.len(), 64 * 64 * 4);
            let corner_alpha = icon.rgba[3];
            assert_eq!(corner_alpha, 0, "the corner is transparent");
            assert!(
                icon.rgba.chunks(4).any(|p| p[3] == 255),
                "something is drawn"
            );
        }
        // The microphone is white and the sign red, so the HUD can tint them.
        let opaque = |icon: &Icon| -> [u8; 3] {
            let p = icon.rgba.chunks(4).find(|p| p[3] == 255).unwrap();
            [p[0], p[1], p[2]]
        };
        assert_eq!(opaque(&icons.icons[0]), [255, 255, 255]);
        assert_eq!(opaque(&icons.icons[1]), [255, 0, 0]);
    }
}
