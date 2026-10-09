//! Item icons drawn from the item's own model, on each player's machine.
//!
//! An Add-On item may ask for its icon to be rendered rather than shipped:
//! `<icon>.render.json` beside the icon it names (see
//! `docs/modding/weapons.md`). The icon is drawn from the item's model as
//! the game has it, posed and framed exactly like a stock item's icon
//! (`pose_like`): the pose is found by fitting that item's model to the
//! silhouette of its icon, so the new icon sits in the tool slots at the
//! same angle and size as the stock ones. Nothing is shipped but the
//! request; the picture is made from the player's own game files.
//!
//! The look is the model's own colour, lit, with an optional veined skin
//! over it: the Gravity Gun's alien shell (`gravity-gun-tool/assets/skins/
//! alien.wgsl`), ported here so the icon matches the gun as drawn in play.
//! All CPU, deterministic, and small: icons are 128 pixels or less.
//!
//! Drawing one takes a noticeable part of a second (most of it finding the
//! stock icon's pose), so a drawn icon is kept on disk, named by a hash of
//! everything it is drawn from (`Request::digest`), and the game draws it
//! off the load path (`crate::items::ItemAssets::draw_icons`).
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use bri_render::scene::{MaterialKind, SceneData, SceneImage};
use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// `<icon>.render.json`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub schema_version: u32,
    /// The stock item whose icon's pose and framing this one takes.
    pub pose_like: String,
    /// Clockwise screen-space quarter turns after stock-profile framing.
    /// Changes icon orientation without changing the model or its lighting.
    #[serde(default, skip_serializing_if = "zero_turns")]
    pub clockwise_quarter_turns: u8,
    #[serde(default)]
    pub look: Look,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    /// The model's own colour, seen where no skin is: by default the
    /// item's colour in play (its image's tint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<[f32; 3]>,
    /// Draw the model's own textures and colours (times `base`) rather than
    /// `base` alone: a tool of wood and iron shows both.
    #[serde(default)]
    pub textured: bool,
    #[serde(default)]
    pub skin: Option<Skin>,
}

/// A dark shell with an oil-slick sheen and glowing veins, puffed out a
/// little along the model's normals like the in-game skin.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Skin {
    #[serde(default = "shell")]
    pub shell: [f32; 3],
    /// By default the colour of the item's skin in play (`looks.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub veins: Option<[f32; 3]>,
    #[serde(default = "puff")]
    pub puff: f32,
}

fn zero_turns(turns: &u8) -> bool {
    *turns == 0
}

fn shell() -> [f32; 3] {
    [0.035, 0.025, 0.05]
}
fn puff() -> f32 {
    0.012
}

impl Spec {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let spec: Self = serde_json::from_slice(bytes)?;
        ensure!(
            spec.schema_version == 1,
            "unknown icon render schema {}",
            spec.schema_version
        );
        ensure!(!spec.pose_like.is_empty(), "pose_like names no item");
        ensure!(
            spec.clockwise_quarter_turns < 4,
            "icon clockwise_quarter_turns must be 0 to 3"
        );
        let colours = spec.look.base.iter().flatten().chain(
            spec.look
                .skin
                .iter()
                .flat_map(|s| s.shell.iter().chain(s.veins.iter().flatten())),
        );
        ensure!(
            colours
                .into_iter()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(c)),
            "icon render colours must be 0 to 1"
        );
        ensure!(
            spec.look
                .skin
                .as_ref()
                .is_none_or(|s| s.puff.is_finite() && (0.0..=0.1).contains(&s.puff)),
            "skin puff must be 0 to 0.1"
        );
        Ok(spec)
    }
    /// Colours the request leaves out, from the item's look in play: its
    /// colour, and its skin's colour (`crate::items::Appearance`).
    pub fn with_defaults(mut self, base: [f32; 3], veins: Option<[f32; 3]>) -> Self {
        self.look.base.get_or_insert(base);
        if let Some(skin) = &mut self.look.skin
            && skin.veins.is_none()
        {
            skin.veins = veins;
        }
        self
    }
}

/// A model's triangles in its own space: positions and per-vertex normals,
/// what colours its surface (texture coordinates, vertex colours and the
/// texture each triangle draws with), and which way the item points.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub indices: Vec<u32>,
    pub uvs: Vec<Vec2>,
    pub colors: Vec<Vec3>,
    /// Per triangle: its texture in `images`, if any, and whether the
    /// texture is paint over the vertex colour (a v20 opaque material,
    /// `MaterialKind::BrickOverlay`) rather than multiplied with it.
    pub triangle_images: Vec<Option<(usize, bool)>>,
    pub images: Vec<SceneImage>,
    pub axes: Axes,
}

/// Which way an item model points, in its own space. Item models point
/// down +Y with +Z up, as they sit in the hand (`mountPoint`); one with a
/// `muzzlePoint` points from its mount to its muzzle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axes {
    pub forward: Vec3,
    pub up: Vec3,
}

impl Default for Axes {
    fn default() -> Self {
        Self {
            forward: Vec3::Y,
            up: Vec3::Z,
        }
    }
}

impl Axes {
    /// Forward along `forward` with `up` made square to it; the default
    /// where they do not make a direction.
    pub fn new(forward: Vec3, up: Vec3) -> Self {
        let forward = forward.normalize_or_zero();
        let up = (up - forward * up.dot(forward)).normalize_or_zero();
        if forward == Vec3::ZERO || up == Vec3::ZERO || !forward.is_finite() || !up.is_finite() {
            return Self::default();
        }
        Self { forward, up }
    }
}

