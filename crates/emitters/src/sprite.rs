//! The sprite emitter's vertex fill (`USpriteEmitter::FillVertexBuffer`, Engine.dll
//! 0x2296f0) and the render setup around it (`RenderParticles`, 0x22b010), as read in
//! `native-research/specs/sprite-vertex-fill.md`. Four vertices per live particle,
//! in the order and with the texture rectangles the original writes; the two
//! triangles of each quad are `(0, 1, 2)` and `(0, 2, 3)`.
use crate::draw::DrawState;
use crate::particle::{Particle, flags};
use crate::props::{
    CoordinateSystem, DirectionUsage, ParticleEmitterProps, RotationSource, SpriteExtra,
};
use crate::rotation::rotate_angle_axis;
use glam::{Mat3, Mat4, Vec3};

/// What the original read from the scene node: the world-space vectors for screen
/// "up" and "right" (`Deproject` of the points (0,-1000,0) and (1000,0,0) minus the
/// origin; any length), and the eye.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub up: Vec3,
    pub right: Vec3,
}

/// The emitter's owner. `local_to_world` is the original's `Rotation x Translation`.
#[derive(Clone, Copy, Debug)]
pub struct Owner {
    pub local_to_world: Mat4,
    /// The owner's rotation alone, for `UseRotationFrom == Actor` (the projection
    /// normal is carried through it).
    pub rotation: Mat3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteVertex {
    pub position: Vec3,
    /// D3D colour dword: A << 24 | R << 16 | G << 8 | B.
    pub color: u32,
    pub uv: [f32; 2],
    /// The second frame's coordinates (see `blend_subdivisions`).
    pub uv2: [f32; 2],
}

/// What the render interface was told besides the vertices (`RenderParticles`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteDraw {
    pub state: DrawState,
    /// Mix the frame of `uv` with the frame of `uv2` by the vertex alpha (weight of
    /// the second frame), the way the original's second texture stage does.
    pub blend_subdivisions: bool,
    pub alpha_test: bool,
    pub alpha_ref: i32,
    pub z_test: bool,
    pub z_write: bool,
    pub two_sided: bool,
    pub accepts_projectors: bool,
    /// The transform vertices are submitted under (`None`: identity).
    pub local_to_world: Option<Mat4>,
}

#[derive(Clone, Debug)]
pub struct SpriteBatch {
    pub vertices: Vec<SpriteVertex>,
    pub draw: SpriteDraw,
}

impl SpriteBatch {
    /// Indices of the quads: `(0, 1, 2)` and `(0, 2, 3)` per four vertices.
    pub fn indices(&self) -> Vec<u32> {
        (0..self.vertices.len() as u32 / 4)
            .flat_map(|k| [0, 1, 2, 0, 2, 3].map(|i| 4 * k + i))
            .collect()
    }
}

const EPS: f32 = 1e-8;

fn normalize_or_leave(v: Vec3) -> Vec3 {
    if v.length_squared() >= EPS {
        v * (1.0 / v.length_squared().sqrt())
    } else {
        v
    }
}
fn normalize_or_zero(v: Vec3) -> Vec3 {
    if v.length_squared() >= EPS {
        v * (1.0 / v.length_squared().sqrt())
    } else {
        Vec3::ZERO
    }
}

/// A frame number's rectangle in the USub x VSub grid: columns run V-fastest.
fn frame_rect(f: i32, usub: i32, vsub: i32, clamp: bool) -> (f32, f32, f32, f32) {
    let (du, dv) = (1.0 / usub as f32, 1.0 / vsub as f32);
    let q = f / vsub;
    let r = f % vsub;
    let ulo = q as f32 * du;
    let vlo = r as f32 * dv;
    let (mut uhi, mut vhi) = (ulo + du, vlo + dv);
    if clamp {
        uhi = uhi.min(1.0);
        vhi = vhi.min(1.0);
    }
    (ulo, uhi, vlo, vhi)
}

