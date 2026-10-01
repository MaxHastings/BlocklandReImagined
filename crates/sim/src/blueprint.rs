//! A copied build: bricks held relative to a pivot, to be placed again
//! anywhere on the stud grid in any quarter turn. This is the engine's
//! copy-and-place mechanism. Which bricks to copy, with which tool and for
//! whom is an Add-On's policy (`copy_build`); placing goes through the normal
//! plant rules (`Session::place_blueprint`).
//!
//! The pivot sits on a stud corner (x and z multiples of 0.5) at the
//! bottom of the copy, so a quarter turn about it keeps every brick on the
//! grid, and a copy placed at a grid point stays on the grid.
use crate::{definitions::Definitions, grid::Bounds, mirror::MirrorImage};
use anyhow::{Context, Result, ensure};
use bri_world::{Brick, ContentRef};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Most bricks one copy may hold, whatever an Add-On asks for.
pub const MAX_BLUEPRINT_BRICKS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blueprint {
    /// The item that shows and places the copy (an Add-On's tool).
    pub tool: String,
    /// The pivot's place in the world when the copy was taken.
    pub origin: [f32; 3],
    /// Grid size of the copy unturned: studs along x, plates, studs along z.
    pub size: [i32; 3],
    /// The bricks, positions relative to the pivot, owned by nobody.
    pub bricks: Vec<Brick>,
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

impl Blueprint {
    /// Copy `bricks` as they stand in the world. Only their shape and look
    /// come along: owner, name, events, lights, emitters, items, sounds and
    /// vehicles stay with the original.
    pub fn capture(tool: &str, bricks: &[Brick], definitions: &Definitions) -> Result<Self> {
        ensure!(
            !bricks.is_empty() && bricks.len() <= MAX_BLUEPRINT_BRICKS,
            "A copy holds 1 to {MAX_BLUEPRINT_BRICKS} bricks"
        );
        let mut min = [i32::MAX; 3];
        let mut max = [i32::MIN; 3];
        for brick in bricks {
            let bounds = Bounds::new(brick, &definitions.get(brick)?.mesh)?;
            for axis in 0..3 {
                min[axis] = min[axis].min(bounds.min[axis]);
                max[axis] = max[axis].max(bounds.max()[axis]);
            }
        }
        // The stud corner nearest the middle, at the bottom plate.
        let pivot = [
            (min[0] + max[0]).div_euclid(2) as f32 * 0.5,
            min[1] as f32 * 0.2,
            (min[2] + max[2]).div_euclid(2) as f32 * 0.5,
        ];
        let bricks = bricks
            .iter()
            .map(|b| {
                let mut copy = Brick::new(
                    b.definition.clone(),
                    (Vec3::from(b.position) - Vec3::from(pivot)).to_array(),
                    0,
                );
                copy.quarter_turns = b.quarter_turns;
                copy.color = b.color;
                copy.print.clone_from(&b.print);
                copy.color_effect = b.color_effect;
                copy.shape_effect = b.shape_effect;
                copy.raycast = b.raycast;
                copy.colliding = b.colliding;
                copy.visible = b.visible;
                copy
            })
            .collect();
        Ok(Self {
            tool: tool.into(),
            origin: pivot,
            size: std::array::from_fn(|a| max[a] - min[a]),
            bricks,
        })
    }

    /// Shape checks for a copy from the network.
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
        for brick in &self.bricks {
            ensure!(
                finite(&brick.position)
                    && brick.quarter_turns < 4
                    && matches!(&brick.definition, ContentRef::Resolved(id) if !id.is_empty() && id.len() <= 512),
                "Invalid copied brick"
            );
        }
        Ok(())
    }

    /// The copy's bricks with the pivot at `anchor`, turned `turns` quarter
    /// turns clockwise seen from above (the way a brick's own
    /// `quarter_turns` turns it).
    pub fn placed(&self, anchor: [f32; 3], turns: u8) -> Vec<Brick> {
        let anchor = Vec3::from(anchor);
        self.bricks
            .iter()
            .map(|b| {
                let mut brick = b.clone();
                brick.position = (anchor + turn(Vec3::from(b.position), turns)).to_array();
                brick.quarter_turns = (b.quarter_turns + turns) % 4;
                brick
            })
            .collect()
    }

    /// The copy seen in a mirror standing across its pivot's x axis: each
    /// brick moves to the other side and becomes its mirror image (itself
    /// turned, or its twin; see [`crate::mirror`]). The pivot and the size
    /// stay. Mirroring across z is this turned half way round. Returns the
    /// copy and how many bricks had no exact image.
    pub fn mirrored(&self, mut image: impl FnMut(&str) -> MirrorImage) -> (Self, usize) {
        let mut inexact = 0;
        let bricks = self
            .bricks
            .iter()
            .map(|b| {
                let mut brick = b.clone();
                brick.position[0] = -brick.position[0];
                if let ContentRef::Resolved(id) = &b.definition {
                    let found = image(id);
                    inexact += usize::from(!found.exact);
                    brick.quarter_turns = (found.turns + 4 - b.quarter_turns % 4) % 4;
                    brick.definition = ContentRef::Resolved(found.definition);
                }
                brick
            })
            .collect();
        (
            Self {
                tool: self.tool.clone(),
                origin: self.origin,
                size: self.size,
                bricks,
            },
            inexact,
        )
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
    /// whole build moves by the least that puts its first brick on the
    /// grid; a brick still off the grid after that, or of a kind this
    /// server lacks, is left out. Returns the copy and how many were.
    pub fn from_loose(
        tool: &str,
        bricks: &[Brick],
        definitions: &Definitions,
    ) -> Result<(Self, usize)> {
        let known: Vec<&Brick> = bricks
            .iter()
            .filter(|b| definitions.get(b).is_ok())
            .collect();
        let first = known.first().context("No brick of the copy is on this server")?;
        let mesh = &definitions.get(first)?.mesh;
        let [w, d] = mesh.footprint_studs.map(|v| v as f32);
        let h = mesh.height_plates as f32;
        let size = if first.quarter_turns.is_multiple_of(2) {
            [w, h, d]
        } else {
            [d, h, w]
        };
        let shift: [f32; 3] = std::array::from_fn(|axis| {
            let cell = crate::grid::CELL[axis];
            let lower = first.position[axis] - size[axis] * cell * 0.5;
            (lower / cell).round() * cell - lower
        });
        let mut fitted = Vec::with_capacity(known.len());
        for brick in known {
            let mut brick = brick.clone();
            brick.position = (Vec3::from(brick.position) + Vec3::from(shift)).to_array();
            if Bounds::new(&brick, &definitions.get(&brick)?.mesh).is_ok() {
                fitted.push(brick);
            }
        }
        let left_out = bricks.len() - fitted.len();
        Ok((Self::capture(tool, &fitted, definitions)?, left_out))
    }
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
        assert!(copy.bricks.iter().all(|b| b.owner == 0 && b.color == 3));
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
