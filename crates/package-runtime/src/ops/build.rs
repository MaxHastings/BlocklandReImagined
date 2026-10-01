//! Operations behind the `build` capability.
use super::*;

/// Copy the stack at `brick` for `player` to place with `tool`, as
/// v20's duplicators select one (`reach`, `rule`), cut short at
/// `limit` bricks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopyBuild {
    pub player: u64,
    pub brick: u64,
    pub limit: u32,
    pub reach: StackReach,
    pub rule: CopyRule,
    pub tool: String,
    pub hold: CopyHold,
}
impl ScriptOp for CopyBuild {
    const CAPABILITY: &str = "build";
    const NAME: &str = "copy_build";
    fn bounded(&self) -> bool {
        let CopyBuild { limit, tool, .. } = self;
        (1..=MAX_COPY_BRICKS).contains(limit) && item(tool)
    }
}

/// Copy every brick lying wholly inside the box from `min` to `max`
/// (world units, grown out to the stud and plate grid; not `limited`,
/// every brick reaching into it) that `rule` lets `player` take, for
/// them to place with `tool`, cut short at `limit` bricks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopyBox {
    pub player: u64,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub limited: bool,
    pub limit: u32,
    pub rule: CopyRule,
    pub tool: String,
    pub hold: CopyHold,
}
impl ScriptOp for CopyBox {
    const CAPABILITY: &str = "build";
    const NAME: &str = "copy_box";
    fn bounded(&self) -> bool {
        let CopyBox {
            min,
            max,
            limit,
            tool,
            ..
        } = self;
        (1..=MAX_COPY_BRICKS).contains(limit) && item(tool) && span(min, max)
    }
}

/// Light the bricks `player`'s copy was taken from in the palette
/// colour nearest `color` (RGBA), glowing, for `seconds`, then give
/// them their own colours back, as v20's duplicators showed a
/// selection. Everyone sees it. A negative `seconds` keeps them lit
/// until the copy is let go or lit again; 0 puts them out now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HighlightCopy {
    pub player: u64,
    /// `None` lights them in their own colours (only the glow, as the
    /// New Duplicator did).
    pub color: Option<[f32; 4]>,
    pub seconds: f32,
}
impl ScriptOp for HighlightCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "highlight_copy";
    fn bounded(&self) -> bool {
        let HighlightCopy { color, seconds, .. } = self;
        color.iter().flatten().all(|c| (0.0..=1.0).contains(c)) && *seconds <= 60.0
    }
}

/// Keep the copy `player` holds on the host under `name` (see
/// [`copy_name`]). One saved under that name before is replaced, or,
/// without `overwrite`, kept, and the save reports `exists`. The
/// package's `on_copy` hears how it went (`action` `"save"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveCopy {
    pub player: u64,
    pub name: String,
    pub overwrite: bool,
}
impl ScriptOp for SaveCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "save_copy";
    fn bounded(&self) -> bool {
        let SaveCopy { name, .. } = self;
        copy_name(name).as_deref() == Some(name.as_str())
    }
}

/// The names copies are saved under on the host that contain `filter`
/// (any case; every one when empty), in order: the package's `on_copy`
/// hears them (`action` `"list"`, `names`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListCopies {
    pub player: u64,
    pub filter: String,
}
impl ScriptOp for ListCopies {
    const CAPABILITY: &str = "build";
    const NAME: &str = "list_copies";
    fn bounded(&self) -> bool {
        let ListCopies { filter, .. } = self;
        filter.is_empty() || copy_name(filter).as_deref() == Some(filter.as_str())
    }
}

/// Give `player` the copy saved under `name` to place with `tool`, at
/// most `limit` bricks of it (the first ones saved), replacing any copy
/// they hold (or, with `whole`, nothing when it holds more). Saved
/// copies are the host's: copies saved with any
/// duplicator, and v20 duplication files in the host's saves. The
/// package's `on_copy` hears how it went (`action` `"load"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadCopy {
    pub player: u64,
    pub name: String,
    pub limit: u32,
    pub tool: String,
    pub partial: bool,
    /// Take nothing, and report `limit`, when the copy holds more.
    pub whole: bool,
}
impl ScriptOp for LoadCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "load_copy";
    fn bounded(&self) -> bool {
        let LoadCopy {
            name, limit, tool, ..
        } = self;
        copy_name(name).as_deref() == Some(name.as_str())
            && (1..=MAX_COPY_BRICKS).contains(limit)
            && item(tool)
    }
}

/// Mirror the copy `player` holds across `axis`. It shows and plants
/// mirrored; mirroring it again the same way puts it back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirrorCopy {
    pub player: u64,
    pub axis: MirrorAxis,
}
impl ScriptOp for MirrorCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "mirror_copy";
    fn bounded(&self) -> bool {
        true
    }
}

/// Mirror `player`'s ghost brick (the brick in their hand, where it
/// would plant) across `axis` where it stands: it becomes its mirror
/// image, itself turned or its twin. A brick with no exact image in
/// that mirror stays as it is and the player is told `asymmetric`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirrorGhost {
    pub player: u64,
    pub axis: MirrorAxis,
    pub asymmetric: String,
}
impl ScriptOp for MirrorGhost {
    const CAPABILITY: &str = "build";
    const NAME: &str = "mirror_ghost";
    fn bounded(&self) -> bool {
        let MirrorGhost { asymmetric, .. } = self;
        chat(asymmetric)
    }
}

