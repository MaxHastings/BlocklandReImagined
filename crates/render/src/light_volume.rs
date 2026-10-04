//! Baked light for vertex-lit objects (players, items, vehicles, bricks)
//! inside lightmapped interiors.
//!
//! Interior lights such as the Bedroom lamp exist only in the baked
//! lightmaps; the mission has no light object. Vertex-lit meshes see only the
//! sun, its ambient and dynamic point lights, so on a map whose sun and ambient
//! are black (BedroomDark, KitchenDark) a player standing in the lamp's light
//! renders as a black silhouette. The classic engine family answered this by
//! lighting a shape with the lightmap colour of the interior surface under it
//! (`SceneObject::getLightingAmbientColor`, a 100 unit ray down).
//!
//! This volume keeps that rule and widens it: each cell holds the brighter of
//! the lightmap under it and the mean lightmap over every direction, so a
//! player inside the lit lamp shade picks up the shade's light even though the
//! bars under their feet are unlit. The shader lights vertex-lit surfaces with
//! the brighter of their sun lighting and this volume, so maps with a bright
//! sun keep their look and nothing gets darker.
use crate::scene::{AlphaMode, MaterialKind, SceneData, SceneImage};
use bri_console::Clamp;
use glam::Vec3;

/// Directions per cell. A Fibonacci sphere; enough to find a lamp shade.
const RAYS: usize = 24;
/// Rays a baked cell casts: every direction and the one down.
pub const RAYS_PER_CELL: u64 = RAYS as u64 + 1;
/// A cell whose rays mostly leave through back faces is inside a wall.
const SOLID_BACKFACES: f32 = 0.3;
/// How far below a cell the lightmap under it is looked for (the classic
/// engine's `cRayLength`).
const FLOOR_RAY: f32 = 100.0;
/// Names the bake and the stored layout; change it whenever either changes.
const FORMAT: &[u8; 8] = b"BRILV\0\0\x01";
/// Cells between the first baked corners on each axis; failing blocks halve
/// down to 2.
const BLOCK: u32 = 8;
/// Largest corner difference, in 1/255, a block may interpolate across.
const TOLERANCE: u8 = 12;

