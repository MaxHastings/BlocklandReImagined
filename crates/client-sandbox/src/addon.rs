//! The client code an Add-On carries: the `client` section of its
//! `package.json`, and the module and shader files it names.
//!
//! ```json
//! "client": {
//!   "module": "client/main.wasm",
//!   "capabilities": ["render.layer", "render.shader"],
//!   "shaders": ["client/cube.wgsl"],
//!   "sounds": [],
//!   "personal": false
//! }
//! ```
//!
//! `personal` (default false) makes the code each player's own choice, run
//! wherever they play; otherwise the host decides and joiners run the
//! host's (`bri_package::library::CodeOwner`).
//!
//! Everything is checked here, when the Add-On loads and before the player
//! is asked to trust it: the module compiles, it imports only the functions
//! of the capabilities it declares, and every shader passes
//! [`crate::shader::compile`]. Nothing runs yet.
use crate::capability::{self, Capability, Tier};
use crate::shader::{self, Shader};
use bri_package::diag::Diagnostic;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

pub const MANIFEST_FILE: &str = "package.json";
/// Largest module: bounds download size and, more importantly, the time
/// Cranelift may spend compiling it.
pub const MAX_MODULE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_SHADERS: usize = 64;
pub const MAX_SOUNDS: usize = 256;
/// Largest sound file an Add-On may carry.
pub const MAX_SOUND_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
struct PackageJson {
    id: String,
    version: String,
    #[serde(default)]
    name: String,
    client: Option<ClientSection>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientSection {
    module: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    shaders: Vec<String>,
    #[serde(default)]
    sounds: Vec<String>,
    /// Each player's own choice rather than the host's
    /// (`bri_package::library::CodeOwner::Player`). Read by the package
    /// library, which decides the Add-On's side from it.
    #[serde(default)]
    #[allow(dead_code)]
    personal: bool,
}

/// One Add-On's checked client code.
#[derive(Debug, Clone)]
pub struct AddOnCode {
    pub id: String,
    pub version: String,
    /// The player-facing name.
    pub name: String,
    pub module: Vec<u8>,
    pub capabilities: BTreeSet<Capability>,
    pub shaders: Vec<Shader>,
    pub sounds: Vec<String>,
    /// Each sound file's bytes, read when the Add-On loads.
    pub sound_files: Vec<(String, Vec<u8>)>,
    /// SHA-256 over everything that can run or be run: the module, every
    /// shader and the declared capabilities. Trust is remembered against it.
    pub code_hash: String,
}

impl AddOnCode {
    /// The highest tier any of its capabilities needs.
    pub fn tier(&self) -> Tier {
        self.capabilities
            .iter()
            .map(|c| c.tier())
            .max()
            .unwrap_or(Tier::Sandboxed)
    }

