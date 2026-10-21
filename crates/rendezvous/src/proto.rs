//! What game clients and the rendezvous service say to each other, over one
//! bidirectional QUIC stream per request. Frames are a little-endian `u32`
//! length followed by that many bytes of JSON, at most [`MAX_FRAME`].
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::net::SocketAddr;

/// Rendezvous protocol version, independent of the game protocol.
pub const VERSION: u32 = 1;
/// Default UDP port of the service.
pub const DEFAULT_PORT: u16 = 28100;
pub const MAX_FRAME: usize = 16 * 1024;
/// Local addresses a peer may publish.
pub const MAX_LOCALS: usize = 8;
/// A host key: the first 16 bytes of the SHA-256 of its certificate.
pub type Key = [u8; 16];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Request {
    /// A host asks for a code. The stream stays open for as long as the host
    /// wants the code; the service sends [`Answer::Joiner`] on it.
    Host {
        version: u32,
        key: Key,
        /// The host's addresses on its own network (same-network joiners).
        locals: Vec<SocketAddr>,
        name: String,
    },
    /// A player asks how to reach the host with this code.
    Join {
        version: u32,
        code: String,
        locals: Vec<SocketAddr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Answer {
    /// The host's code, and its game port's address as the service saw it.
    Hosting { code: String, observed: SocketAddr },
    /// Sent to the host: a player is about to connect. Send a packet to each
    /// address from the game port so the router lets the player in.
    Joiner {
        observed: SocketAddr,
        locals: Vec<SocketAddr>,
        /// The relay port assigned to the host for this player.
        relay: Option<SocketAddr>,
    },
    /// Sent to the player: where to try, in order, and whose key to expect.
    Found {
        key: Key,
        name: String,
        /// The host's public (observed) address, then its local ones.
        candidates: Vec<SocketAddr>,
        /// The relay port assigned to the player, if the service relays.
        relay: Option<SocketAddr>,
        observed: SocketAddr,
    },
    /// The request was refused; the text is for the player.
    Refused(String),
}

pub async fn write<T: Serialize>(send: &mut quinn::SendStream, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= MAX_FRAME, "Rendezvous frame too large");
    send.write_all(&(bytes.len() as u32).to_le_bytes()).await?;
    send.write_all(&bytes).await?;
    Ok(())
}

/// Read one frame; `None` when the peer finished the stream.
pub async fn read<T: DeserializeOwned>(receive: &mut quinn::RecvStream) -> Result<Option<T>> {
    let mut length = [0u8; 4];
    match receive.read_exact(&mut length).await {
        Ok(()) => {}
        Err(quinn::ReadExactError::FinishedEarly(0)) => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let length = u32::from_le_bytes(length) as usize;
    ensure!(length <= MAX_FRAME, "Rendezvous frame too large");
    let mut bytes = vec![0; length];
    receive.read_exact(&mut bytes).await?;
    Ok(Some(
        serde_json::from_slice(&bytes).context("Malformed rendezvous frame")?,
    ))
}

/// Crockford base32: no I, L, O or U, so codes read aloud and type cleanly.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Characters in a code (30 bits); shown as `ABC-DEF`.
pub const CODE_LEN: usize = 6;

/// A fresh random code, formatted for display.
pub fn new_code() -> Result<String> {
    let mut bytes = [0u8; CODE_LEN];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("OS randomness failed: {e}"))?;
    let raw: String = bytes
        .iter()
        .map(|b| ALPHABET[(b & 31) as usize] as char)
        .collect();
    Ok(format!("{}-{}", &raw[..3], &raw[3..]))
}

/// A typed code in display form (`ABC-DEF`), or None when `text` is not a
/// code. Case, a space instead of the dash, and the look-alikes O/I/L are
/// forgiven. The dash (or space) is required, so a six-letter host name is
/// never taken for a code.
pub fn parse_code(text: &str) -> Option<String> {
    let text = text.trim();
    let bytes = text.as_bytes();
    if bytes.len() != CODE_LEN + 1 || !matches!(bytes[3], b'-' | b' ') {
        return None;
    }
    let mut code = String::with_capacity(CODE_LEN + 1);
    for (i, &b) in bytes.iter().enumerate() {
        if i == 3 {
            code.push('-');
            continue;
        }
        let c = match b.to_ascii_uppercase() {
            b'O' => b'0',
            b'I' | b'L' => b'1',
            other => other,
        };
        if !ALPHABET.contains(&c) {
            return None;
        }
        code.push(c as char);
    }
    Some(code)
}

/// The key of a certificate: the first 16 bytes of its SHA-256.
pub fn key_of(certificate: &[u8]) -> Key {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(certificate);
    let mut key = [0; 16];
    key.copy_from_slice(&digest[..16]);
    key
}

/// RFC 4648 base32, lower case, unpadded: no characters that URLs, chat or
/// fonts mangle, and case-insensitive when typed.
const KEY_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

pub fn encode_key(key: &Key) -> String {
    let mut out = String::new();
    let (mut buffer, mut bits) = (0u32, 0);
    for &byte in key {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(KEY_ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(KEY_ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn decode_key(text: &str) -> Option<Key> {
    (text.len() == 26).then_some(())?;
    let mut bytes = Vec::with_capacity(16);
    let (mut buffer, mut bits) = (0u32, 0);
    for c in text.bytes() {
        let value = KEY_ALPHABET
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

/// Where a game finds the service and the key it must present:
/// `host[:port]/key`, as printed by `bri-rendezvous` and set in the game's
/// `$pref::Net::JoinCodeService`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceAddress {
    pub host: String,
    pub port: u16,
    pub key: Key,
}
impl ServiceAddress {
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        let (address, key) = text
            .rsplit_once('/')
            .context("A join code service address looks like host:28100/key")?;
        let key = decode_key(key).context("The join code service key is not valid")?;
        let (host, port) = match address.rsplit_once(':') {
            Some((host, port)) if !host.ends_with(':') => (
                host,
                port.parse().context("Invalid join code service port")?,
            ),
            _ => (address, DEFAULT_PORT),
        };
        let host = host.trim_start_matches('[').trim_end_matches(']');
        ensure!(!host.is_empty(), "A join code service address needs a host");
        Ok(Self {
            host: host.to_ascii_lowercase(),
            port,
            key,
        })
    }
}
impl std::fmt::Display for ServiceAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}/{}", self.host, self.port, encode_key(&self.key))
        } else {
            write!(f, "{}:{}/{}", self.host, self.port, encode_key(&self.key))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_addresses_round_trip() {
        let key = key_of(b"cert");
        let text = format!("Codes.Example.com:28100/{}", encode_key(&key));
        let address = ServiceAddress::parse(&text).unwrap();
        assert_eq!(
            (address.host.as_str(), address.port, address.key),
            ("codes.example.com", 28100, key)
        );
        assert_eq!(
            ServiceAddress::parse(&address.to_string()).unwrap(),
            address
        );
        let bare = ServiceAddress::parse(&format!("203.0.113.5/{}", encode_key(&key))).unwrap();
        assert_eq!(bare.port, DEFAULT_PORT);
        assert!(ServiceAddress::parse("codes.example.com:28100").is_err());
        assert!(ServiceAddress::parse("codes.example.com:28100/short").is_err());
    }
    #[test]
    fn codes_are_forgiving_to_type_and_never_host_names() {
        let code = new_code().unwrap();
        assert_eq!(code.len(), 7);
        assert_eq!(parse_code(&code.to_ascii_lowercase()), Some(code.clone()));
        assert_eq!(parse_code(" k7q 4mz "), Some("K7Q-4MZ".into()));
        assert_eq!(parse_code("oil-abc"), Some("011-ABC".into()));
        for not_a_code in [
            "k7q4mz",
            "server",
            "k7q-4m",
            "k7q-4mzz",
            "k7q-4mu",
            "play.example.com",
            "k7q_4mz",
        ] {
            assert_eq!(parse_code(not_a_code), None, "{not_a_code}");
        }
    }
}