impl Mesh {
    pub fn from_scene(scene: &SceneData) -> Self {
        let mut mesh = Self::from_scene_tinted(scene);
        // An untinted texture is not multiplied by the model's colour
        // (`bri_render::scene::Material::untinted`).
        for batch in &scene.batches {
            if scene
                .materials
                .get(batch.material)
                .is_some_and(|m| m.untinted)
            {
                for &i in scene
                    .indices
                    .get(batch.indices.start as usize..batch.indices.end as usize)
                    .unwrap_or(&[])
                {
                    if let Some(c) = mesh.colors.get_mut(i as usize) {
                        *c = Vec3::ONE;
                    }
                }
            }
        }
        mesh
    }
    fn from_scene_tinted(scene: &SceneData) -> Self {
        let mut triangle_images = vec![None; scene.indices.len() / 3];
        for batch in &scene.batches {
            let image = scene.materials.get(batch.material).map(|m| {
                let overlay = matches!(
                    m.kind,
                    MaterialKind::BrickOverlay | MaterialKind::UnlitOverlay
                );
                (m.images[0], overlay)
            });
            let image = image.filter(|(i, _)| *i < scene.images.len());
            let end = (batch.indices.end as usize / 3).min(triangle_images.len());
            let start = (batch.indices.start as usize / 3).min(end);
            triangle_images[start..end].fill(image);
        }
        Self {
            positions: scene
                .vertices
                .iter()
                .map(|v| Vec3::from(v.position))
                .collect(),
            normals: scene
                .vertices
                .iter()
                .map(|v| Vec3::from(v.normal).normalize_or_zero())
                .collect(),
            indices: scene.indices.clone(),
            uvs: scene.vertices.iter().map(|v| Vec2::from(v.uv)).collect(),
            colors: scene
                .vertices
                .iter()
                .map(|v| Vec3::new(v.color[0], v.color[1], v.color[2]))
                .collect(),
            triangle_images,
            images: scene.images.clone(),
            axes: Axes::default(),
        }
    }
    /// Each whole triangle, with its number.
    fn triangles(&self) -> impl Iterator<Item = (usize, [usize; 3])> + '_ {
        self.indices
            .chunks_exact(3)
            .enumerate()
            .filter_map(|(n, t)| {
                let t = [t[0] as usize, t[1] as usize, t[2] as usize];
                t.iter()
                    .all(|&i| i < self.positions.len())
                    .then_some((n, t))
            })
    }
    /// The surface's own colour at barycentric `bary` of triangle `n`: its
    /// texture (nearest texel, repeating) over or times its vertex colours,
    /// as the game draws it. White where the model has neither.
    fn surface(&self, n: usize, t: [usize; 3], bary: Vec3) -> Vec3 {
        let blend = |v: &[Vec3]| {
            if t.iter().all(|&i| i < v.len()) {
                v[t[0]] * bary.x + v[t[1]] * bary.y + v[t[2]] * bary.z
            } else {
                Vec3::ONE
            }
        };
        let tint = blend(&self.colors);
        let Some((image, overlay)) = self.triangle_images.get(n).copied().flatten() else {
            return tint;
        };
        let image = &self.images[image];
        if !t.iter().all(|&i| i < self.uvs.len()) || image.width == 0 || image.height == 0 {
            return tint;
        }
        let uv = self.uvs[t[0]] * bary.x + self.uvs[t[1]] * bary.y + self.uvs[t[2]] * bary.z;
        let (w, h) = (image.width as usize, image.height as usize);
        let x = ((uv.x.rem_euclid(1.0) * w as f32) as usize).min(w - 1);
        let y = ((uv.y.rem_euclid(1.0) * h as f32) as usize).min(h - 1);
        let texel = &image.rgba[(y * w + x) * 4..(y * w + x) * 4 + 4];
        let colour = Vec3::new(texel[0] as f32, texel[1] as f32, texel[2] as f32) / 255.0;
        if overlay {
            tint.lerp(colour, texel[3] as f32 / 255.0)
        } else {
            tint * colour
        }
    }
}

/// How an icon shows its model: turned by `rotation`, then drawn looking
/// down -Z with `scale` pixels a unit, the model's origin at `centre`
/// (pixels from the top left, y down), on a `size` picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub rotation: Quat,
    pub scale: f32,
    pub centre: Vec2,
    pub size: [u32; 2],
}

impl Pose {
    fn project(&self, p: Vec3) -> Vec3 {
        let q = self.rotation * p;
        Vec3::new(
            self.centre.x + q.x * self.scale,
            self.centre.y - q.y * self.scale,
            q.z,
        )
    }
}

#[cfg(test)]
fn euler(yaw: f32, pitch: f32, roll: f32) -> Quat {
    Quat::from_rotation_z(roll) * Quat::from_rotation_x(pitch) * Quat::from_rotation_y(yaw)
}

/// How a stock item icon shows its item: a side profile, the item's forward
/// across the picture (to the right or the left, `side` 1 or -1) and its up
/// up the picture, then tipped in the picture by `roll` and turned a little
/// towards or away from the camera by `yaw` (about the picture's up) and
/// `pitch` (about its across), all radians. An item posed by the same
/// profile points the same way in its slot whatever its model's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub side: f32,
    pub roll: f32,
    pub yaw: f32,
    pub pitch: f32,
}

/// Most a profile tips or turns, radians: past this an item is no longer
/// seen side on, as no stock icon shows one.
const MOST_ROLL: f32 = 1.05;
const MOST_TURN: f32 = 0.8;

impl Profile {
    /// The turn that shows a model with `axes` this way (view space: +X
    /// right, +Y up, +Z toward the viewer).
    pub fn rotation(&self, axes: Axes) -> Quat {
        let right = axes.forward.cross(axes.up);
        let model = glam::Mat3::from_cols(axes.forward, axes.up, right);
        let view = glam::Mat3::from_cols(Vec3::X * self.side, Vec3::Y, Vec3::Z * self.side);
        let side_on = Quat::from_mat3(&(view * model.transpose()));
        Quat::from_rotation_z(self.roll)
            * Quat::from_rotation_y(self.yaw)
            * Quat::from_rotation_x(self.pitch)
            * side_on
    }
    fn clamped(self) -> Self {
        Self {
            side: self.side,
            roll: self.roll.clamped(-MOST_ROLL, MOST_ROLL),
            yaw: self.yaw.clamped(-MOST_TURN, MOST_TURN),
            pitch: self.pitch.clamped(-MOST_TURN, MOST_TURN),
        }
    }
}

/// Which pixels of a `w` x `h` grid the model covers under `pose`
/// (scaled to that grid).
fn silhouette(mesh: &Mesh, pose: &Pose, w: usize, h: usize) -> Vec<bool> {
    let mut mask = vec![false; w * h];
    let sx = w as f32 / pose.size[0] as f32;
    let sy = h as f32 / pose.size[1] as f32;
    let projected: Vec<Vec2> = mesh
        .positions
        .iter()
        .map(|p| {
            let s = pose.project(*p);
            Vec2::new(s.x * sx, s.y * sy)
        })
        .collect();
    for (_, [a, b, c]) in mesh.triangles() {
        let (a, b, c) = (projected[a], projected[b], projected[c]);
        raster(a, b, c, w, h, |x, y, _| mask[y * w + x] = true);
    }
    mask
}

/// Every pixel centre inside the triangle, with its barycentric weights.
fn raster(a: Vec2, b: Vec2, c: Vec2, w: usize, h: usize, mut f: impl FnMut(usize, usize, Vec3)) {
    let area = (b - a).perp_dot(c - a);
    if area.abs() < 1e-9 || !area.is_finite() {
        return;
    }
    let lo = a.min(b).min(c).max(Vec2::ZERO);
    let hi = a.max(b).max(c).min(Vec2::new(w as f32, h as f32));
    if lo.x >= hi.x || lo.y >= hi.y {
        return;
    }
    for y in (lo.y.floor() as usize)..(hi.y.ceil() as usize).min(h) {
        for x in (lo.x.floor() as usize)..(hi.x.ceil() as usize).min(w) {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let wa = (c - b).perp_dot(p - b) / area;
            let wb = (a - c).perp_dot(p - c) / area;
            let wc = 1.0 - wa - wb;
            if wa >= 0.0 && wb >= 0.0 && wc >= 0.0 {
                f(x, y, Vec3::new(wa, wb, wc));
            }
        }
    }
}

/// The icon's covered pixels (alpha at least half), on a `w` x `h` grid.
fn icon_mask(icon: &SceneImage, w: usize, h: usize) -> Vec<bool> {
    let (iw, ih) = (icon.width as usize, icon.height as usize);
    let mut mask = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let ix = ((x as f32 + 0.5) * iw as f32 / w as f32) as usize;
            let iy = ((y as f32 + 0.5) * ih as f32 / h as f32) as usize;
            mask[y * w + x] = icon.rgba[(iy.min(ih - 1) * iw + ix.min(iw - 1)) * 4 + 3] >= 128;
        }
    }
    mask
}

