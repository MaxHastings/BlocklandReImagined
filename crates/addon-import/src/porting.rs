//! The porting workflow, for a person or the agent they hand it to.
//!
//! `bri-import-addon port ADDON DIR` sets up a work folder: the plain import,
//! the original scripts to read, a drafted port (complete for v20's spread
//! weapons), checks that state what v20 does, stubs quoting every function
//! still to port, and an `AGENT.md` with the instructions. `bri-import-addon
//! check-port DIR` imports the Add-On again with the drafted port, runs the
//! checks and prints the `ports.json` entry to submit. Both run from the
//! release folder; neither needs a checkout. Recipe: `docs/modding/porting.md`.
use crate::ports::{Entry, Ports};
use crate::report::Report;
use crate::{Options, content_id, import_with, namespace_for, source};
use anyhow::{Context, Result, ensure};
use bri_weapons::*;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// v20's common spread `onFire`: `%shellcount` projectiles, each turned by
/// random angles scaled by `%spread`, after a `setVelocity` recoil along the
/// eye vector. The same patterns cover `Weapon_Shotgun` in `ports.json`.
pub const SPREAD_PATTERNS: [(&str, &str); 3] = [
    ("projectiles", r"%shellcount\s*=\s*(\d+)\s*;"),
    ("spread", r"%spread\s*=\s*([0-9]*\.?[0-9]+)\s*;"),
    (
        "recoil",
        r#"setvelocity\s*\(.*geteyevector\s*\(\s*\)\s*,\s*"?\s*-\s*([0-9]*\.?[0-9]+)"#,
    ),
];

/// `work.json`: what `check-port` needs to import the Add-On again.
#[derive(Debug, Serialize, Deserialize)]
pub struct Work {
    pub schema_version: u32,
    pub source: PathBuf,
    pub reference: Option<PathBuf>,
    pub core: Vec<PathBuf>,
}

/// `port/checks.json`: what v20 does, stated from the v20 script.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checks {
    pub schema_version: u32,
    pub checks: Vec<Check>,
}

/// One click of a weapon, aimed straight ahead at rest. Each field left null
/// is not checked.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// The weapon item id in the imported Add-On.
    pub fire: String,
    /// Projectiles one click fires.
    pub projectiles: Option<u32>,
    /// Speed the shooter is pushed back along their aim.
    pub recoil: Option<f32>,
    /// Largest angle any projectile may leave the aim by, in degrees.
    pub max_spread_degrees: Option<f32>,
    /// Where in the v20 script these numbers come from.
    #[serde(default)]
    pub why: String,
}

/// What `port` set up.
#[derive(Debug)]
pub struct Scaffold {
    pub dir: PathBuf,
    pub report: Report,
    /// Functions whose port was drafted completely.
    pub drafted: Vec<String>,
    /// Functions still to port by hand.
    pub to_port: Vec<String>,
    /// The port already in the built-in list, if any.
    pub listed: Option<String>,
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::create_dir_all(path.parent().context("path")?)?;
    std::fs::write(path, bytes).with_context(|| path.display().to_string())
}

