//! CPU mip chains for uploaded RGBA8 images. sRGB images are averaged in
//! linear light; colour is weighted by alpha so transparent texels do not
//! darken edges and brick overlays keep their average pigment coverage.
use bri_console::Clamp;
use std::borrow::Cow;

/// Level 0 (borrowed) through 1x1, as (width, height, pixels).
pub fn chain(width: u32, height: u32, rgba: &[u8], srgb: bool) -> Vec<(u32, u32, Cow<'_, [u8]>)> {
    let mut levels = vec![(width, height, Cow::Borrowed(rgba))];
    let decode: [f32; 256] = std::array::from_fn(|v| {
        let v = v as f32 / 255.0;
        if srgb { srgb_to_linear(v) } else { v }
    });
    // Linear light to 8-bit sRGB through a 4096-step table.
    const STEPS: usize = 4096;
    let encode_table: Vec<u8> = (0..=STEPS)
        .map(|i| {
            let v = i as f32 / STEPS as f32;
            let v = if srgb { linear_to_srgb(v) } else { v };
            (v * 255.0 + 0.5) as u8
        })
        .collect();
    let encode = |v: f32| -> u8 {
        if srgb {
            encode_table[(v.clamped(0.0, 1.0) * STEPS as f32 + 0.5) as usize]
        } else {
            (v.clamped(0.0, 1.0) * 255.0 + 0.5) as u8
        }
    };
    let decode = |v: u8| decode[usize::from(v)];
    while let Some((w, h, pixels)) = levels.last().filter(|(w, h, _)| *w > 1 || *h > 1) {
        let (w, h) = (*w as usize, *h as usize);
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = Vec::with_capacity(nw * nh * 4);
        for y in 0..nh {
            for x in 0..nw {
                let mut color = [0.0f32; 3];
                let mut plain = [0.0f32; 3];
                let mut alpha = 0.0f32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (x * 2 + dx).min(w - 1);
                    let sy = (y * 2 + dy).min(h - 1);
                    let texel = &pixels[(sy * w + sx) * 4..][..4];
                    let a = f32::from(texel[3]) / 255.0;
                    for c in 0..3 {
                        let value = decode(texel[c]);
                        color[c] += value * a;
                        plain[c] += value;
                    }
                    alpha += a;
                }
                for c in 0..3 {
                    next.push(encode(if alpha > 0.0 {
                        color[c] / alpha
                    } else {
                        plain[c] / 4.0
                    }));
                }
                next.push((alpha / 4.0 * 255.0 + 0.5) as u8);
            }
        }
        levels.push((nw as u32, nh as u32, Cow::Owned(next)));
    }
    levels
}

/// As `chain`, for alpha-tested images: each smaller level's alpha is scaled
/// so the fraction of texels passing `cutoff` matches level 0. Plain
/// averaging would thin cut-out grass and leaves until they vanish.
pub fn chain_preserving_coverage(
    width: u32,
    height: u32,
    rgba: &[u8],
    srgb: bool,
    cutoff: f32,
) -> Vec<(u32, u32, Cow<'_, [u8]>)> {
    let threshold = (cutoff.clamped(0.0, 1.0) * 255.0) as u32;
    let coverage = |pixels: &[u8], scale: f32| {
        let passing = pixels
            .chunks_exact(4)
            .filter(|t| ((f32::from(t[3]) * scale) as u32).min(255) > threshold)
            .count();
        passing as f32 / (pixels.len() / 4).max(1) as f32
    };
    let mut levels = chain(width, height, rgba, srgb);
    let target = coverage(rgba, 1.0);
    for (_, _, pixels) in levels.iter_mut().skip(1) {
        let (mut low, mut high) = (0.0f32, 64.0f32);
        for _ in 0..20 {
            let mid = (low + high) / 2.0;
            if coverage(pixels, mid) < target {
                low = mid;
            } else {
                high = mid;
            }
        }
        // Small levels may not reach the target exactly; take the nearer side.
        let scale =
            if (coverage(pixels, low) - target).abs() < (coverage(pixels, high) - target).abs() {
                low
            } else {
                high
            };
        let pixels = pixels.to_mut();
        for texel in pixels.chunks_exact_mut(4) {
            texel[3] = (f32::from(texel[3]) * scale).min(255.0) as u8;
        }
    }
    levels
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_halve_to_one_texel_including_odd_sizes() {
        let rgba = vec![255; 5 * 3 * 4];
        let sizes: Vec<_> = chain(5, 3, &rgba, true)
            .iter()
            .map(|(w, h, p)| {
                assert_eq!(p.len(), (*w * *h * 4) as usize);
                (*w, *h)
            })
            .collect();
        assert_eq!(sizes, [(5, 3), (2, 1), (1, 1)]);
        assert_eq!(chain(1, 1, &[1, 2, 3, 4], false).len(), 1);
    }

    #[test]
    fn srgb_averages_in_linear_light_and_alpha_weights_colour() {
        // Black and white average to linear 0.5, which is sRGB 188.
        let checker = [
            0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
        ];
        assert_eq!(&chain(2, 2, &checker, true)[1].2[..], &[188, 188, 188, 255]);
        assert_eq!(
            &chain(2, 2, &checker, false)[1].2[..],
            &[128, 128, 128, 255]
        );
        // A transparent black texel does not darken an opaque red neighbour.
        let edge = [255, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0];
        assert_eq!(&chain(2, 2, &edge, false)[1].2[..], &[255, 0, 0, 128]);
        // Fully transparent texels keep their plain average colour.
        let clear = [200, 100, 0, 0, 100, 50, 0, 0, 200, 100, 0, 0, 100, 50, 0, 0];
        assert_eq!(&chain(2, 2, &clear, false)[1].2[..], &[150, 75, 0, 0]);
    }

    #[test]
    fn cut_out_coverage_survives_down_to_small_levels() {
        // Scattered opaque texels (about 30%) in a transparent 64x64 image.
        let (w, h) = (64u32, 64u32);
        let mut seed = 12345u32;
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let a = if (seed >> 16) % 10 < 3 { 255 } else { 0 };
                [40, 160, 40, a]
            })
            .collect();
        let passing = |p: &[u8]| {
            p.chunks_exact(4).filter(|t| t[3] > 127).count() as f32 / (p.len() / 4) as f32
        };
        let base = passing(&rgba);
        // Plain averaging thins the cut-out; the preserving chain keeps it.
        assert!(passing(&chain(w, h, &rgba, true)[2].2) < base * 0.5);
        let kept = chain_preserving_coverage(w, h, &rgba, true, 0.5);
        for (_, _, level) in &kept[1..4] {
            assert!(
                (passing(level) - base).abs() < 0.12,
                "{} vs {base}",
                passing(level)
            );
        }
    }
}
