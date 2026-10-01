//! Planar mirrors: each frame the world is drawn again from the view a
//! mirror reflects, into a texture its surface then shows, as engines have
//! drawn flat mirrors and polished floors since the 1990s. The same passes
//! draw windows onto elsewhere (portals): a surface that shows the view out
//! of another place, moved there by a rigid transform instead of reflected.
//!
//! Coplanar mirrors (a wall of mirror bricks) share one plane and one extra
//! pass. The planes that fill the most of the screen reflect live, up to the
//! player's Reflections setting; the rest, and mirrors seen inside another
//! mirror, show a plain silver. Each pass draws only what lies in front of
//! its mirror (an oblique near plane) inside the part of the screen the
//! mirror covers, so a small mirror costs a small pass.
use crate::scene::{Camera, DEPTH_FORMAT, GpuInstances, GpuScene, SceneRenderer, WorldPass};
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};
use std::ops::Range;
use wgpu::util::DeviceExt;

/// What a surface shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Looks {
    /// The room in front of it, reflected.
    Reflect,
    /// Another place: the matrix takes what lies beyond that place's
    /// window to where it shows behind this surface (a rigid move).
    Through(Mat4),
    /// Only its `fallback` colour (a window with nowhere to look).
    Plain,
}
/// Unlit polished silver (display encoded): what a mirror shows when it
/// does not reflect live.
pub const SILVER: [f32; 3] = [0.55, 0.57, 0.6];

/// One flat view surface in world space: a mirror, or a window onto
/// elsewhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mirror {
    /// Counterclockwise seen from the side it faces.
    pub corners: [Vec3; 4],
    /// Multiplies the picture.
    pub tint: [f32; 3],
    /// 1 hides what the surface is drawn on; less blends over it.
    pub strength: f32,
    pub looks: Looks,
    /// Shown when the surface is not drawn live (display encoded).
    pub fallback: [f32; 3],
    /// Drawn as an open box this deep behind the corners instead of flat:
    /// a window the eye is about to pass through keeps covering the screen
    /// past the near plane. 0 is flat.
    pub recess: f32,
}
impl Mirror {
    /// A flat mirror.
    pub fn reflecting(corners: [Vec3; 4], tint: [f32; 3], strength: f32) -> Self {
        Self {
            corners,
            tint,
            strength,
            looks: Looks::Reflect,
            fallback: SILVER,
            recess: 0.0,
        }
    }
    /// Takes what the surface shows to where it appears: the reflection
    /// through its plane, or its window's move.
    fn transfer(&self, plane: Vec4) -> Option<Mat4> {
        match self.looks {
            Looks::Reflect => Some(reflection_matrix(plane)),
            Looks::Through(matrix) => Some(matrix),
            Looks::Plain => None,
        }
    }
    /// The plane `n·p + d = 0`, `n` toward the reflecting side, or None for
    /// a degenerate or non-finite mirror.
    pub fn plane(&self) -> Option<Vec4> {
        let [a, b, c, _] = self.corners;
        let normal = (b - a).cross(c - b).try_normalize()?;
        let plane = normal.extend(-normal.dot(a));
        (self.corners.iter().all(|p| p.is_finite())
            && self.tint.iter().chain(&self.fallback).all(|t| t.is_finite())
            && self.strength.is_finite()
            && self.strength > 0.0
            && self.recess.is_finite()
            && match self.looks {
                Looks::Through(m) => m.is_finite() && m.determinant().abs() > 1e-4,
                _ => true,
            })
            .then_some(plane)
    }
}

/// How many planes reflect live and how finely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReflectionSettings {
    /// Planes drawn live each frame; 0 shows every mirror as silver.
    pub planes: usize,
    /// Reflection resolution as a share of the screen's. Below 1 the
    /// reflection is upscaled and its textures read a coarser mip, so it
    /// looks soft and greyer than the room; only Low trades that for speed.
    pub scale: f32,
    /// Mirrors further than this from the eye stay silver (windows go live
    /// at any distance).
    pub distance: f32,
}
impl ReflectionSettings {
    pub const OFF: Self = Self {
        planes: 0,
        scale: 0.5,
        distance: 0.0,
    };
    pub const LOW: Self = Self {
        planes: 1,
        scale: 0.5,
        distance: 48.0,
    };
    pub const MEDIUM: Self = Self {
        planes: 2,
        scale: 1.0,
        distance: 64.0,
    };
    pub const HIGH: Self = Self {
        planes: 3,
        scale: 1.0,
        distance: 96.0,
    };
    /// The most planes any setting draws.
    pub const MAX_PLANES: usize = 3;
}

/// At most this many mirrors, nearest first, are drawn at all.
pub const MAX_MIRRORS: usize = 4096;
/// Least distance a live view's eye keeps behind the plane it is clipped
/// at (see `plan`).
const CLIP_CLEARANCE: f32 = 0.01;

/// A plane chosen to reflect live this frame, seen from the player's view
/// or, a bounce deeper, from another live plane's reflected view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlannedPlane {
    pub plane: Vec4,
    /// The plane its view is clipped at (kept side positive): the mirror's
    /// own, or the window's partner's.
    pub clip: Vec4,
    /// The view is mirrored (x flipped so its triangles keep their
    /// winding): a surface position `u` samples it at `mirror_u - u`.
    /// Otherwise a window's picture lines up with the screen.
    pub flipped: bool,
    /// The surface's view: moved, clipped at `clip` and cropped to
    /// `viewport`.
    pub view_projection: Mat4,
    /// The eye reflected in the plane (and in each plane it is seen in).
    pub eye: Vec3,
    /// x, y, width, height in target pixels; the same share of the screen.
    pub viewport: [f32; 4],
    /// Left plus right edge of the viewport as a share of the target's
    /// width: a screen position `u` samples a flipped view at this less `u`.
    pub mirror_u: f32,
    /// The view this plane is seen in: 0 the player's, 1 + i live plane i's.
    pub parent: usize,
    /// The coplanar group of mirrors it shows (an index into `Plan::groups`).
    pub group: usize,
    /// The reflections from the player's view down to this plane,
    /// composed: it takes the player's eye and camera axes to this view's.
    pub unreflect: Mat4,
}

impl PlannedPlane {
    /// A direction as the mirror shows it: what faces the player's camera
    /// faces the reflected eye once turned by this.
    pub fn reflect_direction(&self, direction: Vec3) -> Vec3 {
        self.unreflect.transform_vector3(direction)
    }
}