/// Move the copy `player` holds against the surface at `point` whose
/// outward `normal` is given, as a ghost brick is put where it is
/// aimed: its box's middle sits half its size out along the normal,
/// on the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoveCopy {
    pub player: u64,
    pub point: [f32; 3],
    pub normal: [f32; 3],
}
impl ScriptOp for MoveCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "move_copy";
    fn bounded(&self) -> bool {
        let MoveCopy { point, normal, .. } = self;
        finite(point) && point.iter().all(|v| v.abs() <= 1_000_000.0) && finite(normal)
    }
}

/// Take away the copy `player` holds, as if they had never copied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropCopy {
    pub player: u64,
}
impl ScriptOp for DropCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "drop_copy";
    fn bounded(&self) -> bool {
        true
    }
}

/// Give `player` the copy they hold as a selection
/// ([`CopyHold::hidden`]) to place, where it was taken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowCopy {
    pub player: u64,
}
impl ScriptOp for ShowCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "show_copy";
    fn bounded(&self) -> bool {
        true
    }
}

/// Keep the copy `player` holds as a selection only: the ghost they
/// place it with goes, the copy and the bricks it came from stay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HideCopy {
    pub player: u64,
}
impl ScriptOp for HideCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "hide_copy";
    fn bounded(&self) -> bool {
        true
    }
}

/// Move the copy `player` places as their brick shift keys would:
/// `offset` is studs away from and to the left of their facing and
/// plates up, `super_shift` moves by the copy's own size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShiftCopy {
    pub player: u64,
    pub offset: [i32; 3],
    pub super_shift: bool,
}
impl ScriptOp for ShiftCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "shift_copy";
    fn bounded(&self) -> bool {
        let ShiftCopy { offset, .. } = self;
        (-1..=1).contains(&offset[0])
            && (-1..=1).contains(&offset[1])
            && (-3..=3).contains(&offset[2])
    }
}

/// Turn the copy `player` places a quarter turn as their rotate keys
/// would: 1 clockwise seen from above, -1 the other way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotateCopy {
    pub player: u64,
    pub direction: i8,
}
impl ScriptOp for RotateCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "rotate_copy";
    fn bounded(&self) -> bool {
        let RotateCopy { direction, .. } = self;
        matches!(direction, -1 | 1)
    }
}

/// Plant the copy `player` places where it stands, as their plant key
/// would; with `float`, bricks with nothing under them plant this once
/// as if they stood on the ground (v20's force plant).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlantCopy {
    pub player: u64,
    pub float: bool,
}
impl ScriptOp for PlantCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "plant_copy";
    fn bounded(&self) -> bool {
        true
    }
}

/// Let every plant of the copy `player` holds float, or not; with
/// `admin_only`, only while they are an administrator (a plant that
/// finds them not one does not float, and `on_place` says so with
/// `float_refused`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloatCopy {
    pub player: u64,
    pub float: bool,
    #[serde(default)]
    pub admin_only: bool,
}
impl ScriptOp for FloatCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "float_copy";
    fn bounded(&self) -> bool {
        true
    }
}

/// After each plant of a copy, `player`'s next copy plant waits this
/// long; one sooner is refused and `on_place` hears `error` `wait`,
/// with the seconds left in `wait`. 0 lets them plant at once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlantWait {
    pub player: u64,
    pub seconds: f32,
}
impl ScriptOp for PlantWait {
    const CAPABILITY: &str = "build";
    const NAME: &str = "plant_wait";
    fn bounded(&self) -> bool {
        let PlantWait { seconds, .. } = self;
        (0.0..=60.0).contains(seconds)
    }
}

/// Stop `player`'s copy work that is going on over several ticks (a big
/// selection, plant, cut, paint, wrench, undo or load). What it did so
/// far stays done, as one step of their undo; the Add-On's `on_copy`
/// (or `on_place`) hears it with `error` `canceled` (`canceled` true).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelCopy {
    pub player: u64,
}
impl ScriptOp for CancelCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "cancel_copy";
    fn bounded(&self) -> bool {
        true
    }
}

/// What the copy `player` places turns about and is put against a
/// clicked surface by: the whole copy (`whole`), else the brick it was
/// taken from first (the clicked brick of a stack).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PivotCopy {
    pub player: u64,
    pub whole: bool,
}
impl ScriptOp for PivotCopy {
    const CAPABILITY: &str = "build";
    const NAME: &str = "pivot_copy";
    fn bounded(&self) -> bool {
        true
    }
}

/// Plant `player`'s copies into another player's brick group: `target`
/// names them (a player's name or part of it, or a BL_ID); empty plants
/// into their own again. Each plant needs build trust with that group,
/// or `admin` and an administrator. The package's `on_copy` hears the
/// group chosen (`action` `"plant_as"`, `name`, or `error` `missing`
/// or `trust`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlantAs {
    pub player: u64,
    pub target: String,
    pub admin: bool,
}
impl ScriptOp for PlantAs {
    const CAPABILITY: &str = "build";
    const NAME: &str = "plant_as";
    fn bounded(&self) -> bool {
        let PlantAs { target, .. } = self;
        target.len() <= 64 && !target.chars().any(char::is_control)
    }
}

/// Let the held image take `player`'s paint and FX cans (its
/// `commands.paint`) instead of the can coming out, or stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakePaint {
    pub player: u64,
    pub take: bool,
}
impl ScriptOp for TakePaint {
    const CAPABILITY: &str = "build";
    const NAME: &str = "take_paint";
    fn bounded(&self) -> bool {
        true
    }
}
