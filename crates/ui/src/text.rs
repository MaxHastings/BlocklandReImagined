//! Text with the original Torque font caches (bitmap fonts at fixed pixel
//! sizes, Windows-1252 code page) and the subset of Torque ML markup that v20
//! screens use.
//!
//! Colour codes (`\c0`..`\c9`) are stored as U+E000..U+E009. `\cr`, `\cp` and
//! `\co` (reset/push/pop) are U+E00A..U+E00C.

use crate::draw::{DrawList, Filter};
use crate::geom::Rgba;
use crate::pack::{Pack, TexKey};
use crate::schema::{FontEntry, Glyph, Justify, Style};

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

pub struct Font<'a> {
    pub id: &'a str,
    pub entry: &'a FontEntry,
}

impl<'a> Font<'a> {
    pub fn get(pack: &'a Pack, id: &'a str) -> Option<Font<'a>> {
        pack.font(id).map(|entry| Font { id, entry })
    }
    pub fn line_height(&self) -> i32 {
        self.entry.line_height as i32
    }
    pub fn glyph(&self, c: char) -> Option<Glyph> {
        let b = to_cp1252(c).unwrap_or(b'?');
        self.entry.glyphs.get(b as usize).copied().flatten()
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
            let Some(g) = self.glyph(c) else { continue };
            if g.w > 0 && g.h > 0 {
                dl.image(
                    TexKey::FontSheet(self.id.to_string(), g.sheet),
                    [g.x as f32, g.y as f32, g.w as f32, g.h as f32],
                    [
                        pen + g.x_origin as f32,
                        top + self.entry.baseline as f32 - g.y_origin as f32,
                        g.w as f32,
                        g.h as f32,
                    ],
                    cur,
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
                self.draw(dl, x + dx, top + dy, &plain, o, &[]);
            }
        }
        self.draw(dl, x, top, s, color, palette)
    }
}

/// One laid-out line of rich text.
/// Private-use delimiters around an inline `<bitmap:...>` image id.
pub const BITMAP_START: char = '\u{F000}';
pub const BITMAP_END: char = '\u{F001}';
/// Split a laid-out ML line into text runs and inline bitmap ids.
pub fn ml_runs(line: &str) -> Vec<(bool, &str)> {
    let mut runs = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find(BITMAP_START) {
        if start > 0 {
            runs.push((false, &rest[..start]));
        }
        let after = &rest[start + BITMAP_START.len_utf8()..];
        let Some(end) = after.find(BITMAP_END) else {
            rest = after;
            break;
        };
        runs.push((true, &after[..end]));
        rest = &after[end + BITMAP_END.len_utf8()..];
    }
    if !rest.is_empty() {
        runs.push((false, rest));
    }
    runs
}
#[derive(Debug, Clone, PartialEq)]
pub struct MlLine {
    pub text: String,
    pub justify: Justify,
    pub width: i32,
}

/// Parse the Torque ML subset used by v20 (`<just:...>`, `<br>`, colour codes,
/// `<a:url>..</a>`, `<spush>/<spop>`, `<font:..>` ignored) and word-wrap to
/// `max_width` (0 = no wrapping).
pub fn layout_ml(font: &Font, src: &str, max_width: i32, default: Justify) -> Vec<MlLine> {
    let mut lines = Vec::new();
    let mut just = default;
    let mut cur = String::new();
    let mut rest = src;
    let mut paragraphs: Vec<(String, Justify)> = Vec::new();
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix('<')
            && let Some(end) = stripped.find('>')
        {
            let tag = &stripped[..end];
            let low = tag.to_ascii_lowercase();
            let known = low.starts_with("just:")
                || low == "br"
                || low.starts_with("a:")
                || low == "/a"
                || low == "spush"
                || low == "spop"
                || low.starts_with("font:")
                || low.starts_with("color:")
                || low.starts_with("bitmap:")
                || low == "linkcolor"
                || low.starts_with("tab:");
            if known {
                // Inline bitmaps (death icons) survive layout as a marked run
                // that the ML renderer draws as an image.
                if low.starts_with("bitmap:") {
                    cur.push(BITMAP_START);
                    cur.push_str(&tag["bitmap:".len()..]);
                    cur.push(BITMAP_END);
                }
                if let Some(j) = low.strip_prefix("just:") {
                    just = match j {
                        "center" => Justify::Center,
                        "right" => Justify::Right,
                        _ => Justify::Left,
                    };
                }
                if low == "br" {
                    paragraphs.push((std::mem::take(&mut cur), just));
                }
                rest = &stripped[end + 1..];
                continue;
            }
        }
        let c = rest.chars().next().expect("non-empty");
        rest = &rest[c.len_utf8()..];
        if c == '\n' {
            paragraphs.push((std::mem::take(&mut cur), just));
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() || paragraphs.is_empty() {
        paragraphs.push((cur, just));
    }
    for (p, j) in paragraphs {
        if max_width <= 0 || font.width(&p) <= max_width {
            lines.push(MlLine {
                width: font.width(&p),
                text: p,
                justify: j,
            });
            continue;
        }
        // Greedy word wrap, keeping colour markers attached to words.
        let mut line = String::new();
        for word in p.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if font.width(&candidate) > max_width && !line.is_empty() {
                lines.push(MlLine {
                    width: font.width(&line),
                    text: std::mem::take(&mut line),
                    justify: j,
                });
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        lines.push(MlLine {
            width: font.width(&line),
            text: line,
            justify: j,
        });
    }
    lines
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
