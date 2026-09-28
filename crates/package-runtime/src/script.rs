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
use crate::ops::{ObjectRef, Op};
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
            // One chunk is generated per tick, so its budget fits a tick.
            Self::Generate => 400_000,
        }
    }
}
const MAX_OPS_PER_CALL: usize = 1024;
const MAX_OUTPUT_LINES: usize = 32;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerView {
    pub id: u64,
    pub key: PlayerKey,
    pub name: String,
    pub position: [f32; 3],
    pub alive: bool,
    pub admin: bool,
    /// Where the player sees from, and the unit direction they look.
    #[serde(default)]
    pub eye: [f32; 3],
    #[serde(default)]
    pub look: [f32; 3],
    #[serde(default)]
    pub velocity: [f32; 3],
    /// The item in their hand (`namespace:weapon/name`), or empty.
    #[serde(default)]
    pub item: String,
}
/// A loose physics body or other movable thing, as scripts see it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectView {
    pub object: ObjectRef,
    /// A vehicle's definition, an entity's kind; empty for players.
    pub definition: String,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub mass: f32,
    /// Radius of a sphere around it, units.
    pub radius: f32,
    /// The player it belongs to (a vehicle's spawner, the player
    /// themselves), when there is one.
    pub owner: Option<u64>,
    /// The package that spawned it (`spawn_vehicle`), or empty.
    #[serde(default)]
    pub package: String,
}
/// What a player holds (`hold`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldView {
    pub player: u64,
    pub object: ObjectRef,
    pub distance: f32,
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
    /// The brick's block and its state, when it shows a block.
    #[serde(default)]
    pub look: Option<(String, String)>,
    pub position: [f32; 3],
    pub distance: f32,
    /// The movable object the aim met before any brick, and whether the
    /// caller may move it under the minigame and trust rules.
    #[serde(default)]
    pub object: Option<ObjectRef>,
    #[serde(default)]
    pub movable: bool,
}
/// Read-only game facts for one tick.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub tick: u64,
    pub seed: i64,
    pub players: Vec<PlayerView>,
    pub entities: Vec<EntityView>,
    /// Vehicles and other loose physics bodies (players and entities are
    /// in their own lists, and in [`object`](Self::object)'s answers).
    pub objects: Vec<ObjectView>,
    pub holds: Vec<HoldView>,
}
impl Snapshot {
    /// Any movable object by reference, players and entities included.
    pub fn object(&self, object: ObjectRef) -> Option<ObjectView> {
        match object {
            ObjectRef::Vehicle(_) => self.objects.iter().find(|o| o.object == object).cloned(),
            ObjectRef::Player(id) => self.players.iter().find(|p| p.id == id).map(|p| ObjectView {
                object,
                definition: String::new(),
                position: p.position,
                velocity: p.velocity,
                mass: crate::ops::PLAYER_MASS,
                radius: 1.3,
                owner: Some(id),
                package: String::new(),
            }),
            ObjectRef::Entity(id) => self.entities.iter().find(|e| e.id == id).map(|e| ObjectView {
                object,
                definition: e.kind.clone(),
                position: e.position,
                velocity: [0.0; 3],
                mass: crate::ops::PLAYER_MASS,
                radius: 1.3,
                owner: None,
                package: String::new(),
            }),
        }
    }
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
    /// Package-local variables of the package's entities, shared by every
    /// call in a tick; a call's writes come back in its [`Outcome`].
    pub entity_vars: Arc<EntityVars>,
}
/// Each entity's package-local variables.
pub type EntityVars = BTreeMap<u64, BTreeMap<String, serde_json::Value>>;
/// One call's results. Only produced when the call succeeded.
#[derive(Debug)]
pub struct Outcome {
    pub returned: Dynamic,
    pub ops: Vec<Op>,
    pub state: Namespace,
    /// The complete variables of each entity the call wrote to.
    pub entity_vars: EntityVars,
    pub output: Vec<String>,
}