/// What one frame draws: the live planes (a parent before the planes seen
/// in it) and the mirrors, in coplanar groups.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub planes: Vec<PlannedPlane>,
    /// Coplanar mirrors (a wall of mirror bricks) showing one view, by
    /// index into the mirrors given; every drawn mirror is in exactly one.
    pub groups: Vec<Vec<usize>>,
    /// Each group's plane.
    pub group_planes: Vec<Vec4>,
    /// The mirrors drawn at all (the nearest `MAX_MIRRORS`), in order.
    pub drawn: Vec<usize>,
    /// Each group's identity from frame to frame: its plane, rounded, and
    /// what it shows.
    pub group_keys: Vec<GroupKey>,
    /// Groups whose picture from the frame before is kept, by key, with the
    /// reflection target that holds it ([`Shows::Last`]).
    pub last: Vec<(GroupKey, usize)>,
}
/// See [`Plan::group_keys`].
pub type GroupKey = ([i32; 4], Vec<i64>);
/// What a mirror surface shows in one view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shows {
    /// Live plane i's reflection, drawn this frame for this view.
    Live(usize),
    /// Live plane i's last picture, reprojected: a mirror seen deeper than
    /// the passes reach shows what the same mirror showed nearer the
    /// player, a frame late, so facing mirrors repeat into the distance.
    Echo(usize),
    /// The frame before's picture of this group, kept in target i: what a
    /// surface shows where no live pass draws it this frame (a window in
    /// sight of its own partner, deeper than the passes), as Valve's Portal
    /// does past its recursion limit, instead of a flat colour.
    Last(usize),
    Silver,
}
impl Plan {
    /// For each group, what `view` (0 the player's, 1 + i live plane i's)
    /// shows on it; None for the view's own plane, which lies on its clip
    /// plane.
    pub fn slots(&self, view: usize) -> Vec<Option<Shows>> {
        let mut out = vec![Some(Shows::Silver); self.groups.len()];
        // The nearest plane of a group is the one echoed.
        for (i, plane) in self.planes.iter().enumerate().rev() {
            out[plane.group] = Some(Shows::Echo(i));
        }
        for (i, plane) in self.planes.iter().enumerate() {
            if plane.parent == view {
                out[plane.group] = Some(Shows::Live(i));
            }
        }
        // What lies on the view's clip plane (its mirror, the window it
        // looks out of) is not drawn in it.
        if let Some(i) = view.checked_sub(1)
            && let Some(own) = self.planes.get(i)
        {
            for (group, plane) in self.group_planes.iter().enumerate() {
                if on_plane(*plane, own.clip) {
                    out[group] = None;
                }
            }
            // A view never shows the picture it is drawing: a window seen
            // in its own view (a portal in sight of its partner) would
            // sample the target being drawn. It shows its fallback there,
            // as surfaces past the passes do.
            for slot in &mut out {
                if matches!(slot, Some(Shows::Live(j) | Shows::Echo(j)) if *j == i) {
                    *slot = Some(Shows::Silver);
                }
            }
        }
        for (slot, key) in out.iter_mut().zip(&self.group_keys) {
            if *slot == Some(Shows::Silver)
                && let Some((_, k)) = self.last.iter().find(|(last, _)| last == key)
            {
                *slot = Some(Shows::Last(*k));
            }
        }
        out
    }
}

/// The screen rectangle (NDC min x, min y, max x, max y) a quad covers,
/// clipped to the near plane and the screen; None when off screen.
fn screen_rect(corners: &[Vec3], view_projection: Mat4) -> Option<[f32; 4]> {
    let clip: Vec<Vec4> = corners
        .iter()
        .map(|p| view_projection * p.extend(1.0))
        .collect();
    // Sutherland-Hodgman against the near plane (z <= w in reversed 0..1
    // depth, `scene::DEPTH_CLEAR`).
    let ahead = |c: Vec4| c.w - c.z;
    let mut kept = Vec::with_capacity(8);
    for i in 0..clip.len() {
        let (a, b) = (clip[i], clip[(i + 1) % clip.len()]);
        if ahead(a) >= 0.0 {
            kept.push(a);
        }
        if (ahead(a) >= 0.0) != (ahead(b) >= 0.0) {
            let t = ahead(a) / (ahead(a) - ahead(b));
            kept.push(a + (b - a) * t);
        }
    }
    let mut rect = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    for c in kept {
        if c.w <= 1e-6 {
            continue;
        }
        let (x, y) = (c.x / c.w, c.y / c.w);
        rect = [rect[0].min(x), rect[1].min(y), rect[2].max(x), rect[3].max(y)];
    }
    let rect = [
        rect[0].max(-1.0),
        rect[1].max(-1.0),
        rect[2].min(1.0),
        rect[3].min(1.0),
    ];
    (rect[0] < rect[2] && rect[1] < rect[3]).then_some(rect)
}

/// Whether two planes are the same one, either way round (a hundredth of
/// a degree, 5 mm).
fn on_plane(a: Vec4, b: Vec4) -> bool {
    let same = |b: Vec4| a.xyz().dot(b.xyz()) > 0.99999 && (a.w - b.w).abs() < 0.005;
    same(b) || same(-b)
}

/// Reflection through the plane `n·p + d = 0`.
pub fn reflection_matrix(plane: Vec4) -> Mat4 {
    let n = plane.xyz();
    let column = |axis: Vec3| (axis - 2.0 * n.dot(axis) * n).extend(0.0);
    Mat4::from_cols(
        column(Vec3::X),
        column(Vec3::Y),
        column(Vec3::Z),
        (-2.0 * plane.w * n).extend(1.0),
    )
}

/// `matrix` with its near plane moved onto `plane` (kept side positive),
/// Lengyel's oblique frustum for reversed 0..1 depth: nothing behind the
/// mirror draws, and the far plane (depth 0) still encloses the old
/// frustum.
pub fn oblique(matrix: Mat4, plane: Vec4) -> Mat4 {
    // The near plane `w - z >= 0` becomes `plane / reach`: depth
    // `1 - plane·p / (w reach)`, which stays at or above 0 over the old
    // frustum when `reach` is the plane's largest value over its clip box
    // (x, y in -1..1, depth 0..1, per unit w).
    let clip = matrix.inverse().transpose() * plane;
    let reach = clip.x.abs() + clip.y.abs() + clip.z.max(0.0) + clip.w;
    if !reach.is_finite() || reach < 1e-9 {
        return matrix;
    }
    let row = matrix.row(3) - plane / reach;
    rows([matrix.row(0), matrix.row(1), row, matrix.row(3)])
}

fn rows(r: [Vec4; 4]) -> Mat4 {
    Mat4::from_cols(r[0], r[1], r[2], r[3]).transpose()
}

/// One view planes may be seen in: its clip matrix, the target pixels its
/// clip space fills, its eye, the reflections leading to it and the plane
/// it looks out of.
struct View {
    view_projection: Mat4,
    viewport: [f32; 4],
    eye: Vec3,
    unreflect: Mat4,
    /// The plane it is clipped at.
    own: Option<Vec4>,
}
/// A group a view sees: its screen rectangle in target pixels (x0, y0,
/// x1, y1) and the pixels it covers.
fn seen(
    view: &View,
    plane: Vec4,
    members: &[usize],
    mirrors: &[Mirror],
    settings: &ReflectionSettings,
) -> Option<[f32; 4]> {
    // A recessed window (the eye about to pass through it) still shows with
    // the eye inside its box, behind its face.
    let recess = members
        .iter()
        .map(|&i| mirrors[i].recess)
        .fold(0.0, f32::max);
    let front = if recess > 0.0 { -recess } else { 1e-3 };
    if view.own.is_some_and(|own| on_plane(own, plane))
        || plane.xyz().dot(view.eye) + plane.w <= front
        || mirrors[members[0]].looks == Looks::Plain
    {
        return None;
    }
    // Reach runs along the reflected path: the view's eye is as far behind
    // its mirrors as the light has travelled.
    let near = members
        .iter()
        .flat_map(|&i| mirrors[i].corners)
        .map(|p| p.distance(view.eye))
        .fold(f32::INFINITY, f32::min);
    // A mirror shows its surroundings, so past the setting's distance it
    // stays silver. A window shows somewhere else, which no flat colour
    // stands in for (Max, v0.1.11: far portals turned light blue), so it
    // goes live at any distance: its view is fitted to the little screen
    // it covers, so it draws only what lies in that narrow cone, and it
    // still competes for the setting's passes by the screen it fills.
    if near > settings.distance && !matches!(mirrors[members[0]].looks, Looks::Through(_)) {
        return None;
    }
    let mut rect: Option<[f32; 4]> = None;
    // What it covers on screen is what it draws: its quad, or its box,
    // which stays on screen with the eye closer than the near plane.
    for &i in members {
        let shapes: Vec<Vec<Vec3>> = if mirrors[i].recess > 0.0 {
            surface_triangles(&mirrors[i])
                .iter()
                .map(|t| t.to_vec())
                .collect()
        } else {
            vec![mirrors[i].corners.to_vec()]
        };
        for shape in shapes {
            if let Some(r) = screen_rect(&shape, view.view_projection) {
                rect = Some(rect.map_or(r, |a| {
                    [a[0].min(r[0]), a[1].min(r[1]), a[2].max(r[2]), a[3].max(r[3])]
                }));
            }
        }
    }
    // Clip space to whole target pixels within the view's viewport.
    let [vx, vy, vw, vh] = view.viewport;
    let rect = rect?;
    let x0 = (vx + (rect[0] + 1.0) * 0.5 * vw).floor().max(vx);
    let x1 = (vx + (rect[2] + 1.0) * 0.5 * vw).ceil().min(vx + vw);
    let y0 = (vy + (1.0 - rect[3]) * 0.5 * vh).floor().max(vy);
    let y1 = (vy + (1.0 - rect[1]) * 0.5 * vh).ceil().min(vy + vh);
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1, y1])
}

