//! Opening a host to the internet and telling the host, in plain words,
//! whether friends can reach it.
//!
//! No service of ours is involved: the router is asked to forward the game
//! port (UPnP IGD, then NAT-PMP), a public STUN server reports the public
//! address, and the host then probes its own public address over the game
//! port. A router that forwards the port and loops the probe back proves the
//! path works; one that does not loop back leaves a "should work" verdict.
//! Hosts behind a shared address (carrier-grade or double NAT) are told so,
//! because no router setting on their side helps.
use crate::{
    client::{HostPin, probe},
    invite::{host_key, invite},
    upnp::is_public,
};
use std::{
    net::{IpAddr, SocketAddr, UdpSocket},
    time::Duration,
};

/// Router forwards held open while a host runs. Dropping removes them.
pub enum Forward {
    Upnp(crate::upnp::PortMapping),
    NatPmp(crate::natpmp::Mapping),
}
impl Forward {
    /// Blocking: extend the lease.
    pub fn renew(&mut self) -> anyhow::Result<()> {
        match self {
            Self::Upnp(mapping) => mapping.renew(),
            Self::NatPmp(mapping) => mapping.renew(),
        }
    }
    fn method(&self) -> &'static str {
        match self {
            Self::Upnp(_) => "UPnP",
            Self::NatPmp(_) => "NAT-PMP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The host answered at its public address: confirmed.
    Reachable,
    /// The router opened the port, but could not be tested from inside.
    Likely,
    /// Behind a shared public address: outside players cannot connect.
    SharedAddress,
    /// The router did not open the port; the player must forward it.
    NeedsForward,
    /// The public address could not be found (offline, or STUN blocked).
    Unknown,
}

/// Everything the check learned, for the host player and logs.
#[derive(Debug, Clone)]
pub struct Report {
    pub verdict: Verdict,
    pub port: u16,
    /// This computer's address on the home network.
    pub local_ip: Option<IpAddr>,
    /// The address friends connect to, when known.
    pub public: Option<SocketAddr>,
    /// The router's own outside address, when it reported one.
    pub router_ip: Option<IpAddr>,
    /// How the port was opened ("UPnP", "NAT-PMP").
    pub method: Option<&'static str>,
    /// Why automatic forwarding failed.
    pub forward_error: Option<String>,
    /// The invite to share, when there is a public address.
    pub invite: Option<String>,
}

impl Report {
    /// Plain sentences for the host player, most important first.
    pub fn lines(&self) -> Vec<String> {
        let port = self.port;
        let invite = self.invite.as_deref().unwrap_or("");
        let here = self
            .local_ip
            .map_or("this PC".to_string(), |ip| format!("this PC ({ip})"));
        match self.verdict {
            Verdict::Reachable => vec![
                "Friends can join you over the internet: your game answered at your public address.".into(),
                format!("Share your invite: {invite}"),
            ],
            Verdict::Likely => vec![
                format!(
                    "Your router opened port {port} ({}). Friends should be able to join; your router does not let the game test this from inside your home.",
                    self.method.unwrap_or("automatically")
                ),
                format!("Share your invite: {invite}"),
            ],
            Verdict::SharedAddress => vec![
                "Friends outside your home probably cannot join: your internet provider (or a second router) shares one public address between several homes.".into(),
                "Ask your provider for a public IP address, or play together through a virtual LAN tool such as Tailscale or ZeroTier. Players on your own network can still join.".into(),
            ],
            Verdict::NeedsForward => {
                let mut lines = vec![format!(
                    "Your router did not open port {port} automatically, so friends outside your home cannot join yet."
                )];
                lines.push(format!(
                    "Turn on UPnP in your router's settings and host again, or forward UDP port {port} to {here}."
                ));
                if !invite.is_empty() {
                    lines.push(format!("Once that is done, share your invite: {invite}"));
                }
                lines
            }
            Verdict::Unknown => vec![
                "Could not reach the internet to find your public address. Players on your own network can still join.".into(),
            ],
        }
    }
}

/// How long the self-probe waits for the host's own public address.
const SELF_PROBE: Duration = Duration::from_secs(3);

/// Open `port` on the router and check whether the host serving
/// `certificate` on it can be reached. Returns the forward to hold for as
/// long as the host runs.
pub async fn open_and_check(port: u16, certificate: Vec<u8>) -> (Report, Option<Forward>) {
    let local_ip = local_ip();
    let forward = tokio::task::spawn_blocking(move || open_forward(port, local_ip)).await;
    let (forward, forward_error) = match forward {
        Ok(Ok(forward)) => (Some(forward), None),
        Ok(Err(error)) => (None, Some(error)),
        Err(error) => (None, Some(error.to_string())),
    };
    let router_ip = forward.as_ref().and_then(|f| match f {
        Forward::Upnp(m) => m.external_ip,
        Forward::NatPmp(m) => m.external_ip.map(IpAddr::V4),
    });
    let external_port = match &forward {
        Some(Forward::NatPmp(m)) if m.external_port != 0 => m.external_port,
        _ => port,
    };
    let seen = crate::stun::public_address(crate::stun::SERVERS, Duration::from_secs(3))
        .await
        .ok()
        .map(|a| a.ip());
    let public_ip = seen.or(router_ip.filter(|ip| is_public(*ip)));
    let public = public_ip.map(|ip| SocketAddr::new(ip, external_port));
    let reached = match public {
        Some(address) => probe(address, &HostPin::Key(host_key(&certificate)), SELF_PROBE)
            .await
            .is_ok(),
        None => false,
    };
    let verdict = verdict(reached, public_ip, router_ip, forward.is_some());
    let report = Report {
        verdict,
        port,
        local_ip,
        public,
        router_ip,
        method: forward.as_ref().map(Forward::method),
        forward_error,
        invite: public.map(|address| invite(address, &certificate)),
    };
    (report, forward)
}

fn verdict(
    reached: bool,
    public_ip: Option<IpAddr>,
    router_ip: Option<IpAddr>,
    forwarded: bool,
) -> Verdict {
    if reached {
        return Verdict::Reachable;
    }
    let Some(public_ip) = public_ip else {
        // The router's outside address is private and nothing else answered.
        return if router_ip.is_some_and(|ip| !is_public(ip)) {
            Verdict::SharedAddress
        } else {
            Verdict::Unknown
        };
    };
    // The router's outside address is not the one the internet sees: another
    // router or the provider's NAT sits in front of it.
    let shared = router_ip.is_some_and(|ip| !is_public(ip) || ip != public_ip);
    if shared {
        Verdict::SharedAddress
    } else if forwarded {
        Verdict::Likely
    } else {
        Verdict::NeedsForward
    }
}

/// Ask the router to forward `port`: UPnP first, then NAT-PMP.
fn open_forward(port: u16, local_ip: Option<IpAddr>) -> Result<Forward, String> {
    let upnp = match crate::upnp::PortMapping::open(&[port]) {
        Ok(mapping) => return Ok(Forward::Upnp(mapping)),
        Err(error) => format!("{error:#}"),
    };
    let gateway = crate::natpmp::default_gateway().ok().or_else(|| {
        // Without a route table, guess the usual router address.
        match local_ip {
            Some(IpAddr::V4(ip)) => {
                let [a, b, c, _] = ip.octets();
                Some(std::net::Ipv4Addr::new(a, b, c, 1))
            }
            _ => None,
        }
    });
    match gateway.map(|g| crate::natpmp::Mapping::open(g, port, crate::upnp::LEASE_SECONDS)) {
        Some(Ok(mapping)) => Ok(Forward::NatPmp(mapping)),
        Some(Err(natpmp)) => Err(format!("{upnp}; {natpmp:#}")),
        None => Err(upnp),
    }
}

/// This computer's address on the network that leads to the internet.
pub fn local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("192.0.2.1", 9)).ok()?;
    Some(socket.local_addr().ok()?.ip()).filter(|ip| !ip.is_unspecified())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }
    #[test]
    fn verdicts_follow_the_evidence() {
        let public = Some(ip("203.0.113.10"));
        assert_eq!(verdict(true, public, None, false), Verdict::Reachable);
        assert_eq!(verdict(false, public, public, true), Verdict::Likely);
        assert_eq!(verdict(false, public, None, false), Verdict::NeedsForward);
        // The router's outside address is private, or differs from the one
        // the internet sees: someone else's NAT is in front.
        assert_eq!(verdict(false, public, Some(ip("100.72.1.2")), true), Verdict::SharedAddress);
        assert_eq!(verdict(false, public, Some(ip("198.51.100.1")), true), Verdict::SharedAddress);
        assert_eq!(verdict(false, None, Some(ip("10.0.0.2")), true), Verdict::SharedAddress);
        assert_eq!(verdict(false, None, None, false), Verdict::Unknown);
    }
    #[test]
    fn every_verdict_reads_as_plain_advice() {
        for verdict in [
            Verdict::Reachable,
            Verdict::Likely,
            Verdict::SharedAddress,
            Verdict::NeedsForward,
            Verdict::Unknown,
        ] {
            let report = Report {
                verdict,
                port: 28000,
                local_ip: Some(ip("192.168.1.23")),
                public: Some("203.0.113.10:28000".parse().unwrap()),
                router_ip: None,
                method: Some("UPnP"),
                forward_error: None,
                invite: Some("bri://203.0.113.10:28000/key".into()),
            };
            let text = report.lines().join(" ");
            assert!(!text.is_empty());
            match verdict {
                Verdict::Reachable | Verdict::Likely => assert!(text.contains("bri://"), "{text}"),
                Verdict::NeedsForward => {
                    assert!(text.contains("UDP port 28000") && text.contains("192.168.1.23"), "{text}")
                }
                _ => assert!(!text.contains("bri://"), "{text}"),
            }
        }
    }
}
