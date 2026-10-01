//! A copied build: bricks held relative to a pivot, to be placed again
//! anywhere on the stud grid in any quarter turn. This is the engine's
//! copy-and-place mechanism. Which bricks to copy, with which tool and for
//! whom is an Add-On's policy (`copy_build`); placing goes through the normal
//! plant rules (`Session::place_blueprint`).
//!
//! The pivot sits on a stud corner (x and z multiples of 0.5) at the
//! bottom of the copy, so a quarter turn about it keeps every brick on the
//! grid, and a copy placed at a grid point stays on the grid.
//!
//! A copy may hold a million bricks, so each is kept small
//! ([`CopyBrick`]: its kind and paint as indices, about 32 bytes, where a
//! world brick takes over 500) and is made a world brick only as it is
//! placed ([`Placement`]), a slice at a time.
use crate::{
    definitions::Definitions,
    grid::Bounds,
    mirror::{MirrorImage, Reflection},
};
use anyhow::{Context, Result, ensure};
use bri_world::{Brick, ContentRef};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Most bricks one copy may hold, whatever an Add-On asks for: the New
/// Duplicator's own limit for administrators.
pub const MAX_BLUEPRINT_BRICKS: usize = bri_package_runtime::ops::MAX_COPY_BRICKS as usize;

/// Most bricks of a copy its player is sent to show as the ghost: the
/// rest plant where the ghost says, unseen, as the New Duplicator showed
/// at most `MaxGhostBricks` of a selection.
pub const MAX_GHOST_BRICKS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blueprint {
    /// The item that shows and places the copy (an Add-On's tool).
    pub tool: String,
    /// The pivot's place in the world when the copy was taken.
    pub origin: [f32; 3],
    /// Grid size of the copy unturned: studs along x, plates, studs along z.
    pub size: [i32; 3],
    /// The brick definitions its bricks are, each once.
    pub kinds: Vec<String>,
    /// The prints its bricks show, each once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prints: Vec<ContentRef>,
    /// The bricks, positions relative to the pivot.
    pub bricks: Vec<CopyBrick>,
}

/// One brick of a copy: its shape and look, owned by nobody.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyBrick {
    /// Index into [`Blueprint::kinds`].
    #[serde(rename = "k")]
    pub kind: u32,
    #[serde(rename = "p")]
    pub position: [f32; 3],
    #[serde(rename = "t", default, skip_serializing_if = "is_zero")]
    pub quarter_turns: u8,
    #[serde(rename = "c", default, skip_serializing_if = "is_zero")]
    pub color: u8,
    #[serde(rename = "f", default, skip_serializing_if = "is_zero")]
    pub color_effect: u8,
    #[serde(rename = "s", default, skip_serializing_if = "is_zero")]
    pub shape_effect: u8,
    /// Index into [`Blueprint::prints`].
    #[serde(rename = "i", default, skip_serializing_if = "Option::is_none")]
    pub print: Option<u32>,
    /// Raycasting, colliding and rendering turned off, as bits 1, 2, 4.
    #[serde(rename = "o", default, skip_serializing_if = "is_zero")]
    pub off: u8,
}
fn is_zero(v: &u8) -> bool {
    *v == 0
}
const NO_RAYCAST: u8 = 1;
const NO_COLLIDE: u8 = 2;
const HIDDEN: u8 = 4;

/// The bricks of a mirrored or upside-down copy that had no exact image
/// (see [`crate::mirror`]): their definitions, each once, in copy order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inexact {
    /// Mirrored left to right.
    pub side: Vec<String>,
    /// Turned upside down.
    pub upside_down: Vec<String>,
}

/// A box outlined for one player while `tool` is in their hand (an
/// Add-On's selection), in world units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outline {
    pub tool: String,
    pub min: [f32; 3],
    pub max: [f32; 3],
}
impl Outline {
    /// Shape checks for an outline from the network.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            bri_package::id::is_content_ref(&self.tool, Some("weapon")),
            "Invalid outline tool"
        );
        let finite = |p: &[f32; 3]| p.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.0);
        ensure!(
            finite(&self.min)
                && finite(&self.max)
                && (0..3).all(|a| self.max[a] >= self.min[a]
                    && self.max[a] - self.min[a] <= bri_package_runtime::ops::MAX_BOX_SPAN + 1.0),
            "Invalid outline"
        );
        Ok(())
    }
}

