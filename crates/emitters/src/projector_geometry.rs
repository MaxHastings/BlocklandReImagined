//! The receiving geometry of a projector, built as the original builds it (see
//! `specs/projectors.md` sections 3.4 to 3.6): BSP polygon records, terrain sector triangle
//! lists and static-mesh triangles with per-vertex fade, ready for a projected-surface pass.
#![allow(clippy::manual_clamp)]
use crate::rotation::safe_normal;
use glam::{Mat4, Vec2, Vec3, Vec4};

/// One vertex of a receiving polygon or triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReceiverVertex {
    pub position: Vec3,
    /// The surface's own texture coordinate (mask and base picture).
    pub uv: Vec2,
    /// The share of the projection at this vertex.
    pub fade: f32,
}

/// `UModel::AttachProjector` (Engine.dll 0x179f40) steps 2, 4 and 5: the node's polygon
/// vertices in reverse order, the mask coordinates from the surface's mapping, and the
/// clip against the projector's six planes when `bClipBSP` is set. The clip is not a
/// standard one: it replaces the vertex at an edge crossing with the crossing point, so a
/// corner is lost on the entering side. It is reproduced as read.
pub struct BspMapping {
    /// `Points[surf.pBase]`, `Vectors[vTextureU]`, `Vectors[vTextureV]`.
    pub base: Vec3,
    pub u_axis: Vec3,
    pub v_axis: Vec3,
    /// The mask material's size in texels.
    pub mask_size: Option<(f32, f32)>,
}

pub fn bsp_record(
    polygon: &[Vec3],
    mapping: &BspMapping,
    clip_planes: Option<&[Vec4; 6]>,
) -> Vec<ReceiverVertex> {
    let mut verts: Vec<(Vec3, Vec2)> = polygon
        .iter()
        .rev()
        .map(|&v| {
            let uv = match mapping.mask_size {
                Some((w, h)) => Vec2::new(
                    (v - mapping.base).dot(mapping.u_axis) / w,
                    (v - mapping.base).dot(mapping.v_axis) / h,
                ),
                None => Vec2::ZERO,
            };
            (v, uv)
        })
        .collect();
    if let Some(planes) = clip_planes {
        for plane in planes {
            let n = plane.truncate();
            let mut dist: Vec<f32> = verts.iter().map(|(v, _)| v.dot(n) - plane.w).collect();
            let count = verts.len();
            for i in 0..count {
                let prev = (i + count - 1) % count;
                if dist[i] * dist[prev] < 0.0 {
                    let t = dist[prev] / (dist[prev] - dist[i]);
                    let (pp, puv) = verts[prev];
                    let (cp, cuv) = verts[i];
                    verts[i] = (pp + t * (cp - pp), cuv + t * (cuv - puv));
                    dist[i] = 0.0;
                }
            }
            let kept: Vec<(Vec3, Vec2)> = verts
                .iter()
                .zip(&dist)
                .filter(|(_, d)| **d >= 0.0)
                .map(|(v, _)| *v)
                .collect();
            verts = kept;
            if verts.is_empty() {
                break;
            }
        }
    }
    verts
        .into_iter()
        .map(|(position, uv)| ReceiverVertex {
            position,
            uv,
            fade: 1.0,
        })
        .collect()
}

/// A fan of triangles for a convex polygon of `n` vertices.
pub fn fan_indices(n: usize, base: u32) -> Vec<u32> {
    (1..n.saturating_sub(1) as u32)
        .flat_map(|k| [base, base + k, base + k + 1])
        .collect()
}

/// Which planes' outsides each vertex lies on (bit k set when `plane_k . v - W_k <= 0`),
/// as `0x23f010` computes them for terrain.
pub fn plane_outcode(v: Vec3, planes: &[Vec4; 6]) -> u8 {
    let mut code = 0;
    for (k, p) in planes.iter().enumerate() {
        if p.truncate().dot(v) - p.w <= 0.0 {
            code |= 1 << k;
        }
    }
    code
}

/// What the terrain sector builder needs (`UTerrainSector::AttachProjector`, 0x24dfe0).
pub struct TerrainInput<'a> {
    /// The terrain's vertex grid, row-major with `stride` vertices per row.
    pub positions: &'a [Vec3],
    pub normals: &'a [Vec3],
    pub stride: usize,
    /// The sector's first tile and tile counts.
    pub origin: (usize, usize),
    pub tiles: (usize, usize),
    /// Quad visibility bitmap and diagonal choice.
    pub quad_visible: &'a dyn Fn(usize, usize) -> bool,
    pub diagonal: &'a dyn Fn(usize, usize) -> bool,
    pub planes: [Vec4; 6],
    pub projector_location: Vec3,
    /// The projection direction `F`.
    pub forward: Vec3,
    pub max_trace_distance: f32,
    pub project_on_backfaces: bool,
    /// The render info's gradient flag (`bGradient` with a picture).
    pub gradient: bool,
}

