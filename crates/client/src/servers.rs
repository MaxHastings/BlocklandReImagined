//! Servers a player joined or starred, shown in the Join Server list next to
//! LAN games (`servers.json` in the client state folder).
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Recently joined servers kept besides favourites.
pub const RECENT: usize = 10;
/// Favourites kept at most.
pub const FAVORITES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedServer {
    /// The address as typed, normalized (`JoinTarget::address`).
    pub address: String,
    /// The invite when the host's key is known, so joining from the list is
    /// verified even before a pin exists.
    #[serde(default)]
    pub invite: Option<String>,
    /// The name the server gave when last seen.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub favorite: bool,
    /// Unix seconds of the last join.
    #[serde(default)]
    pub last_joined: u64,
}
impl SavedServer {
    /// What the list joins: the invite when there is one.
    pub fn target(&self) -> &str {
        self.invite.as_deref().unwrap_or(&self.address)
    }
}

/// Which host a join must reach, strongest evidence first: an invite's key
/// (just copied from the host), the certificate pinned on an earlier join,
/// then a LAN listing's certificate. LAN listings are unsigned broadcast
/// replies that anyone on the network can send for any address, so one may
/// stand in for a first join but never overrides a pin. The flag says the
/// pin came from the saved pins, the only pin a failed join may forget.
pub fn join_pin(
    invite: Option<bri_net::invite::HostKey>,
    saved: Option<Vec<u8>>,
    lan: Option<&Vec<u8>>,
) -> (bri_net::client::HostPin, bool) {
    use bri_net::client::HostPin;
    match (invite, saved, lan) {
        (Some(key), _, _) => (HostPin::Key(key), false),
        (None, Some(certificate), _) => (HostPin::Certificate(certificate), true),
        (None, None, Some(certificate)) => (HostPin::Certificate(certificate.clone()), false),
        (None, None, None) => (HostPin::FirstUse, false),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedServers {
    pub servers: Vec<SavedServer>,
}

impl SavedServers {
    pub fn load(path: &Path) -> Self {
        crate::app::read_small_json(path).unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        bri_files::replace(path, &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
    pub fn find(&self, address: &str) -> Option<&SavedServer> {
        self.servers.iter().find(|s| {
            s.address.eq_ignore_ascii_case(address) || s.target().eq_ignore_ascii_case(address)
        })
    }
    /// Record a successful join.
    pub fn joined(&mut self, address: &str, invite: Option<String>, name: &str, now: u64) {
        let favorite = self.find(address).is_some_and(|s| s.favorite);
        self.servers
            .retain(|s| !s.address.eq_ignore_ascii_case(address));
        self.servers.insert(
            0,
            SavedServer {
                address: address.to_string(),
                invite,
                name: name.to_string(),
                favorite,
                last_joined: now,
            },
        );
        self.trim();
    }
    /// Star or unstar a server; starring one never joined adds it.
    pub fn toggle_favorite(&mut self, address: &str, invite: Option<String>, name: &str) -> bool {
        let starred = match self
            .servers
            .iter_mut()
            .find(|s| s.address.eq_ignore_ascii_case(address))
        {
            Some(server) => {
                server.favorite = !server.favorite;
                if invite.is_some() {
                    server.invite = invite;
                }
                server.favorite
            }
            None => {
                self.servers.push(SavedServer {
                    address: address.to_string(),
                    invite,
                    name: name.to_string(),
                    favorite: true,
                    last_joined: 0,
                });
                true
            }
        };
        self.trim();
        starred
    }
    /// Favourites first (in the order starred), then recent joins, newest
    /// first; older recent entries and excess favourites are dropped.
    fn trim(&mut self) {
        let (mut favorites, mut recent): (Vec<_>, Vec<_>) =
            self.servers.drain(..).partition(|s| s.favorite);
        favorites.truncate(FAVORITES);
        recent.sort_by(|a, b| b.last_joined.cmp(&a.last_joined));
        recent.truncate(RECENT);
        self.servers = favorites.into_iter().chain(recent).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn favourites_stay_and_recent_joins_roll_over() {
        let mut saved = SavedServers::default();
        for i in 0..15u64 {
            saved.joined(&format!("host{i}.example.com:28000"), None, "Server", i);
        }
        assert_eq!(saved.servers.len(), RECENT);
        assert_eq!(saved.servers[0].address, "host14.example.com:28000");
        assert!(saved.toggle_favorite("host5.example.com:28000", None, "Old"));
        for i in 15..30u64 {
            saved.joined(&format!("host{i}.example.com:28000"), None, "Server", i);
        }
        assert_eq!(
            saved.servers[0].address, "host5.example.com:28000",
            "favourite first"
        );
        assert_eq!(saved.servers.len(), RECENT + 1);
        // Joining a favourite keeps its star; unstarring lets it age out.
        saved.joined(
            "HOST5.example.com:28000",
            Some("bri://x/y".into()),
            "Renamed",
            40,
        );
        let five = saved.find("host5.example.com:28000").unwrap();
        assert!(five.favorite && five.name == "Renamed" && five.target() == "bri://x/y");
        assert!(saved.find("bri://x/y").is_some(), "found by invite too");
        assert!(!saved.toggle_favorite("HOST5.example.com:28000", None, ""));
        assert_eq!(saved.servers.len(), RECENT);
    }
    #[test]
    fn a_lan_listing_never_overrides_a_saved_pin() {
        use bri_net::client::HostPin;
        let (saved, spoofed) = (b"real host".to_vec(), b"someone on the LAN".to_vec());
        // Anyone on the network can answer the LAN query for any address.
        assert_eq!(
            join_pin(None, Some(saved.clone()), Some(&spoofed)),
            (HostPin::Certificate(saved.clone()), true)
        );
        // First joins may take the listing, but a failure there must not
        // forget anything.
        assert_eq!(
            join_pin(None, None, Some(&spoofed)),
            (HostPin::Certificate(spoofed.clone()), false)
        );
        let key = bri_net::invite::host_key(b"invite");
        assert_eq!(
            join_pin(Some(key), Some(saved), None),
            (HostPin::Key(key), false)
        );
        assert_eq!(join_pin(None, None, None), (HostPin::FirstUse, false));
    }

    #[test]
    fn saved_servers_round_trip_through_their_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("servers.json");
        let mut saved = SavedServers::default();
        saved.toggle_favorite("play.example.com:28000", None, "Play");
        saved.save(&path).unwrap();
        assert_eq!(SavedServers::load(&path), saved);
        assert_eq!(
            SavedServers::load(&dir.path().join("missing.json")),
            SavedServers::default()
        );
    }

    /// A list this version cannot read (a field it does not know, a torn
    /// edit) is moved aside before the next save writes a new one, and the
    /// player is told: favourites are never wiped without a copy.
    #[test]
    fn an_unreadable_server_list_is_kept_before_it_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("servers.json");
        let mut saved = SavedServers::default();
        saved.toggle_favorite("play.example.com:28000", None, "Play");
        let mut newer = serde_json::to_value(&saved).unwrap();
        newer["from_a_newer_version"] = true.into();
        let original = serde_json::to_vec(&newer).unwrap();
        std::fs::write(&path, &original).unwrap();
        let mut loaded = SavedServers::load(&path);
        assert_eq!(loaded, SavedServers::default());
        loaded.joined("other.example.com:28000", None, "Other", 1);
        loaded.save(&path).unwrap();
        let kept: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("servers.damaged-")
            })
            .collect();
        assert_eq!(kept.len(), 1, "the old list was not kept");
        assert_eq!(std::fs::read(&kept[0]).unwrap(), original);
        let told = crate::app::take_damaged_files(dir.path());
        assert!(
            told.iter()
                .any(|(file, copy)| file == &path && copy == &kept[0])
        );
    }
}