/// A copy taken a brick at a time ([`Blueprint::capture`] in slices): each
/// brick as it stands in the world, then the pivot once all are in.
pub struct CopyBuilder {
    tool: String,
    kinds: Vec<String>,
    kind_of: HashMap<String, u32>,
    prints: Vec<ContentRef>,
    print_of: HashMap<ContentRef, u32>,
    bricks: Vec<CopyBrick>,
    min: [i32; 3],
    max: [i32; 3],
}
impl CopyBuilder {
    pub fn new(tool: &str) -> Self {
        Self {
            tool: tool.into(),
            kinds: Vec::new(),
            kind_of: HashMap::new(),
            prints: Vec::new(),
            print_of: HashMap::new(),
            bricks: Vec::new(),
            min: [i32::MAX; 3],
            max: [i32::MIN; 3],
        }
    }
    pub fn len(&self) -> usize {
        self.bricks.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bricks.is_empty()
    }
    /// Take `brick` as it stands: only its shape and look come along
    /// (owner, name, events, lights, emitters, items, sounds and vehicles
    /// stay with the original). Refused when it is off the grid or of a
    /// kind this server lacks, or the copy is full.
    pub fn push(&mut self, brick: &Brick, definitions: &Definitions) -> Result<()> {
        self.push_moved(brick, [0.0; 3], definitions)
    }
    /// [`Self::push`] with the brick moved by `shift`.
    pub fn push_moved(
        &mut self,
        brick: &Brick,
        shift: [f32; 3],
        definitions: &Definitions,
    ) -> Result<()> {
        ensure!(
            self.bricks.len() < MAX_BLUEPRINT_BRICKS,
            "A copy holds 1 to {MAX_BLUEPRINT_BRICKS} bricks"
        );
        let position: [f32; 3] = std::array::from_fn(|a| brick.position[a] + shift[a]);
        let bounds = Bounds::at(position, brick.quarter_turns, &definitions.get(brick)?.mesh)?;
        let ContentRef::Resolved(id) = &brick.definition else {
            anyhow::bail!("Unresolved brick definition");
        };
        let kind = match self.kind_of.get(id) {
            Some(&kind) => kind,
            None => {
                let kind = self.kinds.len() as u32;
                self.kinds.push(id.clone());
                self.kind_of.insert(id.clone(), kind);
                kind
            }
        };
        let print = brick.print.as_ref().map(|print| match self.print_of.get(print) {
            Some(&index) => index,
            None => {
                let index = self.prints.len() as u32;
                self.prints.push(print.clone());
                self.print_of.insert(print.clone(), index);
                index
            }
        });
        for axis in 0..3 {
            self.min[axis] = self.min[axis].min(bounds.min[axis]);
            self.max[axis] = self.max[axis].max(bounds.max()[axis]);
        }
        let off = (u8::from(!brick.raycast) * NO_RAYCAST)
            | (u8::from(!brick.colliding) * NO_COLLIDE)
            | (u8::from(!brick.visible) * HIDDEN);
        self.bricks.push(CopyBrick {
            kind,
            position,
            quarter_turns: brick.quarter_turns,
            color: brick.color,
            color_effect: brick.color_effect,
            shape_effect: brick.shape_effect,
            print,
            off,
        });
        Ok(())
    }
    /// The copy, its pivot the stud corner nearest the middle at its
    /// bottom plate.
    pub fn finish(self) -> Result<Blueprint> {
        ensure!(
            !self.bricks.is_empty(),
            "A copy holds 1 to {MAX_BLUEPRINT_BRICKS} bricks"
        );
        let (min, max) = (self.min, self.max);
        let pivot = [
            (min[0] + max[0]).div_euclid(2) as f32 * 0.5,
            min[1] as f32 * 0.2,
            (min[2] + max[2]).div_euclid(2) as f32 * 0.5,
        ];
        let mut bricks = self.bricks;
        for brick in &mut bricks {
            brick.position = std::array::from_fn(|a| brick.position[a] - pivot[a]);
        }
        Ok(Blueprint {
            tool: self.tool,
            origin: pivot,
            size: std::array::from_fn(|a| max[a] - min[a]),
            kinds: self.kinds,
            prints: self.prints,
            bricks,
        })
    }
}

