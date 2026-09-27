//! Server-owned, namespaced package state.
//!
//! Each package owns one namespace: server-wide keys and per-player keys.
//! Players are keyed by their durable identity ([`PlayerKey`]), so state
//! survives reconnects and, through [`Store::save`], host restarts. Only the
//! server writes it; clients receive the keys a package marks public.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const STORE_SCHEMA: u32 = 1;
const MAX_VALUE_BYTES: usize = 4096;
const MAX_STORE_BYTES: usize = 64 * 1024 * 1024;

/// The durable key a player's package state is stored under.
///
/// Today this is the player's verified public-key principal (hex) when the
/// transport proved one, otherwise a per-session fallback that does not
/// survive a reconnect. It is one small type so the platform's player
/// identity can replace it without touching package state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlayerKey(pub String);
impl PlayerKey {
    pub fn principal(bytes: &[u8; 32]) -> Self {
        Self(format!("principal:{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()))
    }
    pub fn session(owner: u64) -> Self {
        Self(format!("session:{owner}"))
    }
    pub fn durable(&self) -> bool {
        self.0.starts_with("principal:")
    }
}

/// Values are JSON scalars, short strings, and small arrays or objects.
pub fn check_value(value: &Value) -> Result<()> {
    fn depth(v: &Value) -> usize {
        match v {
            Value::Array(a) => 1 + a.iter().map(depth).max().unwrap_or(0),
            Value::Object(o) => 1 + o.values().map(depth).max().unwrap_or(0),
            _ => 0,
        }
    }
    ensure!(depth(value) <= 4, "state values nest at most 4 deep");
    ensure!(
        serde_json::to_vec(value)?.len() <= MAX_VALUE_BYTES,
        "a state value is at most {MAX_VALUE_BYTES} bytes"
    );
    if let Value::Number(n) = value {
        ensure!(n.as_f64().is_some_and(f64::is_finite), "numbers must be finite");
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Namespace {
    pub global: BTreeMap<String, Value>,
    pub players: BTreeMap<PlayerKey, BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Store {
    pub namespaces: BTreeMap<String, Namespace>,
}

#[derive(Serialize, Deserialize)]
struct SavedStore {
    schema_version: u32,
    store: Store,
}

impl Store {
    pub fn namespace(&self, package: &str) -> Option<&Namespace> {
        self.namespaces.get(package)
    }
    pub fn namespace_mut(&mut self, package: &str) -> &mut Namespace {
        self.namespaces.entry(package.into()).or_default()
    }
    pub fn player(&self, package: &str, player: &PlayerKey, key: &str) -> Option<&Value> {
        self.namespaces.get(package)?.players.get(player)?.get(key)
    }
    pub fn global(&self, package: &str, key: &str) -> Option<&Value> {
        self.namespaces.get(package)?.global.get(key)
    }
    /// Only persisted keys of durable players, and persisted global keys.
    pub fn persistent(&self, set: &crate::Catalog) -> Self {
        let mut out = Self::default();
        for (package, ns) in &self.namespaces {
            let Some(schema) = set.packages.get(package).and_then(|p| p.behaviour.as_ref()).map(|b| &b.state) else {
                continue;
            };
            let target = out.namespace_mut(package);
            target.global = ns
                .global
                .iter()
                .filter(|(k, _)| schema.global.get(*k).is_some_and(|d| d.persist))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (player, values) in ns.players.iter().filter(|(p, _)| p.durable()) {
                let kept: BTreeMap<_, _> = values
                    .iter()
                    .filter(|(k, _)| schema.player.get(*k).is_some_and(|d| d.persist))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if !kept.is_empty() {
                    target.players.insert(player.clone(), kept);
                }
            }
        }
        out
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec_pretty(&SavedStore {
            schema_version: STORE_SCHEMA,
            store: self.clone(),
        })?;
        ensure!(bytes.len() <= MAX_STORE_BYTES, "package state exceeds 64 MiB");
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_STORE_BYTES, "package state exceeds 64 MiB");
        let saved: SavedStore = serde_json::from_slice(bytes).context("Package state file is damaged")?;
        ensure!(saved.schema_version == STORE_SCHEMA, "Unsupported package state schema");
        for ns in saved.store.namespaces.values() {
            for v in ns.global.values().chain(ns.players.values().flat_map(|m| m.values())) {
                check_value(v)?;
            }
        }
        Ok(saved.store)
    }
    /// Crash-safe write through a temporary file.
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        let bytes = self.encode()?;
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, bytes)?;
        std::fs::rename(&temp, path)?;
        Ok(())
    }
    pub fn load(path: &std::path::Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Self::decode(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).context("Could not read package state"),
        }
    }
}
