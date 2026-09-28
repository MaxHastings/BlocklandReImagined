//! `check`: validate a package directory and say how the game reads it.
use crate::archive::{self, Built, Entry, Limits, Side};
use crate::diag::{Diagnostic, Diagnostics};
use crate::manifest::{MANIFEST_FILE, Manifest};
use crate::{capability, kind, slot};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A package directory as the game reads it.
#[derive(Debug, Clone)]
pub struct Package {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub entries: Vec<Entry>,
    pub data: Built,
    pub server: Built,
}

impl Package {
    pub fn entry(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// Read a declared file's bytes.
    pub fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        let entry = self
            .entry(path)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, path.to_string()))?;
        std::fs::read(&entry.source)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Interpretation {
    pub id: String,
    pub version: String,
    pub api: u32,
    pub name: String,
    pub license: String,
    pub provenance: String,
    /// Downloaded by every joining client.
    pub data_archive: Built,
    /// Never leaves the server.
    pub server_archive: Built,
    pub client_files: Vec<String>,
    pub server_files: Vec<String>,
    pub provides: Vec<ProvideView>,
    /// What the server owner is asked to allow, in plain language.
    pub capabilities: Vec<CapabilityView>,
    pub slots: Vec<SlotView>,
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvideView {
    pub id: String,
    pub kind: String,
    pub file: String,
    pub side: Side,
    pub consumer: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityView {
    pub name: String,
    pub meaning: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotView {
    pub slot: String,
    pub mode: crate::manifest::SlotMode,
    pub id: String,
    pub meaning: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub ok: bool,
    pub package_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpretation: Option<Interpretation>,
    pub diagnostics: Diagnostics,
}

/// Load a package directory. Errors are in the diagnostics; the package is
/// returned only when there are none.
pub fn load(dir: &Path, limits: &Limits) -> (Option<Package>, Diagnostics) {
    let mut out = Diagnostics::default();
    if !dir.is_dir() {
        out.push(
            Diagnostic::error("package.not_found", format!("{} is not a directory", dir.display()))
                .hint("pass the package's folder, the one containing package.json"),
        );
        return (None, out);
    }
    let manifest_path = dir.join(MANIFEST_FILE);
    let bytes = match std::fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            out.push(
                Diagnostic::error("manifest.missing", "package.json is missing")
                    .at(MANIFEST_FILE)
                    .hint("create one with `bri-mod new <template> <dir>`"),
            );
            return (None, out);
        }
    };
    let (manifest, diagnostics) = Manifest::parse(&bytes);
    out.extend(diagnostics.0);
    let (entries, diagnostics) = archive::collect(dir, limits);
    out.extend(diagnostics.0);
    let Some(manifest) = manifest else {
        return (None, out);
    };
    for (i, provide) in manifest.provides.iter().enumerate() {
        let at = format!("{MANIFEST_FILE}#/provides/{i}/file");
        let Some(spec) = kind::find(&provide.kind) else { continue };
        let Some(entry) = entries.iter().find(|e| e.path == provide.file) else {
            out.push(
                Diagnostic::error(
                    "provides.file.missing",
                    format!("`{}` declares file `{}`, which is not in the package", provide.id, provide.file),
                )
                .at(at),
            );
            continue;
        };
        if entry.side != spec.side {
            let hint = match spec.side {
                Side::Server => format!("move it under server/ (e.g. server/{})", provide.file),
                Side::Client => "move it out of server/; clients need it".to_string(),
            };
            out.push(
                Diagnostic::error(
                    "provides.file.wrong_side",
                    format!(
                        "{} files must be {}",
                        provide.kind,
                        if spec.side == Side::Server { "server-only (under server/)" } else { "client data" }
                    ),
                )
                .at(&at)
                .hint(hint),
            );
        }
        let ext = archive::extension(&provide.file);
        if !spec.extensions.contains(&ext.as_str()) {
            out.push(
                Diagnostic::error(
                    "provides.file.type",
                    format!("a {} must be one of: {}", provide.kind, spec.extensions.join(", ")),
                )
                .at(at),
            );
        }
    }
    for claim in &manifest.slots {
        if let Some(spec) = slot::find(&claim.slot) {
            if !spec.modes.contains(&claim.mode) {
                out.push(Diagnostic::error(
                    "slots.mode",
                    format!("slot `{}` does not accept mode {:?}", claim.slot, claim.mode),
                ));
            }
            if let Some(provide) = manifest.provides.iter().find(|p| p.id == claim.id)
                && provide.kind != spec.kind
            {
                out.push(Diagnostic::error(
                    "slots.kind",
                    format!("slot `{}` takes a {}, not a {}", claim.slot, spec.kind, provide.kind),
                ));
            }
        }
    }
    let declared: Vec<&str> = manifest.provides.iter().map(|p| p.file.as_str()).collect();
    for entry in entries.iter().filter(|e| e.side == Side::Server) {
        if !declared.contains(&entry.path.as_str()) && archive::extension(&entry.path) == "luau" {
            out.push(
                Diagnostic::warning(
                    "file.unreferenced_behaviour",
                    "this script is not declared in `provides`, so it never runs",
                )
                .at(&entry.path)
                .hint("add {\"kind\": \"behaviour\", \"id\": \"<package>:behaviour/<name>\", \"file\": \"...\"}"),
            );
        }
    }
    if out.has_errors() {
        return (None, out);
    }
    let data = archive::write(&entries, Side::Client, &mut std::io::sink());
    let server = archive::write(&entries, Side::Server, &mut std::io::sink());
    match (data, server) {
        (Ok(data), Ok(server)) => (
            Some(Package {
                dir: dir.to_path_buf(),
                manifest,
                entries,
                data,
                server,
            }),
            out,
        ),
        (Err(error), _) | (_, Err(error)) => {
            out.push(Diagnostic::error("file.unreadable", format!("{error:#}")));
            (None, out)
        }
    }
}

pub fn interpret(package: &Package) -> Interpretation {
    let m = &package.manifest;
    Interpretation {
        id: m.id.clone(),
        version: m.version.clone(),
        api: m.api,
        name: m.name.clone(),
        license: m.license.clone(),
        provenance: m.provenance.source.clone(),
        data_archive: package.data.clone(),
        server_archive: package.server.clone(),
        client_files: package
            .entries
            .iter()
            .filter(|e| e.side == Side::Client)
            .map(|e| e.path.clone())
            .collect(),
        server_files: package
            .entries
            .iter()
            .filter(|e| e.side == Side::Server)
            .map(|e| e.path.clone())
            .collect(),
        provides: m
            .provides
            .iter()
            .map(|p| {
                let spec = kind::find(&p.kind);
                ProvideView {
                    id: p.id.clone(),
                    kind: p.kind.clone(),
                    file: p.file.clone(),
                    side: archive::side_of(&p.file),
                    consumer: spec.map_or("", |s| s.consumer).into(),
                }
            })
            .collect(),
        capabilities: m
            .capabilities
            .iter()
            .map(|c| CapabilityView {
                name: c.clone(),
                meaning: capability::describe(c).unwrap_or("").into(),
            })
            .collect(),
        slots: m
            .slots
            .iter()
            .map(|s| SlotView {
                slot: s.slot.clone(),
                mode: s.mode,
                id: s.id.clone(),
                meaning: slot::find(&s.slot).map_or("", |spec| spec.meaning).into(),
            })
            .collect(),
        dependencies: m.dependencies.clone(),
    }
}

/// Full `check` of one directory.
pub fn check(dir: &Path, limits: &Limits) -> Report {
    let (package, diagnostics) = load(dir, limits);
    Report {
        ok: package.is_some() && !diagnostics.has_errors(),
        package_dir: dir.display().to_string(),
        interpretation: package.as_ref().map(interpret),
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, text: &str) {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }

    const MANIFEST: &str = r#"{
        "schema_version": 1, "id": "greeter", "version": "1.0.0", "api": 1,
        "name": "Greeter", "license": "CC0-1.0",
        "capabilities": ["chat.send"],
        "provides": [
            {"kind": "behaviour", "id": "greeter:behaviour/main", "file": "server/main.luau"},
            {"kind": "asset", "id": "greeter:asset/banner", "file": "assets/banner.png"}
        ]
    }"#;

    #[test]
    fn a_good_package_is_interpreted() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "package.json", MANIFEST);
        write(dir.path(), "server/main.luau", "-- hi");
        write(dir.path(), "assets/banner.png", "png");
        let report = check(dir.path(), &Limits::default());
        assert!(report.ok, "{:?}", report.diagnostics);
        let i = report.interpretation.unwrap();
        assert_eq!(i.client_files, ["assets/banner.png", "package.json"]);
        assert_eq!(i.server_files, ["server/main.luau"]);
        assert_eq!(i.capabilities[0].meaning, "send chat messages to players");
        assert_eq!(i.provides[0].side, Side::Server);
        assert_eq!(i.data_archive.files, 2);
    }

    #[test]
    fn declared_files_must_exist_on_the_right_side() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "package.json",
            &MANIFEST.replace("server/main.luau", "main.luau"),
        );
        write(dir.path(), "main.luau", "--");
        write(dir.path(), "server/stray.luau", "--");
        let report = check(dir.path(), &Limits::default());
        assert!(!report.ok);
        let mut codes = report.diagnostics.codes();
        codes.sort();
        assert_eq!(
            codes,
            [
                "file.code_in_client_data",
                "file.unreferenced_behaviour",
                "provides.file.missing",
                "provides.file.missing",
            ]
        );
    }

    #[test]
    fn missing_manifest_says_how_to_start() {
        let dir = tempfile::tempdir().unwrap();
        let report = check(dir.path(), &Limits::default());
        assert_eq!(report.diagnostics.codes(), ["manifest.missing"]);
        assert!(report.diagnostics.0[0].hint.as_ref().unwrap().contains("bri-mod new"));
    }
}