/// Terrain triangles for one sector: the vertices of the sector's tile range, each with its
/// alpha, and the triangle indices that survive the plane test (a triangle whose three
/// outcodes share a bit is dropped). Indices refer to the returned vertices. The split of
/// a quad into triangles follows the diagonal bitmap (inferred: false cuts v00-v11).
pub fn terrain_receivers(t: &TerrainInput<'_>) -> (Vec<ReceiverVertex>, Vec<u32>) {
    let (tx, ty) = t.tiles;
    let (ox, oy) = t.origin;
    let cols = tx + 1;
    let mut vertices = Vec::with_capacity(cols * (ty + 1));
    let mut codes = Vec::with_capacity(cols * (ty + 1));
    for y in 0..=ty {
        for x in 0..=tx {
            let i = (oy + y) * t.stride + ox + x;
            let p = t.positions[i];
            let nf = (-t.forward.dot(t.normals[i])).max(0.0);
            let g = if t.gradient {
                (1.0 - (p - t.projector_location).dot(t.forward) * (1.0 / t.max_trace_distance))
                    .max(0.0)
                    .min(1.0)
            } else {
                1.0
            };
            let fade = if t.project_on_backfaces { g } else { g * nf };
            vertices.push(ReceiverVertex {
                position: p,
                uv: Vec2::ZERO,
                fade,
            });
            codes.push(plane_outcode(p, &t.planes));
        }
    }
    let mut indices = Vec::new();
    let mut tri = |a: usize, b: usize, c: usize| {
        if codes[a] & codes[b] & codes[c] == 0 {
            indices.extend([a as u32, b as u32, c as u32]);
        }
    };
    for y in 0..ty {
        for x in 0..tx {
            if !(t.quad_visible)(ox + x, oy + y) {
                continue;
            }
            let v00 = y * cols + x;
            let v10 = v00 + 1;
            let v01 = v00 + cols;
            let v11 = v01 + 1;
            if (t.diagonal)(ox + x, oy + y) {
                tri(v00, v10, v01);
                tri(v10, v11, v01);
            } else {
                tri(v00, v10, v11);
                tri(v00, v11, v01);
            }
        }
    }
    (vertices, indices)
}

/// The six outcode bits of the static-mesh builder (`0x1bd930`), for a point already in
/// projector space (perspective: `x`, `y` and `z` as the matrix gives them).
pub fn static_mesh_outcode(p: Vec3, near: f32, far: f32) -> u8 {
    let mut code = 0;
    if p.x < -0.001 {
        code |= 1;
    }
    if p.z + 0.001 < p.x {
        code |= 2;
    }
    if p.y < -0.001 {
        code |= 4;
    }
    if p.z + 0.001 < p.y {
        code |= 8;
    }
    if p.z < near - 0.001 {
        code |= 16;
    }
    if p.z > far + 0.001 {
        code |= 32;
    }
    code
}

/// What the static-mesh builder (0x1c1ba0) needs, for a perspective projector.
pub struct StaticMeshInput<'a> {
    pub positions: &'a [Vec3],
    pub normals: &'a [Vec3],
    pub uvs: &'a [Vec2],
    pub triangles: &'a [[u32; 3]],
    /// Mesh space to world.
    pub to_world: Mat4,
    /// The projector's world-to-picture matrix (perspective form).
    pub projector: Mat4,
    /// `d` and `d + MaxTraceDistance`: the picture plane's distance from the eye and the far.
    pub near: f32,
    pub far: f32,
    /// The projector's direction, world space.
    pub forward: Vec3,
    pub project_on_backfaces: bool,
}

/// Static-mesh receivers: every vertex with its alpha `max(0, N . Dir)` (1 when backfaces are
/// allowed), the triangles whose outcodes share no bit. The optional clip against the six
/// planes (`bClipStaticMesh`) is left to the caller.
pub fn static_mesh_receivers(m: &StaticMeshInput<'_>) -> (Vec<ReceiverVertex>, Vec<u32>) {
    // Dir: the negated projector direction taken into mesh space.
    let inverse = m.to_world.inverse();
    let dir = safe_normal(inverse.transform_vector3(-m.forward));
    let mut codes = Vec::with_capacity(m.positions.len());
    let vertices: Vec<ReceiverVertex> = m
        .positions
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let world = m.to_world.transform_point3(p);
            let in_projector = m.projector * world.extend(1.0);
            codes.push(static_mesh_outcode(in_projector.truncate(), m.near, m.far));
            let alpha = if m.project_on_backfaces {
                1.0
            } else {
                m.normals[i].dot(dir).max(0.0)
            };
            ReceiverVertex {
                position: world,
                uv: m.uvs.get(i).copied().unwrap_or(Vec2::ZERO),
                fade: alpha,
            }
        })
        .collect();
    let mut indices = Vec::new();
    for tri in m.triangles {
        let [a, b, c] = tri.map(|i| i as usize);
        if codes[a] & codes[b] & codes[c] == 0 {
            indices.extend(tri);
        }
    }
    (vertices, indices)
}
