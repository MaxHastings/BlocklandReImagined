//! Blockland v20 Torque font cache (`base/client/ui/cache/<Face>_<size>.gft`).
//!
//! Layout (little-endian, verified on all 39 caches in the v20 install; this is
//! the older TGE layout, not Torque3D's `GFont::read`):
//! ```text
//! u32 version (=1) | u32 line height | u32 baseline | u32 glyph count
//! glyph count x { u16 sheet | u8 x | u8 y | u8 w | u8 h | i8 xOrigin | i8 yOrigin | i8 advance }
//! u32 sheet count | sheet count x embedded PNG (8-bit coverage)
//! 256 x u16 remap: character code -> glyph index (0xFFFF = none)
//! ```

use anyhow::{Context, Result, bail, ensure};
use bri_ui::schema::Glyph;

pub struct GftFont {
    pub line_height: u32,
    pub baseline: u32,
    /// Original PNG byte streams, one per sheet.
    pub sheets: Vec<Vec<u8>>,
    /// Indexed by character code 0..=255.
    pub glyphs: Vec<Option<Glyph>>,
}

fn u32_at(d: &[u8], off: usize) -> Result<u32> {
    let b = d.get(off..off + 4).context("truncated gft")?;
    Ok(u32::from_le_bytes(b.try_into().expect("4 bytes")))
}

pub fn parse(d: &[u8]) -> Result<GftFont> {
    let version = u32_at(d, 0)?;
    ensure!(version == 1, "unsupported gft version {version}");
    let line_height = u32_at(d, 4)?;
    let baseline = u32_at(d, 8)?;
    ensure!(
        line_height > 0 && line_height <= 4096 && baseline <= 4096,
        "invalid font metrics"
    );
    let count = u32_at(d, 12)? as usize;
    ensure!(count <= 65536, "implausible glyph count {count}");
    let mut off = 16;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let r = d.get(off..off + 9).context("truncated glyph table")?;
        records.push(Glyph {
            sheet: u16::from_le_bytes([r[0], r[1]]),
            x: r[2] as u16,
            y: r[3] as u16,
            w: r[4] as u16,
            h: r[5] as u16,
            x_origin: r[6] as i8 as i16,
            y_origin: r[7] as i8 as i16,
            advance: r[8] as i8 as i16,
        });
        off += 9;
    }
    let sheet_count = u32_at(d, off)? as usize;
    off += 4;
    ensure!(sheet_count <= 64, "implausible sheet count {sheet_count}");
    let mut sheets = Vec::with_capacity(sheet_count);
    for _ in 0..sheet_count {
        ensure!(
            d.get(off..off + 8) == Some(b"\x89PNG\r\n\x1a\n"),
            "expected PNG sheet at {off}"
        );
        let end = png_end(d, off)?;
        let (width, height) = image::ImageReader::new(std::io::Cursor::new(&d[off..end]))
            .with_guessed_format()?
            .into_dimensions()?;
        ensure!(
            width > 0 && height > 0 && width <= 4096 && height <= 4096,
            "oversized or empty font sheet"
        );
        image::load_from_memory(&d[off..end]).context("invalid font sheet PNG")?;
        sheets.push(d[off..end].to_vec());
        off = end;
    }
    let rest = &d[off..];
    if rest.len() != 512 {
        bail!("expected 512-byte remap table, found {}", rest.len());
    }
    let mut glyphs = vec![None; 256];
    for (code, slot) in glyphs.iter_mut().enumerate() {
        let idx = u16::from_le_bytes([rest[code * 2], rest[code * 2 + 1]]);
        if idx != 0xFFFF {
            let g = *records
                .get(idx as usize)
                .context("remap index out of range")?;
            ensure!((g.sheet as usize) < sheet_count, "glyph sheet out of range");
            let (width, height) =
                image::ImageReader::new(std::io::Cursor::new(&sheets[g.sheet as usize]))
                    .with_guessed_format()?
                    .into_dimensions()?;
            ensure!(
                u32::from(g.x) + u32::from(g.w) <= width
                    && u32::from(g.y) + u32::from(g.h) <= height,
                "glyph rectangle outside sheet"
            );
            *slot = Some(g);
        }
    }
    Ok(GftFont {
        line_height,
        baseline,
        sheets,
        glyphs,
    })
}

fn png_end(d: &[u8], start: usize) -> Result<usize> {
    let mut off = start.checked_add(8).context("PNG offset overflow")?;
    loop {
        let header = d.get(off..off + 8).context("truncated PNG chunk")?;
        let len = u32::from_be_bytes(header[..4].try_into()?) as usize;
        let end = off
            .checked_add(12)
            .and_then(|v| v.checked_add(len))
            .context("PNG chunk overflow")?;
        ensure!(end <= d.len(), "truncated PNG payload");
        if &header[4..8] == b"IEND" {
            ensure!(len == 0, "invalid IEND length");
            return Ok(end);
        }
        off = end;
    }
}

/// Split `Face Name_18.gft` into (`Face Name`, 18).
pub fn face_and_size(file_stem: &str) -> Option<(String, u32)> {
    let (face, size) = file_stem.rsplit_once('_')?;
    Some((face.to_string(), size.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_chunk_walker_ignores_iend_inside_payload() {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&4u32.to_be_bytes());
        bytes.extend_from_slice(b"tEXtIENDxxxx");
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.extend_from_slice(b"IENDxxxx");
        assert_eq!(png_end(&bytes, 0).unwrap(), bytes.len());
        assert!(png_end(&bytes[..bytes.len() - 1], 0).is_err());
    }

    fn png_1x1() -> Vec<u8> {
        let mut buf = Vec::new();
        let img = image::GrayImage::from_pixel(16, 16, image::Luma([255]));
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn parses_synthetic_cache_and_rejects_truncation() {
        let mut d = Vec::new();
        for v in [1u32, 14, 11, 2] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 4]); // space: advance 4
        d.extend_from_slice(&[0, 0, 1, 2, 3, 9, 0xFF, 9, 7]); // 'A': xo -1
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&png_1x1());
        let mut remap = vec![0xFFu8; 512];
        remap[32 * 2] = 0;
        remap[32 * 2 + 1] = 0;
        remap[65 * 2] = 1;
        remap[65 * 2 + 1] = 0;
        d.extend_from_slice(&remap);
        let f = parse(&d).unwrap();
        assert_eq!((f.line_height, f.baseline), (14, 11));
        assert_eq!(f.glyphs[32].unwrap().advance, 4);
        let a = f.glyphs[65].unwrap();
        assert_eq!(
            (a.x, a.y, a.w, a.h, a.x_origin, a.advance),
            (1, 2, 3, 9, -1, 7)
        );
        assert!(f.glyphs[66].is_none());
        for cut in [3, 20, 40, d.len() - 1] {
            assert!(parse(&d[..cut]).is_err(), "prefix {cut} must fail");
        }
        assert_eq!(
            face_and_size("Palatino Linotype_24"),
            Some(("Palatino Linotype".into(), 24))
        );
    }
}
