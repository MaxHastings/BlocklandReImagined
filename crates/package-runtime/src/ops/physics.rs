//! Operations behind the `physics` capability.
use super::*;

/// Change an object's velocity by `velocity` (units per second). `by`
/// is the player credited when what it hits is hurt or broken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Push {
    pub target: ObjectRef,
    pub velocity: [f32; 3],
    pub by: Option<u64>,
}
impl ScriptOp for Push {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "push";
    fn bounded(&self) -> bool {
        let Push { velocity, .. } = self;
        finite(velocity) && glam_length(velocity) <= MAX_PUSH_SPEED
    }
}

/// Knock a player off their feet into a tumble, flying at `velocity`;
/// for `seconds` (0.1 to 60) when given, else until it settles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tumble {
    pub player: u64,
    pub velocity: [f32; 3],
    pub by: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f32>,
}
impl ScriptOp for Tumble {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "tumble";
    fn bounded(&self) -> bool {
        let Tumble {
            velocity, seconds, ..
        } = self;
        finite(velocity)
            && glam_length(velocity) <= MAX_PUSH_SPEED
            && seconds.is_none_or(|s| (0.1..=60.0).contains(&s))
    }
}

/// Keep `target` floating `distance` ahead of `player`'s eye, where
/// they look, until let go. The engine pulls it there every tick; heavy
/// things lag. A player holds one thing at a time.
///
/// `at` is the point on the object it is held by (world space, now),
/// else its middle. `force` limits how hard it pulls (the engine's
/// default otherwise). With `turn`, the object keeps the turn it had
/// relative to the holder's heading, so it swings round with them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hold {
    pub player: u64,
    pub target: ObjectRef,
    pub distance: f32,
    pub at: Option<[f32; 3]>,
    pub force: Option<f32>,
    pub turn: bool,
}
impl ScriptOp for Hold {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "hold";
    fn bounded(&self) -> bool {
        let Hold {
            distance,
            at,
            force,
            ..
        } = self;
        distance.is_finite()
            && (0.5..=MAX_HOLD_DISTANCE).contains(distance)
            && at.as_ref().is_none_or(|a| finite(a))
            && force.is_none_or(|f| f.is_finite() && f > 0.0 && f <= MAX_HOLD_FORCE)
    }
}

/// Carry what `player` holds `distance` from their eye from now on
/// (reeling it in or out).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoldDistance {
    pub player: u64,
    pub distance: f32,
}
impl ScriptOp for HoldDistance {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "hold_distance";
    fn bounded(&self) -> bool {
        let HoldDistance { distance, .. } = self;
        distance.is_finite() && (0.5..=MAX_HOLD_DISTANCE).contains(distance)
    }
}

/// Let go of what `player` holds, and stop reaching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LetGo {
    pub player: u64,
}
impl ScriptOp for LetGo {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "let_go";
    fn bounded(&self) -> bool {
        true
    }
}

/// Tie `player` to `anchor` with a rope `length` long (`None`: exactly
/// as long as it spans now): they move freely within it and swing on
/// it (the player motor's `Tether`). `brick`
/// ties it to that brick, and the rope breaks when the brick goes;
/// `object` ties it to that spot on a player, vehicle or entity, which
/// carries the anchor along as it moves and turns, and the rope breaks
/// when it goes. `reel` is how fast `TetherLength` changes it and
/// `swing` how hard the movement keys push a hanging player (the
/// engine's defaults otherwise). `keys` (`[shortest, longest]`) lets
/// the player's jump and crouch keys reel it in and out between those.
/// With `straight`, reeling in draws the player straight along it.
/// A player has one rope; a new one replaces it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tether {
    pub player: u64,
    pub anchor: [f32; 3],
    pub length: Option<f32>,
    pub brick: Option<u64>,
    pub reel: Option<f32>,
    pub swing: Option<f32>,
    #[serde(default)]
    pub object: Option<ObjectRef>,
    #[serde(default)]
    pub keys: Option<[f32; 2]>,
    #[serde(default)]
    pub straight: bool,
}
impl ScriptOp for Tether {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "tether";
    fn bounded(&self) -> bool {
        let Tether {
            anchor,
            length,
            brick,
            reel,
            swing,
            object,
            keys,
            ..
        } = self;
        let span = MIN_TETHER_LENGTH..=MAX_TETHER_LENGTH;
        finite(anchor)
            && length.is_none_or(|l| span.contains(&l))
            && reel.is_none_or(|r| (0.0..=MAX_TETHER_REEL).contains(&r))
            && swing.is_none_or(|s| (0.0..=MAX_TETHER_SWING).contains(&s))
            && !(brick.is_some() && object.is_some())
            && keys.is_none_or(|[short, long]| {
                span.contains(&short) && span.contains(&long) && short <= long
            })
    }
}