/// What one kind of brick of a copy becomes as the player set it to be
/// placed (upside down, mirrored): its definition, and how its own turn
/// changes.
#[derive(Debug, Clone)]
struct KindImage {
    definition: ContentRef,
    /// Upside down: the image's turn, added.
    flip: Option<u8>,
    /// Mirrored: the image's turn, less the brick's own.
    mirror: Option<u8>,
}

/// How a copy goes into the world: where its pivot is, its turn, and
/// whether it is upside down and mirrored. Makes each brick of the copy
/// the world brick it plants as, so a big copy is placed a slice at a
/// time without a second copy of it.
#[derive(Debug, Clone)]
pub struct Placement {
    anchor: Vec3,
    turns: u8,
    height: f32,
    mirrored: bool,
    kinds: Vec<KindImage>,
}
impl Placement {
    /// `copy` placed with its pivot at `anchor`, turned `turns` quarter
    /// turns clockwise seen from above, upside down if `flipped`, then
    /// mirrored across its x axis if `mirrored` (the two commute). Also
    /// the bricks that had no exact image, each once: worked out per kind,
    /// not per brick.
    pub fn new(
        copy: &Blueprint,
        anchor: [f32; 3],
        turns: u8,
        (flipped, mirrored): (bool, bool),
        mut image: impl FnMut(&str, Reflection) -> MirrorImage,
    ) -> (Self, Inexact) {
        let mut inexact = Inexact::default();
        let kinds = copy
            .kinds
            .iter()
            .map(|id| {
                let mut kind = KindImage {
                    definition: ContentRef::Resolved(id.clone()),
                    flip: None,
                    mirror: None,
                };
                let mut id = id.clone();
                if flipped {
                    let found = image(&id, Reflection::UpsideDown);
                    if !found.exact && !inexact.upside_down.contains(&id) {
                        inexact.upside_down.push(id.clone());
                    }
                    kind.flip = Some(found.turns);
                    id = found.definition;
                }
                if mirrored {
                    let found = image(&id, Reflection::Side);
                    if !found.exact && !inexact.side.contains(&id) {
                        inexact.side.push(id.clone());
                    }
                    kind.mirror = Some(found.turns);
                    id = found.definition;
                }
                kind.definition = ContentRef::Resolved(id);
                kind
            })
            .collect();
        (
            Self {
                anchor: Vec3::from(anchor),
                turns: turns % 4,
                height: copy.size[1] as f32 * 0.2,
                mirrored,
                kinds,
            },
            inexact,
        )
    }

    /// Brick `b` of `copy` as it goes into the world.
    pub fn brick(&self, copy: &Blueprint, b: &CopyBrick) -> Brick {
        let kind = &self.kinds[b.kind as usize];
        let mut position = Vec3::from(b.position);
        let mut own = b.quarter_turns % 4;
        if let Some(turns) = kind.flip {
            position.y = self.height - position.y;
            // Top to bottom commutes with a quarter turn.
            own = (turns + own) % 4;
        }
        if let Some(turns) = kind.mirror {
            position.x = -position.x;
            own = (turns + 4 - own) % 4;
        }
        let mut brick = Brick::new(
            kind.definition.clone(),
            (self.anchor + turn(position, self.turns)).to_array(),
            0,
        );
        brick.quarter_turns = (own + self.turns) % 4;
        brick.color = b.color;
        brick.color_effect = b.color_effect;
        brick.shape_effect = b.shape_effect;
        brick.print = b.print.and_then(|i| copy.prints.get(i as usize).cloned());
        brick.raycast = b.off & NO_RAYCAST == 0;
        brick.colliding = b.off & NO_COLLIDE == 0;
        brick.visible = b.off & HIDDEN == 0;
        brick
    }