/// A baked grid of RGBA8 texels: RGB premultiplied by A, A the share of the
/// cell outside solid geometry, so filtering never pulls light toward black
/// walls. Cell `(x, y, z)` is centred at `origin + (index + 0.5) * cell`.
#[derive(Clone, Debug, PartialEq)]
pub struct LightVolume {
    pub origin: [f32; 3],
    pub cell: f32,
    pub dims: [u32; 3],
    pub texels: Vec<[u8; 4]>,
    /// Rays the bake cast: its work, the same on every machine and build.
    pub rays: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct Triangle {
    pub(crate) a: Vec3,
    pub(crate) e1: Vec3,
    pub(crate) e2: Vec3,
    /// Faces toward the side the authored normals point to; zero when the
    /// material is double sided.
    front: Vec3,
    uv: [[f32; 2]; 3],
    image: usize,
}

#[derive(Clone)]
struct Node {
    min: Vec3,
    max: Vec3,
    start: usize,
    count: usize,
    right: usize,
    /// Split axis; the left child holds the lower triangles on it.
    axis: usize,
}

#[derive(Clone)]
pub(crate) struct Bvh {
    triangles: Vec<Triangle>,
    nodes: Vec<Node>,
}

struct Hit {
    t: f32,
    triangle: usize,
    u: f32,
    v: f32,
}

impl Triangle {
    /// An occluder only (no lightmap lookup).
    pub(crate) fn occluder(a: Vec3, b: Vec3, c: Vec3) -> Self {
        Self {
            a,
            e1: b - a,
            e2: c - a,
            front: Vec3::ZERO,
            uv: [[0.0; 2]; 3],
            image: 0,
        }
    }
}

impl Bvh {
    /// Whether anything lies strictly between `origin` and `origin +
    /// direction * far` (`direction` normalized).
    pub(crate) fn blocked(&self, origin: Vec3, direction: Vec3, far: f32) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let inverse = direction.recip();
        let mut stack = [0usize; 64];
        let mut depth = 1;
        while depth > 0 {
            depth -= 1;
            let index = stack[depth];
            let node = &self.nodes[index];
            let t0 = (node.min - origin) * inverse;
            let t1 = (node.max - origin) * inverse;
            let enter = t0.min(t1).max_element().max(0.0);
            let exit = t0.max(t1).min_element().min(far);
            if enter > exit || enter.is_nan() {
                continue;
            }
            if node.count == 0 {
                stack[depth] = node.right;
                stack[depth + 1] = index + 1;
                depth += 2;
                continue;
            }
            for tri in &self.triangles[node.start..node.start + node.count] {
                let p = direction.cross(tri.e2);
                let det = tri.e1.dot(p);
                if det.abs() < 1e-9 {
                    continue;
                }
                let s = origin - tri.a;
                let u = s.dot(p) / det;
                if !(0.0..=1.0).contains(&u) {
                    continue;
                }
                let q = s.cross(tri.e1);
                let v = direction.dot(q) / det;
                if v < 0.0 || u + v > 1.0 {
                    continue;
                }
                let t = tri.e2.dot(q) / det;
                if t > 1e-4 && t < far {
                    return true;
                }
            }
        }
        false
    }
    pub(crate) fn bounds_of_all(&self) -> Option<(Vec3, Vec3)> {
        self.nodes.first().map(|n| (n.min, n.max))
    }
    pub(crate) fn new(mut triangles: Vec<Triangle>) -> Self {
        let mut nodes = Vec::new();
        let len = triangles.len();
        Self::build(&mut triangles, &mut nodes, 0, len);
        Self { triangles, nodes }
    }
    fn bounds(triangle: &Triangle) -> (Vec3, Vec3) {
        let b = triangle.a + triangle.e1;
        let c = triangle.a + triangle.e2;
        (triangle.a.min(b).min(c), triangle.a.max(b).max(c))
    }
    fn build(triangles: &mut [Triangle], nodes: &mut Vec<Node>, start: usize, end: usize) -> usize {
        if start == end {
            return 0;
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for triangle in &triangles[start..end] {
            let (lo, hi) = Self::bounds(triangle);
            min = min.min(lo);
            max = max.max(hi);
        }
        let index = nodes.len();
        nodes.push(Node {
            min,
            max,
            start,
            count: end - start,
            right: 0,
            axis: 0,
        });
        if end - start <= 4 {
            return index;
        }
        let axis = (max - min).max_position();
        let centre = |t: &Triangle| {
            let (lo, hi) = Self::bounds(t);
            (lo + hi)[axis]
        };
        let mid = (start + end) / 2;
        triangles[start..end]
            .select_nth_unstable_by(mid - start, |a, b| centre(a).total_cmp(&centre(b)));
        Self::build(triangles, nodes, start, mid);
        let right = Self::build(triangles, nodes, mid, end);
        nodes[index].count = 0;
        nodes[index].right = right;
        nodes[index].axis = axis;
        index
    }
    fn cast(&self, origin: Vec3, direction: Vec3, limit: f32) -> Option<Hit> {
        let inverse = direction.recip();
        let mut best: Option<Hit> = None;
        // Median splits of up to 2^32 triangles stay far below this depth.
        let mut stack = [0usize; 64];
        let mut depth = 1;
        while depth > 0 {
            depth -= 1;
            let index = stack[depth];
            let node = &self.nodes[index];
            let far = best.as_ref().map_or(limit, |h| h.t);
            let t0 = (node.min - origin) * inverse;
            let t1 = (node.max - origin) * inverse;
            let enter = t0.min(t1).max_element().max(0.0);
            let exit = t0.max(t1).min_element().min(far);
            if enter > exit {
                continue;
            }
            if node.count == 0 {
                // Nearer child last, so it is searched first and its hit
                // prunes the farther one.
                let (near, far) = if direction[node.axis] < 0.0 {
                    (node.right, index + 1)
                } else {
                    (index + 1, node.right)
                };
                stack[depth] = far;
                stack[depth + 1] = near;
                depth += 2;
                continue;
            }
            for i in node.start..node.start + node.count {
                let tri = &self.triangles[i];
                let p = direction.cross(tri.e2);
                let det = tri.e1.dot(p);
                if det.abs() < 1e-9 {
                    continue;
                }
                let s = origin - tri.a;
                let u = s.dot(p) / det;
                if !(0.0..=1.0).contains(&u) {
                    continue;
                }
                let q = s.cross(tri.e1);
                let v = direction.dot(q) / det;
                if v < 0.0 || u + v > 1.0 {
                    continue;
                }
                let t = tri.e2.dot(q) / det;
                if t > 1e-4 && t < best.as_ref().map_or(limit, |h| h.t) {
                    best = Some(Hit {
                        t,
                        triangle: i,
                        u,
                        v,
                    });
                }
            }
        }
        best
    }
    /// Whether any triangle's bounds overlap the box.
    fn touches(&self, lo: Vec3, hi: Vec3) -> bool {
        let mut stack = [0usize; 64];
        let mut depth = 1;
        while depth > 0 {
            depth -= 1;
            let index = stack[depth];
            let node = &self.nodes[index];
            if node.min.cmpgt(hi).any() || node.max.cmplt(lo).any() {
                continue;
            }
            if node.count == 0 {
                stack[depth] = index + 1;
                stack[depth + 1] = node.right;
                depth += 2;
                continue;
            }
            let hit = self.triangles[node.start..node.start + node.count]
                .iter()
                .any(|t| {
                    let (a, b) = Self::bounds(t);
                    !(a.cmpgt(hi).any() || b.cmplt(lo).any())
                });
            if hit {
                return true;
            }
        }
        false
    }
}

/// Runs `f` over `0..count` on every core, handing out small batches so
/// costly regions (near geometry) do not leave threads idle.
pub(crate) fn parallel<T: Send>(count: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    const BATCH: usize = 256;
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut parts: Vec<(usize, Vec<T>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let start = next.fetch_add(BATCH, Ordering::Relaxed);
                        if start >= count {
                            break done;
                        }
                        let end = (start + BATCH).min(count);
                        done.push((start, (start..end).map(&f).collect::<Vec<_>>()));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("light volume worker panicked"))
            .collect()
    });
    parts.sort_by_key(|(start, _)| *start);
    parts.into_iter().flat_map(|(_, part)| part).collect()
}