/// Choose this frame's live planes. `target` is the reflection textures'
/// size, a `settings.scale` share of the screen. Planes the player sees
/// and planes seen in their reflections (two mirrors facing each other)
/// compete for the setting's passes by the screen they fill, so a bounce
/// deeper costs a pass only when it shows; beyond the passes, mirrors are
/// silver.
pub fn plan(
    mirrors: &[Mirror],
    view_projection: Mat4,
    eye: Vec3,
    settings: &ReflectionSettings,
    target: (u32, u32),
) -> Plan {
    let mut out = Plan::default();
    // The nearest mirrors, each by its nearest corner.
    let mut near: Vec<(f32, usize, Vec4)> = mirrors
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let plane = m.plane()?;
            let distance = m
                .corners
                .iter()
                .map(|p| p.distance(eye))
                .fold(f32::INFINITY, f32::min);
            Some((distance, i, plane))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    near.truncate(MAX_MIRRORS);
    out.drawn = near.iter().map(|(_, i, _)| *i).collect();
    out.drawn.sort_unstable();
    // Coplanar mirrors, found by their plane rounded (a hundredth of a
    // degree, 5 mm) and what they show (windows onto one place), then kept
    // in mirror order.
    let mut planes: Vec<Vec4> = Vec::new();
    let mut transfers: Vec<Option<Mat4>> = Vec::new();
    let mut by_plane = std::collections::HashMap::new();
    for &i in &out.drawn {
        let mirror = &mirrors[i];
        let plane = mirror.plane().expect("drawn mirrors have planes");
        let key = (plane.xyz() * 1e4).round().as_ivec3().extend((plane.w * 200.0).round() as i32);
        let looks: Vec<i64> = match mirror.looks {
            Looks::Reflect => vec![0],
            Looks::Plain => vec![1],
            Looks::Through(m) => m
                .to_cols_array()
                .iter()
                .map(|v| (v * 200.0).round() as i64)
                .collect(),
        };
        let group = *by_plane.entry((key.to_array(), looks.clone())).or_insert_with(|| {
            out.group_keys.push((key.to_array(), looks));
            planes.push(plane);
            transfers.push(mirror.transfer(plane));
            out.groups.push(Vec::new());
            planes.len() - 1
        });
        out.groups[group].push(i);
    }
    out.group_planes = planes.clone();
    if settings.planes == 0 || target.0 == 0 || target.1 == 0 {
        return out;
    }
    let (width, height) = (target.0 as f32, target.1 as f32);
    let mut views = vec![View {
        view_projection,
        viewport: [0.0, 0.0, width, height],
        eye,
        unreflect: Mat4::IDENTITY,
        own: None,
    }];
    // Candidates: (view, group, pixel rect); the largest goes live next.
    let mut candidates: Vec<(usize, usize, [f32; 4])> = Vec::new();
    let look = |view: usize, views: &[View], candidates: &mut Vec<_>| {
        for (group, members) in out.groups.iter().enumerate() {
            if let Some(rect) = seen(&views[view], planes[group], members, mirrors, settings) {
                candidates.push((view, group, rect));
            }
        }
    };
    look(0, &views, &mut candidates);
    while out.planes.len() < settings.planes {
        let area = |r: &[f32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
        let Some(best) = (0..candidates.len()).max_by(|&a, &b| {
            area(&candidates[a].2)
                .total_cmp(&area(&candidates[b].2))
                .then(b.cmp(&a))
        }) else {
            break;
        };
        let (parent, group, [x0, y0, x1, y1]) = candidates.swap_remove(best);
        let plane = planes[group];
        let Some(transfer) = transfers[group] else {
            continue;
        };
        let seen_from = &views[parent];
        // The parent's clip space over the covered pixels: left, right,
        // bottom, top.
        let [vx, vy, vw, vh] = seen_from.viewport;
        let (left, right) = ((x0 - vx) / vw * 2.0 - 1.0, (x1 - vx) / vw * 2.0 - 1.0);
        let (bottom, top) = (1.0 - (y1 - vy) / vh * 2.0, 1.0 - (y0 - vy) / vh * 2.0);
        // What shows is what the transfer puts behind the surface: clip at
        // the plane that takes to it (the mirror's own; the partner's).
        let clip = -(transfer.transpose() * plane);
        let mut clip = clip / clip.xyz().length();
        let back = transfer.inverse();
        let moved_eye = back.transform_point3(seen_from.eye);
        // The moved eye must stay behind the clip plane for the oblique
        // near plane to hold; an eye inside a window's recess (about to
        // pass through) is on or past it, so the plane steps back from the
        // eye by a hair.
        let behind = clip.xyz().dot(moved_eye) + clip.w;
        if behind > -CLIP_CLEARANCE {
            clip.w -= behind + CLIP_CLEARANCE;
        }
        let moved = oblique(seen_from.view_projection * transfer, clip);
        // A reflection turns triangles over; flipping x turns them back.
        let flipped = transfer.determinant() < 0.0;
        let flip = if flipped { -1.0 } else { 1.0 };
        let w = moved.row(3);
        let view_projection = rows([
            flip * (2.0 * moved.row(0) - (left + right) * w) / (right - left),
            (2.0 * moved.row(1) - (bottom + top) * w) / (top - bottom),
            moved.row(2),
            w,
        ]);
        let unreflect = back * seen_from.unreflect;
        out.planes.push(PlannedPlane {
            plane,
            clip,
            flipped,
            view_projection,
            eye: moved_eye,
            viewport: [x0, y0, x1 - x0, y1 - y0],
            mirror_u: (x0 + x1) / width,
            parent,
            group,
            unreflect,
        });
        views.push(View {
            view_projection,
            viewport: [x0, y0, x1 - x0, y1 - y0],
            eye: moved_eye,
            unreflect,
            own: Some(clip),
        });
        look(views.len() - 1, &views, &mut candidates);
    }
    out
}

/// A surface's triangles, facing where it faces: its quad, or with a
/// recess the inside of an open box behind it. Either covers exactly the
/// screen the quad does from in front, and each pixel samples its slot by
/// screen position, so the box shows the same picture; only it still covers
/// the screen once the eye is closer than the near plane.
fn surface_triangles(mirror: &Mirror) -> Vec<[Vec3; 3]> {
    let [a, b, c, d] = mirror.corners;
    let quad = |p: [Vec3; 4]| [[p[0], p[1], p[2]], [p[0], p[2], p[3]]];
    let depth = mirror.recess;
    let normal = (b - a).cross(c - b).try_normalize();
    let (Some(normal), true) = (normal, depth > 0.0) else {
        return quad(mirror.corners).to_vec();
    };
    let back = mirror.corners.map(|p| p - normal * depth);
    let [ba, bb, bc, bd] = back;
    let mut out = quad(back).to_vec();
    // Each wall faces into the box, toward the opening.
    for (p, q, bp, bq) in [(a, b, ba, bb), (b, c, bb, bc), (c, d, bc, bd), (d, a, bd, ba)] {
        out.extend(quad([p, q, bq, bp]));
    }
    out
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MirrorVertex {
    position: [f32; 3],
    tint: [f32; 4],
    fallback: [f32; 4],
}
/// One view's camera and fog for mirror surfaces.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
    view_projection: [f32; 16],
    eye: [f32; 4],
    fog_color: [f32; 4],
    atmosphere: [f32; 4],
    /// Target width and height in pixels.
    screen: [f32; 4],
}
fn frame_uniform(camera: &Camera, view_projection: Mat4, eye: Vec3, size: (u32, u32)) -> FrameUniform {
    FrameUniform {
        view_projection: view_projection.to_cols_array(),
        eye: eye.extend(1.0).to_array(),
        fog_color: camera.fog_color,
        atmosphere: camera.atmosphere,
        screen: [size.0 as f32, size.1 as f32, 0.0, 0.0],
    }
}
/// What a slot shows and how it samples it (see the fields).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SlotUniform {
    /// Echo: the view the plane was seen in, world to clip space.
    reproject: [f32; 16],
    /// mirror_u, then 1 live or 2 echo (0 the fallback colour), then 1
    /// when the view is flipped (a reflection).
    sample: [f32; 4],
    /// Echo: that view's viewport, then the plane's, in target pixels.
    parent: [f32; 4],
    viewport: [f32; 4],
    /// Target width and height in pixels.
    target: [f32; 4],
}
impl SlotUniform {
    fn silver() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

struct Target {
    /// Multisampled colour when the world pass is; it resolves into `picture`.
    color: Option<wgpu::TextureView>,
    picture: wgpu::TextureView,
    picture_texture: wgpu::Texture,
    /// The picture from the frame before, copied before this frame's
    /// passes draw over it ([`Shows::Last`]).
    previous: wgpu::TextureView,
    previous_texture: wgpu::Texture,
    depth: wgpu::TextureView,
}
struct Bound {
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
}

/// Mirror surfaces and the textures their reflections render into, for
/// one colour format and sample count (the world pass's).
pub struct Reflections {
    settings: ReflectionSettings,
    samples: u32,
    format: wgpu::TextureFormat,
    /// Reflection texture size.
    size: (u32, u32),
    targets: Vec<Target>,
    /// Opaque (strength 1), then blended.
    pipelines: [wgpu::RenderPipeline; 2],
    frame_layout: wgpu::BindGroupLayout,
    slot_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// One per view: the player's, then each live plane's.
    frames: Vec<Bound>,
    /// One per live plane, then the silver one every other mirror shows.
    slots: Vec<Bound>,
    silver: wgpu::TextureView,
    /// Per target, the echo sampling of the picture it drew last frame
    /// (its group's key with it), to show it a frame later.
    held: Vec<Option<(GroupKey, SlotUniform)>>,
    /// This frame's echo sampling per live plane.
    echoes: Vec<SlotUniform>,
    vertices: Option<wgpu::Buffer>,
    /// Vertex ranges per pipeline, by coplanar group.
    ranges: [Vec<Range<u32>>; 2],
    plan: Plan,
    camera: Camera,
}

const SHADER: &str = include_str!("mirror.wgsl");

impl Reflections {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        samples: u32,
        settings: ReflectionSettings,
    ) -> Self {
        let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirror frame"),
            entries: &[uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let slot_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirror reflection"),
            entries: &[
                uniform(0, wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mirror surfaces"),
            bind_group_layouts: &[Some(&frame_layout), Some(&slot_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mirror surfaces"),
            source: wgpu::ShaderSource::Wgsl(crate::color::shader_source(SHADER).into()),
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x4];
        let pipeline = |blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mirror surfaces"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<MirrorVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(blend.is_none()),
                    depth_compare: Some(crate::scene::DEPTH_NEARER),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &crate::color::output_constants(format),
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipelines = [pipeline(None), pipeline(Some(wgpu::BlendState::ALPHA_BLENDING))];
        let silver = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("silver mirror"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mirror reflection"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            settings,
            samples,
            format,
            size: (0, 0),
            targets: Vec::new(),
            pipelines,
            frame_layout,
            slot_layout,
            sampler,
            frames: Vec::new(),
            slots: Vec::new(),
            silver,
            held: Vec::new(),
            echoes: Vec::new(),
            vertices: None,
            ranges: Default::default(),
            plan: Plan::default(),
            camera: Camera::default(),
        }
    }
    pub fn settings(&self) -> ReflectionSettings {
        self.settings
    }
    /// A new setting frees or grows the reflection textures on the next
    /// frame that needs them.
    pub fn set_settings(&mut self, settings: ReflectionSettings) {
        if settings != self.settings {
            self.settings = settings;
            self.targets.clear();
            self.slots.clear();
        }
    }
    pub fn matches(&self, format: wgpu::TextureFormat, samples: u32) -> bool {
        self.format == format && self.samples == samples
    }
    /// This frame's plan, after `prepare`.
    pub fn plan(&self) -> &Plan {
        &self.plan
    }
    /// Live planes this frame.
    pub fn live(&self) -> usize {
        self.plan.planes.len()
    }
    /// Plan the frame and upload what it draws: `camera` is the player's
    /// (as last given to `update_camera`) and `screen` the world target's
    /// size. Gives the renderer a view per live plane.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut SceneRenderer,
        camera: &Camera,
        screen: (u32, u32),
        mirrors: &[Mirror],
    ) -> Result<()> {
        ensure!(screen.0 > 0 && screen.1 > 0, "Empty mirror screen");
        let settings = self.settings;
        let size = (
            ((screen.0 as f32 * settings.scale).ceil() as u32).clamp(1, screen.0),
            ((screen.1 as f32 * settings.scale).ceil() as u32).clamp(1, screen.1),
        );
        if size != self.size {
            self.size = size;
            self.targets.clear();
            self.slots.clear();
        }
        // What each kept target drew last frame, shown a frame late.
        self.held = (0..self.targets.len())
            .map(|i| {
                let plane = self.plan.planes.get(i)?;
                Some((self.plan.group_keys[plane.group].clone(), *self.echoes.get(i)?))
            })
            .collect();
        let eye = Vec4::from(camera.eye).truncate();
        self.plan = plan(
            mirrors,
            Mat4::from_cols_array(&camera.view_projection),
            eye,
            &settings,
            self.size,
        );
        self.camera = *camera;
        self.plan.last = self
            .held
            .iter()
            .enumerate()
            .filter_map(|(k, h)| Some((h.as_ref()?.0.clone(), k)))
            .collect();
        let live = self.plan.planes.len();
        // Textures only once a mirror is live, then kept for the setting.
        while self.targets.len() < live {
            self.targets.push(self.target(device));
            self.slots.clear();
        }
        // Each target shows live, then silver, then each target echoed,
        // then each target's last picture.
        let kept = self.targets.len();
        if self.slots.len() != 3 * kept + 1 {
            let pictures = || self.targets.iter().map(|t| &t.picture);
            self.slots = pictures()
                .chain(std::iter::once(&self.silver))
                .chain(pictures())
                .chain(self.targets.iter().map(|t| &t.previous))
                .map(|view| self.bound_slot(device, view))
                .collect();
        }
        self.echoes.clear();
        let target = [self.size.0 as f32, self.size.1 as f32, 0.0, 0.0];
        for i in 0..kept {
            let (live, echo) = match self.plan.planes.get(i) {
                Some(plane) => {
                    let (reproject, parent) = match plane.parent.checked_sub(1) {
                        None => (
                            Mat4::from_cols_array(&camera.view_projection),
                            [0.0, 0.0, target[0], target[1]],
                        ),
                        Some(p) => {
                            let seen_from = &self.plan.planes[p];
                            (seen_from.view_projection, seen_from.viewport)
                        }
                    };
                    let flip = if plane.flipped { 1.0 } else { 0.0 };
                    let uniform = |mode: f32| SlotUniform {
                        reproject: reproject.to_cols_array(),
                        sample: [plane.mirror_u, mode, flip, 0.0],
                        parent,
                        viewport: plane.viewport,
                        target,
                    };
                    (uniform(1.0), uniform(2.0))
                }
                // A kept texture no plane uses this frame: silver.
                None => (SlotUniform::silver(), SlotUniform::silver()),
            };
            queue.write_buffer(&self.slots[i].buffer, 0, bytemuck::bytes_of(&live));
            queue.write_buffer(&self.slots[kept + 1 + i].buffer, 0, bytemuck::bytes_of(&echo));
            if i < self.plan.planes.len() {
                self.echoes.push(echo);
            }
            let last = match self.held.get(i) {
                Some(Some((_, held))) => *held,
                _ => SlotUniform::silver(),
            };
            queue.write_buffer(&self.slots[2 * kept + 1 + i].buffer, 0, bytemuck::bytes_of(&last));
        }
        renderer.set_view_count(device, 1 + live);
        self.grow_frames(device, 1 + live);
        let frame = |view_projection: Mat4, eye: Vec3, size: (u32, u32)| {
            frame_uniform(camera, view_projection, eye, size)
        };
        queue.write_buffer(
            &self.frames[0].buffer,
            0,
            bytemuck::bytes_of(&frame(
                Mat4::from_cols_array(&camera.view_projection),
                eye,
                screen,
            )),
        );
        for (i, plane) in self.plan.planes.iter().enumerate() {
            queue.write_buffer(
                &self.frames[1 + i].buffer,
                0,
                bytemuck::bytes_of(&frame(plane.view_projection, plane.eye, self.size)),
            );
            renderer.update_view(
                queue,
                1 + i,
                &Camera {
                    view_projection: plane.view_projection.to_cols_array(),
                    eye: plane.eye.extend(1.0).to_array(),
                    ..*camera
                },
            );
        }
        // Surfaces by pipeline, then by coplanar group.
        let mut vertices = Vec::new();
        for (p, ranges) in self.ranges.iter_mut().enumerate() {
            ranges.clear();
            for members in &self.plan.groups {
                let start = vertices.len() as u32;
                for &i in members {
                    let mirror = &mirrors[i];
                    if usize::from(mirror.strength < 1.0) != p {
                        continue;
                    }
                    let tint = [mirror.tint[0], mirror.tint[1], mirror.tint[2], mirror.strength];
                    let fallback = [mirror.fallback[0], mirror.fallback[1], mirror.fallback[2], 1.0];
                    for [a, b, c] in surface_triangles(mirror) {
                        for position in [a, b, c] {
                            vertices.push(MirrorVertex {
                                position: position.to_array(),
                                tint,
                                fallback,
                            });
                        }
                    }
                }
                ranges.push(start..vertices.len() as u32);
            }
        }
        if !vertices.is_empty() {
            let bytes: &[u8] = bytemuck::cast_slice(&vertices);
            if self
                .vertices
                .as_ref()
                .is_none_or(|b| b.size() < bytes.len() as u64)
            {
                self.vertices = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("mirror surfaces"),
                    contents: &vec![0; bytes.len().next_power_of_two()],
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                }));
            }
            if let Some(buffer) = &self.vertices {
                queue.write_buffer(buffer, 0, bytes);
            }
        }
        Ok(())
    }
    fn target(&self, device: &wgpu::Device) -> Target {
        let make = |label, samples, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: self.size.0,
                    height: self.size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let view = |t: &wgpu::Texture| t.create_view(&Default::default());
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let sampled = wgpu::TextureUsages::TEXTURE_BINDING;
        let picture = make(
            "mirror reflection",
            1,
            self.format,
            attachment | sampled | wgpu::TextureUsages::COPY_SRC,
        );
        let previous = make(
            "mirror reflection, frame before",
            1,
            self.format,
            sampled | wgpu::TextureUsages::COPY_DST,
        );
        Target {
            color: (self.samples > 1).then(|| {
                view(&make("mirror reflection samples", self.samples, self.format, attachment))
            }),
            picture: view(&picture),
            picture_texture: picture,
            previous: view(&previous),
            previous_texture: previous,
            depth: view(&make("mirror reflection depth", self.samples, DEPTH_FORMAT, attachment)),
        }
    }
    fn bound_slot(&self, device: &wgpu::Device, picture: &wgpu::TextureView) -> Bound {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirror reflection"),
            contents: bytemuck::bytes_of(&SlotUniform::silver()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirror reflection"),
            layout: &self.slot_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(picture),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        Bound { buffer, group }
    }
    /// Draw each live plane's reflection: the world from its reflected view,
    /// the planes seen in it (drawn first) on their mirrors and other
    /// mirrors in silver. `scenes` and `instances` are what the
    /// mirrors may show (the player's own body even in first person);
    /// `after` records last in each plane's pass, given its view (1 + the
    /// plane's index), for what draws outside the scene renderer.
    pub fn render(
        &self,
        renderer: &SceneRenderer,
        encoder: &mut wgpu::CommandEncoder,
        scenes: &[&GpuScene],
        instances: &[(&GpuScene, &GpuInstances)],
        clear: wgpu::Color,
        after: &dyn Fn(&mut wgpu::RenderPass<'_>, usize),
    ) {
        // Keep last frame's pictures before this frame draws over them.
        for (target, held) in self.targets.iter().zip(&self.held) {
            if held.is_some() {
                encoder.copy_texture_to_texture(
                    target.picture_texture.as_image_copy(),
                    target.previous_texture.as_image_copy(),
                    target.picture_texture.size(),
                );
            }
        }
        // A plane is planned after the view it is seen in: deepest first.
        for (i, plane) in self.plan.planes.iter().enumerate().rev() {
            let Some(target) = self.targets.get(i) else {
                continue;
            };
            let surfaces = |pass: &mut wgpu::RenderPass<'_>| self.draw_surfaces(pass, 1 + i);
            let late = |pass: &mut wgpu::RenderPass<'_>| after(pass, 1 + i);
            renderer.render_world(
                encoder,
                WorldPass {
                    view: 1 + i,
                    color: target.color.as_ref().unwrap_or(&target.picture),
                    resolve: target.color.as_ref().map(|_| &target.picture),
                    depth: &target.depth,
                    viewport: Some(plane.viewport),
                    clear: Some(clear),
                    after_opaque: Some(&surfaces),
                    after_all: Some(&late),
                },
                scenes,
                instances,
            );
        }
    }
    fn grow_frames(&mut self, device: &wgpu::Device, count: usize) {
        while self.frames.len() < count {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mirror frame"),
                size: std::mem::size_of::<FrameUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mirror frame"),
                layout: &self.frame_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            self.frames.push(Bound { buffer, group });
        }
    }
    /// Mirror surfaces seen from another pass's view past the live planes
    /// (an environment probe's face), after [`Self::prepare`]: each mirror
    /// shows its last picture (an echo) or silver, as surfaces past the
    /// passes do. Drawn by [`Self::draw_surfaces`] with the same `view`.
    pub fn prepare_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: usize,
        view_projection: Mat4,
        eye: Vec3,
        size: (u32, u32),
    ) {
        if view <= self.plan.planes.len() {
            return;
        }
        self.grow_frames(device, view + 1);
        let frame = frame_uniform(&self.camera, view_projection, eye, size);
        queue.write_buffer(&self.frames[view].buffer, 0, bytemuck::bytes_of(&frame));
    }
    /// Mirror surfaces as `view` sees them (0 the player's, 1 + i live
    /// plane i's, or a view [`Self::prepare_view`] set), for
    /// `WorldPass::after_opaque`.
    pub fn draw_surfaces(&self, pass: &mut wgpu::RenderPass<'_>, view: usize) {
        let (Some(vertices), Some(frame)) = (&self.vertices, self.frames.get(view)) else {
            return;
        };
        let slots = self.plan.slots(view);
        let kept = self.targets.len();
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_bind_group(0, &frame.group, &[]);
        for (pipeline, ranges) in self.pipelines.iter().zip(&self.ranges) {
            if ranges.iter().all(|r| r.is_empty()) {
                continue;
            }
            pass.set_pipeline(pipeline);
            // Neighbouring groups showing the same slot draw as one run:
            // silver runs broken only by the planes this view sees.
            let mut run: Option<(Shows, Range<u32>)> = None;
            let mut flush = |run: Option<(Shows, Range<u32>)>| {
                if let Some((shows, range)) = run
                    && !range.is_empty()
                    && let Some(bound) = self.slots.get(match shows {
                        Shows::Live(i) => i,
                        Shows::Silver => kept,
                        Shows::Echo(i) => kept + 1 + i,
                        Shows::Last(i) => 2 * kept + 1 + i,
                    })
                {
                    pass.set_bind_group(1, &bound.group, &[]);
                    pass.draw(range, 0..1);
                }
            };
            for (range, slot) in ranges.iter().zip(&slots) {
                match (&mut run, slot) {
                    (Some((current, r)), Some(slot)) if current == slot && r.end == range.start => {
                        r.end = range.end;
                    }
                    (_, Some(slot)) => flush(run.replace((*slot, range.clone()))),
                    (_, None) => flush(run.take()),
                }
            }
            flush(run);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(eye: Vec3, target: Vec3) -> Mat4 {
        let view = glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y);
        crate::scene::perspective(1.2, 16.0 / 9.0, 0.05, 1000.0) * view
    }
    /// A 2x2 mirror in the plane z = 0 facing +z, centred at `x`.
    fn wall(x: f32) -> Mirror {
        Mirror {
            corners: [
                Vec3::new(x - 1.0, -1.0, 0.0),
                Vec3::new(x + 1.0, -1.0, 0.0),
                Vec3::new(x + 1.0, 1.0, 0.0),
                Vec3::new(x - 1.0, 1.0, 0.0),
            ],
            tint: [1.0; 3],
            strength: 1.0,
            looks: Looks::Reflect,
            fallback: SILVER,
            recess: 0.0,
        }
    }

    #[test]
    fn the_reflected_view_sees_the_viewer_where_the_mirror_shows_them() {
        let eye = Vec3::new(0.3, 0.2, 4.0);
        let main = camera(eye, Vec3::ZERO);
        let mirrors = [wall(0.0)];
        let plan = plan(&mirrors, main, eye, &ReflectionSettings::MEDIUM, (960, 540));
        assert_eq!(plan.groups, vec![vec![0]]);
        let plane = plan.planes[0];
        assert_eq!((plane.parent, plane.group), (0, 0));
        assert!(plane.eye.abs_diff_eq(Vec3::new(0.3, 0.2, -4.0), 1e-5));
        // Something in front of the mirror (a head beside the viewer) lands
        // in the reflection where the mirror's surface shows it: the screen
        // point where the eye's ray meets the mirror, flipped across the
        // viewport as the surface shader samples it.
        let head = Vec3::new(-0.2, 0.1, 2.0);
        let image = Vec3::new(head.x, head.y, -head.z);
        let seen = main.project_point3(image);
        let hit = plane.view_projection.project_point3(head);
        let [x, y, w, h] = plane.viewport;
        let texel = Vec3::new((hit.x + 1.0) * 0.5 * w + x, (1.0 - hit.y) * 0.5 * h + y, 0.0);
        let screen_u = (seen.x + 1.0) * 0.5;
        let sampled_u = plane.mirror_u - screen_u;
        assert!((texel.x / 960.0 - sampled_u).abs() < 1e-3, "{texel} {sampled_u}");
        assert!((texel.y / 540.0 - (1.0 - seen.y) * 0.5).abs() < 1e-3);
        // Behind the mirror clips at its near plane; in front stays in.
        assert!(hit.z > 0.0 && hit.z < 1.0);
        let behind = plane.view_projection * Vec4::new(0.0, 0.0, -1.0, 1.0);
        assert!(behind.z > behind.w);
        // Winding is kept, so the reflection draws with the usual culling: a
        // face turned to the mirror is front-facing in it, as one turned to
        // the viewer is on screen.
        let facing = |z: f32| {
            let (a, b) = (Vec3::new(0., 0., 2.), Vec3::new(0.2, 0., 2.));
            [a, b, Vec3::new(0., 0.2 * z, 2.)]
        };
        let area = |m: Mat4, tri: [Vec3; 3]| {
            let [a, b, c] = tri.map(|p| m.project_point3(p));
            (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
        };
        assert!(area(main, facing(1.0)) > 0.0);
        assert!(area(plane.view_projection, facing(-1.0)) > 0.0);
        assert!(area(plane.view_projection, facing(1.0)) < 0.0);
    }

    #[test]
    fn a_window_shows_the_view_out_of_its_partner_unflipped() {
        // A window at z = 0 facing +z whose partner stands 10 along x: what
        // lies behind the partner (z < 0 there) shows behind the window.
        let eye = Vec3::new(0.3, 0.2, 4.0);
        let main = camera(eye, Vec3::ZERO);
        let through = Mat4::from_translation(Vec3::new(-10.0, 0.0, 0.0));
        let window = Mirror {
            looks: Looks::Through(through),
            fallback: [0.2; 3],
            ..wall(0.0)
        };
        // Far past the mirrors' distance it still shows, not its colour.
        let far = ReflectionSettings {
            distance: 1.0,
            ..ReflectionSettings::LOW
        };
        assert_eq!(plan(&[window], main, eye, &far, (960, 540)).planes.len(), 1);
        let plan = plan(&[window], main, eye, &ReflectionSettings::LOW, (960, 540));
        let plane = plan.planes[0];
        assert!(!plane.flipped);
        assert!(plane.eye.abs_diff_eq(Vec3::new(10.3, 0.2, 4.0), 1e-5));
        assert!(plane.clip.abs_diff_eq(Vec4::new(0.0, 0.0, -1.0, 0.0), 1e-5));
        // Something beyond the partner lands where the window shows it: the
        // same screen position, not mirrored.
        let head = Vec3::new(10.2, 0.1, -2.0);
        let seen = main.project_point3(head - Vec3::new(10.0, 0.0, 0.0));
        let hit = plane.view_projection.project_point3(head);
        let [x, y, w, h] = plane.viewport;
        let texel = Vec3::new((hit.x + 1.0) * 0.5 * w + x, (1.0 - hit.y) * 0.5 * h + y, 0.0);
        assert!((texel.x / 960.0 - (seen.x + 1.0) * 0.5).abs() < 1e-3, "{texel} {seen}");
        assert!((texel.y / 540.0 - (1.0 - seen.y) * 0.5).abs() < 1e-3);
        assert!(hit.z > 0.0 && hit.z < 1.0);
        // What stands in front of the partner is not seen through it.
        let before = plane.view_projection * Vec4::new(10.0, 0.0, 1.0, 1.0);
        assert!(before.z > before.w);
        // Its window draws in its own view on the clip plane: not at all.
        assert_eq!(plan.slots(1), vec![None]);
        // A window with nowhere to look only shows its colour.
        let plain = Mirror {
            looks: Looks::Plain,
            ..window
        };
        let p = super::plan(&[plain], main, eye, &ReflectionSettings::HIGH, (960, 540));
        assert!(p.planes.is_empty() && p.drawn == vec![0]);
        // Windows onto different places never share a pass.
        let other = Mirror {
            looks: Looks::Through(Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0))),
            ..wall(2.0)
        };
        let p = super::plan(&[window, other], main, eye, &ReflectionSettings::HIGH, (960, 540));
        assert_eq!(p.groups.len(), 2);
    }

    /// Two linked windows (portals) at `a` and `b`, facing `+z` and turned
    /// by `turn` about y: each shows what lies past the other, as
    /// `bri_client::mirrors` builds them from a brick's `link`.
    fn portal_pair(a: Vec3, b: Vec3, turn: f32) -> [Mirror; 2] {
        let pose = |at: Vec3, turn: f32| Mat4::from_translation(at) * Mat4::from_rotation_y(turn);
        let (pa, pb) = (pose(a, 0.0), pose(b, turn));
        let face = |pose: Mat4| {
            let quad = wall(0.0).corners.map(|c| pose.transform_point3(c));
            Mirror {
                corners: quad,
                fallback: [0.35, 0.42, 0.55],
                ..wall(0.0)
            }
        };
        // Going in one comes out of the other's face, turned half about y:
        // `carry` = partner * half turn * self⁻¹, shown by its inverse.
        let half = Mat4::from_rotation_y(std::f32::consts::PI);
        let carry = |from: Mat4, to: Mat4| to * half * from.inverse();
        [
            Mirror {
                looks: Looks::Through(carry(pa, pb).inverse()),
                ..face(pa)
            },
            Mirror {
                looks: Looks::Through(carry(pb, pa).inverse()),
                ..face(pb)
            },
        ]
    }

    #[test]
    fn a_view_never_draws_with_the_picture_it_is_drawing() {
        // Max's crash: portals near each other, one seen in its own view,
        // sampled its own target while drawing into it. Every placement,
        // turn and setting: no view's surfaces show its own picture.
        let settings = [
            ReflectionSettings::LOW,
            ReflectionSettings::MEDIUM,
            ReflectionSettings::HIGH,
        ];
        let mut views = 0;
        for (b, turn) in [
            (Vec3::new(3.0, 0.0, 0.0), 0.0),
            (Vec3::new(3.0, 0.0, -2.0), 0.0),
            (Vec3::new(3.0, 0.0, 2.0), 0.0),
            (Vec3::new(4.0, 0.0, 1.0), std::f32::consts::FRAC_PI_2),
            (Vec3::new(-4.0, 0.0, 1.0), -std::f32::consts::FRAC_PI_2),
            (Vec3::new(0.0, 0.0, 6.0), std::f32::consts::PI),
        ] {
            let pair = portal_pair(Vec3::ZERO, b, turn);
            for eye in [
                Vec3::new(1.5, 0.3, 5.0),
                Vec3::new(-2.0, 0.5, 3.0),
                Vec3::new(0.2, 0.1, 0.5),
            ] {
                let main = camera(eye, (Vec3::ZERO + b) * 0.5);
                for settings in &settings {
                    let p = plan(&pair, main, eye, settings, (960, 540));
                    for (i, _) in p.planes.iter().enumerate() {
                        views += 1;
                        for slot in p.slots(1 + i) {
                            assert!(
                                !matches!(slot, Some(Shows::Live(j) | Shows::Echo(j)) if j == i),
                                "view {} shows its own picture: {:?}",
                                1 + i,
                                p.slots(1 + i)
                            );
                        }
                    }
                }
            }
        }
        assert!(views > 20, "only {views} live views checked");
    }

    #[test]
    fn a_window_the_eye_is_passing_through_stays_live_past_the_near_plane() {
        // Max's flicker: halfway through, the eye closer to the window than
        // the near plane (0.05), its quad clipped away; the window lost its
        // pass and its recess showed the flat idle colour over the screen.
        let [window, _] = portal_pair(Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), 0.0);
        for distance in [0.3, 0.1, 0.04, 0.01, 0.001, -0.0005] {
            let eye = Vec3::new(0.2, 0.3, distance);
            let main = camera(eye, eye + Vec3::NEG_Z);
            let recessed = Mirror {
                recess: 0.2,
                ..window
            };
            let p = plan(&[recessed], main, eye, &ReflectionSettings::LOW, (960, 540));
            assert_eq!(p.planes.len(), 1, "no live view {distance} from the window");
            let plane = p.planes[0];
            // Covering the whole screen, and the view's eye behind the
            // plane it is clipped at.
            assert_eq!(plane.viewport, [0.0, 0.0, 960.0, 540.0], "{distance}");
            assert!(plane.clip.xyz().dot(plane.eye) + plane.clip.w <= -CLIP_CLEARANCE + 1e-5);
            assert!(plane.view_projection.is_finite());
        }
    }

