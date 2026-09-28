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

#[derive(Clone, Copy)]
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
    /// Width of the text runs (inline bitmaps not included).
    pub width: i32,
    /// Left margin (`<lmargin%:N>`), in pixels.
    pub indent: i32,
    /// The tallest font on the line.
    pub height: i32,
}

/// Private-use delimiters around a `<font:Face:Size>` switch (the font id,
/// `face_size` lower-cased as the pack names it) and a `<color:RRGGBB>`.
pub const FONT_START: char = '\u{F002}';
pub const FONT_END: char = '\u{F003}';
pub const RGB_START: char = '\u{F004}';
pub const RGB_END: char = '\u{F005}';

/// One piece of a laid-out ML line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MlRun<'a> {
    Text(&'a str),
    Bitmap(&'a str),
    /// Later text uses this font (a pack font id).
    Font(&'a str),
    /// Later text uses this colour.
    Rgb(Rgba),
}

/// Split a laid-out ML line into text, inline bitmaps, font and colour
/// switches.
pub fn ml_rich_runs(line: &str) -> Vec<MlRun<'_>> {
    let mut runs = Vec::new();
    let mut rest = line;
    loop {
        let next = rest
            .char_indices()
            .find(|(_, c)| matches!(*c, BITMAP_START | FONT_START | RGB_START));
        let Some((at, open)) = next else {
            if !rest.is_empty() {
                runs.push(MlRun::Text(rest));
            }
            return runs;
        };
        if at > 0 {
            runs.push(MlRun::Text(&rest[..at]));
        }
        let close = match open {
            BITMAP_START => BITMAP_END,
            FONT_START => FONT_END,
            _ => RGB_END,
        };
        let after = &rest[at + open.len_utf8()..];
        let Some(end) = after.find(close) else {
            return runs;
        };
        let body = &after[..end];
        match open {
            BITMAP_START => runs.push(MlRun::Bitmap(body)),
            FONT_START => runs.push(MlRun::Font(body)),
            _ => {
                if let Some(rgb) = parse_rgb(body) {
                    runs.push(MlRun::Rgb(rgb));
                }
            }
        }
        rest = &after[end + close.len_utf8()..];
    }
}

fn parse_rgb(hex: &str) -> Option<Rgba> {
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if hex.len() >= 8 { byte(6)? } else { 255 },
    ])
}

/// A pack font by id, borrowing the pack's own name for it.
pub fn font_named<'a>(pack: &'a Pack, id: &str) -> Option<Font<'a>> {
    let (id, entry) = pack.data.fonts.get_key_value(id)?;
    Some(Font { id, entry })
}

/// The pack font `<font:Face:Size>` names.
fn font_tag(tag: &str) -> Option<String> {
    let (face, size) = tag.rsplit_once(':')?;
    Some(format!(
        "{}_{}",
        face.trim().to_ascii_lowercase(),
        size.trim().parse::<u32>().ok()?
    ))
}

/// Width of marked-up text, the tallest font in it, and the font in use at
/// its end, starting in `font`.
fn measure<'a>(pack: &'a Pack, font: Font<'a>, s: &str) -> (i32, i32, Font<'a>) {
    let (mut width, mut height, mut font) = (0, font.line_height(), font);
    for run in ml_rich_runs(s) {
        match run {
            MlRun::Text(t) => width += font.width(t),
            MlRun::Font(id) => {
                if let Some(f) = font_named(pack, id) {
                    font = f;
                    height = height.max(f.line_height());
                }
            }
            MlRun::Bitmap(_) | MlRun::Rgb(_) => {}
        }
    }
    (width, height, font)
}

/// The font and colour markers in force at the end of `s`, to carry onto
/// the next line.
fn carried(s: &str, font: &mut Option<String>, rgb: &mut Option<String>) {
    for run in ml_rich_runs(s) {
        match run {
            MlRun::Font(id) => *font = Some(id.to_string()),
            MlRun::Rgb([r, g, b, a]) => *rgb = Some(format!("{r:02x}{g:02x}{b:02x}{a:02x}")),
            MlRun::Text(_) | MlRun::Bitmap(_) => {}
        }
    }
}

fn prefix(font: &Option<String>, rgb: &Option<String>) -> String {
    let mut p = String::new();
    if let Some(f) = font {
        p.push(FONT_START);
        p.push_str(f);
        p.push(FONT_END);
    }
    if let Some(c) = rgb {
        p.push(RGB_START);
        p.push_str(c);
        p.push(RGB_END);
    }
    p
}

