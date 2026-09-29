//! Torque `GuiMLTextCtrl` markup: one parser, layout and renderer for every
//! place that shows server- or event-supplied rich text (center and bottom
//! prints, chat, message boxes, Tutorial prompts and authored ML controls).
//!
//! Supported tags (case-insensitive, as in the Torque ML control):
//! `<br>`, `<sbreak>`, `<color:RRGGBB[AA]>`, `<shadowcolor:RRGGBB[AA]>`,
//! `<shadow:x:y>`, `<linkcolor:..>`, `<linkcolorhl:..>`, `<font:face:size>`,
//! `<just:left|center|right>`, `<lmargin[%]:n>`, `<rmargin[%]:n>`,
//! `<tab:a,b,..>` (stops for `\t`), `<tab>`, `<spush>`, `<spop>`,
//! `<a:url>..</a>` and `<bitmap:path>`. Colour codes `\c0`..`\c9` (U+E000+N),
//! `\cr`, `\cp` and `\co` apply when the control allows colour characters.
//!
//! Other well-formed tags (`<tag:..>`, `<clip:..>`, `<div:..>`, unknown
//! names) are dropped quietly. A `<` that does not start a well-formed tag
//! is ordinary text, so "a < b" and "<3" stay readable.
//!
//! Markup comes from untrusted servers and saves, so parsing is bounded
//! ([`MAX_SOURCE_BYTES`], [`MAX_TAGS`], [`MAX_STYLE_DEPTH`], [`MAX_BITMAPS`],
//! [`MAX_LINES`]), fonts resolve only to the pack's cached fonts, and bitmaps
//! only to pack images under `base/client/ui/` or `add-ons/`. Nothing is read
//! from disk or the network; links render but never open anything.

use crate::draw::{DrawList, Filter};
use crate::geom::{Rect, Rgba};
use crate::pack::{Pack, TexKey};
use crate::schema::Justify;
use crate::text::{COLOR_CODE_BASE, Font};

/// Longest markup source laid out; the rest is ignored.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// Longest single server message kept by [`sanitize`].
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024;
/// Tags honoured per source; later tags are still consumed but ignored.
pub const MAX_TAGS: usize = 1024;
/// Longest tag, `<` to `>` inclusive; longer runs are plain text.
pub const MAX_TAG_BYTES: usize = 256;
/// `<spush>` depth (and `\cp` depth); deeper pushes are ignored.
pub const MAX_STYLE_DEPTH: usize = 32;
/// Inline bitmaps per source.
pub const MAX_BITMAPS: usize = 64;
/// Laid-out lines per source.
pub const MAX_LINES: usize = 512;
/// Largest `<shadow:x:y>` offset in pixels.
pub const MAX_SHADOW: i32 = 8;

/// Link colours when the profile has none: `GuiDefaultProfile`'s
/// `fontColorLink`/`fontColorLinkHL` (v20 allClientScripts c:19603-19604).
pub const DEFAULT_LINK: Rgba = [0, 0, 204, 255];
pub const DEFAULT_LINK_HL: Rgba = [85, 26, 139, 255];

const CODE_RESET: u32 = COLOR_CODE_BASE + 10;
const CODE_PUSH: u32 = COLOR_CODE_BASE + 11;
const CODE_POP: u32 = COLOR_CODE_BASE + 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Margin {
    Px(i32),
    Percent(i32),
}
/// Torque's `GuiMLTextCtrl` keeps each margin as an edge position: `lmargin`
/// and `lmargin%` put the left edge that far in, `rmargin` puts the right
/// edge that many pixels from the right, but `rmargin%` puts it at that share
/// of the width from the left. The help pages' `<lmargin%:3><rmargin%:97>`
/// is a 3% border on each side, not a 3%-wide column.
impl Margin {
    fn left_edge(self, width: i32) -> i32 {
        match self {
            Margin::Px(p) => p,
            Margin::Percent(p) => width.max(0) * p / 100,
        }
    }
    fn right_edge(self, width: i32) -> i32 {
        match self {
            Margin::Px(p) => width - p,
            Margin::Percent(p) => width.max(0) * p / 100,
        }
    }
}

/// One markup element.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Text, possibly with `\t` and colour-code markers.
    Text(String),
    Break,
    Tab,
    Font {
        face: String,
        size: u32,
    },
    Color(Rgba),
    ShadowColor(Rgba),
    Shadow(i32, i32),
    LinkColor(Rgba),
    LinkColorHl(Rgba),
    Just(Justify),
    LMargin(Margin),
    RMargin(Margin),
    TabStops(Vec<i32>),
    Push,
    Pop,
    LinkStart(String),
    LinkEnd,
    /// Validated pack image id (lower case, no extension).
    Bitmap(String),
}

/// Keep a server message printable: bounded length, no control characters
/// other than line breaks and tabs. Markup and colour codes stay intact.
pub fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(MAX_MESSAGE_BYTES));
    for c in text.chars() {
        if out.len() + c.len_utf8() > MAX_MESSAGE_BYTES {
            break;
        }
        if c.is_control() && c != '\n' && c != '\t' {
            continue;
        }
        out.push(c);
    }
    out
}

