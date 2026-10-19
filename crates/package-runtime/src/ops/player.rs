//! Operations behind the `player` capability.
use super::*;

/// Colour a player's avatar over their own colours, per avatar slot
/// (`torso`, `larm`, `rleg`, ...): a team's uniform. An empty map
/// gives them their own colours back. Kept across respawns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetAvatarColors {
    pub player: u64,
    pub colors: BTreeMap<String, [f32; 4]>,
}
impl ScriptOp for SetAvatarColors {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_avatar_colors";
    fn bounded(&self) -> bool {
        let SetAvatarColors { colors, .. } = self;
        colors.len() <= AVATAR_SLOTS.len()
            && colors.iter().all(|(slot, c)| {
                AVATAR_SLOTS.contains(&slot.as_str())
                    && c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            })
    }
}

/// Dress a player's avatar in parts over their own choices, per part
/// slot (`hat: "copHat"`, `pack: "none"`), and a face and decal: a
/// team's full uniform (Slayer's `hideAllNodes` and `unHideNode`). A
/// part, face or decal the server's avatar pack lacks is left as theirs.
/// No parts, face or decal gives them their own back. Kept across
/// respawns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetAvatarParts {
    pub player: u64,
    pub parts: BTreeMap<String, String>,
    pub face: Option<String>,
    pub decal: Option<String>,
}
impl ScriptOp for SetAvatarParts {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_avatar_parts";
    fn bounded(&self) -> bool {
        let SetAvatarParts {
            parts, face, decal, ..
        } = self;
        let name = |n: &str| !n.is_empty() && n.len() <= 64 && n.is_ascii();
        parts.len() <= AVATAR_PARTS.len()
            && parts
                .iter()
                .all(|(slot, part)| AVATAR_PARTS.contains(&slot.as_str()) && name(part))
            && face
                .iter()
                .chain(decal)
                .all(|n| n.len() <= 256 && n.is_ascii())
    }
}

/// Move a living player, keeping their facing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Teleport {
    pub player: u64,
    pub position: [f32; 3],
}
impl ScriptOp for Teleport {
    const CAPABILITY: &str = "player";
    const NAME: &str = "teleport";
    fn bounded(&self) -> bool {
        let Teleport { position, .. } = self;
        finite(position)
    }
}

/// Give a player a new life at a spawn point, alive or dead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Respawn {
    pub player: u64,
}
impl ScriptOp for Respawn {
    const CAPABILITY: &str = "player";
    const NAME: &str = "respawn";
    fn bounded(&self) -> bool {
        true
    }
}

/// Take a living player's body away without a death (`player.delete()`):
/// nobody scores, no death line, no `on_death`. They wait to respawn as
/// the dead do (held, if their respawn is held).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoveBody {
    pub player: u64,
}
impl ScriptOp for RemoveBody {
    const CAPABILITY: &str = "player";
    const NAME: &str = "remove_body";
    fn bounded(&self) -> bool {
        true
    }
}

/// Make a player this archetype (a package's `archetype` id or v20's
/// `v20.player.<datablock>`), now and at every respawn. An empty id
/// hands the choice back to the mini-game's player type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetArchetype {
    pub player: u64,
    pub archetype: String,
}
impl ScriptOp for SetArchetype {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_archetype";
    fn bounded(&self) -> bool {
        let SetArchetype { archetype, .. } = self;
        archetype.len() <= 160 && !archetype.chars().any(char::is_control)
    }
}

/// Lay an archetype over a living player's own for a while (Kai's
/// `pushDatablock`: a machine gunner walking slowly as they fire). The
/// player moves as the newest one laid on, keeping the damage they
/// have taken; `set_archetype` meanwhile changes the one underneath.
/// Refused quietly for a body of another model, or one already laid
/// on. All are lifted when the player dies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushArchetype {
    pub player: u64,
    pub archetype: String,
}
impl ScriptOp for PushArchetype {
    const CAPABILITY: &str = "player";
    const NAME: &str = "push_archetype";
    fn bounded(&self) -> bool {
        let PushArchetype { archetype, .. } = self;
        !archetype.is_empty() && archetype.len() <= 160 && !archetype.chars().any(char::is_control)
    }
}

/// Lift an archetype [`Op::PushArchetype`] laid on (`popDatablock`);
/// nothing when it is not on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopArchetype {
    pub player: u64,
    pub archetype: String,
}
impl ScriptOp for PopArchetype {
    const CAPABILITY: &str = "player";
    const NAME: &str = "pop_archetype";
    fn bounded(&self) -> bool {
        let PopArchetype { archetype, .. } = self;
        !archetype.is_empty() && archetype.len() <= 160 && !archetype.chars().any(char::is_control)
    }
}

