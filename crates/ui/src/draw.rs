//! Renderer-independent draw list. The UI produces quads in logical pixels.
//! The GPU backend (or a test) consumes them. Order is painter's order.

use crate::geom::{Rect, Rgba};
use crate::pack::TexKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// Stretched artwork (Torque draws bitmaps with bilinear filtering).
    Linear,
    /// Glyphs and 1:1 pixel art.
    Nearest,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    /// Textured quad: `src` in texture pixels (normalized UVs for External),
    /// `dst` in logical pixels.
    Image {
        tex: TexKey,
        src: [f32; 4],
        dst: [f32; 4],
        tint: Rgba,
        filter: Filter,
        clip: Rect,
    },
    /// Solid rectangle (alpha blended).
    Fill { dst: Rect, color: Rgba, clip: Rect },
}

#[derive(Debug, Default, Clone)]
pub struct DrawList {
    pub cmds: Vec<DrawCmd>,
    clip_stack: Vec<Rect>,
}

impl DrawList {
    pub fn new(screen: Rect) -> Self {
        DrawList {
            cmds: Vec::new(),
            clip_stack: vec![screen],
        }
    }
    pub fn clip(&self) -> Rect {
        *self
            .clip_stack
            .last()
            .unwrap_or(&Rect::new(0, 0, i32::MAX / 2, i32::MAX / 2))
    }
    /// Push a clip rectangle (intersected with the current one). Returns
    /// false (and pushes nothing) when the result is empty.
    pub fn push_clip(&mut self, r: Rect) -> bool {
        match self.clip().intersect(&r) {
            Some(c) => {
                self.clip_stack.push(c);
                true
            }
            None => false,
        }
    }
    pub fn pop_clip(&mut self) {
        if self.clip_stack.len() > 1 {
            self.clip_stack.pop();
        }
    }
    pub fn fill(&mut self, dst: Rect, color: Rgba) {
        if color[3] == 0 || dst.w <= 0 || dst.h <= 0 {
            return;
        }
        let clip = self.clip();
        if clip.intersect(&dst).is_some() {
            self.cmds.push(DrawCmd::Fill { dst, color, clip });
        }
    }
    pub fn image(&mut self, tex: TexKey, src: [f32; 4], dst: [f32; 4], tint: Rgba, filter: Filter) {
        if tint[3] == 0 || dst[2] <= 0.0 || dst[3] <= 0.0 {
            return;
        }
        let clip = self.clip();
        let d = Rect::new(
            dst[0].floor() as i32,
            dst[1].floor() as i32,
            dst[2].ceil() as i32 + 1,
            dst[3].ceil() as i32 + 1,
        );
        if clip.intersect(&d).is_some() {
            self.cmds.push(DrawCmd::Image {
                tex,
                src,
                dst,
                tint,
                filter,
                clip,
            });
        }
    }
    /// Outline (1 logical pixel).
    pub fn frame(&mut self, r: Rect, color: Rgba) {
        self.fill(Rect::new(r.x, r.y, r.w, 1), color);
        self.fill(Rect::new(r.x, r.bottom() - 1, r.w, 1), color);
        self.fill(Rect::new(r.x, r.y + 1, 1, r.h - 2), color);
        self.fill(Rect::new(r.right() - 1, r.y + 1, 1, r.h - 2), color);
    }
    /// All text drawn with a given font, for tests (glyph sheets only).
    pub fn glyph_count(&self) -> usize {
        self.cmds
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    DrawCmd::Image {
                        tex: TexKey::FontSheet(..) | TexKey::Fallback(..),
                        ..
                    }
                )
            })
            .count()
    }
}
