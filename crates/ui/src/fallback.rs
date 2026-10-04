//! Glyphs the v20 font caches lack.
//!
//! The caches hold Windows-1252 (codes 32..255). Torque's `GFont` filled a
//! missing glyph by rasterising it from the system's font of that face; this
//! does the same for any other character (other scripts, symbols, emoji in
//! player names and chat): the first system font that has the character,
//! the cache's own face (Arial) first, scaled so its ascent matches the
//! cache's baseline. Outline glyphs are coverage, tinted like cache glyphs;
//! colour bitmap glyphs (emoji fonts without outlines) keep their colours.
//!
//! Fonts are memory-mapped once per process and only opened when a
//! character the caches lack is first drawn.
use ab_glyph::{Font as _, FontRef, GlyphImageFormat, PxScale, ScaleFont as _, point};
use bri_console::Clamp;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// One rasterised glyph, placed like a cache glyph: drawn at
/// (pen + `x_origin`, top + baseline - `y_origin`).
#[derive(Debug, Clone)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    /// RGBA: white with coverage in alpha, or the glyph's own colours when
    /// `color`.
    pub rgba: Vec<u8>,
    pub color: bool,
    pub x_origin: i16,
    pub y_origin: i16,
    pub advance: i16,
}

/// Where missing glyphs come from: the system's fonts, or a fixed source in
/// tests.
pub trait GlyphSource: Send {
    /// `c` with an ascent of `ascent` pixels above the baseline, or `None`
    /// if no font has it.
    fn raster(&mut self, c: char, ascent: u32) -> Option<Raster>;
}

/// The system's fonts, shared by every pack in the process.
pub struct SystemFonts;

impl GlyphSource for SystemFonts {
    fn raster(&mut self, c: char, ascent: u32) -> Option<Raster> {
        static FONTS: OnceLock<Mutex<Faces>> = OnceLock::new();
        let faces = FONTS.get_or_init(|| Mutex::new(Faces::new(candidates())));
        let mut faces = faces.lock().unwrap_or_else(|e| e.into_inner());
        let face = faces.covering(c)?;
        rasterise(face, c, ascent)
    }
}

/// Font files, opened in order as characters need them.
struct Faces {
    files: Vec<PathBuf>,
    opened: usize,
    faces: Vec<FontRef<'static>>,
}

/// Font files above this size are skipped (whole-script collections).
const MAX_FONT_FILE: u64 = 256 << 20;
/// Font files looked at, at most, across all font folders.
const MAX_FONT_FILES: usize = 2048;

impl Faces {
    fn new(files: Vec<PathBuf>) -> Self {
        Self {
            files,
            opened: 0,
            faces: Vec::new(),
        }
    }
    /// The first font (in candidate order) that has `c`. Fonts are opened
    /// until one has it, and stay open: mapping costs address space, not
    /// memory, and the next character usually needs the same font.
    fn covering(&mut self, c: char) -> Option<&FontRef<'static>> {
        if let Some(i) = self.faces.iter().position(|f| f.glyph_id(c).0 != 0) {
            return self.faces.get(i);
        }
        while self.opened < self.files.len() {
            let path = self.files[self.opened].clone();
            self.opened += 1;
            if let Some(face) = open(&path) {
                let has = face.glyph_id(c).0 != 0;
                self.faces.push(face);
                if has {
                    return self.faces.last();
                }
            }
        }
        None
    }
}

/// The first face of a font file, mapped for the rest of the process.
fn open(path: &Path) -> Option<FontRef<'static>> {
    let file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len == 0 || len > MAX_FONT_FILE {
        return None;
    }
    // SAFETY: font files are read-only system files; the mapping is kept
    // (leaked) for the process's lifetime, so the slice never dangles.
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    let data: &'static [u8] = Box::leak(Box::new(map));
    FontRef::try_from_slice_and_index(data, 0).ok()
}

