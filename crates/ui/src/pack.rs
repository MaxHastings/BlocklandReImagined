//! Loading a converted UI pack and decoding its images lazily.

use crate::fallback::{GlyphSource, Raster, SystemFonts};
use crate::schema::{FontEntry, Glyph, PACK_SCHEMA_VERSION, UiPack};
use anyhow::{Context, Result, ensure};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Decoded RGBA8 pixels (non-premultiplied, display space).
#[derive(Debug)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn decode_image(file: &Path) -> Result<Pixels> {
    let img = image::open(file)?.to_rgba8();
    Ok(Pixels {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
    })
}

/// Something the renderer can sample: an image or a font sheet.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TexKey {
    Image(String),
    FontSheet(String, u16),
    /// A glyph the font caches lack, drawn from a system font
    /// ([`crate::fallback`]): the cache's baseline and the character.
    Fallback(u32, char),
    /// A texture supplied by the host at runtime (brick icon render targets,
    /// the avatar preview, map previews from a catalog). The UI never loads
    /// these itself; the renderer looks them up in its external table.
    External(u64),
}

/// An image id and its pixels, or None when it failed to decode.
type Prefetched = (String, Option<Pixels>);

/// Torque's `GuiControlProfile` stores `fontColor`, `fontColorHL`,
/// `fontColorNA` and `fontColorSEL` as references to `fontColors[0..3]`, so
/// the later assignment wins. The pack keeps both fields; in every v20
/// profile that sets both, `fontColors[n]` comes later (checked against all
/// 222 aliased slots in allClientScripts). So `BlockChatTextProfile`'s base
/// colour is `fontColors[0]` = 255 0 64, which is also what `\c0` restores:
/// center/bottom prints, chat and Tutorial prompts use it for uncoloured
/// text.
pub fn alias_font_colors(style: &mut crate::schema::Style) {
    if style.font_colors.len() < 4 {
        style.font_colors.resize(4, None);
    }
    let fields = [
        &mut style.font_color,
        &mut style.font_color_hl,
        &mut style.font_color_na,
        &mut style.font_color_sel,
    ];
    for (slot, field) in style.font_colors.iter_mut().zip(fields) {
        match slot {
            Some(c) => *field = Some(*c),
            None => *slot = *field,
        }
    }
}

/// Rasterised fallback glyphs by (baseline, character).
type FallbackCache = HashMap<(u32, char), Option<Rc<Raster>>>;

pub struct Pack {
    pub data: UiPack,
    pub dir: PathBuf,
    cache: RefCell<HashMap<TexKey, Option<Rc<Pixels>>>>,
    /// Images a worker thread is decoding ahead of first use (`prefetch`).
    prefetched: RefCell<Option<std::sync::mpsc::Receiver<Prefetched>>>,
    glyph_source: RefCell<Box<dyn GlyphSource>>,
    /// Rasterised fallback glyphs by (baseline, character); `None` when no
    /// font has the character.
    fallback: RefCell<FallbackCache>,
}

impl Pack {
    pub fn load(dir: &Path) -> Result<Self> {
        let json = std::fs::read(dir.join("ui-pack.json"))
            .with_context(|| format!("reading {}/ui-pack.json", dir.display()))?;
        let data: UiPack = serde_json::from_slice(&json)?;
        ensure!(
            data.schema_version == PACK_SCHEMA_VERSION,
            "UI pack schema {} unsupported (expected {PACK_SCHEMA_VERSION})",
            data.schema_version
        );
        Ok(Self::from_parts(data, dir.to_path_buf()))
    }

    /// Build from in-memory data (tests use synthetic packs).
    pub fn from_parts(mut data: UiPack, dir: PathBuf) -> Self {
        for style in data.styles.values_mut() {
            alias_font_colors(style);
        }
        Pack {
            data,
            dir,
            cache: RefCell::new(HashMap::new()),
            prefetched: RefCell::new(None),
            glyph_source: RefCell::new(Box::new(SystemFonts)),
            fallback: RefCell::new(HashMap::new()),
        }
    }

