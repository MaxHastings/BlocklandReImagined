//! Offline mission lighting bake from cache-free originals.
//!
//! A port of the classic engine's sun lighting pass (`SceneLighting`, pinned
//! OpenMBG `sceneGraph/sceneLighting.cc`): the terrain heightfield sweep with
//! its ambient/diffuse lightmap and 5-bit packing, and additive sun light on
//! outside-visible interior lightmaps. The engine measured shadowed lexel area
//! with a shadow-volume BSP; here the same occluders (light-facing
//! outside-visible interior surfaces and terrain) are sampled with rays over
//! each lexel. Everything runs in native Y-up world space except the terrain
//! sweep, which works on the original grid exactly as the engine did.
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use bri_content::{
    interior::Interior,
    scene::{Kind, Node, Scene},
    terrain_field::TerrainField,
};
use glam::{Mat4, Vec3};
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path};

/// `SurfaceOutsideVisible` in the classic interior surface flags.
const OUTSIDE_VISIBLE: u8 = 1 << 4;
/// `gParellelVectorThresh`: surfaces this close to edge-on are unlit.
const PARALLEL: f32 = 0.01;
/// Stock v20 interiors reserve a 10-texel lightmap border around each
/// surface's stored rectangle, and the mission lighting fills it as well.
/// Measured against the reference caches: a border of 10 takes the mean
/// interior error from 90 to 2.8 levels on Kitchen, while 9 or 11 give 12-16.
const LIGHTMAP_BORDER: u32 = 10;
/// Shadow samples per lexel side when a lexel is partly shadowed.
const SAMPLES: usize = 4;
const LIGHTMAP_SIZE: usize = 512;
const BLOCK_SIZE: usize = 256;

/// One `Sun` object: a vector light. Direction points from the sun.
#[derive(Clone, Copy, Debug)]
pub struct Sun {
    /// Native world space (Y up), normalized.
    pub direction: Vec3,
    /// Original Torque world space (Z up), normalized.
    pub source_direction: Vec3,
    pub color: Vec3,
    pub ambient: Vec3,
}

fn floats<const N: usize>(node: &Node, key: &str, default: [f32; N]) -> Result<[f32; N]> {
    let Some(text) = node.properties.get(key) else {
        return Ok(default);
    };
    let values: Vec<f32> = text
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .with_context(|| format!("Sun {key} is not numeric: {text}"))?;
    let mut out = default;
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = value;
    }
    Ok(out)
}

/// `Sun` defaults and `conformLight`: clamped colors. The direction comes from
/// `azimuth`/`elevation` (`Sun::packUpdate` with `MathUtils::getVectorFromAngles`),
/// not the stale `direction` field: Slopes and Tutorial author no direction,
/// and the dark maps' `direction` points straight up. The bake reproduces the
/// reference caches only with the angles.
pub fn suns(scene: &Scene) -> Result<Vec<Sun>> {
    let mut out = Vec::new();
    for node in scene.nodes.iter().filter(|n| matches!(n.kind, Kind::Sun)) {
        let [azimuth] = floats(node, "azimuth", [0.0])?;
        let [elevation] = floats(node, "elevation", [35.0])?;
        // Portable `libm` trigonometry: imported content must be byte-identical
        // on every machine, and C runtime sines can differ in the last bit.
        let d = bri_content::scene::sun_direction(azimuth, elevation, libm::sinf, libm::cosf);
        ensure!(d.is_finite(), "Invalid sun angles");
        let color = floats(node, "color", [0.7, 0.7, 0.7, 1.0])?;
        let ambient = floats(node, "ambient", [0.3, 0.3, 0.3, 1.0])?;
        let clamp = |c: [f32; 4]| Vec3::new(c[0], c[1], c[2]).clamp(Vec3::ZERO, Vec3::ONE);
        out.push(Sun {
            direction: Vec3::new(d.x, d.z, -d.y),
            source_direction: d,
            color: clamp(color),
            ambient: clamp(ambient),
        });
    }
    Ok(out)
}

/// `ColorI = ColorF`: round to the nearest byte.
fn to_byte(v: f32) -> u8 {
    (v.clamped(0.0, 1.0) * 255.0 + 0.5) as u8
}

// --- Occlusion ---------------------------------------------------------------

