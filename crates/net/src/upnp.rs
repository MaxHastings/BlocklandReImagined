//! Router port forwarding for internet hosts. Asks the home router (UPnP
//! IGD) to forward the game port, so friends can join without manual router
//! setup. Best effort: `reach` falls back to NAT-PMP, then tells the host
//! how to forward the port by hand.
use anyhow::{Context, Result};
use igd_next::{AddPortError, Gateway, PortMappingProtocol, SearchOptions};
use std::{
    net::{IpAddr, SocketAddr, UdpSocket},
    time::Duration,
};

/// Lease requested from the router; renewed well before it lapses so a
/// crashed host does not leave ports open for long.
pub const LEASE_SECONDS: u32 = 3600;
pub const RENEW_EVERY: Duration = Duration::from_secs(20 * 60);
const DESCRIPTION: &str = "Blockland ReImagined";

/// Open UDP forwards on the router. Removed again when dropped.
pub struct PortMapping {
    gateway: Gateway,
    local_ip: IpAddr,
    ports: Vec<u16>,
    lease: u32,
    /// The router's public address, when it reports one.
    pub external_ip: Option<IpAddr>,
}
impl PortMapping {
    /// Blocking: find the router and forward `ports` (same number outside and
    /// inside) to this computer.
    pub fn open(ports: &[u16]) -> Result<Self> {
        let gateway = search().context("No router answered the UPnP search")?;
        // The local address the router sees for this computer.
        let probe = UdpSocket::bind(("0.0.0.0", 0))?;
        probe.connect(gateway.addr)?;
        let local_ip = probe.local_addr()?.ip();
        let mut mapping = Self {
            gateway,
            local_ip,
            ports: Vec::new(),
            lease: LEASE_SECONDS,
            external_ip: None,
        };
        for &port in ports {
            mapping.add(port)?;
            mapping.ports.push(port);
        }
        mapping.external_ip = mapping.gateway.get_external_ip().ok();
        Ok(mapping)
    }
    fn add(&mut self, port: u16) -> Result<()> {
        let local = SocketAddr::new(self.local_ip, port);
        let udp = PortMappingProtocol::UDP;
        match self
            .gateway
            .add_port(udp, port, local, self.lease, DESCRIPTION)
        {
            // Some routers only accept permanent leases; removal on shutdown
            // still cleans them up.
            Err(AddPortError::OnlyPermanentLeasesSupported) => {
                self.lease = 0;
                self.gateway.add_port(udp, port, local, 0, DESCRIPTION)
            }
            other => other,
        }
        .with_context(|| format!("The router refused to forward UDP port {port}"))
    }
    /// Blocking: extend the lease before it runs out.
    pub fn renew(&mut self) -> Result<()> {
        if self.lease == 0 {
            return Ok(());
        }
        for port in self.ports.clone() {
            self.add(port)?;
        }
        Ok(())
    }
    /// The router's public address is itself private or shared (double NAT
    /// or carrier-grade NAT), so outside players still cannot reach it.
    pub fn behind_another_router(&self) -> bool {
        self.external_ip.is_some_and(|ip| !is_public(ip))
    }
}
impl Drop for PortMapping {
    fn drop(&mut self) {
        for &port in &self.ports {
            let _ = self.gateway.remove_port(PortMappingProtocol::UDP, port);
        }
    }
}

/// Search from the default-route interface: with virtual adapters present
/// (Hyper-V, WSL, VPNs) a search bound to 0.0.0.0 can leave on the wrong one.
fn search() -> Result<Gateway, igd_next::SearchError> {
    let local = UdpSocket::bind(("0.0.0.0", 0))
        .and_then(|s| s.connect(("192.0.2.1", 9)).map(|_| s))
        .and_then(|s| s.local_addr())
        .map(|a| a.ip())
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));
    igd_next::search_gateway(SearchOptions {
        bind_addr: SocketAddr::new(local, 0),
        timeout: Some(Duration::from_secs(4)),
        ..Default::default()
    })
}

/// Whether an address is reachable from the internet at large.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                // 100.64.0.0/10 carrier-grade NAT (also Tailscale).
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(v6) => !(v6.is_loopback() || v6.is_unspecified()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Read-only: finds the router and its public address; forwards nothing.
    #[test]
    #[ignore = "talks to the local network's router"]
    fn local_router_answers_upnp() {
        let gateway = search().expect("router UPnP search");
        let ip = gateway.get_external_ip().expect("external address");
        println!(
            "router {} public={} external={ip}",
            gateway.addr,
            is_public(ip)
        );
    }
    #[test]
    fn shared_and_private_router_addresses_are_not_public() {
        for ip in [
            "192.168.1.1",
            "10.0.0.1",
            "172.20.0.1",
            "100.72.1.2",
            "0.0.0.0",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["203.0.113.10", "8.8.8.8", "100.128.0.1"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }
}
