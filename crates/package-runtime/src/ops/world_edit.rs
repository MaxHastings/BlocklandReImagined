//! Operations behind the `world.edit` capability.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoveBrick {
    pub brick: u64,
}
impl ScriptOp for RemoveBrick {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "remove_brick";
    fn bounded(&self) -> bool {
        true
    }
}

/// Add a world-owned brick of a known shape: an arena, a gate, a board.
/// The colour is matched to the nearest colour of the world's palette.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceBrick {
    pub shape: String,
    pub position: [f32; 3],
    pub color: [f32; 4],
}
impl ScriptOp for PlaceBrick {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "place_brick";
    fn bounded(&self) -> bool {
        let PlaceBrick {
            shape,
            position,
            color,
        } = self;
        !shape.is_empty()
            && shape.len() <= 128
            && finite(position)
            && color.iter().all(|c| (0.0..=1.0).contains(c))
    }
}

/// Plant a brick of kind `kind` (a brick catalog id) into build
/// `owner` (a brick's `owner`, 0 for the world's own), palette colour `color`, `turns`
/// clockwise quarter turns, centred at `position` (snapped to the stud
/// and plate grid). Where it does not fit (another brick, a player,
/// the map) nothing is planted and nothing is reported, as v20's
/// `fxDTSBrick::plant` returned an error the script checked. A brick
/// split, merged or piled by a rule (Trench Digging's dirt) stays its
/// builder's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlantBrick {
    pub kind: String,
    pub position: [f32; 3],
    pub turns: u8,
    pub color: u8,
    pub owner: u64,
}
impl ScriptOp for PlantBrick {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "plant_brick";
    fn bounded(&self) -> bool {
        let PlantBrick {
            kind,
            position,
            turns,
            ..
        } = self;
        (bri_package::id::is_content_ref(kind, Some("brick"))
            || kind
                .strip_prefix("v20/brick/")
                .is_some_and(|n| !n.is_empty() && n.len() <= 128))
            && finite(position)
            && *turns < 4
    }
}

/// Put a voxel of the generated world's `material` (its id) at voxel
/// coordinates `position`: dirt thrown back into a trench. It becomes
/// part of the world, saved with its edits, and is refused where
/// something is in the way (a brick, a player, a vehicle).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceVoxel {
    pub position: [i64; 3],
    pub material: String,
}
impl ScriptOp for PlaceVoxel {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "place_voxel";
    fn bounded(&self) -> bool {
        let PlaceVoxel { position, material } = self;
        position.iter().all(|c| c.abs() <= 1_000_000)
            && bri_package::id::ContentId::parse(material).is_ok()
    }
}

/// Show a block brick in one of its block's named states (`""` for the
/// block's own faces): a dig tool cracks it, a switch lights it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetBlockState {
    pub brick: u64,
    pub state: String,
}
impl ScriptOp for SetBlockState {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "set_block_state";
    fn bounded(&self) -> bool {
        let SetBlockState { state, .. } = self;
        state.len() <= 64 && !state.chars().any(char::is_control)
    }
}

/// Remove the bricks `player`'s copy was taken from, as their hammer
/// would (their full trust), as one step Ctrl+Z puts back as it was:
/// all or none, or with `each` every brick they may cut, the rest
/// counted (`on_copy`, `action` `"cut"`, `refused`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutCopy {
    pub player: u64,
    #[serde(default)]
    pub each: bool,
}
impl ScriptOp for CutCopy {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "cut_copy";
    fn bounded(&self) -> bool {
        true
    }
}

/// Paint the bricks `player`'s copy was taken from with `paint`, as
/// their spray or FX can would, as one step Ctrl+Z takes back. With
/// `each`, every brick they may paint is painted and the rest are
/// counted (`on_copy`, `action` `"paint"`); else all or none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintCopy {
    pub player: u64,
    pub paint: FillPaint,
    pub each: bool,
}
impl ScriptOp for PaintCopy {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "paint_copy";
    fn bounded(&self) -> bool {
        let PaintCopy { paint, .. } = self;
        paint.valid()
    }
}

/// Open `player`'s wrench on every brick their copy was taken from: the
/// settings they tick apply to each brick they may change, as one step
/// Ctrl+Z takes back (`on_copy`, `action` `"wrench"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WrenchCopy {
    pub player: u64,
}
impl ScriptOp for WrenchCopy {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "wrench_copy";
    fn bounded(&self) -> bool {
        true
    }
}

/// Remove every brick reaching into the box from `min` to `max` that
/// `player` may hammer, and put plain bricks back over the parts that
/// stuck out of it (v20's New Duplicator's supercut), as one step
/// Ctrl+Z takes back (`on_copy`, `action` `"supercut"`). A copy job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuperCut {
    pub player: u64,
    pub min: [f32; 3],
    pub max: [f32; 3],
}
impl ScriptOp for SuperCut {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "super_cut";
    fn bounded(&self) -> bool {
        let SuperCut { min, max, .. } = self;
        span(min, max)
    }
}

