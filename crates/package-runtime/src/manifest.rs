//! A mod package's own `package.json`, in the platform manifest shape.
//!
//! `packages.json` (crate `bri-package`) says which packages a peer loads,
//! their version and side. The manifest inside a mod package's directory
//! says what the package provides and needs. The mod platform lane owns the
//! manifest format (`docs/modding/package-format.md`); this reader accepts
//! that JSON and checks only what the runtime relies on, and is replaced by
//! the lane's reader when it lands.
use bri_package::diag::Diagnostic;
use bri_package::id::{self, ContentId, Requirement, Version};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MANIFEST_FILE: &str = "package.json";
pub const MANIFEST_SCHEMA: u32 = 1;
const MAX_MANIFEST_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub api: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub license: String,
    #[serde(default)]
    pub provenance: serde_json::Value,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub provides: Vec<Provide>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provide {
    pub kind: String,
    pub id: String,
    pub file: String,
}

/// `package/file` for a diagnostic location.
pub fn location(package: &str, file: impl std::fmt::Display) -> String {
    format!("{package}/{file}")
}

impl Manifest {
    /// Parse and check a manifest. `expected` is the id `packages.json` lists
    /// it under; the two must agree.
    pub fn parse(bytes: &[u8], expected: &str) -> Result<Self, Vec<Diagnostic>> {
        let at = location(expected, MANIFEST_FILE);
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(vec![
                Diagnostic::error("manifest.too_large", "package.json is over 256 KiB").at(at),
            ]);
        }
        let manifest: Self = serde_json::from_slice(bytes).map_err(|e| {
            vec![Diagnostic::error("manifest.json", e.to_string())
                .at(format!("{at}:{}:{}", e.line(), e.column()))
                .hint("package.json is one JSON object with schema_version, id, version, api, name, license and provides")]
        })?;
        let mut out = Vec::new();
        if manifest.schema_version != MANIFEST_SCHEMA {
            out.push(
                Diagnostic::error(
                    "manifest.schema_version",
                    format!(
                        "schema_version {} is not supported",
                        manifest.schema_version
                    ),
                )
                .at(format!("{at}#/schema_version"))
                .hint(format!("set \"schema_version\": {MANIFEST_SCHEMA}")),
            );
        }
        if manifest.id != expected {
            out.push(
                Diagnostic::error(
                    "manifest.id.mismatch",
                    format!(
                        "package.json says `{}` but packages.json lists `{expected}`",
                        manifest.id
                    ),
                )
                .at(format!("{at}#/id")),
            );
        }
        if let Some(problem) = id::namespace_problem(&manifest.id) {
            out.push(
                Diagnostic::error(
                    "manifest.id.invalid",
                    format!("package id `{}` {problem}", manifest.id),
                )
                .at(format!("{at}#/id")),
            );
        } else if id::is_reserved(&manifest.id) {
            out.push(
                Diagnostic::error(
                    "manifest.id.reserved",
                    format!("package id `{}` is reserved for the game", manifest.id),
                )
                .at(format!("{at}#/id"))
                .hint("pick your own id"),
            );
        }
        if let Err(problem) = Version::parse(&manifest.version) {
            out.push(
                Diagnostic::error("manifest.version", problem)
                    .at(format!("{at}#/version"))
                    .hint("use three numbers, e.g. \"1.0.0\""),
            );
        }
        if manifest.api == 0 || manifest.api > bri_package::API_LEVEL {
            out.push(
                Diagnostic::error(
                    "manifest.api",
                    format!(
                        "needs platform API level {}; this game provides {}",
                        manifest.api,
                        bri_package::API_LEVEL
                    ),
                )
                .at(format!("{at}#/api"))
                .hint(format!("set \"api\": {}", bri_package::API_LEVEL)),
            );
        }
        if manifest.name.trim().is_empty() || manifest.name.len() > 64 {
            out.push(
                Diagnostic::error("manifest.name", "name must be 1 to 64 characters")
                    .at(format!("{at}#/name")),
            );
        }
        if manifest.license.trim().is_empty() {
            out.push(
                Diagnostic::error("manifest.license", "license is required")
                    .at(format!("{at}#/license"))
                    .hint("use an SPDX id such as CC0-1.0, or `proprietary`"),
            );
        }
        for (dependency, requirement) in &manifest.dependencies {
            if let Err(problem) = Requirement::parse(requirement) {
                out.push(
                    Diagnostic::error("manifest.dependency", format!("`{dependency}`: {problem}"))
                        .at(format!("{at}#/dependencies")),
                );
            }
        }
        for capability in &manifest.capabilities {
            if !crate::ops::CAPABILITIES.contains(&capability.as_str()) {
                out.push(
                    Diagnostic::error(
                        "manifest.capability",
                        format!("unknown capability `{capability}`"),
                    )
                    .at(format!("{at}#/capabilities"))
                    .hint(format!(
                        "known capabilities: {}",
                        crate::ops::CAPABILITIES.join(", ")
                    )),
                );
            }
        }
        for (i, provide) in manifest.provides.iter().enumerate() {
            let pointer = format!("{at}#/provides/{i}");
            match ContentId::parse(&provide.id) {
                Err(problem) => {
                    out.push(Diagnostic::error("manifest.provide.id", problem).at(pointer.clone()))
                }
                Ok(content) => {
                    if content.namespace != id::content_namespace(&manifest.id) {
                        out.push(
                            Diagnostic::error(
                                "manifest.provide.namespace",
                                format!(
                                    "`{}` is outside this package's namespace `{}`",
                                    provide.id, manifest.id
                                ),
                            )
                            .at(pointer.clone()),
                        );
                    }
                    if content.kind != provide.kind {
                        out.push(
                            Diagnostic::error(
                                "manifest.provide.kind",
                                format!(
                                    "`{}` has kind `{}` but is declared as `{}`",
                                    provide.id, content.kind, provide.kind
                                ),
                            )
                            .at(pointer.clone()),
                        );
                    }
                }
            }
            if crate::content::Kind::parse(&provide.kind).is_none() {
                out.push(
                    Diagnostic::error(
                        "manifest.provide.unknown_kind",
                        format!("unknown kind `{}`", provide.kind),
                    )
                    .at(pointer)
                    .hint(format!(
                        "known kinds: {}",
                        crate::content::Kind::NAMES.join(", ")
                    )),
                );
            }
        }
        if out.is_empty() {
            Ok(manifest)
        } else {
            Err(out)
        }
    }
}