    /// Load the client code of the Add-On in `dir`, or `None` when it has
    /// none (a data-only Add-On).
    pub fn load(dir: &Path) -> Result<Option<Self>, Vec<Diagnostic>> {
        // Base game packages and plain data folders carry no manifest.
        if !dir.join(MANIFEST_FILE).exists() {
            return Ok(None);
        }
        let manifest = read(dir, MANIFEST_FILE, 256 * 1024)?;
        let json: PackageJson = serde_json::from_slice(&manifest).map_err(|e| {
            vec![Diagnostic::error("client.manifest", e.to_string()).at(MANIFEST_FILE)]
        })?;
        let Some(client) = json.client else {
            return Ok(None);
        };
        let at = |field: &str| format!("{}/{MANIFEST_FILE}#/client/{field}", json.id);
        let mut problems = Vec::new();
        let mut capabilities = BTreeSet::new();
        for name in &client.capabilities {
            match Capability::parse(name) {
                Some(c) => {
                    capabilities.insert(c);
                }
                None => problems.push(
                    Diagnostic::error(
                        "client.capability.unknown",
                        format!("unknown client capability `{name}`"),
                    )
                    .at(at("capabilities"))
                    .hint(format!(
                        "known: {}",
                        Capability::ALL
                            .iter()
                            .map(|c| c.name())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                ),
            }
        }
        if client.shaders.len() > MAX_SHADERS || client.sounds.len() > MAX_SOUNDS {
            problems.push(
                Diagnostic::error(
                    "client.too_many_files",
                    format!("at most {MAX_SHADERS} shaders and {MAX_SOUNDS} sounds"),
                )
                .at(at("shaders")),
            );
        }
        if !client.shaders.is_empty() && !capabilities.contains(&Capability::RenderShader) {
            problems.push(
                Diagnostic::error(
                    "client.capability.undeclared",
                    "shaders are listed but `render.shader` is not declared",
                )
                .at(at("shaders")),
            );
        }
        let mut hash = Sha256::new();
        hash.update(b"bri-client-code-1\0");
        let module = match read(dir, &client.module, MAX_MODULE_BYTES) {
            Ok(bytes) => bytes,
            Err(mut e) => {
                problems.append(&mut e);
                Vec::new()
            }
        };
        hash.update((module.len() as u64).to_le_bytes());
        hash.update(&module);
        let mut shaders = Vec::new();
        for file in &client.shaders {
            let bytes = match read(dir, file, shader::MAX_SHADER_BYTES) {
                Ok(bytes) => bytes,
                Err(mut e) => {
                    problems.append(&mut e);
                    continue;
                }
            };
            hash.update(file.as_bytes());
            hash.update([0]);
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(&bytes);
            let Ok(source) = std::str::from_utf8(&bytes) else {
                problems.push(
                    Diagnostic::error("shader.utf8", "shader is not UTF-8")
                        .at(format!("{}/{file}", json.id)),
                );
                continue;
            };
            match shader::compile(file, source) {
                Ok(s) => shaders.push(s),
                Err(e) => problems
                    .push(Diagnostic::error(e.code, e.message).at(format!("{}/{file}", json.id))),
            }
        }
        let mut sound_files = Vec::new();
        for sound in &client.sounds {
            match check_path(dir, sound) {
                Err(mut e) => problems.append(&mut e),
                Ok(path) => {
                    let lower = sound.to_ascii_lowercase();
                    let bytes = std::fs::read(&path).unwrap_or_default();
                    if !(lower.ends_with(".wav") || lower.ends_with(".ogg")) {
                        problems.push(
                            Diagnostic::error("client.sound", "sounds are .wav or .ogg files")
                                .at(format!("{}/{sound}", json.id)),
                        );
                    } else if bytes.is_empty() || bytes.len() > MAX_SOUND_BYTES {
                        problems.push(
                            Diagnostic::error(
                                "client.sound",
                                format!("a sound file is 1 byte to {MAX_SOUND_BYTES} bytes"),
                            )
                            .at(format!("{}/{sound}", json.id)),
                        );
                    } else {
                        sound_files.push((sound.clone(), bytes));
                    }
                }
            }
        }
        for c in &capabilities {
            hash.update(c.name().as_bytes());
            hash.update([0]);
        }
        if !module.is_empty() {
            problems.extend(check_imports(&json.id, &module, &capabilities));
        }
        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(Some(Self {
            name: if json.name.trim().is_empty() {
                json.id.clone()
            } else {
                json.name
            },
            id: json.id,
            version: json.version,
            module,
            capabilities,
            shaders,
            sounds: client.sounds,
            sound_files,
            code_hash: hex(&hash.finalize()),
        }))
    }
}

/// Every import must be a known host function whose capability the Add-On
/// declared. Checked without compiling, so a refused module costs nothing.
pub fn check_imports(
    package: &str,
    module: &[u8],
    declared: &BTreeSet<Capability>,
) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    // Only the import section matters here; full validation happens when
    // the sandbox compiles the module.
    let imports = match crate::wasm_imports::function_imports(module) {
        Ok(i) => i,
        Err(e) => {
            return vec![
                Diagnostic::error("client.module.malformed", e)
                    .at(format!("{package}/client module")),
            ];
        }
    };
    for import in imports {
        if import.module != capability::IMPORT_MODULE {
            problems.push(
                Diagnostic::error(
                    "client.import.unknown",
                    format!(
                        "imports `{}.{}`; client code may import only from `bri`",
                        import.module, import.name
                    ),
                )
                .at(package.to_string()),
            );
            continue;
        }
        match capability::function_capability(&import.name) {
            Err(capability::UnknownFunction) => problems.push(
                Diagnostic::error(
                    "client.import.unknown",
                    format!("`bri.{}` is not a host function", import.name),
                )
                .at(package.to_string()),
            ),
            Ok(Some(c)) if !declared.contains(&c) => problems.push(
                Diagnostic::error(
                    "client.capability.denied",
                    format!(
                        "imports `bri.{}`, which needs `{}`, but the Add-On does not declare it",
                        import.name,
                        c.name()
                    ),
                )
                .at(package.to_string())
                .hint(format!(
                    "add \"{}\" to client.capabilities in package.json",
                    c.name()
                )),
            ),
            Ok(_) => {}
        }
    }
    problems
}

fn check_path(dir: &Path, relative: &str) -> Result<std::path::PathBuf, Vec<Diagnostic>> {
    // TODO(PR #1): use bri_package::path::inside, the one package path rule.
    let bad = relative.is_empty()
        || relative.len() > 240
        || relative.starts_with('/')
        || relative.contains('\\')
        || relative.contains(':')
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    if bad {
        return Err(vec![
            Diagnostic::error(
                "client.path",
                format!("`{relative}` is not a plain relative path inside the Add-On"),
            )
            .at(relative.to_string()),
        ]);
    }
    let path = dir.join(relative);
    // No links anywhere along the path: a link could point outside.
    let mut walk = dir.to_path_buf();
    for part in relative.split('/') {
        walk.push(part);
        match std::fs::symlink_metadata(&walk) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(vec![
                    Diagnostic::error("client.path", format!("`{relative}` goes through a link"))
                        .at(relative.to_string()),
                ]);
            }
            Ok(_) => {}
            Err(e) => {
                return Err(vec![
                    Diagnostic::error("client.missing", format!("`{relative}`: {e}"))
                        .at(relative.to_string()),
                ]);
            }
        }
    }
    Ok(path)
}

fn read(dir: &Path, relative: &str, limit: usize) -> Result<Vec<u8>, Vec<Diagnostic>> {
    let path = check_path(dir, relative)?;
    let size = std::fs::metadata(&path)
        .map_err(|e| vec![Diagnostic::error("client.missing", e.to_string()).at(relative)])?
        .len();
    if size > limit as u64 {
        return Err(vec![
            Diagnostic::error(
                "client.too_large",
                format!("`{relative}` is {size} bytes; the limit is {limit}"),
            )
            .at(relative),
        ]);
    }
    std::fs::read(&path)
        .map_err(|e| vec![Diagnostic::error("client.read", e.to_string()).at(relative)])
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