fn bounds(mask: &[bool], w: usize) -> Option<(Vec2, Vec2)> {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for (i, _) in mask.iter().enumerate().filter(|(_, m)| **m) {
        let p = Vec2::new((i % w) as f32, (i / w) as f32);
        lo = lo.min(p);
        hi = hi.max(p + 1.0);
    }
    (lo.x <= hi.x).then_some((lo, hi))
}

fn overlap(a: &[bool], b: &[bool]) -> f32 {
    let both = a.iter().zip(b).filter(|(a, b)| **a && **b).count();
    let either = a.iter().zip(b).filter(|(a, b)| **a || **b).count();
    if either == 0 {
        0.0
    } else {
        both as f32 / either as f32
    }
}

/// Scale and place the model turned by `rotation` so its outline's box
/// fills the icon's.
fn framed(
    mesh: &Mesh,
    rotation: Quat,
    target: (Vec2, Vec2),
    grid: usize,
    size: [u32; 2],
) -> Option<Pose> {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for p in &mesh.positions {
        let q = rotation * *p;
        let s = Vec2::new(q.x, -q.y);
        lo = lo.min(s);
        hi = hi.max(s);
    }
    let extent = hi - lo;
    if !(extent.x > 1e-6 && extent.y > 1e-6) {
        return None;
    }
    // Grid pixels to icon pixels.
    let to_icon = Vec2::new(size[0] as f32, size[1] as f32) / grid as f32;
    let (tlo, thi) = (target.0 * to_icon, target.1 * to_icon);
    let want = thi - tlo;
    let scale = ((want.x / extent.x) * (want.y / extent.y)).sqrt();
    let centre = (tlo + thi) * 0.5 - (lo + hi) * 0.5 * scale;
    Some(Pose {
        rotation,
        scale,
        centre,
        size,
    })
}

/// The pose that draws `mesh` over `icon`'s silhouette, and the profile
/// it is seen in. Only side profiles are tried (`Profile`), as every stock
/// item icon is one: an outline alone also fits tumbled turns that no icon
/// shows (the Gravity Gun's first icon was one, seen from above and
/// behind). Searched in coarse steps, then refined. `None` when nothing
/// matches well (a model and icon that are not the same thing).
pub fn fit_pose(mesh: &Mesh, icon: &SceneImage) -> Option<(Pose, Profile, f32)> {
    const GRID: usize = 48;
    if icon.width == 0
        || icon.height == 0
        || icon.rgba.len() < (icon.width * icon.height * 4) as usize
    {
        return None;
    }
    let size = [icon.width, icon.height];
    let target_mask = icon_mask(icon, GRID, GRID);
    let target = bounds(&target_mask, GRID)?;
    let score = |profile: Profile| -> Option<(Pose, f32)> {
        let pose = framed(mesh, profile.rotation(mesh.axes), target, GRID, size)?;
        Some((
            pose,
            overlap(&silhouette(mesh, &pose, GRID, GRID), &target_mask),
        ))
    };
    // The coarse search at a quarter of the pixels.
    const COARSE: usize = GRID / 2;
    let coarse_mask = icon_mask(icon, COARSE, COARSE);
    let coarse_target = bounds(&coarse_mask, COARSE)?;
    let rough = |profile: Profile| -> Option<f32> {
        let pose = framed(
            mesh,
            profile.rotation(mesh.axes),
            coarse_target,
            COARSE,
            size,
        )?;
        Some(overlap(
            &silhouette(mesh, &pose, COARSE, COARSE),
            &coarse_mask,
        ))
    };
    // The best few coarse profiles, each refined: an outline can look
    // alike from two far-apart turns, and only refining tells them apart.
    let mut coarse: Vec<(Profile, f32)> = Vec::new();
    let steps = |most: f32, n: i32| (-n..=n).map(move |i| most * i as f32 / n as f32);
    for side in [1.0, -1.0] {
        for roll in steps(MOST_ROLL, 8) {
            for yaw in steps(MOST_TURN, 3) {
                for pitch in steps(MOST_TURN, 3) {
                    let profile = Profile {
                        side,
                        roll,
                        yaw,
                        pitch,
                    };
                    if let Some(s) = rough(profile) {
                        coarse.push((profile, s));
                    }
                }
            }
        }
    }
    coarse.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut best: Option<(Pose, Profile, f32)> = None;
    for (mut profile, _) in coarse.into_iter().take(8) {
        let Some((mut pose, mut s)) = score(profile) else {
            continue;
        };
        let mut delta = 8f32.to_radians();
        while delta > 0.25f32.to_radians() {
            let mut moved = true;
            while moved {
                moved = false;
                for axis in 0..3 {
                    for sign in [-1.0, 1.0] {
                        let mut p = profile;
                        *[&mut p.roll, &mut p.yaw, &mut p.pitch][axis] += sign * delta;
                        let p = p.clamped();
                        if p != profile
                            && let Some((q, t)) = score(p)
                            && t > s
                        {
                            (profile, pose, s, moved) = (p, q, t, true);
                        }
                    }
                }
            }
            delta *= 0.5;
        }
        if best.as_ref().is_none_or(|b| s > b.2) {
            best = Some((pose, profile, s));
        }
    }
    let (mut pose, profile, mut s) = best?;
    // Then the framing itself: a clipped or padded outline's box is not
    // quite the model's.
    let fits = |p: &Pose| overlap(&silhouette(mesh, p, GRID, GRID), &target_mask);
    let mut delta = 1.0f32;
    while delta > 0.05 {
        let mut moved = true;
        while moved {
            moved = false;
            let unit = size[0] as f32 / GRID as f32 * delta;
            for change in [
                Vec3::new(unit, 0.0, 0.0),
                Vec3::new(-unit, 0.0, 0.0),
                Vec3::new(0.0, unit, 0.0),
                Vec3::new(0.0, -unit, 0.0),
                Vec3::new(0.0, 0.0, 0.04 * delta),
                Vec3::new(0.0, 0.0, -0.04 * delta),
            ] {
                let p = Pose {
                    centre: pose.centre + change.truncate(),
                    scale: pose.scale * (1.0 + change.z),
                    ..pose
                };
                let t = fits(&p);
                if t > s {
                    (pose, s, moved) = (p, t, true);
                }
            }
        }
        delta *= 0.5;
    }
    (s >= 0.6).then_some((pose, profile, s))
}

/// Light for icons, in view space: from above, the left and the front.
const LIGHT: Vec3 = Vec3::new(-0.45, 0.65, 0.62);
const SAMPLES: usize = 3;
/// Icons are shot under a stronger light than play: the stock icons show
/// their faces clearly apart. The skin's near-black shell is brightened by
/// this much so its faces read the same way at icon size.
const EXPOSURE: f32 = 3.0;
/// How far behind the nearest surface an edge still shows, model units:
/// past the skin's puff, short of the far side.
const EDGE_DEPTH: f32 = 0.04;
/// Faces meeting at more than this, degrees, make a hard edge.
const CREASE_DEGREES: f32 = 35.0;

