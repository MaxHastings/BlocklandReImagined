//! Text with the original Torque font caches (bitmap fonts at fixed pixel
//! sizes, Windows-1252 code page). ML markup is in [`crate::ml`].
//!
//! Colour codes (`\c0`..`\c9`) are stored as U+E000..U+E009. `\cr`, `\cp` and
//! `\co` (reset/push/pop) are U+E00A..U+E00C.

use crate::draw::{DrawList, Filter};
use crate::geom::Rgba;
use crate::pack::{Pack, TexKey};
use crate::schema::{FontEntry, Glyph, Style};

pub const COLOR_CODE_BASE: u32 = 0xE000;

/// Map a Unicode scalar to a Windows-1252 byte (`None` if unrepresentable).
pub fn to_cp1252(c: char) -> Option<u8> {
    let u = c as u32;
    if u < 0x80 || (0xA0..=0xFF).contains(&u) {
        return Some(u as u8);
    }
    const HIGH: [(char, u8); 27] = [
        ('€', 0x80),
        ('‚', 0x82),
        ('ƒ', 0x83),
        ('„', 0x84),
        ('…', 0x85),
        ('†', 0x86),
        ('‡', 0x87),
        ('ˆ', 0x88),
        ('‰', 0x89),
        ('Š', 0x8A),
        ('‹', 0x8B),
        ('Œ', 0x8C),
        ('Ž', 0x8E),
        ('‘', 0x91),
        ('’', 0x92),
        ('“', 0x93),
        ('”', 0x94),
        ('•', 0x95),
        ('–', 0x96),
        ('—', 0x97),
        ('˜', 0x98),
        ('™', 0x99),
        ('š', 0x9A),
        ('›', 0x9B),
        ('œ', 0x9C),
        ('ž', 0x9E),
        ('Ÿ', 0x9F),
    ];
    HIGH.iter().find(|(ch, _)| *ch == c).map(|(_, b)| *b)
}

/// Colour-code index if `c` is a `\cN` marker.
pub fn color_code(c: char) -> Option<usize> {
    let u = c as u32;
    (COLOR_CODE_BASE..COLOR_CODE_BASE + 10)
        .contains(&u)
        .then(|| (u - COLOR_CODE_BASE) as usize)
}

/// Turn TorqueScript-style escapes typed by code (`\c3`) into markers.
pub fn markup_colors(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' && it.peek() == Some(&'c') {
            it.next();
            if let Some(d) = it.peek().copied().filter(char::is_ascii_digit) {
                it.next();
                out.push(char::from_u32(COLOR_CODE_BASE + d as u32 - '0' as u32).expect("valid"));
                continue;
            }
            out.push_str("\\c");
            continue;
        }
        out.push(c);
    }
    out
}

#[derive(Clone, Copy)]
pub struct Font<'a> {
    pub id: &'a str,
    pub entry: &'a FontEntry,
    pub pack: &'a Pack,
}

/// Where a glyph's pixels are: a cache sheet, or a fallback glyph (with its
/// own colours when `color`).
enum Placed {
    Cache(Glyph),
    Fallback { glyph: Glyph, color: bool },
}

