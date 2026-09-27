//! Script functions that data cannot express: what each one hooks, which
//! engine operations it performs, and which sandboxed behaviour capabilities
//! a native rewrite would need. This reads calls; it does not run anything.
use crate::report::Location;
use bri_convert::tscript::{Call, Function};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Capabilities the sandboxed package runtime grants today
/// (`bri_package_runtime::ops::CAPABILITIES`). Anything else an operation
/// needs is reported as missing: a platform requirement, not an import failure.
pub use bri_package_runtime::ops::CAPABILITIES as HOST_CAPABILITIES;

#[derive(Debug, Clone, Serialize)]
pub struct NeedsBehaviour {
    /// Suggested id for the native behaviour that replaces this function.
    pub id: String,
    /// `shotgunImage::onFire`.
    pub function: String,
    pub source: Location,
    pub end_line: usize,
    pub hook: Hook,
    pub operations: Vec<Operation>,
    /// Capabilities the host has that a rewrite needs.
    pub capabilities: Vec<String>,
    /// Capabilities or platform primitives the host does not offer yet.
    pub missing_capabilities: Vec<String>,
    /// Calls to functions neither this Add-On nor the engine table defines:
    /// a dependency's framework or engine methods this analysis does not know.
    pub unknown_calls: Vec<String>,
    /// Per-entity script fields it reads or writes (`%obj.lastFireTime`).
    pub entity_state: Vec<String>,
    /// Constructs that cannot be translated mechanically.
    pub blockers: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hook {
    /// `image_state_script`, `datablock_callback`, `framework_callback`,
    /// `global_override`, `console_command` or `helper`.
    pub kind: String,
    /// The datablock, class or function it attaches to.
    pub target: String,
    /// What the native runtime does when this function is absent.
    pub native_default: Option<String>,
    /// The package runtime hook a rewrite can use today, or None when the
    /// runtime has no such hook yet (`crates/package-runtime`).
    pub runtime_hook: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Operation {
    pub op: String,
    pub capability: Option<String>,
    /// `callee@line` evidence.
    pub calls: Vec<String>,
}

/// Engine calls grouped by the native operation they perform. The second
/// field is the capability; `None` means presentation or pure computation.
const OPERATIONS: &[(&str, &str, Option<&str>)] = &[
    (
        "new:projectile",
        "spawn_projectile",
        Some("projectiles.spawn"),
    ),
    ("setvelocity", "set_velocity", Some("players.move")),
    ("addvelocity", "set_velocity", Some("players.move")),
    ("applyimpulse", "apply_impulse", Some("players.move")),
    ("settransform", "teleport", Some("players.move")),
    ("playthread", "play_animation", Some("entities.animate")),
    ("stopthread", "play_animation", Some("entities.animate")),
    ("setdatablock", "change_entity_kind", Some("entities.kind")),
    ("sethealth", "set_health", Some("damage")),
    ("damage", "damage", Some("damage")),
    ("kill", "kill", Some("damage")),
    ("addhealth", "set_health", Some("damage")),
    ("setmeleedamage", "set_melee_damage", Some("damage")),
    (
        "setnodecolor",
        "set_appearance",
        Some("entities.appearance"),
    ),
    (
        "applybodyparts",
        "set_appearance",
        Some("entities.appearance"),
    ),
    (
        "applybodycolors",
        "set_appearance",
        Some("entities.appearance"),
    ),
    (
        "gameconnection::applybodyparts",
        "set_appearance",
        Some("entities.appearance"),
    ),
    (
        "gameconnection::applybodycolors",
        "set_appearance",
        Some("entities.appearance"),
    ),
    ("setweapon", "change_inventory", Some("players.inventory")),
    ("mountimage", "change_inventory", Some("players.inventory")),
    (
        "unmountimage",
        "change_inventory",
        Some("players.inventory"),
    ),
    ("additem", "change_inventory", Some("players.inventory")),
    ("getmuzzlevector", "read_aim", Some("players.read")),
    ("getmuzzlepoint", "read_aim", Some("players.read")),
    ("geteyevector", "read_aim", Some("players.read")),
    ("getposition", "read_position", Some("players.read")),
    ("getvelocity", "read_velocity", Some("players.read")),
    ("gettransform", "read_position", Some("players.read")),
    ("getdamagepercent", "read_health", Some("players.read")),
    ("getstate", "read_health", Some("players.read")),
    (
        "initcontainerradiussearch",
        "query_radius",
        Some("world.read"),
    ),
    ("containersearchnext", "query_radius", Some("world.read")),
    ("containerraycast", "raycast", Some("world.read")),
    ("messageclient", "send_chat", Some("chat")),
    ("messageall", "send_chat", Some("chat")),
    ("commandtoclient", "send_client_command", Some("chat")),
    ("centerprint", "send_chat", Some("chat")),
    ("bottomprint", "send_chat", Some("chat")),
    ("schedule", "schedule", Some("schedule")),
    ("cancel", "schedule", Some("schedule")),
    ("getrandom", "random", Some("random.seeded")),
    ("getsimtime", "read_time", None),
    ("serverplay3d", "play_sound", None),
    ("serverplay2d", "play_sound", None),
    ("playaudio", "play_sound", None),
    ("spawnexplosion", "spawn_explosion", Some("effects.spawn")),
    ("missioncleanup.add", "register_cleanup", None),
    ("parent", "call_original", None),
];

/// Pure helpers with no effect on the world.
const PURE: &[&str] = &[
    "vectoradd",
    "vectorsub",
    "vectorscale",
    "vectordist",
    "vectorlen",
    "vectornormalize",
    "vectordot",
    "vectorcross",
    "matrixcreatefromeuler",
    "matrixmulvector",
    "matrixmulpoint",
    "eulertomatrix",
    "getword",
    "getwords",
    "getwordcount",
    "strlen",
    "strlwr",
    "strupr",
    "getsubstr",
    "strpos",
    "mabs",
    "mfloor",
    "mceil",
    "mclamp",
    "msin",
    "mcos",
    "mpow",
    "msqrt",
    "isobject",
    "getdatablock",
    "getname",
    "getclassname",
    "getid",
    "getmountedimage",
    "echo",
    "error",
    "warn",
    "mfloatlength",
    "getmin",
    "getmax",
    "getcount",
    "getobject",
    "getplayername",
];

fn owned_body(body: &str) -> bool {
    body.lines()
        .map(|l| l.split("//").next().unwrap_or("").trim())
        .any(|l| !l.is_empty())
}

pub struct Context<'a> {
    pub namespace: &'a str,
    pub file: &'a str,
    /// Lower-case datablock name to class, for this Add-On's datablocks.
    pub datablocks: &'a BTreeMap<String, (String, BTreeMap<String, String>)>,
    /// Lower-case names of functions this Add-On defines.
    pub own_functions: &'a BTreeSet<String>,
    /// Lower-case function name to the reference Add-On that defines it.
    pub reference_functions: &'a BTreeMap<String, String>,
}

fn key(call: &Call) -> String {
    if call.receiver.as_deref() == Some("new") {
        let class = call.callee.to_ascii_lowercase();
        return if class.contains("projectile") {
            "new:projectile".into()
        } else {
            format!("new:{class}")
        };
    }
    let callee = call.callee.to_ascii_lowercase();
    if callee.starts_with("parent::") {
        return "parent".into();
    }
    match call.receiver.as_deref().map(str::to_ascii_lowercase) {
        Some(r) if r == "missioncleanup" => "missioncleanup.add".into(),
        _ => callee,
    }
}

fn hook(f: &Function, cx: &Context) -> Hook {
    let target = f.namespace.clone().unwrap_or_default();
    let ns = target.to_ascii_lowercase();
    let name = f.name.to_ascii_lowercase();
    if let Some(package) = &f.package {
        return Hook {
            kind: "global_override".into(),
            target: f.qualified(),
            native_default: Some(format!(
                "the engine's own `{}`; this Add-On replaces it for every object of that class while package `{package}` is active, then calls the original through Parent::",
                f.qualified()
            )),
            runtime_hook: None,
        };
    }
    if f.namespace.is_none() && name.starts_with("servercmd") {
        return Hook {
            kind: "console_command".into(),
            target: f.name.clone(),
            native_default: None,
            runtime_hook: Some(format!(
                "behaviour command `{}` (script `cmd_{}`)",
                &f.name[9..],
                f.name[9..].to_ascii_lowercase()
            )),
        };
    }
    if let Some((class, fields)) = cx.datablocks.get(&ns) {
        let state_script = fields.iter().any(|(k, v)| {
            k.starts_with("statescript[") && crate::literal(v).eq_ignore_ascii_case(&f.name)
        });
        if state_script {
            let default = if name == "onfire" {
                "the weapons runtime spawns the image's projectile once, from data".to_string()
            } else {
                format!(
                    "the weapons runtime runs no script for state script `{}`",
                    f.name
                )
            };
            return Hook {
                kind: "image_state_script".into(),
                target: target.clone(),
                native_default: Some(default),
                runtime_hook: None,
            };
        }
        let framework =
            ["onbotloop", "onbotfollow", "onbotcollision", "onbotdeath"].contains(&name.as_str());
        return Hook {
            kind: if framework {
                "framework_callback"
            } else {
                "datablock_callback"
            }
            .into(),
            target: format!("{target} ({class})"),
            native_default: Some(match name.as_str() {
                "onadd" => "entity spawns with its datablock's data only".into(),
                "ondamage" | "damage" => "default damage handling".into(),
                _ if framework => "none: the Bot_Hole AI framework calls this".into(),
                _ => "none".into(),
            }),
            runtime_hook: framework
                .then(|| "entity `think` for a package-owned entity kind".into()),
        };
    }
    Hook {
        kind: "helper".into(),
        target: f.qualified(),
        native_default: None,
        runtime_hook: Some("a script function the hooks above call".into()),
    }
}

pub fn analyse(f: &Function, cx: &Context) -> Option<NeedsBehaviour> {
    if !owned_body(&f.body) {
        return None;
    }
    let mut ops: BTreeMap<String, Operation> = BTreeMap::new();
    let mut unknown = BTreeSet::new();
    let mut blockers = vec![];
    for call in &f.calls {
        let k = key(call);
        let evidence = format!("{}@{}", call.callee, call.line);
        if let Some((_, op, cap)) = OPERATIONS.iter().find(|(c, _, _)| *c == k) {
            let entry = ops.entry((*op).into()).or_insert_with(|| Operation {
                op: (*op).into(),
                capability: cap.map(str::to_owned),
                calls: vec![],
            });
            entry.calls.push(evidence);
        } else if k == "eval" || k == "call" {
            blockers.push(format!(
                "`{}` at line {} builds code or a call name at run time; rewrite by hand",
                call.callee, call.line
            ));
        } else if k.starts_with("new:") {
            ops.entry(k.clone())
                .or_insert_with(|| Operation {
                    op: format!("create_object:{}", &k[4..]),
                    capability: Some("entity".into()),
                    calls: vec![],
                })
                .calls
                .push(evidence);
        } else if PURE.contains(&k.as_str()) || cx.own_functions.contains(&k) {
            // Pure maths, or another function of this Add-On (analysed on its own).
        } else if let Some(addon) = cx.reference_functions.get(&k) {
            unknown.insert(format!("{} (defined by {addon})", call.callee));
        } else {
            unknown.insert(call.callee.clone());
        }
    }
    let assign = regex::Regex::new(r"%(\w+)\.(\w+)\s*=[^=]").expect("static regex");
    let mut entity_state: BTreeSet<String> = assign
        .captures_iter(&f.body)
        .map(|c| format!("%{}.{}", &c[1], &c[2]))
        .collect();
    let read = regex::Regex::new(r"%obj\.(h\w+|last\w+|is\w+)\b").expect("static regex");
    entity_state.extend(
        read.captures_iter(&f.body)
            .map(|c| format!("%obj.{}", &c[1])),
    );
    let lower = f.body.to_ascii_lowercase();
    if lower.contains("for(") || lower.contains("for (") || lower.contains("while") {
        blockers.push("loops: repeats operations a data default does once".into());
    }
    let capabilities: BTreeSet<String> =
        ops.values().filter_map(|o| o.capability.clone()).collect();
    let (have, missing): (Vec<_>, Vec<_>) = capabilities
        .into_iter()
        .partition(|c| HOST_CAPABILITIES.contains(&c.as_str()));
    let hook = hook(f, cx);
    let mut summary = match hook.kind.as_str() {
        "global_override" => format!("overrides `{}` for every object of that class", hook.target),
        "image_state_script" => format!(
            "runs on image state script `{}` of `{}`",
            f.name, hook.target
        ),
        "framework_callback" => format!(
            "callback of a dependency's AI framework on `{}`",
            hook.target
        ),
        "datablock_callback" => format!("callback on `{}`", hook.target),
        "console_command" => "client-invoked server command".to_string(),
        _ => "helper called by other script".to_string(),
    };
    let names: Vec<_> = ops.keys().map(|o| o.replace('_', " ")).collect();
    if !names.is_empty() {
        summary.push_str(&format!("; does {}", names.join(", ")));
    }
    if !blockers.is_empty() {
        summary.push_str(&format!("; {} blocker(s)", blockers.len()));
    }
    Some(NeedsBehaviour {
        id: format!(
            "{}:behaviour/{}",
            cx.namespace,
            f.qualified().to_ascii_lowercase().replace("::", ".")
        ),
        function: f.qualified(),
        source: Location::new(cx.file, f.line),
        end_line: f.end_line,
        hook,
        operations: ops.into_values().collect(),
        capabilities: have,
        missing_capabilities: missing,
        unknown_calls: unknown.into_iter().collect(),
        entity_state: entity_state.into_iter().collect(),
        blockers,
        summary,
    })
}