/// The model's hard edges: where faces meet at a sharp angle, or where
/// the surface ends. Positions are welded first, since a flat-shaded
/// model repeats each corner once per face.
fn creases(mesh: &Mesh) -> Vec<(Vec3, Vec3)> {
    use std::collections::BTreeMap;
    let key = |p: Vec3| (p * 1.0e4).round().to_array().map(|v| v as i64);
    type Edge = ([i64; 3], [i64; 3]);
    let mut faces: BTreeMap<Edge, (Vec3, Vec3, Vec<Vec3>)> = BTreeMap::new();
    for (_, t) in mesh.triangles() {
        let [a, b, c] = t.map(|i| mesh.positions[i]);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        if normal == Vec3::ZERO {
            continue;
        }
        for (p, q) in [(a, b), (b, c), (c, a)] {
            let (kp, kq) = (key(p), key(q));
            let k = if kp <= kq { (kp, kq) } else { (kq, kp) };
            faces.entry(k).or_insert((p, q, Vec::new())).2.push(normal);
        }
    }
    let cos = CREASE_DEGREES.to_radians().cos();
    faces
        .into_values()
        .filter(|(_, _, normals)| {
            normals.len() == 1
                || normals
                    .iter()
                    .any(|n| normals.iter().any(|m| n.dot(*m) < cos))
        })
        .map(|(p, q, _)| (p, q))
        .collect()
}

/// A pose turned by `rotation` whose drawing (with the skin's `puff`)
/// fills `target` (a box in icon pixels) as far as it can without
/// leaving it, centred in it.
pub fn frame(
    mesh: &Mesh,
    rotation: Quat,
    puff: f32,
    target: (Vec2, Vec2),
    size: [u32; 2],
) -> Option<Pose> {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
        let q = rotation * (*p + *n * puff);
        let s = Vec2::new(q.x, -q.y);
        lo = lo.min(s);
        hi = hi.max(s);
    }
    let extent = hi - lo;
    let room = target.1 - target.0;
    if !(extent.x > 1e-6 && extent.y > 1e-6 && room.x > 0.0 && room.y > 0.0) {
        return None;
    }
    let scale = (room.x / extent.x).min(room.y / extent.y);
    let centre = (target.0 + target.1) * 0.5 - (lo + hi) * 0.5 * scale;
    Some(Pose {
        rotation,
        scale,
        centre,
        size,
    })
}

/// The box a stock icon's drawing fills, in its pixels, kept at least
/// `MARGIN` of the icon clear of every edge (stock icons drawn right to the
/// edge would leave a new one no clear border).
fn filled_box(icon: &SceneImage) -> Option<(Vec2, Vec2)> {
    let (w, h) = (icon.width as usize, icon.height as usize);
    let mask: Vec<bool> = icon.rgba.chunks_exact(4).map(|p| p[3] >= 128).collect();
    let (lo, hi) = bounds(&mask, w)?;
    let margin = Vec2::new(w as f32, h as f32) * MARGIN;
    let lo = lo.max(margin);
    let hi = hi.min(Vec2::new(w as f32, h as f32) - margin);
    (hi.x > lo.x && hi.y > lo.y).then_some((lo, hi))
}
/// The least clear border an icon keeps on each side, of its size.
const MARGIN: f32 = 0.06;

/// How many fully clear rows or columns an icon has at its top, right,
/// bottom and left.
pub fn clear_border(image: &SceneImage) -> [usize; 4] {
    let (w, h) = (image.width as usize, image.height as usize);
    let clear = |x: usize, y: usize| image.rgba[(y * w + x) * 4 + 3] == 0;
    let row = |y: usize| (0..w).all(|x| clear(x, y));
    let column = |x: usize| (0..h).all(|y| clear(x, y));
    [
        (0..h).take_while(|y| row(*y)).count(),
        (0..w).rev().take_while(|x| column(*x)).count(),
        (0..h).rev().take_while(|y| row(*y)).count(),
        (0..w).take_while(|x| column(*x)).count(),
    ]
}

/// The model drawn under `pose` with `look`, on a clear background.
pub fn render(mesh: &Mesh, pose: &Pose, look: &Look, label: &str) -> SceneImage {
    render_oriented(mesh, pose, look, label, 0)
}

fn render_oriented(mesh: &Mesh, pose: &Pose, look: &Look, label: &str, turns: u8) -> SceneImage {
    let [iw, ih] = pose.size.map(|v| v as usize);
    let (w, h) = if turns.is_multiple_of(2) {
        (iw, ih)
    } else {
        (ih, iw)
    };
    let (sw, sh) = (iw * SAMPLES, ih * SAMPLES);
    let mut fine = *pose;
    fine.scale *= SAMPLES as f32;
    fine.centre *= SAMPLES as f32;
    let mut depth = vec![f32::MIN; sw * sh];
    let mut colour = vec![None::<Vec3>; sw * sh];
    let light = LIGHT.normalize();
    let mut draw = |positions: &[Vec3], shade: &dyn Fn(Vec3, Vec3, Vec3) -> Vec3| {
        let projected: Vec<Vec3> = positions.iter().map(|p| fine.project(*p)).collect();
        for (n, t) in mesh.triangles() {
            let [a, b, c] = t.map(|i| projected[i]);
            raster(
                a.truncate(),
                b.truncate(),
                c.truncate(),
                sw,
                sh,
                |x, y, bary| {
                    let z = a.z * bary.x + b.z * bary.y + c.z * bary.z;
                    let i = y * sw + x;
                    if z > depth[i] {
                        depth[i] = z;
                        let local = positions[t[0]] * bary.x
                            + positions[t[1]] * bary.y
                            + positions[t[2]] * bary.z;
                        let normal = (mesh.normals[t[0]] * bary.x
                            + mesh.normals[t[1]] * bary.y
                            + mesh.normals[t[2]] * bary.z)
                            .normalize_or_zero();
                        let surface = if look.textured {
                            mesh.surface(n, t, bary)
                        } else {
                            Vec3::ONE
                        };
                        colour[i] = Some(shade(local, pose.rotation * normal, surface));
                    }
                },
            );
        }
    };
    let base = Vec3::from(look.base.unwrap_or([1.0; 3]));
    draw(&mesh.positions, &|_, n, surface| {
        base * surface * (0.45 + 0.6 * n.dot(light).max(0.0))
    });
    if let Some(skin) = &look.skin {
        // Puffed along its normals, as the in-game skin is drawn over the
        // model: at hard edges the model's own colour shows through.
        let puffed: Vec<Vec3> = mesh
            .positions
            .iter()
            .zip(&mesh.normals)
            .map(|(p, n)| *p + *n * skin.puff)
            .collect();
        let shell = Vec3::from(skin.shell);
        let veins = Vec3::from(skin.veins.unwrap_or([1.0; 3]));
        let pixel = 1.0 / pose.scale.max(1e-3);
        draw(&puffed, &|local, n, _| {
            veined(local, n, light, shell, veins, pixel)
        });
    }
    if look.skin.is_some() {
        // The in-game skin is puffed along split normals, so the model's
        // own colour shows along every hard edge: at icon size those
        // cracks are thinner than a pixel, so they are drawn a pixel wide.
        let edge = base * (0.55 + 0.55 * Vec3::new(0.0, 0.3, 0.95).normalize().dot(light).max(0.0));
        let radius = 0.55 * SAMPLES as f32;
        for (a, b) in creases(mesh) {
            let (a, b) = (fine.project(a), fine.project(b));
            let steps = (a.truncate().distance(b.truncate()) / 0.5).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let p = a.lerp(b, k as f32 / steps as f32);
                let (x0, x1) = (
                    (p.x - radius).floor().max(0.0) as usize,
                    ((p.x + radius).ceil() as usize).min(sw),
                );
                let (y0, y1) = (
                    (p.y - radius).floor().max(0.0) as usize,
                    ((p.y + radius).ceil() as usize).min(sh),
                );
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = y * sw + x;
                        let centre = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                        // Only where the edge is the surface seen there.
                        if centre.distance(p.truncate()) <= radius
                            && colour[i].is_some()
                            && p.z >= depth[i] - EDGE_DEPTH
                        {
                            colour[i] = Some(edge);
                        }
                    }
                }
            }
        }
    }
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            // Screen orientation is a texel-coordinate transform at output,
            // so stock framing, shading, antialiasing and artwork stay exact.
            let (ix, iy) = match turns {
                1 => (y, ih - 1 - x),
                2 => (iw - 1 - x, ih - 1 - y),
                3 => (iw - 1 - y, x),
                _ => (x, y),
            };
            let (mut sum, mut covered) = (Vec3::ZERO, 0usize);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    if let Some(c) = colour[(iy * SAMPLES + sy) * sw + ix * SAMPLES + sx] {
                        sum += c;
                        covered += 1;
                    }
                }
            }
            let c = if covered > 0 {
                sum / covered as f32
            } else {
                Vec3::ZERO
            };
            let byte = |v: f32| (v.clamped(0.0, 1.0) * 255.0).round() as u8;
            rgba.extend([byte(c.x), byte(c.y), byte(c.z)]);
            rgba.push(((covered * 255) as f32 / (SAMPLES * SAMPLES) as f32).round() as u8);
        }
    }
    SceneImage {
        label: label.into(),
        width: w as u32,
        height: h as u32,
        rgba,
        srgb: false,
    }
}