    /// Whether the copy is placed mirrored.
    pub fn mirrored(&self) -> bool {
        self.mirrored
    }
}

impl Blueprint {
    /// Copy `bricks` as they stand in the world ([`CopyBuilder`]).
    pub fn capture(tool: &str, bricks: &[Brick], definitions: &Definitions) -> Result<Self> {
        ensure!(
            !bricks.is_empty() && bricks.len() <= MAX_BLUEPRINT_BRICKS,
            "A copy holds 1 to {MAX_BLUEPRINT_BRICKS} bricks"
        );
        let mut builder = CopyBuilder::new(tool);
        for brick in bricks {
            builder.push(brick, definitions)?;
        }
        builder.finish()
    }

    pub fn len(&self) -> usize {
        self.bricks.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bricks.is_empty()
    }

    /// Brick `i` relative to the pivot, owned by nobody.
    pub fn brick(&self, i: usize) -> Brick {
        let (placement, _) = Placement::new(self, [0.0; 3], 0, (false, false), |_, _| {
            unreachable!("neither mirrored nor upside down")
        });
        placement.brick(self, &self.bricks[i])
    }

    /// Every brick relative to the pivot ([`Self::brick`]).
    pub fn world_bricks(&self) -> Vec<Brick> {
        self.placed([0.0; 3], 0)
    }