/// `RRGGBB` or `RRGGBBAA` hex.
pub fn parse_color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if !(s.len() == 6 || s.len() == 8) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if s.len() == 8 { byte(6)? } else { 255 },
    ])
}

/// Pack image id for a `<bitmap:...>` path, if it names UI or add-on art.
pub fn bitmap_id(path: &str) -> Option<String> {
    let p = path.trim().replace('\\', "/").to_ascii_lowercase();
    let p = p.strip_prefix("./").unwrap_or(&p);
    if p.is_empty()
        || p.len() > 128
        || p.contains("..")
        || p.contains("//")
        || !p
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b" /_-.".contains(&b))
        || !["base/client/ui/", "add-ons/"]
            .iter()
            .any(|r| p.starts_with(r))
    {
        return None;
    }
    let p = [".png", ".jpg", ".jpeg"]
        .iter()
        .find_map(|e| p.strip_suffix(e))
        .unwrap_or(p);
    Some(p.to_string())
}

/// A tag body (between `<` and `>`) as a token. `None` = drop quietly.
fn tag_token(body: &str) -> Option<Token> {
    let (name, arg) = match body.find(':') {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let name = name.to_ascii_lowercase();
    let int = |s: &str| s.trim().parse::<i32>().ok();
    Some(match (name.as_str(), arg) {
        ("br" | "sbreak", None) => Token::Break,
        ("tab", None) => Token::Tab,
        ("spush", None) => Token::Push,
        ("spop", None) => Token::Pop,
        ("/a", None) => Token::LinkEnd,
        ("color", Some(a)) => Token::Color(parse_color(a)?),
        ("shadowcolor", Some(a)) => Token::ShadowColor(parse_color(a)?),
        ("linkcolor", Some(a)) => Token::LinkColor(parse_color(a)?),
        ("linkcolorhl", Some(a)) => Token::LinkColorHl(parse_color(a)?),
        ("shadow", Some(a)) => {
            let (x, y) = a.split_once(':')?;
            Token::Shadow(
                int(x)?.clamp(-MAX_SHADOW, MAX_SHADOW),
                int(y)?.clamp(-MAX_SHADOW, MAX_SHADOW),
            )
        }
        ("font", Some(a)) => {
            let (face, size) = a.rsplit_once(':')?;
            let size = int(size)?;
            let face = face.trim();
            if face.is_empty() || face.len() > 64 || !(1..=256).contains(&size) {
                return None;
            }
            Token::Font {
                face: face.to_ascii_lowercase(),
                size: size as u32,
            }
        }
        ("just", Some(a)) => Token::Just(match a.trim().to_ascii_lowercase().as_str() {
            "left" => Justify::Left,
            "center" => Justify::Center,
            "right" => Justify::Right,
            _ => return None,
        }),
        ("lmargin", Some(a)) => Token::LMargin(Margin::Px(int(a)?.clamp(0, 4096))),
        ("lmargin%", Some(a)) => Token::LMargin(Margin::Percent(int(a)?.clamp(0, 100))),
        ("rmargin", Some(a)) => Token::RMargin(Margin::Px(int(a)?.clamp(0, 4096))),
        ("rmargin%", Some(a)) => Token::RMargin(Margin::Percent(int(a)?.clamp(0, 100))),
        ("tab", Some(a)) => Token::TabStops(
            a.split(',')
                .take(32)
                .filter_map(int)
                .map(|t| t.clamp(0, 4096))
                .collect(),
        ),
        ("a", Some(a)) => Token::LinkStart(a.chars().take(256).collect()),
        ("bitmap", Some(a)) => Token::Bitmap(bitmap_id(a)?),
        _ => return None,
    })
}

/// Length of a well-formed tag at the start of `s` (which begins with `<`):
/// `<` + name + optional `:args` + `>`, with no nested `<`.
fn tag_len(s: &str) -> Option<usize> {
    let body = &s[1..];
    let end = body
        .char_indices()
        .take_while(|(i, _)| *i < MAX_TAG_BYTES)
        .find(|(_, c)| *c == '>' || *c == '<' || *c == '\n')
        .filter(|(_, c)| *c == '>')?
        .0;
    let body = &body[..end];
    let name = body.split(':').next().unwrap_or("");
    let name = name.strip_prefix('/').unwrap_or(name);
    let starts_alpha = name.bytes().next().is_some_and(|b| b.is_ascii_alphabetic());
    let valid = starts_alpha && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'%');
    valid.then_some(end + 2)
}