/// Hand a player's movement input to one of the package's entities
/// (`entity`), or back to the player's own body (`None`). The avatar
/// stands where it was while the entity is driven.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Control {
    pub player: u64,
    pub entity: Option<u64>,
}
impl ScriptOp for Control {
    const CAPABILITY: &str = "player";
    const NAME: &str = "control";
    fn bounded(&self) -> bool {
        true
    }
}

/// Switch what `player`'s mouse wheel and number keys pick
/// (`clientCmdSetScrollMode`), without changing what is in hand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetScrollMode {
    pub player: u64,
    pub mode: ScrollMode,
}
impl ScriptOp for SetScrollMode {
    const CAPABILITY: &str = "player";
    const NAME: &str = "scroll_mode";
    fn bounded(&self) -> bool {
        true
    }
}

/// For `seconds`, a player looks different (`SetTempColor`,
/// `setFaceName`, `setNodeColor`), then as they were.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetTempLook {
    pub player: u64,
    pub look: TempLook,
    pub seconds: f32,
}
impl ScriptOp for SetTempLook {
    const CAPABILITY: &str = "player";
    const NAME: &str = "temp_look";
    fn bounded(&self) -> bool {
        let SetTempLook { look, seconds, .. } = self;
        (0.0..=MAX_TEMP_LOOK_SECONDS).contains(seconds)
            && look
                .color
                .is_none_or(|c| c.iter().all(|v| (0.0..=1.0).contains(v)))
            && look.face.as_ref().is_none_or(|f| {
                !f.is_empty() && f.len() <= 64 && f.chars().all(|c| c.is_ascii_graphic())
            })
            && look.alpha.len() <= AVATAR_SLOTS.len()
            && look
                .alpha
                .iter()
                .all(|(slot, a)| AVATAR_SLOTS.contains(&slot.as_str()) && (0.0..=1.0).contains(a))
    }
}

/// Put an item in a player's tool list (unless they carry it) and,
/// with `equip`, in their hand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GiveItem {
    pub player: u64,
    pub item: String,
    pub equip: bool,
}
impl ScriptOp for GiveItem {
    const CAPABILITY: &str = "player";
    const NAME: &str = "give_item";
    fn bounded(&self) -> bool {
        let GiveItem { item: id, .. } = self;
        item(id)
    }
}

/// Put a whole tool list in a living player's hands, slot by slot
/// (`forceEquip`, a team's start tools): `None` empties a slot, slots
/// past the list are emptied, and items the server lacks leave theirs
/// empty. What they held is put away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetTools {
    pub player: u64,
    pub tools: Vec<Option<String>>,
}
impl ScriptOp for SetTools {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_tools";
    fn bounded(&self) -> bool {
        let SetTools { tools, .. } = self;
        tools.len() <= MAX_TOOL_SLOTS
            && tools
                .iter()
                .flatten()
                .all(|id| !id.is_empty() && id.len() <= 160 && id.is_ascii())
    }
}

/// Take one `item` out of a player's tool list (`%obj.tool[%slot] =
/// 0`): the held slot if it holds one, else the first that does. A held
/// item is put away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakeItem {
    pub player: u64,
    pub item: String,
}
impl ScriptOp for TakeItem {
    const CAPABILITY: &str = "player";
    const NAME: &str = "take_item";
    fn bounded(&self) -> bool {
        let TakeItem { item: id, .. } = self;
        item(id)
    }
}

/// Put an item of this package (or one it depends on) in the world as
/// a pickup at `position`, moving at `velocity`, that pops after ten
/// seconds like a dropped tool. `data` travels with it to `on_pickup`
/// as `info.data`, as what `on_drop` keeps does (a dead player's
/// ammo in the bag they leave).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropItem {
    pub item: String,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    /// A palette colour tinting it (a team's flag).
    #[serde(default)]
    pub paint: Option<u8>,
    /// Kept with it: `on_pickup` sees it as `info.data`, `drops()` too.
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    /// Seconds until it pops (1 to [`MAX_DROP_SECONDS`]); `None` is
    /// v20's ten.
    #[serde(default)]
    pub seconds: Option<u32>,
}
impl ScriptOp for DropItem {
    const CAPABILITY: &str = "player";
    const NAME: &str = "drop_item";
    fn bounded(&self) -> bool {
        let DropItem {
            item: id,
            position,
            velocity,
            data,
            seconds,
            ..
        } = self;
        seconds.is_none_or(|s| (1..=MAX_DROP_SECONDS).contains(&s))
            && data
                .as_ref()
                .is_none_or(|d| serde_json::to_vec(d).is_ok_and(|b| b.len() <= MAX_DROP_DATA_BYTES))
            && item(id)
            && finite(position)
            && finite(velocity)
            && glam_length(velocity) <= MAX_PUSH_SPEED
            && data
                .as_ref()
                .is_none_or(|d| crate::state::check_value(d).is_ok())
    }
}

