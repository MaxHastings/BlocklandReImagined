//! CPU mip chains for uploaded RGBA8 images. sRGB images are averaged in
//! linear light; colour is weighted by alpha so transparent texels do not
//! darken edges and brick overlays keep their average pigment coverage.
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
            encode_table[(v.clamp(0.0, 1.0) * STEPS as f32 + 0.5) as usize]
        } else {
            (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
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
}