/// `alien.wgsl`'s `fs_main` at rest, seen from the front (+Z in view
/// space) under the icon light.
///
/// `pixel` is one icon pixel in model units: the veins are drawn at least
/// about a pixel wide, as they are at any distance in play, or the icon's
/// few pixels would miss them.
fn veined(local: Vec3, n: Vec3, light: Vec3, shell: Vec3, vein: Vec3, pixel: f32) -> Vec3 {
    let view = Vec3::Z;
    let lit = shell * (Vec3::splat(0.55) * 1.2 + Vec3::ONE * n.dot(light).max(0.0)) * EXPOSURE;
    let edge = 1.0 - n.dot(view).abs();
    let hue = edge * 1.3 + local.dot(Vec3::new(0.6, 0.9, 0.4));
    let film = (Vec3::splat(hue) + Vec3::new(0.0, 0.33, 0.67))
        .map(|v| 0.5 + 0.5 * (std::f32::consts::TAU * v).cos());
    let cold =
        Vec3::new(0.25, 0.1, 0.55).lerp(Vec3::new(0.1, 0.75, 0.8), film.y) * (0.6 + 0.4 * film.z);
    let sheen = cold * edge.powi(3) * 0.4;
    let reflected = (-light) - 2.0 * (-light).dot(n) * n;
    let spec = reflected.dot(view).max(0.0).powf(48.0) * 0.8;
    let q = local * 3.2;
    let w = (q.x * 7.0 + 2.0 * (q.y * 5.0 + q.z * 3.0).sin()).sin()
        + (q.y * 6.0 + 2.0 * (q.z * 4.0 + q.x * 5.0).sin()).sin()
        + 0.7 * (q.z * 8.0 + 1.5 * (q.x * 4.0 + q.y * 2.0).sin()).sin();
    // The wave changes about 22 a unit across the model.
    let t = (w.abs() / (22.0 * 0.6 * pixel).max(0.16)).clamped(0.0, 1.0);
    let lines = 1.0 - t * t * (3.0 - 2.0 * t);
    let flow = 0.5 + 0.5 * (local.y * 9.0).sin();
    let glow = vein * lines * 0.35 * (0.45 + 0.55 * flow);
    lit + sheen + Vec3::splat(spec) + glow
}

/// Render the icon `spec` asks for: `mesh` posed like `reference` (the
/// stock item's model and its icon).
pub fn render_like(
    spec: &Spec,
    mesh: &Mesh,
    reference: (&Mesh, &SceneImage),
    label: &str,
) -> Result<SceneImage> {
    let (pose, profile, _) = fit_pose(reference.0, reference.1)
        .with_context(|| format!("{}'s model does not match its icon", spec.pose_like))?;
    render_posed(spec, mesh, &(pose, profile), reference.1, label)
}

/// A stock item's pose and side profile, fitted to its icon (`fit_pose`).
pub type Fitted = (Pose, Profile);

/// [`render_like`] from the stock item's pose, already fitted to its
/// `icon` (a pose is fitted once for every icon posed like that item).
pub fn render_posed(
    spec: &Spec,
    mesh: &Mesh,
    fitted: &Fitted,
    icon: &SceneImage,
    label: &str,
) -> Result<SceneImage> {
    ensure!(!mesh.indices.is_empty(), "the item has no model to draw");
    // The stock icon's profile, applied to this model's own axes, so it
    // points the way the stock item does; the framing is this model's own,
    // filling the box the stock drawing fills.
    let (fitted, profile) = fitted;
    let target = filled_box(icon).context("the stock icon is empty")?;
    let puff = spec.look.skin.as_ref().map_or(0.0, |s| s.puff);
    let pose = frame(mesh, profile.rotation(mesh.axes), puff, target, fitted.size)
        .context("the model has no size")?;
    ensure!(
        spec.clockwise_quarter_turns < 4,
        "icon clockwise_quarter_turns must be 0 to 3"
    );
    Ok(render_oriented(
        mesh,
        &pose,
        &spec.look,
        label,
        spec.clockwise_quarter_turns,
    ))
}

/// Bumped whenever the drawing changes, so icons kept on disk from an
/// older drawing are drawn again.
const DRAWING: u32 = 4;

/// Everything one icon is drawn from.
#[derive(Clone, Debug)]
pub struct Request {
    pub spec: Spec,
    /// The item's model.
    pub mesh: Mesh,
    /// The stock item's model and icon (`spec.pose_like`).
    pub reference: Mesh,
    pub icon: SceneImage,
    pub label: String,
}