/// Parse markup into tokens, applying every bound.
pub fn parse(src: &str) -> Vec<Token> {
    let mut src = src;
    if src.len() > MAX_SOURCE_BYTES {
        let mut cut = MAX_SOURCE_BYTES;
        while !src.is_char_boundary(cut) {
            cut -= 1;
        }
        src = &src[..cut];
    }
    let mut out = Vec::new();
    let mut text = String::new();
    let mut tags = 0usize;
    let mut bitmaps = 0usize;
    let flush = |text: &mut String, out: &mut Vec<Token>| {
        if !text.is_empty() {
            out.push(Token::Text(std::mem::take(text)));
        }
    };
    let mut rest = src;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(len) = tag_len(rest)
        {
            let body = &rest[1..len - 1];
            rest = &rest[len..];
            tags += 1;
            if tags > MAX_TAGS {
                continue;
            }
            let Some(token) = tag_token(body) else {
                continue;
            };
            if matches!(token, Token::Bitmap(_)) {
                bitmaps += 1;
                if bitmaps > MAX_BITMAPS {
                    continue;
                }
            }
            flush(&mut text, &mut out);
            out.push(token);
            continue;
        }
        rest = &rest[c.len_utf8()..];
        match c {
            '\n' => {
                flush(&mut text, &mut out);
                out.push(Token::Break);
            }
            '\t' => text.push('\t'),
            '\r' => {}
            c if c.is_control() => {}
            c => text.push(c),
        }
    }
    flush(&mut text, &mut out);
    out
}

/// Text state that `<spush>`/`<spop>` save and restore.
#[derive(Debug, Clone, PartialEq)]
struct TextStyle {
    font: String,
    color: Rgba,
    shadow: (i32, i32),
    shadow_color: Rgba,
    link: Rgba,
    link_hl: Rgba,
}

/// Where layout starts: the control's profile and fields.
#[derive(Debug, Clone)]
pub struct MlDefaults<'a> {
    pub font: &'a str,
    pub color: Rgba,
    /// `\c0`..`\c9` palette (profile `fontColors`).
    pub palette: &'a [Option<Rgba>],
    /// Control field `allowColorChars`.
    pub allow_color_chars: bool,
    pub justify: Justify,
    /// Pixels added between lines. Torque registers `lineSpacing` on the
    /// control but its layout never reads it (`emitNewLine` advances by the
    /// font height), so v20 controls pass 0.
    pub line_spacing: i32,
    pub link: Rgba,
    pub link_hl: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Text {
        x: i32,
        font: String,
        text: String,
        color: Rgba,
        /// Offset and colour of the drop shadow, if any.
        shadow: Option<((i32, i32), Rgba)>,
        /// Index into [`Layout::links`].
        link: Option<usize>,
        width: i32,
    },
    Bitmap {
        x: i32,
        id: String,
        w: i32,
        h: i32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Top relative to the layout origin.
    pub y: i32,
    pub height: i32,
    /// Baseline below `y`.
    pub ascent: i32,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub lines: Vec<Line>,
    pub height: i32,
    pub links: Vec<String>,
}

impl Layout {
    /// The link under a point relative to the layout origin.
    pub fn link_at(&self, x: i32, y: i32) -> Option<&str> {
        let line = self.lines.iter().find(|l| y >= l.y && y < l.y + l.height)?;
        line.items.iter().find_map(|i| match i {
            Item::Text {
                x: ix,
                width,
                link: Some(link),
                ..
            } if x >= *ix && x < ix + width => self.links.get(*link).map(String::as_str),
            _ => None,
        })
    }

    /// Plain text of the layout, one string per line (tests, logs).
    pub fn plain_lines(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|l| {
                l.items
                    .iter()
                    .map(|i| match i {
                        Item::Text { text, .. } => text.as_str(),
                        Item::Bitmap { .. } => "",
                    })
                    .collect()
            })
            .collect()
    }
}

/// Pack font id for a face and size: exact, else the nearest cached size of
/// that face. `None` when the face has no cache (the caller keeps its font).
pub fn resolve_font(pack: &Pack, face: &str, size: u32) -> Option<String> {
    let exact = format!("{face}_{size}");
    if pack.font(&exact).is_some() {
        return Some(exact);
    }
    pack.data
        .fonts
        .iter()
        .filter(|(_, f)| f.face.eq_ignore_ascii_case(face))
        .min_by_key(|(_, f)| (f.size.abs_diff(size), f.size))
        .map(|(id, _)| id.clone())
}

struct Builder<'p> {
    pack: &'p Pack,
    width: i32,
    wrap: bool,
    line_spacing: i32,
    lines: Vec<Line>,
    items: Vec<Item>,
    pen: i32,
    y: i32,
    just: Justify,
    line_just: Justify,
    lmargin: Margin,
    rmargin: Margin,
    line_left: i32,
    line_right: i32,
    tabs: Vec<i32>,
    min_height: (i32, i32),
    truncated: bool,
    soft: bool,
    /// Text atoms started by wrapping or an inline bitmap. Torque's
    /// `drawAtomText` restarts each atom in its style colour, so a `\cN`
    /// colour ends there.
    atoms: u32,
}

