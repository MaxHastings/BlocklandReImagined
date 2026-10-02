//! A save's picture for Load Bricks' preview. v20 wrote a HUD-less
//! screenshot beside each save (`saves/<Map>/<name>.jpg` next to
//! `<name>.bls`, `saveBricks`), and `LoadBricks_FileClick` showed it,
//! falling back to the map's picture. Native saves keep the same pairing:
//! `<name>.jpg` beside `<name>.world.json`.
use anyhow::{Context, Result, ensure};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc,
};

/// The external UI texture the picked save's picture is drawn from.
pub const ID: u64 = 0x425249_53415645;
/// Pictures are kept to twice v20's 294x220 preview box, which covers the
/// interface scaled up on a large screen.
pub const FIT: [u32; 2] = [588, 440];
/// Larger files are not a save picture players meant to show.
pub(crate) const MAX_BYTES: u64 = 32 * 1024 * 1024;

/// The picture belonging to the save file `save` (`.bls` or `.world.json`).
pub fn path_for(save: &Path) -> Option<PathBuf> {
    let name = save.file_name()?.to_str()?;
    let stem = name.strip_suffix(".world.json").or_else(|| {
        name.len().checked_sub(4).and_then(|end| {
            name[end..]
                .eq_ignore_ascii_case(".bls")
                .then(|| &name[..end])
        })
    })?;
    Some(save.with_file_name(format!("{stem}.jpg")))
}

/// A decoded picture, scaled to fit [`FIT`].
#[derive(Debug)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Read and scale the picture at `path`; `None` when there is none.
pub fn read(path: &Path) -> Result<Option<Picture>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| path.display().to_string()),
    };
    let mut bytes = vec![];
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_BYTES, "Save picture is too large");
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    let image = reader.decode()?;
    let image = if image.width() > FIT[0] || image.height() > FIT[1] {
        image.resize(FIT[0], FIT[1], image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let rgba = image.into_rgba8();
    Ok(Some(Picture {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    }))
}

/// Put `picture` in the UI texture [`ID`] draws from.
pub fn upload(frame: &mut crate::platform::RenderContext<'_>, picture: &Picture) {
    upload_as(frame, ID, picture);
}

/// Put `picture` in UI texture `id`.
pub fn upload_as(frame: &mut crate::platform::RenderContext<'_>, id: u64, picture: &Picture) {
    let size = wgpu::Extent3d {
        width: picture.width,
        height: picture.height,
        depth_or_array_layers: 1,
    };
    let texture = frame.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Save picture"),
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
        &picture.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * picture.width),
            rows_per_image: Some(picture.height),
        },
        size,
    );
    frame.ui_renderer.set_external(
        id,
        texture.create_view(&Default::default()),
        (picture.width, picture.height),
    );
}

/// A save: (map, file name), as Load Bricks names it.
pub type Key = (String, String);

/// The picture Load Bricks asked for, read on a worker so a large file never
/// stalls a frame. A newer pick replaces the one being read.
#[derive(Default)]
pub struct Previews {
    reading: Option<(Key, mpsc::Receiver<Result<Option<Picture>>>)>,
    /// Read and waiting for the next frame to upload it.
    pub ready: Option<(Key, Picture)>,
}
impl Previews {
    pub fn start(&mut self, key: Key, path: PathBuf, runtime: &tokio::runtime::Runtime) {
        let (tx, rx) = mpsc::channel();
        runtime.spawn_blocking(move || {
            let _ = tx.send(read(&path));
        });
        self.reading = Some((key, rx));
    }
    /// The save whose reading finished without a picture to show.
    pub fn poll(&mut self) -> Option<Key> {
        let (key, rx) = self.reading.as_ref()?;
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(anyhow::anyhow!("reader stopped")),
        };
        let key = key.clone();
        self.reading = None;
        match result {
            Ok(Some(picture)) => {
                self.ready = Some((key, picture));
                None
            }
            Ok(None) => Some(key),
            Err(error) => {
                bri_console::warn(format!("Save picture for {} unreadable: {error:#}", key.1));
                Some(key)
            }
        }
    }
    /// Forget a pick nobody waits for any more.
    pub fn cancel(&mut self) {
        self.reading = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_shares_its_saves_name() {
        let dir = Path::new("saves/Slate");
        assert_eq!(
            path_for(&dir.join("Afghanistan DM .bls")),
            Some(dir.join("Afghanistan DM .jpg")),
            "v20 kept a trailing space"
        );
        assert_eq!(
            path_for(&dir.join("House.BLS")),
            Some(dir.join("House.jpg"))
        );
        assert_eq!(
            path_for(&dir.join("House.world.json")),
            Some(dir.join("House.jpg"))
        );
        assert_eq!(path_for(&dir.join("notes.txt")), None);
    }

    #[test]
    fn pictures_scale_down_to_fit_and_missing_ones_are_none() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("bri-save-picture-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let big = dir.join("big.jpg");
        image::RgbImage::from_pixel(1680, 1050, image::Rgb([200, 40, 10])).save(&big)?;
        let picture = read(&big)?.context("a picture")?;
        assert_eq!((picture.width, picture.height), (588, 368));
        assert_eq!(picture.rgba.len(), 588 * 368 * 4);
        let small = dir.join("small.jpg");
        image::RgbImage::from_pixel(294, 220, image::Rgb([0, 90, 200])).save(&small)?;
        let picture = read(&small)?.context("a picture")?;
        assert_eq!(
            (picture.width, picture.height),
            (294, 220),
            "never scaled up"
        );
        assert!(read(&dir.join("none.jpg"))?.is_none());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
