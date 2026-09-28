//! `package.json`: what a package is, what it needs and what it provides.
use crate::diag::{Diagnostic, Diagnostics};
use crate::id::{self, ContentId, Requirement, Version};
use crate::{API_LEVEL, capability, kind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MANIFEST_FILE: &str = "package.json";
pub const MANIFEST_SCHEMA: u32 = 1;
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    /// The package id, which is also the namespace of every id it declares.
    pub id: String,
    /// `major.minor.patch`.
    pub version: String,
    /// The platform API level this package was written against. A server
    /// whose level is lower refuses it.
    pub api: u32,
    /// Display name.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    /// SPDX identifier (`CC0-1.0`, `MIT`, `CC-BY-4.0`) or `proprietary`.
    pub license: String,
    #[serde(default)]
    pub provenance: Provenance,
    /// Package id to version requirement (`">=1.2"`).
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    /// Named capabilities the package's server behaviour needs.
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub provides: Vec<Provide>,
    /// Slots this package fills. `add` composes with other packages;
    /// `replace` claims the slot exclusively and conflicts with any other
    /// package that also replaces it.
    #[serde(default)]
    pub slots: Vec<SlotClaim>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// `original` for work made for this package, otherwise where it came from
    /// (a URL or a description).
    #[serde(default = "original")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}
fn original() -> String {
    "original".into()
}
impl Default for Provenance {
    fn default() -> Self {
        Self {
            source: original(),
            notes: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provide {
    pub kind: String,
    /// `namespace:kind/name`, where the namespace is this package's id and
    /// the kind matches `kind`.
    pub id: String,
    /// Package-relative file holding the definition or asset.
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotClaim {
    pub slot: String,
    pub mode: SlotMode,
    /// The provided id that fills the slot.
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotMode {
    Add,
    Replace,
}

impl Manifest {
    pub fn version(&self) -> Option<Version> {
        Version::parse(&self.version).ok()
    }

    /// Parse and validate a manifest. Returns the manifest when it could be
    /// read at all, plus every problem found; the manifest is only usable
    /// when there are no errors.
    pub fn parse(bytes: &[u8]) -> (Option<Self>, Diagnostics) {
        let mut out = Diagnostics::default();
        let here = MANIFEST_FILE;
        if bytes.len() > MAX_MANIFEST_BYTES {
            out.push(
                Diagnostic::error(
                    "manifest.too_large",
                    format!("{} bytes; the limit is {MAX_MANIFEST_BYTES}", bytes.len()),
                )
                .at(here),
            );
            return (None, out);
        }
        let value: serde_json::Value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(error) => {
                out.push(
                    Diagnostic::error("manifest.json", format!("not valid JSON: {error}"))
                        .at(format!("{here}:{}:{}", error.line(), error.column()))
                        .hint("package.json must be one JSON object; see docs/modding/package-format.md"),
                );
                return (None, out);
            }
        };
        if let Some(schema) = value.get("schema_version").and_then(|v| v.as_u64())
            && schema != MANIFEST_SCHEMA as u64
        {
            out.push(
                Diagnostic::error(
                    "manifest.schema_version",
                    format!("schema_version {schema} is not supported; this game reads {MANIFEST_SCHEMA}"),
                )
                .at(format!("{here}#/schema_version"))
                .hint(format!("set \"schema_version\": {MANIFEST_SCHEMA}")),
            );
            return (None, out);
        }
        let manifest: Self = match serde_json::from_value(value) {
            Ok(manifest) => manifest,
            Err(error) => {
                out.push(
                    Diagnostic::error("manifest.shape", format!("{error}"))
                        .at(here)
                        .hint("compare with the template from `bri-mod new`; unknown fields are rejected so typos are caught"),
                );
                return (None, out);
            }
        };
        manifest.validate(&mut out);
        (Some(manifest), out)
    }

    fn validate(&self, out: &mut Diagnostics) {
        let here = MANIFEST_FILE;
        match id::namespace_problem(&self.id) {
            Some(problem) => out.push(
                Diagnostic::error("manifest.id.invalid", format!("package id `{}` {problem}", self.id))
                    .at(format!("{here}#/id"))
                    .hint("use a short lowercase name such as `creeper` or `space-race`"),
            ),
            None if id::is_reserved(&self.id) => out.push(
                Diagnostic::error(
                    "manifest.id.reserved",
                    format!("package id `{}` is reserved for the base game or engine", self.id),
                )
                .at(format!("{here}#/id"))
                .hint("pick your own id; packages add to vanilla through slots, they cannot take its namespace"),
            ),
            None => {}
        }
        if let Err(problem) = Version::parse(&self.version) {
            out.push(
                Diagnostic::error("manifest.version", problem)
                    .at(format!("{here}#/version"))
                    .hint("use three numbers, e.g. \"1.0.0\""),
            );
        }
        if self.api == 0 || self.api > API_LEVEL {
            out.push(
                Diagnostic::error(
                    "manifest.api",
                    format!(
                        "needs platform API level {}; this game provides level {API_LEVEL}",
                        self.api
                    ),
                )
                .at(format!("{here}#/api"))
                .hint(format!("set \"api\": {API_LEVEL} unless you need a newer game build")),
            );
        }
        if self.name.trim().is_empty() || self.name.len() > 64 || self.name.chars().any(char::is_control) {
            out.push(
                Diagnostic::error("manifest.name", "name must be 1 to 64 printable characters")
                    .at(format!("{here}#/name")),
            );
        }
        if self.description.len() > 2000 {
            out.push(
                Diagnostic::error("manifest.description", "description is longer than 2000 characters")
                    .at(format!("{here}#/description")),
            );
        }
        if self.license.trim().is_empty() {
            out.push(
                Diagnostic::error("manifest.license", "license is empty")
                    .at(format!("{here}#/license"))
                    .hint("use an SPDX id such as CC0-1.0 or MIT, or \"proprietary\""),
            );
        }
        if self.provenance.source.trim().is_empty() {
            out.push(
                Diagnostic::warning("manifest.provenance", "provenance.source is empty")
                    .at(format!("{here}#/provenance/source"))
                    .hint("say \"original\" or where the assets came from"),
            );
        }
        for (dependency, requirement) in &self.dependencies {
            if let Some(problem) = id::namespace_problem(dependency) {
                out.push(
                    Diagnostic::error("manifest.dependency.id", format!("dependency `{dependency}` {problem}"))
                        .at(format!("{here}#/dependencies/{dependency}")),
                );
            }
            if dependency == &self.id {
                out.push(
                    Diagnostic::error("manifest.dependency.self", "a package cannot depend on itself")
                        .at(format!("{here}#/dependencies/{dependency}")),
                );
            }
            if let Err(problem) = Requirement::parse(requirement) {
                out.push(
                    Diagnostic::error("manifest.dependency.requirement", problem)
                        .at(format!("{here}#/dependencies/{dependency}")),
                );
            }
        }
        let mut seen_capabilities = BTreeSet::new();
        for (i, name) in self.capabilities.iter().enumerate() {
            if capability::describe(name).is_none() {
                out.push(
                    Diagnostic::error("manifest.capability.unknown", format!("unknown capability `{name}`"))
                        .at(format!("{here}#/capabilities/{i}"))
                        .hint(format!("known capabilities: {}", capability::names().join(", "))),
                );
            }
            if !seen_capabilities.insert(name) {
                out.push(
                    Diagnostic::warning("manifest.capability.duplicate", format!("`{name}` is listed twice"))
                        .at(format!("{here}#/capabilities/{i}")),
                );
            }
        }
        let mut seen_ids = BTreeSet::new();
        for (i, provide) in self.provides.iter().enumerate() {
            let at = format!("{here}#/provides/{i}");
            let spec = kind::find(&provide.kind);
            if spec.is_none() {
                out.push(
                    Diagnostic::error(
                        "provides.kind.unknown",
                        format!("no system in this game consumes kind `{}`", provide.kind),
                    )
                    .at(format!("{at}/kind"))
                    .hint(format!("known kinds: {}", kind::names().join(", "))),
                );
            }
            match ContentId::parse(&provide.id) {
                Err(problem) => out.push(
                    Diagnostic::error("provides.id.invalid", problem)
                        .at(format!("{at}/id"))
                        .hint(format!("ids look like `{}:{}/name`", self.id, provide.kind)),
                ),
                Ok(parsed) => {
                    if id::is_reserved(&parsed.namespace) {
                        out.push(
                            Diagnostic::error(
                                "provides.id.overrides_vanilla",
                                format!("`{}` is in the reserved `{}` namespace", provide.id, parsed.namespace),
                            )
                            .at(format!("{at}/id"))
                            .hint("a package cannot redefine base-game content; declare your own id and fill a slot instead"),
                        );
                    } else if parsed.namespace != self.id {
                        out.push(
                            Diagnostic::error(
                                "provides.id.foreign_namespace",
                                format!(
                                    "`{}` is in namespace `{}`, but this package owns only `{}`",
                                    provide.id, parsed.namespace, self.id
                                ),
                            )
                            .at(format!("{at}/id"))
                            .hint(format!("rename it to `{}:{}/{}`", self.id, parsed.kind, parsed.name)),
                        );
                    }
                    if parsed.kind != provide.kind {
                        out.push(
                            Diagnostic::error(
                                "provides.id.kind_mismatch",
                                format!("id kind `{}` differs from declared kind `{}`", parsed.kind, provide.kind),
                            )
                            .at(format!("{at}/id")),
                        );
                    }
                }
            }
            if !seen_ids.insert(provide.id.clone()) {
                out.push(
                    Diagnostic::error("provides.id.duplicate", format!("`{}` is declared twice", provide.id))
                        .at(format!("{at}/id")),
                );
            }
        }
        let provided: BTreeSet<&str> = self.provides.iter().map(|p| p.id.as_str()).collect();
        for (i, claim) in self.slots.iter().enumerate() {
            let at = format!("{here}#/slots/{i}");
            if crate::slot::find(&claim.slot).is_none() {
                out.push(
                    Diagnostic::error("slots.unknown", format!("no slot named `{}`", claim.slot))
                        .at(format!("{at}/slot"))
                        .hint(format!("known slots: {}", crate::slot::names().join(", "))),
                );
            }
            if !provided.contains(claim.id.as_str()) {
                out.push(
                    Diagnostic::error(
                        "slots.id.not_provided",
                        format!("slot is filled with `{}`, which this package does not provide", claim.id),
                    )
                    .at(format!("{at}/id")),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(extra: &str) -> String {
        format!(
            r#"{{"schema_version":1,"id":"creeper","version":"1.0.0","api":1,"name":"Creeper","license":"CC0-1.0"{extra}}}"#
        )
    }

    #[test]
    fn minimal_manifest_is_valid() {
        let (m, d) = Manifest::parse(manifest("").as_bytes());
        assert!(d.is_empty(), "{d:?}");
        assert_eq!(m.unwrap().provenance.source, "original");
    }

    #[test]
    fn precise_codes_for_bad_manifests() {
        let codes = |json: &str| {
            let (_, d) = Manifest::parse(json.as_bytes());
            d.codes().into_iter().map(String::from).collect::<Vec<_>>()
        };
        assert_eq!(codes("{"), ["manifest.json"]);
        assert_eq!(codes(&manifest(r#","extra":1"#)), ["manifest.shape"]);
        assert_eq!(
            codes(r#"{"schema_version":9,"id":"x"}"#),
            ["manifest.schema_version"]
        );
        assert_eq!(
            codes(&manifest("").replace("\"creeper\"", "\"v20\"")),
            ["manifest.id.reserved"]
        );
        assert_eq!(
            codes(&manifest("").replace("\"api\":1", "\"api\":99")),
            ["manifest.api"]
        );
        assert_eq!(
            codes(&manifest(
                r#","provides":[{"kind":"asset","id":"v20:asset/brick1x1","file":"a.png"}]"#
            )),
            ["provides.id.overrides_vanilla"]
        );
        assert_eq!(
            codes(&manifest(
                r#","provides":[{"kind":"asset","id":"other:asset/x","file":"a.png"}]"#
            )),
            ["provides.id.foreign_namespace"]
        );
        assert_eq!(
            codes(&manifest(
                r#","provides":[{"kind":"warpdrive","id":"creeper:warpdrive/x","file":"a"}]"#
            )),
            ["provides.kind.unknown"]
        );
        assert_eq!(
            codes(&manifest(r#","capabilities":["root.shell"]"#)),
            ["manifest.capability.unknown"]
        );
        assert_eq!(
            codes(&manifest(r#","dependencies":{"creeper":"*"}"#)),
            ["manifest.dependency.self"]
        );
    }
}
