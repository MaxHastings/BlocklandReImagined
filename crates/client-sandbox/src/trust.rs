//! Whether a player trusts a server to run its Add-Ons' code, and the
//! prompt that asks them.
//!
//! Three tiers (see [`Tier`]): data downloads without asking; sandboxed
//! code asks once per server ("Trust and join" or "Leave"); elevated code
//! asks separately and more strongly, per Add-On. Every choice is
//! remembered per server and per Add-On against the Add-On's code hash, so
//! changed code asks again, and a player can revoke any of it later.
//! Nothing escalates silently: a grant covers exactly the code hash and
//! capabilities that were shown.
use crate::addon::AddOnCode;
use crate::capability::{Capability, Tier};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const TRUST_FILE: &str = "addon-trust.json";
pub const TRUST_SCHEMA: u32 = 1;

/// What the player granted one Add-On on one server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Sandboxed,
    Elevated,
}
impl TrustLevel {
    fn covers(self, tier: Tier) -> bool {
        match tier {
            Tier::Data => true,
            Tier::Sandboxed => true,
            Tier::Elevated => self == Self::Elevated,
        }
    }
}

/// The part of an Add-On's client code the trust decision depends on.
/// Known from its `package.json` and listing before anything downloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSummary {
    pub id: String,
    pub name: String,
    pub code_hash: String,
    pub capabilities: BTreeSet<Capability>,
}
impl CodeSummary {
    pub fn tier(&self) -> Tier {
        self.capabilities
            .iter()
            .map(|c| c.tier())
            .max()
            .unwrap_or(Tier::Sandboxed)
    }
}
impl From<&AddOnCode> for CodeSummary {
    fn from(code: &AddOnCode) -> Self {
        Self {
            id: code.id.clone(),
            name: code.name.clone(),
            code_hash: code.code_hash.clone(),
            capabilities: code.capabilities.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub code_hash: String,
    pub level: TrustLevel,
    pub capabilities: Vec<String>,
}

impl Grant {
    /// Whether this grant covers `code`: the same code hash, a level for its
    /// tier, and no capability the prompt did not show. The hash alone is
    /// not enough, because the prompt is built from what the server says
    /// about its code before it downloads: a server could name the real
    /// hash while listing fewer capabilities than the code declares.
    fn covers(&self, code: &CodeSummary) -> bool {
        self.code_hash == code.code_hash
            && self.level.covers(code.tier())
            && code
                .capabilities
                .iter()
                .all(|c| self.capabilities.iter().any(|shown| shown == c.name()))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerTrust {
    /// The name the server had when the player last chose, for the
    /// Add-Ons screen's list of trusted servers.
    pub name: String,
    pub addons: BTreeMap<String, Grant>,
}

/// Every choice the player made, saved in their settings folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustStore {
    pub schema_version: u32,
    /// Keyed by the server's identity: its host key when it has one,
    /// otherwise its address.
    pub servers: BTreeMap<String, ServerTrust>,
}
impl Default for TrustStore {
    fn default() -> Self {
        Self {
            schema_version: TRUST_SCHEMA,
            servers: BTreeMap::new(),
        }
    }
}

/// One Add-On on the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRow {
    pub id: String,
    pub name: String,
    /// True when the player trusted an earlier version of its code.
    pub changed: bool,
    /// What it will be able to do, in plain words.
    pub can: Vec<&'static str>,
}

/// What the join screen shows before any code downloads or runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustPrompt {
    pub server: String,
    pub level: TrustLevel,
    pub title: String,
    pub body: String,
    pub rows: Vec<PromptRow>,
    /// Always-true limits of the sandbox, or the risk of going beyond it.
    pub footer: String,
    pub accept: &'static str,
    pub decline: &'static str,
    /// Elevated prompts need the player to tick this before accept works.
    pub confirm: Option<String>,
    /// When a native plugin is on the prompt, accept also needs the player
    /// to type this (the server's name), so it cannot be clicked through.
    pub type_to_confirm: Option<String>,
    summaries: Vec<CodeSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustDecision {
    /// Nothing to ask: data only, or every Add-On already trusted as is.
    Join,
    /// Ask. Sandboxed Add-Ons are asked about first; elevated ones get
    /// their own prompt after.
    Ask(Box<TrustPrompt>),
}

impl TrustStore {
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join(TRUST_FILE);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let store: Self = serde_json::from_slice(&bytes)?;
                anyhow::ensure!(
                    store.schema_version == TRUST_SCHEMA,
                    "{} has schema_version {}",
                    path.display(),
                    store.schema_version
                );
                Ok(store)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        let temp = dir.join(format!("{TRUST_FILE}.{}.tmp", std::process::id()));
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&temp, dir.join(TRUST_FILE))?;
        Ok(())
    }