    /// Draw missing glyphs from `source` instead of the system's fonts.
    pub fn set_glyph_source(&self, source: Box<dyn GlyphSource>) {
        *self.glyph_source.borrow_mut() = source;
        self.fallback.borrow_mut().clear();
        self.cache
            .borrow_mut()
            .retain(|k, _| !matches!(k, TexKey::Fallback(..)));
    }

    /// `c` from a system font for a font cache with this baseline, placed
    /// like a cache glyph (its texture is `TexKey::Fallback(baseline, c)`,
    /// sampled whole), and whether it has its own colours.
    pub fn fallback_glyph(&self, baseline: u32, c: char) -> Option<(Glyph, bool)> {
        let raster = self.fallback_raster(baseline, c)?;
        Some((
            Glyph {
                sheet: 0,
                x: 0,
                y: 0,
                w: raster.width as u16,
                h: raster.height as u16,
                x_origin: raster.x_origin,
                y_origin: raster.y_origin,
                advance: raster.advance,
            },
            raster.color,
        ))
    }

    fn fallback_raster(&self, baseline: u32, c: char) -> Option<Rc<Raster>> {
        if let Some(r) = self.fallback.borrow().get(&(baseline, c)) {
            return r.clone();
        }
        let raster = self
            .glyph_source
            .borrow_mut()
            .raster(c, baseline)
            .filter(|r| {
                r.width <= u16::MAX as u32
                    && r.height <= u16::MAX as u32
                    && r.rgba.len() == (r.width * r.height * 4) as usize
            })
            .map(Rc::new);
        self.fallback
            .borrow_mut()
            .insert((baseline, c), raster.clone());
        raster
    }

    pub fn has_image(&self, id: &str) -> bool {
        self.data.images.contains_key(id)
    }

    pub fn image_size(&self, id: &str) -> Option<(u32, u32)> {
        self.data.images.get(id).map(|e| (e.width, e.height))
    }

    pub fn font(&self, id: &str) -> Option<&FontEntry> {
        self.data.fonts.get(id)
    }

    /// Decode (and cache) a texture. Font sheets become white RGB with the
    /// coverage in alpha, so tinting colours the glyphs.
    pub fn pixels(&self, key: &TexKey) -> Option<Rc<Pixels>> {
        self.take_prefetched();
        if let Some(p) = self.cache.borrow().get(key) {
            return p.clone();
        }
        let decoded = self.decode(key).ok();
        let decoded = decoded.map(Rc::new);
        self.cache.borrow_mut().insert(key.clone(), decoded.clone());
        decoded
    }

    /// Decode these images on a worker thread so their first draw (a brick
    /// selector page is hundreds of icons) does not decode them all on the
    /// frame. Anything drawn before its turn is decoded then as usual.
    pub fn prefetch(&self, ids: impl IntoIterator<Item = String>) {
        let files: Vec<_> = ids
            .into_iter()
            .filter(|id| !self.cache.borrow().contains_key(&TexKey::Image(id.clone())))
            .filter_map(|id| {
                let file = self.dir.join(&self.data.images.get(&id)?.file);
                Some((id, file))
            })
            .collect();
        if files.is_empty() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("ui-prefetch".into())
            .spawn(move || {
                for (id, file) in files {
                    if tx.send((id, decode_image(&file).ok())).is_err() {
                        return;
                    }
                }
            });
        if spawned.is_ok() {
            *self.prefetched.borrow_mut() = Some(rx);
        }
    }

    /// Move whatever the prefetch worker has decoded into the cache.
    fn take_prefetched(&self) {
        let mut prefetched = self.prefetched.borrow_mut();
        let Some(rx) = prefetched.as_ref() else {
            return;
        };
        let mut cache = self.cache.borrow_mut();
        loop {
            match rx.try_recv() {
                Ok((id, pixels)) => {
                    cache
                        .entry(TexKey::Image(id))
                        .or_insert_with(|| pixels.map(Rc::new));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    *prefetched = None;
                    return;
                }
            }
        }
    }