#[derive(Clone, Copy)]
struct Triangle {
    a: Vec3,
    e1: Vec3,
    e2: Vec3,
    owner: usize,
}
struct Node3 {
    min: Vec3,
    max: Vec3,
    /// Leaf: triangle range; inner: children.
    start: usize,
    count: usize,
    left: usize,
    right: usize,
}
/// A bounding-volume hierarchy answering "does this ray hit anything".
struct Occluders {
    triangles: Vec<Triangle>,
    nodes: Vec<Node3>,
}
impl Occluders {
    fn new(mut triangles: Vec<Triangle>) -> Self {
        let mut nodes = Vec::new();
        if !triangles.is_empty() {
            let count = triangles.len();
            Self::build(&mut triangles, 0, count, &mut nodes);
        }
        Self { triangles, nodes }
    }
    fn bounds(t: &Triangle) -> (Vec3, Vec3) {
        let (b, c) = (t.a + t.e1, t.a + t.e2);
        (t.a.min(b).min(c), t.a.max(b).max(c))
    }
    fn build(tris: &mut [Triangle], start: usize, count: usize, nodes: &mut Vec<Node3>) -> usize {
        let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for t in &tris[start..start + count] {
            let (lo, hi) = Self::bounds(t);
            min = min.min(lo);
            max = max.max(hi);
        }
        let index = nodes.len();
        nodes.push(Node3 {
            min,
            max,
            start,
            count,
            left: 0,
            right: 0,
        });
        if count > 4 {
            let axis = (max - min).max_position();
            let centroid = |t: &Triangle| (t.a + (t.e1 + t.e2) / 3.0)[axis];
            tris[start..start + count].sort_unstable_by(|a, b| centroid(a).total_cmp(&centroid(b)));
            let half = count / 2;
            let left = Self::build(tris, start, half, nodes);
            let right = Self::build(tris, start + half, count - half, nodes);
            nodes[index].left = left;
            nodes[index].right = right;
            nodes[index].count = 0;
        }
        index
    }
    /// Any hit with `t` in `(near, far)`, ignoring triangles owned by `skip`.
    fn blocked(&self, origin: Vec3, dir: Vec3, near: f32, far: f32, skip: Option<usize>) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let inv = dir.recip();
        let mut stack = [0usize; 64];
        let mut depth = 1;
        while depth > 0 {
            depth -= 1;
            let node = &self.nodes[stack[depth]];
            let t0 = (node.min - origin) * inv;
            let t1 = (node.max - origin) * inv;
            let lo = t0.min(t1).max_element().max(near);
            let hi = t0.max(t1).min_element().min(far);
            if lo > hi || lo.is_nan() {
                continue;
            }
            if node.count > 0 {
                for t in &self.triangles[node.start..node.start + node.count] {
                    if skip == Some(t.owner) {
                        continue;
                    }
                    // Möller-Trumbore, either face.
                    let p = dir.cross(t.e2);
                    let det = t.e1.dot(p);
                    if det.abs() < 1e-12 {
                        continue;
                    }
                    let inv_det = 1.0 / det;
                    let s = origin - t.a;
                    let u = s.dot(p) * inv_det;
                    if !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    let q = s.cross(t.e1);
                    let v = dir.dot(q) * inv_det;
                    if v < 0.0 || u + v > 1.0 {
                        continue;
                    }
                    let d = t.e2.dot(q) * inv_det;
                    if d > near && d < far {
                        return true;
                    }
                }
            } else if depth + 2 <= stack.len() {
                stack[depth] = node.left;
                stack[depth + 1] = node.right;
                depth += 2;
            }
        }
        false
    }
}

/// Light-facing outside-visible surfaces of one detail level, in world space.
/// These are exactly the polygons the engine inserted into its shadow volume.
fn facing_triangles(
    interior: &Interior,
    detail: usize,
    placement: Mat4,
    light: Vec3,
    owner: usize,
) -> Vec<Triangle> {
    let normals = placement.inverse().transpose();
    let mut out = Vec::new();
    for surface in &interior.details[detail].surfaces {
        if surface.flags & OUTSIDE_VISIBLE == 0 || surface.vertices.is_empty() {
            continue;
        }
        let normal = normals
            .transform_vector3(Vec3::from(surface.vertices[0].normal))
            .normalize_or_zero();
        if normal.dot(light) > -PARALLEL {
            continue;
        }
        for tri in &surface.triangles {
            let [a, b, c] = tri.map(|i| {
                placement.transform_point3(Vec3::from(surface.vertices[i as usize].position))
            });
            out.push(Triangle {
                a,
                e1: b - a,
                e2: c - a,
                owner,
            });
        }
    }
    out
}

/// Lit fraction of a lexel, from its four corners and `SAMPLES`² interior
/// points when the corners disagree. `point(s, t)` maps lexel-relative
/// coordinates in `[0, 1]²` to world space.
fn lit_fraction(point: impl Fn(f32, f32) -> Vec3, lit: impl Fn(Vec3) -> bool) -> f32 {
    let probes = [
        (0.5, 0.5),
        (0.02, 0.02),
        (0.98, 0.02),
        (0.98, 0.98),
        (0.02, 0.98),
    ];
    let first = lit(point(probes[0].0, probes[0].1));
    if probes[1..].iter().all(|&(s, t)| lit(point(s, t)) == first) {
        return if first { 1.0 } else { 0.0 };
    }
    let mut count = 0;
    for j in 0..SAMPLES {
        for i in 0..SAMPLES {
            let s = (i as f32 + 0.5) / SAMPLES as f32;
            let t = (j as f32 + 0.5) / SAMPLES as f32;
            count += usize::from(lit(point(s, t)));
        }
    }
    count as f32 / (SAMPLES * SAMPLES) as f32
}

