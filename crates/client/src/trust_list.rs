//! The player's saved trust list (v20 `config/client/prefs-trustList.txt`):
//! who they trust and how much, uploaded to every server they join.
use anyhow::{Context, Result, ensure};
use bri_sim::session::{MAX_TRUST_LIST, TrustEntry, TrustLevel};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Line {
    principal: String,
    level: u8,
    name: String,
}

pub struct TrustList {
    path: PathBuf,
    lines: Vec<Line>,
}

fn hex(principal: &[u8; 32]) -> String {
    principal.iter().map(|b| format!("{b:02x}")).collect()
}
fn parse(hex: &str) -> Option<[u8; 32]> {
    let mut out = [0; 32];
    if hex.len() != 64 {
        return None;
    }
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// The number shown in the BL_ID column for an identity.
pub fn display_id(principal: &[u8; 32]) -> u64 {
    let mut bytes = [0; 8];
    bytes.copy_from_slice(&principal[..8]);
    u64::from_be_bytes(bytes) % 1_000_000
}

/// `NewPlayerListGui::updateTrust` column text.
pub fn label(level: TrustLevel) -> &'static str {
    match level {
        TrustLevel::You => "You",
        TrustLevel::None => "-",
        TrustLevel::Build => "Build",
        TrustLevel::Full => "Full",
        TrustLevel::Lan => "LAN",
    }
}

impl TrustList {
    /// `loadTrustList`: a missing or damaged file is an empty list.
    pub fn load(path: &Path) -> Self {
        let lines = std::fs::metadata(path)
            .ok()
            .filter(|m| m.len() <= 1024 * 1024)
            .and_then(|_| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Vec<Line>>(&bytes).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|l| l.level > 0 && parse(&l.principal).is_some())
            .take(MAX_TRUST_LIST)
            .collect();
        Self {
            path: path.to_owned(),
            lines,
        }
    }
    /// `clientCmdTrustListUpload_Start`.
    pub fn entries(&self) -> Vec<TrustEntry> {
        self.lines
            .iter()
            .filter_map(|l| {
                Some(TrustEntry {
                    principal: parse(&l.principal)?,
                    level: l.level,
                })
            })
            .collect()
    }
    /// `updateClientTrustList` + `saveTrustList`.
    pub fn update(&mut self, principal: &[u8; 32], level: u8, name: &str) -> Result<()> {
        ensure!(level <= 2, "Invalid trust level");
        let key = hex(principal);
        match self.lines.iter_mut().find(|l| l.principal == key) {
            Some(line) => {
                line.level = level;
                line.name = name.into();
            }
            None => {
                ensure!(self.lines.len() < MAX_TRUST_LIST, "Trust list is full");
                self.lines.push(Line {
                    principal: key,
                    level,
                    name: name.into(),
                });
            }
        }
        self.lines.retain(|l| l.level > 0);
        let bytes = serde_json::to_vec_pretty(&self.lines)?;
        bri_files::replace(&self.path, &bytes).context("Could not save the trust list")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trust_list_round_trips_and_drops_removed_entries() {
        let dir = std::env::temp_dir().join(format!("bri-trust-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("trust-list.json");
        let mut list = TrustList::load(&path);
        assert!(list.entries().is_empty());
        list.update(&[7; 32], 2, "Ann").unwrap();
        list.update(&[8; 32], 1, "Bob").unwrap();
        list.update(&[7; 32], 1, "Ann").unwrap();
        list.update(&[8; 32], 0, "Bob").unwrap();
        let loaded = TrustList::load(&path);
        assert_eq!(
            loaded.entries(),
            vec![TrustEntry {
                principal: [7; 32],
                level: 1
            }]
        );
        assert!(display_id(&[7; 32]) < 1_000_000);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