/// Font files in the order they are tried: the v20 caches' own face and
/// broad-coverage fonts of each platform first, then every font in the
/// system's font folders.
fn candidates() -> Vec<PathBuf> {
    let mut preferred: Vec<PathBuf> = Vec::new();
    let mut folders: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        let windir =
            std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        let fonts = windir.join("Fonts");
        for name in [
            "arial.ttf",
            "segoeui.ttf",
            "seguisym.ttf",
            "msyh.ttc",
            "YuGothM.ttc",
            "msgothic.ttc",
            "malgun.ttf",
            "seguiemj.ttf",
        ] {
            preferred.push(fonts.join(name));
        }
        folders.push(fonts);
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            folders.push(PathBuf::from(local).join(r"Microsoft\Windows\Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        for name in [
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/Library/Fonts/Arial Unicode.ttf",
            "/System/Library/Fonts/Apple Symbols.ttf",
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/Apple Color Emoji.ttc",
        ] {
            preferred.push(name.into());
        }
        folders.push("/System/Library/Fonts".into());
        folders.push("/Library/Fonts".into());
        if let Some(home) = std::env::var_os("HOME") {
            folders.push(PathBuf::from(home).join("Library/Fonts"));
        }
    } else {
        folders.push("/usr/share/fonts".into());
        folders.push("/usr/local/share/fonts".into());
        if let Some(home) = std::env::var_os("HOME") {
            folders.push(PathBuf::from(&home).join(".local/share/fonts"));
            folders.push(PathBuf::from(home).join(".fonts"));
        }
    }
    let mut scanned = Vec::new();
    for folder in &folders {
        scan(folder, 0, &mut scanned);
    }
    scanned.sort();
    // Linux has no fixed file names: prefer the usual broad sans faces.
    let rank = |p: &PathBuf| {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        [
            "arial",
            "liberationsans-regular",
            "dejavusans.",
            "notosans-regular",
            "notosanscjk",
            "wqy",
            "notocoloremoji",
        ]
        .iter()
        .position(|k| name.starts_with(k))
        .unwrap_or(usize::MAX)
    };
    scanned.sort_by_key(rank);
    let mut all: Vec<PathBuf> = preferred.into_iter().filter(|p| p.is_file()).collect();
    for p in scanned {
        if !all.contains(&p) {
            all.push(p);
        }
    }
    all.truncate(MAX_FONT_FILES);
    all
}

fn scan(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_FONT_FILES {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            if depth < 4 {
                scan(&path, depth + 1, out);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"))
        {
            out.push(path);
        }
    }
}

/// `c` from `face` with an ascent of `ascent` pixels.
fn rasterise(face: &FontRef<'_>, c: char, ascent: u32) -> Option<Raster> {
    let id = face.glyph_id(c);
    let (face_ascent, face_height) = (face.ascent_unscaled(), face.height_unscaled());
    if id.0 == 0 || face_ascent <= 0.0 || face_height <= 0.0 || ascent == 0 {
        return None;
    }
    let scale = PxScale::from(ascent as f32 * face_height / face_ascent);
    let advance = face
        .as_scaled(scale)
        .h_advance(id)
        .round()
        .clamped(0.0, 512.0) as i16;
    if let Some(outline) = face.outline_glyph(id.with_scale_and_position(scale, point(0.0, 0.0))) {
        let bounds = outline.px_bounds();
        let (width, height) = (bounds.width() as u32, bounds.height() as u32);
        if width == 0 || height == 0 || width > 512 || height > 512 {
            return None;
        }
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        outline.draw(|x, y, coverage| {
            let i = ((y * width + x) * 4) as usize;
            if let Some(px) = rgba.get_mut(i..i + 4) {
                px.copy_from_slice(&[
                    255,
                    255,
                    255,
                    (coverage.clamped(0.0, 1.0) * 255.0).round() as u8,
                ]);
            }
        });
        return Some(Raster {
            width,
            height,
            rgba,
            color: false,
            x_origin: bounds.min.x as i16,
            y_origin: (-bounds.min.y) as i16,
            advance,
        });
    }
    // Colour emoji fonts without outlines (Apple, Noto): a PNG per strike.
    let image = face.glyph_raster_image2(id, u16::MAX)?;
    if !matches!(image.format, GlyphImageFormat::Png) {
        return None;
    }
    let decoded = image::load_from_memory_with_format(image.data, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    if decoded.width() == 0 || decoded.height() == 0 {
        return None;
    }
    // As tall as the text's capitals and a little below the baseline.
    let height = (ascent as f32 * 1.15).round().max(1.0) as u32;
    let width = ((decoded.width() * height) as f32 / decoded.height() as f32)
        .round()
        .max(1.0) as u32;
    let resized = image::imageops::resize(
        &decoded,
        width,
        height,
        image::imageops::FilterType::Triangle,
    );
    Some(Raster {
        width,
        height,
        rgba: resized.into_raw(),
        color: true,
        x_origin: 0,
        y_origin: ascent as i16,
        advance: advance.max(width as i16 + 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This machine's fonts, when it has any: every covered character
    /// rasterises into a glyph with ink, placed about the baseline.
    #[test]
    fn system_fonts_draw_what_they_cover() {
        let mut faces = Faces::new(candidates());
        for c in ['Ж', 'Ω', '★', '中', 'あ', '한'] {
            let Some(face) = faces.covering(c) else {
                continue;
            };
            let r = rasterise(face, c, 11).expect("covered glyph rasterises");
            assert!(r.width > 0 && r.height > 0 && r.advance > 0, "{c}");
            assert!(r.y_origin > 0 && r.y_origin <= 20, "{c}: {}", r.y_origin);
            assert!(r.rgba.chunks(4).any(|p| p[3] > 0), "{c} has ink");
        }
    }
}
