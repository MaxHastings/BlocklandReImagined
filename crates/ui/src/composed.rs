//! Images the UI builds from original art at load, for native controls v20
//! had no art for. The main menu's "Add-Ons" entry is spelled with letters
//! cut from v20's own About, Credits and Options buttons, in each of their
//! states, so it matches the menu. Nothing is drawn or re-encoded from
//! outside the pack: every pixel is an original one.

use crate::pack::Pixels;
use anyhow::{Result, ensure};

/// Image id prefix of the composed Add-Ons button (`_n`, `_h`, `_d`).
pub const ADD_ONS_BUTTON: &str = "native/btnaddons";
/// The v20 button art the letters come from, and its fixed size.
pub const SOURCES: [&str; 3] = [
    "base/client/ui/btnabout",
    "base/client/ui/btncredits",
    "base/client/ui/btnoptions",
];
pub const STATES: [&str; 3] = ["_n", "_h", "_d"];
const WIDTH: u32 = 499;
const HEIGHT: u32 = 72;
/// Width of the hyphen bar.
const HYPHEN: u32 = 26;

/// A vertical strip of an image: columns `x0..x1`, full height.
fn columns(p: &Pixels, x0: u32, x1: u32) -> Pixels {
    let w = x1 - x0;
    let mut rgba = Vec::with_capacity((w * p.height * 4) as usize);
    for y in 0..p.height {
        let row = ((y * p.width + x0) * 4) as usize;
        rgba.extend_from_slice(&p.rgba[row..row + (w * 4) as usize]);
    }
    Pixels {
        width: w,
        height: p.height,
        rgba,
    }
}

fn get(p: &Pixels, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * p.width + x) * 4) as usize;
    [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2], p.rgba[i + 3]]
}

/// Straight-alpha "over" of `src` onto `dst` at (`ox`, `oy`).
fn over(dst: &mut Pixels, src: &Pixels, ox: u32, oy: u32) {
    for y in 0..src.height {
        for x in 0..src.width {
            let (dx, dy) = (ox + x, oy + y);
            if dx >= dst.width || dy >= dst.height {
                continue;
            }
            let s = get(src, x, y);
            let d = get(dst, dx, dy);
            let sa = s[3] as f32 / 255.0;
            let da = d[3] as f32 / 255.0;
            let a = sa + da * (1.0 - sa);
            let i = ((dy * dst.width + dx) * 4) as usize;
            if a <= 0.0 {
                continue;
            }
            for c in 0..3 {
                let v = (s[c] as f32 * sa + d[c] as f32 * da * (1.0 - sa)) / a;
                dst.rgba[i + c] = v.round() as u8;
            }
            dst.rgba[i + 3] = (a * 255.0).round() as u8;
        }
    }
}

/// The hyphen: the lower half of the Options "i" stem (with its rounded,
/// outlined end) laid on its side, mirrored so both ends are rounded, then
/// narrowed to [`HYPHEN`] px.
fn hyphen(options: &Pixels, middle_row: u32) -> Pixels {
    let (x0, x1) = (118, 140);
    let covered = |y: u32| (x0..x1).any(|x| get(options, x, y)[3] > 40);
    let bottom = (0..options.height).rev().find(|&y| covered(y)).unwrap_or(0) + 1;
    let top = bottom.saturating_sub(28);
    // Lay the stem on its side: stem rows become bar columns.
    let half = bottom - top;
    let thick = x1 - x0;
    let long = half * 2;
    let bar = |bx: u32, by: u32| {
        // Right half runs toward the stem's end; left half mirrors it.
        let r = if bx >= half { bx - half } else { half - 1 - bx };
        get(options, x0 + by, top + r)
    };
    let mut out = Pixels {
        width: HYPHEN,
        height: HEIGHT,
        rgba: vec![0; (HYPHEN * HEIGHT * 4) as usize],
    };
    let y0 = middle_row.saturating_sub(thick / 2);
    for by in 0..thick {
        for x in 0..HYPHEN {
            // Nearest column of the long bar.
            let bx = ((x as f32 + 0.5) * long as f32 / HYPHEN as f32) as u32;
            let p = bar(bx.min(long - 1), by);
            let i = (((y0 + by) * HYPHEN + x) * 4) as usize;
            if y0 + by < HEIGHT {
                out.rgba[i..i + 4].copy_from_slice(&p);
            }
        }
    }
    out
}

/// "Add-Ons" in one state, from that state's About, Credits and Options art.
pub fn add_ons_button(about: &Pixels, credits: &Pixels, options: &Pixels) -> Result<Pixels> {
    for p in [about, credits, options] {
        ensure!(
            p.width == WIDTH && p.height == HEIGHT,
            "unexpected v20 button art size {}x{}",
            p.width,
            p.height
        );
    }
    let n = columns(options, 183, 223);
    let rows: Vec<u32> = (0..HEIGHT)
        .filter(|&y| (0..n.width).any(|x| get(&n, x, y)[3] > 40))
        .collect();
    let middle = (rows.first().unwrap_or(&0) + rows.last().unwrap_or(&0)) / 2;
    let d = columns(credits, 121, 161);
    let glyphs = [
        (columns(about, 2, 46), 0),
        (d.clone(), 2),
        (d, 2),
        (hyphen(options, middle), 2),
        (columns(options, 5, 46), 2),
        (n, 3),
        (columns(options, 225, 263), 2),
    ];
    let mut out = Pixels {
        width: WIDTH,
        height: HEIGHT,
        rgba: vec![0; (WIDTH * HEIGHT * 4) as usize],
    };
    let mut x = 2;
    for (glyph, gap) in &glyphs {
        x += gap;
        over(&mut out, glyph, x, 0);
        x += glyph.width;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opaque letters at every column the composer cuts, gaps elsewhere.
    fn art() -> Pixels {
        let mut rgba = vec![0; (WIDTH * HEIGHT * 4) as usize];
        for y in 10..60 {
            for x in 0..WIDTH {
                let i = ((y * WIDTH + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[0, 0, 0, 200]);
            }
        }
        Pixels {
            width: WIDTH,
            height: HEIGHT,
            rgba,
        }
    }

    #[test]
    fn add_ons_is_spelled_from_the_button_art_at_its_size() {
        let a = art();
        let out = add_ons_button(&a, &a, &a).unwrap();
        assert_eq!((out.width, out.height), (WIDTH, HEIGHT));
        // Letters start at the left edge and stop well before the right one.
        let covered = |x: u32| (0..HEIGHT).any(|y| get(&out, x, y)[3] > 0);
        assert!(covered(10));
        assert!(!covered(WIDTH - 1));
        // Other sizes are refused rather than cut at the wrong columns.
        let small = Pixels {
            width: 10,
            height: 10,
            rgba: vec![0; 400],
        };
        assert!(add_ons_button(&small, &a, &a).is_err());
    }
}