/// Reel `player`'s rope toward `length`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TetherLength {
    pub player: u64,
    pub length: f32,
}
impl ScriptOp for TetherLength {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "tether_length";
    fn bounded(&self) -> bool {
        let TetherLength { length, .. } = self;
        (MIN_TETHER_LENGTH..=MAX_TETHER_LENGTH).contains(length)
    }
}

/// Cut `player`'s rope. With `keep` (0 to 1), the player keeps only
/// that fraction of their speed relative to what the rope was tied to,
/// as a rope's grip slows them as it lets go.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Untether {
    pub player: u64,
    #[serde(default)]
    pub keep: Option<f32>,
}
impl ScriptOp for Untether {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "untether";
    fn bounded(&self) -> bool {
        true
    }
}

/// Keep reaching for something to hold: every tick, while `player`
/// holds nothing, the engine looks where they look, up to `distance`,
/// and holds the first thing it meets that they may move, by the spot
/// it met, as far off as it was (at least `near`), as [`Op::Hold`]
/// with `force` and `turn` would. Reaching ends once it holds
/// something, on `let_go`, or when the player dies. The script sees
/// the catch with `held` (a gun whose trigger stays down catches what
/// comes in range, with no second click).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reach {
    pub player: u64,
    pub distance: f32,
    pub near: f32,
    pub force: Option<f32>,
    pub turn: bool,
}
impl ScriptOp for Reach {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "reach";
    fn bounded(&self) -> bool {
        let Reach {
            distance,
            near,
            force,
            ..
        } = self;
        distance.is_finite()
            && near.is_finite()
            && (0.5..=MAX_HOLD_DISTANCE).contains(near)
            && (*near..=MAX_HOLD_DISTANCE).contains(distance)
            && force.is_none_or(|f| f.is_finite() && f > 0.0 && f <= MAX_HOLD_FORCE)
    }
}

/// Spawn a vehicle definition (`namespace:vehicle/name`) of this package
/// or one it depends on, turned `yaw` radians and moving at `velocity`.
/// `owner` is the player it belongs to (their trust and minigame rules
/// apply), or the world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnVehicle {
    pub definition: String,
    pub position: [f32; 3],
    pub yaw: f32,
    pub velocity: [f32; 3],
    pub owner: Option<u64>,
}
impl ScriptOp for SpawnVehicle {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "spawn_vehicle";
    fn bounded(&self) -> bool {
        let SpawnVehicle {
            definition,
            position,
            yaw,
            velocity,
            ..
        } = self;
        bri_package::id::is_content_ref(definition, Some("vehicle"))
            && finite(position)
            && yaw.is_finite()
            && finite(velocity)
            && glam_length(velocity) <= MAX_PUSH_SPEED
    }
}

/// Remove a vehicle this package spawned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoveVehicle {
    pub vehicle: u64,
}
impl ScriptOp for RemoveVehicle {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "remove_vehicle";
    fn bounded(&self) -> bool {
        true
    }
}

/// Seat player `rider` on player `mount`'s mount point `node`
/// (`%mount.mountObject(%rider, %node)`; a Blockhead's `Mount<node>`):
/// carried with it and drawn on that node as it animates. With
/// `can_dismount` false the rider cannot get off by jumping
/// (`canDismount = 0`). Riders a rule seats stay on through the mount
/// changing body while the new one has the node. `turn` (radians,
/// clockwise seen from above) turns the rider's body on the mount
/// point, as a `setTransform` on a mounted player sets its `mRot.z`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MountObject {
    pub mount: u64,
    pub rider: u64,
    pub node: u8,
    pub can_dismount: bool,
    pub turn: f32,
}
impl ScriptOp for MountObject {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "mount_object";
    fn bounded(&self) -> bool {
        let MountObject {
            mount,
            rider,
            node,
            turn,
            ..
        } = self;
        mount != rider && usize::from(*node) < MAX_MOUNT_POINTS && turn.is_finite()
    }
}

/// Take `rider` off the player they ride, where they are, moving as
/// the mount moved (`unMountObject`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnmountObject {
    pub rider: u64,
}
impl ScriptOp for UnmountObject {
    const CAPABILITY: &str = "physics";
    const NAME: &str = "unmount_object";
    fn bounded(&self) -> bool {
        true
    }
}
