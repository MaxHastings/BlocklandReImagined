//! Offline mission-cache conversion. Runtime receives finished ordinary PNGs.
use crate::{Reader, interior::png};
use anyhow::{Context, Result, ensure};
use bri_content::{
    interior::Interior,
    scene::{Kind, Scene},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

pub struct Cache {
    pub mission_crc: u32,
    pub terrains: Vec<(u32, Vec<u8>)>,
    pub interiors: Vec<InteriorLight>,
}
pub struct InteriorLight {
    pub crc: u32,
    pub detail_counts: Vec<usize>,
    pub indices: Vec<usize>,
    pub pngs: Vec<Vec<u8>>,
    pub vertex_data: Vec<u8>,
}
fn words(r: &mut Reader<'_>) -> Result<Vec<usize>> {
    let n = r.count(65536)?;
    (0..n).map(|_| r.count(1_000_000)).collect()
}
pub fn read(bytes: &[u8]) -> Result<Cache> {
    let mut r = Reader::new(bytes);
    ensure!(r.u32()? == 16, "Unsupported mission-lighting version");
    let count = r.count(65536)?;
    ensure!(count > 0, "Empty lighting cache");
    ensure!(
        r.u32()? == 0,
        "Lighting cache must start with mission chunk"
    );
    let mut cache = Cache {
        mission_crc: r.u32()?,
        terrains: vec![],
        interiors: vec![],
    };
    for _ in 1..count {
        let kind = r.u32()?;
        let crc = r.u32()?;
        match kind {
            2 => cache.terrains.push((crc, png(&mut r)?)),
            1 => {
                let size = r.count(64 * 1024 * 1024)?;
                let mut mini = Reader::new(r.bytes(size)?);
                let detail_counts = words(&mut mini)?;
                let indices = words(&mut mini)?;
                let image_count = mini.count(65536)?;
                let pngs = (0..image_count)
                    .map(|_| png(&mut mini))
                    .collect::<Result<Vec<_>>>()?;
                mini.finish()?;
                ensure!(
                    detail_counts.iter().sum::<usize>() == indices.len()
                        && indices.len() == pngs.len(),
                    "Lighting index/count mismatch"
                );
                let vertex_data = r.blob32(64 * 1024 * 1024)?;
                cache.interiors.push(InteriorLight {
                    crc,
                    detail_counts,
                    indices,
                    pngs,
                    vertex_data,
                });
            }
            _ => anyhow::bail!("Unsupported lighting chunk {kind}"),
        }
    }
    r.finish()?;
    Ok(cache)
}
/// Legacy caches are additive differences in the authored 8-bit lightmap domain.
/// Compose there, before the renderer interprets lighting; do not add in sRGB.
fn combine(base: &[u8], difference: &[u8]) -> Result<Vec<u8>> {
    let mut base = image::load_from_memory(base)?.to_rgb8();
    let delta = image::load_from_memory(difference)?.to_rgb8();
    ensure!(
        base.dimensions() == delta.dimensions(),
        "Cached lightmap dimensions disagree with interior"
    );
    for (a, b) in base.as_mut().iter_mut().zip(delta.as_raw()) {
        *a = a.saturating_add(*b);
    }
    let mut out = Cursor::new(Vec::new());
    base.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}
/// Cache v16 stores five-bit B,G,R values in an RGB PNG container. These are
/// packed-lightmap components, not an ordinary eight-bit color image.
fn terrain_lightmap(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut rgb = image::load_from_memory(bytes)?.to_rgb8();
    ensure!(
        rgb.dimensions() == (512, 512),
        "Unexpected legacy terrain lightmap size"
    );
    for pixel in rgb.pixels_mut() {
        let [b, g, r] = pixel.0;
        ensure!(
            b <= 31 && g <= 31 && r <= 31,
            "Terrain cache is not five-bit lighting"
        );
        // The classic blender maps five-bit light to a six-bit alpha table.
        let expand = |v: u8| ((u16::from(v) * 2 * 255 + 32) / 63) as u8;
        pixel.0 = [expand(r), expand(g), expand(b)];
    }
    let mut out = Cursor::new(Vec::new());
    rgb.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}
/// Torque's CRC retains the accumulator (no final complement).
fn resource_crc(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    crc
}
fn candidates(archive: &zip::ZipArchive<File>, stem: &str) -> Vec<String> {
    archive
        .file_names()
        .filter(|n| {
            n.to_lowercase().starts_with(&format!("{stem}_")) && n.to_lowercase().ends_with(".ml")
        })
        .map(str::to_owned)
        .collect()
}
fn compatible(interior: &Interior, lighting: &InteriorLight) -> Result<bool> {
    if lighting.detail_counts.len() != interior.details.len() {
        return Ok(false);
    }
    let mut offset = 0;
    for (detail, count) in lighting.detail_counts.iter().enumerate() {
        let mut used = std::collections::BTreeSet::new();
        for i in offset..offset + count {
            let slot = lighting.indices[i];
            let Some(base) = interior.details[detail].lightmaps.get(slot) else {
                return Ok(false);
            };
            if !used.insert(slot) {
                return Ok(false);
            }
            let dimensions = |bytes: &[u8]| -> Result<(u32, u32)> {
                Ok(image::ImageReader::new(Cursor::new(bytes))
                    .with_guessed_format()?
                    .into_dimensions()?)
            };
            if dimensions(&base.png)? != dimensions(&lighting.pngs[i])? {
                return Ok(false);
            }
        }
        offset += count;
    }
    Ok(true)
}
/// Reject ambiguous cache-to-instance mappings instead of relying on ZIP or
/// scene enumeration order. Bounded search stops once a second solution exists.
fn unique_association(candidates: &[Vec<usize>]) -> Result<Vec<usize>> {
    ensure!(
        candidates.len() <= 64 && candidates.iter().flatten().all(|&i| i < candidates.len()),
        "Invalid lighting association candidates"
    );
    fn visit(
        candidates: &[Vec<usize>],
        order: &[usize],
        depth: usize,
        assignment: &mut [usize],
        used: &mut [bool],
        solutions: &mut Vec<Vec<usize>>,
        budget: &mut usize,
    ) -> Result<()> {
        ensure!(*budget > 0, "Lighting association search budget exceeded");
        *budget -= 1;
        if depth == order.len() {
            solutions.push(assignment.to_vec());
            return Ok(());
        }
        let chunk = order[depth];
        for &node in &candidates[chunk] {
            if !used[node] {
                used[node] = true;
                assignment[chunk] = node;
                visit(
                    candidates,
                    order,
                    depth + 1,
                    assignment,
                    used,
                    solutions,
                    budget,
                )?;
                used[node] = false;
                if solutions.len() > 1 {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
    let count = candidates.len();
    let mut order: Vec<_> = (0..count).collect();
    order.sort_by_key(|&i| candidates[i].len());
    let mut solutions = Vec::new();
    visit(
        candidates,
        &order,
        0,
        &mut vec![0; count],
        &mut vec![false; count],
        &mut solutions,
        &mut 100_000,
    )?;
    ensure!(
        solutions.len() == 1,
        "Missing/ambiguous lighting instance association"
    );
    Ok(solutions.remove(0))
}
#[allow(clippy::too_many_arguments)]
pub fn bake(
    root: &Path,
    source: &str,
    scene: &Scene,
    assets: &BTreeMap<String, String>,
    content: &Path,
    output: &Path,
    secondary_root: Option<&Path>,
) -> Result<serde_json::Value> {
    let rest = source
        .strip_prefix("Add-Ons/")
        .context("Cache packaging currently requires an add-on mission")?;
    let (addon, mission) = rest.split_once('/').context("Invalid mission path")?;
    let stem = mission
        .rsplit_once('.')
        .context("Mission extension missing")?
        .0
        .to_lowercase();
    let mut archive = zip::ZipArchive::new(File::open(
        root.join("Add-Ons").join(format!("{addon}.zip")),
    )?)?;
    let mut choices = candidates(&archive, &stem);
    let mut cache_root = root;
    let mut verified_secondary = Vec::new();
    if choices.is_empty()
        && let Some(secondary) = secondary_root
    {
        let path = secondary.join("Add-Ons").join(format!("{addon}.zip"));
        if path.is_file() {
            let other = zip::ZipArchive::new(File::open(path)?)?;
            let other_choices = candidates(&other, &stem);
            if !other_choices.is_empty() {
                // A supplemental cache may only accompany byte-identical source
                // mission and referenced geometry. It never supplies new assets.
                let mut sources = vec![source.to_string()];
                sources.extend(
                    scene
                        .nodes
                        .iter()
                        .filter_map(|n| n.asset.as_ref())
                        .map(|id| id.strip_prefix("v20/").unwrap_or(id).to_string()),
                );
                sources.sort();
                sources.dedup();
                for path in sources {
                    let original = crate::environment::read_original(root, &path)?
                        .with_context(|| format!("Missing primary lighting source {path}"))?;
                    let supplement = crate::environment::read_original(secondary, &path)?
                        .with_context(|| format!("Missing secondary lighting source {path}"))?;
                    ensure!(
                        original == supplement,
                        "Secondary lighting input differs from reference: {path}"
                    );
                    verified_secondary.push(serde_json::json!({"source":path,"sha256":format!("{:x}",Sha256::digest(&original))}));
                }
                archive = other;
                choices = other_choices;
                cache_root = secondary;
            }
        }
    }
    if choices.is_empty() && !scene.nodes.iter().any(|n| matches!(n.kind, Kind::Terrain)) {
        return Ok(
            serde_json::json!({"source":null,"status":"embedded_only","interiors":[],"terrain":[],"warning":"No authored mission-lighting cache in archive; retained original embedded interior lightmaps. Dynamic light/flare behavior remains required."}),
        );
    }
    ensure!(
        choices.len() == 1,
        "Missing/ambiguous stock lighting cache for {source}"
    );
    let mut bytes = Vec::new();
    archive
        .by_name(&choices[0])?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 64 * 1024 * 1024, "Oversized lighting cache");
    let cache = read(&bytes)?;
    let mission_bytes =
        crate::environment::read_original(root, source)?.context("Original mission missing")?;
    ensure!(
        cache.mission_crc == (resource_crc(&mission_bytes) ^ 16),
        "Mission lighting cache CRC does not match original mission {source}"
    );
    let terrains: Vec<_> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, Kind::Terrain))
        .collect();
    let interiors: Vec<_> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, Kind::Interior))
        .collect();
    // No terrain ordering ambiguity: every current reference map has <=1 terrain.
    ensure!(
        terrains.len() == cache.terrains.len()
            && terrains.len() <= 1
            && interiors.len() == cache.interiors.len(),
        "Lighting instance association needs explicit mapping"
    );
    let native_interiors = interiors
        .iter()
        .map(|(_, n)| -> Result<Interior> {
            let file = assets
                .get(n.asset.as_ref().context("Interior ID missing")?)
                .context("Native interior missing")?;
            Ok(serde_json::from_slice(&std::fs::read(content.join(file))?)?)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut possible = Vec::new();
    for lighting in &cache.interiors {
        let mut options = Vec::new();
        for (i, interior) in native_interiors.iter().enumerate() {
            if compatible(interior, lighting)? {
                options.push(i);
            }
        }
        possible.push(options);
    }
    let association = unique_association(&possible)?;
    let export = |png: &[u8]| -> Result<String> {
        let name = format!("{:x}.png", Sha256::digest(png));
        std::fs::write(output.join(&name), png)?;
        Ok(name)
    };
    let mut terrain_maps = Vec::new();
    let mut interior_maps = Vec::new();
    let mut vertex_bytes = 0;
    for ((node, _), (crc, png)) in terrains.iter().zip(&cache.terrains) {
        let decoded = terrain_lightmap(png)?;
        terrain_maps.push(serde_json::json!({"node":node,"source_crc":crc,"file":export(&decoded)?,"source_encoding":"bgr5_in_rgb8_png","native_encoding":"rgb8_modulation"}));
    }
    for (chunk, lighting) in cache.interiors.iter().enumerate() {
        let interior = &native_interiors[association[chunk]];
        let (node, _) = interiors[association[chunk]];
        ensure!(
            lighting.detail_counts.len() == interior.details.len(),
            "Cached interior detail count differs"
        );
        let mut offset = 0;
        for (detail, count) in lighting.detail_counts.iter().enumerate() {
            let mut used = std::collections::BTreeSet::new();
            for i in offset..offset + count {
                let slot = lighting.indices[i];
                ensure!(used.insert(slot), "Duplicate cached lightmap slot");
                let base = &interior.details[detail]
                    .lightmaps
                    .get(slot)
                    .context("Cached lightmap slot out of range")?
                    .png;
                let baked = combine(base, &lighting.pngs[i])?;
                interior_maps.push(serde_json::json!({"node":node,"detail":detail,"slot":slot,"file":export(&baked)?}));
            }
            offset += count;
        }
        vertex_bytes += lighting.vertex_data.len();
    }
    // Keep the optional compressed vertex-lighting data in conversion provenance.
    let provenance = output.join("provenance");
    std::fs::create_dir_all(&provenance)?;
    std::fs::write(
        provenance.join(format!("{:x}.source.ml", Sha256::digest(&bytes))),
        &bytes,
    )?;
    Ok(
        serde_json::json!({"source":format!("Add-Ons/{addon}/{}",choices[0]),"source_root":cache_root,"source_sha256":format!("{:x}",Sha256::digest(&bytes)),"mission_crc":cache.mission_crc,"mission_crc_verified":true,"secondary_source_matches":verified_secondary,"association":"unique complete matching of detail slots and image dimensions; resource CRC sentinel does not independently validate geometry","interior_chunk_nodes":association.iter().map(|&i|interiors[i].0).collect::<Vec<_>>(),"terrain":terrain_maps,"interiors":interior_maps,"vertex_lighting_source_bytes":vertex_bytes}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crc_and_unique_matching_reject_ambiguous_or_incomplete_associations() {
        assert_eq!(resource_crc(b"123456789"), 0x340bc6d9);
        assert_eq!(
            unique_association(&[vec![0, 1], vec![0]]).unwrap(),
            vec![1, 0]
        );
        assert!(unique_association(&[vec![0, 1], vec![0, 1]]).is_err());
        assert!(unique_association(&[vec![0], vec![0]]).is_err());
        assert!(unique_association(&[vec![2]]).is_err());
        assert_eq!(unique_association(&[]).unwrap(), Vec::<usize>::new());
    }
    #[test]
    fn terrain_cache_decodes_five_bit_bgr_and_rejects_other_encodings() {
        let encode = |rgb: [u8; 3]| {
            let mut out = Cursor::new(Vec::new());
            image::RgbImage::from_pixel(512, 512, image::Rgb(rgb))
                .write_to(&mut out, image::ImageFormat::Png)
                .unwrap();
            out.into_inner()
        };
        let decoded = terrain_lightmap(&encode([31, 16, 0])).unwrap();
        assert_eq!(
            image::load_from_memory(&decoded)
                .unwrap()
                .to_rgb8()
                .get_pixel(0, 0)
                .0,
            [0, 130, 251]
        );
        assert!(terrain_lightmap(&encode([32, 0, 0])).is_err());
        assert!(terrain_lightmap(&png([1, 2, 3])).is_err());
    }
    fn png(rgb: [u8; 3]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(1, 1, image::Rgb(rgb))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }
    #[test]
    fn additive_cache_saturates_without_gamma_or_replacement() {
        let out = combine(&png([20, 240, 80]), &png([15, 40, 0])).unwrap();
        assert_eq!(
            image::load_from_memory(&out).unwrap().to_rgb8().as_raw(),
            &[35, 255, 80]
        );
    }
    #[test]
    fn terrain_cache_boundaries_and_truncation() {
        let mut bytes = Vec::new();
        for v in [16_u32, 2, 0, 99, 2, u32::MAX] {
            bytes.extend(v.to_le_bytes());
        }
        bytes.extend(png([20, 30, 40]));
        assert_eq!(read(&bytes).unwrap().terrains.len(), 1);
        for len in 0..bytes.len() {
            assert!(read(&bytes[..len]).is_err());
        }
        bytes.push(0);
        assert!(read(&bytes).is_err());
    }
}
