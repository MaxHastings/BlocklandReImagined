//! Operations behind the `damage` capability.
use super::*;

/// Damage players within `radius` (falling off linearly) and destroy
/// bricks within `brick_radius`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Explode {
    pub position: [f32; 3],
    pub radius: f32,
    pub damage: f32,
    pub brick_radius: f32,
    /// How it looks and sounds: an explosion of the weapons pack by
    /// name (`rocketExplosion`, an imported Add-On's own); the rocket's
    /// when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explosion: Option<String>,
}
impl ScriptOp for Explode {
    const CAPABILITY: &str = "damage";
    const NAME: &str = "explode";
    fn bounded(&self) -> bool {
        let Explode {
            position,
            radius,
            damage,
            brick_radius,
            explosion,
        } = self;
        explosion.as_deref().is_none_or(|e| {
            (1..=64).contains(&e.len()) && e.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        }) && finite(position)
            && (0.0..=32.0).contains(radius)
            && (0.0..=1000.0).contains(damage)
            && (0.0..=16.0).contains(brick_radius)
    }
}

/// Damage a player, vehicle or entity (`%obj.damage`). `by` is the
/// player credited; `damage_type` names a weapons pack damage type (its
/// kill message, vehicle scale and whether it is a direct hit), or the
/// package itself when `None`. Scripts decide who may be hurt; they ask
/// the minigame rules with `can_damage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Damage {
    pub target: ObjectRef,
    pub amount: f32,
    pub by: Option<u64>,
    pub damage_type: Option<String>,
}
impl ScriptOp for Damage {
    const CAPABILITY: &str = "damage";
    const NAME: &str = "damage";
    fn bounded(&self) -> bool {
        let Damage {
            amount,
            damage_type,
            ..
        } = self;
        amount.is_finite()
            && (0.0..=1000.0).contains(amount)
            && damage_type
                .as_deref()
                .is_none_or(|t| !t.is_empty() && t.len() <= 64 && !t.chars().any(char::is_control))
    }
}

/// Launch a projectile of this package's weapons, or a dependency's,
/// from `position` at `velocity`: a creature's gun, a trap, a fireball.
/// With `by` it is that player's shot, hurting whom their shots may;
/// without, the package's own, which hurts any living player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fire {
    pub projectile: String,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub by: Option<u64>,
}
impl ScriptOp for Fire {
    const CAPABILITY: &str = "damage";
    const NAME: &str = "fire";
    fn bounded(&self) -> bool {
        let Fire {
            projectile,
            position,
            velocity,
            ..
        } = self;
        bri_package::id::is_content_ref(projectile, Some("projectile"))
            && finite(position)
            && finite(velocity)
            && glam_length(velocity) <= MAX_FIRE_SPEED
    }
}

/// A projectile's explosion on a living player (`%obj.spawnExplosion`),
/// `scale` times its size (0.1 to 10): an emote, a crit's burst. It
/// hurts and pushes as the explosion would.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnExplosion {
    pub player: u64,
    pub projectile: String,
    pub scale: f32,
}
impl ScriptOp for SpawnExplosion {
    const CAPABILITY: &str = "damage";
    const NAME: &str = "spawn_explosion";
    fn bounded(&self) -> bool {
        let SpawnExplosion {
            projectile, scale, ..
        } = self;
        bri_package::id::is_content_ref(projectile, Some("projectile"))
            && (0.1..=10.0).contains(scale)
    }
}

/// Give a living player health, up to their archetype's most.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Heal {
    pub player: u64,
    pub amount: f32,
}
impl ScriptOp for Heal {
    const CAPABILITY: &str = "damage";
    const NAME: &str = "heal";
    fn bounded(&self) -> bool {
        let Heal { amount, .. } = self;
        amount.is_finite() && (0.0..=100_000.0).contains(amount)
    }
}
