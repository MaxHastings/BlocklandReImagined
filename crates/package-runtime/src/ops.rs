//! The operations package behaviour may ask the engine to perform, and the
//! one place they are checked against a package's declared capabilities.
use bri_package::diag::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Variables an entity may be given when it is spawned.
pub const MAX_SPAWN_VARS: usize = 16;

/// Every capability a manifest may declare.
pub const CAPABILITIES: &[&str] = &[
    // Add world-owned bricks to the world and remove them.
    "world.edit",
    // Explosions and direct damage to players and bricks.
    "damage",
    // Spawn, steer and remove the package's own entities.
    "entity",
    // Send chat lines to players.
    "chat",
    // Move players and respawn them (a race start, a round reset).
    "player",
];

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
    /// Damage a player; `by` is the player credited if it kills.
    DamagePlayer {
        player: u64,
        amount: f32,
        by: Option<u64>,
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
}
impl Op {
    pub fn capability(&self) -> &'static str {
        match self {
            Self::RemoveBrick { .. } | Self::PlaceBrick { .. } => "world.edit",
            Self::Explode { .. } | Self::DamagePlayer { .. } => "damage",
            Self::SpawnEntity { .. }
            | Self::RemoveEntity { .. }
            | Self::Steer { .. }
            | Self::Label { .. } => "entity",
            Self::Tell { .. } | Self::Broadcast { .. } => "chat",
            Self::Teleport { .. } | Self::Respawn { .. } => "player",
        }
    }
    /// Shape limits, independent of who asks.
    fn bounded(&self) -> Result<(), String> {
        let finite = |v: &[f32]| v.iter().all(|x| x.is_finite() && x.abs() <= 1_000_000.0);
        let chat =
            |t: &str| !t.trim().is_empty() && t.len() <= 256 && !t.chars().any(char::is_control);
        let ok = match self {
            Self::RemoveBrick { .. } | Self::RemoveEntity { .. } | Self::Respawn { .. } => true,
            Self::Teleport { position, .. } => finite(position),
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
            Self::DamagePlayer { amount, .. } => {
                amount.is_finite() && (0.0..=1000.0).contains(amount)
            }
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
        };
        if ok {
            Ok(())
        } else {
            Err(format!("{self:?} is outside the operation's limits"))
        }
    }
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
        Op::DamagePlayer { .. } => "damage",
        Op::SpawnEntity { .. } => "spawn_entity",
        Op::RemoveEntity { .. } => "remove_entity",
        Op::Steer { .. } => "steer",
        Op::Label { .. } => "label",
        Op::Tell { .. } => "tell",
        Op::Teleport { .. } => "teleport",
        Op::Respawn { .. } => "respawn",
        Op::Broadcast { .. } => "broadcast",
    }
}
