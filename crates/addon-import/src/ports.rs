//! Native ports of v20 Add-On scripts. `ports/ports.json` lists each v20
//! Add-On (by folder name and content hash) that has a port, with its status;
//! `ports/<port>/` holds the port: JSON merge patches for the files the
//! importer writes, and any files it adds. A port carries only the native
//! rewrite, never the original Add-On's files, so the list ships with the
//! game and each player's own copy supplies the rest. Recipe:
//! `docs/modding/porting.md`.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

mod builtin {
    include!(concat!(env!("OUT_DIR"), "/ports.rs"));
}

pub const LIST_SCHEMA: u32 = 1;
const STATUSES: &[&str] = &["verified", "partial"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct List {
    pub schema_version: u32,
    pub ports: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The v20 Add-On's folder or zip name (`Weapon_Shotgun`).
    pub addon: String,
    pub title: String,
    /// The folder under `ports/` holding the port.
    pub port: String,
    /// `verified`: its tests show it behaves like v20 for everything it
    /// covers. `partial`: some of the Add-On's behaviour is still missing.
    pub status: String,
    /// Source hashes (`import-report.json` `source.sha256`) of copies the
    /// port was checked against. Other copies still get the port when every
    /// function it covers matches.
    #[serde(default)]
    pub sha256: Vec<String>,
    /// Each script function the port replaces, with named patterns its body
    /// must match (case-insensitive). A pattern's first group, when it has
    /// one, is the value the port's patches use as `{name}`.
    pub covers: BTreeMap<String, BTreeMap<String, String>>,
    /// Tests that prove the port, `path name`.
    #[serde(default)]
    pub tests: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub schema_version: u32,
    #[serde(default)]
    pub notes: String,
    /// Package-relative JSON file to an RFC 7396 merge patch. A string value
    /// that is exactly `{name}` becomes that captured value (a number when it
    /// reads as one); `{name}` inside a longer string becomes its text.
    #[serde(default)]
    pub patch: BTreeMap<String, Value>,
}

/// The ports list and every port's files.
#[derive(Debug, Clone)]
pub struct Ports {
    pub list: List,
    /// `port/relative/path` to bytes.
    files: BTreeMap<String, Vec<u8>>,
}

impl Ports {
    /// The ports this importer was built with.
    pub fn builtin() -> Self {
        Self::from_files(
            builtin::FILES
                .iter()
                .map(|(p, b)| ((*p).to_owned(), b.to_vec()))
                .collect(),
        )
        .expect("crates/addon-import/ports is valid; tests/ports.rs checks it")
    }

    /// No ports: the plain import.
    pub fn empty() -> Self {
        Self {
            list: List {
                schema_version: LIST_SCHEMA,
                ports: vec![],
            },
            files: BTreeMap::new(),
        }
    }

    /// One port being written: its list entry and its folder.
    pub fn single(entry: Entry, port_dir: &Path) -> Result<Self> {
        let mut files = BTreeMap::new();
        let list = List {
            schema_version: LIST_SCHEMA,
            ports: vec![entry],
        };
        files.insert("ports.json".to_owned(), serde_json::to_vec(&list)?);
        let port = &list.ports[0].port;
        let mut stack = vec![port_dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)
                .with_context(|| format!("reading {}", d.display()))?
                .flatten()
            {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path
                        .strip_prefix(port_dir)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.insert(format!("{port}/{rel}"), std::fs::read(&path)?);
                }
            }
        }
        Self::from_files(files)
    }

    /// A ports folder laid out like `crates/addon-import/ports`.
    pub fn from_dir(root: &Path) -> Result<Self> {
        let mut files = BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)?.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path
                        .strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.insert(rel, std::fs::read(&path)?);
                }
            }
        }
        Self::from_files(files)
    }

    fn from_files(mut files: BTreeMap<String, Vec<u8>>) -> Result<Self> {
        let list: List = serde_json::from_slice(
            &files
                .remove("ports.json")
                .context("ports.json is missing")?,
        )
        .context("ports.json")?;
        ensure!(
            list.schema_version == LIST_SCHEMA,
            "ports.json schema_version"
        );
        let ports = Self { list, files };
        for e in &ports.list.ports {
            ensure!(
                STATUSES.contains(&e.status.as_str()),
                "{}: status must be verified or partial",
                e.addon
            );
            ensure!(!e.covers.is_empty(), "{}: covers nothing", e.addon);
            for h in &e.sha256 {
                ensure!(
                    h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()),
                    "{}: bad sha256 {h}",
                    e.addon
                );
            }
            for patterns in e.covers.values() {
                for p in patterns.values() {
                    pattern(p).with_context(|| e.addon.clone())?;
                }
            }
            ports.port(e).with_context(|| format!("port {}", e.port))?;
        }
        Ok(ports)
    }

    fn port(&self, e: &Entry) -> Result<Port> {
        let bytes = self
            .files
            .get(&format!("{}/port.json", e.port))
            .with_context(|| format!("{}/port.json is missing", e.port))?;
        let port: Port = serde_json::from_slice(bytes)?;
        ensure!(port.schema_version == 1, "port.json schema_version");
        for file in port.patch.keys() {
            safe_relative(file)?;
            ensure!(
                file.ends_with(".json"),
                "{file}: only JSON files are patched"
            );
        }
        Ok(port)
    }

    /// Files a port adds, under `ports/<port>/files/`, by package-relative path.
    fn added_files(&self, port: &str) -> Vec<(String, &[u8])> {
        let prefix = format!("{port}/files/");
        self.files
            .iter()
            .filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?.to_owned(), v.as_slice())))
            .collect()
    }

    pub fn find(&self, addon: &str) -> Option<&Entry> {
        self.list
            .ports
            .iter()
            .find(|e| e.addon.eq_ignore_ascii_case(addon))
    }
}