struct Frames {
    set0: (f32, f32, f32, f32),
    set1: (f32, f32, f32, f32),
    /// The alpha byte that replaces the particle's own, when blending frames.
    alpha: Option<u8>,
}

/// Step 7: which part of the texture a particle shows, and the second frame.
fn frames(p: &Particle, e: &ParticleEmitterProps, ratio: f32) -> Frames {
    let whole = (0.0, 1.0, 0.0, 1.0);
    let (usub, vsub) = (e.texture_u_subdivisions, e.texture_v_subdivisions);
    if usub == 0 || vsub == 0 {
        return Frames {
            set0: whole,
            set1: whole,
            alpha: None,
        };
    }
    if p.subdivision != -1 {
        return Frames {
            set0: frame_rect(p.subdivision, usub, vsub, true),
            set1: whole,
            alpha: None,
        };
    }
    if p.max_lifetime == 0.0 {
        return Frames {
            set0: whole,
            set1: whole,
            alpha: None,
        };
    }
    let total = usub * vsub;
    let scale = &e.subdivision_scale;
    let (mut f, n) = if e.use_subdivision_scale {
        let at = scale
            .iter()
            .position(|&s| ratio <= s)
            .unwrap_or(scale.len());
        (at as i32, total)
    } else if e.subdivision_end != 0 {
        let n = (e.subdivision_end - e.subdivision_start).max(1);
        (e.subdivision_start + (n as f32 * ratio) as i32, n)
    } else {
        ((total as f32 * ratio) as i32, total)
    };
    f = f.clamp(0, total - 1);
    let set0 = frame_rect(f, usub, vsub, true);
    if !e.blend_between_subdivisions {
        return Frames {
            set0,
            set1: whole,
            alpha: None,
        };
    }
    let scale_branch = e.use_subdivision_scale && scale.len() as i32 > n;
    let (frac, g) = if scale_branch {
        // As read: Scale[f - 2] (index -2 for f = 0 reads outside the array; taken as 0),
        // and the second frame is f itself.
        let get = |i: i32| usize::try_from(i).ok().and_then(|i| scale.get(i)).copied();
        let prev = if f - 1 == 0 {
            0.0
        } else {
            get(f - 2).unwrap_or(0.0)
        };
        let cur = get(f - 1).unwrap_or(1.0).min(1.0);
        ((ratio - prev) / (cur - prev), f)
    } else {
        let x = n as f32 * ratio;
        (x - (x as i32) as f32, f + 1)
    };
    let alpha = ((frac * 255.0) as i32 & 0xff) as u8;
    let set1 = if g == n {
        set0
    } else {
        frame_rect(g, usub, vsub, false)
    };
    Frames {
        set0,
        set1,
        alpha: Some(alpha),
    }
}