// --- Terrain -----------------------------------------------------------------

/// `TerrainBlock::getNormal` at a grid vertex, in original grid space.
fn grid_normal(field: &TerrainField, x: i32, y: i32) -> Vec3 {
    let h = |x: i32, y: i32| field.sample(x & 255, y & 255);
    let (x, y) = (x & 255, y & 255);
    let size = field.spacing;
    let (bl, br, tl, tr) = (h(x, y), h(x + 1, y), h(x, y + 1), h(x + 1, y + 1));
    // At the square's own corner (xp = yp = 0).
    let normal = if (x ^ y) & 1 == 0 {
        Vec3::new(tl - tr, bl - tl, size)
    } else {
        Vec3::new(bl - br, bl - tl, size)
    };
    normal.normalize()
}

/// `TerrainProxy::lightVector` for generate level 0: accumulates one sun into
/// `lightmap` (512², original grid order). `interior_lit(lx, ly)` is the lit
/// fraction of a lexel against interior shadows.
fn light_terrain(
    field: &TerrainField,
    sun: &Sun,
    lightmap: &mut [Vec3],
    interior_lit: &dyn Fn(usize, usize) -> f32,
) {
    let light = sun.source_direction;
    if light.x == 0.0 && light.y == 0.0 {
        return;
    }
    let get = |p: [i32; 2]| field.sample(p[0] & 255, p[1] & 255);
    let add = |a: [i32; 2], b: [i32; 2]| [a[0] + b[0], a[1] + b[1]];
    let sub = |a: [i32; 2], b: [i32; 2]| [a[0] - b[0], a[1] - b[1]];
    let generate_dim = LIGHTMAP_SIZE;
    let generate_mask = generate_dim - 1;
    let square = field.spacing;
    let step_size = square / (generate_dim / BLOCK_SIZE) as f32;
    let (mut z_step, mut frac, col_step, row_step, block_first, lm_first);
    if light.x.abs() >= light.y.abs() {
        if light.x > 0.0 {
            z_step = light.z / light.x;
            frac = light.y / light.x;
            (col_step, row_step, block_first, lm_first) = ([1, 0], [0, 1], [0, 0], [0, 0]);
        } else {
            z_step = -light.z / light.x;
            frac = -light.y / light.x;
            (col_step, row_step, block_first, lm_first) =
                ([-1, 0], [0, 1], [255, 0], [LIGHTMAP_SIZE as i32 - 1, 0]);
        }
    } else if light.y > 0.0 {
        z_step = light.z / light.y;
        frac = light.x / light.y;
        (col_step, row_step, block_first, lm_first) = ([0, 1], [1, 0], [0, 0], [0, 0]);
    } else {
        z_step = -light.z / light.y;
        frac = -light.x / light.y;
        (col_step, row_step, block_first, lm_first) =
            ([0, -1], [1, 0], [0, 255], [0, LIGHTMAP_SIZE as i32 - 1]);
    }
    z_step *= step_size;
    let mut frac_step: i32 = -1;
    if frac < 0.0 {
        frac_step = 1;
        frac = -frac;
    }
    let one_minus_frac = 1.0 - frac;
    // generate level 0: two samples per square, one per lexel.
    let block_shift = 1u32;
    let block_step = 1usize << block_shift;
    let block_mask = block_step - 1;
    let height_step = 1.0 / block_step as f32;
    let mut heights = vec![0.0f32; generate_dim];
    let mut next_heights = vec![0.0f32; generate_dim];
    let mut cur = [0.0f32; BLOCK_SIZE];
    let mut next = [0.0f32; BLOCK_SIZE];
    let mut row_z = [0.0f32; BLOCK_SIZE];
    let mut next_row_z = [0.0f32; BLOCK_SIZE];
    let mut col_z = [0.0f32; BLOCK_SIZE];
    let fill = |at: [i32; 2], out: &mut [f32; BLOCK_SIZE]| {
        let mut p = at;
        for v in out.iter_mut() {
            *v = get(p);
            p = add(p, row_step);
        }
    };
    let steps = |cur: &[f32; BLOCK_SIZE],
                 next: &[f32; BLOCK_SIZE],
                 row_z: &mut [f32; BLOCK_SIZE],
                 next_row_z: &mut [f32; BLOCK_SIZE],
                 col_z: &mut [f32; BLOCK_SIZE]| {
        for i in 0..BLOCK_SIZE {
            let j = (i + 1) & (BLOCK_SIZE - 1);
            row_z[i] = (cur[j] - cur[i]) * height_step;
            next_row_z[i] = (next[j] - next[i]) * height_step;
            col_z[i] = (next[i] - cur[i]) * height_step;
        }
    };
    // The interpolated height of a sample inside the current square pair.
    #[allow(clippy::too_many_arguments)]
    fn sample_height(
        walk: [i32; 2],
        xmask: usize,
        ymask: usize,
        block_step: usize,
        bi: usize,
        cur: &[f32; BLOCK_SIZE],
        next: &[f32; BLOCK_SIZE],
        row_z: &[f32; BLOCK_SIZE],
        next_row_z: &[f32; BLOCK_SIZE],
        col_z: &[f32; BLOCK_SIZE],
    ) -> (f32, f32, f32) {
        let binext = (bi + 1) & (BLOCK_SIZE - 1);
        let (xm, ym) = (xmask as f32, ymask as f32);
        if (walk[0] ^ walk[1]) & 1 != 0 {
            let xsub = block_step - xmask;
            if xsub > ymask {
                let (xs, ys) = (col_z[bi], row_z[bi]);
                (cur[bi] + xm * xs + ym * ys, xs, ys)
            } else {
                let (xs, ys) = (-col_z[binext], next_row_z[bi]);
                (next[bi] + xsub as f32 * xs + ym * ys, xs, ys)
            }
        } else if xmask > ymask {
            let (xs, ys) = (col_z[bi], next_row_z[bi]);
            (cur[bi] + xm * xs + ym * ys, xs, ys)
        } else {
            let (xs, ys) = (col_z[binext], row_z[bi]);
            (cur[bi] + xm * xs + ym * ys, xs, ys)
        }
    }
    // Initial shadow-height run.
    fill(block_first, &mut cur);
    fill(add(block_first, col_step), &mut next);
    steps(&cur, &next, &mut row_z, &mut next_row_z, &mut col_z);
    for (i, h) in heights.iter_mut().enumerate() {
        let bi = i >> block_shift;
        *h = cur[bi] + (i & block_mask) as f32 * row_z[bi];
    }
    let mut bp = block_first;
    for x in 1..generate_dim {
        let xmask = x & block_mask;
        if xmask == 0 {
            std::mem::swap(&mut cur, &mut next);
            bp = add(bp, col_step);
            fill(bp, &mut next);
            steps(&cur, &next, &mut row_z, &mut next_row_z, &mut col_z);
        }
        let mut walk = sub(bp, row_step);
        for y in 0..generate_dim {
            let ymask = y & block_mask;
            if ymask == 0 {
                walk = add(walk, row_step);
            }
            let bi = y >> block_shift;
            let (height, _, _) = sample_height(
                walk,
                xmask,
                ymask,
                block_step,
                bi,
                &cur,
                &next,
                &row_z,
                &next_row_z,
                &col_z,
            );
            let shadow = heights[y] * one_minus_frac
                + heights[(y as i32 + frac_step) as usize & generate_mask] * frac
                + z_step;
            next_heights[y] = height.max(shadow);
        }
        std::mem::swap(&mut heights, &mut next_heights);
    }
    // Normal interpolation weights from the four square corners.
    let corners = [[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut table = vec![0.0f32; block_step * block_step * 4];
    let step = 1.0 / block_step as f32;
    let mut idx = 0;
    for x in 0..block_step {
        for y in 0..block_step {
            let pos = [x as f32 * step, y as f32 * step];
            for c in corners {
                let d = ((pos[0] - c[0]).powi(2) + (pos[1] - c[1]).powi(2)).sqrt();
                table[idx] = 1.0 - d.min(1.0);
                idx += 1;
            }
        }
    }
    let mut normals = [Vec3::ZERO; BLOCK_SIZE];
    let mut next_normals = [Vec3::ZERO; BLOCK_SIZE];
    let normal_row = |at: [i32; 2], out: &mut [Vec3; BLOCK_SIZE]| {
        let mut p = at;
        for n in out.iter_mut() {
            *n = grid_normal(field, p[0], p[1]);
            p = add(p, row_step);
        }
    };
    fill(block_first, &mut cur);
    normal_row(block_first, &mut next_normals);
    std::mem::swap(&mut cur, &mut next);
    let mut bp = sub(block_first, col_step);
    for x in 0..generate_dim {
        let xmask = x & block_mask;
        if xmask == 0 {
            bp = add(bp, col_step);
            std::mem::swap(&mut normals, &mut next_normals);
            normal_row(add(bp, col_step), &mut next_normals);
            std::mem::swap(&mut cur, &mut next);
            fill(add(bp, col_step), &mut next);
            steps(&cur, &next, &mut row_z, &mut next_row_z, &mut col_z);
        }
        let mut walk = sub(bp, row_step);
        for y in 0..generate_dim {
            let ymask = y & block_mask;
            if ymask == 0 {
                walk = add(walk, row_step);
            }
            let bi = y >> block_shift;
            let binext = (bi + 1) & (BLOCK_SIZE - 1);
            let (height, _, _) = sample_height(
                walk,
                xmask,
                ymask,
                block_step,
                bi,
                &cur,
                &next,
                &row_z,
                &next_row_z,
                &col_z,
            );
            let shadow = heights[y] * one_minus_frac
                + heights[(y as i32 + frac_step) as usize & generate_mask] * frac
                + z_step;
            let lm = add(
                add(lm_first, [col_step[0] * x as i32, col_step[1] * x as i32]),
                [row_step[0] * y as i32, row_step[1] * y as i32],
            );
            let (lx, ly) = (lm[0] as usize, lm[1] as usize);
            let color = &mut lightmap[lx + ly * LIGHTMAP_SIZE];
            if height >= shadow {
                let i = (xmask + (ymask << block_shift)) << 2;
                let normal = (normals[bi] * table[i]
                    + normals[binext] * table[i + 1]
                    + next_normals[binext] * table[i + 2]
                    + next_normals[bi] * table[i + 3])
                    .normalize();
                next_heights[y] = height;
                let scale = -normal.dot(light);
                if scale > 0.0 {
                    *color += sun.ambient + sun.color * scale * interior_lit(lx, ly);
                } else {
                    *color += sun.ambient;
                }
            } else {
                next_heights[y] = shadow;
                *color += sun.ambient;
            }
        }
        std::mem::swap(&mut heights, &mut next_heights);
    }
}

/// The classic terrain blender maps five-bit light through a six-bit alpha table.
fn expand_five_bit(v: u8) -> u8 {
    ((u16::from(v) * 2 * 255 + 32) / 63) as u8
}
/// Classic `convertColor` 5-bit packing, then the blender's expansion into
/// ordinary eight-bit modulation for the native renderer.
fn terrain_png(lightmap: &[Vec3]) -> Result<Vec<u8>> {
    let mut image = image::RgbImage::new(LIGHTMAP_SIZE as u32, LIGHTMAP_SIZE as u32);
    for (pixel, color) in image.pixels_mut().zip(lightmap) {
        let c = color.clamp(Vec3::ZERO, Vec3::ONE);
        let five = |v: f32| (v * 31.0 + 0.5) as u8;
        pixel.0 = [five(c.x), five(c.y), five(c.z)].map(expand_five_bit);
    }
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}

// --- Interiors ---------------------------------------------------------------

/// One placed interior with its original lightmap rectangles.
pub struct PlacedInterior {
    pub node: usize,
    pub interior: Interior,
    pub placement: Mat4,
    /// Per detail, per source surface: `[offsetX, offsetY, sizeX, sizeY]`.
    pub rects: Vec<Vec<[u8; 4]>>,
}

/// Native interior-local position from original lightmap texel coordinates on
/// a surface's plane. `texgen` maps local Torque axes to normalized lightmap
/// coordinates: `u = p[axes.0] * scale.0 + offset.0`.
struct LexelFrame {
    axes: [usize; 2],
    scale: [f32; 2],
    offset: [f32; 2],
    normal: Vec3,
    d: f32,
    size: [f32; 2],
}
impl LexelFrame {
    fn local(&self, texel: [f32; 2]) -> Vec3 {
        let mut p = [0.0f32; 3];
        for k in 0..2 {
            p[self.axes[k]] = (texel[k] / self.size[k] - self.offset[k]) / self.scale[k];
        }
        let third = 3 - self.axes[0] - self.axes[1];
        let n = self.normal.to_array();
        p[third] =
            -(n[self.axes[0]] * p[self.axes[0]] + n[self.axes[1]] * p[self.axes[1]] + self.d)
                / n[third];
        // Torque (x, y, z) -> native (x, z, -y).
        Vec3::new(p[0], p[2], -p[1])
    }
}
/// Recover a surface's lightmap texgen from its native vertices: each texgen
/// is one scaled Torque axis plus an offset, so two distinct vertices fix it.
fn lexel_frame(surface: &bri_content::interior::Surface, size: [f32; 2]) -> Option<LexelFrame> {
    let torque = |v: [f32; 3]| [v[0], -v[2], v[1]];
    let points: Vec<[f32; 3]> = surface
        .vertices
        .iter()
        .map(|v| torque(v.position))
        .collect();
    let n = Vec3::from(torque(surface.vertices.first()?.normal));
    let d = -n.dot(Vec3::from(points[0]));
    let mut axes = [0usize; 2];
    let mut scale = [0.0f32; 2];
    let mut offset = [0.0f32; 2];
    for k in 0..2 {
        let uv = |i: usize| surface.vertices[i].lightmap_uv[k];
        let mut best = None;
        for axis in 0..3 {
            // Find two vertices differing on this axis; check linearity on all.
            let Some(j) =
                (1..points.len()).find(|&j| (points[j][axis] - points[0][axis]).abs() > 1e-4)
            else {
                continue;
            };
            let s = (uv(j) - uv(0)) / (points[j][axis] - points[0][axis]);
            if s.abs() < 1e-9 {
                continue;
            }
            let o = uv(0) - points[0][axis] * s;
            let fits = (0..points.len()).all(|i| (points[i][axis] * s + o - uv(i)).abs() < 1e-4);
            if fits {
                best = Some((axis, s, o));
                break;
            }
        }
        let (axis, s, o) = best?;
        axes[k] = axis;
        scale[k] = s;
        offset[k] = o;
    }
    let third = 3 - axes[0] - axes[1];
    (axes[0] != axes[1] && n.to_array()[third].abs() > 1e-6).then_some(LexelFrame {
        axes,
        scale,
        offset,
        normal: n,
        d,
        size,
    })
}

/// Load a mission's lit objects and bake it. `assets` maps asset IDs to native
/// files in `content`; `source` maps an asset ID to its original virtual path,
/// read from `root` for the interiors' lightmap rectangles.
pub fn bake_scene(
    root: &Path,
    scene: &Scene,
    assets: &std::collections::BTreeMap<String, String>,
    content: &Path,
    source: &dyn Fn(&str) -> Result<String>,
    terrains: &[bri_content::terrain_field::TerrainInstance],
    output: &Path,
) -> Result<serde_json::Value> {
    let mut interiors = Vec::new();
    let mut fields = Vec::new();
    for (index, node) in scene.nodes.iter().enumerate() {
        match node.kind {
            Kind::Interior => {
                let id = node.asset.as_ref().context("Interior lacks asset")?;
                let file = assets.get(id).context("Interior asset missing")?;
                let interior: Interior =
                    serde_json::from_slice(&std::fs::read(content.join(file))?)?;
                let path = source(id)?;
                let original = crate::environment::read_original(root, &path)?
                    .with_context(|| format!("Missing original interior {path}"))?;
                let (_, provenance) = crate::interior::read(&original, id.clone())?;
                ensure!(
                    provenance.lightmap_rects.len() == interior.details.len(),
                    "Interior detail count differs from its original {path}"
                );
                interiors.push(PlacedInterior {
                    node: index,
                    interior,
                    placement: Mat4::from_cols_array(&node.transform),
                    rects: provenance.lightmap_rects,
                });
            }
            Kind::Terrain => {
                let instance = terrains
                    .iter()
                    .find(|t| t.node == index)
                    .context("Terrain placement lacks an instance")?;
                let id = node.asset.as_ref().context("Terrain lacks asset")?;
                let file = assets.get(id).context("Terrain asset missing")?;
                let terrain = serde_json::from_slice(&std::fs::read(content.join(file))?)?;
                fields.push(TerrainField::new(terrain, instance)?);
            }
            _ => {}
        }
    }
    // Every reference mission has at most one terrain block, as in the engine.
    ensure!(
        fields.len() <= 1,
        "Multiple terrain blocks are not supported"
    );
    bake(scene, &interiors, fields.first(), output)
}

/// Bake one mission. Returns the lighting record for `bundle.json`, writing
/// lightmaps into `output`. `terrain` is the mission's single terrain field.
pub fn bake(
    scene: &Scene,
    interiors: &[PlacedInterior],
    terrain: Option<&TerrainField>,
    output: &Path,
) -> Result<serde_json::Value> {
    let suns = suns(scene)?;
    let export = |png: &[u8]| -> Result<String> {
        let name = format!("{:x}.png", Sha256::digest(png));
        std::fs::write(output.join(&name), png)?;
        Ok(name)
    };
    if suns.is_empty() {
        ensure!(
            terrain.is_none(),
            "Mission {} has terrain but no Sun to light it",
            scene.id
        );
        return Ok(
            serde_json::json!({"status":"no_sun","terrain":[],"interiors":[],
            "note":"No Sun object: the classic engine leaves original interior lightmaps unlit by the mission"}),
        );
    }
    // Shadow-detail (lowest) occluders of every interior, per sun.
    let mut terrain_maps = Vec::new();
    let mut interior_maps = Vec::new();
    let world_bounds = interiors
        .iter()
        .flat_map(|p| {
            p.interior
                .details
                .iter()
                .flat_map(|d| &d.surfaces)
                .flat_map(|s| &s.vertices)
                .map(|v| p.placement.transform_point3(Vec3::from(v.position)))
        })
        .fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(p), hi.max(p)),
        );
    let reach = (world_bounds.1 - world_bounds.0).length().max(1.0) * 4.0 + 10_000.0;
    let shadow_sets: Vec<Occluders> = suns
        .iter()
        .map(|sun| {
            Occluders::new(
                interiors
                    .iter()
                    .enumerate()
                    .flat_map(|(k, p)| {
                        facing_triangles(
                            &p.interior,
                            p.interior.details.len() - 1,
                            p.placement,
                            sun.direction,
                            k,
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    if let Some(field) = terrain {
        let mut lightmap = vec![Vec3::ZERO; LIGHTMAP_SIZE * LIGHTMAP_SIZE];
        for (sun, occluders) in suns.iter().zip(&shadow_sets) {
            let toward = -sun.direction;
            let lexel = field.spacing * BLOCK_SIZE as f32 / LIGHTMAP_SIZE as f32;
            let interior_lit = |lx: usize, ly: usize| -> f32 {
                if occluders.nodes.is_empty() {
                    return 1.0;
                }
                lit_fraction(
                    |s, t| {
                        let gx = (lx as f32 + s) * lexel / field.spacing;
                        let gy = (ly as f32 + t) * lexel / field.spacing;
                        let wx = field.origin.x + gx * field.spacing;
                        let wz = field.origin.z - gy * field.spacing;
                        let y = field.height(wx, wz).unwrap_or(field.origin.y);
                        Vec3::new(wx, y, wz)
                    },
                    |p| !occluders.blocked(p + toward * 0.01, toward, 0.0, reach, None),
                )
            };
            light_terrain(field, sun, &mut lightmap, &interior_lit);
        }
        terrain_maps.push(
            serde_json::json!({"node":field.node,"file":export(&terrain_png(&lightmap)?)?,
            "native_encoding":"rgb8_modulation","source":"baked"}),
        );
    }
    // Interiors: the engine clears to the base lightmap for each light, so the
    // last sun defines the result.
    let (sun, others) = (suns.last().unwrap(), shadow_sets.last().unwrap());
    let toward = -sun.direction;
    for (k, placed) in interiors.iter().enumerate() {
        let normals = placed.placement.inverse().transpose();
        for (level, detail) in placed.interior.details.iter().enumerate() {
            let own = Occluders::new(facing_triangles(
                &placed.interior,
                level,
                placed.placement,
                sun.direction,
                k,
            ));
            let lit = |p: Vec3| {
                !own.blocked(p, toward, 1e-3, reach, None)
                    && !others.blocked(p, toward, 1e-3, reach, Some(k))
                    && terrain.is_none_or(|f| f.cast_ray(p, toward, reach).is_none())
            };
            let mut images: Vec<Option<image::RgbImage>> = vec![None; detail.lightmaps.len()];
            let rects = placed
                .rects
                .get(level)
                .context("Interior lightmap rectangles missing")?;
            for surface in &detail.surfaces {
                if surface.flags & OUTSIDE_VISIBLE == 0 || surface.vertices.is_empty() {
                    continue;
                }
                let Some(slot) = surface.lightmap else {
                    continue;
                };
                let rect = *rects
                    .get(surface.source_index)
                    .context("Surface lightmap rectangle out of range")?;
                let image = match &mut images[slot] {
                    Some(image) => image,
                    empty => empty
                        .insert(image::load_from_memory(&detail.lightmaps[slot].png)?.to_rgb8()),
                };
                let (w, h) = image.dimensions();
                let normal = normals
                    .transform_vector3(Vec3::from(surface.vertices[0].normal))
                    .normalize_or_zero();
                let dot = normal.dot(toward);
                let fill = |image: &mut image::RgbImage, x: u32, y: u32, color: Vec3| {
                    if x < w && y < h {
                        let add = [to_byte(color.x), to_byte(color.y), to_byte(color.z)];
                        let px = image.get_pixel_mut(x, y);
                        for (channel, add) in px.0.iter_mut().zip(add) {
                            *channel = channel.saturating_add(add);
                        }
                    }
                };
                let border = LIGHTMAP_BORDER;
                let [ox, oy, sx, sy] = rect.map(u32::from);
                let (ox, oy) = (ox.saturating_sub(border), oy.saturating_sub(border));
                let (sx, sy) = (
                    (sx + 2 * border).min(w.saturating_sub(ox)),
                    (sy + 2 * border).min(h.saturating_sub(oy)),
                );
                if -dot > -PARALLEL {
                    // Facing away from the sun: ambient only (`addInterior`).
                    for y in oy..oy + sy {
                        for x in ox..ox + sx {
                            fill(image, x, y, sun.ambient);
                        }
                    }
                    continue;
                }
                let Some(frame) = lexel_frame(surface, [w as f32, h as f32]) else {
                    // Degenerate texgen: light without shadows.
                    for y in oy..oy + sy {
                        for x in ox..ox + sx {
                            fill(image, x, y, sun.color * dot + sun.ambient);
                        }
                    }
                    continue;
                };
                let lift = normal * 0.005;
                for y in oy..oy + sy {
                    for x in ox..ox + sx {
                        let fraction = lit_fraction(
                            |s, t| {
                                placed
                                    .placement
                                    .transform_point3(frame.local([x as f32 + s, y as f32 + t]))
                                    + lift
                            },
                            lit,
                        );
                        fill(image, x, y, sun.color * dot * fraction + sun.ambient);
                    }
                }
            }
            for (slot, image) in images.into_iter().enumerate() {
                let Some(image) = image else { continue };
                let mut out = Cursor::new(Vec::new());
                image.write_to(&mut out, image::ImageFormat::Png)?;
                interior_maps.push(
                    serde_json::json!({"node":placed.node,"detail":level,"slot":slot,
                    "file":export(&out.into_inner())?}),
                );
            }
        }
    }
    Ok(serde_json::json!({
        "status":"baked",
        "method":"classic sun lighting port: terrain heightfield sweep and outside-visible interior lightmaps; shadow coverage ray-sampled",
        "suns":suns.iter().map(|s| serde_json::json!({"direction":s.source_direction.to_array(),"color":s.color.to_array(),"ambient":s.ambient.to_array()})).collect::<Vec<_>>(),
        "terrain":terrain_maps,
        "interiors":interior_maps,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rays_hit_only_within_range_and_skip_owner() {
        let tri = Triangle {
            a: Vec3::new(-1.0, 1.0, -1.0),
            e1: Vec3::new(2.0, 0.0, 0.0),
            e2: Vec3::new(0.0, 0.0, 2.0),
            owner: 3,
        };
        let o = Occluders::new(vec![tri; 9]);
        assert!(o.blocked(Vec3::new(-0.5, 0.0, -0.5), Vec3::Y, 1e-3, 10.0, None));
        assert!(!o.blocked(Vec3::new(-0.5, 0.0, -0.5), Vec3::Y, 1e-3, 0.5, None));
        assert!(!o.blocked(Vec3::new(-0.5, 0.0, -0.5), -Vec3::Y, 1e-3, 10.0, None));
        assert!(!o.blocked(Vec3::new(-0.5, 0.0, -0.5), Vec3::Y, 1e-3, 10.0, Some(3)));
        assert!(!o.blocked(Vec3::new(5.0, 0.0, 0.2), Vec3::Y, 1e-3, 10.0, None));
    }

    #[test]
    fn partial_coverage_is_sampled_and_uniform_lexels_are_exact() {
        let half = lit_fraction(|s, t| Vec3::new(s, 0.0, t), |p| p.x < 0.5);
        assert!((half - 0.5).abs() < 1e-6);
        assert_eq!(lit_fraction(|s, t| Vec3::new(s, 0.0, t), |_| true), 1.0);
        assert_eq!(lit_fraction(|s, t| Vec3::new(s, 0.0, t), |_| false), 0.0);
    }

    #[test]
    fn five_bit_terrain_light_expands_through_the_alpha_table() {
        assert_eq!([0, 16, 31].map(expand_five_bit), [0, 130, 251]);
    }

    #[test]
    fn sun_angles_follow_get_vector_from_angles() {
        let mut scene: Scene = serde_json::from_value(serde_json::json!({
            "schema_version":1,"id":"v20/test.mis","name":"t","pending_scripts":[],
            "nodes":[{"name":"","parent":null,"kind":"sun","asset":null,"transform":Mat4::IDENTITY.to_cols_array(),
                "properties":{"azimuth":"0","elevation":"35","direction":"0.57735 0.57735 -0.57735"}}]}))
        .unwrap();
        let sun = suns(&scene).unwrap()[0];
        let expected = Vec3::new(0.0, -35f32.to_radians().cos(), -35f32.to_radians().sin());
        assert!((sun.source_direction - expected).length() < 1e-5);
        // Torque (x, y, z) -> native (x, z, -y).
        assert!((sun.direction - Vec3::new(0.0, expected.z, -expected.y)).length() < 1e-5);
        scene.nodes[0]
            .properties
            .insert("azimuth".into(), "90".into());
        assert!(suns(&scene).unwrap()[0].source_direction.x < -0.8);
    }

    #[test]
    fn colors_round_like_the_classic_engine() {
        assert_eq!(to_byte(0.0), 0);
        assert_eq!(to_byte(0.5), 128);
        assert_eq!(to_byte(1.2), 255);
    }
}
