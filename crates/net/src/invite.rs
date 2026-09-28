//! What a player types or pastes to join a game, and the invite a host
//! copies for friends.
//!
//! Accepted forms:
//! - an IP address or host name with an optional port (`203.0.113.10`,
//!   `play.example.com:28001`, `[2001:db8::1]:28000`), trusted on first use;
//! - an invite, `bri://203.0.113.10:28000/<key>`, which also carries the
//!   host's key (the start of its certificate's SHA-256), so the first join
//!   is already verified and a changed pin is replaced by the invite.
//!
//! Join codes resolved through a rendezvous service would be another
//! [`JoinTarget`] variant resolving to the same [`Route`]s
//! (`docs/architecture/hosting.md`).
use anyhow::{Context, Result, ensure};
use sha2::Digest;
use std::net::{IpAddr, SocketAddr};

/// The game's UDP port when an address leaves it out, as Torque's was.
pub const DEFAULT_PORT: u16 = 28000;
/// Invite scheme. Registering it as a Windows URL handler is future work.
pub const SCHEME: &str = "bri://";
/// Bytes of the certificate's SHA-256 an invite carries (128 bits, 26
/// base32 characters): far beyond any second-preimage search.
pub const KEY_BYTES: usize = 16;
pub type HostKey = [u8; KEY_BYTES];

const HINT: &str = "Enter a server address, like 203.0.113.10, play.example.com:28001 or an invite starting with bri://";

/// The key that identifies a host: the start of its certificate's SHA-256.
pub fn host_key(certificate: &[u8]) -> HostKey {
    let digest = sha2::Sha256::digest(certificate);
    let mut key = [0; KEY_BYTES];
    key.copy_from_slice(&digest[..KEY_BYTES]);
    key
}

/// Something a player asked to join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinTarget {
    /// A host reached directly: an IP or host name and port, with the host's
    /// key when it came from an invite.
    Direct {
        host: String,
        port: u16,
        key: Option<HostKey>,
    },
}

/// A resolved way to reach a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    pub address: SocketAddr,
    pub key: Option<HostKey>,
}

impl JoinTarget {
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        if let Some(rest) = strip_scheme(text) {
            let (address, key) = rest.trim_end_matches('/').rsplit_once('/').context(HINT)?;
            let key = decode_key(key).context(
                "This invite is damaged (its key is not valid). Ask the host to copy it again.",
            )?;
            let (host, port) = parse_address(address)?;
            return Ok(Self::Direct {
                host,
                port,
                key: Some(key),
            });
        }
        let (host, port) = parse_address(text)?;
        Ok(Self::Direct {
            host,
            port,
            key: None,
        })
    }
    /// Where this target is remembered: pins and saved servers are keyed by
    /// the address as typed (normalized), not by the resolved IP.
    pub fn address(&self) -> String {
        match self {
            Self::Direct { host, port, .. } => join_host_port(host, *port),
        }
    }
    pub fn key(&self) -> Option<HostKey> {
        match self {
            Self::Direct { key, .. } => *key,
        }
    }
    /// Look up how to reach the host (DNS for host names).
    pub async fn resolve(&self) -> Result<Route> {
        match self {
            Self::Direct { host, port, key } => {
                let address = if let Ok(ip) = host.parse::<IpAddr>() {
                    SocketAddr::new(ip, *port)
                } else {
                    tokio::net::lookup_host((host.as_str(), *port))
                        .await
                        .ok()
                        .and_then(|mut found| found.find(|a| a.is_ipv4()).or_else(|| found.next()))
                        .with_context(|| {
                            format!("Could not find a server called {host}. Check the address for typos.")
                        })?
                };
                Ok(Route { address, key: *key })
            }
        }
    }
}

impl std::fmt::Display for JoinTarget {
    /// The shareable form: an invite when the key is known.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct { key: Some(key), .. } => {
                write!(f, "{SCHEME}{}/{}", self.address(), encode_key(key))
            }
            Self::Direct { .. } => f.write_str(&self.address()),
        }
    }
}

/// The invite a host shares: its public address and its certificate's key.
pub fn invite(address: SocketAddr, certificate: &[u8]) -> String {
    JoinTarget::Direct {
        host: address.ip().to_string(),
        port: address.port(),
        key: Some(host_key(certificate)),
    }
    .to_string()
}

fn strip_scheme(text: &str) -> Option<&str> {
    let head = text.get(..SCHEME.len())?;
    head.eq_ignore_ascii_case(SCHEME).then(|| &text[SCHEME.len()..])
}

