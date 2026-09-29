//! Offline DIF resource 44 / interior 0 decoder. Never used by the runtime.
use crate::Reader;
use anyhow::{Context, Result, ensure};
use bri_content::interior::*;
use glam::Vec3;
use serde::Serialize;
const MAX: usize = 4_000_000;
fn axis([x, y, z]: [f32; 3]) -> [f32; 3] {
    [x, z, -y]
}
fn v3(r: &mut Reader<'_>) -> Result<[f32; 3]> {
    Ok([r.f32()?, r.f32()?, r.f32()?])
}
fn items<T>(r: &mut Reader<'_>, mut f: impl FnMut(&mut Reader<'_>) -> Result<T>) -> Result<Vec<T>> {
    let n = r.count(MAX)?;
    (0..n).map(|_| f(r)).collect()
}
#[derive(Clone, Serialize)]
pub struct Provenance {
    pub warnings: Vec<String>,
    pub preserved_sections: Vec<Section>,
    /// Per detail level, each source surface's lightmap rectangle in texels:
    /// `[mapOffsetX, mapOffsetY, mapSizeX, mapSizeY]`. Offline lighting only.
    #[serde(skip)]
    pub lightmap_rects: Vec<Vec<[u8; 4]>>,
}
#[derive(Clone, Serialize)]
pub struct Section {
    pub label: String,
    pub offset: usize,
    pub length: usize,
}
fn keep(r: &mut Reader<'_>, p: &mut Provenance, label: &str, size: usize) -> Result<()> {
    let offset = r.position();
    r.bytes(size)?;
    p.preserved_sections.push(Section {
        label: label.into(),
        offset,
        length: size,
    });
    Ok(())
}
fn skip_array(r: &mut Reader<'_>, p: &mut Provenance, label: &str, stride: usize) -> Result<()> {
    let n = r.count(MAX)?;
    keep(
        r,
        p,
        label,
        n.checked_mul(stride).context("Array size overflow")?,
    )
}
fn packed(r: &mut Reader<'_>, regular: usize, compact: usize) -> Result<Vec<u32>> {
    let count = r.u32()?;
    let small = count & 0x80000000 != 0;
    let n = (count & 0x7fffffff) as usize;
    ensure!(n <= MAX, "Packed array too large");
    if small {
        r.u8()?;
    }
    (0..n)
        .map(|_| match if small { compact } else { regular } {
            1 => Ok(r.u8()? as u32),
            2 => Ok(r.u16()? as u32),
            4 => r.u32(),
            _ => unreachable!(),
        })
        .collect()
}
pub(crate) fn png(r: &mut Reader<'_>) -> Result<Vec<u8>> {
    let mut out = r.bytes(8)?.to_vec();
    ensure!(
        out == b"\x89PNG\r\n\x1a\n",
        "Invalid embedded PNG signature"
    );
    loop {
        let header = r.bytes(8)?;
        let size = u32::from_be_bytes(header[..4].try_into()?) as usize;
        ensure!(
            size <= 16 * 1024 * 1024 && out.len() + size + 12 <= 32 * 1024 * 1024,
            "Embedded PNG exceeds limit"
        );
        out.extend(header);
        out.extend(r.bytes(size + 4)?);
        if &header[4..] == b"IEND" {
            ensure!(size == 0, "Malformed PNG end chunk");
            break;
        }
    }
    Ok(out)
}
struct SourceSurface {
    start: usize,
    count: usize,
    plane: u16,
    material: usize,
    texgen: usize,
    flags: u8,
    fan: u32,
    lm_word: u16,
    lm_offset: [f32; 2],
    map_rect: [u8; 4],
}
fn oriented_plane(planes: &[([f32; 3], f32)], index: u16) -> Result<Vec3> {
    let (normal, _) = planes
        .get((index & 0x7fff) as usize)
        .context("Invalid plane reference")?;
    Ok(Vec3::from(axis(*normal)) * if index & 0x8000 != 0 { -1.0 } else { 1.0 })
}
fn face_triangles(points: &[[f32; 3]], indices: &[usize], normal: Vec3) -> Vec<[[f32; 3]; 3]> {
    let mut out = Vec::new();
    for i in 2..indices.len() {
        let mut tri = [
            points[indices[0]],
            points[indices[i - 1]],
            points[indices[i]],
        ]
        .map(axis);
        let n = (Vec3::from(tri[1]) - Vec3::from(tri[0]))
            .cross(Vec3::from(tri[2]) - Vec3::from(tri[0]));
        if n.length_squared() < 1e-12 {
            continue;
        }
        if n.dot(normal) < 0.0 {
            tri.swap(1, 2);
        }
        out.push(tri);
    }
    out
}
fn detail(r: &mut Reader<'_>, p: &mut Provenance) -> Result<Detail> {
    let mut candidate = r.clone();
    let mut provenance = p.clone();
    match detail_variant(&mut candidate, &mut provenance, true) {
        Ok(detail) => {
            *r = candidate;
            *p = provenance;
            Ok(detail)
        }
        Err(extended_error) => detail_variant(r, p, false)
            .with_context(|| format!("TGEA layout also rejected: {extended_error:#}")),
    }
}
fn detail_variant(r: &mut Reader<'_>, p: &mut Provenance, extended: bool) -> Result<Detail> {
    ensure!(
        r.u32()? == 0,
        "Unsupported interior geometry version (expected 0)"
    );
    let _level = r.u32()?;
    let minimum_pixels = r.u32()?;
    for _ in 0..10 {
        r.f32()?;
    }
    let has_alarm = r.u8()? != 0;
    let _light_entries = r.u32()?;
    let normals = items(r, v3)?;
    let planes = items(r, |r| {
        let n = r.u16()? as usize;
        Ok((*normals.get(n).context("Invalid plane normal")?, r.f32()?))
    })?;
    let points = items(r, v3)?;
    skip_array(r, p, "point visibility", 1)?;
    let texgens = items(r, |r| {
        Ok([
            [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
            [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
        ])
    })?;
    skip_array(r, p, "BSP nodes", 6)?;
    skip_array(r, p, "solid leaves", 6)?;
    ensure!(r.u8()? == 1, "Unsupported interior material list");
    let materials = items(r, |r| r.string8())?;
    let windings = items(r, |r| r.count(points.len().saturating_sub(1)))?;
    skip_array(r, p, "winding indices", 8)?;
    skip_array(r, p, "zones", 12)?;
    skip_array(r, p, "zone surfaces", 2)?;
    skip_array(r, p, "zone portals", 2)?;
    skip_array(r, p, "portals", 12)?;
    let surface_count = r.count(MAX)?;
    let read_surfaces = |r: &mut Reader<'_>, extended: bool| -> Result<Vec<SourceSurface>> {
        let mut out = Vec::new();
        for _ in 0..surface_count {
            let start = r.count(MAX)?;
            let count = r.u8()? as usize;
            let plane = r.u16()?;
            let material = r.u16()? as usize;
            let texgen = r.count(MAX)?;
            let flags = r.u8()?;
            let fan = r.u32()?;
            let lm_word = r.u16()?;
            let lm_offset = [r.f32()?, r.f32()?];
            r.u16()?;
            r.u32()?;
            let map_rect: [u8; 4] = r.bytes(4)?.try_into()?;
            if extended {
                r.u8()?;
            }
            ensure!(
                start
                    .checked_add(count)
                    .is_some_and(|n| n <= windings.len())
                    && ((plane & 0x7fff) as usize) < planes.len()
                    && material < materials.len()
                    && texgen < texgens.len(),
                "Invalid interior surface references"
            );
            out.push(SourceSurface {
                start,
                count,
                plane,
                material,
                texgen,
                flags,
                fan,
                lm_word,
                lm_offset,
                map_rect,
            });
        }
        Ok(out)
    };
    let surfaces = read_surfaces(r, extended)?;
    p.lightmap_rects
        .push(surfaces.iter().map(|s| s.map_rect).collect());
    let normal_lm = packed(r, 1, 1)?;
    let alarm_lm = items(r, |r| Ok(r.u8()? as u32))?;
    let nulls = items(r, |r| {
        Ok((r.count(MAX)?, r.u16()?, r.u8()?, r.u8()? as usize))
    })?;
    let lightmaps = items(r, |r| {
        Ok(Lightmap {
            png: png(r)?,
            auxiliary_png: if extended { Some(png(r)?) } else { None },
            keep: r.u8()? != 0,
        })
    })?;
    packed(r, 4, 2)?;
    skip_array(r, p, "animated lights", 16)?;
    skip_array(r, p, "light states", 13)?;
    skip_array(r, p, "light state data", 10)?;
    let size = r.count(64_000_000)?;
    r.u32()?;
    keep(r, p, "light state buffer", size)?;
    skip_array(r, p, "name buffer", 1)?;
    ensure!(
        r.u32()? == 0,
        "Interior embedded subobject adaptation required"
    );
    let hulls = items(r, |r| {
        let start = r.count(MAX)?;
        let count = r.u16()? as usize;
        for _ in 0..6 {
            r.f32()?;
        }
        let surfaces = r.count(MAX)?;
        let surface_count = r.u16()? as usize;
        for _ in 0..4 {
            r.u32()?;
        }
        Ok((start, count, surfaces, surface_count))
    })?;
    skip_array(r, p, "hull emit strings", 1)?;
    let hull_indices = packed(r, 4, 2)?;
    packed(r, 2, 2)?;
    packed(r, 4, 2)?;
    // Compact surface indices use a different null-surface bit.
    let mut look = r.clone();
    let compact = look.u32()? & 0x80000000 != 0;
    let hull_surfaces = packed(r, 4, 2)?;
    let null_mask = if compact { 0x8000 } else { 0x80000000 };
    packed(r, 2, 2)?;
    packed(r, 4, 2)?;
    skip_array(r, p, "polylist strings", 1)?;
    keep(r, p, "coordinate bins", 16 * 16 * 8)?;
    packed(r, 2, 2)?;
    r.u32()?;
    let ambient = r.bytes(4)?.try_into()?;
    let alarm_ambient = r.bytes(4)?.try_into()?;
    for _ in 0..3 {
        ensure!(r.u32()? == 0, "Unknown interior extension");
    }
    match r.u32()? {
        0 => {}
        1 => {
            r.u32()?;
            ensure!(r.u32()? == 0, "Unknown lightmap extension");
        }
        _ => anyhow::bail!("Unknown extended lightmap data"),
    }
    let mut convex_hulls = Vec::new();
    let mut collision_refs = std::collections::BTreeSet::new();
    for (start, count, surface_start, surface_count) in hulls {
        let indices = hull_indices
            .get(start..start + count)
            .context("Hull points out of range")?;
        convex_hulls.push(
            indices
                .iter()
                .map(|i| {
                    Ok(axis(
                        *points
                            .get(*i as usize)
                            .context("Hull point index out of range")?,
                    ))
                })
                .collect::<Result<_>>()?,
        );
        collision_refs.extend(
            hull_surfaces
                .get(surface_start..surface_start + surface_count)
                .context("Hull surfaces out of range")?
                .iter()
                .copied(),
        );
    }
    let mut collision_triangles = Vec::new();
    for reference in collision_refs {
        if reference & null_mask != 0 {
            let (start, plane, _, count) = *nulls
                .get((reference & !null_mask) as usize)
                .context("Null collision surface out of range")?;
            let indices = windings
                .get(start..start + count)
                .context("Null collision winding out of range")?;
            collision_triangles.extend(face_triangles(
                &points,
                indices,
                oriented_plane(&planes, plane)?,
            ));
        } else {
            let surface = surfaces
                .get(reference as usize)
                .context("Collision surface out of range")?;
            ensure!(surface.count <= 32, "Collision fan exceeds bit mask");
            let order = std::iter::once(0)
                .chain((1..surface.count).step_by(2))
                .chain((2..surface.count).step_by(2).rev());
            let indices: Vec<_> = order
                .enumerate()
                .filter(|(i, _)| surface.fan & (1 << i) != 0)
                .map(|(_, i)| windings[surface.start + i])
                .collect();
            collision_triangles.extend(face_triangles(
                &points,
                &indices,
                oriented_plane(&planes, surface.plane)?,
            ));
        }
    }
    let mut native_surfaces = Vec::new();
    for (index, s) in surfaces.into_iter().enumerate() {
        let normal = oriented_plane(&planes, s.plane)?.normalize_or_zero();
        let axes = [[0, 1], [0, 2], [1, 0], [1, 2], [2, 0], [2, 1]];
        let selected = *axes
            .get(((s.lm_word >> 13) & 7) as usize)
            .context("Invalid lightmap axes")?;
        let factors = [
            2.0_f32.powi(-(((s.lm_word >> 6) & 63) as i32)),
            2.0_f32.powi(-((s.lm_word & 63) as i32)),
        ];
        let vertices: Vec<_> = windings[s.start..s.start + s.count]
            .iter()
            .map(|i| {
                let point = points[*i];
                let uv = texgens[s.texgen]
                    .map(|g| g[0] * point[0] + g[1] * point[1] + g[2] * point[2] + g[3]);
                Vertex {
                    position: axis(point),
                    normal: normal.to_array(),
                    uv,
                    lightmap_uv: [
                        point[selected[0]] * factors[0] + s.lm_offset[0],
                        point[selected[1]] * factors[1] + s.lm_offset[1],
                    ],
                }
            })
            .collect();
        let mut triangles = Vec::new();
        for last in 2..s.count {
            let mut tri = [last - 2, last - 1, last];
            let [a, b, c] = tri.map(|i| Vec3::from(vertices[i].position));
            if (b - a).cross(c - a).dot(normal) < 0.0 {
                tri.swap(0, 1);
            }
            triangles.push(tri.map(|i| i as u32));
        }
        let lm = |list: &[u32]| {
            list.get(index)
                .copied()
                .filter(|i| (*i as usize) < lightmaps.len())
                .map(|i| i as usize)
        };
        native_surfaces.push(Surface {
            source_index: index,
            material: s.material,
            flags: s.flags,
            vertices,
            triangles,
            lightmap: lm(&normal_lm),
            alarm_lightmap: lm(&alarm_lm),
        });
    }
    Ok(Detail {
        minimum_pixels,
        materials,
        surfaces: native_surfaces,
        lightmaps,
        collision_triangles,
        convex_hulls,
        ambient,
        alarm_ambient,
        has_alarm,
    })
}

fn vehicle(r: &mut Reader<'_>, p: &mut Provenance) -> Result<VehicleCollision> {
    ensure!(r.u32()? <= 14, "Unknown vehicle collision version");
    let hulls = items(r, |r| {
        let start = r.count(MAX)?;
        let count = r.u16()? as usize;
        for _ in 0..6 {
            r.f32()?;
        }
        let surfaces = r.count(MAX)?;
        let surface_count = r.u16()? as usize;
        for _ in 0..4 {
            r.u32()?;
        }
        Ok((start, count, surfaces, surface_count))
    })?;
    skip_array(r, p, "vehicle hull emit strings", 1)?;
    let hull_indices = packed(r, 4, 2)?;
    packed(r, 2, 2)?;
    packed(r, 4, 2)?;
    let hull_surfaces = packed(r, 4, 2)?;
    packed(r, 2, 2)?;
    packed(r, 4, 2)?;
    skip_array(r, p, "vehicle polylist strings", 1)?;
    let nulls = items(r, |r| Ok((r.count(MAX)?, r.u16()?, r.u8()?, r.count(MAX)?)))?;
    let points = items(r, v3)?;
    let planes = items(r, |r| Ok((v3(r)?, r.f32()?)))?;
    let windings = packed(r, 4, 2)?;
    skip_array(r, p, "vehicle winding indices", 8)?;
    let mut convex_hulls = Vec::new();
    let mut refs = std::collections::BTreeSet::new();
    for (start, count, surface_start, surface_count) in hulls {
        convex_hulls.push(
            hull_indices
                .get(start..start + count)
                .context("Invalid vehicle hull range")?
                .iter()
                .map(|i| {
                    Ok(axis(
                        *points
                            .get(*i as usize)
                            .context("Invalid vehicle hull point")?,
                    ))
                })
                .collect::<Result<_>>()?,
        );
        refs.extend(
            hull_surfaces
                .get(surface_start..surface_start + surface_count)
                .context("Invalid vehicle hull surfaces")?
                .iter()
                .copied(),
        );
    }
    let mut triangles = Vec::new();
    for reference in refs {
        let (start, plane, _, count) = *nulls
            .get((reference & 0x7fffffff) as usize)
            .context("Invalid vehicle null surface")?;
        let indices: Vec<_> = windings
            .get(start..start + count)
            .context("Invalid vehicle winding")?
            .iter()
            .map(|i| *i as usize)
            .collect();
        ensure!(
            indices.iter().all(|i| *i < points.len()),
            "Vehicle point out of range"
        );
        triangles.extend(face_triangles(
            &points,
            &indices,
            oriented_plane(&planes, plane)?,
        ));
    }
    Ok(VehicleCollision {
        convex_hulls,
        triangles,
    })
}

pub fn read(data: &[u8], id: String) -> Result<(Interior, Provenance)> {
    let mut r = Reader::new(data);
    ensure!(r.u32()? == 44, "Unsupported DIF resource version");
    let mut p = Provenance {
        warnings: vec![],
        preserved_sections: vec![],
        lightmap_rects: vec![],
    };
    if r.u8()? != 0 {
        let start = r.position();
        png(&mut r)?;
        p.preserved_sections.push(Section {
            label: "preview PNG".into(),
            offset: start,
            length: r.position() - start,
        });
    }
    let count = r.count(64)?;
    let mut details = Vec::new();
    for i in 0..count {
        details.push(
            detail(&mut r, &mut p)
                .with_context(|| format!("Interior detail {i} at byte {}", r.position()))?,
        );
    }
    let count = r.count(1024)?;
    let mut subobjects = Vec::new();
    for _ in 0..count {
        subobjects.push(detail(&mut r, &mut p)?);
    }
    // Only the detail levels are lit; subobject rectangles are not needed.
    p.lightmap_rects.truncate(details.len());
    for label in ["triggers", "path followers", "force fields", "AI nodes"] {
        ensure!(
            r.u32()? == 0,
            "DIF {label} need native adaptation at byte {}",
            r.position()
        );
    }
    let vehicle_collision = match r.u32()? {
        0 => None,
        1 => Some(vehicle(&mut r, &mut p).context("Vehicle collision")?),
        _ => anyhow::bail!("Unknown vehicle collision block"),
    };
    match r.u32()? {
        0 => {}
        2 => {
            ensure!(r.u32()? == 0, "DIF game entities need native adaptation");
            ensure!(r.u32()? == 0, "Unknown DIF entity extension");
        }
        _ => anyhow::bail!("Unknown DIF extension"),
    };
    r.finish()?;
    p.warnings.push("BSP/zone acceleration and animated-light metadata indexed in separate source archive; native runtime adaptation pending".into());
    let interior = Interior {
        schema_version: 1,
        id,
        details,
        subobjects,
        vehicle_collision,
    };
    interior.validate()?;
    Ok((interior, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        fn w(v: &mut Vec<u8>, n: u32) {
            v.extend(n.to_le_bytes());
        }
        fn h(v: &mut Vec<u8>, n: u16) {
            v.extend(n.to_le_bytes());
        }
        fn floats(v: &mut Vec<u8>, values: &[f32]) {
            for f in values {
                w(v, f.to_bits());
            }
        }
        let mut v = Vec::new();
        w(&mut v, 44);
        v.push(0);
        w(&mut v, 1);
        for x in [0, 0, 64] {
            w(&mut v, x);
        }
        floats(&mut v, &[0.0; 10]);
        v.push(0);
        w(&mut v, 0);
        w(&mut v, 1);
        floats(&mut v, &[0.0, 0.0, 1.0]);
        w(&mut v, 1);
        h(&mut v, 0);
        floats(&mut v, &[0.0]);
        w(&mut v, 4);
        floats(
            &mut v,
            &[
                -1.0, -1.0, 0.0, 1.0, -1.0, 0.0, -1.0, 1.0, 0.0, 1.0, 1.0, 0.0,
            ],
        );
        w(&mut v, 0);
        w(&mut v, 1);
        floats(&mut v, &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        w(&mut v, 0);
        w(&mut v, 0);
        v.push(1);
        w(&mut v, 1);
        v.push(4);
        v.extend(b"test");
        w(&mut v, 4);
        for x in 0..4 {
            w(&mut v, x);
        }
        for _ in 0..5 {
            w(&mut v, 0);
        }
        w(&mut v, 1);
        w(&mut v, 0);
        v.push(4);
        h(&mut v, 0);
        h(&mut v, 0);
        w(&mut v, 0);
        v.push(0);
        w(&mut v, 15);
        h(&mut v, 6 * 64 + 6);
        floats(&mut v, &[0.5, 0.5]);
        h(&mut v, 0);
        w(&mut v, 0);
        v.extend([0; 4]);
        w(&mut v, 1);
        v.push(255);
        w(&mut v, 0);
        w(&mut v, 0);
        w(&mut v, 0);
        // leaf surfaces, lights, states, state data, buffer length/flags, names, subobjects.
        for _ in 0..8 {
            w(&mut v, 0);
        }
        w(&mut v, 1);
        w(&mut v, 0);
        h(&mut v, 4);
        floats(&mut v, &[-1.0, 1.0, -1.0, 1.0, 0.0, 0.0]);
        w(&mut v, 0);
        h(&mut v, 1);
        for _ in 0..4 {
            w(&mut v, 0);
        }
        w(&mut v, 0);
        w(&mut v, 4);
        for x in 0..4 {
            w(&mut v, x);
        }
        w(&mut v, 0);
        w(&mut v, 0);
        w(&mut v, 1);
        w(&mut v, 0);
        for _ in 0..3 {
            w(&mut v, 0);
        }
        v.extend([0; 16 * 16 * 8]);
        w(&mut v, 0);
        w(&mut v, 0);
        v.extend([0; 8]);
        for _ in 0..4 {
            w(&mut v, 0);
        }
        for _ in 0..7 {
            w(&mut v, 0);
        }
        v
    }
    #[test]
    fn tge_single_surface_preserves_uvs_collision_and_rejects_truncation() {
        let bytes = fixture();
        let (native, _) = read(&bytes, "fixture".into()).unwrap();
        let d = &native.details[0];
        let s = &d.surfaces[0];
        assert_eq!(s.vertices[0].position, [-1.0, 0.0, 1.0]);
        assert_eq!(s.vertices[0].uv, [-1.0, -1.0]);
        assert_eq!(s.vertices[0].lightmap_uv, [0.484375; 2]);
        assert_eq!(d.collision_triangles.len(), 2);
        for size in 0..bytes.len() {
            assert!(
                read(&bytes[..size], "fixture".into()).is_err(),
                "Accepted truncation at {size}"
            );
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(read(&trailing, "fixture".into()).is_err());
    }
}