/// Fill the batch for the live particles (`flags & ALIVE`).
pub fn fill(
    e: &ParticleEmitterProps,
    sprite: &SpriteExtra,
    particles: &[Particle],
    camera: &Camera,
    owner: Option<&Owner>,
) -> SpriteBatch {
    // Step 1 and 2: the camera in the frame the vertices are built in.
    let mut cam = camera.position;
    let mut up = camera.up;
    let mut right = camera.right;
    let mut fwd = right.cross(up);
    let relative = e.coordinate_system == CoordinateSystem::Relative;
    let local_to_world = relative.then(|| owner.map_or(Mat4::IDENTITY, |o| o.local_to_world));
    if let Some(m) = local_to_world {
        let inverse = m.inverse();
        cam = inverse.transform_point3(cam);
        up = inverse.transform_vector3(up);
        right = inverse.transform_vector3(right);
        fwd = inverse.transform_vector3(fwd);
    }
    let up = normalize_or_leave(up);
    let right = normalize_or_leave(right);
    let fwd = normalize_or_leave(fwd);
    let mut projection_normal = sprite.real_projection_normal;
    if let (RotationSource::Actor, Some(o)) = (e.use_rotation_from, owner) {
        projection_normal = o.rotation * projection_normal;
    }
    let mode = sprite.use_direction_as;
    let mut vertices = Vec::new();
    for p in particles.iter().filter(|p| p.flags & flags::ALIVE != 0) {
        let t = if p.time > p.max_lifetime || p.time.is_nan() {
            p.max_lifetime
        } else {
            p.time
        };
        let ratio = if p.max_lifetime == 0.0 {
            0.0
        } else {
            let r = t / p.max_lifetime;
            if r.is_nan() || r > 1.0 {
                1.0
            } else if r < 0.0 {
                0.0
            } else {
                r
            }
        };
        // Step 4.
        let vel = p.location - p.old_location;
        let n_v = normalize_or_zero(vel);
        let d = p.location - cam;
        let (n1, n2) = match mode {
            DirectionUsage::Normal => (projection_normal, Vec3::ZERO),
            DirectionUsage::Forward => (n_v, Vec3::ZERO),
            DirectionUsage::UpAndNormal | DirectionUsage::RightAndNormal => {
                (n_v, n_v.cross(sprite.real_projection_normal))
            }
            _ => (n_v, normalize_or_zero(n_v.cross(d))),
        };
        let (sx, sy) = (p.size.x, p.size.y);
        // Step 5.
        let (mut a, mut b, axis) = match mode {
            DirectionUsage::None => (up * sx, right * sx, fwd),
            DirectionUsage::Up | DirectionUsage::UpAndNormal => (n1 * sy, n2 * sx, fwd),
            DirectionUsage::Right | DirectionUsage::RightAndNormal => (n2 * sy, n1 * sx, fwd),
            DirectionUsage::Forward | DirectionUsage::Normal => {
                let q = if n1.x.abs() > 0.57 {
                    Vec3::Y
                } else if n1.y.abs() > 0.57 {
                    Vec3::Z
                } else {
                    Vec3::X
                };
                let a_prime = n1.cross(q);
                (a_prime * sy, normalize_or_zero(n1.cross(a_prime)) * sx, n1)
            }
            DirectionUsage::Scale => (up * sy, right * sx, fwd),
        };
        // Step 6.
        if e.spin_particles {
            let sps = p.spins_per_second.x;
            let ss = p.start_spin.x;
            let angle = if sps >= 0.0 {
                (t * sps + ss) as i32 & 0xffff
            } else {
                0xffff - ((ss - t * sps) as i32 & 0xffff)
            };
            a = rotate_angle_axis(a, angle, axis);
            b = rotate_angle_axis(b, angle, axis);
        }
        // Step 7 and 8.
        let fr = frames(p, e, ratio);
        let mut color = p.color;
        if let Some(alpha) = fr.alpha {
            color.a = alpha;
        }
        let dword = u32::from(color.a) << 24
            | u32::from(color.r) << 16
            | u32::from(color.g) << 8
            | u32::from(color.b);
        // Step 9.
        let (ulo, uhi, vlo, vhi) = fr.set0;
        let (u1lo, u1hi, v1lo, v1hi) = fr.set1;
        let l = p.location;
        for (position, uv, uv2) in [
            (l + a - b, [ulo, vlo], [u1lo, v1lo]),
            (l + a + b, [uhi, vlo], [u1hi, v1lo]),
            (l - a + b, [uhi, vhi], [u1hi, v1hi]),
            (l - a - b, [ulo, vhi], [u1lo, v1hi]),
        ] {
            vertices.push(SpriteVertex {
                position,
                color: dword,
                uv,
                uv2,
            });
        }
    }
    SpriteBatch {
        vertices,
        draw: SpriteDraw {
            state: e.draw_style.draw_state(),
            blend_subdivisions: e.blend_between_subdivisions && !e.use_random_subdivision,
            alpha_test: e.alpha_test,
            alpha_ref: e.alpha_ref,
            z_test: e.z_test,
            z_write: e.z_write,
            two_sided: true,
            accepts_projectors: e.accepts_projectors,
            local_to_world,
        },
    }
}
