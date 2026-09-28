//! The sandboxed server script runtime (a Rhai prototype, not a commitment).
//!
//! A script is a set of functions. The engine calls them with a read-only
//! snapshot of the game; a script reads that snapshot, edits its own
//! package's state and entity variables, and asks for operations. Nothing a
//! script does touches the game directly: the engine commits the state and
//! applies the operations afterwards, through [`crate::ops::authorize`].
//!
//! Sandbox: no file, network, clock, module or `eval` access; a fixed
//! operation budget per call; bounded strings, arrays, maps, call depth and
//! operation count. A failing or over-budget call changes nothing.
use crate::manifest::location;
use crate::ops::Op;
use crate::state::{Namespace, PlayerKey, check_value};
use bri_package::diag::Diagnostic;
use rhai::{AST, Array, Dynamic, Engine, EvalAltResult, Map};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Operation budgets per kind of call.
#[derive(Debug, Clone, Copy)]
pub enum Budget {
    Command,
    Think,
    Tick,
    Generate,
}
impl Budget {
    pub fn operations(self) -> u64 {
        match self {
            Self::Command => 200_000,
            Self::Think => 100_000,
            Self::Tick => 400_000,
            Self::Generate => 4_000_000,
        }
    }
}
const MAX_OPS_PER_CALL: usize = 1024;
const MAX_OUTPUT_LINES: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerView {
    pub id: u64,
    pub key: PlayerKey,
    pub name: String,
    pub position: [f32; 3],
    pub alive: bool,
    pub admin: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityView {
    pub id: u64,
    pub kind: String,
    pub position: [f32; 3],
    pub yaw: f32,
    pub label: String,
    pub health: f32,
    /// Horizontal speed, units per second.
    pub speed: f32,
}
/// What the caller is aiming at, resolved by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aim {
    pub brick: Option<u64>,
    /// The brick's provider tag (for generated voxels, the material id).
    pub tag: Option<String>,
    pub position: [f32; 3],
    pub distance: f32,
}
/// Read-only game facts for one tick.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub tick: u64,
    pub seed: i64,
    pub players: Vec<PlayerView>,
    pub entities: Vec<EntityView>,
}

/// One call's inputs.
pub struct Call<'a> {
    pub function: &'a str,
    pub args: Vec<Dynamic>,
    pub budget: Budget,
    pub snapshot: Arc<Snapshot>,
    pub caller: Option<u64>,
    pub aim: Option<Aim>,
    /// The entity a `think` call is for.
    pub entity: Option<u64>,
    pub state: Namespace,
    /// Package-local variables of the package's entities.
    pub entity_vars: BTreeMap<u64, BTreeMap<String, serde_json::Value>>,
}
/// One call's results. Only produced when the call succeeded.
#[derive(Debug)]
pub struct Outcome {
    pub returned: Dynamic,
    pub ops: Vec<Op>,
    pub state: Namespace,
    pub entity_vars: BTreeMap<u64, BTreeMap<String, serde_json::Value>>,
    pub output: Vec<String>,
}