fn lines(src: &source::Source, file: &str, from: usize, to: usize) -> String {
    src.get(file)
        .map(|f| {
            String::from_utf8_lossy(&f.bytes)
                .replace('\r', "")
                .lines()
                .skip(from.saturating_sub(1))
                .take(to.saturating_sub(from) + 1)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn captures(body: &str) -> Option<BTreeMap<&'static str, String>> {
    SPREAD_PATTERNS
        .iter()
        .map(|(name, p)| {
            let re = regex::RegexBuilder::new(p)
                .case_insensitive(true)
                .build()
                .expect("static pattern");
            Some((*name, re.captures(body)?[1].to_owned()))
        })
        .collect()
}

/// `bri-import-addon port`: sets up the work folder `dir`.
pub fn scaffold(
    input: &Path,
    dir: &Path,
    reference: Option<PathBuf>,
    core: Vec<PathBuf>,
) -> Result<Scaffold> {
    ensure!(
        !dir.exists(),
        "{} already exists; choose a fresh folder",
        dir.display()
    );
    let input = input
        .canonicalize()
        .with_context(|| format!("{} not found", input.display()))?;
    let reference = reference.map(|r| r.canonicalize()).transpose()?;
    let core = core
        .iter()
        .map(|c| c.canonicalize())
        .collect::<std::io::Result<Vec<_>>>()?;
    let src = source::read(&input)?;
    let imported = dir.join("imported");
    let report = import_with(
        &Options {
            input: input.clone(),
            out: imported.clone(),
            reference: reference.clone(),
            core: core.clone(),
            version: "1.0.0".into(),
        },
        &Ports::empty(),
    )?;
    let ns = namespace_for(&src.name)?;

    // The scripts to read, exactly as the Add-On has them.
    for f in src.files.values() {
        let member = src.member(f);
        let lower = member.to_ascii_lowercase();
        if lower.ends_with(".cs") || lower.ends_with(".txt") {
            write(&dir.join("original").join(member), &f.bytes)?;
        }
    }

    let pack: Option<Value> = std::fs::read(imported.join("assets/weapons.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let items_using = |image: &str| -> Vec<String> {
        pack.as_ref()
            .and_then(|p| p["items"].as_object())
            .map(|items| {
                items
                    .iter()
                    .filter(|(_, it)| it["image"] == image)
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default()
    };

    let mut covers: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut images = serde_json::Map::new();
    let mut checks = Checks {
        schema_version: 1,
        checks: vec![],
    };
    let (mut drafted, mut to_port) = (vec![], vec![]);
    let mut stubs = String::new();
    let _ = writeln!(
        stubs,
        "// Stubs for the script functions of {} that data does not cover yet.\n\
         // Each quotes the v20 function it replaces. Delete a stub once its\n\
         // function is ported (see AGENT.md); none of this file is shipped.\n",
        src.name
    );
    for b in &report.needs_behaviour {
        let body = lines(&src, &b.source.file, b.source.line, b.end_line);
        let is_fire = b.hook.kind == "image_state_script"
            && b.function.to_ascii_lowercase().ends_with("::onfire");
        let image_name = b.hook.target.clone();
        let image = content_id(&ns, "image", &image_name);
        let has_image = pack
            .as_ref()
            .is_some_and(|p| p["images"].get(&image).is_some());
        if is_fire
            && has_image
            && let Some(values) = captures(&body)
        {
            let key = image_name.to_ascii_lowercase();
            let name = |v: &str| format!("{key}.{v}");
            covers.insert(
                b.function.clone(),
                SPREAD_PATTERNS
                    .iter()
                    .map(|(n, p)| (name(n), (*p).to_owned()))
                    .collect(),
            );
            images.insert(
                image.clone(),
                json!({ "shot": {
                    "projectiles": format!("{{{}}}", name("projectiles")),
                    "spread": format!("{{{}}}", name("spread")),
                    "recoil": format!("{{{}}}", name("recoil")),
                }}),
            );
            let spread: f32 = values["spread"].parse().unwrap_or(0.0);
            for item in items_using(&image) {
                checks.checks.push(Check {
                    fire: item,
                    projectiles: values["projectiles"].parse().ok(),
                    recoil: values["recoil"].parse().ok(),
                    max_spread_degrees: Some(
                        (3f32.sqrt() * 5.0 * std::f32::consts::PI * spread).to_degrees() + 0.01,
                    ),
                    why: format!(
                        "{} at {}: %shellcount {}, %spread {} (each axis turns by up to 5π·spread rad), recoil {}",
                        b.function, b.source, values["projectiles"], values["spread"], values["recoil"]
                    ),
                });
            }
            drafted.push(b.function.clone());
            let _ = writeln!(
                stubs,
                "// {} ({}): drafted as the image's shot data in port/port.json.\n",
                b.function, b.source
            );
            continue;
        }
        if is_fire && has_image {
            for item in items_using(&image) {
                checks.checks.push(Check {
                    fire: item,
                    why: format!(
                        "{} at {}: fill in what one click does in v20, read from the script",
                        b.function, b.source
                    ),
                    ..Check::default()
                });
            }
        }
        to_port.push(b.function.clone());
        let _ = writeln!(
            stubs,
            "// ---- {} ({}-{})",
            b.function, b.source, b.end_line
        );
        let _ = writeln!(stubs, "// Hook: {} on {}", b.hook.kind, b.hook.target);
        if let Some(d) = &b.hook.native_default {
            let _ = writeln!(stubs, "// Without a port: {d}");
        }
        if is_fire && has_image {
            let _ = writeln!(
                stubs,
                "// Native form to try first: the image's shot data (projectiles, spread, recoil) in port/port.json"
            );
        } else if let Some(h) = &b.hook.runtime_hook {
            let _ = writeln!(stubs, "// Native hook to use: {h}");
        } else {
            let _ = writeln!(stubs, "// Native hook to use: none yet");
        }
        let ops: Vec<_> = b.operations.iter().map(|o| o.op.as_str()).collect();
        if !ops.is_empty() {
            let _ = writeln!(stubs, "// Does: {}", ops.join(", "));
        }
        if !b.missing_capabilities.is_empty() {
            let _ = writeln!(
                stubs,
                "// Missing from the platform: {}",
                b.missing_capabilities.join(", ")
            );
        }
        for blocker in &b.blockers {
            let _ = writeln!(stubs, "// Blocker: {blocker}");
        }
        let _ = writeln!(stubs, "//\n// v20 source:");
        for l in body.lines() {
            let _ = writeln!(stubs, "//   {l}");
        }
        let stub: String = b
            .function
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let _ = writeln!(
            stubs,
            "fn {stub}() {{\n    // TODO: port, or leave and mark the port partial\n}}\n"
        );
    }

    let mut patch = serde_json::Map::new();
    if !images.is_empty() {
        patch.insert("assets/weapons.json".into(), json!({ "images": images }));
    }
    let port_dir = dir.join("port");
    write(
        &port_dir.join("port.json"),
        &serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "notes": if drafted.is_empty() {
                "TODO: what the port does, in a sentence or two.".to_string()
            } else {
                format!("{} use v20's spread onFire; the images' shot data does the same, with the numbers read from the script.", drafted.join(", "))
            },
            "patch": patch,
        }))?,
    )?;
    write(
        &port_dir.join("checks.json"),
        &serde_json::to_vec_pretty(&checks)?,
    )?;
    let entry = Entry {
        addon: src.name.clone(),
        title: report.source.title.clone(),
        port: ns.clone(),
        status: "partial".into(),
        sha256: vec![src.sha256.clone()],
        covers,
        tests: vec![format!("{ns}/checks.json")],
    };
    write(&dir.join("entry.json"), &serde_json::to_vec_pretty(&entry)?)?;
    write(&dir.join("stubs.rhai"), stubs.as_bytes())?;
    write(
        &dir.join("work.json"),
        &serde_json::to_vec_pretty(&Work {
            schema_version: 1,
            source: input,
            reference,
            core,
        })?,
    )?;
    let listed = Ports::builtin()
        .find(&src.name)
        .map(|e| format!("{} ({})", e.port, e.status));
    write(
        &dir.join("AGENT.md"),
        agent_md(&src.name, &ns, &drafted, &to_port, listed.as_deref()).as_bytes(),
    )?;
    Ok(Scaffold {
        dir: dir.to_path_buf(),
        report,
        drafted,
        to_port,
        listed,
    })
}

fn agent_md(
    addon: &str,
    ns: &str,
    drafted: &[String],
    to_port: &[String],
    listed: Option<&str>,
) -> String {
    let mut m = String::new();
    let _ = writeln!(m, "# Port {addon} to Blockland ReImagined\n");
    if let Some(l) = listed {
        let _ = writeln!(
            m,
            "The game already lists a port of this Add-On: `{l}`. Improve that one rather than starting another.\n"
        );
    }
    let _ = writeln!(
        m,
        "Old Blockland Add-On scripts never run in Blockland ReImagined. Its datablocks already \
         imported as data (`imported/`). This folder is for porting what its scripts did, so every \
         player who imports {addon} gets it.\n"
    );
    let _ = writeln!(m, "## What is here\n");
    let _ = writeln!(
        m,
        "| Path | What it is |\n|---|---|\n\
         | `imported/` | the plain import; `imported/IMPORT-REPORT.md` and `imported/import-report.json` list every script function under needs behaviour |\n\
         | `original/` | the Add-On's own scripts, to read (never submit these) |\n\
         | `port/port.json` | the port: JSON merge patches for files in `imported/` |\n\
         | `port/files/` | files the port adds to the Add-On (create it if you need it) |\n\
         | `port/checks.json` | what v20 does, which `check-port` tests |\n\
         | `entry.json` | this Add-On's line in the ports list: which functions the port covers, and patterns their v20 bodies must match |\n\
         | `stubs.rhai` | one stub per function still to port, quoting its v20 source |\n"
    );
    let _ = writeln!(m, "## Status\n");
    if drafted.is_empty() {
        let _ = writeln!(m, "Nothing was drafted automatically.");
    } else {
        let _ = writeln!(
            m,
            "Drafted completely (v20's spread code, as the image's `shot` data): {}.",
            drafted
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if to_port.is_empty() {
        let _ = writeln!(m, "Nothing is left to port by hand.\n");
    } else {
        let _ = writeln!(
            m,
            "To port by hand: {}.\n",
            to_port
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let _ = writeln!(
        m,
        "## Prompt for your agent\n\n\
         > Port the Blockland Add-On `{addon}` in this folder. For each function in `stubs.rhai`, read its \
         v20 source and decide its native form from the table in AGENT.md. Write the port in `port/`, add each \
         ported function to `covers` in `entry.json` with patterns that match its v20 body and capture every \
         number the port uses, and state what v20 does in `port/checks.json`, from the v20 script and not from \
         your port. Then run `bri-import-addon check-port .` until it passes. Never copy the original Add-On's \
         files into `port/`.\n"
    );
    let _ = writeln!(m, "## Native forms\n");
    let _ = writeln!(
        m,
        "| The v20 function | Port it as |\n|---|---|\n\
         | an image's `onFire` with v20's spread code (`%shellcount`, `%spread`, a `setVelocity` recoil) | the image's `shot` data: `projectiles`, `spread`, `recoil` (drafted automatically) |\n\
         | a fire-rate check on `lastFireTime` and `minShotTime` | nothing: the image's `min_shot_ticks` does it from the datablock |\n\
         | a value a data field already expresses (speed, damage, gravity, reload time, names) | a patch setting that field in `imported/assets/*.json` |\n\
         | a `serverCmd` slash command in an Add-On with no weapons, vehicles or bricks | a rule: `behaviour.json` and a Rhai script under `port/files/`, and a `package.json` patch adding them to `provides` with their `capabilities` |\n\
         | anything whose report entry has no native hook, or needs a missing capability | not portable yet: leave it out of `covers`; the port is partial |\n"
    );
    let _ = writeln!(
        m,
        "## Patches and patterns\n\n\
         `port/port.json` `patch` maps a file in `imported/` (for example `assets/weapons.json`) to an RFC 7396 \
         merge patch. A string that is exactly `{{name}}` becomes the value a pattern named `name` captured, a \
         number when it reads as one. Patterns in `entry.json` are regular expressions, case-insensitive; the \
         first group captures the value. Read every number from the script this way, so the port stays right \
         for other copies of the Add-On.\n"
    );
    let _ = writeln!(
        m,
        "## Check and submit\n\n\
         ```\nbri-import-addon check-port <this folder>\n```\n\n\
         It imports {addon} again with your port, runs `port/checks.json` and lists any function still \
         unported. When the checks pass it prints this Add-On's entry for the ports list and writes it to \
         `submit.json`: `verified` when every function is covered, otherwise `partial`. To submit, add that \
         entry to `crates/addon-import/ports/ports.json` in the Blockland ReImagined repository and copy \
         `port/` to `crates/addon-import/ports/{ns}/`.\n"
    );
    m
}

/// What `check-port` found.
#[derive(Debug)]
pub struct CheckOutcome {
    pub report: Report,
    pub applied: bool,
    pub reason: Option<String>,
    /// One line per check, and whether it passed.
    pub results: Vec<(String, bool)>,
    pub unported: Vec<String>,
    /// The ports.json entry to submit.
    pub entry: Entry,
}

impl CheckOutcome {
    pub fn passed(&self) -> bool {
        self.applied && !self.results.is_empty() && self.results.iter().all(|(_, ok)| *ok)
    }
}

/// `bri-import-addon check-port`: imports again with the port in `dir` and
/// runs its checks.
pub fn check(dir: &Path) -> Result<CheckOutcome> {
    let read = |f: &str| {
        std::fs::read(dir.join(f)).with_context(|| format!("{} is missing", dir.join(f).display()))
    };
    let work: Work = serde_json::from_slice(&read("work.json")?).context("work.json")?;
    let mut entry: Entry = serde_json::from_slice(&read("entry.json")?).context("entry.json")?;
    ensure!(
        !entry.covers.is_empty(),
        "entry.json covers no function yet: add each function the port replaces"
    );
    let ports = Ports::single(entry.clone(), &dir.join("port"))?;
    let out = dir.join("check-output");
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    let report = import_with(
        &Options {
            input: work.source,
            out: out.clone(),
            reference: work.reference,
            core: work.core,
            version: "1.0.0".into(),
        },
        &ports,
    )?;
    let applied = report.ports.first().is_some_and(|p| p.applied);
    let reason = report.ports.first().and_then(|p| p.reason.clone());
    let checks: Checks =
        serde_json::from_slice(&read("port/checks.json")?).context("port/checks.json")?;
    let results = if applied {
        let mut results = vec![match crate::ports::check_pins(&out) {
            Ok(()) => (
                "the Add-On loads: its presentation matches its weapons".to_string(),
                true,
            ),
            Err(e) => (format!("{e:#}"), false),
        }];
        if checks.checks.is_empty() {
            results.push((
                "port/checks.json states nothing v20 does".to_string(),
                false,
            ));
        }
        results.extend(run_checks(&out, &checks)?);
        results
    } else {
        vec![]
    };
    let unported: Vec<String> = report
        .needs_behaviour
        .iter()
        .filter(|b| !b.port.as_ref().is_some_and(|p| p.applied))
        .map(|b| b.function.clone())
        .collect();
    let passed = applied && !results.is_empty() && results.iter().all(|(_, ok)| *ok);
    entry.status = if passed && unported.is_empty() {
        "verified"
    } else {
        "partial"
    }
    .into();
    if !entry.sha256.contains(&report.source.sha256) {
        entry.sha256.push(report.source.sha256.clone());
    }
    let outcome = CheckOutcome {
        report,
        applied,
        reason,
        results,
        unported,
        entry,
    };
    if outcome.passed() {
        std::fs::write(
            dir.join("submit.json"),
            serde_json::to_vec_pretty(&outcome.entry)?,
        )?;
    }
    Ok(outcome)
}

/// Runs each check against the weapons pack in the package `out`.
pub fn run_checks(out: &Path, checks: &Checks) -> Result<Vec<(String, bool)>> {
    let mut results = vec![];
    for c in &checks.checks {
        let (pellets, recoil) = fire_once(out, &c.fire)?;
        let mut line = format!("fire {}: {} projectile(s)", c.fire, pellets.len());
        let mut ok = true;
        if let Some(n) = c.projectiles {
            ok &= pellets.len() == n as usize;
            let _ = write!(line, ", v20 fires {n}");
        }
        if let Some(r) = c.recoil {
            let got = recoil.iter().map(|v| v.length()).sum::<f32>();
            ok &= (got - r).abs() <= 1e-3 * r.max(1.0);
            let _ = write!(line, "; recoil {got}, v20 {r}");
        }
        if let Some(max) = c.max_spread_degrees {
            let widest = pellets
                .iter()
                .map(|v| v.angle_between(Vec3::NEG_Z).to_degrees())
                .fold(0.0f32, f32::max);
            ok &= widest <= max;
            let _ = write!(line, "; widest {widest:.3}°, v20 at most {max:.3}°");
        }
        if c.projectiles.is_none() && c.recoil.is_none() && c.max_spread_degrees.is_none() {
            ok = false;
            line.push_str("; checks nothing: fill in what v20 does");
        }
        results.push((line, ok));
    }
    Ok(results)
}

struct Empty;
impl Query for Empty {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        vec![]
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

/// One click of `item`, aimed down -Z at rest in an empty world: the
/// velocities of the projectiles spawned and of the recoil.
pub fn fire_once(package: &Path, item: &str) -> Result<(Vec<Vec3>, Vec<Vec3>)> {
    let pack = Pack::from_json(
        &std::fs::read(package.join("assets/weapons.json")).context("the Add-On has no weapons")?,
    )?;
    let mut world = WeaponsWorld::new(pack)?;
    world.add_actor(ActorId(1), 5)?;
    let slot = world.give(ActorId(1), item)?;
    world.equip(ActorId(1), Some(slot))?;
    let (mut spawned, mut recoil) = (vec![], vec![]);
    for tick in 0..240 {
        if tick == 60 || tick == 61 {
            world.trigger(ActorId(1), tick == 60)?;
        }
        for e in world.step(&mut Empty) {
            match e {
                Event::Spawned { velocity, .. } => spawned.push(velocity),
                Event::Recoil { velocity, .. } => recoil.push(velocity),
                _ => {}
            }
        }
    }
    Ok((spawned, recoil))
}

/// The built-in shotgun port uses the drafter's spread patterns.
#[cfg(test)]
mod tests {
    #[test]
    fn shotgun_entry_uses_the_spread_patterns() {
        let ports = super::Ports::builtin();
        let e = ports.find("Weapon_Shotgun").unwrap();
        let patterns = &e.covers["shotgunImage::onFire"];
        for (name, p) in super::SPREAD_PATTERNS {
            assert_eq!(patterns[name], p, "{name}");
        }
    }
}
