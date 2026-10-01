//! The import report: what converted, what was recognised, what could not be,
//! and what an agent must still build natively. `import-report.json` is the
//! machine-readable form; `IMPORT-REPORT.md` a short human summary of it.
use serde::Serialize;
use std::fmt::Write;

pub const REPORT_SCHEMA: u32 = 2;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub source: SourceInfo,
    pub package: PackageInfo,
    pub summary: Summary,
    pub assets: Vec<AssetEntry>,
    pub datablocks: Vec<DatablockEntry>,
    pub ids: Vec<IdEntry>,
    pub dependencies: Vec<Dependency>,
    pub unsupported: Vec<Finding>,
    /// What the importer could not convert but the applied port carries
    /// out, each with how (`resolution`): a top-level call a port's table
    /// reads.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ported: Vec<Finding>,
    pub ambiguous: Vec<Finding>,
    pub needs_behaviour: Vec<crate::behaviour::NeedsBehaviour>,
    /// The listed native port for this Add-On (`crates/addon-import/ports`),
    /// whether it was applied, and why not.
    pub ports: Vec<crate::ports::Applied>,
    /// The companion host Add-On holding the import's host-only content
    /// (`ports::Host`), when no port's rules carried it.
    pub host: Option<crate::ports::RulesPackage>,
    /// Structure the readers skipped or repaired, per file.
    pub diagnostics: Vec<String>,
}

