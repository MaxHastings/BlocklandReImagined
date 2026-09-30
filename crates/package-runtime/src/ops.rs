//! The operations package behaviour may ask the engine to perform, and the
//! one place they are checked against a package's declared capabilities.
use bri_package::diag::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Variables an entity may be given when it is spawned.
pub const MAX_SPAWN_VARS: usize = 16;
/// Fastest a script may set anything moving, units per second.
pub const MAX_PUSH_SPEED: f32 = 200.0;
/// Fastest projectile `fire` launches, units a second (the weapons
/// runtime's own limit).
pub const MAX_FIRE_SPEED: f32 = 10_000.0;
/// The mass scripts see for a player or entity body (Torque's player
/// `mass` is 90 as well).
pub const PLAYER_MASS: f32 = 90.0;
/// Farthest ahead of a player's eye a held object may float.
pub const MAX_HOLD_DISTANCE: f32 = 32.0;
/// Longest ray `raycast` casts, and longest `beam`, in units.
pub const MAX_RAY_RANGE: f32 = 2000.0;
/// Rays one script call may cast.
pub const MAX_RAYS_PER_CALL: usize = 64;
/// The field of view `set_fov` may give, degrees (Torque's player camera
/// `cameraMinFov` and `cameraMaxFov`).
pub const FOV_RANGE: std::ops::RangeInclusive<f32> = 5.0..=120.0;
/// Widest `beam`, units, and longest it lasts, seconds.
pub const MAX_BEAM_WIDTH: f32 = 16.0;
pub const MAX_BEAM_SECONDS: f32 = 10.0;
/// Widest sphere `set_map_lights` covers, units, and brightest it makes a
/// light (times its recovered colour).
pub const MAX_LIGHT_RADIUS: f32 = 2000.0;
pub const MAX_LIGHT_TINT: f32 = 4.0;

/// Something in the world that moves: a player, a vehicle (any loose
/// physics body: cars, balls, tumbling bodies) or a package entity.
/// Scripts name one as `"player:3"`, `"vehicle:12"` or `"entity:7"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ObjectRef {
    Player(u64),
    Vehicle(u64),
    Entity(u64),
}
impl ObjectRef {
    pub fn parse(text: &str) -> Option<Self> {
        let (kind, id) = text.split_once(':')?;
        let id: u64 = id.parse().ok()?;
        match kind {
            "player" => Some(Self::Player(id)),
            "vehicle" => Some(Self::Vehicle(id)),
            "entity" => Some(Self::Entity(id)),
            _ => None,
        }
    }
    pub fn kind(self) -> &'static str {
        match self {
            Self::Player(_) => "player",
            Self::Vehicle(_) => "vehicle",
            Self::Entity(_) => "entity",
        }
    }
    pub fn id(self) -> u64 {
        match self {
            Self::Player(id) | Self::Vehicle(id) | Self::Entity(id) => id,
        }
    }
}
impl std::fmt::Display for ObjectRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.kind(), self.id())
    }
}

