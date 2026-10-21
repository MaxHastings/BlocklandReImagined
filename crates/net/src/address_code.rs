//! A host's public address and port as a short code a friend can type.
//!
//! There is no lookup service: the code *is* the address. 48 bits (the IPv4
//! address and the port) are written as ten letters and digits plus one check
//! character, grouped `XXXX-XXXX-XXX`. The alphabet leaves out `I`, `L`, `O`
//! and `U`, and typing is forgiving: case, spaces and dashes do not matter,
//! and `O`, `I` and `L` read as `0`, `1` and `1`. The check character is a
//! weighted sum with odd weights, so any single wrong character is caught.
//!
//! A code carries no host key, so the first join trusts the certificate the
//! host presents and pins it, as typing an address does.
use crate::upnp::is_public;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const DATA_CHARS: usize = 10;
const CODE_CHARS: usize = DATA_CHARS + 1;

/// The code for `address`, or `None` for an address a friend could not reach
/// anyway (not public IPv4).
pub fn encode(address: SocketAddr) -> Option<String> {
    let IpAddr::V4(ip) = address.ip() else {
        return None;
    };
    if !is_public(address.ip()) {
        return None;
    }
    let value = (u64::from(u32::from(ip)) << 16) | u64::from(address.port());
    let mut digits = [0u8; CODE_CHARS];
    for (i, digit) in digits.iter_mut().take(DATA_CHARS).enumerate() {
        *digit = ((value >> (5 * (DATA_CHARS - 1 - i))) & 31) as u8;
    }
    digits[DATA_CHARS] = check(&digits[..DATA_CHARS]);
    let text: String = digits
        .iter()
        .map(|&d| ALPHABET[d as usize] as char)
        .collect();
    Some(format!("{}-{}-{}", &text[..4], &text[4..8], &text[8..]))
}

/// The address a typed code names, if `text` is a code: eleven characters in
/// groups of 4-4-3 (the dashes are required, so a host name is never
/// mistaken for one), a matching check character, and a public address.
pub fn decode(text: &str) -> Option<SocketAddr> {
    let groups: Vec<&str> = text.trim().split(['-', ' ']).collect();
    let [a, b, c] = groups[..] else {
        return None;
    };
    (a.len() == 4 && b.len() == 4 && c.len() == 3).then_some(())?;
    let mut digits = [0u8; CODE_CHARS];
    for (slot, ch) in digits
        .iter_mut()
        .zip(a.chars().chain(b.chars()).chain(c.chars()))
    {
        *slot = digit(ch)?;
    }
    (digits[DATA_CHARS] == check(&digits[..DATA_CHARS])).then_some(())?;
    let value = digits[..DATA_CHARS]
        .iter()
        .fold(0u64, |acc, &d| (acc << 5) | u64::from(d));
    // Ten characters hold 50 bits; the address and port use 48.
    (value >> 48 == 0).then_some(())?;
    let port = (value & 0xffff) as u16;
    let ip = Ipv4Addr::from((value >> 16) as u32);
    (port != 0 && is_public(IpAddr::V4(ip))).then(|| SocketAddr::new(IpAddr::V4(ip), port))
}

fn digit(ch: char) -> Option<u8> {
    let ch = match ch.to_ascii_uppercase() {
        'O' => '0',
        'I' | 'L' => '1',
        other => other,
    };
    ALPHABET
        .iter()
        .position(|&a| a as char == ch)
        .map(|p| p as u8)
}

/// A weighted sum with odd weights: a wrong character changes the sum by a
/// nonzero multiple of an odd number, which is never 0 modulo 32.
fn check(digits: &[u8]) -> u8 {
    let sum: u32 = digits
        .iter()
        .enumerate()
        .map(|(i, &d)| u32::from(d) * (2 * i as u32 + 1))
        .sum();
    (sum % 32) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    fn addr(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }
    #[test]
    fn a_code_names_its_address_and_forgives_typing() {
        for text in ["203.0.113.10:28000", "8.8.4.4:1", "198.51.100.200:65535"] {
            let code = encode(addr(text)).unwrap();
            assert_eq!(code.len(), 13, "{code}");
            assert_eq!(decode(&code), Some(addr(text)), "{code}");
            assert_eq!(decode(&code.to_ascii_lowercase()), Some(addr(text)));
            assert_eq!(decode(&code.replace('-', " ")), Some(addr(text)));
        }
        let code = encode(addr("203.0.113.10:28000")).unwrap();
        let confused = code.replace('0', "O").replace('1', "l");
        assert_eq!(decode(&confused), Some(addr("203.0.113.10:28000")));
    }
    #[test]
    fn any_single_wrong_character_is_refused() {
        let code = encode(addr("203.0.113.10:28000")).unwrap();
        for at in (0..code.len()).filter(|&i| code.as_bytes()[i] != b'-') {
            for &other in ALPHABET {
                if other == code.as_bytes()[at] {
                    continue;
                }
                let mut typed = code.clone().into_bytes();
                typed[at] = other;
                let typed = String::from_utf8(typed).unwrap();
                assert_eq!(decode(&typed), None, "{typed} for {code}");
            }
        }
    }
    #[test]
    fn only_public_ipv4_addresses_have_codes_and_names_are_not_codes() {
        for text in ["192.168.1.5:28000", "127.0.0.1:28000", "100.64.0.1:28000"] {
            assert_eq!(encode(addr(text)), None, "{text}");
        }
        assert_eq!(encode(addr("[2001:db8::1]:28000")), None);
        for text in [
            "",
            "localhost",
            "play-example-com",
            "abcd-efgh-ijk",
            "K7Q-4MZ",
        ] {
            assert_eq!(decode(text), None, "{text}");
        }
    }
}
