use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub const BRICK_SCHEMA: u32 = 1;
pub const STUD: f32 = 0.5;
pub const PLATE: f32 = 0.2;

#[derive(Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub schema_version: u32,
    pub bricks: Vec<CatalogEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub subcategory: String,
    pub mesh_id: String,
    pub collision_source: Option<String>,
    pub icon_source: String,
    pub print_aspect_ratio: Option<String>,
    pub orientation_fix: u8,
    pub can_cover: bool,
    pub indestructible: bool,
    pub special_kind: Option<String>,
    /// Declarative source expressions retained for later feature adaptation.
    pub other_properties: std::collections::BTreeMap<String, String>,
    /// Sides of the brick that are mirrors (not in v20; an Add-On sets it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reflection: Option<Reflection>,
    /// Sides that open onto another placed brick of this kind (not in v20;
    /// an Add-On sets it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    /// Built on `mesh_id`'s shape at another size: width and depth in
    /// studs, height in plates (not in v20; an Add-On sets it). See
    /// [`Brick::stretched`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stretch: Option<[u32; 3]>,
}

/// Flat mirrors on a brick's sides. Each player's game draws what a mirror
/// faces as a live reflection; nothing about it is simulated or sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reflection {
    /// The sides that reflect, in the brick's unrotated frame (not `omni`).
    pub faces: Vec<Face>,
    /// Where each mirror sits between its side (0) and the opposite side
    /// (1): a window's pane is 0.5.
    #[serde(default)]
    pub depth: f32,
    /// Frame left bare around each mirror, in world units (a stud is 0.5,
    /// a plate 0.2).
    #[serde(default)]
    pub inset: f32,
    /// Multiplies the reflected picture.
    #[serde(default = "Reflection::white")]
    pub tint: [f32; 3],
    /// 1 replaces the side's own look; less lets the painted brick show
    /// through (a polished floor).
    #[serde(default = "Reflection::full")]
    pub strength: f32,
}
impl Reflection {
    fn white() -> [f32; 3] {
        [1.0; 3]
    }
    fn full() -> f32 {
        1.0
    }
    /// Checked against the brick it belongs to: the inset must leave some
    /// mirror on every reflecting side.
    pub fn validate(&self, mesh: &Brick) -> Result<()> {
        ensure!(
            !self.faces.is_empty()
                && !self.faces.contains(&Face::Omni)
                && (1..self.faces.len()).all(|i| !self.faces[..i].contains(&self.faces[i])),
            "Reflection faces must be distinct sides, not omni"
        );
        ensure!(
            (0.0..=1.0).contains(&self.depth)
                && self.inset.is_finite()
                && self.inset >= 0.0
                && self.tint.iter().all(|c| (0.0..=1.0).contains(c))
                && (0.0..=1.0).contains(&self.strength)
                && self.strength > 0.0,
            "Reflection depth, tint and strength must be 0 to 1, inset at least 0"
        );
        for face in &self.faces {
            let [(_, half_u), (_, half_v)] = mesh.face_axes(*face);
            ensure!(
                self.inset < half_u.min(half_v),
                "Reflection inset leaves no mirror on the brick's {face:?} side"
            );
        }
        Ok(())
    }
    /// Each reflecting side's mirror in the brick's own frame: corners
    /// counterclockwise seen from in front, pushed a millimetre off the
    /// brick's own surface so it draws over it.
    pub fn quads(&self, mesh: &Brick) -> Vec<[[f32; 3]; 4]> {
        self.faces
            .iter()
            .map(|&face| {
                let normal = face_normal(face);
                let [(u, half_u), (v, half_v)] = mesh.face_axes(face);
                let half_n = mesh.half_extent(normal);
                let centre = normal * (half_n * (1.0 - 2.0 * self.depth) + 0.001);
                let (du, dv) = (u * (half_u - self.inset), v * (half_v - self.inset));
                [
                    centre - du - dv,
                    centre + du - dv,
                    centre + du + dv,
                    centre - du + dv,
                ]
                .map(|p| p.to_array())
            })
            .collect()
    }
    /// Whether a full mirror takes the place of `quad`: a translucent
    /// surface of the brick lying flat across one of its mirrored sides (a
    /// window's glass), which would film the reflection over. Opaque
    /// surfaces (the frame) stay; a partial mirror keeps the brick's look.
    pub fn replaces(&self, mesh: &Brick, quad: &Quad) -> bool {
        const EDGE: f32 = 0.01;
        let translucent = quad
            .colors
            .is_some_and(|colors| colors.iter().any(|c| c[3] < 1.0));
        let [a, b, c, _] = quad.vertices.map(|v| glam::Vec3::from(v.position));
        let Some(facing) = (b - a).cross(c - b).try_normalize() else {
            return false;
        };
        self.strength >= 1.0
            && translucent
            && self.faces.iter().any(|&face| {
                let [(u, half_u), (v, half_v)] = mesh.face_axes(face);
                facing.dot(face_normal(face)).abs() > 0.99
                    && quad.vertices.iter().all(|vertex| {
                        let p = glam::Vec3::from(vertex.position);
                        p.dot(u).abs() <= half_u + EDGE && p.dot(v).abs() <= half_v + EDGE
                    })
            })
    }
}
/// Sides of a brick that open onto a linked brick: each shows the view out
/// of its partner, and with `pass` bodies crossing it come out of the
/// partner. Two placed bricks of one definition and one owner with the same
/// brick name are linked (more than two form a ring, each leading to the
/// next); placing two in a row names them alike, as v20 teledoors do. The
/// view is drawn by each player's game; crossing is decided by the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    /// The open sides, in the brick's unrotated frame (not `omni`). Going
    /// in through one comes out of the partner's opposite side when that
    /// is open too (a doorway), else out of the same side (a wall portal).
    pub faces: Vec<Face>,
    /// Where each opening sits between its side (0) and the opposite side
    /// (1): a window's pane is 0.5.
    #[serde(default)]
    pub depth: f32,
    /// Frame left around each opening's picture, in world units.
    #[serde(default)]
    pub inset: f32,
    /// Multiplies the view.
    #[serde(default = "Reflection::white")]
    pub tint: [f32; 3],
    /// Shown on a linked side too far away, or past the player's Mirrors
    /// setting, to show its view live.
    #[serde(default = "Link::haze")]
    pub idle: [f32; 3],
    /// Bodies may pass: the brick's collision becomes a frame `frame` wide
    /// around each opening.
    #[serde(default)]
    pub pass: bool,
    #[serde(default)]
    pub frame: Frame,
    /// Stem of the names placing gives a new pair (`Portal` names them
    /// `Portal_1a2b3`): letters, digits and underscores.
    pub name: String,
}
/// How wide a linked brick's frame is around each opening, in world units:
/// `top` along its upper edge, `bottom` along its lower one (a sill), and
/// `sides` along the rest (every edge of a top or bottom side). Written as
/// one number for all edges, or `{ "sides", "top", "bottom" }`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "FrameSpec")]
pub struct Frame {
    pub sides: f32,
    pub top: f32,
    pub bottom: f32,
}
impl Frame {
    /// The same width on every edge.
    pub fn even(width: f32) -> Self {
        Self {
            sides: width,
            top: width,
            bottom: width,
        }
    }
    fn widths(&self) -> [f32; 3] {
        [self.sides, self.top, self.bottom]
    }
    /// Less on the low and high edge along `axis` (unit, the brick's own).
    fn along(&self, axis: glam::Vec3) -> (f32, f32) {
        if axis.y.abs() > 0.5 {
            if axis.y > 0.0 {
                (self.bottom, self.top)
            } else {
                (self.top, self.bottom)
            }
        } else {
            (self.sides, self.sides)
        }
    }
}
#[derive(Deserialize)]
#[serde(untagged)]
enum FrameSpec {
    Even(f32),
    Edges(FrameEdges),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameEdges {
    #[serde(default)]
    sides: f32,
    #[serde(default)]
    top: f32,
    #[serde(default)]
    bottom: f32,
}
impl From<FrameSpec> for Frame {
    fn from(spec: FrameSpec) -> Self {
        match spec {
            FrameSpec::Even(width) => Frame::even(width),
            FrameSpec::Edges(FrameEdges { sides, top, bottom }) => Frame { sides, top, bottom },
        }
    }
}
/// One open side of a linked brick in the brick's own frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opening {
    pub face: Face,
    /// Centre on the opening's plane.
    pub centre: glam::Vec3,
    /// Out of the side.
    pub normal: glam::Vec3,
    /// In-plane axes (`u` crossed with `v` is `normal`) and half sizes
    /// along each.
    pub u: glam::Vec3,
    pub v: glam::Vec3,
    pub half: glam::Vec2,
}
impl Link {
    /// The default `idle`: a blue haze.
    pub fn haze() -> [f32; 3] {
        [0.35, 0.42, 0.55]
    }
    /// Checked against the brick it belongs to.
    pub fn validate(&self, mesh: &Brick) -> Result<()> {
        ensure!(
            !self.faces.is_empty()
                && !self.faces.contains(&Face::Omni)
                && (1..self.faces.len()).all(|i| !self.faces[..i].contains(&self.faces[i])),
            "Link faces must be distinct sides, not omni"
        );
        ensure!(
            (0.0..=1.0).contains(&self.depth)
                && std::iter::once(self.inset)
                    .chain(self.frame.widths())
                    .all(|v| v.is_finite() && v >= 0.0)
                && self
                    .tint
                    .iter()
                    .chain(&self.idle)
                    .all(|c| (0.0..=1.0).contains(c)),
            "Link depth, tint and idle must be 0 to 1, inset and frame at least 0"
        );
        ensure!(
            !self.name.is_empty()
                && self.name.len() <= 16
                && self
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && self.name.starts_with(|c: char| c.is_ascii_alphabetic()),
            "Link name must be up to 16 letters, digits or underscores, starting with a letter"
        );
        for face in &self.faces {
            let [(_, half_u), (_, half_v)] = mesh.face_axes(*face);
            let open = self.passage(mesh, *face).half;
            ensure!(
                self.inset < half_u.min(half_v) && open.x > 0.0 && open.y > 0.0,
                "Link inset or frame leaves no opening on the brick's {face:?} side"
            );
        }
        Ok(())
    }
    /// A side's opening, less `frame` on its edges.
    fn opening(&self, mesh: &Brick, face: Face, frame: Frame) -> Opening {
        let normal = face_normal(face);
        let [(u, half_u), (v, half_v)] = mesh.face_axes(face);
        let ((u_low, u_high), (v_low, v_high)) = (frame.along(u), frame.along(v));
        Opening {
            face,
            centre: normal * mesh.half_extent(normal) * (1.0 - 2.0 * self.depth)
                + u * (u_low - u_high) * 0.5
                + v * (v_low - v_high) * 0.5,
            normal,
            u,
            v,
            half: glam::Vec2::new(
                half_u - (u_low + u_high) * 0.5,
                half_v - (v_low + v_high) * 0.5,
            ),
        }
    }
    fn passage(&self, mesh: &Brick, face: Face) -> Opening {
        self.opening(mesh, face, self.frame)
    }
    /// What each open side shows, as a picture `inset` in from its edges.
    pub fn views(&self, mesh: &Brick) -> Vec<Opening> {
        self.faces
            .iter()
            .map(|&f| self.opening(mesh, f, Frame::even(self.inset)))
            .collect()
    }
    /// Where bodies pass through each open side: inside the frame.
    pub fn passages(&self, mesh: &Brick) -> Vec<Opening> {
        self.faces.iter().map(|&f| self.passage(mesh, f)).collect()
    }
    /// The side a body going in through `face` comes out of.
    pub fn exit(&self, face: Face) -> Face {
        let opposite = opposite(face);
        if self.faces.contains(&opposite) {
            opposite
        } else {
            face
        }
    }
    /// Takes a point in this brick's frame, going in through `face`, to
    /// where it comes out in the partner's frame: `face`'s opening onto the
    /// exit side's, what lies behind the one in front of the other.
    pub fn carry(&self, mesh: &Brick, face: Face) -> glam::Affine3A {
        let from = self.opening(mesh, face, Frame::default());
        let to = self.opening(mesh, self.exit(face), Frame::default());
        let turn = if to.face == face {
            // Half a turn about the side's upright (a wall's up stays up).
            let axis = if from.normal.y.abs() > 0.5 {
                from.u
            } else {
                glam::Vec3::Y
            };
            glam::Quat::from_axis_angle(axis, std::f32::consts::PI)
        } else {
            glam::Quat::IDENTITY
        };
        glam::Affine3A::from_translation(to.centre)
            * glam::Affine3A::from_quat(turn)
            * glam::Affine3A::from_translation(-from.centre)
    }
    /// The brick's collision when bodies pass: its box less a hole through
    /// each opening, as boxes (centre, size) in its own frame.
    pub fn frame_boxes(&self, mesh: &Brick) -> Vec<CollisionBox> {
        let size = glam::Vec3::new(
            mesh.footprint_studs[0] as f32 * STUD,
            mesh.height_plates as f32 * PLATE,
            mesh.footprint_studs[1] as f32 * STUD,
        );
        // Split the box along each hole's edges, then keep the cells no
        // hole runs through.
        let mut cuts: [Vec<f32>; 3] = std::array::from_fn(|a| vec![-size[a] / 2.0, size[a] / 2.0]);
        let holes: Vec<(usize, glam::Vec3, glam::Vec3)> = self
            .passages(mesh)
            .iter()
            .map(|o| {
                let axis = o.normal.abs().max_position();
                let reach = o.u.abs() * o.half.x + o.v.abs() * o.half.y;
                (axis, o.centre - reach, o.centre + reach)
            })
            .collect();
        for (axis, min, max) in &holes {
            for a in (0..3).filter(|a| a != axis) {
                cuts[a].extend([min[a], max[a]]);
            }
        }
        for c in &mut cuts {
            c.sort_by(f32::total_cmp);
            c.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
        }
        let mut out = Vec::new();
        for x in cuts[0].windows(2) {
            for y in cuts[1].windows(2) {
                for z in cuts[2].windows(2) {
                    let (lo, hi) = (
                        glam::Vec3::new(x[0], y[0], z[0]),
                        glam::Vec3::new(x[1], y[1], z[1]),
                    );
                    let middle = (lo + hi) * 0.5;
                    let open = holes.iter().any(|(axis, min, max)| {
                        (0..3)
                            .filter(|a| a != axis)
                            .all(|a| middle[a] > min[a] && middle[a] < max[a])
                    });
                    if !open {
                        out.push(CollisionBox {
                            center: middle.to_array(),
                            size: (hi - lo).to_array(),
                        });
                    }
                }
            }
        }
        out
    }
}
/// Give a quad moved from `old` to `moved` corners the texture of the same
/// size it had: its texture coordinates follow position as they did, from
/// its first corner (a quad whose coordinates do not follow its position
/// evenly keeps them).
fn retexture(quad: &mut Quad, old: &[glam::Vec3; 4], moved: &[glam::Vec3; 4]) {
    use glam::{Vec2, Vec3};
    let uv = quad.vertices.map(|v| Vec2::from(v.uv));
    let (e1, e2) = (old[1] - old[0], old[3] - old[0]);
    let (a, b, c) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
    let det = a * c - b * b;
    if det.abs() < 1e-10 {
        return;
    }
    // `d` in the old quad's own two directions.
    let along = |d: Vec3| {
        let (p, q) = (e1.dot(d), e2.dot(d));
        ((c * p - b * q) / det, (a * q - b * p) / det)
    };
    let texture = |s: f32, t: f32| uv[0] + (uv[1] - uv[0]) * s + (uv[3] - uv[0]) * t;
    let (s, t) = along(old[2] - old[0]);
    if texture(s, t).distance(uv[2]) > 1e-3 {
        return;
    }
    for (v, p) in quad.vertices.iter_mut().zip(moved) {
        let (s, t) = along(*p - moved[0]);
        v.uv = texture(s, t).to_array();
    }
}