/// Take back an item this package put in the world with `drop_item`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoveDrop {
    pub drop: u64,
}
impl ScriptOp for RemoveDrop {
    const CAPABILITY: &str = "player";
    const NAME: &str = "remove_drop";
    fn bounded(&self) -> bool {
        true
    }
}

/// Float `text` over an item this package put in the world, in palette
/// colour `color` (`setShapeName` with `setShapeNameColor`: a dropped
/// flag's countdown), or take it away with `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NameDrop {
    pub drop: u64,
    pub text: Option<String>,
    pub color: u8,
}
impl ScriptOp for NameDrop {
    const CAPABILITY: &str = "player";
    const NAME: &str = "name_drop";
    fn bounded(&self) -> bool {
        let NameDrop { text, .. } = self;
        text.as_deref().is_none_or(|t| {
            t.chars().count() <= bri_weapons::MAX_DROP_NAME && !t.chars().any(char::is_control)
        })
    }
}

/// Set a player's field of view (`setControlCameraFov`), or hand it back
/// to their own setting with `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetFov {
    pub player: u64,
    pub fov: Option<f32>,
}
impl ScriptOp for SetFov {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_fov";
    fn bounded(&self) -> bool {
        let SetFov { fov, .. } = self;
        fov.is_none_or(|f| FOV_RANGE.contains(&f))
    }
}

/// Move a player's body at this share of its running, crouching and
/// swimming speeds (0 to 4; 1 is its archetype's own) until changed or
/// they respawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetSpeedScale {
    pub player: u64,
    pub scale: f32,
}
impl ScriptOp for SetSpeedScale {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_speed_scale";
    fn bounded(&self) -> bool {
        let SetSpeedScale { scale, .. } = self;
        scale.is_finite() && (0.0..=MAX_SPEED_SCALE).contains(scale)
    }
}

/// A named share of a player's speeds, for one Add-On system to own: the
/// shares of every key (and of `set_speed_scale`, which is the key "") are
/// multiplied, so a lock another system writes (0, as a posture change
/// does) cannot be undone by a system that writes its own key every tick.
/// A key set to 1 is dropped. 0 to 4 each, at most [`MAX_SPEED_KEYS`] keys
/// a player; they clear when the player respawns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetSpeedModifier {
    pub player: u64,
    pub key: String,
    pub scale: f32,
}
/// Most [`SetSpeedModifier`] keys one player may hold at once.
pub const MAX_SPEED_KEYS: usize = 16;
/// Whether `key` is a fine modifier key: 1 to 32 of a to z, 0 to 9 and `_`.
pub fn speed_key_ok(key: &str) -> bool {
    (1..=32).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
impl ScriptOp for SetSpeedModifier {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_speed_modifier";
    fn bounded(&self) -> bool {
        speed_key_ok(&self.key) && self.scale.is_finite() && (0.0..=MAX_SPEED_SCALE).contains(&self.scale)
    }
}

/// Lay a player down or get them up, for a body whose archetype can lie
/// prone (`movement.prone`). Getting up needs room to crouch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetProne {
    pub player: u64,
    pub prone: bool,
}
impl ScriptOp for SetProne {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_prone";
    fn bounded(&self) -> bool {
        true
    }
}

/// Lean a player, -1 (left) to 1 (right), 0 upright, for a body whose
/// archetype can lean (`movement.lean`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetLean {
    pub player: u64,
    pub lean: f32,
}
impl ScriptOp for SetLean {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_lean";
    fn bounded(&self) -> bool {
        self.lean.is_finite() && (-1.0..=1.0).contains(&self.lean)
    }
}

/// Add rounds of `ammo` to a player's reserve (an ammo box), up to the
/// most its magazines carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GiveAmmo {
    pub player: u64,
    pub ammo: String,
    pub rounds: u64,
}
impl ScriptOp for GiveAmmo {
    const CAPABILITY: &str = "player";
    const NAME: &str = "give_ammo";
    fn bounded(&self) -> bool {
        let GiveAmmo { ammo, rounds, .. } = self;
        ammo_name(ammo) && (1..=MAX_AMMO_ROUNDS).contains(rounds)
    }
}

