//! Plain words for why a connection failed or ended.
//!
//! Transport, join and host errors reach the client as technical text (the
//! QUIC close frame, `Join rejected: …`, anyhow chains). The Connection Failed
//! dialog shows [`explain`] of that text; the raw text stays in
//! `ConnectionState::Failed` and the log for bug reports.

/// Firewall ports a direct-IP host needs open (game and discovery).
const PORTS: &str = "UDP ports 28000 and 28050";

/// The player-facing explanation of `reason`. Text that is not recognised is
/// returned unchanged, so already plain messages pass through.
pub fn explain(reason: &str) -> String {
    // Add-On mismatches have their own Can't Join dialog, which reads the raw
    // refusal; keep that text intact for it.
    if reason.contains("Your content does not match the server: ") {
        return reason.to_string();
    }
    // The server's own close reason (quinn: "closed by peer: <reason> (code N)").
    if let Some(rest) = reason.split("closed by peer: ").nth(1) {
        let said = rest
            .rsplit_once(" (code ")
            .map_or(rest, |(said, _)| said)
            .trim();
        return match said {
            "Session ended" | "Server shutdown" => "The host closed the server.".into(),
            "Administration disconnect" => "An admin removed you from the server.".into(),
            "Reliable backlog exceeded" | "Host state exceeds transfer budget" => {
                "Your connection couldn't keep up with the server, so it disconnected you. \
                 Try joining again."
                    .into()
            }
            "Administration state unavailable" => {
                "The server had a problem and disconnected everyone. Try joining again.".into()
            }
            // Admin disconnects carry a message written for the player.
            said if !said.is_empty() && !said.chars().all(|c| c.is_ascii_digit()) => {
                said.to_string()
            }
            _ => "The server closed the connection.".into(),
        };
    }
    // A rejection the server wrote for the player ("You are banned …").
    if let Some(said) = reason.strip_prefix("Join rejected: ")
        && said.starts_with("You ")
    {
        return said.to_string();
    }
    for (needle, text) in [
        (
            "Incompatible protocol version",
            "This server is running a different version of Blockland ReImagined. \
             You and the host need the same version to play together."
                .to_string(),
        ),
        (
            "Server is full",
            "The server is full. Try again when someone leaves.".into(),
        ),
        (
            "connection is banned",
            "You are banned from this server.".into(),
        ),
        (
            "No Blockland ReImagined host answered at that address",
            format!(
                "No server answered at that address. Check the IP and port, make sure the \
                 host is running, and that their firewall or router allows {PORTS}."
            ),
        ),
        (
            "Password authentication is not connected yet",
            "This server needs a password, and joining password-protected servers isn't \
             supported yet."
                .into(),
        ),
        (
            "The server join password is not connected yet",
            "Server passwords aren't supported yet. Clear the password to start the server.".into(),
        ),
        (
            "Connection/content preparation timed out",
            "Joining took too long and was stopped. The server may be busy or unreachable; \
             try again."
                .into(),
        ),
        (
            "timed out",
            format!(
                "The connection to the server timed out. The server may have stopped, or a \
                 firewall or router may be blocking {PORTS}."
            ),
        ),
        ("reset by peer", "The server closed the connection.".into()),
        (
            "Server map has no supported native render bundle yet",
            "This server is using a map your game can't show yet.".into(),
        ),
        (
            "Could not load the new map",
            "The server changed maps, and the new map couldn't be loaded.".into(),
        ),
    ] {
        if reason.contains(needle) {
            return text;
        }
    }
    reason.to_string()
}

#[cfg(test)]
mod tests {
    use super::explain;

    #[test]
    fn server_close_reasons_read_as_plain_words() {
        assert_eq!(
            explain("Connection closed: closed by peer: Server shutdown (code 0)"),
            "The host closed the server."
        );
        assert_eq!(
            explain("Connection closed: closed by peer: Administration disconnect (code 0)"),
            "An admin removed you from the server."
        );
        // A message the server wrote for the player is shown as written.
        assert_eq!(
            explain(
                "Connection closed: closed by peer: You were banned from this server for \
                 10 minutes. Reason: spam (code 0)"
            ),
            "You were banned from this server for 10 minutes. Reason: spam"
        );
        assert_eq!(
            explain("Connection closed: closed by peer: 7"),
            "The server closed the connection."
        );
    }

    #[test]
    fn join_failures_say_what_to_do() {
        assert!(explain("Join rejected: Incompatible protocol version").contains("same version"));
        assert!(explain("Join rejected: Server is full").starts_with("The server is full"));
        assert_eq!(
            explain("Join rejected: connection is banned"),
            "You are banned from this server."
        );
        assert_eq!(
            explain("Join rejected: You are banned from this server for 5 minutes."),
            "You are banned from this server for 5 minutes."
        );
        assert!(explain("No Blockland ReImagined host answered at that address").contains("28050"));
        assert!(explain("Connection closed: timed out").contains("timed out"));
        assert!(explain("Connection/content preparation timed out").contains("took too long"));
    }

    #[test]
    fn add_on_refusals_and_unknown_text_pass_through() {
        let refusal = "Join rejected: Your content does not match the server: server has \
                       a:b/c 1.0.0 (abc), you do not";
        assert_eq!(explain(refusal), refusal);
        assert_eq!(
            explain("Enter a server address."),
            "Enter a server address."
        );
    }
}