/// The side facing the other way.
pub fn opposite(face: Face) -> Face {
    match face {
        Face::Top => Face::Bottom,
        Face::Bottom => Face::Top,
        Face::North => Face::South,
        Face::South => Face::North,
        Face::East => Face::West,
        Face::West => Face::East,
        Face::Omni => Face::Omni,
    }
}
/// The outward normal of a side in a brick's unrotated frame (north is -Z).
pub fn face_normal(face: Face) -> glam::Vec3 {
    match face {
        Face::Top => glam::Vec3::Y,
        Face::Bottom => glam::Vec3::NEG_Y,
        Face::North => glam::Vec3::NEG_Z,
        Face::East => glam::Vec3::X,
        Face::South => glam::Vec3::Z,
        Face::West => glam::Vec3::NEG_X,
        Face::Omni => glam::Vec3::ZERO,
    }
}

impl CatalogEntry {
    /// Hidden state variants still need native geometry and stable save identity.
    pub fn selectable(&self) -> bool {
        !self.display_name.is_empty() && !self.category.is_empty() && !self.subcategory.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brick {
    pub schema_version: u32,
    pub id: String,
    pub footprint_studs: [u32; 2],
    pub height_plates: u32,
    /// Slice order: original depth, descending height, then width left to right.
    /// b=attach both, u=above, d=below, x=occupied, -=empty.
    pub attachment_rows: Vec<String>,
    pub collision_boxes: Vec<CollisionBox>,
    /// A missing BLB box is not permission to use visual triangles for collision.
    pub needs_external_collision: bool,
    /// In order top, bottom, original north/east/south/west.
    pub coverage: Option<[Coverage; 6]>,
    pub quads: Vec<Quad>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionBox {
    pub center: [f32; 3],
    pub size: [f32; 3],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Coverage {
    pub hides_adjacent: bool,
    pub required_area: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Top,
    Side,
    BottomEdge,
    BottomLoop,
    Ramp,
    Print,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Face {
    Top,
    Bottom,
    North,
    East,
    South,
    West,
    Omni,
}
pub const FACES: [Face; 7] = [
    Face::Top,
    Face::Bottom,
    Face::North,
    Face::East,
    Face::South,
    Face::West,
    Face::Omni,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quad {
    pub face: Face,
    pub surface: Surface,
    /// Counterclockwise winding in native X-right/Y-up/-Z-forward coordinates.
    pub vertices: [Vertex; 4],
    /// Authored RGBA values may include legacy additive/subtractive sentinels.
    /// Absent means use the player's paint color, not authored white.
    pub colors: Option<[[f32; 4]; 4]>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// How much of each edge of a brick [`Brick::stretched`] keeps as it is,
/// in world units (half a stud): frames, sills, bevels and studs' edges.
pub const STRETCH_KEEP: f32 = 0.25;

impl Brick {
    /// Half the brick's grid size along each axis of its own frame.
    pub fn half_size(&self) -> glam::Vec3 {
        glam::Vec3::new(
            self.footprint_studs[0] as f32 * STUD,
            self.height_plates as f32 * PLATE,
            self.footprint_studs[1] as f32 * STUD,
        ) * 0.5
    }
    /// Where [`Self::stretched`] to `[width, depth, height]` moves a point
    /// of this shape's frame: the one map its faces, collision (boxes and
    /// collision recipes alike, [`crate::collision::CollisionBody::stretched`])
    /// and openings all follow.
    pub fn stretching(
        &self,
        [width, depth, height]: [u32; 3],
    ) -> Result<impl Fn(glam::Vec3) -> glam::Vec3 + use<>> {
        let from = self.half_size();
        let to = glam::Vec3::new(
            width as f32 * STUD,
            height as f32 * PLATE,
            depth as f32 * STUD,
        ) * 0.5;
        let keep = (from * 0.5).min(glam::Vec3::splat(STRETCH_KEEP));
        ensure!(
            (to - keep).min_element() > 0.0,
            "too small to stretch {} to",
            self.id
        );
        Ok(move |p: glam::Vec3| {
            glam::Vec3::from_array(std::array::from_fn(|a| {
                let (h, n, k) = (from[a], to[a], keep[a]);
                if p[a].abs() >= h - k {
                    p[a] + p[a].signum() * (n - h)
                } else {
                    p[a] * (n - k) / (h - k)
                }
            }))
        })
    }
    /// This shape at another size (`[width, depth]` studs and `height`
    /// plates), named `id`, as a nine-slice picture stretches: what lies
    /// within [`STRETCH_KEEP`] of an edge moves out with that edge
    /// unchanged and the middle stretches, so a window's frame keeps its
    /// width round a bigger pane. Studs and bottoms keep their texture's
    /// size (more studs); other surfaces stretch theirs.
    pub fn stretched(&self, id: &str, [width, depth, height]: [u32; 3]) -> Result<Brick> {
        let mut out = Brick {
            id: id.into(),
            footprint_studs: [width, depth],
            height_plates: height,
            ..self.clone()
        };
        let point = self
            .stretching([width, depth, height])
            .with_context(|| format!("Brick {id}"))?;
        let (from, to) = (self.half_size(), out.half_size());
        let keep = (from * 0.5).min(glam::Vec3::splat(STRETCH_KEEP));
        for quad in &mut out.quads {
            let old = quad.vertices.map(|v| glam::Vec3::from(v.position));
            let moved = old.map(&point);
            if matches!(
                quad.surface,
                Surface::Top | Surface::BottomEdge | Surface::BottomLoop
            ) {
                retexture(quad, &old, &moved);
            }
            for (v, p) in quad.vertices.iter_mut().zip(moved) {
                v.position = p.to_array();
            }
        }
        for b in &mut out.collision_boxes {
            let (c, h) = (glam::Vec3::from(b.center), glam::Vec3::from(b.size) * 0.5);
            let (lo, hi) = (point(c - h), point(c + h));
            b.center = ((lo + hi) * 0.5).to_array();
            b.size = (hi - lo).to_array();
        }
        // Each new grid cell takes the old one its middle stretches from.
        let cell = |i: u32, old: u32, unit: f32, a: usize| {
            let c = -to[a] + (i as f32 + 0.5) * unit;
            let (h, n, k) = (from[a], to[a], keep[a]);
            let back = if c.abs() >= n - k {
                c - c.signum() * (n - h)
            } else {
                c * (h - k) / (n - k)
            };
            (((back + h) / unit).floor() as i64).clamp(0, i64::from(old) - 1) as usize
        };
        let [old_width, old_depth] = self.footprint_studs;
        let rows: Vec<&[u8]> = self.attachment_rows.iter().map(|r| r.as_bytes()).collect();
        out.attachment_rows = (0..depth)
            .flat_map(|z| (0..height).map(move |y| (z, y)))
            .map(|(z, y)| {
                let row = cell(z, old_depth, STUD, 2) * self.height_plates as usize
                    + cell(y, self.height_plates, PLATE, 1);
                (0..width)
                    .map(|x| char::from(rows[row][cell(x, old_width, STUD, 0)]))
                    .collect()
            })
            .collect();
        if let Some(coverage) = &mut out.coverage {
            let ratio = to / from;
            let areas = [
                ratio.x * ratio.z,
                ratio.x * ratio.z,
                ratio.x * ratio.y,
                ratio.z * ratio.y,
                ratio.x * ratio.y,
                ratio.z * ratio.y,
            ];
            for (c, area) in coverage.iter_mut().zip(areas) {
                c.required_area *= area;
            }
        }
        out.validate()?;
        Ok(out)
    }
    /// Half the brick's grid size along a unit axis of its own frame.
    pub fn half_extent(&self, axis: glam::Vec3) -> f32 {
        (axis.abs() * self.half_size()).element_sum()
    }
    /// Two axes spanning a side, with the brick's half size along each;
    /// the first crossed with the second points out of the side.
    pub fn face_axes(&self, face: Face) -> [(glam::Vec3, f32); 2] {
        use glam::Vec3;
        let [u, v] = match face {
            Face::Top => [Vec3::Z, Vec3::X],
            Face::Bottom => [Vec3::X, Vec3::Z],
            Face::North => [Vec3::Y, Vec3::X],
            Face::South => [Vec3::X, Vec3::Y],
            Face::East => [Vec3::Y, Vec3::Z],
            Face::West | Face::Omni => [Vec3::Z, Vec3::Y],
        };
        [(u, self.half_extent(u)), (v, self.half_extent(v))]
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == BRICK_SCHEMA, "Unknown brick schema");
        ensure!(!self.id.is_empty(), "Empty brick ID");
        let [width, depth] = self.footprint_studs;
        ensure!(
            width > 0 && depth > 0 && self.height_plates > 0,
            "Empty brick dimensions"
        );
        ensure!(
            u64::from(width) * u64::from(depth) * u64::from(self.height_plates) <= 2_000_000,
            "Brick grid too large"
        );
        ensure!(
            self.attachment_rows.len() == (depth * self.height_plates) as usize,
            "Attachment row count mismatch"
        );
        for row in &self.attachment_rows {
            ensure!(
                row.len() == width as usize && row.bytes().all(|b| b"budx-".contains(&b)),
                "Invalid attachment grid row"
            );
        }
        ensure!(
            !self.quads.is_empty() && self.quads.len() <= 100_000,
            "Invalid quad count"
        );
        for quad in &self.quads {
            for v in &quad.vertices {
                ensure!(
                    v.position
                        .iter()
                        .chain(&v.normal)
                        .chain(&v.uv)
                        .all(|v| v.is_finite()),
                    "Non-finite brick vertex"
                );
                let length: f32 = v.normal.iter().map(|v| v * v).sum();
                ensure!((length - 1.0).abs() < 0.001, "Non-unit normal");
            }
            if let Some(colors) = &quad.colors {
                ensure!(
                    colors.iter().flatten().all(|c| c.is_finite()),
                    "Non-finite color"
                );
            }
        }
        for b in &self.collision_boxes {
            ensure!(
                b.center.iter().all(|v| v.is_finite())
                    && b.size.iter().all(|v| v.is_finite() && *v > 0.0),
                "Invalid collision box"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_portal_frame_leaves_a_sill_and_the_way_through_stays_upright() {
        let mesh = window();
        let link: Link = serde_json::from_str(
            r#"{"faces": ["north", "south"], "depth": 0.5, "pass": true, "name": "Portal",
                "frame": {"sides": 0.05, "top": 0.05, "bottom": 0.2}}"#,
        )
        .unwrap();
        link.validate(&mesh).unwrap();
        for opening in link.passages(&mesh) {
            let (lo, hi) = (
                opening.centre
                    - opening.u.abs() * opening.half.x
                    - opening.v.abs() * opening.half.y,
                opening.centre
                    + opening.u.abs() * opening.half.x
                    + opening.v.abs() * opening.half.y,
            );
            assert!(
                lo.abs_diff_eq(glam::Vec3::new(-0.95, -1.3, 0.0), 1e-5),
                "{lo}"
            );
            assert!(
                hi.abs_diff_eq(glam::Vec3::new(0.95, 1.45, 0.0), 1e-5),
                "{hi}"
            );
        }
        // The collision is the frame: nothing inside the hole, a 0.2 sill.
        let boxes = link.frame_boxes(&mesh);
        let inside = |p: glam::Vec3| {
            boxes.iter().any(|b| {
                let (c, h) = (glam::Vec3::from(b.center), glam::Vec3::from(b.size) / 2.0);
                (p - c).abs().cmple(h).all()
            })
        };
        assert!(
            !inside(glam::Vec3::new(0.0, -1.25, 0.0)) && !inside(glam::Vec3::new(0.9, 1.4, 0.1))
        );
        assert!(
            inside(glam::Vec3::new(0.0, -1.35, 0.0)) && inside(glam::Vec3::new(0.97, 0.0, 0.0))
        );
        // One number is the same frame all round; a stray key is refused.
        let even: Link =
            serde_json::from_str(r#"{"faces": ["north"], "frame": 0.1, "name": "P"}"#).unwrap();
        assert_eq!(even.frame, Frame::even(0.1));
        assert!(
            serde_json::from_str::<Link>(
                r#"{"faces": ["north"], "frame": {"side": 0.1}, "name": "P"}"#
            )
            .is_err()
        );
        // A wall portal (one open side) turns half about the upright, so
        // what goes in comes back out the same side standing up.
        let wall = Link {
            faces: vec![Face::South],
            ..link
        };
        let carry = wall.carry(&mesh, Face::South);
        assert!(
            carry
                .transform_vector3(glam::Vec3::Y)
                .abs_diff_eq(glam::Vec3::Y, 1e-5)
        );
        assert!(
            carry
                .transform_vector3(glam::Vec3::NEG_Z)
                .abs_diff_eq(glam::Vec3::Z, 1e-5)
        );
    }

    /// A 4x1 brick five bricks (15 plates) tall: the stock window's size.
    fn window() -> Brick {
        Brick {
            schema_version: BRICK_SCHEMA,
            id: "mesh/window".into(),
            footprint_studs: [4, 1],
            height_plates: 15,
            attachment_rows: vec!["bbbb".into(); 15],
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![],
        }
    }
    /// A quad of `surface` with these corners and texture coordinates.
    fn quad(surface: Surface, corners: [[f32; 3]; 4], uv: [[f32; 2]; 4]) -> Quad {
        let normal = (glam::Vec3::from(corners[1]) - glam::Vec3::from(corners[0]))
            .cross(glam::Vec3::from(corners[2]) - glam::Vec3::from(corners[1]))
            .normalize();
        Quad {
            face: Face::Omni,
            surface,
            vertices: std::array::from_fn(|i| Vertex {
                position: corners[i],
                normal: normal.to_array(),
                uv: uv[i],
            }),
            colors: None,
        }
    }

    #[test]
    fn a_stretched_window_keeps_its_frame_and_studs_and_grows_its_pane() {
        let mut mesh = window();
        // Studs on top, one texture repeat a stud; the right post of the
        // frame; the pane; studs on the top row of the grid.
        mesh.quads = vec![
            quad(
                Surface::Top,
                [[-1.0, 1.5, 0.25], [1.0, 1.5, 0.25], [1.0, 1.5, -0.25], [-1.0, 1.5, -0.25]],
                [[0.0, 0.0], [4.0, 0.0], [4.0, 1.0], [0.0, 1.0]],
            ),
            quad(
                Surface::Side,
                [[0.95, -1.5, 0.25], [1.0, -1.5, 0.25], [1.0, 1.5, 0.25], [0.95, 1.5, 0.25]],
                [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            ),
            quad(
                Surface::Side,
                [[-0.95, -1.3, 0.0], [0.95, -1.3, 0.0], [0.95, 1.45, 0.0], [-0.95, 1.45, 0.0]],
                [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            ),
        ];
        mesh.attachment_rows = std::iter::once("uuuu".to_owned())
            .chain(std::iter::repeat_n("xxxx".to_owned(), 13))
            .chain(std::iter::once("dddd".to_owned()))
            .collect();
        mesh.collision_boxes = vec![CollisionBox {
            center: [0.0, 0.0, 0.0],
            size: [2.0, 3.0, 0.5],
        }];
        let big = mesh.stretched("mesh/window@8x1x30", [8, 1, 30]).unwrap();
        assert_eq!((big.footprint_studs, big.height_plates), ([8, 1], 30));
        let corners = |q: &Quad| q.vertices.map(|v| glam::Vec3::from(v.position));
        let near = |a: glam::Vec3, b: [f32; 3]| a.abs_diff_eq(glam::Vec3::from(b), 1e-5);
        // Eight studs across the top, the same size as before.
        let top = &big.quads[0];
        assert!(near(corners(top)[2], [2.0, 3.0, -0.25]));
        assert_eq!(top.vertices[2].uv, [8.0, 1.0]);
        // The post moves out to the new edge, as thin as it was.
        let post = corners(&big.quads[1]);
        assert!(near(post[0], [1.95, -3.0, 0.25]) && near(post[2], [2.0, 3.0, 0.25]));
        assert_eq!(big.quads[1].vertices[2].uv, [1.0, 1.0]);
        // The pane fills the bigger opening.
        let pane = corners(&big.quads[2]);
        assert!(near(pane[0], [-1.95, -2.8, 0.0]) && near(pane[2], [1.95, 2.95, 0.0]));
        assert_eq!(big.attachment_rows.len(), 30);
        assert_eq!(big.attachment_rows[0], "uuuuuuuu");
        assert_eq!(big.attachment_rows[15], "xxxxxxxx");
        assert_eq!(big.attachment_rows[29], "dddddddd");
        assert!(near(glam::Vec3::from(big.collision_boxes[0].size), [4.0, 6.0, 0.5]));
        // The portal's way through grows the same and keeps its frame.
        let link: Link = serde_json::from_str(
            r#"{"faces": ["north", "south"], "depth": 0.5, "pass": true, "name": "Portal",
                "frame": {"sides": 0.05, "top": 0.05, "bottom": 0.2}}"#,
        )
        .unwrap();
        link.validate(&big).unwrap();
        for o in link.passages(&big) {
            let half = glam::Vec2::new(o.half.min_element(), o.half.max_element());
            assert!(half.abs_diff_eq(glam::Vec2::new(1.95, 2.875), 1e-5), "{o:?}");
        }
        // Too small to keep the edges it has.
        assert!(mesh.stretched("mesh/sliver", [4, 1, 1]).is_err());
    }
    fn reflection(faces: Vec<Face>) -> Reflection {
        Reflection {
            faces,
            depth: 0.5,
            inset: 0.1,
            tint: [1.0; 3],
            strength: 1.0,
        }
    }

    #[test]
    fn a_window_pane_mirror_sits_mid_brick_inside_its_frame_facing_out() {
        let mesh = window();
        let quads = reflection(vec![Face::North, Face::South]).quads(&mesh);
        assert_eq!(quads.len(), 2);
        for (quad, normal) in quads.iter().zip([glam::Vec3::NEG_Z, glam::Vec3::Z]) {
            let [a, b, c, _] = quad.map(glam::Vec3::from);
            // Counterclockwise seen from the side it faces.
            assert!((b - a).cross(c - b).normalize().abs_diff_eq(normal, 1e-5));
            // The pane's plane, a millimetre toward its viewer.
            assert!((a.dot(normal) - 0.001).abs() < 1e-5);
            // 2 wide and 3 tall, less a 0.1 frame on every edge.
            let size = quad
                .iter()
                .map(|p| glam::Vec3::from(*p))
                .fold(glam::Vec3::splat(-9.0), glam::Vec3::max);
            assert!(size.abs_diff_eq(glam::Vec3::new(0.9, 1.4, size.z), 1e-5));
        }
        // On the side itself (depth 0), a floor tile's top.
        let mut top = reflection(vec![Face::Top]);
        top.depth = 0.0;
        let quad = top.quads(&mesh)[0];
        assert!(quad.iter().all(|p| (p[1] - 1.501).abs() < 1e-5));
    }

    #[test]
    fn a_full_mirror_replaces_the_glass_it_covers_and_keeps_the_frame() {
        let mesh = window();
        let quad = |x: [f32; 2], y: [f32; 2], z: f32, alpha: Option<f32>| Quad {
            face: Face::North,
            surface: Surface::Side,
            vertices: [[x[0], y[0]], [x[1], y[0]], [x[1], y[1]], [x[0], y[1]]].map(|[x, y]| {
                Vertex {
                    position: [x, y, z],
                    normal: [0.0, 0.0, -1.0],
                    uv: [0.0; 2],
                }
            }),
            colors: alpha.map(|a| [[0.6, 0.8, 0.7, a]; 4]),
        };
        let mirror = reflection(vec![Face::North, Face::South]);
        // Glass reaching under the frame, off the brick's middle.
        let glass = quad([-0.95, 0.95], [-1.45, 1.45], 0.05, Some(0.4));
        assert!(mirror.replaces(&mesh, &glass));
        // The painted and the opaque frame stay.
        assert!(!mirror.replaces(&mesh, &quad([0.9, 1.0], [-1.4, 1.4], -0.25, None)));
        assert!(!mirror.replaces(&mesh, &quad([0.9, 1.0], [-1.4, 1.4], -0.25, Some(1.0))));
        // Glass across a side that does not reflect stays.
        let east = Quad {
            vertices: glass.vertices.map(|mut v| {
                v.position = [v.position[2], v.position[1], v.position[0] * 0.2];
                v
            }),
            ..glass.clone()
        };
        assert!(!mirror.replaces(&mesh, &east));
        // A partial mirror lets the brick's own look show through.
        let floor = Reflection { strength: 0.5, ..mirror };
        assert!(!floor.replaces(&mesh, &glass));
    }

    #[test]
    fn reflections_refuse_what_cannot_be_drawn() {
        let mesh = window();
        assert!(reflection(vec![Face::North]).validate(&mesh).is_ok());
        for bad in [
            reflection(vec![]),
            reflection(vec![Face::Omni]),
            reflection(vec![Face::North, Face::North]),
            Reflection { depth: 1.5, ..reflection(vec![Face::North]) },
            Reflection { strength: 0.0, ..reflection(vec![Face::North]) },
            Reflection { tint: [2.0, 1.0, 1.0], ..reflection(vec![Face::North]) },
            // A one-stud side is only 0.25 each way from its centre.
            Reflection { inset: 0.25, ..reflection(vec![Face::East]) },
        ] {
            assert!(bad.validate(&mesh).is_err(), "{bad:?}");
        }
        // Catalog entries without one still read, and write no field.
        let entry: CatalogEntry = serde_json::from_str(
            r#"{"id":"a","display_name":"","category":"","subcategory":"","mesh_id":"m",
            "collision_source":null,"icon_source":"","print_aspect_ratio":null,
            "orientation_fix":0,"can_cover":true,"indestructible":false,
            "special_kind":null,"other_properties":{}}"#,
        )
        .unwrap();
        assert!(entry.reflection.is_none());
        assert!(!serde_json::to_string(&entry).unwrap().contains("reflection"));
    }
}