    #[test]
    fn a_recessed_window_covers_the_screen_its_quad_does() {
        let window = Mirror {
            recess: 0.3,
            ..wall(0.0)
        };
        let triangles = surface_triangles(&window);
        assert_eq!(triangles.len(), 10);
        // Every triangle faces the opening and lies behind it.
        for [a, b, c] in &triangles {
            let normal = (b - a).cross(c - b);
            let centre = (*a + *b + *c) / 3.0;
            assert!(normal.dot(Vec3::new(0.0, 0.0, 1.0) - centre) > 0.0, "{a} {b} {c}");
            assert!(a.z <= 0.0 && b.z <= 0.0 && c.z <= 0.0);
        }
        assert_eq!(surface_triangles(&wall(0.0)).len(), 2);
    }

    #[test]
    fn coplanar_mirrors_share_a_plane_and_the_largest_planes_go_live() {
        let eye = Vec3::new(0.0, 0.0, 6.0);
        let main = camera(eye, Vec3::ZERO);
        // Three bricks of one wall, and a small mirror on a nearer plane.
        let mut small = wall(0.0);
        for corner in &mut small.corners {
            *corner = *corner * 0.1 + Vec3::new(2.0, 0.0, 1.0);
        }
        let mirrors = [wall(-2.0), wall(0.0), wall(2.0), small];
        let one = ReflectionSettings {
            planes: 1,
            ..ReflectionSettings::MEDIUM
        };
        let p = plan(&mirrors, main, eye, &one, (960, 540));
        assert_eq!(p.planes.len(), 1);
        assert_eq!(p.groups, vec![vec![0, 1, 2], vec![3]]);
        assert_eq!(p.slots(0), vec![Some(Shows::Live(0)), Some(Shows::Silver)]);
        assert_eq!(p.drawn, vec![0, 1, 2, 3]);
        let p = plan(&mirrors, main, eye, &ReflectionSettings::MEDIUM, (960, 540));
        assert_eq!((p.planes[1].parent, p.planes[1].group), (0, 1));
        // Off, out of reach, behind the viewer or seen from behind: silver.
        let off = plan(&mirrors, main, eye, &ReflectionSettings::OFF, (960, 540));
        assert!(off.planes.is_empty() && off.drawn.len() == 4);
        let far = ReflectionSettings {
            distance: 3.0,
            ..ReflectionSettings::MEDIUM
        };
        assert!(plan(&mirrors, main, eye, &far, (960, 540)).planes.is_empty());
        let behind = Vec3::new(0.0, 0.0, -6.0);
        let back = camera(behind, Vec3::new(0.0, 0.0, -20.0));
        assert!(
            plan(&mirrors, back, behind, &ReflectionSettings::MEDIUM, (960, 540))
                .planes
                .is_empty()
        );
        let facing = camera(behind, Vec3::ZERO);
        assert!(
            plan(&mirrors[..3], facing, behind, &ReflectionSettings::MEDIUM, (960, 540))
                .planes
                .is_empty()
        );
    }

