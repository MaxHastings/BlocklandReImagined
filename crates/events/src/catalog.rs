use crate::*;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Param {
    Int {
        min: i64,
        max: i64,
        default: i64,
    },
    Float {
        min: f32,
        max: f32,
        step: f32,
        default: f32,
    },
    Bool,
    String {
        max_length: u32,
        width: i32,
    },
    Datablock {
        class_name: String,
    },
    Vector {
        max_length: f32,
    },
    PaintColor {
        default: u8,
    },
    IntList {
        width: i32,
    },
    List {
        items: Vec<(String, i64)>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputDef {
    pub id: String,
    pub class_name: String,
    pub name: String,
    pub targets: Vec<(String, String)>,
    pub source: String,
    pub source_line: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputDef {
    pub id: String,
    pub class_name: String,
    pub name: String,
    pub params: Vec<Param>,
    pub append_client: bool,
    pub source: String,
    pub source_line: u32,
    /// The Add-On whose rules run this output (`registerOutputEvent` in an
    /// Add-On); the engine's own outputs have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}
/// A target an Add-On adds to the inputs (v20's `registerEventTarget`):
/// Slayer's `Team(Client)` stands for the triggering client's team. Every
/// input that has the slot `from` (`Self` for the source brick) lists it
/// as `(name, class_name)`, and a row aimed at it runs the Add-On outputs
/// of `class_name` on that slot's entity, whose rules find what the target
/// stands for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetDef {
    pub id: String,
    pub name: String,
    pub class_name: String,
    pub from: String,
    pub package: String,
    pub source: String,
    pub source_line: u32,
    /// What it stands for, in one line, for the wrench (the Add-On's
    /// `brick_targets` `description`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
/// What the running Add-Ons add to the event catalog: their inputs,
/// targets and outputs. The host sends it to players so their wrench lists
/// them too.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Extension {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<InputDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputDef>,
}
impl Extension {
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty() && self.targets.is_empty() && self.outputs.is_empty()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub schema_version: u32,
    pub inputs: Vec<InputDef>,
    pub outputs: Vec<OutputDef>,
    /// Targets Add-Ons add to the inputs (`with_targets`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetDef>,
    pub sources: Vec<serde_json::Value>,
    pub scope: serde_json::Value,
}
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bindings {
    pub palette_len: usize,
    pub datablocks: BTreeMap<String, BTreeSet<String>>,
}
impl Bindings {
    pub fn contains(&self, class: &str, id: &str) -> bool {
        self.datablocks
            .iter()
            .any(|(c, ids)| c.eq_ignore_ascii_case(class) && ids.contains(id))
    }
}
impl Catalog {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let f = std::fs::File::open(path)?;
        ensure!(f.metadata()?.len() <= 4 << 20, "Event catalog exceeds 4MiB");
        let mut b = Vec::new();
        f.take((4 << 20) + 1).read_to_end(&mut b)?;
        Self::from_json(&b)
    }
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 4 << 20, "Event catalog exceeds 4MiB");
        let c: Self = serde_json::from_slice(bytes)?;
        c.validate()?;
        Ok(c)
    }
    pub fn fingerprint(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(self).expect("validated catalog"))
        )
    }
    /// This catalog with what Add-Ons add: their inputs, then their
    /// targets (listed on every input so far, theirs included), then their
    /// outputs.
    pub fn extended(&self, extra: &Extension) -> Result<Self> {
        self.with_inputs(&extra.inputs)
            .context("The Add-Ons' wrench event inputs")?
            .with_targets(&extra.targets)
            .context("The Add-Ons' wrench event targets")?
            .with_outputs(&extra.outputs)
            .context("The Add-Ons' wrench event outputs")
    }
    /// This catalog with inputs Add-Ons declare (`registerInputEvent`)
    /// added after its own. A name already taken, here or among `extra`,
    /// is refused, as is going over the catalog's limits.
    pub fn with_inputs(&self, extra: &[InputDef]) -> Result<Self> {
        let mut out = self.clone();
        for input in extra {
            ensure!(
                out.input(&input.name).is_none() && out.input(&input.id).is_none(),
                "Event input `{}` is already taken",
                input.name
            );
            out.inputs.push(input.clone());
        }
        out.validate()?;
        Ok(out)
    }
    /// This catalog with outputs Add-Ons declare added after its own, each
    /// marked with its package. A name a class already has is refused.
    pub fn with_outputs(&self, extra: &[OutputDef]) -> Result<Self> {
        let mut out = self.clone();
        for output in extra {
            ensure!(
                output.package.is_some(),
                "Event output `{}` names no package",
                output.name
            );
            ensure!(
                Class::parse(&output.class_name).is_some() || out.target_class(&output.class_name),
                "Unknown event output class `{}`",
                output.class_name
            );
            ensure!(
                !out.outputs.iter().any(|o| {
                    same_class(&o.class_name, &output.class_name)
                        && (o.name.eq_ignore_ascii_case(&output.name) || o.id == output.id)
                }),
                "Event output `{}` is already taken",
                output.name
            );
            out.outputs.push(output.clone());
        }
        out.validate()?;
        Ok(out)
    }
    /// This catalog with targets Add-Ons declare (`registerEventTarget`),
    /// each listed on every input that has its `from` slot and no target
    /// of that name yet, as v20 adds them to the inputs registered so far.
    pub fn with_targets(&self, extra: &[TargetDef]) -> Result<Self> {
        let mut out = self.clone();
        for target in extra {
            ensure!(
                out.target(&target.name).is_none() && Slot::parse(&target.name).is_none(),
                "Event target `{}` is already taken",
                target.name
            );
            for input in &mut out.inputs {
                let has_base = target.from.eq_ignore_ascii_case("Self")
                    || input
                        .targets
                        .iter()
                        .any(|(slot, _)| slot.eq_ignore_ascii_case(&target.from));
                let taken = input
                    .targets
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case(&target.name));
                if has_base && !taken {
                    input
                        .targets
                        .push((target.name.clone(), target.class_name.clone()));
                }
            }
            out.targets.push(target.clone());
        }
        out.validate()?;
        Ok(out)
    }
    /// The Add-On target called `name`.
    pub fn target(&self, name: &str) -> Option<&TargetDef> {
        self.targets
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }
    /// Whether an Add-On target stands for objects of `class_name`.
    fn target_class(&self, class_name: &str) -> bool {
        self.targets
            .iter()
            .any(|t| same_class(&t.class_name, class_name))
    }
    /// The class of the entity a row aimed at `target` from `input` acts
    /// on, and its output called `output`. An Add-On target acts on its
    /// base slot's entity with the Add-On's outputs of the target's class.
    pub fn row_output(
        &self,
        input: &str,
        target: &Target,
        output: &str,
    ) -> Result<(Class, &OutputDef)> {
        let input = self.input(input).context("Unknown event input")?;
        let slot_class = |slot: Slot| {
            input
                .targets
                .iter()
                .find(|(s, _)| Slot::parse(s) == Some(slot))
                .and_then(|(_, class)| Class::parse(class))
                .context("Target unavailable for event input")
        };
        match target {
            Target::Named(_) => Ok((
                Class::Brick,
                self.output(Class::Brick, output)
                    .context("Unknown event output/target class")?,
            )),
            Target::Slot(slot) => {
                let class = slot_class(*slot)?;
                Ok((
                    class,
                    self.output(class, output)
                        .context("Unknown event output/target class")?,
                ))
            }
            Target::Derived(name) => {
                let (_, class_name) = input
                    .targets
                    .iter()
                    .find(|(t, _)| t.eq_ignore_ascii_case(name))
                    .context("Target unavailable for event input")?;
                let def = self.target(name).context("Unknown event target")?;
                let base = Slot::parse(&def.from).context("Unknown event target base")?;
                let class = if base == Slot::SelfBrick {
                    Class::Brick
                } else {
                    slot_class(base)?
                };
                let output = self
                    .outputs
                    .iter()
                    .find(|o| {
                        o.package.is_some()
                            && same_class(&o.class_name, class_name)
                            && (o.name.eq_ignore_ascii_case(output) || o.id == output)
                    })
                    .context("Unknown event output/target class")?;
                Ok((class, output))
            }
        }
    }
    pub fn input(&self, name: &str) -> Option<&InputDef> {
        self.inputs
            .iter()
            .find(|i| i.name.eq_ignore_ascii_case(name) || i.id == name)
    }
    pub fn output(&self, class: Class, name: &str) -> Option<&OutputDef> {
        self.outputs.iter().find(|o| {
            Class::parse(&o.class_name) == Some(class)
                && (o.name.eq_ignore_ascii_case(name) || o.id == name)
        })
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && self.inputs.len() <= 128
                && self.outputs.len() <= 512
                && self.targets.len() <= 32
                && self.sources.len() <= 128,
            "Unsupported event catalog"
        );
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for t in &self.targets {
            let from = Slot::parse(&t.from);
            ensure!(
                ids.insert(&t.id)
                    && names.insert(t.name.to_ascii_lowercase())
                    && !t.id.is_empty()
                    && t.id.len() <= 256
                    && !t.name.is_empty()
                    && t.name.len() <= 64
                    && !t.name.chars().any(|c| c.is_control() || c.is_whitespace())
                    && Slot::parse(&t.name).is_none()
                    && !t.class_name.is_empty()
                    && t.class_name.len() <= 128
                    && !t
                        .class_name
                        .chars()
                        .any(|c| c.is_control() || c.is_whitespace())
                    && Class::parse(&t.class_name).is_none()
                    && from.is_some()
                    && !t.package.is_empty()
                    && t.package.len() <= 128,
                "Invalid event target `{}`",
                t.name
            );
        }
        for i in &self.inputs {
            ensure!(
                ids.insert(&i.id)
                    && !i.id.is_empty()
                    && i.id.len() <= 256
                    && !i.name.is_empty()
                    && i.name.len() <= 128
                    && i.class_name == "fxDTSBrick"
                    && !i.targets.is_empty()
                    && i.targets.len() <= 12,
                "Invalid event input"
            );
            for (t, c) in &i.targets {
                let derived = self.target(t).is_some_and(|d| {
                    same_class(&d.class_name, c)
                        && (Slot::parse(&d.from) == Some(Slot::SelfBrick)
                            || i.targets
                                .iter()
                                .any(|(s, _)| s.eq_ignore_ascii_case(&d.from)))
                });
                ensure!(
                    derived || (Slot::parse(t).is_some() && Class::parse(c).is_some()),
                    "Unknown event target"
                );
            }
        }
        for o in &self.outputs {
            ensure!(
                ids.insert(&o.id)
                    && !o.id.is_empty()
                    && o.id.len() <= 256
                    && !o.name.is_empty()
                    && o.name.len() <= 128
                    && o.params.len() <= 4
                    && (Class::parse(&o.class_name).is_some()
                        || (o.package.is_some() && self.target_class(&o.class_name))),
                "Invalid event output"
            );
            for p in &o.params {
                let valid = match p {
                    Param::Int { min, max, default } => {
                        min <= max
                            && min <= default
                            && default <= max
                            && *min >= -1_000_000
                            && *max <= 1_000_000
                    }
                    Param::Float {
                        min,
                        max,
                        step,
                        default,
                    } => {
                        [min, max, step, default].iter().all(|v| v.is_finite())
                            && min <= default
                            && default <= max
                            && *step > 0.
                            && *step <= 10000.
                    }
                    Param::Vector { max_length } => {
                        max_length.is_finite() && *max_length > 0. && *max_length <= 10000.
                    }
                    Param::String { max_length, .. } => *max_length <= 4096,
                    Param::List { items } => {
                        !items.is_empty()
                            && items.len() <= 256
                            && items.iter().all(|(s, _)| s.len() <= 128)
                    }
                    Param::Datablock { class_name } => {
                        !class_name.is_empty() && class_name.len() <= 128
                    }
                    _ => true,
                };
                ensure!(valid, "Invalid native event parameter schema");
            }
            if let Some(package) = &o.package {
                ensure!(
                    !package.is_empty() && package.len() <= 128,
                    "Invalid event output package"
                );
            }
            let values: Vec<_> = o.params.iter().map(Param::default_value).collect();
            if let Some(class) = Class::parse(&o.class_name) {
                compile(class, o, &values)?;
            }
        }
        Ok(())
    }
    pub fn validate_row(&self, row: &Row, bindings: &Bindings) -> Result<Class> {
        ensure!(
            row.conditions.len() <= crate::rules::MAX_CONDITIONS,
            "Too many rule conditions"
        );
        for condition in &row.conditions {
            condition.validate()?;
        }
        if let Some(p) = &row.preserved {
            ensure!(
                p.original.len() <= 16384
                    && p.diagnostic.len() <= 1024
                    && row.input.is_empty()
                    && row.output.is_empty()
                    && row.params.is_empty()
                    && row.delay_ms == 0
                    && matches!(row.target, Target::Slot(Slot::SelfBrick)),
                "Invalid preserved native row"
            );
            return Ok(Class::Brick);
        }
        ensure!(
            row.delay_ms <= 300_000,
            "Native event delay exceeds 300000ms compatibility bound"
        );
        if let Target::Named(name) = &row.target {
            ensure!(
                !name.is_empty() && name.len() <= 128 && !name.contains(['\0', '\n', '\r']),
                "Invalid named brick target"
            );
        }
        let (class, output) = self.row_output(&row.input, &row.target, &row.output)?;
        ensure!(
            class != Class::Projectile || row.delay_ms > 0 || row.conditions.is_empty(),
            "Immediate projectile reflection actions do not support IF conditions"
        );
        ensure!(
            row.params.len() == output.params.len(),
            "Wrong event parameter count"
        );
        for (s, v) in output.params.iter().zip(&row.params) {
            s.validate_value(v, bindings)?;
        }
        if output.source == "core:rules"
            && matches!(output.name.as_str(), "setVariable" | "addVariable")
            && let Some(Value::Text(key)) = row.params.get(1)
        {
            crate::rules::validate_key(key)?;
        }
        compile(class, output, &row.params)?;
        Ok(class)
    }
}
impl Param {
    pub fn default_value(&self) -> Value {
        match self {
            Self::Int { default, .. } => Value::Int(*default),
            Self::Float { default, .. } => Value::Float(*default),
            Self::Bool => Value::Bool(false),
            Self::String { .. } => Value::Text(String::new()),
            Self::Datablock { .. } => Value::Datablock(None),
            Self::Vector { .. } => Value::Vector(glam::Vec3::ZERO),
            Self::PaintColor { default } => Value::Color(*default),
            Self::IntList { .. } => Value::Rows(RowSelection::All),
            Self::List { items } => Value::Int(items.first().map_or(0, |x| x.1)),
        }
    }
    fn validate_value(&self, v: &Value, b: &Bindings) -> Result<()> {
        let ok = match (self, v) {
            (Self::Int { min, max, .. }, Value::Int(v)) => v >= min && v <= max,
            (Self::Float { min, max, step, .. }, Value::Float(v)) => {
                v.is_finite()
                    && v >= min
                    && v <= max
                    && (((v - min) / step).round() - (v - min) / step).abs() <= 0.001
            }
            (Self::Bool, Value::Bool(_)) => true,
            (Self::String { max_length, .. }, Value::Text(s)) => {
                s.chars().count() <= *max_length as usize && !s.contains('\0')
            }
            (Self::Datablock { class_name }, Value::Datablock(id)) => id
                .as_ref()
                .is_none_or(|id| id.len() <= 256 && b.contains(class_name, id)),
            (Self::Vector { max_length }, Value::Vector(v)) => {
                v.is_finite() && v.length() <= *max_length + 0.0001
            }
            (Self::PaintColor { .. }, Value::Color(v)) => (*v as usize) < b.palette_len,
            (Self::IntList { .. }, Value::Rows(RowSelection::All)) => true,
            (Self::IntList { .. }, Value::Rows(RowSelection::Indices(v))) => {
                v.len() <= 4096 && v.iter().all(|n| *n < 4096)
            }
            (Self::List { items }, Value::Int(v)) => items.iter().any(|(_, n)| n == v),
            _ => false,
        };
        ensure!(ok, "Invalid typed event parameter {v:?}");
        Ok(())
    }
}
/// Whether two Torque class names are the same class (case-insensitive).
fn same_class(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}
fn bad() -> anyhow::Error {
    anyhow::anyhow!("Event implementation parameter mismatch")
}
/// What a row of `output` with parameters `p` does: an Add-On's output is
/// a call of its rules, the engine's its own action.
pub(crate) fn compile(class: Class, output: &OutputDef, p: &[Value]) -> Result<Action> {
    if output.source == "core:rules" && output.package.is_none() {
        return crate::rules::compile(output, p);
    }
    if let Some(package) = &output.package {
        ensure!(p.len() == output.params.len(), bad());
        return Ok(Action::Intent(Intent::Package(PackageCall {
            package: package.clone(),
            output: output.name.clone(),
            params: p.to_vec(),
        })));
    }
    compile_native(class, &output.name, p)
}
fn compile_native(class: Class, name: &str, p: &[Value]) -> Result<Action> {
    // A print count step, saturated rather than wrapped (and negatable).
    let print_step = |v: i64| v.clamp(-i64::from(i8::MAX), i64::from(i8::MAX)) as i8;
    let i = |n| {
        if let Some(Value::Int(v)) = p.get(n) {
            Ok(*v)
        } else {
            Err(bad())
        }
    };
    let f = |n| {
        if let Some(Value::Float(v)) = p.get(n) {
            Ok(*v)
        } else {
            Err(bad())
        }
    };
    let v = |n| {
        if let Some(Value::Vector(v)) = p.get(n) {
            Ok(*v)
        } else {
            Err(bad())
        }
    };
    let b = |n| {
        if let Some(Value::Bool(v)) = p.get(n) {
            Ok(*v)
        } else {
            Err(bad())
        }
    };
    let d = |n| {
        if let Some(Value::Datablock(v)) = p.get(n) {
            Ok(v.clone())
        } else {
            Err(bad())
        }
    };
    let t = |n| {
        if let Some(Value::Text(v)) = p.get(n) {
            Ok(v.clone())
        } else {
            Err(bad())
        }
    };
    let rows = |n| {
        if let Some(Value::Rows(v)) = p.get(n) {
            Ok(v.clone())
        } else {
            Err(bad())
        }
    };
    let name = name.to_ascii_lowercase();
    let op = match class {
        Class::Brick => {
            use BrickOp::*;
            let brick = match name.as_str() {
                "cancelevents" => return Ok(Action::Cancel),
                "firerelay" => return Ok(Action::Relay(None)),
                "firerelayup" => return Ok(Action::Relay(Some(Direction::Up))),
                "firerelaydown" => return Ok(Action::Relay(Some(Direction::Down))),
                "firerelaynorth" => return Ok(Action::Relay(Some(Direction::North))),
                "firerelayeast" => return Ok(Action::Relay(Some(Direction::East))),
                "firerelaysouth" => return Ok(Action::Relay(Some(Direction::South))),
                "firerelaywest" => return Ok(Action::Relay(Some(Direction::West))),
                "seteventenabled" => return Ok(Action::SetEnabled(rows(0)?, b(1)?)),
                "toggleeventenabled" => return Ok(Action::Toggle(rows(0)?)),
                "incrementprintcount" => {
                    return Ok(Action::Print {
                        delta: print_step(i(0)?),
                        set: None,
                    });
                }
                "decrementprintcount" => {
                    return Ok(Action::Print {
                        delta: -print_step(i(0)?),
                        set: None,
                    });
                }
                "setprintcount" => {
                    return Ok(Action::Print {
                        delta: 0,
                        set: Some(i(0)? as u8),
                    });
                }
                "setcolor" => Color(if let Some(Value::Color(v)) = p.first() {
                    *v
                } else {
                    return Err(bad());
                }),
                "setcolorfx" => ColorFx(i(0)? as u8),
                "setshapefx" => ShapeFx(i(0)? as u8),
                "setcolliding" => Colliding(b(0)?),
                "setrendering" => Rendering(b(0)?),
                "setraycasting" => RayCasting(b(0)?),
                "disappear" => Disappear {
                    seconds: i(0)? as i32,
                },
                "fakekillbrick" => FakeKill {
                    velocity: v(0)?,
                    // `mClamp(%time, 0, 300)` (allGameScripts.cs:17459).
                    seconds: i(1)?.clamp(0, 300) as u32,
                },
                "respawn" => Respawn,
                "setemitter" => Emitter(d(0)?),
                "setemitterdirection" => EmitterDirection(Direction::from_index(i(0)?)),
                "setlight" => Light(d(0)?),
                "setitem" => Item(d(0)?),
                "setitemdirection" => ItemDirection(Direction::from_index(i(0)?)),
                "setitemposition" => ItemPosition(Direction::from_index(i(0)?)),
                "setmusic" => Music(d(0)?),
                "playsound" => PlaySound(d(0)?),
                "spawnitem" => SpawnItem {
                    velocity: v(0)?,
                    item: d(1)?,
                },
                "spawnprojectile" => SpawnProjectile {
                    velocity: v(0)?,
                    projectile: d(1)?,
                    variance: v(2)?,
                    scale: f(3)?,
                },
                "spawnexplosion" => SpawnExplosion {
                    projectile: d(0)?,
                    scale: f(1)?,
                },
                "setvehicle" => Vehicle(d(0)?),
                "respawnvehicle" => RespawnVehicle,
                "recovervehicle" => RecoverVehicle,
                "radiusimpulse" => RadiusImpulse {
                    radius: i(0)? as f32,
                    force: i(1)? as f32,
                    vertical_force: i(2)? as f32,
                },
                _ => bail!("Unknown native Brick output {name}"),
            };
            Intent::Brick(brick)
        }
        Class::Player => {
            use PlayerOp::*;
            Intent::Player(match name.as_str() {
                "kill" => Kill,
                "burnplayer" => Burn {
                    seconds: i(0)? as u32,
                },
                "clearburn" => ClearBurn,
                "setvelocity" => SetVelocity(v(0)?),
                "addvelocity" => AddVelocity(v(0)?),
                "setplayerscale" => Scale(f(0)?),
                "addhealth" => AddHealth(i(0)? as i32),
                "sethealth" => SetHealth(i(0)? as u32),
                "changedatablock" => DataBlock(d(0)?),
                "dismount" => Dismount,
                "spawnprojectile" => SpawnProjectile {
                    speed: i(0)? as f32,
                    projectile: d(1)?,
                    variance: v(2)?,
                    scale: f(3)?,
                },
                "spawnexplosion" => SpawnExplosion {
                    projectile: d(0)?,
                    scale: f(1)?,
                },
                "cleartools" => ClearTools,
                "instantrespawn" => InstantRespawn,
                _ => bail!("Unknown native Player output {name}"),
            })
        }
        Class::Client => Intent::Client(match name.as_str() {
            "centerprint" => ClientOp::Message {
                kind: MessageKind::Center,
                text: t(0)?,
                seconds: i(1)? as u32,
            },
            "bottomprint" => ClientOp::Message {
                kind: MessageKind::Bottom,
                text: t(0)?,
                seconds: i(1)? as u32,
            },
            "chatmessage" => ClientOp::Message {
                kind: MessageKind::Chat,
                text: t(0)?,
                seconds: 0,
            },
            "incscore" => ClientOp::IncScore(i(0)?),
            "playsound" => ClientOp::PlaySound(d(0)?),
            _ => bail!("Unknown native Client output {name}"),
        }),
        Class::MiniGame => Intent::MiniGame(match name.as_str() {
            "centerprintall" => MiniGameOp::Message {
                kind: MessageKind::Center,
                text: t(0)?,
                seconds: i(1)? as u32,
            },
            "bottomprintall" => MiniGameOp::Message {
                kind: MessageKind::Bottom,
                text: t(0)?,
                seconds: i(1)? as u32,
            },
            "chatmsgall" => MiniGameOp::Message {
                kind: MessageKind::Chat,
                text: t(0)?,
                seconds: 0,
            },
            "reset" => MiniGameOp::Reset,
            "respawnall" => MiniGameOp::RespawnAll,
            _ => bail!("Unknown native MiniGame output {name}"),
        }),
        Class::Projectile => Intent::Projectile(match name.as_str() {
            "explode" => ProjectileOp::Explode,
            "delete" => ProjectileOp::Delete,
            "bounce" => ProjectileOp::Bounce(f(0)?),
            "redirect" => ProjectileOp::Redirect {
                vector: v(0)?,
                normalized: b(1)?,
            },
            _ => bail!("Unknown native Projectile output {name}"),
        }),
        Class::Vehicle => bail!("No vanilla Vehicle-class output registration"),
    };
    Ok(Action::Intent(op))
}