/// Bilinear, clamped, like the shader's `clamped_exact` lightmap sampler.
fn sample(image: &SceneImage, uv: [f32; 2]) -> Vec3 {
    let (w, h) = (image.width as i64, image.height as i64);
    let x = uv[0] * w as f32 - 0.5;
    let y = uv[1] * h as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let texel = |x: i64, y: i64| {
        let i = (y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize * 4;
        Vec3::new(
            image.rgba[i] as f32,
            image.rgba[i + 1] as f32,
            image.rgba[i + 2] as f32,
        ) / 255.0
    };
    let (x0, y0) = (x0 as i64, y0 as i64);
    let top = texel(x0, y0).lerp(texel(x0 + 1, y0), fx);
    let bottom = texel(x0, y0 + 1).lerp(texel(x0 + 1, y0 + 1), fx);
    top.lerp(bottom, fy)
}

fn directions() -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    (0..RAYS)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / RAYS as f32;
            let r = (1.0 - y * y).sqrt();
            let a = golden * i as f32;
            Vec3::new(r * a.cos(), y, r * a.sin())
        })
        .collect()
}

/// What a bake reads from a map scene: its lightmapped triangles and their
/// lightmaps, so the bake can run on another thread after the scene moves on.
#[derive(Clone)]
pub struct Baker {
    bvh: Bvh,
    images: Vec<SceneImage>,
    /// The scene image each of `images` came from.
    sources: Vec<usize>,
}

impl LightVolume {
    /// `Baker::new` then `Baker::bake`.
    pub fn bake(scene: &SceneData, min_cell: f32, max_cells: usize) -> Option<Self> {
        Baker::new(scene).map(|baker| baker.bake(min_cell, max_cells))
    }
}

