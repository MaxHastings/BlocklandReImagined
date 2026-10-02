//! Experimental native rule vocabulary. The existing bounded scheduler executes
//! these rules; this is an authored format, not a second interpreter.
use crate::*;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_CONDITIONS: usize = 8;

/// Context is resolved by the authoritative host, never by object numbers sent
/// by a player. Instigator is optional and never falls back to the brick owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Subject {
    SelfBrick,
    Target,
    Player,
    Instigator,
    MiniGame,
    Team,
    Object,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Property {
    Exists,
    Alive,
    IsInstigator,
    Score,
    Team,
    RoundOver,
    Color,
    Kind,
    Speed,
    Variable,
    Occupants,
    Opponents,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Compare {
    Equal,
    NotEqual,
    Less,
    AtMost,
    Greater,
    AtLeast,
}
pub const SUBJECTS: &[(&str, Subject)] = &[
    ("Self", Subject::SelfBrick),
    ("Target", Subject::Target),
    ("Player", Subject::Player),
    ("Instigator", Subject::Instigator),
    ("MiniGame", Subject::MiniGame),
    ("Team", Subject::Team),
    ("Object", Subject::Object),
];
pub const PROPERTIES: &[(&str, Property)] = &[
    ("Exists", Property::Exists),
    ("Alive", Property::Alive),
    ("Is Instigator", Property::IsInstigator),
    ("Score", Property::Score),
    ("Team number", Property::Team),
    ("Round ended", Property::RoundOver),
    ("Color", Property::Color),
    ("Object kind", Property::Kind),
    ("Speed", Property::Speed),
    ("Variable", Property::Variable),
    ("Players in region", Property::Occupants),
    ("Opponents in region", Property::Opponents),
];
pub const COMPARISONS: &[(&str, Compare)] = &[
    ("=", Compare::Equal),
    ("!=", Compare::NotEqual),
    ("<", Compare::Less),
    ("<=", Compare::AtMost),
    (">", Compare::Greater),
    (">=", Compare::AtLeast),
];
pub fn default_condition() -> Condition {
    Condition {
        subject: Subject::Instigator,
        property: Property::Exists,
        key: String::new(),
        compare: Compare::Equal,
        value: Datum::Bool(true),
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Datum {
    Number(i64),
    Bool(bool),
    Text(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub subject: Subject,
    pub property: Property,
    /// Only Variable uses a name. Other queries have no arbitrary reflection.
    pub key: String,
    pub compare: Compare,
    pub value: Datum,
}
impl Datum {
    pub fn label(&self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Bool(v) => {
                if *v {
                    "Yes".into()
                } else {
                    "No".into()
                }
            }
            Self::Text(t) => t.clone(),
        }
    }
}
impl Condition {
    pub fn validate(&self) -> Result<()> {
        if self.property == Property::Variable {
            validate_key(&self.key)?;
        } else {
            ensure!(self.key.is_empty(), "Only variables take a key");
        }
        let valid = match self.property {
            Property::Exists | Property::Alive | Property::RoundOver | Property::IsInstigator => {
                matches!(self.value, Datum::Bool(_))
            }
            Property::Kind => matches!(self.value, Datum::Text(_)),
            _ => matches!(self.value, Datum::Number(_)),
        };
        ensure!(valid, "Condition value has the wrong type");
        if !matches!(self.value, Datum::Number(_)) {
            ensure!(
                matches!(self.compare, Compare::Equal | Compare::NotEqual),
                "This type only supports equals or differs"
            );
        }
        if let Datum::Text(text) = &self.value {
            ensure!(text.len() <= 128, "Condition text is too long");
        }
        Ok(())
    }
    pub fn matches(&self, actual: Option<Datum>) -> bool {
        let Some(actual) = actual else { return false };
        let ordering = match (&actual, &self.value) {
            (Datum::Number(a), Datum::Number(b)) => a.cmp(b),
            (Datum::Bool(a), Datum::Bool(b)) => a.cmp(b),
            (Datum::Text(a), Datum::Text(b)) => a.cmp(b),
            _ => return false,
        };
        use std::cmp::Ordering::*;
        match self.compare {
            Compare::Equal => ordering == Equal,
            Compare::NotEqual => ordering != Equal,
            Compare::Less => ordering == Less,
            Compare::AtMost => ordering != Greater,
            Compare::Greater => ordering == Greater,
            Compare::AtLeast => ordering != Less,
        }
    }
}
pub fn validate_key(key: &str) -> Result<()> {
    ensure!(
        !key.is_empty()
            && key.len() <= 48
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "Variable names use 1-48 letters, digits, underscores or dashes"
    );
    Ok(())
}

/// Core actions stay typed and call existing specialized session operations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RuleOp {
    Variable {
        scope: Subject,
        key: String,
        value: i64,
        add: bool,
    },
    AddScore(i32),
    AddTeamScore(i32),
    WinRound,
    EndRound,
    SetTeam(u32),
    ObjectVelocity(glam::Vec3),
    ResetObject,
    RegionSize(glam::Vec3),
    Explain,
}

/// One catalog supplies runtime, editor and documentation, including Add-On
/// outputs supplied by Catalog::extended. These builtins need no private assets.
pub fn workshop_catalog(catalog: &Catalog) -> Result<Catalog> {
    let mut out = catalog.clone();
    let targets = [
        ("Self", "fxDTSBrick"),
        ("Player", "Player"),
        ("Client", "GameConnection"),
        ("MiniGame", "MiniGame"),
        ("Instigator", "Player"),
        ("Object", "Vehicle"),
    ];
    for name in [
        "onRegionEnter",
        "onRegionLeave",
        "onRegionStay",
        "onObjectEnter",
        "onObjectLeave",
        "onObjectStay",
        "onRulePlayerDied",
        "onRulePlayerSpawned",
        "onRuleRoundStart",
        "onRuleRoundEnd",
        "onRuleScoreChanged",
        "onRuleTimer",
        "onRuleVariableChanged",
    ] {
        if out.input(name).is_none() {
            out.inputs.push(InputDef {
                id: format!("core:fact/{name}"),
                class_name: "fxDTSBrick".into(),
                name: name.into(),
                targets: targets
                    .iter()
                    .map(|(a, b)| (a.to_string(), b.to_string()))
                    .collect(),
                source: "core:rules".into(),
                source_line: 0,
            });
        }
    }
    // Instigator is available on existing facts when the host can attribute them.
    for input in &mut out.inputs {
        if !input.targets.iter().any(|(s, _)| s == "Instigator") {
            input.targets.push(("Instigator".into(), "Player".into()));
        }
    }
    let number = || Param::Int {
        min: -1_000_000,
        max: 1_000_000,
        default: 1,
    };
    let key = || Param::String {
        max_length: 48,
        width: 150,
    };
    let scope = || Param::List {
        items: [
            ("Brick", 0),
            ("Player", 1),
            ("MiniGame", 2),
            ("Team", 3),
            ("Object", 4),
        ]
        .iter()
        .map(|(s, n)| (s.to_string(), *n))
        .collect(),
    };
    let vector = || Param::Vector { max_length: 200.0 };
    let mut add =
        |class: &str, name: &str, params: Vec<Param>| {
            if !out.outputs.iter().any(|o| {
                o.class_name.eq_ignore_ascii_case(class) && o.name.eq_ignore_ascii_case(name)
            }) {
                out.outputs.push(OutputDef {
                    id: format!("core:action/{class}/{name}"),
                    class_name: class.into(),
                    name: name.into(),
                    params,
                    append_client: false,
                    source: "core:rules".into(),
                    source_line: 0,
                    package: None,
                });
            }
        };
    add("fxDTSBrick", "setVariable", vec![scope(), key(), number()]);
    add("fxDTSBrick", "addVariable", vec![scope(), key(), number()]);
    add(
        "fxDTSBrick",
        "setRegionSize",
        vec![
            Param::Float {
                min: 0.1,
                max: 100.0,
                step: 0.1,
                default: 4.0
            };
            3
        ],
    );
    add("fxDTSBrick", "explainRules", vec![]);
    for class in ["Player", "GameConnection"] {
        add(class, "addPlayerScore", vec![number()]);
        add(class, "addTeamScore", vec![number()]);
        add(class, "winRound", vec![]);
        add(
            class,
            "setTeam",
            vec![Param::Int {
                min: 1,
                max: 32,
                default: 1,
            }],
        );
    }
    add("MiniGame", "endRound", vec![]);
    add("Vehicle", "setObjectVelocity", vec![vector()]);
    add("Vehicle", "resetObject", vec![]);
    out.validate()?;
    Ok(out)
}
pub(crate) fn compile(output: &OutputDef, p: &[Value]) -> Result<Action> {
    let n = |i| match p.get(i) {
        Some(Value::Int(n)) => Ok(*n),
        _ => anyhow::bail!("Expected number"),
    };
    let v = || match p.first() {
        Some(Value::Vector(v)) => Ok(*v),
        _ => anyhow::bail!("Expected vector"),
    };
    let op = match output.name.as_str() {
        "setVariable" | "addVariable" => {
            let key = match p.get(1) {
                Some(Value::Text(key)) => key.clone(),
                _ => anyhow::bail!("Expected variable name"),
            };
            let scope = match n(0)? {
                0 => Subject::Target,
                1 => Subject::Player,
                2 => Subject::MiniGame,
                3 => Subject::Team,
                4 => Subject::Object,
                _ => anyhow::bail!("Invalid variable scope"),
            };
            RuleOp::Variable {
                scope,
                key,
                value: n(2)?,
                add: output.name == "addVariable",
            }
        }
        "addPlayerScore" => RuleOp::AddScore(i32::try_from(n(0)?)?),
        "addTeamScore" => RuleOp::AddTeamScore(i32::try_from(n(0)?)?),
        "winRound" => RuleOp::WinRound,
        "endRound" => RuleOp::EndRound,
        "setTeam" => RuleOp::SetTeam(u32::try_from(n(0)?)?),
        "setObjectVelocity" => RuleOp::ObjectVelocity(v()?),
        "resetObject" => RuleOp::ResetObject,
        "setRegionSize" => {
            let f = |i| match p.get(i) {
                Some(Value::Float(v)) => Ok(*v),
                _ => anyhow::bail!("Expected region dimension"),
            };
            let size = glam::Vec3::new(f(0)?, f(1)?, f(2)?);
            ensure!(
                size.min_element() > 0.0 && size.max_element() <= 100.0,
                "Region dimensions must be greater than zero and at most 100"
            );
            RuleOp::RegionSize(size)
        }
        "explainRules" => RuleOp::Explain,
        _ => anyhow::bail!("Unknown core rule action"),
    };
    Ok(Action::Intent(Intent::Rule(op)))
}

/// Help for the core primitives; provider metadata and parameter types come
/// from the same authoritative catalog the runtime validates.
pub fn action_hint(name: &str) -> &'static str {
    match name {
        "setVariable" | "addVariable" => {
            "Scope, name, integer. Brick means the output target. Default 0; reset each round."
        }
        "setRegionSize" => {
            "Width, height, depth in world units. Dimensions are saved with this brick."
        }
        "addPlayerScore" => "Add points to this player in the rule owner's mini-game.",
        "addTeamScore" => {
            "Add real points to this player. IF Team Score sums current team members."
        }
        "winRound" => "End the round with this player (and their team) as winner.",
        "endRound" => "End the round without a winner. MiniGame Reset starts another round.",
        "setTeam" => "Move this player to the numbered team. Configure teams in MiniGame.",
        "resetObject" => "Respawn this object at its spawn brick. Clears prior attribution.",
        "setObjectVelocity" => "Set the object's velocity using the familiar X/Y/Z vector fields.",
        "explainRules" => {
            "Start tracing this brick; repeat to see the latest conditions and actions."
        }
        _ => "",
    }
}
