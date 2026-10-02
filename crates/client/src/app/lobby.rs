//! Finding games: LAN hosts and queries, the update check and the firewall fix.
use super::*;

/// Finding games: LAN hosts and queries, the update check and the firewall fix.
pub(super) struct Lobby {
    /// The start-up release check's answer, until it is shown.
    pub(super) update_check: Option<mpsc::Receiver<crate::updates::Newer>>,
    /// LAN listings from the last discovery query: address -> certificate.
    pub(super) lan_hosts: BTreeMap<String, Vec<u8>>,
    pub(super) lan_query: Option<mpsc::Receiver<JoinList>>,
    /// The elevated firewall helper's outcome.
    pub(super) firewall_fix: Option<mpsc::Receiver<Result<(), String>>>,
}