/// Set a player's reserve of `ammo`; `None` never runs out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetReserve {
    pub player: u64,
    pub ammo: String,
    pub rounds: Option<u64>,
}
impl ScriptOp for SetReserve {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_reserve";
    fn bounded(&self) -> bool {
        let SetReserve { ammo, rounds, .. } = self;
        ammo_name(ammo) && rounds.is_none_or(|r| r <= MAX_AMMO_ROUNDS)
    }
}

/// Set the rounds in a player's magazine of `item`, up to its size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetRounds {
    pub player: u64,
    pub item: String,
    pub rounds: u64,
}
impl ScriptOp for SetRounds {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_rounds";
    fn bounded(&self) -> bool {
        let SetRounds {
            item: id, rounds, ..
        } = self;
        item(id) && *rounds <= MAX_AMMO_ROUNDS
    }
}

/// Start reloading the gun in a player's hand, as the light key does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reload {
    pub player: u64,
}
impl ScriptOp for Reload {
    const CAPABILITY: &str = "player";
    const NAME: &str = "reload";
    fn bounded(&self) -> bool {
        true
    }
}

/// Send the image in a player's hand to its state named `state`, as a
/// trigger would: a melee swing with whatever is held, an inspect, a
/// special move a key of the Add-On's starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetImageState {
    pub player: u64,
    pub state: String,
}
impl ScriptOp for SetImageState {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_image_state";
    fn bounded(&self) -> bool {
        !self.state.trim().is_empty()
            && self.state.len() <= 64
            && !self.state.chars().any(char::is_control)
    }
}

/// Use a carried item once without switching to it for good: take it out,
/// pull its trigger until it fires (a thrown grenade, a quick heal), then
/// put the item held before back in hand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickUse {
    pub player: u64,
    pub item: String,
}
impl ScriptOp for QuickUse {
    const CAPABILITY: &str = "player";
    const NAME: &str = "quick_use";
    fn bounded(&self) -> bool {
        item(&self.item)
    }
}

/// Whether the image in a player's hand has ammo (`setImageAmmo`), which
/// its states' `ammo` transitions read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetImageAmmo {
    pub player: u64,
    pub ammo: bool,
}
impl ScriptOp for SetImageAmmo {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_image_ammo";
    fn bounded(&self) -> bool {
        true
    }
}

/// Whether the image in a player's hand is loaded (`setImageLoaded`),
/// which its states' `loaded` and `not_loaded` transitions read: a tool
/// that spins while it works.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetImageLoaded {
    pub player: u64,
    pub loaded: bool,
}
impl ScriptOp for SetImageLoaded {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_image_loaded";
    fn bounded(&self) -> bool {
        true
    }
}

/// Scale the random spread of a player's next shots (`shot.spread` and its
/// stance variants): 1 leaves it, 0 makes shots true, 2 doubles the cone. A
/// rule that models how steady a shooter is sets it as that changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetAimScale {
    pub player: u64,
    pub scale: f32,
}
impl ScriptOp for SetAimScale {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_aim_scale";
    fn bounded(&self) -> bool {
        self.scale.is_finite() && (0.0..=8.0).contains(&self.scale)
    }
}

/// Turn a player's own view by `pitch` degrees up and `yaw` degrees to the
/// right and leave it there: a rifle's recoil climb. The player's game adds
/// it to where they look; nothing pulls it back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KickView {
    pub player: u64,
    pub pitch: f32,
    pub yaw: f32,
}
impl ScriptOp for KickView {
    const CAPABILITY: &str = "player";
    const NAME: &str = "kick_view";
    fn bounded(&self) -> bool {
        [self.pitch, self.yaw].iter().all(|a| a.is_finite() && a.abs() <= 30.0)
    }
}

/// Put another image in a player's hand, keeping their tool slot
/// (`mountImage`): a scope, a second fire mode. `None` puts back the
/// selected tool's own image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MountImage {
    pub player: u64,
    pub image: Option<String>,
}
impl ScriptOp for MountImage {
    const CAPABILITY: &str = "player";
    const NAME: &str = "mount_image";
    fn bounded(&self) -> bool {
        let MountImage { image, .. } = self;
        image
            .as_deref()
            .is_none_or(|i| bri_package::id::is_content_ref(i, Some("image")))
    }
}