impl Baker {
    /// The lightmapped (`Surface`) opaque and masked geometry of a map scene.
    /// None when it has none, or its batches are malformed.
    pub fn new(scene: &SceneData) -> Option<Self> {
        let mut triangles = Vec::new();
        let mut images = Vec::new();
        let mut remap = std::collections::BTreeMap::new();
        let mut sources = Vec::new();
        for batch in &scene.batches {
            let material = scene.materials.get(batch.material)?;
            if material.kind != MaterialKind::Surface
                || matches!(material.alpha, AlphaMode::Blend | AlphaMode::Additive)
            {
                continue;
            }
            let source = material.images[8];
            let image = *remap.entry(source).or_insert(images.len());
            if image == images.len() {
                sources.push(source);
                images.push(scene.images.get(source)?.clone());
            }

            let range = batch.indices.start as usize..batch.indices.end as usize;
            for corner in scene.indices.get(range)?.chunks_exact(3) {
                let v = [0, 1, 2].map(|k| scene.vertices.get(corner[k] as usize));
                let [Some(a), Some(b), Some(c)] = v else {
                    return None;
                };
                let v = [a, b, c];
                let [a, b, c] = v.map(|v| Vec3::from(v.position));
                let (e1, e2) = (b - a, c - a);
                let geometric = e1.cross(e2);
                if geometric.length_squared() < 1e-12 {
                    continue;
                }
                let authored: Vec3 = v.iter().map(|v| Vec3::from(v.normal)).sum();
                let front = if material.double_sided {
                    Vec3::ZERO
                } else if geometric.dot(authored) < 0.0 {
                    -geometric.normalize()
                } else {
                    geometric.normalize()
                };
                triangles.push(Triangle {
                    a,
                    e1,
                    e2,
                    front,
                    uv: v.map(|v| v.lightmap_uv),
                    image,
                });
            }
        }
        if triangles.is_empty() {
            return None;
        }
        Some(Self {
            bvh: Bvh::new(triangles),
            images,
            sources,
        })
    }

    /// This input with some lightmaps (by scene image index) replaced by
    /// images of the same layout: the light the map fit leaves unexplained
    /// (`crate::map_lighting`), gathered the same way.
    pub fn replace(mut self, replaced: &std::collections::BTreeMap<usize, SceneImage>) -> Self {
        for (k, source) in self.sources.iter().enumerate() {
            if let Some(image) = replaced.get(source) {
                self.images[k] = image.clone();
            }
        }
        self
    }