    /// Shape checks for a copy from the network or a file.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.bricks.is_empty() && self.bricks.len() <= MAX_BLUEPRINT_BRICKS,
            "A copy holds 1 to {MAX_BLUEPRINT_BRICKS} bricks"
        );
        ensure!(
            bri_package::id::is_content_ref(&self.tool, Some("weapon")),
            "Invalid copy tool"
        );
        let finite = |p: &[f32; 3]| p.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.0);
        ensure!(finite(&self.origin), "Invalid copy origin");
        ensure!(
            self.size.iter().all(|v| (1..=100_000).contains(v)),
            "Invalid copy size"
        );
        ensure!(
            !self.kinds.is_empty()
                && self.kinds.len() <= self.bricks.len()
                && self.prints.len() <= self.bricks.len()
                && self.kinds.iter().all(|id| !id.is_empty() && id.len() <= 512),
            "Invalid copied brick kinds"
        );
        for brick in &self.bricks {
            ensure!(
                finite(&brick.position)
                    && brick.quarter_turns < 4
                    && (brick.kind as usize) < self.kinds.len()
                    && brick.print.is_none_or(|i| (i as usize) < self.prints.len()),
                "Invalid copied brick"
            );
        }
        Ok(())
    }

    /// At most `most` of the copy's bricks, spread evenly through it and
    /// its first brick first (the brick a stack was taken from): what its
    /// player is shown as the ghost.
    pub fn ghost(&self, most: usize) -> Self {
        let mut ghost = Self {
            tool: self.tool.clone(),
            origin: self.origin,
            size: self.size,
            kinds: self.kinds.clone(),
            prints: self.prints.clone(),
            bricks: Vec::new(),
        };
        let n = self.bricks.len();
        ghost.bricks = if n <= most {
            self.bricks.clone()
        } else {
            (0..most.max(1)).map(|i| self.bricks[i * n / most.max(1)]).collect()
        };
        ghost
    }

    /// The copy's bricks with the pivot at `anchor`, turned `turns` quarter
    /// turns clockwise seen from above (the way a brick's own
    /// `quarter_turns` turns it).
    pub fn placed(&self, anchor: [f32; 3], turns: u8) -> Vec<Brick> {
        let (placement, _) = Placement::new(self, anchor, turns, (false, false), |_, _| {
            unreachable!("neither mirrored nor upside down")
        });
        self.bricks
            .iter()
            .map(|b| placement.brick(self, b))
            .collect()
    }

    /// The copy as the player set it to be placed: upside down if
    /// `flipped`, then mirrored across its x axis if `mirrored` (the two
    /// commute). Host and player both place copies through [`Placement`],
    /// so the ghost is what plants. Returns the copy and the bricks that
    /// had no exact image.
    pub fn seen(
        &self,
        flipped: bool,
        mirrored: bool,
        image: impl FnMut(&str, Reflection) -> MirrorImage,
    ) -> (Self, Inexact) {
        let (placement, inexact) = Placement::new(self, [0.0; 3], 0, (flipped, mirrored), image);
        let mut copy = self.clone();
        copy.kinds = placement
            .kinds
            .iter()
            .map(|k| match &k.definition {
                ContentRef::Resolved(id) => id.clone(),
                ContentRef::Unresolved { .. } => unreachable!("copies hold resolved kinds"),
            })
            .collect();
        for (brick, b) in copy.bricks.iter_mut().zip(&self.bricks) {
            let placed = placement.brick(self, b);
            brick.position = placed.position;
            brick.quarter_turns = placed.quarter_turns;
        }
        (copy, inexact)
    }

    /// The copy seen in a mirror standing across its pivot's x axis: each
    /// brick moves to the other side and becomes its mirror image (itself
    /// turned, or its twin; see [`crate::mirror`]). The pivot and the size
    /// stay. Mirroring across z is this turned half way round. Returns the
    /// copy and the bricks that had no exact image, each once.
    pub fn mirrored(&self, mut image: impl FnMut(&str) -> MirrorImage) -> (Self, Vec<String>) {
        let (copy, inexact) = self.seen(false, true, |id, _| image(id));
        (copy, inexact.side)
    }

    /// The copy upside down: each brick moves to the other side of the
    /// copy's middle plate and becomes its image top to bottom (itself, or
    /// its twin; see [`crate::mirror`]). The pivot and the size stay, so it
    /// stands where it stood. Returns the copy and the bricks that had no
    /// exact image, each once.
    pub fn flipped(&self, mut image: impl FnMut(&str) -> MirrorImage) -> (Self, Vec<String>) {
        let (copy, inexact) = self.seen(true, false, |id, _| image(id));
        (copy, inexact.upside_down)
    }

    /// Grid size turned `turns` quarter turns: studs along x, plates,
    /// studs along z.
    pub fn turned_size(&self, turns: u8) -> [i32; 3] {
        let [x, y, z] = self.size;
        if turns.is_multiple_of(2) {
            [x, y, z]
        } else {
            [z, y, x]
        }
    }
}

/// One offset turned like `Brick::transform`: each quarter turn takes
/// (x, z) to (-z, x).
fn turn(offset: Vec3, turns: u8) -> Vec3 {
    let mut v = offset;
    for _ in 0..turns % 4 {
        v = Vec3::new(-v.z, v.y, v.x);
    }
    v
}

/// The nearest pivot point: a stud corner at a plate boundary.
pub fn snap_anchor(p: [f32; 3]) -> [f32; 3] {
    [
        (p[0] / 0.5).round() * 0.5,
        (p[1] / 0.2).round() * 0.2,
        (p[2] / 0.5).round() * 0.5,
    ]
}

/// Move a copy's pivot with the brick shift keys, relative to the body's
/// facing as a single ghost moves. A super shift moves by the copy's own
/// size, as a super shift moves a brick by its size.
pub fn shift(
    anchor: [f32; 3],
    size: [i32; 3],
    forward: Vec3,
    away: i32,
    left: i32,
    up: i32,
    super_shift: bool,
) -> [f32; 3] {
    let facing = crate::ghost::cardinal(forward);
    let leftward = Vec3::Y.cross(facing);
    let mut delta = facing * away as f32 + leftward * left as f32;
    if super_shift {
        delta.x *= size[0] as f32;
        delta.z *= size[2] as f32;
    }
    delta *= 0.5;
    delta.y = up as f32 * 0.2 * if super_shift { size[1] as f32 } else { 1.0 };
    snap_anchor((Vec3::from(anchor) + delta).to_array())
}

