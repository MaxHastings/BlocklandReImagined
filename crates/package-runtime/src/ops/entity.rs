//! Operations behind the `entity` capability.
use super::*;

/// `vars` are the entity's first package-local variables, so what a
/// package creates is addressable from its first think (an owner, a
/// team, a home).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnEntity {
    pub kind: String,
    pub position: [f32; 3],
    pub vars: BTreeMap<String, serde_json::Value>,
}
impl ScriptOp for SpawnEntity {
    const CAPABILITY: &str = "entity";
    const NAME: &str = "spawn_entity";
    fn bounded(&self) -> bool {
        let SpawnEntity {
            kind,
            position,
            vars,
        } = self;
        kind.len() <= 128
            && finite(position)
            && vars.len() <= MAX_SPAWN_VARS
            && vars.values().all(|v| crate::state::check_value(v).is_ok())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoveEntity {
    pub entity: u64,
}
impl ScriptOp for RemoveEntity {
    const CAPABILITY: &str = "entity";
    const NAME: &str = "remove_entity";
    fn bounded(&self) -> bool {
        true
    }
}

/// Walk direction on the ground plane (normalised by the engine), jump.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Steer {
    pub entity: u64,
    pub direction: [f32; 2],
    pub jump: bool,
}
impl ScriptOp for Steer {
    const CAPABILITY: &str = "entity";
    const NAME: &str = "steer";
    fn bounded(&self) -> bool {
        let Steer { direction, .. } = self;
        finite(direction)
    }
}

/// A short replicated label clients may present (model colours).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub entity: u64,
    pub label: String,
}
impl ScriptOp for Label {
    const CAPABILITY: &str = "entity";
    const NAME: &str = "label";
    fn bounded(&self) -> bool {
        let Label { label, .. } = self;
        label.len() <= 32 && !label.chars().any(char::is_control)
    }
}
