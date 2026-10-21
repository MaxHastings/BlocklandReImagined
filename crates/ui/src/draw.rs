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
        /// Turned about the middle of `dst`, radians clockwise.
        turn: f32,
    },
    /// Solid rectangle (alpha blended).
    Fill { dst: Rect, color: Rgba, clip: Rect },
}

/// The corners (top left, top right, bottom right, bottom left) of `dst`
/// turned `turn` radians clockwise about its middle.
pub fn corners(dst: [f32; 4], turn: f32) -> [[f32; 2]; 4] {
    let (cx, cy) = (dst[0] + dst[2] / 2.0, dst[1] + dst[3] / 2.0);
    let (hw, hh) = (dst[2] / 2.0, dst[3] / 2.0);
    let (sin, cos) = turn.sin_cos();
    // Screen y points down, so a clockwise turn is the plain rotation.
    [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]]
        .map(|[x, y]| [cx + x * cos - y * sin, cy + x * sin + y * cos])
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
        self.image_turned(tex, src, dst, 0.0, tint, filter);
    }
    /// [`image`](Self::image) turned about the middle of `dst`, radians
    /// clockwise.
    pub fn image_turned(
        &mut self,
        tex: TexKey,
        src: [f32; 4],
        dst: [f32; 4],
        turn: f32,
        tint: Rgba,
        filter: Filter,
    ) {
        if tint[3] == 0 || dst[2] <= 0.0 || dst[3] <= 0.0 || !turn.is_finite() {
            return;
        }
        let clip = self.clip();
        // The box that holds the picture however it is turned.
        let reach = if turn == 0.0 {
            0.0
        } else {
            dst[2].hypot(dst[3])
        };
        let bound = if turn == 0.0 {
            dst
        } else {
            let (cx, cy) = (dst[0] + dst[2] / 2.0, dst[1] + dst[3] / 2.0);
            [cx - reach / 2.0, cy - reach / 2.0, reach, reach]
        };
        let d = Rect::new(
            bound[0].floor() as i32,
            bound[1].floor() as i32,
            bound[2].ceil() as i32 + 1,
            bound[3].ceil() as i32 + 1,
        );
        if clip.intersect(&d).is_some() {
            self.cmds.push(DrawCmd::Image {
                tex,
                src,
                dst,
                tint,
                filter,
                clip,
                turn,
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

#[cfg(test)]
mod tests {
    use super::corners;
    use std::f32::consts::FRAC_PI_2;

    fn near(a: [[f32; 2]; 4], b: [[f32; 2]; 4]) -> bool {
        a.iter()
            .zip(&b)
            .all(|(p, q)| (p[0] - q[0]).abs() < 1e-4 && (p[1] - q[1]).abs() < 1e-4)
    }

    #[test]
    fn a_clockwise_quarter_turn_takes_each_corner_to_the_next() {
        let square = [10.0, 20.0, 4.0, 4.0];
        assert!(near(
            corners(square, 0.0),
            [[10.0, 20.0], [14.0, 20.0], [14.0, 24.0], [10.0, 24.0]]
        ));
        // Top left goes to top right, and so round.
        assert!(near(
            corners(square, FRAC_PI_2),
            [[14.0, 20.0], [14.0, 24.0], [10.0, 24.0], [10.0, 20.0]]
        ));
    }
}