fn join_host_port(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// An IP or host name with an optional port.
fn parse_address(text: &str) -> Result<(String, u16)> {
    if let Ok(address) = text.parse::<SocketAddr>() {
        ensure!(address.port() > 0, "{HINT}");
        return Ok((address.ip().to_string(), address.port()));
    }
    let bare = text.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return Ok((ip.to_string(), DEFAULT_PORT));
    }
    let (host, port) = match text.rsplit_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>().ok().filter(|p| *p > 0).context(HINT)?,
        ),
        None => (text, DEFAULT_PORT),
    };
    let valid = !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    ensure!(valid, "{HINT}");
    Ok((host.to_ascii_lowercase(), port))
}

/// RFC 4648 base32, lower case, unpadded: no characters that URLs, chat or
/// fonts mangle, and case-insensitive when typed.
const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

fn encode_key(key: &HostKey) -> String {
    let mut out = String::new();
    let (mut buffer, mut bits) = (0u32, 0);
    for &byte in key {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn decode_key(text: &str) -> Option<HostKey> {
    (text.len() == (KEY_BYTES * 8).div_ceil(5)).then_some(())?;
    let mut bytes = Vec::with_capacity(KEY_BYTES);
    let (mut buffer, mut bits) = (0u32, 0);
    for c in text.bytes() {
        let value = ALPHABET
            .iter()
            .position(|&a| a == c.to_ascii_lowercase())? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    // Exactly the key, with no stray trailing bits.
    (buffer == 0).then_some(())?;
    bytes.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn direct(host: &str, port: u16) -> JoinTarget {
        JoinTarget::Direct {
            host: host.into(),
            port,
            key: None,
        }
    }
    #[test]
    fn addresses_accept_ips_and_host_names_with_or_without_port() {
        let parsed = |text| JoinTarget::parse(text).unwrap();
        assert_eq!(parsed(" 203.0.113.10:28001 "), direct("203.0.113.10", 28001));
        assert_eq!(parsed("100.64.1.2"), direct("100.64.1.2", 28000));
        assert_eq!(parsed("[2001:db8::1]"), direct("2001:db8::1", 28000));
        assert_eq!(parsed("[2001:db8::1]:28005"), direct("2001:db8::1", 28005));
        assert_eq!(parsed("Play.Example.com"), direct("play.example.com", 28000));
        assert_eq!(parsed("play.example.com:28001"), direct("play.example.com", 28001));
        assert_eq!(parsed("localhost"), direct("localhost", 28000));
        assert_eq!(parsed("[2001:db8::1]:28005").address(), "[2001:db8::1]:28005");
        for bad in ["", "play example.com", "host:notaport", "host:0", "-bad.com", "a..b", "1.2.3.4:0"] {
            let error = JoinTarget::parse(bad).unwrap_err().to_string();
            assert!(error.contains("play.example.com"), "{bad}: {error}");
        }
    }
    #[test]
    fn invites_round_trip_and_carry_the_host_key() {
        let certificate = b"not really a certificate";
        let text = invite("203.0.113.10:28000".parse().unwrap(), certificate);
        assert!(text.starts_with("bri://203.0.113.10:28000/"), "{text}");
        assert_eq!(text.len(), "bri://203.0.113.10:28000/".len() + 26);
        let target = JoinTarget::parse(&text).unwrap();
        assert_eq!(target.key(), Some(host_key(certificate)));
        assert_eq!(target.address(), "203.0.113.10:28000");
        assert_eq!(target.to_string(), text);
        // Pasted from chat: upper case, trailing slash, spaces.
        let shouted = format!(" {}/ ", text.to_ascii_uppercase());
        assert_eq!(JoinTarget::parse(&shouted).unwrap(), target);
        let v6 = invite("[2001:db8::1]:28000".parse().unwrap(), certificate);
        assert_eq!(JoinTarget::parse(&v6).unwrap().address(), "[2001:db8::1]:28000");
    }
    #[test]
    fn damaged_invites_are_named() {
        let text = invite("203.0.113.10:28000".parse().unwrap(), b"cert");
        for damaged in [&text[..text.len() - 1], &format!("{text}a"), &text.replace('a', "1")] {
            if damaged == text {
                continue;
            }
            let error = JoinTarget::parse(damaged).unwrap_err().to_string();
            assert!(error.contains("damaged") || error.contains("bri://"), "{damaged}: {error}");
        }
        assert!(JoinTarget::parse("bri://").is_err());
    }
    #[tokio::test]
    async fn literal_addresses_resolve_without_dns() {
        let route = JoinTarget::parse("127.0.0.1:28001").unwrap().resolve().await.unwrap();
        assert_eq!(route.address, "127.0.0.1:28001".parse().unwrap());
    }
}
