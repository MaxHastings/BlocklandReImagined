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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DesiredState {
    CarryReturn(CarryReturn),
    Work(Work),
    Guard(Guard),
}

/// Most bots that may work or guard one offer at once.
pub const MAX_CLAIMANTS: u8 = 8;
/// Longest a place may need its control held, seconds.
pub const MAX_WORK_SECONDS: f32 = 600.0;
/// Widest a place's "close enough" may be, world units.
pub const MAX_WORK_RADIUS: f32 = 16.0;
/// Largest offer weight.
pub const MAX_WEIGHT: f32 = 100.0;

/// Stand at a place and hold a control until the rules say it is done:
/// a valve, a control panel, a capture point, a terminal. The host walks the
/// bot there (ordinary routes), holds the rules command as a player's key
/// would, and counts the offer done when `completion` rises.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Work {
    pub id: String,
    /// Where to stand (feet), world units.
    pub at: [f32; 3],
    /// How close counts as at it.
    pub radius: f32,
    pub command: WorkCommand,
    /// How long the rules need it held; times out a stalled hold and makes a
    /// longer job less attractive.
    pub seconds: f32,
    /// Workable while this declared state is truthy; gone when it falls.
    #[serde(default)]
    pub open: Option<CounterBinding>,
    /// A declared counter that must rise for the offer to count as done.
    pub completion: CounterBinding,
    /// Most bots working it at once.
    #[serde(default = "one")]
    pub claimants: u8,
    /// The rules' priority among offers.
    #[serde(default = "unit")]
    pub weight: f32,
}

/// The rules command a bot sends, as the player's key would.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkCommand {
    /// The package declaring it; the offering Add-On's own when left out.
    #[serde(default)]
    pub package: Option<String>,
    pub name: String,
    /// Held like a key: the command (taking one `bool`) is sent `true` when
    /// the bot arrives and `false` when it lets go. Otherwise it is sent
    /// once, with no arguments.
    #[serde(default)]
    pub hold: bool,
}

/// Stand within `radius` of a place and fight what comes: for the side that
/// does not progress the place. Scores below a `Work` of the same weight.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    pub id: String,
    pub at: [f32; 3],
    pub radius: f32,
    #[serde(default = "one")]
    pub claimants: u8,
    #[serde(default = "unit")]
    pub weight: f32,
}