fn pattern(p: &str) -> Result<regex::Regex> {
    let re = regex::RegexBuilder::new(p)
        .case_insensitive(true)
        .size_limit(1 << 20)
        .build()?;
    Ok(re)
}

fn safe_relative(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && !path.contains(':')
            && path
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != ".."),
        "{path}: not a plain package-relative path"
    );
    Ok(())
}

/// What the importer did with the port listed for this Add-On.
#[derive(Debug, Clone, Serialize)]
pub struct Applied {
    pub addon: String,
    pub port: String,
    pub status: String,
    pub applied: bool,
    /// `listed` when this copy's hash is in the list, otherwise `unlisted`.
    pub copy: String,
    pub covers: Vec<String>,
    /// Values read from this copy's script, by pattern name.
    pub values: BTreeMap<String, String>,
    pub files_changed: Vec<String>,
    /// Why it was not applied.
    pub reason: Option<String>,
    pub notes: String,
}

/// Script function bodies by lower-case qualified name.
pub type Bodies = BTreeMap<String, String>;

/// Applies the listed port for `addon`, if any, to the package in `out`.
/// All or nothing: a port that does not fit this copy changes no file.
pub fn apply(
    ports: &Ports,
    addon: &str,
    sha256: &str,
    bodies: &Bodies,
    out: &Path,
) -> Option<Applied> {
    let e = ports.find(addon)?;
    let mut applied = Applied {
        addon: e.addon.clone(),
        port: e.port.clone(),
        status: e.status.clone(),
        applied: false,
        copy: if e.sha256.iter().any(|h| h.eq_ignore_ascii_case(sha256)) {
            "listed"
        } else {
            "unlisted"
        }
        .into(),
        covers: e.covers.keys().cloned().collect(),
        values: BTreeMap::new(),
        files_changed: vec![],
        reason: None,
        notes: String::new(),
    };
    match try_apply(ports, e, bodies, out, &mut applied) {
        Ok(()) => applied.applied = true,
        Err(err) => applied.reason = Some(format!("{err:#}")),
    }
    Some(applied)
}

fn try_apply(
    ports: &Ports,
    e: &Entry,
    bodies: &Bodies,
    out: &Path,
    applied: &mut Applied,
) -> Result<()> {
    for (function, patterns) in &e.covers {
        let Some(body) = bodies.get(&function.to_ascii_lowercase()) else {
            bail!("this copy has no `{function}`");
        };
        for (name, p) in patterns {
            let caps = pattern(p)?
                .captures(body)
                .with_context(|| format!("`{function}` does not match the port's `{name}`"))?;
            if let Some(value) = caps.get(1) {
                applied
                    .values
                    .insert(name.clone(), value.as_str().to_owned());
            }
        }
    }
    let port = ports.port(e)?;
    applied.notes = port.notes.clone();
    let mut writes: Vec<(String, Vec<u8>)> = vec![];
    for (file, patch) in &port.patch {
        let path = out.join(file);
        let mut doc: Value = serde_json::from_slice(
            &std::fs::read(&path).with_context(|| format!("the import wrote no {file}"))?,
        )
        .with_context(|| file.clone())?;
        merge(&mut doc, &fill(patch, &applied.values)?);
        let bytes = serde_json::to_vec_pretty(&doc)?;
        if file == "assets/weapons.json" {
            bri_weapons::Pack::from_json(&bytes).context("the patched weapons.json")?;
        }
        writes.push((file.clone(), bytes));
    }
    for (file, bytes) in ports.added_files(&e.port) {
        safe_relative(&file)?;
        ensure!(
            !out.join(&file).exists(),
            "{file} would replace an imported file"
        );
        writes.push((file, bytes.to_vec()));
    }
    repin(out, &mut writes)?;
    for (file, bytes) in writes {
        let path = out.join(&file);
        std::fs::create_dir_all(path.parent().context("output path")?)?;
        std::fs::write(path, bytes)?;
        applied.files_changed.push(file);
    }
    Ok(())
}