/// Every capability a manifest may declare (with plain-language words in
/// `bri_package::capability`).
pub use bri_package::capability::CAPABILITIES;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Op {
    RemoveBrick {
        brick: u64,
    },
    /// Add a world-owned brick of a known shape: an arena, a gate, a board.
    /// The colour is matched to the nearest colour of the world's palette.
    PlaceBrick {
        shape: String,
        position: [f32; 3],
        color: [f32; 4],
    },
    /// Damage players within `radius` (falling off linearly) and destroy
    /// bricks within `brick_radius`.
    Explode {
        position: [f32; 3],
        radius: f32,
        damage: f32,
        brick_radius: f32,
    },
    /// Damage a player, vehicle or entity (`%obj.damage`). `by` is the
    /// player credited; `damage_type` names a weapons pack damage type (its
    /// kill message, vehicle scale and whether it is a direct hit), or the
    /// package itself when `None`. Scripts decide who may be hurt; they ask
    /// the minigame rules with `can_damage`.
    Damage {
        target: ObjectRef,
        amount: f32,
        by: Option<u64>,
        damage_type: Option<String>,
    },
    /// Move a living player, keeping their facing.
    Teleport {
        player: u64,
        position: [f32; 3],
    },
    /// Give a player a new life at a spawn point, alive or dead.
    Respawn {
        player: u64,
    },
    /// Make a player this archetype (a package's `archetype` id or v20's
    /// `v20.player.<datablock>`), now and at every respawn. An empty id
    /// hands the choice back to the mini-game's player type.
    SetArchetype {
        player: u64,
        archetype: String,
    },
    /// Show a block brick in one of its block's named states (`""` for the
    /// block's own faces): a dig tool cracks it, a switch lights it.
    SetBlockState {
        brick: u64,
        state: String,
    },
    /// Hand a player's movement input to one of the package's entities
    /// (`entity`), or back to the player's own body (`None`). The avatar
    /// stands where it was while the entity is driven.
    Control {
        player: u64,
        entity: Option<u64>,
    },
    /// `vars` are the entity's first package-local variables, so what a
    /// package creates is addressable from its first think (an owner, a
    /// team, a home).
    SpawnEntity {
        kind: String,
        position: [f32; 3],
        vars: BTreeMap<String, serde_json::Value>,
    },
    RemoveEntity {
        entity: u64,
    },
    /// Walk direction on the ground plane (normalised by the engine), jump.
    Steer {
        entity: u64,
        direction: [f32; 2],
        jump: bool,
    },
    /// A short replicated label clients may present (model colours).
    Label {
        entity: u64,
        label: String,
    },
    Tell {
        player: u64,
        text: String,
    },
    Broadcast {
        text: String,
    },
    /// Copy the build at `brick` for `player` to place with `tool`: the
    /// brick and every brick joined to it that the player may build on,
    /// with `above_only` none below the brick. More than `limit` bricks is
    /// refused.
    CopyBuild {
        player: u64,
        brick: u64,
        limit: u32,
        above_only: bool,
        tool: String,
    },
    /// Put an item in a player's tool list (unless they carry it) and,
    /// with `equip`, in their hand.
    GiveItem {
        player: u64,
        item: String,
        equip: bool,
    },
    /// Change an object's velocity by `velocity` (units per second). `by`
    /// is the player credited when what it hits is hurt or broken.
    Push {
        target: ObjectRef,
        velocity: [f32; 3],
        by: Option<u64>,
    },
    /// Knock a player off their feet into a tumble, flying at `velocity`.
    Tumble {
        player: u64,
        velocity: [f32; 3],
        by: Option<u64>,
    },
    /// Keep `target` floating `distance` ahead of `player`'s eye, where
    /// they look, until let go. The engine pulls it there every tick; heavy
    /// things lag. A player holds one thing at a time.
    Hold {
        player: u64,
        target: ObjectRef,
        distance: f32,
    },
    /// Let go of what `player` holds.
    LetGo {
        player: u64,
    },
    /// Spawn a vehicle definition (`namespace:vehicle/name`) of this package
    /// or one it depends on, turned `yaw` radians and moving at `velocity`.
    /// `owner` is the player it belongs to (their trust and minigame rules
    /// apply), or the world.
    SpawnVehicle {
        definition: String,
        position: [f32; 3],
        yaw: f32,
        velocity: [f32; 3],
        owner: Option<u64>,
    },
    /// Remove a vehicle this package spawned.
    RemoveVehicle {
        vehicle: u64,
    },
    /// Launch a projectile of this package's weapons, or a dependency's,
    /// from `position` at `velocity`: a creature's gun, a trap, a fireball.
    /// With `by` it is that player's shot, hurting whom their shots may;
    /// without, the package's own, which hurts any living player.
    Fire {
        projectile: String,
        position: [f32; 3],
        velocity: [f32; 3],
        by: Option<u64>,
    },
    /// Give a living player health, up to their archetype's most.
    Heal {
        player: u64,
        amount: f32,
    },
    /// Text in the middle of the screen (`centerPrint`), or above the
    /// bottom edge (`bottomPrint`), for `seconds`: one player's, or
    /// everyone's when `player` is `None`. Empty text clears it.
    Print {
        player: Option<u64>,
        text: String,
        seconds: f32,
        bottom: bool,
    },
    /// Play a sound profile (an Add-On weapons pack's `sounds`, or v20's):
    /// at `position` for everyone near, or at one player's ears.
    Sound {
        profile: String,
        at: SoundAt,
    },
    /// A straight beam from `from` to `to` for `seconds`, fading out: a
    /// tracer, a laser, a bolt. With `muzzle`, clients start it at that
    /// player's held muzzle as they draw it. Presentation only.
    Beam {
        from: [f32; 3],
        to: [f32; 3],
        color: [f32; 4],
        width: f32,
        seconds: f32,
        muzzle: Option<u64>,
    },
    /// Play an animation on a player's body (`playThread`): thread 2 the
    /// arms with what they hold, thread 3 a gesture; `root` stops it.
    PlayThread {
        player: u64,
        thread: u8,
        sequence: String,
    },
    /// Every map light within `radius` of `position` shines at `tint` times
    /// its recovered colour (0 switches it off, 1 is as the map was lit),
    /// for every player, until the map changes.
    SetMapLights {
        position: [f32; 3],
        radius: f32,
        tint: [f32; 3],
    },
    /// Set a player's field of view (`setControlCameraFov`), or hand it back
    /// to their own setting with `None`.
    SetFov {
        player: u64,
        fov: Option<f32>,
    },
    /// Whether the image in a player's hand has ammo (`setImageAmmo`), which
    /// its states' `ammo` transitions read.
    SetImageAmmo {
        player: u64,
        ammo: bool,
    },
    /// Put another image in a player's hand, keeping their tool slot
    /// (`mountImage`): a scope, a second fire mode. `None` puts back the
    /// selected tool's own image.
    MountImage {
        player: u64,
        image: Option<String>,
    },
}
/// Where [`Op::Sound`] plays.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SoundAt {
    /// In the world, heard by everyone near it.
    Position([f32; 3]),
    /// At one player's ears only.
    Player(u64),
}
/// Longest text a print may show.
pub const MAX_PRINT_CHARS: usize = 512;
impl Op {
    pub fn capability(&self) -> &'static str {
        match self {
            Self::RemoveBrick { .. } | Self::PlaceBrick { .. } | Self::SetBlockState { .. } => {
                "world.edit"
            }
            Self::Explode { .. }
            | Self::Damage { .. }
            | Self::Heal { .. }
            | Self::Fire { .. } => "damage",
            Self::SpawnEntity { .. }
            | Self::RemoveEntity { .. }
            | Self::Steer { .. }
            | Self::Label { .. } => "entity",
            Self::Tell { .. } | Self::Broadcast { .. } | Self::Print { .. } => "chat",
            Self::Sound { .. } | Self::Beam { .. } | Self::PlayThread { .. } => "effects",
            Self::CopyBuild { .. } => "build",
            Self::SetMapLights { .. } => "lighting",
            Self::Teleport { .. }
            | Self::Respawn { .. }
            | Self::SetArchetype { .. }
            | Self::Control { .. }
            | Self::GiveItem { .. }
            | Self::SetFov { .. }
            | Self::SetImageAmmo { .. }
            | Self::MountImage { .. } => "player",
            Self::Push { .. }
            | Self::Tumble { .. }
            | Self::Hold { .. }
            | Self::LetGo { .. }
            | Self::SpawnVehicle { .. }
            | Self::RemoveVehicle { .. } => "physics",
        }
    }
    /// Shape limits, independent of who asks.
    fn bounded(&self) -> Result<(), String> {
        let finite = |v: &[f32]| v.iter().all(|x| x.is_finite() && x.abs() <= 1_000_000.0);
        let chat =
            |t: &str| !t.trim().is_empty() && t.len() <= 256 && !t.chars().any(char::is_control);
        let item = |t: &str| bri_package::id::is_content_ref(t, Some("weapon"));
        let ok = match self {
            Self::RemoveBrick { .. }
            | Self::RemoveEntity { .. }
            | Self::Respawn { .. }
            | Self::Control { .. }
            | Self::SetImageAmmo { .. } => true,
            Self::Teleport { position, .. } => finite(position),
            Self::SetBlockState { state, .. } => {
                state.len() <= 64 && !state.chars().any(char::is_control)
            }
            Self::SetArchetype { archetype, .. } => {
                archetype.len() <= 160 && !archetype.chars().any(char::is_control)
            }
            Self::PlaceBrick {
                shape,
                position,
                color,
            } => {
                !shape.is_empty()
                    && shape.len() <= 128
                    && finite(position)
                    && color.iter().all(|c| (0.0..=1.0).contains(c))
            }
            Self::Explode {
                position,
                radius,
                damage,
                brick_radius,
            } => {
                finite(position)
                    && (0.0..=32.0).contains(radius)
                    && (0.0..=1000.0).contains(damage)
                    && (0.0..=16.0).contains(brick_radius)
            }
            Self::Damage {
                amount,
                damage_type,
                ..
            } => {
                amount.is_finite()
                    && (0.0..=1000.0).contains(amount)
                    && damage_type.as_deref().is_none_or(|t| {
                        !t.is_empty() && t.len() <= 64 && !t.chars().any(char::is_control)
                    })
            }
            Self::Beam {
                from,
                to,
                color,
                width,
                seconds,
                ..
            } => {
                let span = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
                finite(from)
                    && finite(to)
                    && glam_length(&span) <= MAX_RAY_RANGE
                    && color.iter().all(|c| (0.0..=1.0).contains(c))
                    && width.is_finite()
                    && *width > 0.0
                    && *width <= MAX_BEAM_WIDTH
                    && seconds.is_finite()
                    && *seconds > 0.0
                    && *seconds <= MAX_BEAM_SECONDS
            }
            Self::PlayThread {
                thread, sequence, ..
            } => {
                (2..=3).contains(thread)
                    && !sequence.is_empty()
                    && sequence.len() <= 64
                    && sequence
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            }
            Self::SetFov { fov, .. } => fov.is_none_or(|f| FOV_RANGE.contains(&f)),
            Self::SetMapLights {
                position,
                radius,
                tint,
            } => {
                finite(position)
                    && radius.is_finite()
                    && (0.0..=MAX_LIGHT_RADIUS).contains(radius)
                    && tint.iter().all(|t| t.is_finite() && (0.0..=MAX_LIGHT_TINT).contains(t))
            }
            Self::MountImage { image, .. } => image
                .as_deref()
                .is_none_or(|i| bri_package::id::is_content_ref(i, Some("image"))),
            Self::SpawnEntity {
                kind,
                position,
                vars,
            } => {
                kind.len() <= 128
                    && finite(position)
                    && vars.len() <= MAX_SPAWN_VARS
                    && vars.values().all(|v| crate::state::check_value(v).is_ok())
            }
            Self::Steer { direction, .. } => finite(direction),
            Self::Label { label, .. } => label.len() <= 32 && !label.chars().any(char::is_control),
            Self::Tell { text, .. } | Self::Broadcast { text } => chat(text),
            Self::CopyBuild { limit, tool, .. } => (1..=10_000).contains(limit) && item(tool),
            Self::GiveItem { item: id, .. } => item(id),
            Self::Push { velocity, .. } | Self::Tumble { velocity, .. } => {
                finite(velocity) && glam_length(velocity) <= MAX_PUSH_SPEED
            }
            Self::Hold { distance, .. } => {
                distance.is_finite() && (0.5..=MAX_HOLD_DISTANCE).contains(distance)
            }
            Self::LetGo { .. } | Self::RemoveVehicle { .. } => true,
            Self::Fire {
                projectile,
                position,
                velocity,
                ..
            } => {
                bri_package::id::is_content_ref(projectile, Some("projectile"))
                    && finite(position)
                    && finite(velocity)
                    && glam_length(velocity) <= MAX_FIRE_SPEED
            }
            Self::Heal { amount, .. } => amount.is_finite() && (0.0..=100_000.0).contains(amount),
            Self::Print { text, seconds, .. } => {
                text.chars().count() <= MAX_PRINT_CHARS
                    && !text.chars().any(|c| c.is_control() && c != '\n')
                    && seconds.is_finite()
                    && (0.0..=600.0).contains(seconds)
            }
            Self::Sound { profile, at } => {
                !profile.is_empty()
                    && profile.len() <= 128
                    && !profile.chars().any(char::is_control)
                    && match at {
                        SoundAt::Position(p) => finite(p),
                        SoundAt::Player(_) => true,
                    }
            }
            Self::SpawnVehicle {
                definition,
                position,
                yaw,
                velocity,
                ..
            } => {
                bri_package::id::is_content_ref(definition, Some("vehicle"))
                    && finite(position)
                    && yaw.is_finite()
                    && finite(velocity)
                    && glam_length(velocity) <= MAX_PUSH_SPEED
            }
        };
        if ok {
            Ok(())
        } else {
            Err(format!("{self:?} is outside the operation's limits"))
        }
    }
}