    fn decode(&self, key: &TexKey) -> Result<Pixels> {
        match key {
            TexKey::Image(id) => {
                let e = self.data.images.get(id).context("unknown image")?;
                decode_image(&self.dir.join(&e.file))
            }
            TexKey::FontSheet(font, sheet) => {
                let f = self.data.fonts.get(font).context("unknown font")?;
                let file = f.sheets.get(*sheet as usize).context("unknown sheet")?;
                let img = image::open(self.dir.join(file))?.to_luma8();
                let mut rgba = Vec::with_capacity(img.len() * 4);
                for &l in img.as_raw() {
                    rgba.extend_from_slice(&[255, 255, 255, l]);
                }
                Ok(Pixels {
                    width: img.width(),
                    height: img.height(),
                    rgba,
                })
            }
            TexKey::Fallback(baseline, c) => {
                let r = self.fallback_raster(*baseline, *c).context("no font has it")?;
                Ok(Pixels {
                    width: r.width,
                    height: r.height,
                    rgba: r.rgba.clone(),
                })
            }
            TexKey::External(_) => anyhow::bail!("external textures are supplied by the host"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ImageEntry;

    #[test]
    fn prefetched_icons_arrive_decoded_and_missing_ones_still_decode_on_use() {
        let dir = std::env::temp_dir().join(format!("bri-ui-prefetch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut data = UiPack::default();
        for (id, shade) in [("a", 10u8), ("b", 200)] {
            image::RgbaImage::from_pixel(2, 3, image::Rgba([shade, 0, 0, 255]))
                .save(dir.join(format!("{id}.png")))
                .unwrap();
            data.images.insert(
                id.into(),
                ImageEntry {
                    file: format!("{id}.png"),
                    width: 2,
                    height: 3,
                    sha256: String::new(),
                    source: String::new(),
                },
            );
        }
        let pack = Pack::from_parts(data, dir.clone());
        pack.prefetch(["a".to_string(), "nowhere".to_string()]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while pack.prefetched.borrow().is_some() && std::time::Instant::now() < deadline {
            pack.take_prefetched();
            std::thread::yield_now();
        }
        assert!(pack.cache.borrow().contains_key(&TexKey::Image("a".into())));
        let a = pack.pixels(&TexKey::Image("a".into())).unwrap();
        assert_eq!((a.width, a.height, a.rgba[0]), (2, 3, 10));
        // Not prefetched: decoded on first use, as before.
        let b = pack.pixels(&TexKey::Image("b".into())).unwrap();
        assert_eq!(b.rgba[0], 200);
        std::fs::remove_dir_all(dir).unwrap();
    }

    use crate::schema::Style;

    #[test]
    fn named_font_colours_alias_the_palette() {
        // BlockChatTextProfile: fontColor "0 0 0" then fontColors[0] "255 0 64".
        let mut s = Style {
            font_color: Some([0, 0, 0, 255]),
            font_color_hl: Some([130, 130, 130, 255]),
            font_color_na: Some([255, 0, 0, 255]),
            font_colors: vec![
                Some([255, 0, 64, 255]),
                Some([64, 64, 255, 255]),
                None,
                None,
            ],
            ..Default::default()
        };
        super::alias_font_colors(&mut s);
        assert_eq!(s.font_color, Some([255, 0, 64, 255]));
        assert_eq!(s.font_color_hl, Some([64, 64, 255, 255]));
        assert_eq!(s.font_color_na, Some([255, 0, 0, 255]));
        assert_eq!(s.font_colors[2], Some([255, 0, 0, 255]));
        assert_eq!(s.font_colors[3], None);
    }
}