struct Invocation {
    snapshot: Arc<Snapshot>,
    caller: Option<u64>,
    aim: Option<Aim>,
    entity: Option<u64>,
    state: Namespace,
    entity_vars: BTreeMap<u64, BTreeMap<String, serde_json::Value>>,
    ops: Vec<Op>,
    output: Vec<String>,
}
thread_local! {
    static CURRENT: RefCell<Option<Invocation>> = const { RefCell::new(None) };
}
type Fallible<T> = Result<T, Box<EvalAltResult>>;
fn fail<T>(message: impl Into<String>) -> Fallible<T> {
    Err(message.into().into())
}
fn with<T>(f: impl FnOnce(&mut Invocation) -> Fallible<T>) -> Fallible<T> {
    CURRENT.with(|c| match c.borrow_mut().as_mut() {
        Some(invocation) => f(invocation),
        None => fail("no script call is active"),
    })
}
fn number(value: &Dynamic) -> Fallible<f64> {
    if let Ok(i) = value.as_int() {
        Ok(i as f64)
    } else if let Ok(f) = value.as_float() {
        Ok(f)
    } else {
        fail(format!("expected a number, got {}", value.type_name()))
    }
}
fn float(value: &Dynamic) -> Fallible<f32> {
    let v = number(value)?;
    if v.is_finite() {
        Ok(v as f32)
    } else {
        fail("number is not finite")
    }
}
fn id(value: &Dynamic) -> Fallible<u64> {
    match value.as_int() {
        Ok(i) if i >= 0 => Ok(i as u64),
        _ => fail(format!("expected an id, got {}", value.type_name())),
    }
}
fn push(op: Op) -> Fallible<()> {
    with(|i| {
        if i.ops.len() >= MAX_OPS_PER_CALL {
            return fail(format!(
                "more than {MAX_OPS_PER_CALL} operations in one call"
            ));
        }
        i.ops.push(op);
        Ok(())
    })
}
fn to_json(value: &Dynamic) -> Fallible<serde_json::Value> {
    let json: serde_json::Value = rhai::serde::from_dynamic(value)?;
    check_value(&json).map_err(|e| e.to_string())?;
    Ok(json)
}
fn to_dynamic(value: &serde_json::Value) -> Dynamic {
    rhai::serde::to_dynamic(value).unwrap_or(Dynamic::UNIT)
}
fn map(entries: impl IntoIterator<Item = (&'static str, Dynamic)>) -> Dynamic {
    let mut m = Map::new();
    for (k, v) in entries {
        m.insert(k.into(), v);
    }
    Dynamic::from_map(m)
}
fn position(p: [f32; 3]) -> [(&'static str, Dynamic); 3] {
    [
        ("x", Dynamic::from_float(p[0] as f64)),
        ("y", Dynamic::from_float(p[1] as f64)),
        ("z", Dynamic::from_float(p[2] as f64)),
    ]
}
fn player_map(p: &PlayerView) -> Dynamic {
    let [x, y, z] = position(p.position);
    map([
        ("id", Dynamic::from_int(p.id as i64)),
        ("name", p.name.clone().into()),
        x,
        y,
        z,
        ("alive", p.alive.into()),
        ("admin", p.admin.into()),
    ])
}
pub fn entity_map(e: &EntityView) -> Dynamic {
    let [x, y, z] = position(e.position);
    map([
        ("id", Dynamic::from_int(e.id as i64)),
        ("kind", e.kind.clone().into()),
        x,
        y,
        z,
        ("yaw", Dynamic::from_float(e.yaw as f64)),
        ("label", e.label.clone().into()),
        ("health", Dynamic::from_float(e.health as f64)),
        ("speed", Dynamic::from_float(e.speed as f64)),
    ])
}
fn player_key(i: &Invocation, player: &Dynamic) -> Fallible<PlayerKey> {
    let player = id(player)?;
    i.snapshot
        .players
        .iter()
        .find(|p| p.id == player)
        .map(|p| p.key.clone())
        .ok_or_else(|| format!("no player {player}").into())
}

fn register_api(engine: &mut Engine) {
    engine.register_fn("tick", || with(|i| Ok(i.snapshot.tick as i64)));
    engine.register_fn("seed", || with(|i| Ok(i.snapshot.seed)));
    engine.register_fn("caller", || {
        with(|i| {
            Ok(i.caller
                .map_or(Dynamic::UNIT, |c| Dynamic::from_int(c as i64)))
        })
    });
    engine.register_fn("players", || {
        with(|i| Ok(i.snapshot.players.iter().map(player_map).collect::<Array>()))
    });
    engine.register_fn("player", |player: Dynamic| {
        with(|i| {
            let player = id(&player)?;
            Ok(i.snapshot
                .players
                .iter()
                .find(|p| p.id == player)
                .map_or(Dynamic::UNIT, player_map))
        })
    });
    engine.register_fn("entities", || {
        with(|i| {
            Ok(i.snapshot
                .entities
                .iter()
                .map(entity_map)
                .collect::<Array>())
        })
    });
    engine.register_fn("me", || {
        with(|i| {
            let Some(me) = i.entity else {
                return Ok(Dynamic::UNIT);
            };
            Ok(i.snapshot
                .entities
                .iter()
                .find(|e| e.id == me)
                .map_or(Dynamic::UNIT, entity_map))
        })
    });
    engine.register_fn("aim", || {
        with(|i| {
            Ok(i.aim.as_ref().map_or(Dynamic::UNIT, |a| {
                let [x, y, z] = position(a.position);
                map([
                    (
                        "brick",
                        a.brick
                            .map_or(Dynamic::UNIT, |b| Dynamic::from_int(b as i64)),
                    ),
                    ("tag", a.tag.clone().map_or(Dynamic::UNIT, Dynamic::from)),
                    x,
                    y,
                    z,
                    ("distance", Dynamic::from_float(a.distance as f64)),
                ])
            }))
        })
    });
    // Package state: global keys, then per-player keys.
    engine.register_fn("get", |key: &str| {
        with(|i| Ok(i.state.global.get(key).map_or(Dynamic::UNIT, to_dynamic)))
    });
    engine.register_fn("set", |key: &str, value: Dynamic| {
        with(|i| {
            i.state.global.insert(key.into(), to_json(&value)?);
            Ok(())
        })
    });
    engine.register_fn("get_player", |player: Dynamic, key: &str| {
        with(|i| {
            let k = player_key(i, &player)?;
            Ok(i.state
                .players
                .get(&k)
                .and_then(|m| m.get(key))
                .map_or(Dynamic::UNIT, to_dynamic))
        })
    });
    engine.register_fn(
        "set_player",
        |player: Dynamic, key: &str, value: Dynamic| {
            with(|i| {
                let k = player_key(i, &player)?;
                let v = to_json(&value)?;
                i.state.players.entry(k).or_default().insert(key.into(), v);
                Ok(())
            })
        },
    );
    engine.register_fn(
        "add_player",
        |player: Dynamic, key: &str, amount: Dynamic| {
            with(|i| {
                let k = player_key(i, &player)?;
                let values = i.state.players.entry(k).or_default();
                let current = values
                    .get(key)
                    .cloned()
                    .unwrap_or(serde_json::Value::from(0));
                let next = match (current.as_i64(), amount.as_int()) {
                    (Some(a), Ok(b)) => {
                        serde_json::Value::from(a.checked_add(b).ok_or("state number overflow")?)
                    }
                    _ => {
                        let sum = current.as_f64().unwrap_or(0.0) + number(&amount)?;
                        serde_json::Number::from_f64(sum)
                            .map(serde_json::Value::Number)
                            .ok_or("state number is not finite")?
                    }
                };
                values.insert(key.into(), next.clone());
                Ok(to_dynamic(&next))
            })
        },
    );
    // Package-local entity variables.
    engine.register_fn("entity_get", |entity: Dynamic, key: &str| {
        with(|i| {
            let e = id(&entity)?;
            Ok(i.entity_vars
                .get(&e)
                .and_then(|m| m.get(key))
                .map_or(Dynamic::UNIT, to_dynamic))
        })
    });
    engine.register_fn(
        "entity_set",
        |entity: Dynamic, key: &str, value: Dynamic| {
            with(|i| {
                let e = id(&entity)?;
                if !i.entity_vars.contains_key(&e) {
                    return fail(format!("entity {e} does not belong to this package"));
                }
                let v = to_json(&value)?;
                i.entity_vars.entry(e).or_default().insert(key.into(), v);
                Ok(())
            })
        },
    );
    // Pure helpers.
    engine.register_fn("noise", |seed: i64, x: Dynamic, z: Dynamic| {
        Ok::<_, Box<EvalAltResult>>(crate::noise::value2(seed, number(&x)?, number(&z)?))
    });
    engine.register_fn("hash3", |seed: i64, x: i64, y: i64, z: i64| {
        crate::noise::hash3(seed, x, y, z)
    });
    // Operations.
    engine.register_fn("remove_brick", |brick: Dynamic| {
        push(Op::RemoveBrick { brick: id(&brick)? })
    });
    engine.register_fn(
        "explode",
        |x: Dynamic,
         y: Dynamic,
         z: Dynamic,
         radius: Dynamic,
         damage: Dynamic,
         brick_radius: Dynamic| {
            push(Op::Explode {
                position: [float(&x)?, float(&y)?, float(&z)?],
                radius: float(&radius)?,
                damage: float(&damage)?,
                brick_radius: float(&brick_radius)?,
            })
        },
    );
    engine.register_fn("damage", |player: Dynamic, amount: Dynamic| {
        push(Op::DamagePlayer {
            player: id(&player)?,
            amount: float(&amount)?,
            by: None,
        })
    });
    engine.register_fn(
        "damage",
        |player: Dynamic, amount: Dynamic, by: Dynamic| {
            push(Op::DamagePlayer {
                player: id(&player)?,
                amount: float(&amount)?,
                by: Some(id(&by)?),
            })
        },
    );
    engine.register_fn(
        "teleport",
        |player: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic| {
            push(Op::Teleport {
                player: id(&player)?,
                position: [float(&x)?, float(&y)?, float(&z)?],
            })
        },
    );
    engine.register_fn("respawn", |player: Dynamic| {
        push(Op::Respawn {
            player: id(&player)?,
        })
    });
    engine.register_fn(
        "spawn_entity",
        |kind: &str, x: Dynamic, y: Dynamic, z: Dynamic| {
            push(Op::SpawnEntity {
                kind: kind.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
            })
        },
    );
    engine.register_fn("remove_entity", |entity: Dynamic| {
        push(Op::RemoveEntity {
            entity: id(&entity)?,
        })
    });
    engine.register_fn(
        "steer",
        |entity: Dynamic, dx: Dynamic, dz: Dynamic, jump: bool| {
            push(Op::Steer {
                entity: id(&entity)?,
                direction: [float(&dx)?, float(&dz)?],
                jump,
            })
        },
    );
    engine.register_fn("label", |entity: Dynamic, label: &str| {
        push(Op::Label {
            entity: id(&entity)?,
            label: label.into(),
        })
    });
    engine.register_fn("tell", |player: Dynamic, text: &str| {
        push(Op::Tell {
            player: id(&player)?,
            text: text.into(),
        })
    });
    engine.register_fn("broadcast", |text: &str| {
        push(Op::Broadcast { text: text.into() })
    });
}

fn sandbox() -> Engine {
    use rhai::packages::{
        BasicArrayPackage, BasicMapPackage, BasicMathPackage, BasicStringPackage, CorePackage,
        LogicPackage, MoreStringPackage, Package,
    };
    let mut engine = Engine::new_raw();
    CorePackage::new().register_into_engine(&mut engine);
    LogicPackage::new().register_into_engine(&mut engine);
    BasicMathPackage::new().register_into_engine(&mut engine);
    BasicArrayPackage::new().register_into_engine(&mut engine);
    BasicMapPackage::new().register_into_engine(&mut engine);
    BasicStringPackage::new().register_into_engine(&mut engine);
    MoreStringPackage::new().register_into_engine(&mut engine);
    engine.disable_symbol("eval");
    engine.set_max_call_levels(32);
    engine.set_max_expr_depths(64, 32);
    engine.set_max_string_size(4096);
    engine.set_max_array_size(65_536);
    engine.set_max_map_size(1024);
    engine.set_max_variables(256);
    engine.set_max_functions(256);
    engine.on_print(|text| {
        CURRENT.with(|c| {
            if let Some(i) = c.borrow_mut().as_mut()
                && i.output.len() < MAX_OUTPUT_LINES
            {
                i.output.push(text.chars().take(256).collect());
            }
        })
    });
    engine.on_debug(|_, _, _| {});
    register_api(&mut engine);
    engine
}

/// Compiled scripts for every package with behaviour.
pub struct Runtime {
    engine: Engine,
    scripts: BTreeMap<String, Arc<AST>>,
    /// Script file per package, for diagnostics.
    sources: BTreeMap<String, String>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            engine: sandbox(),
            scripts: BTreeMap::new(),
            sources: BTreeMap::new(),
        }
    }
}
impl Runtime {
    /// Compile each package's script and check that the functions its
    /// content names exist with the right arity.
    pub fn compile(set: &crate::Catalog) -> Result<Self, Vec<Diagnostic>> {
        let mut runtime = Self::default();
        let mut problems = Vec::new();
        for (id, package) in &set.packages {
            let Some(behaviour) = &package.behaviour else {
                continue;
            };
            let Some(source) = package.script_source() else {
                continue;
            };
            let ast = match runtime.engine.compile(source) {
                Ok(ast) => ast,
                Err(e) => {
                    let at = match e.1.line() {
                        Some(line) => format!("{}:{line}", location(id, &behaviour.script)),
                        None => location(id, &behaviour.script),
                    };
                    problems.push(Diagnostic::error("script.syntax", e.0.to_string()).at(at));
                    continue;
                }
            };
            if !ast.statements().is_empty() {
                problems.push(
                    Diagnostic::error("script.top_level", "scripts may only define functions")
                        .at(location(id, &behaviour.script))
                        .hint("move top-level statements into a function; keep constants in behaviour state"),
                );
            }
            let has = |name: &str, arity: usize| {
                ast.iter_functions()
                    .any(|f| f.name == name && f.params.len() == arity)
            };
            let mut need = |name: String, arity: usize, why: &str| {
                if !has(&name, arity) {
                    problems.push(
                        Diagnostic::error(
                            "script.missing_function",
                            format!("{why} needs `fn {name}` with {arity} parameter(s)"),
                        )
                        .at(location(id, &behaviour.script)),
                    );
                }
            };
            for c in &behaviour.commands {
                need(
                    format!("cmd_{}", c.name),
                    1 + c.args.len(),
                    &format!("command `{}`", c.name),
                );
            }
            if behaviour.on_join {
                need("on_join".into(), 1, "on_join");
            }
            if behaviour.tick_interval.is_some() {
                need("on_tick".into(), 0, "tick_interval");
            }
            for w in package.worlds.values() {
                need(w.generate.clone(), 2, "the world provider");
            }
            for e in package.entities.values() {
                need(e.think.clone(), 1, &format!("entity `{}`", e.name));
            }
            runtime.scripts.insert(id.clone(), Arc::new(ast));
            runtime.sources.insert(id.clone(), behaviour.script.clone());
        }
        if problems.is_empty() {
            Ok(runtime)
        } else {
            Err(problems)
        }
    }
    pub fn has_script(&self, package: &str) -> bool {
        self.scripts.contains_key(package)
    }
    /// Run one function. On error nothing of the call is kept.
    pub fn call(&mut self, package: &str, call: Call<'_>) -> Result<Outcome, Diagnostic> {
        let ast =
            self.scripts.get(package).cloned().ok_or_else(|| {
                Diagnostic::error("script.none", "package has no script").at(package)
            })?;
        self.engine.set_max_operations(call.budget.operations());
        let previous = CURRENT.with(|c| {
            c.borrow_mut().replace(Invocation {
                snapshot: call.snapshot,
                caller: call.caller,
                aim: call.aim,
                entity: call.entity,
                state: call.state,
                entity_vars: call.entity_vars,
                ops: Vec::new(),
                output: Vec::new(),
            })
        });
        let options = rhai::CallFnOptions::new()
            .eval_ast(false)
            .rewind_scope(true);
        let mut scope = rhai::Scope::new();
        let result = self.engine.call_fn_with_options::<Dynamic>(
            options,
            &mut scope,
            &ast,
            call.function,
            call.args,
        );
        let invocation = CURRENT
            .with(|c| std::mem::replace(&mut *c.borrow_mut(), previous))
            .expect("set above");
        match result {
            Ok(returned) => Ok(Outcome {
                returned,
                ops: invocation.ops,
                state: invocation.state,
                entity_vars: invocation.entity_vars,
                output: invocation.output,
            }),
            Err(e) => {
                let code = match *e {
                    EvalAltResult::ErrorTooManyOperations(_) => "script.budget",
                    EvalAltResult::ErrorDataTooLarge(..) | EvalAltResult::ErrorStackOverflow(_) => {
                        "script.limit"
                    }
                    _ => "script.error",
                };
                let position = e.position();
                let script = self
                    .sources
                    .get(package)
                    .map_or(String::new(), Clone::clone);
                let mut problem =
                    Diagnostic::error(code, format!("{}: {}", call.function, e)).at(match position
                        .line()
                    {
                        Some(line) => format!("{}:{line}", location(package, &script)),
                        None => location(package, &script),
                    });
                if code == "script.budget" {
                    problem = problem.hint(format!(
                        "the call exceeded {} script operations",
                        call.budget.operations()
                    ));
                }
                Err(problem)
            }
        }
    }
}

/// Convert a script's `[[x, y, z, m], ...]` into voxels, bounded.
pub fn voxels(value: &Dynamic, materials: usize, limit: usize) -> Result<Vec<[i64; 4]>, String> {
    let array = value.read_lock::<Array>().ok_or_else(|| {
        format!(
            "expected an array of [x, y, z, material], got {}",
            value.type_name()
        )
    })?;
    if array.len() > limit {
        return Err(format!(
            "{} voxels is more than the chunk limit {limit}",
            array.len()
        ));
    }
    let mut out = Vec::with_capacity(array.len());
    for entry in array.iter() {
        let v = entry
            .read_lock::<Array>()
            .filter(|v| v.len() == 4)
            .ok_or("each voxel is [x, y, z, material]")?;
        let mut item = [0_i64; 4];
        for (slot, value) in item.iter_mut().zip(v.iter()) {
            *slot = value
                .as_int()
                .map_err(|_| "voxel coordinates and material are integers")?;
        }
        if item[3] < 0 || item[3] as usize >= materials {
            return Err(format!("material {} is not declared", item[3]));
        }
        if item.iter().take(3).any(|c| c.abs() > 100_000) {
            return Err("voxel coordinate out of range".into());
        }
        out.push(item);
    }
    Ok(out)
}