/// Fill the empty room in the box from `min` to `max` with the fewest
/// plain bricks of palette colour `color`, as `player`'s own, as one
/// step Ctrl+Z takes back (`on_copy`, `action` `"fill"`). A copy job,
/// stopping at the server's brick limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillBox {
    pub player: u64,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub color: u8,
}
impl ScriptOp for FillBox {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "fill_box";
    fn bounded(&self) -> bool {
        let FillBox { min, max, .. } = self;
        span(min, max)
    }
}

/// Paint `brick` and every brick of its colour joined to it as
/// `player`'s spray cans would paint each one (their full trust; a fill
/// flows around bricks it may not paint), as one step Ctrl+Z takes
/// back. Bricks join through shared faces, or with `reach` through any
/// overlap of a brick's box grown by `reach` (sideways, up and down),
/// as v20's `containerBoxSearch` fills found them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintFill {
    pub player: u64,
    pub brick: u64,
    pub paint: FillPaint,
    pub limit: u32,
    pub reach: Option<[f32; 2]>,
    /// More than `limit` bricks: paint the first `limit` and stop, as
    /// v20 did, instead of refusing the fill.
    pub stop_at_limit: bool,
    /// Centre-printed, for these seconds, when the limit stops a fill.
    pub limit_message: Option<(String, f32)>,
    /// How long a refusal ("does not trust you enough") shows, seconds.
    pub refusal_seconds: Option<f32>,
    /// When the limit stops a fill, the player's plant-limit error
    /// (`MsgPlantError_Limit`) shows too.
    pub limit_error: bool,
}
impl ScriptOp for PaintFill {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "paint_fill";
    fn bounded(&self) -> bool {
        let PaintFill {
            paint,
            limit,
            reach,
            limit_message,
            refusal_seconds,
            ..
        } = self;
        (1..=MAX_FILL_BRICKS as u32).contains(limit)
            && refusal_seconds.is_none_or(|s| (0.0..=30.0).contains(&s))
            && paint.valid()
            && reach.is_none_or(|r| r.iter().all(|v| (0.0..=MAX_FILL_REACH).contains(v)))
            && limit_message.as_ref().is_none_or(|(text, seconds)| {
                text.chars().count() <= MAX_PRINT_CHARS && (0.0..=30.0).contains(seconds)
            })
    }
}

/// Paint a vehicle as `player` (their full trust from its spawn
/// brick's build, the minigame's paint rule), as one step Ctrl+Z takes
/// back. Its riders take the colour for `riders_seconds`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintVehicle {
    pub player: u64,
    pub vehicle: u64,
    pub paint: VehiclePaint,
    pub riders_seconds: Option<f32>,
    /// How long a refusal ("does not trust you enough") shows, seconds.
    pub refusal_seconds: Option<f32>,
}
impl ScriptOp for PaintVehicle {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "paint_vehicle";
    fn bounded(&self) -> bool {
        let PaintVehicle {
            paint,
            riders_seconds,
            refusal_seconds,
            ..
        } = self;
        refusal_seconds.is_none_or(|s| (0.0..=30.0).contains(&s))
            && riders_seconds.is_none_or(|s| (0.0..=MAX_TEMP_LOOK_SECONDS).contains(&s))
            && match paint {
                VehiclePaint::Color(_) => true,
                VehiclePaint::Rgb(c) => c.iter().all(|v| (0.0..=1.0).contains(v)),
            }
    }
}

/// The item a brick holds out to be picked up (`setItem`): an item of
/// this package, a dependency's or v20's, or `None` for none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetBrickItem {
    pub brick: u64,
    pub item: Option<String>,
}
impl ScriptOp for SetBrickItem {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "set_brick_item";
    fn bounded(&self) -> bool {
        let SetBrickItem { item: id, .. } = self;
        id.as_deref().is_none_or(item)
    }
}

/// Repaint a brick in palette colour `color` (`fxDTSBrick::setColor`
/// from a game's script: a capture point taking its holder's colour).
/// Nothing to undo; the brick must be the world's, a mini-game's or one
/// the calling player has full trust on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetBrickColor {
    pub brick: u64,
    pub color: u8,
}
impl ScriptOp for SetBrickColor {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "set_brick_color";
    fn bounded(&self) -> bool {
        true
    }
}

/// Whether a brick draws, collides and stops rays (`setRendering`,
/// `setColliding`, `setRayCasting` from a game's script: Slayer's path
/// nodes hiding as they are planted). The same bricks as
/// [`SetBrickColor`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetBrickShown {
    pub brick: u64,
    pub rendering: bool,
    pub colliding: bool,
    pub raycasting: bool,
}
impl ScriptOp for SetBrickShown {
    const CAPABILITY: &str = "world.edit";
    const NAME: &str = "set_brick_shown";
    fn bounded(&self) -> bool {
        true
    }
}