impl<'p> Builder<'p> {
    fn font(&self, id: &str) -> Option<Font<'p>> {
        let entry = self.pack.font(id)?;
        let (key, _) = self.pack.data.fonts.get_key_value(id)?;
        Some(Font {
            id: key.as_str(),
            entry,
        })
    }
    fn begin_line(&mut self) {
        self.soft = false;
        self.line_just = self.just;
        self.line_left = self.lmargin.left_edge(self.width);
        self.line_right = if self.wrap {
            self.rmargin.right_edge(self.width).max(self.line_left + 1)
        } else {
            i32::MAX / 4
        };
        self.pen = self.line_left;
    }
    /// Soft line break from word wrapping.
    fn wrap_line(&mut self) {
        self.end_line();
        self.begin_line();
        self.soft = true;
        self.atoms += 1;
    }
    fn at_line_start(&self) -> bool {
        self.items.is_empty()
    }
    /// Layout changes (justify, margins) made at the start of a line apply to
    /// that line; later ones apply from the next line.
    fn restart_if_empty(&mut self) {
        if self.at_line_start() {
            self.begin_line();
        }
    }
    fn end_line(&mut self) {
        if self.lines.len() >= MAX_LINES {
            self.truncated = true;
            self.items.clear();
            return;
        }
        // Trailing spaces do not count toward justification.
        if let Some(Item::Text {
            text, width, font, ..
        }) = self.items.last_mut()
        {
            let trimmed = text.trim_end_matches(' ').len();
            if trimmed < text.len() {
                let spaces = (text.len() - trimmed) as i32;
                let space = self
                    .pack
                    .font(font)
                    .and_then(|e| e.glyphs.get(b' ' as usize).copied().flatten())
                    .map_or(0, |g| g.advance as i32);
                text.truncate(trimmed);
                *width -= spaces * space;
            }
        }
        let content_right = self
            .items
            .iter()
            .map(|i| match i {
                Item::Text { x, width, .. } => x + width,
                Item::Bitmap { x, w, .. } => x + w,
            })
            .max()
            .unwrap_or(self.line_left);
        let used = content_right - self.line_left;
        let avail = self.line_right - self.line_left;
        let shift = if self.wrap {
            match self.line_just {
                Justify::Left => 0,
                Justify::Center => ((avail - used) / 2).max(0),
                Justify::Right => (avail - used).max(0),
            }
        } else {
            0
        };
        let (mut ascent, mut descent) = self.min_height;
        let mut items = std::mem::take(&mut self.items);
        for item in &mut items {
            match item {
                Item::Text { x, font, .. } => {
                    *x += shift;
                    if let Some(f) = self.pack.font(font) {
                        ascent = ascent.max(f.baseline as i32);
                        descent = descent.max(f.line_height as i32 - f.baseline as i32);
                    }
                }
                Item::Bitmap { x, h, .. } => {
                    *x += shift;
                    ascent = ascent.max(*h);
                }
            }
        }
        let height = ascent + descent;
        self.lines.push(Line {
            y: self.y,
            height,
            ascent,
            items,
        });
        self.y += height + self.line_spacing;
    }
    fn push_text(&mut self, font: &Font, s: &str, style: RunStyle) {
        if s.is_empty() {
            return;
        }
        let w = font.width(s);
        let run_color = match style.code_atom {
            Some(atom) if atom == self.atoms => style.color,
            _ => style.base,
        };
        if let Some(Item::Text {
            font: f,
            text,
            color,
            shadow,
            link,
            width,
            x,
        }) = self.items.last_mut()
            && f == font.id
            && *color == run_color
            && *shadow == style.shadow
            && *link == style.link
            && *x + *width == self.pen
        {
            text.push_str(s);
            *width += w;
        } else {
            self.items.push(Item::Text {
                x: self.pen,
                font: font.id.to_string(),
                text: s.to_string(),
                color: run_color,
                shadow: style.shadow,
                link: style.link,
                width: w,
            });
        }
        self.pen += w;
    }
    /// Place a word (no spaces), wrapping before it or splitting it when it
    /// alone is wider than the line.
    fn word(&mut self, font: &Font, word: &str, style: RunStyle) {
        let w = font.width(word);
        if self.pen + w > self.line_right && !self.at_line_start() {
            self.wrap_line();
        }
        if self.pen + w <= self.line_right || !self.wrap {
            self.push_text(font, word, style);
            return;
        }
        let mut part = String::new();
        for c in word.chars() {
            let cw = font.width(c.encode_utf8(&mut [0; 4]));
            if self.pen + font.width(&part) + cw > self.line_right && !part.is_empty() {
                self.push_text(font, &part, style);
                part.clear();
                self.wrap_line();
            }
            part.push(c);
        }
        self.push_text(font, &part, style);
    }
    fn space(&mut self, font: &Font, style: RunStyle) {
        // Spaces at the start of a wrapped line are dropped.
        if self.at_line_start() && self.soft {
            return;
        }
        let w = font.width(" ");
        if self.pen + w > self.line_right && self.wrap {
            if !self.at_line_start() {
                self.wrap_line();
            }
            return;
        }
        self.push_text(font, " ", style);
    }
    fn tab(&mut self, font: &Font) {
        let rel = self.pen - self.line_left;
        match self.tabs.iter().find(|t| **t > rel) {
            Some(t) => self.pen = self.line_left + t,
            None => self.pen += font.width(" "),
        }
    }
    fn bitmap(&mut self, id: &str) {
        let Some((w, h)) = self.pack.image_size(id) else {
            return;
        };
        let (w, h) = (w.min(1024) as i32, h.min(1024) as i32);
        if self.pen + w > self.line_right && !self.at_line_start() {
            self.wrap_line();
        }
        self.items.push(Item::Bitmap {
            x: self.pen,
            id: id.to_string(),
            w,
            h,
        });
        self.pen += w;
        self.atoms += 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RunStyle {
    /// Colour while the `\cN` code set in atom `code_atom` is in force.
    color: Rgba,
    /// Style (or link) colour a new atom starts in.
    base: Rgba,
    code_atom: Option<u32>,
    shadow: Option<((i32, i32), Rgba)>,
    link: Option<usize>,
}

/// Lay out markup in a box `width` wide (0 = no wrapping).
pub fn layout(pack: &Pack, src: &str, width: i32, d: &MlDefaults) -> Layout {
    let base = TextStyle {
        font: d.font.to_string(),
        color: d.color,
        shadow: (0, 0),
        shadow_color: [0, 0, 0, 255],
        link: d.link,
        link_hl: d.link_hl,
    };
    let mut b = Builder {
        pack,
        width,
        wrap: width > 0,
        line_spacing: d.line_spacing.clamp(0, 64),
        lines: Vec::new(),
        items: Vec::new(),
        pen: 0,
        y: 0,
        just: d.justify,
        line_just: d.justify,
        lmargin: Margin::Px(0),
        rmargin: Margin::Px(0),
        line_left: 0,
        line_right: 0,
        tabs: Vec::new(),
        min_height: (0, 0),
        truncated: false,
        soft: false,
        atoms: 0,
    };
    b.begin_line();
    let mut style = base.clone();
    let mut stack: Vec<TextStyle> = Vec::new();
    // A `\cN` colour lasts to the end of its text atom. Torque's
    // `drawAtomText` sets the style colour before drawing each atom, and a
    // new atom starts at every tag, tab, line break and word wrap
    // (`emitTextToken`, `splitAtomListEmit`). The atom a code was set in is
    // remembered so a wrap inside a word run also ends it.
    let mut code: Option<(Rgba, u32)> = None;
    let mut code_stack: Vec<Option<(Rgba, u32)>> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    let mut link: Option<usize> = None;
    let default_font = |b: &Builder<'_>, id: &str| {
        b.font(id).map(|f| {
            (
                f.entry.baseline as i32,
                f.line_height() - f.entry.baseline as i32,
            )
        })
    };
    if let Some(m) = default_font(&b, d.font) {
        b.min_height = m;
    }
    for token in parse(src) {
        if b.truncated {
            break;
        }
        match token {
            Token::Text(text) => {
                let Some(font) = b.font(&style.font).or_else(|| b.font(d.font)) else {
                    continue;
                };
                let mut word = String::new();
                let run = |code: Option<(Rgba, u32)>, style: &TextStyle, link: Option<usize>| {
                    let base = if link.is_some() {
                        style.link
                    } else {
                        style.color
                    };
                    RunStyle {
                        color: code.map_or(base, |c| c.0),
                        base,
                        code_atom: code.map(|c| c.1),
                        shadow: (style.shadow != (0, 0))
                            .then_some((style.shadow, style.shadow_color)),
                        link,
                    }
                };
                for c in text.chars() {
                    let u = c as u32;
                    let is_code = (COLOR_CODE_BASE..=CODE_POP).contains(&u);
                    if c == ' ' || c == '\t' || is_code {
                        b.word(&font, &word, run(code, &style, link));
                        word.clear();
                    }
                    if is_code {
                        if !d.allow_color_chars {
                            continue;
                        }
                        match u {
                            CODE_RESET => code = None,
                            CODE_PUSH => {
                                if code_stack.len() < MAX_STYLE_DEPTH {
                                    code_stack.push(code);
                                }
                            }
                            CODE_POP => code = code_stack.pop().flatten(),
                            _ => {
                                let i = (u - COLOR_CODE_BASE) as usize;
                                let c = d.palette.get(i).copied().flatten().unwrap_or(style.color);
                                code = Some((c, b.atoms));
                            }
                        }
                    } else if c == ' ' {
                        b.space(&font, run(code, &style, link));
                    } else if c == '\t' {
                        b.tab(&font);
                        code = None;
                    } else {
                        word.push(c);
                    }
                }
                b.word(&font, &word, run(code, &style, link));
            }
            Token::Break => {
                b.end_line();
                b.begin_line();
                code = None;
            }
            Token::Tab => {
                if let Some(font) = b.font(&style.font).or_else(|| b.font(d.font)) {
                    b.tab(&font);
                }
                code = None;
            }
            Token::Font { face, size } => {
                if let Some(id) = resolve_font(pack, &face, size) {
                    style.font = id;
                }
                code = None;
            }
            Token::Color(c) => {
                style.color = c;
                code = None;
            }
            Token::ShadowColor(c) => {
                style.shadow_color = c;
                code = None;
            }
            Token::Shadow(x, y) => {
                style.shadow = (x, y);
                code = None;
            }
            Token::LinkColor(c) => {
                style.link = c;
                code = None;
            }
            Token::LinkColorHl(c) => {
                style.link_hl = c;
                code = None;
            }
            Token::Just(j) => {
                b.just = j;
                b.restart_if_empty();
                code = None;
            }
            Token::LMargin(m) => {
                b.lmargin = m;
                b.restart_if_empty();
                code = None;
            }
            Token::RMargin(m) => {
                b.rmargin = m;
                b.restart_if_empty();
                code = None;
            }
            Token::TabStops(t) => {
                b.tabs = t;
                code = None;
            }
            Token::Push => {
                if stack.len() < MAX_STYLE_DEPTH {
                    stack.push(style.clone());
                }
                code = None;
            }
            Token::Pop => {
                if let Some(s) = stack.pop() {
                    style = s;
                }
                code = None;
            }
            Token::LinkStart(url) => {
                if links.len() < MAX_TAGS {
                    links.push(url);
                    link = Some(links.len() - 1);
                }
                code = None;
            }
            Token::LinkEnd => {
                link = None;
                code = None;
            }
            Token::Bitmap(id) => {
                b.bitmap(&id);
                code = None;
            }
        }
    }
    if !b.items.is_empty() || b.lines.is_empty() {
        b.end_line();
    }
    let height = b.lines.last().map_or(0, |l| l.y + l.height);
    Layout {
        lines: b.lines,
        height,
        links,
    }
}