    #[test]
    fn facing_mirrors_show_each_other_a_bounce_deeper_within_the_passes() {
        // The viewer between two facing mirrors looks at one (z = 0); the
        // other (z = 6, facing back) is behind them, seen only in the first.
        let eye = Vec3::new(0.3, 0.2, 4.0);
        let main = camera(eye, Vec3::ZERO);
        let mut back = wall(0.0);
        back.corners = [0, 3, 2, 1].map(|i| wall(0.0).corners[i] * 2.0 + Vec3::new(0.0, 0.0, 6.0));
        let mirrors = [wall(0.0), back];
        let p = plan(&mirrors, main, eye, &ReflectionSettings::MEDIUM, (960, 540));
        assert_eq!(p.planes.len(), 2);
        let (first, second) = (p.planes[0], p.planes[1]);
        assert_eq!((first.parent, first.group, second.parent, second.group), (0, 0, 1, 1));
        // Twice reflected: the eye behind the far mirror, as far as the
        // light travelled.
        assert!(second.eye.abs_diff_eq(Vec3::new(0.3, 0.2, 16.0), 1e-4));
        // The player's view shows the first plane (the back mirror is off
        // screen); the first plane's view shows the second, and the
        // second's shows the first mirror as it last looked, out of passes.
        assert_eq!(p.slots(0), vec![Some(Shows::Live(0)), Some(Shows::Echo(1))]);
        assert_eq!(p.slots(1), vec![None, Some(Shows::Live(1))]);
        assert_eq!(p.slots(2), vec![Some(Shows::Echo(0)), None]);
        // A head between the mirrors, seen by way of the back mirror and
        // then the front one, lands where the two surfaces sample it.
        let head = Vec3::new(-0.2, 0.1, 5.0);
        let image = reflection_matrix(first.plane)
            .transform_point3(reflection_matrix(second.plane).transform_point3(head));
        let seen = main.project_point3(image);
        let texel = |plane: &PlannedPlane, point: Vec3| {
            let hit = plane.view_projection.project_point3(point);
            let [x, y, w, h] = plane.viewport;
            ((hit.x + 1.0) * 0.5 * w + x, (1.0 - hit.y) * 0.5 * h + y)
        };
        let screen_x = (seen.x + 1.0) * 0.5 * 960.0;
        let in_first = first.mirror_u * 960.0 - screen_x;
        let in_second = second.mirror_u * 960.0 - in_first;
        let (x, y) = texel(&second, head);
        assert!((x - in_second).abs() < 0.5, "{x} {in_second}");
        assert!((y - (1.0 - seen.y) * 0.5 * 540.0).abs() < 0.5);
        // Billboards in the second view face its eye.
        assert!(second.reflect_direction(Vec3::Z).abs_diff_eq(Vec3::Z, 1e-5));
        assert!(first.reflect_direction(Vec3::Z).abs_diff_eq(Vec3::NEG_Z, 1e-5));
        // With one pass the back mirror stays silver.
        let low = plan(&mirrors, main, eye, &ReflectionSettings::LOW, (960, 540));
        assert_eq!(low.planes.len(), 1);
        assert_eq!(low.slots(1), vec![None, Some(Shows::Silver)]);
        // Past the passes it shows its picture from the frame before, when
        // a target kept one (Max, v0.1.11: a portal in another's view was
        // a flat light blue), never one being drawn this frame.
        let mut low = low;
        low.last = vec![(low.group_keys[1].clone(), 0)];
        assert_eq!(low.slots(1), vec![None, Some(Shows::Last(0))]);
        assert_eq!(low.slots(0), vec![Some(Shows::Live(0)), Some(Shows::Last(0))]);
    }