    /// Names what `bake(min_cell, max_cells)` would produce: the bake
    /// version, its settings, every lightmapped triangle and its lightmap.
    /// Equal keys bake equal volumes, so a stored volume can stand in.
    pub fn key(&self, min_cell: f32, max_cells: usize) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(FORMAT);
        hash.update(min_cell.to_le_bytes());
        hash.update((max_cells as u64).to_le_bytes());
        for t in &self.bvh.triangles {
            for v in [t.a, t.e1, t.e2, t.front] {
                for c in v.to_array() {
                    hash.update(c.to_le_bytes());
                }
            }
            for c in t.uv.as_flattened() {
                hash.update(c.to_le_bytes());
            }
            hash.update((t.image as u64).to_le_bytes());
        }
        for image in &self.images {
            hash.update(image.width.to_le_bytes());
            hash.update(image.height.to_le_bytes());
            hash.update(&image.rgba);
        }
        hash.finalize().into()
    }

    /// Cells are at least `min_cell` units and at most `max_cells` in total.
    pub fn bake(self, min_cell: f32, max_cells: usize) -> LightVolume {
        self.bake_blocks(min_cell, max_cells, BLOCK)
    }

    /// `bake` casting rays from every cell: the reference the interpolated
    /// cells are measured against.
    pub fn bake_every_cell(self, min_cell: f32, max_cells: usize) -> LightVolume {
        self.bake_blocks(min_cell, max_cells, 1)
    }

    fn bake_blocks(self, min_cell: f32, max_cells: usize, block: u32) -> LightVolume {
        let Self { bvh, images, .. } = self;
        let (min, max) = (bvh.nodes[0].min, bvh.nodes[0].max);
        let extent = (max - min).max(Vec3::splat(min_cell));
        let mut cell = min_cell.max(1e-3);
        let dims = |cell: f32| (extent / cell).ceil().as_uvec3() + 2;
        while dims(cell).as_u64vec3().element_product() > max_cells as u64 {
            cell *= 1.1;
        }
        let dims = dims(cell);
        let origin = min - Vec3::splat(cell);
        let directions = directions();
        let count = dims.as_u64vec3().element_product() as usize;
        let rays = std::sync::atomic::AtomicU64::new(0);
        let lightmap = |hit: &Hit| {
            let tri = &bvh.triangles[hit.triangle];
            let w = 1.0 - hit.u - hit.v;
            let uv = [0, 1].map(|k| tri.uv[0][k] * w + tri.uv[1][k] * hit.u + tri.uv[2][k] * hit.v);
            sample(&images[tri.image], uv)
        };
        let bake_cell = |index: usize| -> [u8; 4] {
            let x = index as u32 % dims.x;
            let y = index as u32 / dims.x % dims.y;
            let z = index as u32 / (dims.x * dims.y);
            let centre = origin + (glam::UVec3::new(x, y, z).as_vec3() + 0.5) * cell;
            let mut sum = Vec3::ZERO;
            let mut backfaces = 0;
            let reach = extent.length() + cell * 2.0;
            rays.fetch_add(RAYS as u64, std::sync::atomic::Ordering::Relaxed);
            for &direction in &directions {
                if let Some(hit) = bvh.cast(centre, direction, reach) {
                    if bvh.triangles[hit.triangle].front.dot(direction) > 0.0 {
                        backfaces += 1;
                    } else {
                        sum += lightmap(&hit);
                    }
                }
            }
            let open = 1.0 - backfaces as f32 / RAYS as f32;
            if open < 1.0 - SOLID_BACKFACES {
                return [0; 4];
            }
            let light = sum / (RAYS - backfaces) as f32;
            let byte = |v: f32| (v.clamped(0.0, 1.0) * 255.0 + 0.5) as u8;
            let light = light.clamp(Vec3::ZERO, Vec3::ONE);
            [byte(light.x), byte(light.y), byte(light.z), 255]
        };
        // Bake every `block`-th cell on each axis, then fill the cells between
        // by interpolation where that is close enough: the block touches no
        // geometry (light over open air changes smoothly, and every change
        // in it comes from surfaces the corners also see) and its open
        // corners agree within TOLERANCE. A block of air inside a wall or
        // outside the map interpolates to no light. Blocks that fail are
        // split in half and tried again; what is left is baked cell by cell.
        let flat = |p: [u32; 3]| ((p[2] * dims.y + p[1]) * dims.x + p[0]) as usize;
        let mut known: Vec<Option<[u8; 4]>> = vec![None; count];
        // Blocks still to try at this stride, as their low corner cells.
        let mut stride = block;
        let mut pending: Vec<[u32; 3]> = Vec::new();
        if stride > 1 {
            for z in (0..dims.z - 1).step_by(stride as usize) {
                for y in (0..dims.y - 1).step_by(stride as usize) {
                    for x in (0..dims.x - 1).step_by(stride as usize) {
                        pending.push([x, y, z]);
                    }
                }
            }
        }
        let high =
            |low: [u32; 3], stride: u32| [0, 1, 2].map(|a| (low[a] + stride).min(dims[a] - 1));
        while stride > 1 && !pending.is_empty() {
            let corners_of = |low: [u32; 3]| {
                let hi = high(low, stride);
                (0..8usize)
                    .map(move |k| [0, 1, 2].map(|a| if k >> a & 1 == 1 { hi[a] } else { low[a] }))
            };
            let mut wanted: Vec<usize> = pending
                .iter()
                .flat_map(|&low| corners_of(low))
                .map(flat)
                .filter(|&i| known[i].is_none())
                .collect();
            wanted.sort_unstable();
            wanted.dedup();
            for (index, texel) in wanted
                .iter()
                .zip(parallel(wanted.len(), |i| bake_cell(wanted[i])))
            {
                known[*index] = Some(texel);
            }
            let known_ref = &known;
            let filled = parallel(pending.len(), |i| {
                let low = pending[i];
                let hi = high(low, stride);
                let values: Vec<[u8; 4]> = corners_of(low)
                    .map(|p| known_ref[flat(p)].expect("corner baked"))
                    .collect();
                // Cells inside walls hold no light; the rest must agree.
                let open = values.iter().filter(|v| v[3] == 255);
                for channel in 0..3 {
                    let lo = open.clone().map(|v| v[channel]).min().unwrap_or(0);
                    let hi = open.clone().map(|v| v[channel]).max().unwrap_or(0);
                    if hi - lo > TOLERANCE {
                        return None;
                    }
                }
                let lo_cell = Vec3::from(low.map(|v| v as f32));
                let hi_cell = Vec3::from(hi.map(|v| v as f32));
                // Cell centres, widened by a cell so geometry between this
                // block's edge cells and the next block is caught too.
                if bvh.touches(
                    origin + (lo_cell - 0.5) * cell,
                    origin + (hi_cell + 1.5) * cell,
                ) {
                    return None;
                }
                let mut cells = Vec::new();
                for z in low[2]..=hi[2] {
                    for y in low[1]..=hi[1] {
                        for x in low[0]..=hi[0] {
                            let p = [x, y, z];
                            let t = [0, 1, 2]
                                .map(|a| (p[a] - low[a]) as f32 / (hi[a] - low[a]).max(1) as f32);
                            // Premultiplied, like the GPU's own filtering.
                            let mut out = [0.0f32; 4];
                            for (k, v) in values.iter().enumerate() {
                                let w: f32 = (0..3)
                                    .map(|a| if k >> a & 1 == 1 { t[a] } else { 1.0 - t[a] })
                                    .product();
                                for (o, v) in out.iter_mut().zip(v) {
                                    *o += w * f32::from(*v);
                                }
                            }
                            cells.push((flat(p), out.map(|v| (v + 0.5) as u8)));
                        }
                    }
                }
                Some(cells)
            });
            let mut next = Vec::new();
            let half = stride / 2;
            for (low, cells) in pending.iter().zip(filled) {
                match cells {
                    Some(cells) => {
                        for (index, texel) in cells {
                            known[index].get_or_insert(texel);
                        }
                    }
                    None if half > 1 => {
                        let hi = high(*low, stride);
                        for z in (low[2]..hi[2]).step_by(half as usize) {
                            for y in (low[1]..hi[1]).step_by(half as usize) {
                                for x in (low[0]..hi[0]).step_by(half as usize) {
                                    next.push([x, y, z]);
                                }
                            }
                        }
                    }
                    None => {}
                }
            }
            pending = next;
            stride = half;
        }
        // The light under a cell comes from one ray and can change sharply
        // (a small lit block below open air), so every cell casts it.
        let texels = parallel(count, |index| {
            let mut texel = known[index].unwrap_or_else(|| bake_cell(index));
            if texel[3] == 0 {
                return texel;
            }
            let p = [
                index as u32 % dims.x,
                index as u32 / dims.x % dims.y,
                index as u32 / (dims.x * dims.y),
            ];
            let centre = origin + (glam::UVec3::from(p).as_vec3() + 0.5) * cell;
            rays.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if let Some(hit) = bvh.cast(centre, Vec3::NEG_Y, FLOOR_RAY)
                && bvh.triangles[hit.triangle].front.y > 0.5
            {
                let floor = lightmap(&hit).clamp(Vec3::ZERO, Vec3::ONE) * 255.0 + 0.5;
                // Scaled by the cell's open share, as its light is stored.
                let open = f32::from(texel[3]) / 255.0;
                for (c, v) in texel.iter_mut().zip(floor.to_array()) {
                    *c = (*c).max((v * open) as u8);
                }
            }
            texel
        });
        LightVolume {
            origin: origin.to_array(),
            cell,
            dims: dims.to_array(),
            texels,
            rays: rays.into_inner(),
        }
    }
}