fn glam_length(v: &[f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Check an operation a package asked for. This is the single capability
/// gate: every package operation passes through it before the engine acts.
/// Ownership checks that need live state (a package may steer only its own
/// entities) are the caller's second step and report through the same codes.
pub fn authorize(package: &str, capabilities: &[String], op: &Op) -> Result<(), Diagnostic> {
    op.bounded()
        .map_err(|m| Diagnostic::error("op.bounds", m).at(package))?;
    let needed = op.capability();
    if !capabilities.iter().any(|c| c == needed) {
        return Err(Diagnostic::error(
            "op.capability",
            format!("{} needs capability `{needed}`, which the package does not declare", op_name(op)),
        )
        .at(package)
        .hint(format!("add \"{needed}\" to capabilities in package.json; the server owner sees it when enabling the package")));
    }
    if let Op::SpawnEntity { kind, .. } = op
        && kind.split(':').next() != Some(package)
    {
        return Err(Diagnostic::error(
            "op.foreign_entity",
            format!("cannot spawn `{kind}`: packages spawn only their own entity kinds"),
        )
        .at(package));
    }
    Ok(())
}
pub fn op_name(op: &Op) -> &'static str {
    match op {
        Op::RemoveBrick { .. } => "remove_brick",
        Op::PlaceBrick { .. } => "place_brick",
        Op::Explode { .. } => "explode",
        Op::Damage { .. } => "damage",
        Op::Beam { .. } => "beam",
        Op::PlayThread { .. } => "play_thread",
        Op::SetFov { .. } => "set_fov",
        Op::SetMapLights { .. } => "set_map_lights",
        Op::SetImageAmmo { .. } => "set_image_ammo",
        Op::MountImage { .. } => "mount_image",
        Op::SpawnEntity { .. } => "spawn_entity",
        Op::RemoveEntity { .. } => "remove_entity",
        Op::Steer { .. } => "steer",
        Op::Label { .. } => "label",
        Op::Tell { .. } => "tell",
        Op::Teleport { .. } => "teleport",
        Op::Respawn { .. } => "respawn",
        Op::SetArchetype { .. } => "set_archetype",
        Op::Control { .. } => "control",
        Op::SetBlockState { .. } => "set_block_state",
        Op::Broadcast { .. } => "broadcast",
        Op::CopyBuild { .. } => "copy_build",
        Op::GiveItem { .. } => "give_item",
        Op::Push { .. } => "push",
        Op::Tumble { .. } => "tumble",
        Op::Hold { .. } => "hold",
        Op::LetGo { .. } => "let_go",
        Op::SpawnVehicle { .. } => "spawn_vehicle",
        Op::RemoveVehicle { .. } => "remove_vehicle",
        Op::Fire { .. } => "fire",
        Op::Heal { .. } => "heal",
        Op::Print { bottom: false, .. } => "center_print",
        Op::Print { bottom: true, .. } => "bottom_print",
        Op::Sound {
            at: SoundAt::Position(_),
            ..
        } => "sound_at",
        Op::Sound {
            at: SoundAt::Player(_),
            ..
        } => "play_sound",
    }
}