    /// What to do when joining `server` (identity) named `name`, whose
    /// Add-Ons carry `code`.
    pub fn decide(&self, server: &str, name: &str, code: &[CodeSummary]) -> TrustDecision {
        let granted = self.servers.get(server);
        let pending: Vec<(&CodeSummary, bool)> = code
            .iter()
            .filter_map(|c| {
                let grant = granted.and_then(|s| s.addons.get(&c.id));
                match grant {
                    Some(g) if g.covers(c) => None,
                    Some(g) => Some((c, g.code_hash != c.code_hash)),
                    None => Some((c, false)),
                }
            })
            .collect();
        if pending.is_empty() {
            return TrustDecision::Join;
        }
        let sandboxed: Vec<_> = pending
            .iter()
            .filter(|(c, _)| c.tier() != Tier::Elevated)
            .cloned()
            .collect();
        let (level, rows) = if sandboxed.is_empty() {
            (TrustLevel::Elevated, pending)
        } else {
            (TrustLevel::Sandboxed, sandboxed)
        };
        TrustDecision::Ask(Box::new(prompt(server, name, level, &rows)))
    }

    /// The player accepted `prompt`: remember exactly what it showed.
    pub fn accept(&mut self, prompt: &TrustPrompt, server_name: &str) {
        let entry = self.servers.entry(prompt.server.clone()).or_default();
        entry.name = server_name.to_string();
        for code in &prompt.summaries {
            entry.addons.insert(
                code.id.clone(),
                Grant {
                    code_hash: code.code_hash.clone(),
                    level: prompt.level,
                    capabilities: code
                        .capabilities
                        .iter()
                        .map(|c| c.name().to_string())
                        .collect(),
                },
            );
        }
    }

    /// Forget everything the player granted a server.
    pub fn revoke_server(&mut self, server: &str) -> bool {
        self.servers.remove(server).is_some()
    }

    /// Forget one Add-On's grant on one server.
    pub fn revoke_addon(&mut self, server: &str, addon: &str) -> bool {
        self.servers
            .get_mut(server)
            .is_some_and(|s| s.addons.remove(addon).is_some())
    }

    /// The level a running Add-On was granted, if its code is the code the
    /// player saw. The host refuses to start anything else.
    pub fn granted(&self, server: &str, code: &CodeSummary) -> Option<TrustLevel> {
        let grant = self.servers.get(server)?.addons.get(&code.id)?;
        grant.covers(code).then_some(grant.level)
    }
}

fn prompt(
    server: &str,
    name: &str,
    level: TrustLevel,
    rows: &[(&CodeSummary, bool)],
) -> TrustPrompt {
    let prompt_rows = rows
        .iter()
        .map(|(c, changed)| PromptRow {
            id: c.id.clone(),
            name: c.name.clone(),
            changed: *changed,
            can: c.capabilities.iter().map(|c| c.plain_words()).collect(),
        })
        .collect();
    let changed = rows.iter().any(|(_, changed)| *changed);
    let native = rows
        .iter()
        .any(|(c, _)| c.capabilities.contains(&Capability::Native));
    let (title, body, mut footer, accept, decline, confirm) = match level {
        TrustLevel::Sandboxed => (
            format!("{name} wants to run Add-On code"),
            if changed {
                "Some of this server's Add-Ons changed since you trusted them. They run in a sandbox on your PC and can:".to_string()
            } else {
                "This server's Add-Ons include code that runs on your PC in a sandbox. They can:".to_string()
            },
            "They cannot read your files, reach the internet, see your other Add-Ons or change the game's rules. You can take this back any time on the Add-Ons screen.".to_string(),
            "Trust and join",
            "Leave",
            None,
        ),
        TrustLevel::Elevated => (
            format!("{name} wants full trust"),
            "These Add-Ons ask to go beyond the sandbox. Only allow this if you know and trust whoever runs this server. They can:".to_string(),
            "Outside the sandbox, a harmful Add-On could see or change things on your PC, or show your IP address to other sites. You can take this back any time on the Add-Ons screen.".to_string(),
            "Fully trust and join",
            "Join without them",
            Some(format!("I know who runs {name} and I trust them with my PC")),
        ),
    };
    if native {
        footer = format!(
            "A native plugin runs as a normal program: it can do anything you can do on this PC, and no sandbox limits it. {footer}"
        );
    }
    TrustPrompt {
        type_to_confirm: native.then(|| name.to_string()),
        server: server.to_string(),
        level,
        title,
        body,
        rows: prompt_rows,
        footer,
        accept,
        decline,
        confirm,
        summaries: rows.iter().map(|(c, _)| (*c).clone()).collect(),
    }
}