impl LightVolume {
    /// A stored volume: `FORMAT`, origin, cell, dimensions, then texels.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = FORMAT.to_vec();
        for v in self.origin.iter().chain([&self.cell]) {
            out.extend(v.to_le_bytes());
        }
        for d in self.dims {
            out.extend(d.to_le_bytes());
        }
        out.extend(self.texels.as_flattened());
        out
    }

    /// None unless `bytes` is exactly a volume `to_bytes` wrote. Its `rays`
    /// are 0: loading one casts none.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let rest = bytes.strip_prefix(FORMAT.as_slice())?;
        let (head, texels) = rest.split_at_checked(28)?;
        let word = |i: usize| <[u8; 4]>::try_from(&head[i * 4..i * 4 + 4]).ok();
        let float = |i| word(i).map(f32::from_le_bytes);
        let origin = [float(0)?, float(1)?, float(2)?];
        let cell = float(3)?;
        let dims = [4, 5, 6].map(|i| word(i).map(u32::from_le_bytes));
        let dims = [dims[0]?, dims[1]?, dims[2]?];
        let count = dims
            .iter()
            .try_fold(1usize, |n, d| n.checked_mul(*d as usize))?;
        if texels.len() != count.checked_mul(4)?
            || !(cell.is_finite() && cell > 0.0)
            || !origin.iter().all(|v| v.is_finite())
            || dims.contains(&0)
        {
            return None;
        }
        Some(Self {
            origin,
            cell,
            dims,
            texels: texels
                .chunks_exact(4)
                .map(|t| [t[0], t[1], t[2], t[3]])
                .collect(),
            rays: 0,
        })
    }

    /// The light the shader adds for a surface at `position` facing
    /// `normal`, before combining it with the sun: a CPU mirror of
    /// `baked_surroundings` in scene.wgsl, with trilinear filtering.
    pub fn light(&self, position: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
        let dims = glam::UVec3::from(self.dims).as_vec3();
        let t = (Vec3::from(position) - Vec3::from(self.origin)) / self.cell - 0.5;
        if t.cmplt(Vec3::splat(-0.5)).any() || t.cmpgt(dims - 0.5).any() {
            return [0.0; 3];
        }
        let t = t.clamp(Vec3::ZERO, dims - 1.0);
        let base = t.floor().as_uvec3();
        let f = t - t.floor();
        let mut acc = [0.0f32; 4];
        for corner in 0..8u32 {
            let offset = glam::UVec3::new(corner & 1, corner >> 1 & 1, corner >> 2 & 1);
            let p = (base + offset).min(glam::UVec3::from(self.dims) - 1);
            let weight = [0, 1, 2]
                .map(|k| if offset[k] == 1 { f[k] } else { 1.0 - f[k] })
                .iter()
                .product::<f32>();
            let i = ((p.z * self.dims[1] + p.y) * self.dims[0] + p.x) as usize;
            for (a, v) in acc.iter_mut().zip(self.texels[i]) {
                *a += weight * v as f32 / 255.0;
            }
        }
        shade(acc, normal)
    }
}

/// Toward the brighter side for the volume's form shading: up and over the
/// shoulder, like the classic engine's fixed 0.3 directional share.
pub const FORM_DIRECTION: [f32; 3] = [-0.57735, 0.57735, 0.57735];

fn shade(sample: [f32; 4], normal: [f32; 3]) -> [f32; 3] {
    if sample[3] < 0.01 {
        return [0.0; 3];
    }
    let n = Vec3::from(normal).normalize_or_zero();
    let form = 0.7 + 0.3 * n.dot(Vec3::from(FORM_DIRECTION)).max(0.0);
    let strength = (sample[3] * 2.0).min(1.0) / sample[3];
    [0, 1, 2].map(|k| sample[k] * strength * form)
}