/// Draw a layout at `origin`. `outline` is the profile's font outline
/// (`doFontOutline`), drawn around glyphs under the text.
pub fn draw(
    pack: &Pack,
    dl: &mut DrawList,
    layout: &Layout,
    origin: (i32, i32),
    outline: Option<Rgba>,
) {
    for line in &layout.lines {
        let top = origin.1 + line.y;
        for item in &line.items {
            match item {
                Item::Text {
                    x,
                    font,
                    text,
                    color,
                    shadow,
                    link,
                    width,
                } => {
                    let Some(f) = Font::get(pack, font) else {
                        continue;
                    };
                    let x = (origin.0 + x) as f32;
                    let y = (top + line.ascent - f.entry.baseline as i32) as f32;
                    if let Some(((dx, dy), sc)) = shadow {
                        f.draw(dl, x + *dx as f32, y + *dy as f32, text, *sc, &[]);
                    }
                    f.draw_outlined(dl, x, y, text, *color, outline, &[]);
                    if link.is_some() {
                        let underline = top + line.ascent + 1;
                        if let Some(o) = outline {
                            dl.fill(Rect::new(x as i32 - 1, underline - 1, width + 2, 3), o);
                        }
                        dl.fill(Rect::new(x as i32, underline, *width, 1), *color);
                    }
                }
                Item::Bitmap { x, id, w, h } => {
                    dl.image(
                        TexKey::Image(id.clone()),
                        [0.0, 0.0, *w as f32, *h as f32],
                        [
                            (origin.0 + x) as f32,
                            (top + line.ascent - h) as f32,
                            *w as f32,
                            *h as f32,
                        ],
                        crate::geom::WHITE,
                        Filter::Linear,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{FontEntry, Glyph, ImageEntry, UiPack};

    fn font(face: &str, size: u32) -> FontEntry {
        let mut glyphs = vec![None; 256];
        for (b, g) in glyphs.iter_mut().enumerate().skip(32).take(224) {
            *g = Some(Glyph {
                sheet: 0,
                x: 0,
                y: 0,
                w: if b == 32 { 0 } else { 5 },
                h: 8,
                x_origin: 0,
                y_origin: 8,
                advance: 6,
            });
        }
        FontEntry {
            face: face.into(),
            size,
            line_height: size,
            baseline: size * 3 / 4,
            sheets: vec![],
            glyphs,
            source: String::new(),
            sha256: String::new(),
        }
    }

    fn pack() -> Pack {
        let mut data = UiPack::default();
        data.fonts.insert("arial_14".into(), font("Arial", 14));
        data.fonts.insert("arial_24".into(), font("Arial", 24));
        data.fonts.insert("impact_18".into(), font("Impact", 18));
        data.images.insert(
            "base/client/ui/ci/skull".into(),
            ImageEntry {
                file: String::new(),
                width: 16,
                height: 20,
                sha256: String::new(),
                source: String::new(),
            },
        );
        Pack::from_parts(data, Default::default())
    }

    const PALETTE: [Option<Rgba>; 10] = [
        Some([255, 0, 64, 255]),
        Some([64, 64, 255, 255]),
        Some([0, 255, 0, 255]),
        Some([255, 255, 0, 255]),
        None,
        None,
        Some([255, 255, 255, 255]),
        Some([96, 96, 96, 255]),
        None,
        None,
    ];

    fn defaults() -> MlDefaults<'static> {
        MlDefaults {
            font: "arial_14",
            color: [0, 0, 0, 255],
            palette: &PALETTE,
            allow_color_chars: true,
            justify: Justify::Left,
            line_spacing: 0,
            link: DEFAULT_LINK,
            link_hl: DEFAULT_LINK_HL,
        }
    }

    fn texts(l: &Layout) -> Vec<(String, Rgba)> {
        l.lines
            .iter()
            .flat_map(|l| &l.items)
            .filter_map(|i| match i {
                Item::Text { text, color, .. } => Some((text.clone(), *color)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn badspot_birthday_event_renders_white_on_two_lines() {
        let p = pack();
        let l = layout(
            &p,
            "<just:center><color:FFFFFF>It's no longer Badspot's' Birthday.<br>Attempts to butter Badspot will go ignored from now.\n",
            2000,
            &defaults(),
        );
        assert_eq!(
            l.plain_lines(),
            [
                "It's no longer Badspot's' Birthday.",
                "Attempts to butter Badspot will go ignored from now.",
            ]
        );
        assert!(texts(&l).iter().all(|(_, c)| *c == [255, 255, 255, 255]));
        // Centered: the first line starts right of the left edge.
        let Item::Text { x, width, .. } = &l.lines[0].items[0] else {
            panic!()
        };
        assert_eq!(*x, (2000 - width) / 2);
    }

    #[test]
    fn colour_codes_follow_the_palette_and_reset_on_break() {
        let p = pack();
        let l = layout(&p, "Press \u{E003}W\u{E000} to move\nnext", 0, &defaults());
        assert_eq!(
            texts(&l),
            [
                ("Press ".into(), [0, 0, 0, 255]),
                ("W".into(), [255, 255, 0, 255]),
                (" to move".into(), [255, 0, 64, 255]),
                ("next".into(), [0, 0, 0, 255]),
            ]
        );
        let off = MlDefaults {
            allow_color_chars: false,
            ..defaults()
        };
        let l = layout(&p, "a\u{E003}b", 0, &off);
        assert_eq!(texts(&l), [("ab".into(), [0, 0, 0, 255])]);
    }

    #[test]
    fn spush_spop_restore_colour_font_and_shadow() {
        let p = pack();
        let l = layout(
            &p,
            "<spush><font:impact:18><color:ff0000><shadow:1:1>a<spop>b",
            0,
            &defaults(),
        );
        let items = &l.lines[0].items;
        let Item::Text {
            font,
            color,
            shadow,
            ..
        } = &items[0]
        else {
            panic!()
        };
        assert_eq!((font.as_str(), *color), ("impact_18", [255, 0, 0, 255]));
        assert_eq!(*shadow, Some(((1, 1), [0, 0, 0, 255])));
        let Item::Text {
            font,
            color,
            shadow,
            ..
        } = &items[1]
        else {
            panic!()
        };
        assert_eq!(
            (font.as_str(), *color, *shadow),
            ("arial_14", [0, 0, 0, 255], None)
        );
    }

    #[test]
    fn fonts_fall_back_to_the_nearest_cached_size() {
        let p = pack();
        assert_eq!(resolve_font(&p, "arial", 22).as_deref(), Some("arial_24"));
        assert_eq!(resolve_font(&p, "arial", 999).as_deref(), Some("arial_24"));
        assert_eq!(resolve_font(&p, "comic sans", 14), None);
        let l = layout(&p, "<font:Comic Sans:14>x", 0, &defaults());
        let Item::Text { font, .. } = &l.lines[0].items[0] else {
            panic!()
        };
        assert_eq!(font, "arial_14");
    }

    #[test]
    fn unknown_tags_are_dropped_and_stray_brackets_stay() {
        let p = pack();
        let l = layout(
            &p,
            "a<foo:bar>b<tag:1><clip:5>c </clip>I <3 a < b > c",
            0,
            &defaults(),
        );
        assert_eq!(l.plain_lines(), ["abc I <3 a < b > c"]);
        assert_eq!(
            parse("<Color:00ff00>x"),
            [Token::Color([0, 255, 0, 255]), Token::Text("x".into())]
        );
        assert_eq!(parse("<color:zz>x"), [Token::Text("x".into())]);
    }

    #[test]
    fn justification_margins_and_tabs() {
        let p = pack();
        let l = layout(
            &p,
            "<just:right>ab<br><just:left><lmargin:10>cd<br><tab:30>\te",
            100,
            &defaults(),
        );
        let x = |i: usize| match &l.lines[i].items[0] {
            Item::Text { x, .. } => *x,
            _ => panic!(),
        };
        assert_eq!(x(0), 100 - 12);
        assert_eq!(x(1), 10);
        assert_eq!(x(2), 40);
        let l = layout(&p, "<rmargin%:50>aaaa bbbb", 100, &defaults());
        assert_eq!(l.plain_lines(), ["aaaa", "bbbb"]);
    }

    #[test]
    fn wraps_words_and_splits_overlong_words() {
        let p = pack();
        let l = layout(&p, "hello world abcdefghij", 36, &defaults());
        assert_eq!(l.plain_lines(), ["hello", "world", "abcdef", "ghij"]);
    }

    #[test]
    fn bitmaps_only_resolve_to_pack_ui_art() {
        assert_eq!(
            bitmap_id("base/client/ui/CI/skull.png").as_deref(),
            Some("base/client/ui/ci/skull")
        );
        assert_eq!(
            bitmap_id("Add-Ons/Foo/icon").as_deref(),
            Some("add-ons/foo/icon")
        );
        for bad in [
            "../../secret",
            "http://evil/x.png",
            "C:/x",
            "base/client/ui/../../x",
            "config/prefs",
            "",
        ] {
            assert_eq!(bitmap_id(bad), None, "{bad}");
        }
        let p = pack();
        let l = layout(
            &p,
            "a<bitmap:base/client/ui/ci/skull>b<bitmap:base/client/ui/missing>",
            0,
            &defaults(),
        );
        let bitmaps = l.lines[0]
            .items
            .iter()
            .filter(|i| matches!(i, Item::Bitmap { .. }))
            .count();
        assert_eq!(bitmaps, 1);
        assert_eq!(l.lines[0].ascent, 20);
    }

    #[test]
    fn colour_codes_end_at_wraps_and_tags_like_torque_atoms() {
        let p = pack();
        let l = layout(&p, "\u{E003}aaaa bbbb", 30, &defaults());
        assert_eq!(
            texts(&l),
            [
                ("aaaa".into(), [255, 255, 0, 255]),
                ("bbbb".into(), [0, 0, 0, 255])
            ]
        );
        let l = layout(&p, "\u{E003}a<just:left>b", 0, &defaults());
        assert_eq!(
            texts(&l),
            [
                ("a".into(), [255, 255, 0, 255]),
                ("b".into(), [0, 0, 0, 255])
            ]
        );
    }

    #[test]
    fn link_hit_testing() {
        let p = pack();
        let l = layout(&p, "go <a:example.com>here</a> now", 0, &defaults());
        assert_eq!(l.link_at(19, 3), Some("example.com"));
        assert_eq!(l.link_at(2, 3), None);
        assert_eq!(l.link_at(19, 40), None);
    }

    #[test]
    fn links_use_link_colour_and_are_recorded() {
        let p = pack();
        let l = layout(&p, "<a:blockland.us>site</a> x", 0, &defaults());
        assert_eq!(l.links, ["blockland.us"]);
        assert_eq!(texts(&l)[0], ("site".into(), DEFAULT_LINK));
        let Item::Text { link, .. } = &l.lines[0].items[0] else {
            panic!()
        };
        assert_eq!(*link, Some(0));
    }

    #[test]
    fn hostile_markup_is_bounded() {
        let p = pack();
        let mut s = "<spush><font:impact:999>".repeat(5000);
        s.push_str(&"<zz>".repeat(20_000));
        s.push_str(&"<br>".repeat(5000));
        s.push_str("tail");
        let tokens = parse(&s);
        assert!(tokens.len() <= MAX_TAGS + 1);
        let l = layout(&p, &s, 200, &defaults());
        assert!(l.lines.len() <= MAX_LINES);
        let huge = "x".repeat(MAX_SOURCE_BYTES * 2);
        let l = layout(&p, &huge, 0, &defaults());
        let chars: usize = l.plain_lines().iter().map(|s| s.len()).sum();
        assert!(chars <= MAX_SOURCE_BYTES);
        assert!(sanitize(&huge).len() <= MAX_MESSAGE_BYTES);
        assert_eq!(sanitize("a\u{7}b\nc\u{E003}d"), "ab\nc\u{E003}d");
    }

    #[test]
    fn line_spacing_and_mixed_font_baselines() {
        let p = pack();
        let d = MlDefaults {
            line_spacing: 2,
            ..defaults()
        };
        let l = layout(&p, "a<font:arial:24>B<br>c", 0, &d);
        assert_eq!(l.lines[0].ascent, 18);
        assert_eq!(l.lines[0].height, 24);
        assert_eq!(l.lines[1].y, 26);
    }

    #[test]
    fn percent_margins_are_edges_like_the_help_pages() {
        let p = pack();
        // HelpDlg's pages: a 3% border each side of the 315-pixel HelpText.
        let l = layout(
            &p,
            "<lmargin%:3><rmargin%:97>Press B to open the brick selector.",
            315,
            &defaults(),
        );
        assert_eq!(l.plain_lines(), ["Press B to open the brick selector."]);
        let Item::Text { x, .. } = &l.lines[0].items[0] else {
            panic!()
        };
        assert_eq!(*x, 9);
        // rmargin% is the right edge's position; rmargin in pixels is its
        // distance from the right.
        let l = layout(&p, "<rmargin%:30>aaaa bbbb", 100, &defaults());
        assert_eq!(l.plain_lines(), ["aaaa", "bbbb"]);
        let l = layout(&p, "<rmargin:60>aaaa bbbb", 100, &defaults());
        assert_eq!(l.plain_lines(), ["aaaa", "bbbb"]);
        let l = layout(&p, "<rmargin%:90>aaaa bbbb", 100, &defaults());
        assert_eq!(l.plain_lines(), ["aaaa bbbb"]);
    }
}
