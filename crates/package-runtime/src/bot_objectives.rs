//! Explicit package-owned desired states, not script inference or bot controls.
//! The package's ordinary pickup/zone policy remains the sole gameplay writer.
use crate::{
    script::Outcome,
    state::{Namespace, PlayerKey},
};
use serde::{Deserialize, Serialize};

pub const MAX_OBJECTIVES: usize = 8;
pub const MAX_DESTINATIONS: usize = 8;
const MAX_TEXT_BYTES: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DesiredState {
    CarryReturn(CarryReturn),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarryReturn {
    pub id: String,
    pub source: PickupSource,
    /// Physical pickup capability, checked against the live source's item.
    pub item: String,
    /// Provider identity for this actual incarnation; replacement invalidates it.
    pub epoch: CounterBinding,
    /// Existing package-zone bricks, not arbitrary script-supplied waypoints.
    pub destinations: Vec<u64>,
    pub carriage: Carriage,
    pub completion: CounterBinding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PickupSource {
    Brick { brick: u64 },
    Drop { drop: u64, spawner: u64 },
}
impl PickupSource {
    pub fn spawner(self) -> u64 {
        match self {
            Self::Brick { brick } => brick,
            Self::Drop { spawner, .. } => spawner,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Carriage {
    /// This package's declared per-player state must equal the source spawner.
    pub key: String,
    #[serde(default)]
    pub worn: Option<WornImage>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WornImage {
    pub slot: u8,
    pub image: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterScope {
    Global,
    Player,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterBinding {
    pub scope: CounterScope,
    pub key: String,
    #[serde(default)]
    pub path: Vec<String>,
}
impl CounterBinding {
    /// Missing leaves in a declared counter map mean zero. Missing declared
    /// state, a scalar along the path, or a noninteger counter is unavailable.
    pub fn read(&self, state: &Namespace, player: &PlayerKey) -> Option<i64> {
        self.read_with_missing(state, player, true)
    }

    /// Incarnation identity must be explicitly initialized by owner policy.
    pub fn read_initialized(&self, state: &Namespace, player: &PlayerKey) -> Option<i64> {
        self.read_with_missing(state, player, false)
    }

    fn read_with_missing(
        &self,
        state: &Namespace,
        player: &PlayerKey,
        missing_zero: bool,
    ) -> Option<i64> {
        let mut value = match self.scope {
            CounterScope::Global => state.global.get(&self.key)?,
            CounterScope::Player => state.players.get(player)?.get(&self.key)?,
        };
        for part in &self.path {
            let map = value.as_object()?;
            let Some(next) = map.get(part) else {
                return missing_zero.then_some(0);
            };
            value = next;
        }
        value.as_i64().filter(|n| *n >= 0)
    }
}

fn state_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
fn text(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
fn id(value: u64) -> bool {
    value > 0 && value <= i64::MAX as u64
}

impl DesiredState {
    pub fn validate(&self) -> Result<usize, String> {
        let Self::CarryReturn(goal) = self;
        if !text(&goal.id) || !text(&goal.item) || !goal.item.contains(':') {
            return Err("objective identity/item is invalid".into());
        }
        if !id(goal.source.spawner())
            || matches!(goal.source, PickupSource::Drop { drop, .. } if !id(drop))
        {
            return Err("pickup source identity is invalid".into());
        }
        if goal.destinations.is_empty()
            || goal.destinations.len() > MAX_DESTINATIONS
            || goal.destinations.iter().any(|v| !id(*v))
            || goal
                .destinations
                .iter()
                .enumerate()
                .any(|(i, v)| goal.destinations[..i].contains(v))
        {
            return Err("return destinations exceed bounds or repeat".into());
        }
        if !state_key(&goal.carriage.key)
            || [&goal.completion, &goal.epoch]
                .iter()
                .any(|b| !state_key(&b.key) || b.path.len() > 4 || b.path.iter().any(|p| !text(p)))
        {
            return Err("carriage/counter binding is invalid".into());
        }
        if goal
            .carriage
            .worn
            .as_ref()
            .is_some_and(|w| !matches!(w.slot, 2 | 3) || !text(&w.image) || !w.image.contains(':'))
        {
            return Err("carriage image binding is invalid".into());
        }
        Ok(goal.id.len()
            + goal.item.len()
            + goal.carriage.key.len()
            + [&goal.completion, &goal.epoch]
                .iter()
                .map(|b| b.key.len() + b.path.iter().map(String::len).sum::<usize>())
                .sum::<usize>()
            + goal.carriage.worn.as_ref().map_or(0, |w| w.image.len()))
    }
}

/// Decode only a bounded array. Native admission also checks declarations,
/// namespace, live item/carriage/zone geometry and game/round/team stamps.
pub fn decode(value: &rhai::Dynamic) -> Result<Vec<DesiredState>, String> {
    let array = value
        .read_lock::<rhai::Array>()
        .ok_or("bot_objectives must return an array")?;
    if array.len() > MAX_OBJECTIVES {
        return Err("package objective count exceeds 8".into());
    }
    let mut result = Vec::with_capacity(array.len());
    let mut bytes = 0usize;
    for item in array.iter() {
        // Reject raw collection fanout before serde allocates descriptor Vecs.
        let map = item
            .read_lock::<rhai::Map>()
            .ok_or("package objective must be a typed map")?;
        if map.len() > 8 {
            return Err("package objective fields exceed bounds".into());
        }
        let destinations = map
            .get("destinations")
            .and_then(|v| v.read_lock::<rhai::Array>())
            .ok_or("return destinations must be an array")?;
        if destinations.len() > MAX_DESTINATIONS {
            return Err("return destinations exceed 8".into());
        }
        if ["completion", "epoch"].iter().any(|key| {
            map.get(*key)
                .and_then(|v| v.read_lock::<rhai::Map>())
                .is_some_and(|m| {
                    m.get("path")
                        .and_then(|v| v.read_lock::<rhai::Array>())
                        .is_some_and(|path| path.len() > 4)
                })
        }) {
            return Err("counter path exceeds bounds".into());
        }
        let goal: DesiredState = rhai::serde::from_dynamic(item)
            .map_err(|e| format!("unsupported package objective: {e}"))?;
        bytes += goal.validate()?;
        if bytes > MAX_TEXT_BYTES {
            return Err("package objective text budget exceeded".into());
        }
        if result.iter().any(|old: &DesiredState| {
            let DesiredState::CarryReturn(a) = old;
            let DesiredState::CarryReturn(b) = &goal;
            a.id == b.id
        }) {
            return Err("package objective identity repeated".into());
        }
        result.push(goal);
    }
    Ok(result)
}

/// Defense in depth after Runtime::query rejects even attempted writes.
pub fn read_only(before: &Namespace, outcome: &Outcome) -> Result<(), String> {
    if !outcome.ops.is_empty()
        || outcome.state != *before
        || !outcome.entity_vars.is_empty()
        || !outcome.output.is_empty()
    {
        return Err("objective discovery attempted a gameplay/state/output mutation".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn goal() -> serde_json::Value {
        json!({"kind":"carry_return","id":"strange-task","source":{"kind":"brick","brick":17},
            "item":"unseen:weapon/token","epoch":{"scope":"global","key":"incarnations","path":["17"]},"destinations":[24],
            "carriage":{"key":"burden","worn":{"slot":2,"image":"unseen:image/token"}},
            "completion":{"scope":"global","key":"jobs","path":["1","done","actor:9"]}})
    }
    fn parse(value: serde_json::Value) -> Result<Vec<DesiredState>, String> {
        decode(&rhai::serde::to_dynamic(value).unwrap())
    }
    #[test]
    fn unknown_semantics_and_fields_are_rejected() {
        let mut g = goal();
        g["kind"] = json!("magical_delivery");
        assert!(parse(json!([g])).is_err());
        let mut g = goal();
        g["teleport"] = json!(true);
        assert!(parse(json!([g])).is_err());
    }
    #[test]
    fn bounds_are_checked_before_action_expansion() {
        let mut g = goal();
        g["destinations"] = json!([1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert!(parse(json!([g])).is_err());
        assert!(parse(json!(vec![goal(); 9])).is_err());
        assert!(parse(json!([goal(), goal()])).is_err());
        assert!(parse(json!([goal()])).is_ok());
    }
    #[test]
    fn incarnation_requires_initialized_nonnegative_integer() {
        let DesiredState::CarryReturn(g) = parse(json!([goal()])).unwrap().remove(0);
        let p = PlayerKey("test:9".into());
        let mut ns = Namespace::default();
        ns.global.insert("incarnations".into(), json!({}));
        assert_eq!(g.epoch.read_initialized(&ns, &p), None);
        for value in [json!(-1), json!(1.5), json!("1"), json!(null)] {
            ns.global.insert("incarnations".into(), json!({"17":value}));
            assert_eq!(g.epoch.read_initialized(&ns, &p), None);
        }
        ns.global.insert("incarnations".into(), json!({"17":0}));
        assert_eq!(g.epoch.read_initialized(&ns, &p), Some(0));
        ns.global.insert("incarnations".into(), json!({"17":11}));
        assert_eq!(g.epoch.read_initialized(&ns, &p), Some(11));
    }
    #[test]
    fn completion_reads_the_declared_package_counter_only() {
        let DesiredState::CarryReturn(g) = parse(json!([goal()])).unwrap().remove(0);
        let p = PlayerKey("test:9".into());
        let mut ns = Namespace::default();
        ns.global.insert("jobs".into(), json!({"1":{"done":{}}}));
        assert_eq!(g.completion.read(&ns, &p), Some(0));
        ns.global
            .insert("jobs".into(), json!({"1":{"done":{"actor:9":3}}}));
        assert_eq!(g.completion.read(&ns, &p), Some(3));
        ns.global
            .insert("jobs".into(), json!({"1":{"done":{"actor:9":"3"}}}));
        assert_eq!(g.completion.read(&ns, &p), None);
        ns.global.remove("jobs");
        assert_eq!(g.completion.read(&ns, &p), None);
    }
}