    #[test]
    fn a_mirror_the_eye_stands_beside_covers_only_its_part_of_the_screen() {
        let eye = Vec3::new(0.0, 0.0, 3.0);
        // Looking along the wall's right half at an angle: around a corner.
        let main = camera(eye, Vec3::new(-3.0, 0.0, 0.0));
        let plan = plan(&[wall(-2.0)], main, eye, &ReflectionSettings::MEDIUM, (960, 540));
        let [x, _, w, _] = plan.planes[0].viewport;
        assert!(w < 960.0 && x + w <= 960.0);
        assert!(plan.planes[0].view_projection.is_finite());
    }

    #[test]
    fn bad_mirrors_are_left_out() {
        let mut flat = wall(0.0);
        flat.corners[2] = flat.corners[1];
        flat.corners[3] = flat.corners[0];
        let mut nan = wall(0.0);
        nan.corners[0].x = f32::NAN;
        let clear = Mirror {
            strength: 0.0,
            ..wall(0.0)
        };
        let eye = Vec3::new(0.0, 0.0, 4.0);
        let plan = plan(
            &[flat, nan, clear],
            camera(eye, Vec3::ZERO),
            eye,
            &ReflectionSettings::HIGH,
            (100, 100),
        );
        assert!(plan.drawn.is_empty() && plan.planes.is_empty());
    }
}