/// A copy kept by name on the host (a duplicator's `/saveDup`), to be
/// held again later, on this world or another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedCopy {
    pub schema_version: u32,
    /// Who saved it, as their name showed then.
    pub saved_by: String,
    /// The colours the bricks' palette indices meant where it was saved.
    pub palette: Vec<[f32; 4]>,
    pub copy: Blueprint,
}
impl SavedCopy {
    pub const SCHEMA_VERSION: u32 = 1;
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == Self::SCHEMA_VERSION,
            "Unsupported saved copy schema {}",
            self.schema_version
        );
        ensure!(
            self.palette.len() <= 256 && self.saved_by.len() <= 256,
            "Invalid saved copy"
        );
        self.copy.validate()
    }
}

impl Blueprint {
    /// Copy `bricks` that stand in some frame of their own rather than on
    /// this world's grid, as v20 duplication files hold them (relative to
    /// their first brick, or where they stood on the saving server). The
    /// whole build moves by [`loose_shift`]; a brick still off the grid
    /// after that, or of a kind this server lacks, is left out. Returns
    /// the copy and how many were.
    pub fn from_loose(
        tool: &str,
        bricks: &[Brick],
        definitions: &Definitions,
    ) -> Result<(Self, usize)> {
        let first = bricks
            .iter()
            .find(|b| definitions.get(b).is_ok())
            .context("No brick of the copy is on this server")?;
        let shift = loose_shift(first, definitions)?;
        let mut builder = CopyBuilder::new(tool);
        for brick in bricks {
            if builder.len() < MAX_BLUEPRINT_BRICKS {
                let _ = builder.push_moved(brick, shift, definitions);
            }
        }
        let left_out = bricks.len() - builder.len();
        Ok((builder.finish()?, left_out))
    }
}

