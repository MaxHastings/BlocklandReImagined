//! Integer UI geometry in logical pixels (the authored 640x480 space scaled by
//! Torque's resize rules; physical pixels = logical × UI scale).

use bri_console::Clamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.right() && py < self.bottom()
    }
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
    pub fn inset(&self, d: i32) -> Rect {
        Rect::new(
            self.x + d,
            self.y + d,
            (self.w - 2 * d).max(0),
            (self.h - 2 * d).max(0),
        )
    }
}

pub type Rgba = [u8; 4];

pub const WHITE: Rgba = [255, 255, 255, 255];
pub const BLACK: Rgba = [0, 0, 0, 255];

/// 0..1 float colour to bytes.
pub fn from_f32(c: [f32; 4]) -> Rgba {
    let q = |x: f32| (x.clamped(0.0, 1.0) * 255.0).round() as u8;
    [q(c[0]), q(c[1]), q(c[2]), q(c[3])]
}