/// Mount an image on a player's body in the emote slot
/// (`%player.emote(%image)`), replacing the emote, pain or flames there:
/// every client plays it, and its states run their commands for the
/// wearer (a heal over time). `None` empties the slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Emote {
    pub player: u64,
    pub image: Option<String>,
    /// `%skipSpam`: without it an image counts toward the player's
    /// emote spam check (more than five quick emotes are dropped), as
    /// the stock emotes do.
    #[serde(default)]
    pub skip_spam: bool,
}
impl ScriptOp for Emote {
    const CAPABILITY: &str = "player";
    const NAME: &str = "emote";
    fn bounded(&self) -> bool {
        let Emote { image, .. } = self;
        image
            .as_deref()
            .is_none_or(|i| bri_package::id::is_content_ref(i, Some("image")))
    }
}

/// Put an image in a worn slot (2 or 3) of a player, tinted with a
/// palette colour (`mountImage(%image, 3)`: a flag on the back), or
/// take it off with `None`. With `keep`, no other package replaces or
/// takes off that image while it is worn (Slayer CTF's
/// `Player::mountImage` and `unMountImage` overrides, which guard the
/// flag).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WearImage {
    pub player: u64,
    pub slot: u8,
    pub image: Option<String>,
    pub paint: Option<u8>,
    #[serde(default)]
    pub keep: bool,
}
impl ScriptOp for WearImage {
    const CAPABILITY: &str = "player";
    const NAME: &str = "mount_image";
    fn bounded(&self) -> bool {
        let WearImage { slot, image, .. } = self;
        (2..=3).contains(slot)
            && image
                .as_deref()
                .is_none_or(|i| bri_package::id::is_content_ref(i, Some("image")))
    }
}

/// Empty a player's hand (`unMountImage(0)`): the tool they held is put
/// away, still in its slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnmountImage {
    pub player: u64,
}
impl ScriptOp for UnmountImage {
    const CAPABILITY: &str = "player";
    const NAME: &str = "unmount_image";
    fn bounded(&self) -> bool {
        true
    }
}

/// A player's body scale (`setScale`, `setPlayerScale`); a new body
/// is full size again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetScale {
    pub player: u64,
    pub scale: f32,
}
impl ScriptOp for SetScale {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_scale";
    fn bounded(&self) -> bool {
        let SetScale { scale, .. } = self;
        scale.is_finite() && SCALE_RANGE.contains(scale)
    }
}

/// Bound how far a player's arms and head follow their look
/// (`setLookLimits(%up, %down)`), as `[down, up]` positions from 0
/// (all the way up) to 1, or `None` for the whole range. A new body
/// looks freely again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetLookLimits {
    pub player: u64,
    pub limits: Option<[f32; 2]>,
}
impl ScriptOp for SetLookLimits {
    const CAPABILITY: &str = "player";
    const NAME: &str = "set_look_limits";
    fn bounded(&self) -> bool {
        let SetLookLimits { limits, .. } = self;
        limits.is_none_or(|[down, up]| (0.0..=1.0).contains(&down) && (0.0..=1.0).contains(&up))
    }
}

/// Fly a player's camera along knots (`setControlObject(pathCamera)`),
/// their body standing still, or with `None` hand control back. The
/// package's `on_path_node` hears each knot reached.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FollowPath {
    pub player: u64,
    pub knots: Option<Vec<PathKnot>>,
}
impl ScriptOp for FollowPath {
    const CAPABILITY: &str = "player";
    const NAME: &str = "follow_path";
    fn bounded(&self) -> bool {
        let FollowPath { knots, .. } = self;
        knots.as_ref().is_none_or(|k| {
            (1..=MAX_PATH_KNOTS).contains(&k.len())
                && k.iter().all(|k| {
                    finite(&k.at)
                        && k.yaw.is_finite()
                        && k.pitch.is_finite()
                        && k.pitch.abs() <= std::f32::consts::FRAC_PI_2
                        && (0.1..=1000.0).contains(&k.speed)
                })
        })
    }
}

/// Give a player a free camera from where their camera is
/// (`Camera::setMode("Observer")`), or an orbit around a point
/// (`setOrbitPointMode`); a frozen [`Op::OrbitCamera`]'s `None` hands
/// control back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub player: u64,
    pub camera: CameraOp,
}
impl ScriptOp for Camera {
    const CAPABILITY: &str = "player";
    const NAME: &str = "orbit_point";
    fn bounded(&self) -> bool {
        let Camera { camera, .. } = self;
        match camera {
            CameraOp::Free => true,
            CameraOp::Point { at, distance } => finite(at) && ORBIT_DISTANCES.contains(distance),
            CameraOp::Object { distance, .. } => ORBIT_DISTANCES.contains(distance),
        }
    }
}