impl<'a> Font<'a> {
    pub fn get(pack: &'a Pack, id: &'a str) -> Option<Font<'a>> {
        pack.font(id).map(|entry| Font { id, entry, pack })
    }
    pub fn line_height(&self) -> i32 {
        self.entry.line_height as i32
    }
    pub fn glyph(&self, c: char) -> Option<Glyph> {
        match self.placed(c)? {
            Placed::Cache(g) | Placed::Fallback { glyph: g, .. } => Some(g),
        }
    }
    /// The cache's glyph for a Windows-1252 character; anything else from
    /// a system font ([`crate::fallback`]), or `?` when no font has it.
    fn placed(&self, c: char) -> Option<Placed> {
        let cached = |b: u8| {
            self.entry
                .glyphs
                .get(b as usize)
                .copied()
                .flatten()
                .map(Placed::Cache)
        };
        if let Some(b) = to_cp1252(c) {
            return cached(b);
        }
        // Control characters and private use (our colour markers) are in no
        // font worth looking through.
        if c.is_control() || ('\u{E000}'..='\u{F8FF}').contains(&c) {
            return cached(b'?');
        }
        match self.pack.fallback_glyph(self.entry.baseline, c) {
            Some((glyph, color)) => Some(Placed::Fallback { glyph, color }),
            None => cached(b'?'),
        }
    }
    /// Advance width of plain text (markers are zero-width).
    pub fn width(&self, s: &str) -> i32 {
        s.chars()
            .filter(|c| !(0xE000..0xE010).contains(&(*c as u32)))
            .map(|c| self.glyph(c).map_or(0, |g| g.advance as i32))
            .sum()
    }
    /// Draw a single line at (x, top). Returns the pen x after drawing.
    /// `palette` resolves colour codes; `color` is the starting colour.
    pub fn draw(
        &self,
        dl: &mut DrawList,
        x: f32,
        top: f32,
        s: &str,
        color: Rgba,
        palette: &[Option<Rgba>],
    ) -> f32 {
        self.draw_pass(dl, x, top, s, color, palette, false)
    }
    /// `draw`, or with `outline` an outline pass: colour glyphs (emoji) get
    /// no outline, their own edges show.
    #[allow(clippy::too_many_arguments)]
    fn draw_pass(
        &self,
        dl: &mut DrawList,
        x: f32,
        top: f32,
        s: &str,
        color: Rgba,
        palette: &[Option<Rgba>],
        outline: bool,
    ) -> f32 {
        let mut pen = x;
        let mut cur = color;
        for c in s.chars() {
            if let Some(i) = color_code(c) {
                cur = palette.get(i).copied().flatten().unwrap_or(color);
                continue;
            }
            if c as u32 == COLOR_CODE_BASE + 10 {
                cur = color;
                continue;
            }
            if (0xE000..0xE010).contains(&(c as u32)) {
                continue;
            }
            let (tex, g, color) = match self.placed(c) {
                Some(Placed::Cache(g)) => {
                    (TexKey::FontSheet(self.id.to_string(), g.sheet), g, false)
                }
                Some(Placed::Fallback { glyph, color }) => {
                    (TexKey::Fallback(self.entry.baseline, c), glyph, color)
                }
                None => continue,
            };
            if g.w > 0 && g.h > 0 && !(color && outline) {
                // Colour emoji keep their colours; only the alpha applies.
                let tint = if color { [255, 255, 255, cur[3]] } else { cur };
                dl.image(
                    tex,
                    [g.x as f32, g.y as f32, g.w as f32, g.h as f32],
                    [
                        pen + g.x_origin as f32,
                        top + self.entry.baseline as f32 - g.y_origin as f32,
                        g.w as f32,
                        g.h as f32,
                    ],
                    tint,
                    Filter::Nearest,
                );
            }
            pen += g.advance as f32;
        }
        pen
    }
    /// Draw with a 1-pixel outline (profile `doFontOutline`), 4-neighbour.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_outlined(
        &self,
        dl: &mut DrawList,
        x: f32,
        top: f32,
        s: &str,
        color: Rgba,
        outline: Option<Rgba>,
        palette: &[Option<Rgba>],
    ) -> f32 {
        if let Some(o) = outline {
            let plain: String = s.chars().filter(|c| color_code(*c).is_none()).collect();
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                self.draw_pass(dl, x + dx, top + dy, &plain, o, &[], true);
            }
        }
        self.draw(dl, x, top, s, color, palette)
    }
}

