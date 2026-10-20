//! `AProjector::CalcMatrix` (Engine.dll 0x1be270) and the projector rules around it (see
//! `specs/projectors.md`): the world-to-picture mapping, the six frustum planes, the
//! depth-fade matrix, the surface-angle gate for BSP, the blend operations and the
//! lifetime rule of a decal's render info.
use crate::rotation::{Coords, Rotator, safe_normal};
use glam::{Mat4, Vec3, Vec4};

/// `EProjectorBlending`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectorBlending {
    None,
    Modulate,
    AlphaBlend,
    Add,
}

impl ProjectorBlending {
    pub fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::Modulate,
            2 => Self::AlphaBlend,
            3 => Self::Add,
            _ => Self::None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProjectorProps {
    pub location: Vec3,
    pub rotation: Rotator,
    pub draw_scale: f32,
    /// The projected picture's size in texels.
    pub texture_size: (u32, u32),
    /// Degrees; 0 is orthographic.
    pub fov: i32,
    pub max_trace_distance: i32,
    /// `PHYS_Rotating`: the near corners keep their raw diagonal length.
    pub rotating: bool,
    /// The gradient picture's height in texels, when `bGradient` is set and it has one.
    pub gradient_height: Option<u32>,
}

/// What `CalcMatrix` computes.
#[derive(Clone, Debug)]
pub struct ProjectorVolume {
    /// World position to `(s, t, depth, 1)` (orthographic) or the pre-divide form
    /// `(0.5 x/t + 0.5 z, 0.5 y/t + 0.5 z, z, 1)` in the eye frame (perspective, where the
    /// picture coordinate is the first two components divided by the third).
    pub matrix: Mat4,
    pub perspective: bool,
    /// Inside when `n . p - w >= 0`; near, four sides, far.
    pub planes: [Vec4; 6],
    pub vertices: [Vec3; 8],
    pub box_min: Vec3,
    pub box_max: Vec3,
    /// Output `y` is the gradient's `v` coordinate.
    pub gradient: Option<Mat4>,
    /// The projection direction.
    pub forward: Vec3,
}

/// A matrix from its rows (the original's row-vector matrices applied to column vectors).
fn rows(r0: Vec4, r1: Vec4, r2: Vec4, r3: Vec4) -> Mat4 {
    Mat4::from_cols(r0, r1, r2, r3).transpose()
}

fn plane(normal: Vec3, w: f32) -> Vec4 {
    normal.extend(w)
}

/// `FPlane(A, B, C)`: normal `SafeNormal((B - A) x (C - A))`, `w = A . normal`.
fn plane_from_points(a: Vec3, b: Vec3, c: Vec3) -> Vec4 {
    let n = safe_normal((b - a).cross(c - a));
    plane(n, a.dot(n))
}

/// `CalcMatrix`. `None` when the projector has no picture.
pub fn calc_matrix(p: &ProjectorProps) -> Option<ProjectorVolume> {
    if p.texture_size.0 == 0 || p.texture_size.1 == 0 {
        return None;
    }
    let axes = Coords::UNIT.mul_rotator(p.rotation);
    let (f, r, u) = (axes.x, axes.y, axes.z);
    let l = -r;
    let wd = p.texture_size.0 as f32 * p.draw_scale;
    let ht = p.texture_size.1 as f32 * p.draw_scale;
    let radius = (0.25 * ht * ht + 0.25 * wd * wd).sqrt();
    let loc = p.location;
    let dirs = [l + u, u - l, -u - l, l - u];
    let mut vertices = [Vec3::ZERO; 8];
    for (i, d) in dirs.iter().enumerate() {
        vertices[i] = if p.rotating {
            loc + radius * *d
        } else {
            loc + radius * safe_normal(*d)
        };
    }
    let mut planes = [Vec4::ZERO; 6];
    planes[0] = plane(f, f.dot(loc));
    let max = p.max_trace_distance as f32;
    let (matrix, perspective);
    if p.fov == 0 {
        for i in 0..4 {
            let n = -safe_normal(dirs[i]);
            planes[1 + i] = plane(n, vertices[i].dot(n));
        }
        let c = loc + l * (wd / 2.0) + u * (ht / 2.0);
        let b = r / wd;
        let a = -u / ht;
        matrix = rows(
            b.extend(-c.dot(b)),
            a.extend(-c.dot(a)),
            f.extend(-c.dot(f)),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        );
        perspective = false;
        for i in 0..4 {
            vertices[i + 4] = vertices[i] + f * max;
        }
    } else {
        let half = 0.5 * f64::from(p.fov) * 0.017_453_292_519_943_295;
        let t = half.tan() as f32;
        let c = half.cos() as f32;
        let eye = loc - f * ((wd * 0.5) / t);
        planes[1] = plane_from_points(eye, vertices[1], vertices[0]);
        planes[2] = plane_from_points(eye, vertices[2], vertices[1]);
        planes[3] = plane_from_points(eye, vertices[3], vertices[2]);
        planes[4] = plane_from_points(eye, vertices[0], vertices[3]);
        // Eye frame: x right, y down, z forward.
        let (ex, ey, ez) = (r, -u, f);
        let k = 0.5 / t;
        let row = |axis: Vec3, with_z: f32| {
            (axis * k + f * with_z).extend(-(eye.dot(axis * k + f * with_z)))
        };
        matrix = rows(
            row(ex, 0.5),
            row(ey, 0.5),
            ez.extend(-eye.dot(ez)),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        );
        perspective = true;
        for i in 0..4 {
            vertices[i + 4] = vertices[i] + safe_normal(vertices[i] - eye) * (max / c);
        }
    }
    planes[5] = plane(-f, (-f).dot(vertices[7]));
    let mut box_min = vertices[0];
    let mut box_max = vertices[0];
    for v in &vertices {
        box_min = box_min.min(*v);
        box_max = box_max.max(*v);
    }
    let gradient = p.gradient_height.map(|n| {
        let n = n as f32;
        let q = (p.max_trace_distance / (n as i32).max(1)) as f32;
        let s = (n - 2.0) / (n * max);
        let origin = loc - f * q;
        // Output (0, ((p - origin) . F) * s, 0, 1).
        rows(
            Vec4::ZERO,
            (f * s).extend(-origin.dot(f * s)),
            Vec4::ZERO,
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    });
    Some(ProjectorVolume {
        matrix,
        perspective,
        planes,
        vertices,
        box_min,
        box_max,
        gradient,
        forward: f,
    })
}

/// The BSP attach gate: whether a surface with unit `normal` takes a projection going along
/// `direction`. Rejected when `N . F >= -0.0001`, and (unless parallel surfaces are allowed)
/// when `N . F >= -0.7071068`.
pub fn bsp_accepts(normal: Vec3, direction: Vec3, project_on_parallel: bool) -> bool {
    let d = f64::from(normal.dot(direction));
    if d >= -0.0001 {
        return false;
    }
    project_on_parallel || d < -f64::from(std::f32::consts::FRAC_1_SQRT_2)
}

/// How the projection joins the frame buffer, from `FrameBufferBlendingOp` (0 and 1 both
/// modulate, 2 alpha, 3 add).
pub fn frame_buffer_blend(op: ProjectorBlending) -> FrameBufferBlend {
    match op {
        ProjectorBlending::AlphaBlend => FrameBufferBlend::Alpha,
        ProjectorBlending::Add => FrameBufferBlend::Add,
        ProjectorBlending::None | ProjectorBlending::Modulate => FrameBufferBlend::Modulate2X,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameBufferBlend {
    Modulate2X,
    Alpha,
    Add,
}

/// A decal's render info for the expiry rule (`IsExpired`, Engine.dll 0x177480).
#[derive(Clone, Copy, Debug)]
pub struct DecalLife {
    /// 0 never expires.
    pub expiry: f64,
    /// 0 none, -1 forced detach, otherwise the `DecalSession` it was attached under.
    pub session_stamp: i32,
    pub uid: i32,
    /// When it was last drawn (0 never).
    pub last_seen: f64,
}

/// The level's decal counters.
#[derive(Clone, Copy, Debug)]
pub struct DecalBudget {
    pub session: i32,
    pub youngest: i32,
    pub keep: i32,
}

pub fn is_expired(life: &DecalLife, budget: &DecalBudget, time: f64) -> bool {
    if life.expiry == 0.0 {
        return false;
    }
    if life.session_stamp != 0 && (life.session_stamp == -1 || life.session_stamp != budget.session)
    {
        return true;
    }
    if life.uid > 0 && life.uid >= budget.youngest - budget.keep {
        return false;
    }
    if life.expiry >= time {
        return false;
    }
    if life.last_seen == 0.0 {
        return false;
    }
    time - life.last_seen > 1.0
}