/// Give a player an orbit camera around a player's body (v20's
/// `setOrbitMode` and `setControlObject(camera)`), or (`None`) their
/// body back from the kind of camera `body` names. One seam for both
/// of v20's uses: Throwing's held player, whose click still acts, and
/// a rule's `watch`, whose body freezes and whose keys go to the rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbitCamera {
    pub player: u64,
    pub body: OrbitBody,
    pub orbit: Option<Orbit>,
}
impl ScriptOp for OrbitCamera {
    const CAPABILITY: &str = "player";
    const NAME: &str = "orbit_camera";
    fn bounded(&self) -> bool {
        let OrbitCamera {
            player,
            body,
            orbit,
        } = self;
        orbit.is_none_or(|o| o.valid() && (o.target != *player || *body == OrbitBody::Frozen))
    }
}

/// Kick a player's real view (`HumanController.ClientStartViewShake`): the
/// look turns out along `direction` (radians in the view plane, 0 up and
/// pi/2 across) up to `max`, bounces back at half the speed and settles
/// where it has come back to `min`, so a hit leaves the aim a little off.
/// `rate` is radians a second. Radians throughout; the client runs it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewShake {
    pub player: u64,
    pub direction: f32,
    pub min: f32,
    pub max: f32,
    pub rate: f32,
}
impl ScriptOp for ViewShake {
    const CAPABILITY: &str = "player";
    const NAME: &str = "view_shake";
    fn bounded(&self) -> bool {
        let ViewShake { direction, min, max, rate, .. } = self;
        [direction, min, max, rate].iter().all(|v| v.is_finite())
            && direction.abs() <= 100.0
            && (-1.0..=1.0).contains(min)
            && (0.0..=1.0).contains(max)
            && (0.01..=100.0).contains(&rate.abs())
    }
}

/// Shake a player's camera and nothing else (`camera_shake`): each frame the
/// view is moved a random way up to `translation` units in its own plane and
/// turned a random way up to `rotation` radians, the share of both fading over
/// `seconds` as `1 - (1 - left)^hold` of the time left (1 a straight fade, more
/// holds the shake before it drops). The aim, the body and everyone else's view
/// are untouched, unlike `view_shake`. Presentation only; the client runs it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraShake {
    pub player: u64,
    pub translation: f32,
    pub rotation: f32,
    pub seconds: f32,
    pub hold: f32,
}
impl ScriptOp for CameraShake {
    const CAPABILITY: &str = "player";
    const NAME: &str = "camera_shake";
    fn bounded(&self) -> bool {
        (0.0..=4.0).contains(&self.translation)
            && (0.0..=0.5).contains(&self.rotation)
            && (0.05..=10.0).contains(&self.seconds)
            && (1.0..=8.0).contains(&self.hold)
    }
}

/// Scale a player's velocity: the horizontal part by `horizontal`, the
/// vertical part by `vertical` (both 0 to 2). A jump that takes some of its
/// run with it, as AA's `DoJump` slows a hopper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScaleVelocity {
    pub player: u64,
    pub horizontal: f32,
    pub vertical: f32,
}
impl ScriptOp for ScaleVelocity {
    const CAPABILITY: &str = "player";
    const NAME: &str = "scale_velocity";
    fn bounded(&self) -> bool {
        let ScaleVelocity { horizontal, vertical, .. } = self;
        [horizontal, vertical].iter().all(|v| v.is_finite() && (0.0..=2.0).contains(*v))
    }
}

#[cfg(test)]
mod speed_modifier_tests {
    use super::*;
    #[test]
    fn keys_are_short_lowercase_names_and_shares_are_bounded() {
        assert!(speed_key_ok("prone_lock") && speed_key_ok("movement2"));
        for bad in ["", "Prone", "a b", "a-b", &"k".repeat(33)] {
            assert!(!speed_key_ok(bad), "{bad:?}");
        }
        let op = |key: &str, scale: f32| SetSpeedModifier { player: 1, key: key.into(), scale }.bounded();
        assert!(op("lock", 0.0) && op("lock", 4.0));
        assert!(!op("lock", 4.5) && !op("lock", -0.1) && !op("lock", f32::NAN) && !op("Lock", 1.0));
    }
}