impl Request {
    /// A hash of everything the icon is drawn from, and of the drawing
    /// itself (`DRAWING`): the same inputs, the same picture.
    pub fn digest(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(b"bri item icon\0");
        hash.update(DRAWING.to_le_bytes());
        let spec = serde_json::to_vec(&self.spec).expect("a spec always serializes");
        for mesh in [&self.mesh, &self.reference] {
            hash.update((mesh.positions.len() as u64).to_le_bytes());
            for v in mesh.positions.iter().chain(&mesh.normals) {
                for c in v.to_array() {
                    hash.update(c.to_le_bytes());
                }
            }
            hash.update((mesh.indices.len() as u64).to_le_bytes());
            for i in &mesh.indices {
                hash.update(i.to_le_bytes());
            }
            for c in mesh
                .axes
                .forward
                .to_array()
                .into_iter()
                .chain(mesh.axes.up.to_array())
            {
                hash.update(c.to_le_bytes());
            }
            // What colours its surface: a texture or colour change redraws it.
            hash.update((mesh.uvs.len() as u64).to_le_bytes());
            for c in mesh.uvs.iter().flat_map(|v| v.to_array()) {
                hash.update(c.to_le_bytes());
            }
            hash.update((mesh.colors.len() as u64).to_le_bytes());
            for c in mesh.colors.iter().flat_map(|v| v.to_array()) {
                hash.update(c.to_le_bytes());
            }
            hash.update((mesh.triangle_images.len() as u64).to_le_bytes());
            for t in &mesh.triangle_images {
                let (image, overlay) = t.map_or((u64::MAX, false), |(i, o)| (i as u64, o));
                hash.update(image.to_le_bytes());
                hash.update([u8::from(overlay)]);
            }
            hash.update((mesh.images.len() as u64).to_le_bytes());
            for image in &mesh.images {
                hash.update(image.width.to_le_bytes());
                hash.update(image.height.to_le_bytes());
                hash.update([u8::from(image.srgb)]);
                hash.update((image.rgba.len() as u64).to_le_bytes());
                hash.update(&image.rgba);
            }
        }
        hash.update(self.icon.width.to_le_bytes());
        hash.update(self.icon.height.to_le_bytes());
        hash.update(&self.icon.rgba);
        hash.update((spec.len() as u64).to_le_bytes());
        hash.update(&spec);
        format!("{:x}", hash.finalize())
    }
    fn file(&self, cache: &Path) -> PathBuf {
        cache.join(format!("{}.png", self.digest()))
    }
    /// The icon kept in `cache` from an earlier drawing, if there is one
    /// that reads and is the stock icon's size.
    pub fn cached(&self, cache: &Path) -> Option<SceneImage> {
        let bytes = std::fs::read(self.file(cache)).ok()?;
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .ok()?
            .into_rgba8();
        (image.dimensions() == (self.icon.width, self.icon.height)).then(|| SceneImage {
            label: self.label.clone(),
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
            srgb: false,
        })
    }
    /// Draw the icon, and keep it in `cache` for next time. A lost write
    /// only means drawing it again.
    pub fn draw(&self, cache: Option<&Path>) -> Result<SceneImage> {
        let fitted = self
            .fit()
            .with_context(|| format!("{}'s model does not match its icon", self.spec.pose_like))?;
        self.draw_fitted(&fitted, cache)
    }
    /// The stock item's pose, fitted to its icon: the same for every
    /// request posed like the same item, so fitted once for them all.
    pub fn fit(&self) -> Option<Fitted> {
        fit_pose(&self.reference, &self.icon).map(|(pose, profile, _)| (pose, profile))
    }
    /// [`Self::draw`] from the stock item's pose, already [`Self::fit`]ted.
    pub fn draw_fitted(&self, fitted: &Fitted, cache: Option<&Path>) -> Result<SceneImage> {
        let image = render_posed(&self.spec, &self.mesh, fitted, &self.icon, &self.label)?;
        if let Some(cache) = cache {
            let file = self.file(cache);
            let partial = file.with_extension("partial");
            let _ = std::fs::create_dir_all(cache)
                .map_err(image::ImageError::IoError)
                .and_then(|_| {
                    image::save_buffer_with_format(
                        &partial,
                        &image.rgba,
                        image.width,
                        image.height,
                        image::ColorType::Rgba8,
                        image::ImageFormat::Png,
                    )
                })
                .and_then(|_| std::fs::rename(&partial, &file).map_err(image::ImageError::IoError));
        }
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gun of boxes, lopsided on every axis so only one pose fits.
    fn gun() -> Mesh {
        let mut mesh = Mesh::default();
        let mut add_box = |c: Vec3, h: Vec3| {
            let corners = [
                (Vec3::X, Vec3::Y, Vec3::Z),
                (Vec3::NEG_X, Vec3::Y, Vec3::NEG_Z),
                (Vec3::Y, Vec3::Z, Vec3::X),
                (Vec3::NEG_Y, Vec3::Z, Vec3::NEG_X),
                (Vec3::Z, Vec3::X, Vec3::Y),
                (Vec3::NEG_Z, Vec3::X, Vec3::NEG_Y),
            ];
            for (n, u, v) in corners {
                let base = mesh.positions.len() as u32;
                for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    mesh.positions.push(c + (n + u * a + v * b) * h);
                    mesh.normals.push(n);
                }
                mesh.indices
                    .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        };
        add_box(Vec3::new(0.0, 0.2, 0.0), Vec3::new(0.9, 0.25, 0.2));
        add_box(Vec3::new(0.95, 0.3, 0.05), Vec3::new(0.2, 0.4, 0.3));
        add_box(Vec3::new(-0.45, -0.45, 0.0), Vec3::new(0.15, 0.45, 0.15));
        // The emitter is its front, the grip hangs below.
        mesh.axes = Axes::new(Vec3::X, Vec3::Y);
        mesh
    }

    /// Where `axis` of a model points in the picture under `rotation`
    /// (x right, y up).
    fn on_screen(rotation: Quat, axis: Vec3) -> Vec2 {
        (rotation * axis).truncate().normalize_or_zero()
    }

    fn picture(mesh: &Mesh, pose: &Pose) -> SceneImage {
        render(
            mesh,
            pose,
            &Look {
                base: None,
                textured: false,
                skin: None,
            },
            "reference",
        )
    }

    /// The pose of a stock icon is recovered from its picture alone, so a
    /// new icon drawn with it lines up with the stock one.
    #[test]
    fn a_models_pose_is_recovered_from_its_icon() {
        let mesh = gun();
        let shown = Profile {
            side: 1.0,
            roll: 0.35,
            yaw: 0.3,
            pitch: -0.25,
        };
        let truth = Pose {
            rotation: shown.rotation(mesh.axes),
            scale: 24.0,
            centre: Vec2::new(34.0, 30.0),
            size: [64, 64],
        };
        let icon = picture(&mesh, &truth);
        let (pose, profile, score) = fit_pose(&mesh, &icon).expect("fits");
        assert!(score > 0.9, "outline overlap {score}");
        assert_eq!(profile.side, 1.0, "{profile:?}");
        for axis in [mesh.axes.forward, mesh.axes.up] {
            let (a, b) = (
                on_screen(pose.rotation, axis),
                on_screen(truth.rotation, axis),
            );
            assert!(a.dot(b) > 0.97, "{axis}: fitted {a}, drawn {b}");
        }
        let redrawn = picture(&mesh, &pose);
        let covered = |img: &SceneImage| {
            img.rgba
                .chunks_exact(4)
                .map(|p| p[3] >= 128)
                .collect::<Vec<_>>()
        };
        let agree = overlap(&covered(&redrawn), &covered(&icon));
        assert!(agree > 0.9, "redrawn icon overlaps the stock one {agree}");
        assert_eq!([redrawn.width, redrawn.height], [64, 64]);
    }

    #[test]
    fn screen_quarter_turn_rotates_an_icon_clockwise_and_changes_its_cache_key() {
        let mesh = gun();
        let pose = Pose {
            rotation: Profile {
                side: -1.0,
                roll: -0.3,
                yaw: 0.2,
                pitch: 0.15,
            }
            .rotation(mesh.axes),
            scale: 18.0,
            centre: Vec2::new(34.0, 24.0),
            size: [72, 48],
        };
        let look = Look {
            base: Some([0.35, 1.0, 0.8]),
            skin: Some(Skin {
                shell: shell(),
                veins: Some([0.3, 0.95, 1.0]),
                puff: puff(),
            }),
            textured: false,
        };
        let plain = render(&mesh, &pose, &look, "plain");
        let rotated = render_oriented(&mesh, &pose, &look, "turned", 1);
        assert_eq!((rotated.width, rotated.height), (48, 72));
        assert!(plain.rgba.chunks_exact(4).any(|p| p[3] > 0));
        for y in 0..plain.height as usize {
            for x in 0..plain.width as usize {
                let before = (y * plain.width as usize + x) * 4;
                let after = (x * rotated.width as usize + (plain.height as usize - 1 - y)) * 4;
                for c in 0..4 {
                    assert_eq!(
                        plain.rgba[before + c],
                        rotated.rgba[after + c],
                        "clockwise pixel ({x},{y}) channel {c}"
                    );
                }
            }
        }
        let spec = Spec::parse(br#"{"schema_version":1,"pose_like":"stock"}"#).unwrap();
        assert_eq!(spec.clockwise_quarter_turns, 0);
        let mut request = Request {
            spec,
            mesh: mesh.clone(),
            reference: mesh,
            icon: plain,
            label: "icon".into(),
        };
        let old = request.digest();
        request.spec.clockwise_quarter_turns = 1;
        assert_ne!(request.digest(), old);
        assert!(
            Spec::parse(br#"{"schema_version":1,"pose_like":"stock","clockwise_quarter_turns":4}"#)
                .is_err()
        );
    }

    /// Max, v0.1.10: the Gravity Gun icon was "great just seems to be
    /// wrong perspective angle". An item drawn like a stock one is seen in
    /// the same profile: its own forward and up point the same ways in the
    /// picture as the stock item's, whichever way its model was built, and
    /// it is never seen tumbled (from above or behind, nose rolled down).
    #[test]
    fn an_item_drawn_like_a_stock_one_points_the_same_way() {
        let stock = gun();
        let shown = Profile {
            side: -1.0,
            roll: -0.3,
            yaw: 0.2,
            pitch: 0.15,
        };
        let truth = Pose {
            rotation: shown.rotation(stock.axes),
            scale: 24.0,
            centre: Vec2::new(32.0, 32.0),
            size: [64, 64],
        };
        let icon = picture(&stock, &truth);
        // The same gun built lying another way: forward +Y, up +Z.
        let turn = Quat::from_mat3(&glam::Mat3::from_cols(Vec3::Y, Vec3::Z, Vec3::X));
        let mut item = stock.clone();
        for v in item.positions.iter_mut().chain(&mut item.normals) {
            *v = turn * *v;
        }
        item.axes = Axes::default();
        let spec = Spec {
            schema_version: 1,
            pose_like: "stock".into(),
            clockwise_quarter_turns: 0,
            look: Look {
                base: None,
                skin: None,
                textured: false,
            },
        };
        let (_, profile, _) = fit_pose(&stock, &icon).expect("fits");
        let rotation = profile.rotation(item.axes);
        for (theirs, ours) in [
            (stock.axes.forward, item.axes.forward),
            (stock.axes.up, item.axes.up),
        ] {
            let (a, b) = (on_screen(truth.rotation, theirs), on_screen(rotation, ours));
            assert!(a.dot(b) > 0.97, "stock {a}, item {b}");
        }
        // Seen side on: up points up the picture, forward across it.
        assert!(on_screen(rotation, item.axes.up).y > 0.5);
        assert!(
            on_screen(rotation, item.axes.forward).x < -0.5,
            "nose to the left, as the stock one"
        );
        // And drawn so: the item's own drawing matches the stock outline.
        let drawn = render_like(&spec, &item, (&stock, &icon), "item").unwrap();
        let covered = |img: &SceneImage| {
            img.rgba
                .chunks_exact(4)
                .map(|p| p[3] >= 128)
                .collect::<Vec<_>>()
        };
        assert!(overlap(&covered(&drawn), &covered(&icon)) > 0.7);
    }

    /// A textured model (wood and iron, say) is posed in the stock item's
    /// side profile just as a plain one is, and draws its own texture.
    #[test]
    fn a_textured_item_is_seen_side_on_in_its_own_colours() {
        let stock = gun();
        let shown = Profile {
            side: 1.0,
            roll: 0.4,
            yaw: -0.2,
            pitch: 0.1,
        };
        let truth = Pose {
            rotation: shown.rotation(stock.axes),
            scale: 24.0,
            centre: Vec2::new(32.0, 32.0),
            size: [64, 64],
        };
        let icon = picture(&stock, &truth);
        // Built lying another way (forward +Y, up +Z), all of wood.
        let turn = Quat::from_mat3(&glam::Mat3::from_cols(Vec3::Y, Vec3::Z, Vec3::X));
        let mut item = stock.clone();
        for v in item.positions.iter_mut().chain(&mut item.normals) {
            *v = turn * *v;
        }
        item.axes = Axes::default();
        item.uvs = vec![Vec2::ZERO; item.positions.len()];
        item.colors = vec![Vec3::ONE; item.positions.len()];
        item.triangle_images = vec![Some((0, false)); item.indices.len() / 3];
        item.images = vec![SceneImage {
            label: "wood".into(),
            width: 1,
            height: 1,
            rgba: vec![150, 90, 40, 255],
            srgb: false,
        }];
        let spec = Spec {
            schema_version: 1,
            pose_like: "stock".into(),
            clockwise_quarter_turns: 0,
            look: Look {
                base: None,
                skin: None,
                textured: true,
            },
        };
        let (_, profile, _) = fit_pose(&stock, &icon).expect("fits");
        let rotation = profile.rotation(item.axes);
        assert!(
            on_screen(rotation, item.axes.up).dot(on_screen(truth.rotation, stock.axes.up)) > 0.97
        );
        assert!(
            on_screen(rotation, item.axes.forward)
                .dot(on_screen(truth.rotation, stock.axes.forward))
                > 0.97
        );
        let drawn = render_like(&spec, &item, (&stock, &icon), "item").unwrap();
        let covered = |img: &SceneImage| {
            img.rgba
                .chunks_exact(4)
                .map(|p| p[3] >= 128)
                .collect::<Vec<_>>()
        };
        assert!(
            overlap(&covered(&drawn), &covered(&icon)) > 0.7,
            "the stock outline"
        );
        let solid: Vec<_> = drawn.rgba.chunks_exact(4).filter(|p| p[3] == 255).collect();
        let wood = solid
            .iter()
            .filter(|p| p[0] > p[1] && p[1] > p[2] && p[0] > 40)
            .count();
        assert!(
            wood * 2 > solid.len(),
            "drawn in its wood: {wood} of {}",
            solid.len()
        );
    }

    /// Something else entirely does not pass for the stock item.
    #[test]
    fn a_model_that_is_not_the_icon_does_not_fit() {
        let mesh = gun();
        let mut icon = SceneImage {
            label: "ring".into(),
            width: 64,
            height: 64,
            rgba: vec![0; 64 * 64 * 4],
            srgb: false,
        };
        for y in 0..64 {
            for x in 0..64 {
                let r = ((x as f32 - 31.5).powi(2) + (y as f32 - 31.5).powi(2)).sqrt();
                if (20.0..28.0).contains(&r) {
                    icon.rgba[(y * 64 + x) * 4 + 3] = 255;
                }
            }
        }
        assert!(fit_pose(&mesh, &icon).is_none());
    }

    /// The skin is dark with light veins, the background stays clear, and
    /// the same request draws the same picture every time.
    #[test]
    fn the_veined_skin_draws_a_dark_shell_on_a_clear_background() {
        let mesh = gun();
        let pose = Pose {
            rotation: euler(0.7, 0.35, 0.5),
            scale: 30.0,
            centre: Vec2::new(34.0, 30.0),
            size: [64, 64],
        };
        let look = Look {
            base: Some([0.35, 1.0, 0.8]),
            textured: false,
            skin: Some(Skin {
                shell: [0.035, 0.025, 0.05],
                veins: Some([0.3, 0.95, 1.0]),
                puff: 0.012,
            }),
        };
        let image = render(&mesh, &pose, &look, "gun");
        assert_eq!(image.rgba[3], 0, "the corner is clear");
        let solid: Vec<_> = image.rgba.chunks_exact(4).filter(|p| p[3] == 255).collect();
        assert!(solid.len() > 300, "{} solid pixels", solid.len());
        let dark = solid
            .iter()
            .filter(|p| p[0] < 60 && p[1] < 60 && p[2] < 70)
            .count();
        assert!(
            dark * 2 > solid.len(),
            "mostly the dark shell: {dark} of {}",
            solid.len()
        );
        assert!(
            solid.iter().any(|p| p[1] > 60 && p[2] > 60 && p[0] < p[1]),
            "teal veins show"
        );
        assert_eq!(render(&mesh, &pose, &look, "gun").rgba, image.rgba);
    }

    /// A stock icon drawn right to its edges still gets a new icon with a
    /// clear border on every side, and the drawing fills the box inside it.
    #[test]
    fn the_icon_keeps_a_clear_margin_on_every_side() {
        let mesh = gun();
        let mut stock = SceneImage {
            label: "stock".into(),
            width: 96,
            height: 96,
            rgba: vec![0; 96 * 96 * 4],
            srgb: false,
        };
        for y in 0..96 {
            for x in 0..96 {
                // A band corner to corner, touching all four edges.
                if (x as i32 - y as i32).abs() < 40 {
                    stock.rgba[(y * 96 + x) * 4 + 3] = 255;
                }
            }
        }
        let target = filled_box(&stock).expect("a box");
        let look = Look {
            base: Some([0.35, 1.0, 0.8]),
            textured: false,
            skin: Some(Skin {
                shell: [0.035, 0.025, 0.05],
                veins: Some([0.3, 0.95, 1.0]),
                puff: 0.012,
            }),
        };
        for rotation in [euler(0.7, 0.35, 0.5), euler(-1.2, 0.9, 2.4), Quat::IDENTITY] {
            let pose = frame(&mesh, rotation, 0.012, target, [96, 96]).expect("frames");
            let image = render(&mesh, &pose, &look, "gun");
            let border = clear_border(&image);
            assert!(
                border.iter().all(|b| *b >= 5),
                "clear rows and columns (top, right, bottom, left): {border:?}"
            );
            // It fills the box one way or the other: no shrunken drawing.
            let [top, right, bottom, left] = border.map(|b| b as i32);
            let (w, h) = (96 - left - right, 96 - top - bottom);
            assert!(w >= 80 || h >= 80, "{w}x{h} drawn, border {border:?}");
        }
    }

    /// A drawn icon is kept on disk and found again by the same request;
    /// any change to what it is drawn from draws it again.
    #[test]
    fn a_drawn_icon_is_kept_for_the_same_request() {
        let mesh = gun();
        let truth = Pose {
            rotation: euler(0.7, 0.35, 0.5),
            scale: 24.0,
            centre: Vec2::new(34.0, 30.0),
            size: [64, 64],
        };
        let spec = Spec::parse(
            br#"{"schema_version": 1, "pose_like": "stock",
                "look": {"base": [0.35, 1, 0.8], "skin": {"shell": [0.035, 0.025, 0.05], "veins": [0.3, 0.95, 1]}}}"#,
        )
        .unwrap();
        let request = Request {
            spec,
            mesh: mesh.clone(),
            reference: mesh.clone(),
            icon: picture(&mesh, &truth),
            label: "gun".into(),
        };
        let cache = tempfile::tempdir().unwrap();
        assert!(request.cached(cache.path()).is_none(), "nothing kept yet");
        let drawn = request.draw(Some(cache.path())).unwrap();
        let kept = request.cached(cache.path()).expect("kept on disk");
        assert_eq!(
            (kept.width, kept.height, &kept.rgba, &kept.label),
            (drawn.width, drawn.height, &drawn.rgba, &drawn.label)
        );
        assert_eq!(
            std::fs::read_dir(cache.path()).unwrap().count(),
            1,
            "one file, no partial left"
        );
        let mut other = request.clone();
        other.spec.look.base = Some([1.0, 0.0, 0.0]);
        assert!(other.cached(cache.path()).is_none(), "a changed look");
        let mut other = request.clone();
        other.mesh.positions[0].x += 0.01;
        assert!(other.cached(cache.path()).is_none(), "a changed model");
        let mut other = request.clone();
        other.icon.rgba[3] ^= 0xff;
        assert!(other.cached(cache.path()).is_none(), "a changed stock icon");
        let mut other = request.clone();
        other.mesh.axes = Axes::new(Vec3::Y, Vec3::Z);
        assert!(
            other.cached(cache.path()).is_none(),
            "a model pointing another way"
        );
        // What colours the surface: its texture coordinates, colours,
        // textures and which triangles use them.
        let textured = |mesh: &mut Mesh| {
            mesh.uvs = vec![Vec2::ZERO; mesh.positions.len()];
            mesh.colors = vec![Vec3::ONE; mesh.positions.len()];
            mesh.triangle_images = vec![Some((0, false)); mesh.indices.len() / 3];
            mesh.images = vec![SceneImage {
                label: "wood".into(),
                width: 1,
                height: 1,
                rgba: vec![120, 80, 40, 255],
                srgb: true,
            }];
        };
        let mut base = request.clone();
        textured(&mut base.mesh);
        let digest = base.digest();
        type Change = fn(&mut Mesh);
        let changes: [(&str, Change); 4] = [
            ("uvs", |m| m.uvs[0].x = 0.5),
            ("colours", |m| m.colors[0].y = 0.5),
            ("which texture", |m| m.triangle_images[0] = None),
            ("the texture", |m| m.images[0].rgba[0] = 0),
        ];
        for (what, change) in changes {
            let mut other = base.clone();
            change(&mut other.mesh);
            assert_ne!(other.digest(), digest, "a changed {what}");
        }
    }

    #[test]
    fn the_request_is_checked() {
        let ok = br#"{"schema_version": 1, "pose_like": "v20.weapon.printgun",
            "look": {"base": [0.35, 1, 0.8], "skin": {"shell": [0.035, 0.025, 0.05], "veins": [0.3, 0.95, 1]}}}"#;
        let spec = Spec::parse(ok).unwrap();
        assert_eq!(spec.look.skin.unwrap().puff, 0.012);
        for bad in [
            &br#"{"schema_version": 2, "pose_like": "x"}"#[..],
            br#"{"schema_version": 1, "pose_like": ""}"#,
            br#"{"schema_version": 1, "pose_like": "x", "look": {"base": [2, 0, 0]}}"#,
            br#"{"schema_version": 1, "pose_like": "x", "extra": 1}"#,
        ] {
            assert!(
                Spec::parse(bad).is_err(),
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
    }
}