/// The least move that puts `first` (a brick of a kind this server has)
/// on the grid: what a loose copy moves by.
pub fn loose_shift(first: &Brick, definitions: &Definitions) -> Result<[f32; 3]> {
    let mesh = &definitions.get(first)?.mesh;
    let [w, d] = mesh.footprint_studs.map(|v| v as f32);
    let h = mesh.height_plates as f32;
    let size = if first.quarter_turns.is_multiple_of(2) {
        [w, h, d]
    } else {
        [d, h, w]
    };
    Ok(std::array::from_fn(|axis| {
        let cell = crate::grid::CELL[axis];
        let lower = first.position[axis] - size[axis] * cell * 0.5;
        (lower / cell).round() * cell - lower
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::Definition;
    use bri_content::{
        brick::Brick as Mesh,
        collision::{CollisionBody, Part},
    };
    use rapier3d::prelude::SharedShape;

    fn definitions() -> Definitions {
        let definition = |w: u32, d: u32, h: u32| Definition {
            mesh: Mesh {
                schema_version: 1,
                id: format!("{w}x{d}x{h}"),
                footprint_studs: [w, d],
                height_plates: h,
                attachment_rows: vec!["b".repeat(w as usize); (d * h) as usize],
                collision_boxes: vec![],
                needs_external_collision: false,
                coverage: None,
                quads: vec![],
            },
            shape: SharedShape::cuboid(w as f32 * 0.25, h as f32 * 0.1, d as f32 * 0.25),
            collision: CollisionBody {
                id: "box".into(),
                parts: vec![Part::Box {
                    center: [0.0; 3],
                    size: [w as f32 * 0.5, h as f32 * 0.2, d as f32 * 0.5],
                }],
            },
            indestructible: false,
            special: Default::default(),
            reflection: None,
            link: None,
            glass: [0.0; 4],
        };
        Definitions {
            entries: [
                ("2x1".into(), definition(2, 1, 1)),
                ("1x1".into(), definition(1, 1, 3)),
            ]
            .into(),
        }
    }
    fn brick(id: &str, position: [f32; 3], turns: u8) -> Brick {
        let mut b = Brick::new(ContentRef::Resolved(id.into()), position, 7);
        b.quarter_turns = turns;
        b.color = 3;
        b
    }

    #[test]
    fn a_copy_turns_about_a_stud_corner_and_stays_on_the_grid() {
        let defs = definitions();
        // A 2x1 plate with a 1x1 brick on its left stud.
        let source = [
            brick("2x1", [0.5, 0.1, 0.25], 0),
            brick("1x1", [0.25, 0.5, 0.25], 0),
        ];
        let copy = Blueprint::capture("dup:weapon/tool", &source, &defs).unwrap();
        assert_eq!(copy.origin, [0.5, 0.0, 0.0]);
        assert_eq!(copy.size, [2, 4, 1]);
        assert!(copy.world_bricks().iter().all(|b| b.owner == 0 && b.color == 3));
        copy.validate().unwrap();
        // Unturned at its own origin it is the source again.
        let same = copy.placed(copy.origin, 0);
        for (a, b) in same.iter().zip(&source) {
            assert_eq!(a.position, b.position);
        }
        for turns in 0..4 {
            let placed = copy.placed([3.0, 1.0, -2.5], turns);
            for (brick, original) in placed.iter().zip(&source) {
                // Every brick lands on the grid.
                Bounds::new(brick, &defs.get(brick).unwrap().mesh).unwrap();
                // And matches the source turned as a whole about the pivot.
                let local = Vec3::new(0.2, 0.05, 0.1);
                let world = original.transform().transform_point3(local) - Vec3::from(copy.origin);
                let expected =
                    glam::Mat4::from_rotation_y(-(turns as f32) * std::f32::consts::FRAC_PI_2)
                        .transform_point3(world)
                        + Vec3::new(3.0, 1.0, -2.5);
                let got = brick.transform().transform_point3(local);
                assert!(
                    got.distance(expected) < 1e-4,
                    "{turns}: {got} vs {expected}"
                );
            }
        }
        assert_eq!(copy.turned_size(1), [1, 4, 2]);
    }

    #[test]
    fn shifts_follow_the_body_and_super_shifts_move_by_the_copy() {
        let north = Vec3::new(0.0, 0.0, -1.0);
        assert_eq!(
            shift([0.0; 3], [4, 3, 2], north, 1, 0, 0, false),
            [0.0, 0.0, -0.5]
        );
        assert_eq!(
            shift([0.0; 3], [4, 3, 2], north, 0, 1, 0, false),
            [-0.5, 0.0, 0.0]
        );
        assert_eq!(
            shift([0.0; 3], [4, 3, 2], north, 1, 0, 0, true),
            [0.0, 0.0, -1.0]
        );
        assert_eq!(shift([0.0; 3], [4, 3, 2], north, 0, 0, 1, true)[1], 0.6);
        assert_eq!(snap_anchor([0.26, 0.31, -0.74]), [0.5, 0.4, -0.5]);
    }

    #[test]
    fn copies_refuse_empty_or_malformed_contents() {
        let defs = definitions();
        assert!(Blueprint::capture("dup:weapon/tool", &[], &defs).is_err());
        let error = Blueprint::capture(
            "dup:weapon/tool",
            &[brick("2x1", [0.5, 0.1, 0.25], 1)],
            &defs,
        )
        .unwrap_err()
        .to_string();
        // A 2x1 turned once is 1x2: x 0.5 is off its grid.
        assert!(error.contains("grid"), "{error}");
        let good = Blueprint::capture(
            "dup:weapon/tool",
            &[brick("2x1", [0.25, 0.1, 0.5], 1)],
            &defs,
        )
        .unwrap();
        let mut bad = good.clone();
        bad.tool = "not an item".into();
        assert!(bad.validate().is_err());
        let mut bad = good.clone();
        bad.bricks[0].position[0] = f32::NAN;
        assert!(bad.validate().is_err());
    }
}