fn one() -> u8 {
    1
}
fn unit() -> f32 {
    1.0
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

impl CounterBinding {
    /// Whether the declared state is on: a `true`, or a nonzero integer.
    /// A missing map leaf or a null is off; missing declared state, a scalar
    /// along the path, or any other value type is unavailable.
    pub fn read_truthy(&self, state: &Namespace, player: &PlayerKey) -> Option<bool> {
        let mut value = match self.scope {
            CounterScope::Global => state.global.get(&self.key)?,
            CounterScope::Player => state.players.get(player)?.get(&self.key)?,
        };
        for part in &self.path {
            let map = value.as_object()?;
            let Some(next) = map.get(part) else {
                return Some(false);
            };
            value = next;
        }
        match value {
            serde_json::Value::Null => Some(false),
            serde_json::Value::Bool(on) => Some(*on),
            other => other.as_i64().map(|n| n != 0),
        }
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

fn place(at: &[f32; 3], radius: f32) -> bool {
    at.iter().all(|c| c.is_finite() && c.abs() <= 1.0e6)
        && radius.is_finite()
        && (0.25..=MAX_WORK_RADIUS).contains(&radius)
}
fn binding(b: &CounterBinding) -> bool {
    state_key(&b.key) && b.path.len() <= 4 && b.path.iter().all(|p| text(p))
}
fn binding_bytes(b: &CounterBinding) -> usize {
    b.key.len() + b.path.iter().map(String::len).sum::<usize>()
}
fn share(claimants: u8, weight: f32) -> bool {
    (1..=MAX_CLAIMANTS).contains(&claimants)
        && weight.is_finite()
        && weight > 0.0
        && weight <= MAX_WEIGHT
}

impl DesiredState {
    pub fn id(&self) -> &str {
        match self {
            Self::CarryReturn(goal) => &goal.id,
            Self::Work(work) => &work.id,
            Self::Guard(guard) => &guard.id,
        }
    }
    pub fn validate(&self) -> Result<usize, String> {
        match self {
            Self::CarryReturn(goal) => goal.validate(),
            Self::Work(work) => work.validate(),
            Self::Guard(guard) => {
                if !text(&guard.id) || !place(&guard.at, guard.radius) {
                    return Err("guard identity or place is invalid".into());
                }
                if !share(guard.claimants, guard.weight) {
                    return Err("guard claimants or weight is out of bounds".into());
                }
                Ok(guard.id.len())
            }
        }
    }
}

impl Work {
    fn validate(&self) -> Result<usize, String> {
        if !text(&self.id) || !place(&self.at, self.radius) {
            return Err("work identity or place is invalid".into());
        }
        if !text(&self.command.name)
            || self.command.name.len() > 64
            || self
                .command
                .package
                .as_deref()
                .is_some_and(|p| !text(p) || p.contains(':'))
        {
            return Err("work command is invalid".into());
        }
        if !self.seconds.is_finite() || !(0.1..=MAX_WORK_SECONDS).contains(&self.seconds) {
            return Err("work seconds is out of bounds".into());
        }
        if !share(self.claimants, self.weight) {
            return Err("work claimants or weight is out of bounds".into());
        }
        if !binding(&self.completion) || self.open.as_ref().is_some_and(|b| !binding(b)) {
            return Err("work state binding is invalid".into());
        }
        Ok(self.id.len()
            + self.command.name.len()
            + self.command.package.as_ref().map_or(0, String::len)
            + binding_bytes(&self.completion)
            + self.open.as_ref().map_or(0, binding_bytes))
    }
}

impl CarryReturn {
    fn validate(&self) -> Result<usize, String> {
        let goal = self;
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
            || ![&goal.completion, &goal.epoch].iter().all(|b| binding(b))
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
                .map(|b| binding_bytes(b))
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
        if map.len() > 12 {
            return Err("package objective fields exceed bounds".into());
        }
        let carry = map
            .get("kind")
            .and_then(|v| v.clone().into_immutable_string().ok())
            .is_some_and(|kind| kind == "carry_return");
        if carry {
            let destinations = map
                .get("destinations")
                .and_then(|v| v.read_lock::<rhai::Array>())
                .ok_or("return destinations must be an array")?;
            if destinations.len() > MAX_DESTINATIONS {
                return Err("return destinations exceed 8".into());
            }
        }
        if ["completion", "epoch", "open"].iter().any(|key| {
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
        let goal: DesiredState = rhai::serde::from_dynamic(&crate::script::view::plain(item))
            .map_err(|e| format!("unsupported package objective: {e}"))?;
        bytes += goal.validate()?;
        if bytes > MAX_TEXT_BYTES {
            return Err("package objective text budget exceeded".into());
        }
        if result
            .iter()
            .any(|old: &DesiredState| old.id() == goal.id())
        {
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
    fn work() -> serde_json::Value {
        json!({"kind":"work","id":"valve:primary","at":[1.0,2.0,3.0],"radius":1.5,
            "command":{"name":"use","hold":true},"seconds":10.0,
            "open":{"scope":"global","key":"valve_on","path":[]},
            "completion":{"scope":"player","key":"done","path":[]},"claimants":2,"weight":2.0})
    }
    #[test]
    fn work_and_guard_offers_decode_with_defaults_and_bounds() {
        let offers = parse(json!([
            work(),
            {"kind":"guard","id":"valve:guard","at":[0.0,0.0,0.0],"radius":6.0}
        ]))
        .unwrap();
        let DesiredState::Guard(guard) = &offers[1] else {
            panic!("a guard")
        };
        assert_eq!((guard.claimants, guard.weight), (1, 1.0));
        for (field, bad) in [
            ("radius", json!(0.0)),
            ("seconds", json!(601.0)),
            ("claimants", json!(0)),
            ("claimants", json!(9)),
            ("weight", json!(0.0)),
            ("at", json!([f64::NAN, 0.0, 0.0])),
        ] {
            let mut w = work();
            w[field] = bad;
            assert!(parse(json!([w])).is_err(), "{field}");
        }
        let mut w = work();
        w["teleport"] = json!(true);
        assert!(parse(json!([w])).is_err());
        // Offers share one identity space across kinds.
        let mut g = work();
        g["kind"] = json!("guard");
        assert!(parse(json!([work(), g])).is_err());
    }
    #[test]
    fn open_state_reads_on_and_off() {
        let DesiredState::Work(w) = parse(json!([work()])).unwrap().remove(0) else {
            panic!("work")
        };
        let open = w.open.unwrap();
        let p = PlayerKey("test:9".into());
        let mut ns = Namespace::default();
        assert_eq!(open.read_truthy(&ns, &p), None);
        for (value, on) in [
            (json!(true), true),
            (json!(false), false),
            (json!(0), false),
            (json!(3), true),
            (json!(null), false),
        ] {
            ns.global.insert("valve_on".into(), value);
            assert_eq!(open.read_truthy(&ns, &p), Some(on));
        }
        ns.global.insert("valve_on".into(), json!("yes"));
        assert_eq!(open.read_truthy(&ns, &p), None);
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
        let DesiredState::CarryReturn(g) = parse(json!([goal()])).unwrap().remove(0) else {
            panic!("a carry_return")
        };
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
        let DesiredState::CarryReturn(g) = parse(json!([goal()])).unwrap().remove(0) else {
            panic!("a carry_return")
        };
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