struct Invocation {
    snapshot: Arc<Snapshot>,
    caller: Option<u64>,
    aim: Option<Aim>,
    entity: Option<u64>,
    state: Namespace,
    entity_vars: Arc<EntityVars>,
    /// Entities this call wrote, with all their variables.
    written: EntityVars,
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
fn float_entry(key: &'static str, v: f32) -> (&'static str, Dynamic) {
    (key, Dynamic::from_float(v as f64))
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
        float_entry("ex", p.eye[0]),
        float_entry("ey", p.eye[1]),
        float_entry("ez", p.eye[2]),
        float_entry("lx", p.look[0]),
        float_entry("ly", p.look[1]),
        float_entry("lz", p.look[2]),
        float_entry("vx", p.velocity[0]),
        float_entry("vy", p.velocity[1]),
        float_entry("vz", p.velocity[2]),
        ("item", p.item.clone().into()),
    ])
}
fn object_map(o: &ObjectView) -> Dynamic {
    let [x, y, z] = position(o.position);
    let speed = (o.velocity[0].powi(2) + o.velocity[1].powi(2) + o.velocity[2].powi(2)).sqrt();
    map([
        ("ref", o.object.to_string().into()),
        ("kind", o.object.kind().into()),
        ("id", Dynamic::from_int(o.object.id() as i64)),
        ("definition", o.definition.clone().into()),
        x,
        y,
        z,
        float_entry("vx", o.velocity[0]),
        float_entry("vy", o.velocity[1]),
        float_entry("vz", o.velocity[2]),
        float_entry("speed", speed),
        float_entry("mass", o.mass),
        float_entry("radius", o.radius),
        (
            "owner",
            o.owner
                .map_or(Dynamic::UNIT, |owner| Dynamic::from_int(owner as i64)),
        ),
        ("spawner", o.package.clone().into()),
    ])
}
fn object_ref(value: &Dynamic) -> Fallible<ObjectRef> {
    let text = value.clone().into_string().map_err(|_| {
        format!(
            "expected an object like \"vehicle:3\", got {}",
            value.type_name()
        )
    })?;
    ObjectRef::parse(&text)
        .ok_or_else(|| format!("`{text}` is not an object like \"vehicle:3\"").into())
}
fn credit(value: &Dynamic) -> Fallible<Option<u64>> {
    if value.is_unit() {
        Ok(None)
    } else {
        Ok(Some(id(value)?))
    }
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
                    (
                        "block",
                        a.look
                            .as_ref()
                            .map_or(Dynamic::UNIT, |(b, _)| Dynamic::from(b.clone())),
                    ),
                    (
                        "state",
                        a.look
                            .as_ref()
                            .map_or(Dynamic::UNIT, |(_, s)| Dynamic::from(s.clone())),
                    ),
                    x,
                    y,
                    z,
                    ("distance", Dynamic::from_float(a.distance as f64)),
                    (
                        "object",
                        a.object
                            .map_or(Dynamic::UNIT, |o| Dynamic::from(o.to_string())),
                    ),
                    ("movable", a.movable.into()),
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
            Ok(i.written
                .get(&e)
                .or_else(|| i.entity_vars.get(&e))
                .and_then(|m| m.get(key))
                .map_or(Dynamic::UNIT, to_dynamic))
        })
    });
    engine.register_fn(
        "entity_set",
        |entity: Dynamic, key: &str, value: Dynamic| {
            with(|i| {
                let e = id(&entity)?;
                if !i.written.contains_key(&e) {
                    let Some(vars) = i.entity_vars.get(&e) else {
                        return fail(format!("entity {e} does not belong to this package"));
                    };
                    let vars = vars.clone();
                    i.written.insert(e, vars);
                }
                let v = to_json(&value)?;
                i.written
                    .get_mut(&e)
                    .expect("inserted above")
                    .insert(key.into(), v);
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
        "place_brick",
        |shape: &str, x: Dynamic, y: Dynamic, z: Dynamic, r: Dynamic, g: Dynamic, b: Dynamic| {
            push(Op::PlaceBrick {
                shape: shape.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                color: [float(&r)?, float(&g)?, float(&b)?, 1.0],
            })
        },
    );
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
    engine.register_fn("damage", |player: Dynamic, amount: Dynamic, by: Dynamic| {
        push(Op::DamagePlayer {
            player: id(&player)?,
            amount: float(&amount)?,
            // `()` credits nobody, as `on_death` passes `()` for no killer.
            by: if by.is_unit() { None } else { Some(id(&by)?) },
        })
    });
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
    engine.register_fn("set_archetype", |player: Dynamic, archetype: &str| {
        push(Op::SetArchetype {
            player: id(&player)?,
            archetype: archetype.into(),
        })
    });
    engine.register_fn("set_block_state", |brick: Dynamic, state: &str| {
        push(Op::SetBlockState {
            brick: id(&brick)?,
            state: state.into(),
        })
    });
    engine.register_fn("control", |player: Dynamic, entity: Dynamic| {
        push(Op::Control {
            player: id(&player)?,
            entity: Some(id(&entity)?),
        })
    });
    engine.register_fn("release", |player: Dynamic| {
        push(Op::Control {
            player: id(&player)?,
            entity: None,
        })
    });
    engine.register_fn(
        "spawn_entity",
        |kind: &str, x: Dynamic, y: Dynamic, z: Dynamic| {
            push(Op::SpawnEntity {
                kind: kind.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                vars: BTreeMap::new(),
            })
        },
    );
    engine.register_fn(
        "spawn_entity",
        |kind: &str, x: Dynamic, y: Dynamic, z: Dynamic, vars: Map| {
            let vars = vars
                .into_iter()
                .map(|(k, v)| Ok((k.to_string(), to_json(&v)?)))
                .collect::<Fallible<_>>()?;
            push(Op::SpawnEntity {
                kind: kind.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                vars,
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
    engine.register_fn(
        "copy_build",
        |player: Dynamic, brick: Dynamic, limit: i64, above_only: bool, tool: &str| {
            push(Op::CopyBuild {
                player: id(&player)?,
                brick: id(&brick)?,
                limit: u32::try_from(limit).map_err(|_| "limit must be 1 to 10000")?,
                above_only,
                tool: tool.into(),
            })
        },
    );
    engine.register_fn("give_item", |player: Dynamic, item: &str, equip: bool| {
        push(Op::GiveItem {
            player: id(&player)?,
            item: item.into(),
            equip,
        })
    });
    register_physics(engine);
}

fn push_op(target: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic, by: Dynamic) -> Fallible<()> {
    push(Op::Push {
        target: object_ref(&target)?,
        velocity: [float(&x)?, float(&y)?, float(&z)?],
        by: credit(&by)?,
    })
}
fn tumble_op(player: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic, by: Dynamic) -> Fallible<()> {
    let player = match object_ref(&player) {
        Ok(ObjectRef::Player(p)) => p,
        Ok(other) => return fail(format!("only players tumble, not {other}")),
        Err(_) => id(&player)?,
    };
    push(Op::Tumble {
        player,
        velocity: [float(&x)?, float(&y)?, float(&z)?],
        by: credit(&by)?,
    })
}

/// Movable objects: reading them, and the `physics` operations.
fn register_physics(engine: &mut Engine) {
    engine.register_fn("object", |object: Dynamic| {
        with(|i| {
            let object = object_ref(&object)?;
            Ok(i.snapshot
                .object(object)
                .map_or(Dynamic::UNIT, |o| object_map(&o)))
        })
    });
    engine.register_fn("objects", || {
        with(|i| Ok(i.snapshot.objects.iter().map(object_map).collect::<Array>()))
    });
    engine.register_fn(
        "objects_near",
        |x: Dynamic, y: Dynamic, z: Dynamic, radius: Dynamic| {
            with(|i| {
                let centre = [float(&x)?, float(&y)?, float(&z)?];
                let radius = float(&radius)?;
                let near = |p: [f32; 3]| {
                    (p[0] - centre[0]).powi(2) + (p[1] - centre[1]).powi(2) + (p[2] - centre[2]).powi(2)
                        <= radius * radius
                };
                let players = i
                    .snapshot
                    .players
                    .iter()
                    .filter(|p| p.alive)
                    .map(|p| ObjectRef::Player(p.id));
                let entities = i.snapshot.entities.iter().map(|e| ObjectRef::Entity(e.id));
                let vehicles = i.snapshot.objects.iter().map(|o| o.object);
                Ok(players
                    .chain(entities)
                    .chain(vehicles)
                    .filter_map(|o| i.snapshot.object(o))
                    .filter(|o| near(o.position))
                    .map(|o| object_map(&o))
                    .collect::<Array>())
            })
        },
    );
    engine.register_fn("held", |player: Dynamic| {
        with(|i| {
            let player = id(&player)?;
            Ok(i.snapshot
                .holds
                .iter()
                .find(|h| h.player == player)
                .map_or(Dynamic::UNIT, |h| Dynamic::from(h.object.to_string())))
        })
    });
    engine.register_fn(
        "push",
        |target: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic| {
            push_op(target, x, y, z, Dynamic::UNIT)
        },
    );
    engine.register_fn("push", push_op);
    engine.register_fn(
        "tumble",
        |player: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic| {
            tumble_op(player, x, y, z, Dynamic::UNIT)
        },
    );
    engine.register_fn("tumble", tumble_op);
    engine.register_fn(
        "hold",
        |player: Dynamic, target: Dynamic, distance: Dynamic| {
            push(Op::Hold {
                player: id(&player)?,
                target: object_ref(&target)?,
                distance: float(&distance)?,
            })
        },
    );
    engine.register_fn("let_go", |player: Dynamic| {
        push(Op::LetGo {
            player: id(&player)?,
        })
    });
    engine.register_fn(
        "spawn_vehicle",
        |definition: &str, x: Dynamic, y: Dynamic, z: Dynamic, yaw: Dynamic, velocity: Array, owner: Dynamic| {
            let v = velocity
                .iter()
                .map(float)
                .collect::<Fallible<Vec<f32>>>()?;
            let [vx, vy, vz] = v[..] else {
                return fail("velocity is [x, y, z]");
            };
            push(Op::SpawnVehicle {
                definition: definition.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                yaw: float(&yaw)?,
                velocity: [vx, vy, vz],
                owner: credit(&owner)?,
            })
        },
    );
    engine.register_fn("remove_vehicle", |vehicle: Dynamic| {
        let vehicle = match object_ref(&vehicle) {
            Ok(ObjectRef::Vehicle(v)) => v,
            Ok(other) => return fail(format!("{other} is not a vehicle")),
            Err(_) => id(&vehicle)?,
        };
        push(Op::RemoveVehicle { vehicle })
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
    engine.on_progress(|operations| {
        OPERATIONS.with(|o| o.set(operations));
        None
    });
    register_api(&mut engine);
    engine
}

thread_local! {
    /// Operations the running call has used so far.
    static OPERATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
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
    /// Script operations the last [`call`](Self::call) used, whether it
    /// succeeded or not: what the engine charges to the caller's share.
    pub fn last_operations(&self) -> u64 {
        OPERATIONS.with(std::cell::Cell::get)
    }
    /// Run one function. On error nothing of the call is kept.
    pub fn call(&mut self, package: &str, call: Call<'_>) -> Result<Outcome, Diagnostic> {
        let ast =
            self.scripts.get(package).cloned().ok_or_else(|| {
                Diagnostic::error("script.none", "package has no script").at(package)
            })?;
        self.engine.set_max_operations(call.budget.operations());
        OPERATIONS.with(|o| o.set(0));
        let previous = CURRENT.with(|c| {
            c.borrow_mut().replace(Invocation {
                snapshot: call.snapshot,
                caller: call.caller,
                aim: call.aim,
                entity: call.entity,
                state: call.state,
                entity_vars: call.entity_vars,
                written: BTreeMap::new(),
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
                entity_vars: invocation.written,
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