/// A native port that covers a script function.
#[derive(Debug, Clone, Serialize)]
pub struct PortRef {
    pub port: String,
    /// `verified` or `partial`.
    pub status: String,
    /// False when this copy's script does not match the port; the report's
    /// `ports` entry says why.
    pub applied: bool,
    /// How the game carries it out now: what the port read of it, or its
    /// declaration ([`crate::ports::Port::handles`]). Empty when the port
    /// only checks it (`covers`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub how: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SourceInfo {
    /// The Add-On's folder name, which Blockland uses as its identity.
    pub name: String,
    pub path: String,
    /// SHA-256 of the zip, or of the sorted member list and hashes for a folder.
    pub sha256: String,
    pub format: String,
    pub title: String,
    pub authors: Vec<String>,
    pub description: String,
    /// `rtbInfo.txt` key/values (Return to Blockland listing metadata).
    pub listing: std::collections::BTreeMap<String, String>,
    /// `known` when a licence file was found, otherwise `unknown`.
    pub licence_status: String,
    pub licence_files: Vec<String>,
    /// An SPDX id named in a licence file, when one is.
    pub licence_spdx: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PackageInfo {
    pub id: String,
    pub namespace: String,
    pub version: String,
    pub dir: String,
    /// The line to add to `packages.json` to load this package.
    pub packages_json_entry: serde_json::Value,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub files: usize,
    pub assets_converted: usize,
    pub assets_failed: usize,
    pub scripts: usize,
    pub datablocks: usize,
    pub datablocks_converted: usize,
    pub datablocks_recognised_only: usize,
    pub datablocks_unsupported: usize,
    pub ids_assigned: usize,
    pub dependencies: usize,
    pub dependencies_missing: usize,
    pub unsupported: usize,
    pub ambiguous: usize,
    pub needs_behaviour: usize,
    /// Of those, the ones a listed port covers and the import applied.
    pub needs_behaviour_ported: usize,
    /// What the importer could not convert that the port carries out
    /// ([`Report::ported`]).
    pub ported: usize,
    /// `converted`, `converted_with_gaps` or `recognised_only`.
    pub verdict: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetEntry {
    pub source: String,
    pub sha256: String,
    pub kind: String,
    /// `converted`, `copied`, `consumed` (read as metadata or script),
    /// `failed` or `unsupported`.
    pub status: String,
    /// Package-relative output file.
    pub output: Option<String>,
    pub id: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatablockEntry {
    pub name: String,
    pub class: String,
    pub parent: Option<String>,
    pub source: Location,
    /// Native concept it maps to (`weapon`, `projectile`, `vehicle`, ...).
    pub recognised_as: String,
    /// `converted`, `converted_with_gaps`, `consumed` (folded into another
    /// definition), `recognised_only` or `unsupported`.
    pub status: String,
    pub ids: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdEntry {
    pub id: String,
    pub kind: String,
    pub from: String,
    pub file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    /// The Add-On name the script asks for (`Weapon_Gun`).
    pub addon: String,
    /// `ForceRequiredAddOn`, `LoadRequiredAddOn`, or `reference` when only a
    /// datablock or file of another Add-On is used.
    pub how: String,
    pub source: Option<Location>,
    /// `reference` (found in the v20 reference install), `missing`.
    pub status: String,
    /// The package that provides it, when known (`v20-weapons`).
    pub package: Option<String>,
    /// Datablocks and files of the dependency this Add-On uses.
    pub uses: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub what: String,
    pub source: Option<Location>,
    pub detail: String,
    /// Candidates or the chosen interpretation, when there is one.
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    pub file: String,
    pub line: usize,
}

impl Location {
    pub fn new(file: &str, line: usize) -> Self {
        Self {
            file: file.into(),
            line,
        }
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

impl Report {
    pub fn summarise(&mut self) {
        let status = |s: &str| self.datablocks.iter().filter(|d| d.status == s).count();
        let s = Summary {
            files: self.assets.len(),
            assets_converted: self
                .assets
                .iter()
                .filter(|a| matches!(a.status.as_str(), "converted" | "copied"))
                .count(),
            assets_failed: self.assets.iter().filter(|a| a.status == "failed").count(),
            scripts: self.assets.iter().filter(|a| a.kind == "script").count(),
            datablocks: self.datablocks.len(),
            datablocks_converted: status("converted")
                + status("converted_with_gaps")
                + status("consumed"),
            datablocks_recognised_only: status("recognised_only"),
            datablocks_unsupported: status("unsupported"),
            ids_assigned: self.ids.len(),
            dependencies: self.dependencies.len(),
            dependencies_missing: self
                .dependencies
                .iter()
                .filter(|d| d.status == "missing")
                .count(),
            unsupported: self.unsupported.len(),
            ambiguous: self.ambiguous.len(),
            needs_behaviour: self.needs_behaviour.len(),
            needs_behaviour_ported: self
                .needs_behaviour
                .iter()
                .filter(|b| b.port.as_ref().is_some_and(|p| p.applied))
                .count(),
            ported: self.ported.len(),
            verdict: String::new(),
        };
        let verdict = if s.datablocks_converted == 0 {
            "recognised_only"
        } else if s.needs_behaviour - s.needs_behaviour_ported
            + s.unsupported
            + s.dependencies_missing
            + s.datablocks_recognised_only
            == 0
        {
            "converted"
        } else {
            "converted_with_gaps"
        };
        self.summary = Summary {
            verdict: verdict.into(),
            ..s
        };
    }

    /// A short human summary. The JSON report is the complete record.
    pub fn markdown(&self) -> String {
        let mut m = String::new();
        let s = &self.summary;
        let src = &self.source;
        let _ = writeln!(m, "# Import report: {}\n", src.name);
        let _ = writeln!(
            m,
            "{} by {}. Licence: {}. Package `{}` {}.\n",
            if src.title.is_empty() {
                &src.name
            } else {
                &src.title
            },
            if src.authors.is_empty() {
                "unknown author".to_string()
            } else {
                src.authors.join(", ")
            },
            src.licence_status,
            self.package.id,
            self.package.version
        );
        let _ = writeln!(m, "**Verdict: {}.**\n", s.verdict.replace('_', " "));
        let _ = writeln!(m, "| | Count |\n|---|---|");
        for (label, n) in [
            ("Files", s.files),
            ("Assets converted or copied", s.assets_converted),
            ("Assets failed", s.assets_failed),
            ("Datablocks", s.datablocks),
            ("Datablocks converted", s.datablocks_converted),
            ("Datablocks recognised only", s.datablocks_recognised_only),
            ("Datablocks unsupported", s.datablocks_unsupported),
            ("Ids assigned", s.ids_assigned),
            ("Dependencies (missing)", s.dependencies),
            ("Unsupported", s.unsupported),
            ("Carried out by the port", s.ported),
            ("Ambiguous", s.ambiguous),
            ("Needs behaviour", s.needs_behaviour),
            ("Needs behaviour, ported", s.needs_behaviour_ported),
        ] {
            let n = if label.starts_with("Dependencies") {
                format!("{n} ({})", s.dependencies_missing)
            } else {
                n.to_string()
            };
            let _ = writeln!(m, "| {label} | {n} |");
        }
        if !self.dependencies.is_empty() {
            let _ = writeln!(m, "\n## Dependencies\n");
            for d in &self.dependencies {
                let _ = writeln!(
                    m,
                    "- `{}` ({}, {}{}): uses {}",
                    d.addon,
                    d.how,
                    d.status,
                    d.package
                        .as_ref()
                        .map_or(String::new(), |p| format!(", {p}")),
                    if d.uses.is_empty() {
                        "nothing named".into()
                    } else {
                        d.uses.join(", ")
                    }
                );
            }
        }
        let _ = writeln!(m, "\n## Needs behaviour\n");
        if self.needs_behaviour.is_empty() {
            let _ = writeln!(m, "Nothing: every script function is covered by data.");
        }
        for b in &self.needs_behaviour {
            let port = match &b.port {
                Some(p) if p.applied && p.how.is_empty() => {
                    format!(" **Ported** by `{}` ({}).", p.port, p.status)
                }
                Some(p) if p.applied => format!(
                    " **Ported** by `{}` ({}): {}.",
                    p.port,
                    p.status,
                    p.how.join("; ")
                ),
                Some(p) => format!(
                    " A port exists (`{}`, {}) but this copy does not match it; see Ports.",
                    p.port, p.status
                ),
                None => String::new(),
            };
            let _ = writeln!(
                m,
                "- `{}` at {}: {}.{port}",
                b.function, b.source, b.summary
            );
        }
        if self.needs_behaviour.iter().any(|b| b.port.is_none()) {
            let _ = writeln!(
                m,
                "\nNo port is listed for the rest yet. To make one, follow `docs/modding/porting.md`."
            );
        }
        for p in &self.ports {
            let _ = writeln!(m, "\n## Ports\n");
            if p.applied {
                let values: Vec<_> = p.values.iter().map(|(k, v)| format!("{k} {v}")).collect();
                let _ = writeln!(
                    m,
                    "Applied the {} port `{}` ({} copy){}. Changed: {}.",
                    p.status,
                    p.port,
                    p.copy,
                    if values.is_empty() {
                        String::new()
                    } else {
                        format!(", read from this copy's script: {}", values.join(", "))
                    },
                    p.files_changed.join(", ")
                );
            } else {
                let _ = writeln!(
                    m,
                    "The {} port `{}` was not applied: {}.",
                    p.status,
                    p.port,
                    p.reason.as_deref().unwrap_or("unknown")
                );
            }
        }
        for (title, list) in [
            ("Unsupported", &self.unsupported),
            ("Carried out by the port", &self.ported),
            ("Ambiguous", &self.ambiguous),
        ] {
            if list.is_empty() {
                continue;
            }
            let _ = writeln!(m, "\n## {title}\n");
            for f in list.iter().take(25) {
                let _ = writeln!(
                    m,
                    "- {}{}: {}",
                    f.what,
                    f.source
                        .as_ref()
                        .map_or(String::new(), |l| format!(" ({l})")),
                    f.resolution
                        .as_deref()
                        .filter(|_| std::ptr::eq(list, &self.ported))
                        .unwrap_or(&f.detail)
                );
            }
            if list.len() > 25 {
                let _ = writeln!(m, "- ... {} more in import-report.json", list.len() - 25);
            }
        }
        let _ = writeln!(m, "\nFull detail: `import-report.json`.");
        m
    }
}
