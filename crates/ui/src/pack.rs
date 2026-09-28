//! Loading a converted UI pack and decoding its images lazily.

use crate::schema::{FontEntry, PACK_SCHEMA_VERSION, UiPack};
use anyhow::{Context, Result, ensure};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Decoded RGBA8 pixels (non-premultiplied, display space).
#[derive(Debug, Clone)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Something the renderer can sample: an image or a font sheet.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TexKey {
    Image(String),
    FontSheet(String, u16),
    /// A texture supplied by the host at runtime (brick icon render targets,
    /// the avatar preview, map previews from a catalog). The UI never loads
    /// these itself; the renderer looks them up in its external table.
    External(u64),
}

pub struct Pack {
    pub data: UiPack,
    pub dir: PathBuf,
    cache: RefCell<HashMap<TexKey, Option<Rc<Pixels>>>>,
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
        // Native art composed from original images (crate::composed): listed
        // like any image, built on first use.
        use crate::composed::{ADD_ONS_BUTTON, SOURCES, STATES};
        for state in STATES {
            let sources: Vec<_> = SOURCES
                .iter()
                .filter_map(|s| data.images.get(&format!("{s}{state}")))
                .collect();
            if sources.len() == SOURCES.len() {
                let entry = crate::schema::ImageEntry {
                    file: String::new(),
                    width: sources[0].width,
                    height: sources[0].height,
                    sha256: String::new(),
                    source: format!("composed from {}", SOURCES.join(", ")),
                };
                data.images.insert(format!("{ADD_ONS_BUTTON}{state}"), entry);
            }
        }
        Pack {
            data,
            dir,
            cache: RefCell::new(HashMap::new()),
        }
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
        if let Some(p) = self.cache.borrow().get(key) {
            return p.clone();
        }
        let decoded = self.decode(key).ok();
        let decoded = decoded.map(Rc::new);
        self.cache.borrow_mut().insert(key.clone(), decoded.clone());
        decoded
    }

    fn decode(&self, key: &TexKey) -> Result<Pixels> {
        match key {
            TexKey::Image(id) => {
                if let Some(state) = id.strip_prefix(crate::composed::ADD_ONS_BUTTON) {
                    let [about, credits, options] = crate::composed::SOURCES.map(|s| {
                        self.pixels(&TexKey::Image(format!("{s}{state}")))
                    });
                    return crate::composed::add_ons_button(
                        &*about.context("About button art")?,
                        &*credits.context("Credits button art")?,
                        &*options.context("Options button art")?,
                    );
                }
                let e = self.data.images.get(id).context("unknown image")?;
                let img = image::open(self.dir.join(&e.file))?.to_rgba8();
                Ok(Pixels {
                    width: img.width(),
                    height: img.height(),
                    rgba: img.into_raw(),
                })
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
            TexKey::External(_) => anyhow::bail!("external textures are supplied by the host"),
        }
    }
}