/// The item presentation the importer writes pins the exact bytes of the
/// weapons pack and item physics beside it (hosts and players refuse a
/// mismatch). After a port changes either, pin the new bytes.
const PRESENTATION: &str = "assets/presentation.json";
const PINNED: [(&str, &str); 2] = [
    ("assets/weapons.json", "weapons_sha256"),
    ("assets/item-physics.json", "item_physics_sha256"),
];

fn repin(out: &Path, writes: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    let changed: Vec<(&str, String)> = PINNED
        .iter()
        .filter_map(|(file, field)| {
            let (_, bytes) = writes.iter().find(|(f, _)| f == file)?;
            Some((*field, crate::source::hash(bytes)))
        })
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    let at = writes.iter().position(|(f, _)| f == PRESENTATION);
    let bytes = match at {
        Some(i) => writes[i].1.clone(),
        None => match std::fs::read(out.join(PRESENTATION)) {
            Ok(b) => b,
            // No presentation, nothing pinned.
            Err(_) => return Ok(()),
        },
    };
    let mut doc: Value = serde_json::from_slice(&bytes).context(PRESENTATION)?;
    for (field, sha) in changed {
        doc[field] = Value::String(sha);
    }
    let bytes = serde_json::to_vec_pretty(&doc)?;
    match at {
        Some(i) => writes[i].1 = bytes,
        None => writes.push((PRESENTATION.into(), bytes)),
    }
    Ok(())
}

/// Checks that the presentation in the package `out` pins the weapons pack
/// and item physics beside it, as hosts and players do when they load it.
pub fn check_pins(out: &Path) -> Result<()> {
    let Ok(bytes) = std::fs::read(out.join(PRESENTATION)) else {
        return Ok(());
    };
    let doc: Value = serde_json::from_slice(&bytes).context(PRESENTATION)?;
    for (file, field) in PINNED {
        let sha =
            crate::source::hash(&std::fs::read(out.join(file)).with_context(|| file.to_owned())?);
        ensure!(
            doc[field].as_str() == Some(sha.as_str()),
            "{PRESENTATION} does not match {file}; the game would refuse to load this Add-On"
        );
    }
    Ok(())
}

/// Replaces every string that is exactly `{name}` with that value, as a
/// number when it reads as one, and `{name}` inside longer strings with its
/// text.
fn fill(v: &Value, values: &BTreeMap<String, String>) -> Result<Value> {
    Ok(match v {
        Value::String(s) if s.starts_with('{') && s.ends_with('}') => {
            let name = &s[1..s.len() - 1];
            let value = values
                .get(name)
                .with_context(|| format!("the patch uses `{s}`, which no pattern captures"))?;
            if let Ok(n) = value.parse::<i64>() {
                serde_json::json!(n)
            } else {
                match value.parse::<f64>() {
                    Ok(n) if n.is_finite() => serde_json::json!(n),
                    _ => Value::String(value.clone()),
                }
            }
        }
        Value::String(s) => {
            let mut s = s.clone();
            for (name, value) in values {
                s = s.replace(&format!("{{{name}}}"), value);
            }
            Value::String(s)
        }
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| Ok((k.clone(), fill(v, values)?)))
                .collect::<Result<_>>()?,
        ),
        Value::Array(a) => Value::Array(a.iter().map(|v| fill(v, values)).collect::<Result<_>>()?),
        other => other.clone(),
    })
}

/// RFC 7396 JSON merge patch.
fn merge(target: &mut Value, patch: &Value) {
    let Value::Object(p) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Default::default());
    }
    let t = target.as_object_mut().expect("object");
    for (k, v) in p {
        if v.is_null() {
            t.remove(k);
        } else {
            merge(t.entry(k.clone()).or_insert(Value::Null), v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_patch_follows_rfc_7396() {
        let mut doc = json!({"a": {"b": 1, "c": 2}, "d": [1, 2]});
        merge(&mut doc, &json!({"a": {"b": null, "e": 3}, "d": [3]}));
        assert_eq!(doc, json!({"a": {"c": 2, "e": 3}, "d": [3]}));
    }

    #[test]
    fn fill_reads_numbers_and_refuses_unknown_names() {
        let values = BTreeMap::from([("n".to_string(), "3".to_string())]);
        assert_eq!(
            fill(&json!({"x": "{n}"}), &values).unwrap(),
            json!({"x": 3})
        );
        assert!(fill(&json!("{missing}"), &values).is_err());
    }
}