/// Text colour for a control state.
pub fn style_color(style: &Style, highlighted: bool, inactive: bool) -> Rgba {
    let base = style.font_color.unwrap_or([0, 0, 0, 255]);
    if inactive {
        style.font_color_na.unwrap_or(base)
    } else if highlighted {
        style.font_color_hl.unwrap_or(base)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the system's fonts: `Ж` in outline, `😀` in colour,
    /// nothing else.
    struct TwoGlyphs;
    impl crate::fallback::GlyphSource for TwoGlyphs {
        fn raster(&mut self, c: char, ascent: u32) -> Option<crate::fallback::Raster> {
            let (color, advance) = match c {
                'Ж' => (false, 7),
                '😀' => (true, 12),
                _ => return None,
            };
            Some(crate::fallback::Raster {
                width: 4,
                height: ascent,
                rgba: vec![200; (4 * ascent * 4) as usize],
                color,
                x_origin: 0,
                y_origin: ascent as i16,
                advance,
            })
        }
    }

    fn fallback_pack() -> Pack {
        let mut data = crate::schema::UiPack::default();
        let glyph = |b: usize| Glyph {
            sheet: 0,
            x: b as u16,
            y: 0,
            w: 5,
            h: 8,
            x_origin: 0,
            y_origin: 8,
            advance: 6,
        };
        data.fonts.insert(
            "arial_14".into(),
            FontEntry {
                face: "Arial".into(),
                size: 14,
                line_height: 14,
                baseline: 11,
                sheets: vec![],
                glyphs: (0..256).map(|b| (b >= 32).then(|| glyph(b))).collect(),
                source: String::new(),
                sha256: String::new(),
            },
        );
        let pack = Pack::from_parts(data, std::path::PathBuf::new());
        pack.set_glyph_source(Box::new(TwoGlyphs));
        pack
    }

    #[test]
    fn characters_the_cache_lacks_come_from_the_fallback_fonts() {
        let pack = fallback_pack();
        let font = Font::get(&pack, "arial_14").unwrap();
        // Cache glyphs advance 6; the fallback's own advances; a character
        // no font has is the cache's `?`.
        assert_eq!(font.width("AЖ😀\u{2FFFF}"), 6 + 7 + 12 + 6);
        let mut dl = DrawList::default();
        font.draw_outlined(
            &mut dl,
            0.0,
            0.0,
            "AЖ😀\u{2FFFF}",
            [255, 0, 0, 200],
            Some([0, 0, 0, 255]),
            &[],
        );
        let drawn: Vec<(TexKey, [f32; 4], Rgba)> = dl
            .cmds
            .iter()
            .filter_map(|c| match c {
                crate::draw::DrawCmd::Image { tex, src, tint, .. } => {
                    Some((tex.clone(), *src, *tint))
                }
                _ => None,
            })
            .collect();
        let fell_back = |c: char| {
            drawn
                .iter()
                .filter(|(t, ..)| *t == TexKey::Fallback(11, c))
                .collect::<Vec<_>>()
        };
        // Outline glyphs are tinted and outlined like cache glyphs: four
        // outline passes and the text.
        let zhe = fell_back('Ж');
        assert_eq!(zhe.len(), 5);
        assert_eq!(zhe[4].1, [0.0, 0.0, 4.0, 11.0], "sampled whole");
        assert_eq!(zhe[4].2, [255, 0, 0, 200]);
        // Colour emoji keep their colours and get no outline.
        let smile = fell_back('😀');
        assert_eq!(smile.len(), 1);
        assert_eq!(smile[0].2, [255, 255, 255, 200]);
        let question = drawn
            .iter()
            .filter(|(_, src, _)| src[0] == b'?' as f32)
            .count();
        assert_eq!(question, 5);
        let pixels = pack.pixels(&TexKey::Fallback(11, 'Ж')).unwrap();
        assert_eq!(
            (pixels.width, pixels.height, pixels.rgba.len()),
            (4, 11, 4 * 11 * 4)
        );
        assert!(pack.pixels(&TexKey::Fallback(11, '\u{2FFFF}')).is_none());
    }

    #[test]
    fn cp1252_mapping() {
        assert_eq!(to_cp1252('A'), Some(65));
        assert_eq!(to_cp1252('©'), Some(0xA9));
        assert_eq!(to_cp1252('€'), Some(0x80));
        assert_eq!(to_cp1252('日'), None);
        assert_eq!(
            markup_colors("\\c3Name\\c6: hi"),
            "\u{E003}Name\u{E006}: hi"
        );
    }
}