/// Parse the Torque ML subset used by v20 (`<just:...>`, `<br>`, colour
/// codes, `<a:url>..</a>`, `<spush>/<spop>`, `<font:Face:Size>`,
/// `<color:RRGGBB>`, `<lmargin%:N>`, `<rmargin%:N>`, `<bitmap:...>`) and
/// word-wrap to `max_width` (0 = no wrapping). Fonts the pack lacks keep
/// the current one; each line carries the font and colour it starts in.
pub fn layout_ml(
    pack: &Pack,
    font: &Font,
    src: &str,
    max_width: i32,
    default: Justify,
) -> Vec<MlLine> {
    struct Para {
        text: String,
        just: Justify,
        left: i32,
        right: i32,
    }
    let mut just = default;
    let (mut left, mut right) = (0, 100);
    let mut para_left = None;
    let mut cur = String::new();
    let mut rest = src;
    let mut paragraphs: Vec<Para> = Vec::new();
    fn push(
        paragraphs: &mut Vec<Para>,
        cur: &mut String,
        just: Justify,
        para_left: &mut Option<i32>,
        left: i32,
        right: i32,
    ) {
        paragraphs.push(Para {
            text: std::mem::take(cur),
            just,
            left: para_left.take().unwrap_or(left),
            right,
        });
    }
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
                || low.starts_with("tab:")
                || low.starts_with("lmargin")
                || low.starts_with("rmargin");
            if known {
                // Inline bitmaps (death icons) survive layout as a marked run
                // that the ML renderer draws as an image.
                if low.starts_with("bitmap:") {
                    cur.push(BITMAP_START);
                    cur.push_str(&tag["bitmap:".len()..]);
                    cur.push(BITMAP_END);
                }
                if let Some(id) = low.strip_prefix("font:").and_then(font_tag)
                    && pack.data.fonts.contains_key(&id)
                {
                    cur.push(FONT_START);
                    cur.push_str(&id);
                    cur.push(FONT_END);
                }
                if let Some(hex) = low.strip_prefix("color:")
                    && parse_rgb(hex).is_some()
                {
                    cur.push(RGB_START);
                    cur.push_str(hex);
                    cur.push(RGB_END);
                }
                let percent = |t: &str| {
                    t.split(':')
                        .nth(1)
                        .and_then(|v| v.trim().parse::<i32>().ok())
                        .map(|v| v.clamp(0, 100))
                };
                if low.starts_with("lmargin%")
                    && let Some(v) = percent(&low)
                {
                    left = v;
                }
                if low.starts_with("rmargin%")
                    && let Some(v) = percent(&low)
                {
                    right = v;
                }
                if let Some(j) = low.strip_prefix("just:") {
                    just = match j {
                        "center" => Justify::Center,
                        "right" => Justify::Right,
                        _ => Justify::Left,
                    };
                }
                if low == "br" {
                    push(&mut paragraphs, &mut cur, just, &mut para_left, left, right);
                }
                rest = &stripped[end + 1..];
                continue;
            }
        }
        let c = rest.chars().next().expect("non-empty");
        rest = &rest[c.len_utf8()..];
        if c == '\n' {
            push(&mut paragraphs, &mut cur, just, &mut para_left, left, right);
        } else {
            para_left.get_or_insert(left);
            cur.push(c);
        }
    }
    if !cur.is_empty() || paragraphs.is_empty() {
        push(&mut paragraphs, &mut cur, just, &mut para_left, left, right);
    }
    let mut lines = Vec::new();
    let (mut line_font, mut line_rgb): (Option<String>, Option<String>) = (None, None);
    for p in paragraphs {
        let indent = if max_width > 0 {
            max_width * p.left / 100
        } else {
            0
        };
        let room = if max_width > 0 {
            (max_width * p.right / 100 - indent).max(1)
        } else {
            0
        };
        let start = |f: &Option<String>| {
            f.as_deref()
                .and_then(|id| font_named(pack, id))
                .unwrap_or(*font)
        };
        let mut emit =
            |text: String, line_font: &mut Option<String>, line_rgb: &mut Option<String>| {
                let (width, height, _) = measure(pack, start(line_font), &text);
                let full = format!("{}{text}", prefix(line_font, line_rgb));
                carried(&text, line_font, line_rgb);
                lines.push(MlLine {
                    text: full,
                    justify: p.just,
                    width,
                    indent,
                    height,
                });
            };
        let whole = measure(pack, start(&line_font), &p.text).0;
        if room <= 0 || whole <= room {
            emit(p.text, &mut line_font, &mut line_rgb);
            continue;
        }
        // Greedy word wrap, keeping markers attached to words.
        let mut line = String::new();
        for word in p.text.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if measure(pack, start(&line_font), &candidate).0 > room && !line.is_empty() {
                emit(std::mem::take(&mut line), &mut line_font, &mut line_rgb);
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        emit(line, &mut line_font, &mut line_rgb);
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
