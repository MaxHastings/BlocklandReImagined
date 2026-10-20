//! The sun's glow, lens flares and white-out (`AAGP_Sun::ApplyGlowEffects`, Engine.dll
//! 0x5ce0; see `specs/coronas-lens-flare-laser.md`, Part 2). Everything is drawn as
//! screen-space tiles after the scene, additively (`FB_Brighten`: source alpha, one).
#![allow(clippy::collapsible_if)]
use crate::props::Color;
use crate::ribbon::pack_color;
use crate::rotation::{Coords, Rotator, rotation_matrix, safe_normal};
use crate::sprite::SpriteVertex;
use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct LensFlare {
    /// The host's id for the flare material (texture).
    pub material: u32,
    pub color: Color,
    /// 0 on the sun, 1 at the screen centre, 2 mirrored, negative beyond the sun.
    pub offset: f32,
    pub radius_min: f32,
    pub radius_max: f32,
}

#[derive(Clone, Debug)]
pub struct SunProps {
    pub lens_flares: Vec<LensFlare>,
    pub white_out: Color,
    pub white_out_ramp: f32,
    pub angle_to_start_white_out: f32,
    pub angle_to_start_lens_flares: f32,
    pub intensity_scalar: Color,
    pub draw_scale: f32,
    /// The host's id for `Skins[0]`, the glow texture, when the sun has one.
    pub glow_skin: Option<u32>,
    /// `bLensFlaresInEditor` matters only in the editor; kept for completeness.
    pub lens_flares_in_editor: bool,
}

impl Default for SunProps {
    fn default() -> Self {
        Self {
            lens_flares: Vec::new(),
            white_out: Color::rgba(0, 0, 0, 0),
            white_out_ramp: 1.0,
            angle_to_start_white_out: 20.0,
            angle_to_start_lens_flares: 55.0,
            intensity_scalar: Color::rgba(128, 128, 128, 255),
            draw_scale: 1.0,
            glow_skin: None,
            lens_flares_in_editor: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SunView {
    /// The sun's projected position in canvas pixels.
    pub screen: [f32; 2],
    pub width: f32,
    pub height: f32,
    /// Unit vector to the sun in the sky zone's frame.
    pub sun_dir: Vec3,
    /// The player controller's rotation as a unit vector (not the camera's).
    pub view_dir: Vec3,
    /// The sun's trace from the camera (100000 units) is blocked by geometry.
    pub blocked: bool,
    /// The sun's glow is drawn only when the glow-capture path is off.
    pub using_glow: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileKind {
    /// The square glow around the sun, textured with `Skins[0]`.
    Glow,
    /// Lens flare `i`.
    Flare(usize),
    /// The full-screen colour over everything, untextured.
    WhiteOut,
}

/// One screen-space tile: pixel corners, texture coordinates 0..1, drawn with the
/// bright-over blend (source alpha, one) and no depth.
#[derive(Clone, Debug)]
pub struct Tile {
    pub kind: TileKind,
    /// The host's id of the texture; `None` for the solid white-out.
    pub material: Option<u32>,
    pub vertices: [SpriteVertex; 4],
}

impl Tile {
    pub fn indices(&self) -> [u32; 6] {
        [0, 1, 2, 0, 2, 3]
    }
}

/// `rotation.Vector()`: the forward axis of a rotator.
pub fn rotator_vector(r: Rotator) -> Vec3 {
    Coords::UNIT.mul_rotator(r).x
}

/// The direction from the sky zone to the sun, `(sun - sky) * Rotation(sky)` normalised
/// (the sense of the rotation matrix is not established by the read).
pub fn sun_direction(sun: Vec3, sky_location: Vec3, sky_rotation: Rotator) -> Vec3 {
    safe_normal(rotation_matrix(sky_rotation) * (sun - sky_location))
}

fn tile(kind: TileKind, material: Option<u32>, rect: [f32; 4], color: u32) -> Tile {
    let v = |x: f32, y: f32, u: f32, w: f32| SpriteVertex {
        position: Vec3::new(x, y, 0.0),
        color,
        uv: [u, w],
        uv2: [u, w],
    };
    Tile {
        kind,
        material,
        vertices: [
            v(rect[0], rect[1], 0.0, 0.0),
            v(rect[2], rect[1], 1.0, 0.0),
            v(rect[2], rect[3], 1.0, 1.0),
            v(rect[0], rect[3], 0.0, 1.0),
        ],
    }
}

/// The intensity constant: the scalar colour, or black when the sun is hidden.
fn intensity(props: &SunProps, blocked: bool) -> Color {
    if blocked {
        Color::rgba(0, 0, 0, 255)
    } else {
        props.intensity_scalar
    }
}

/// Multiply a colour's channels by the intensity constant's (the Combiner's multiply).
fn tinted(c: Color, i: Color, alpha: u8) -> u32 {
    let m = |a: u8, b: u8| ((u32::from(a) * u32::from(b)) / 255) as u8;
    pack_color(Color {
        r: m(c.r, i.r),
        g: m(c.g, i.g),
        b: m(c.b, i.b),
        a: alpha,
    })
}

/// `ApplyGlowEffects` after its preconditions held and the screen position is known.
pub fn apply_glow_effects(props: &SunProps, view: &SunView) -> Vec<Tile> {
    let mut tiles = Vec::new();
    let i = intensity(props, view.blocked);
    let (w, h) = (view.width, view.height);
    let [sx, sy] = view.screen;
    let (cx, cy) = (0.5 * w - sx, 0.5 * h - sy);
    // Glow quad.
    if !view.using_glow {
        if let Some(skin) = props.glow_skin {
            let half = 0.5 * h * props.draw_scale;
            // Vertex colour is the scalar; the Combiner multiplies the texture by the
            // intensity constant (the scalar again, or black when the sun is hidden).
            let c = tinted(props.intensity_scalar, i, props.intensity_scalar.a);
            tiles.push(tile(
                TileKind::Glow,
                Some(skin),
                [sx - half, sy - half, sx + half, sy + half],
                c,
            ));
        }
    }
    let dot = view.view_dir.dot(view.sun_dir);
    let cos_flare =
        (f64::from(props.angle_to_start_lens_flares) * 0.017_453_292_519_943_295).cos() as f32;
    if dot > cos_flare {
        let t = (dot - cos_flare) / (1.0 - cos_flare);
        for (n, fl) in props.lens_flares.iter().enumerate() {
            let r = fl.radius_min + (fl.radius_max - fl.radius_min) * t;
            if r <= 0.0 {
                continue;
            }
            let (px, py) = (sx + cx * fl.offset, sy + cy * fl.offset);
            let half_x = 0.5 * r * w;
            let half_y = 0.5 * r * (h / 0.75);
            let alpha =
                (f32::from(props.intensity_scalar.a) / 255.0 * f32::from(fl.color.a) * t) as i32;
            tiles.push(tile(
                TileKind::Flare(n),
                Some(fl.material),
                [px - half_x, py - half_y, px + half_x, py + half_y],
                tinted(fl.color, i, alpha as u8),
            ));
        }
    }
    let cos_white =
        (f64::from(props.angle_to_start_white_out) * 0.017_453_292_519_943_295).cos() as f32;
    if dot > cos_white {
        let wo = ((dot - cos_white) / (1.0 - cos_white)).powf(props.white_out_ramp);
        let alpha = (f32::from(props.white_out.a) * wo) as i32;
        tiles.push(tile(
            TileKind::WhiteOut,
            None,
            [0.0, 0.0, w, h],
            tinted(props.white_out, i, alpha as u8),
        ));
    }
    tiles
}
