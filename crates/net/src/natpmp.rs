//! NAT-PMP port mapping (RFC 6886), for routers that speak it instead of
//! UPnP (Apple routers, many miniupnpd builds, some ISP boxes). Tried after
//! UPnP IGD finds no router. PCP (RFC 6887), NAT-PMP's successor, is not
//! spoken yet; routers that offer both still answer these requests.
use anyhow::{Context, Result, bail, ensure};
use std::{
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    time::Duration,
};

pub const PORT: u16 = 5351;
const UDP: u8 = 1;
/// How long to wait for each answer before asking again: 250 ms, 500 ms and
/// 1 s (RFC 6886 3.1). Short, since there may be no NAT-PMP router at all.
const RFC_WAITS: &[Duration] = &[
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_millis(1000),
];

/// A UDP forward on a NAT-PMP router. Removed again when dropped.
pub struct Mapping {
    gateway: SocketAddrV4,
    internal: u16,
    /// The port the router opened outside (usually the same number).
    pub external_port: u16,
    pub lease: u32,
    pub external_ip: Option<Ipv4Addr>,
    /// The retry schedule for every request to this router.
    waits: &'static [Duration],
}

impl Mapping {
    /// Blocking: ask `gateway` to forward UDP `port` to this computer for
    /// `lease` seconds.
    pub fn open(gateway: Ipv4Addr, port: u16, lease: u32) -> Result<Self> {
        Self::open_at(SocketAddrV4::new(gateway, PORT), port, lease, RFC_WAITS)
    }
    fn open_at(
        gateway: SocketAddrV4,
        port: u16,
        lease: u32,
        waits: &'static [Duration],
    ) -> Result<Self> {
        let external_ip = external_address(gateway, waits).ok();
        let (external_port, lease) = map(gateway, waits, port, port, lease)?;
        Ok(Self {
            gateway,
            internal: port,
            external_port,
            lease,
            external_ip,
            waits,
        })
    }
    /// Blocking: extend the lease before it runs out.
    pub fn renew(&mut self) -> Result<()> {
        let (external_port, lease) = map(
            self.gateway,
            self.waits,
            self.internal,
            self.external_port,
            self.lease,
        )?;
        self.external_port = external_port;
        self.lease = lease;
        Ok(())
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        // Lifetime 0 with external port 0 deletes the mapping (RFC 6886 3.4).
        let _ = map(self.gateway, self.waits, self.internal, 0, 0);
    }
}

fn external_address(gateway: SocketAddrV4, waits: &[Duration]) -> Result<Ipv4Addr> {
    let answer = exchange(gateway, waits, &[0, 0], 12)?;
    ensure!(answer[1] == 128, "Unexpected NAT-PMP answer");
    Ok(Ipv4Addr::new(answer[8], answer[9], answer[10], answer[11]))
}

/// Returns the external port and granted lease.
fn map(
    gateway: SocketAddrV4,
    waits: &[Duration],
    internal: u16,
    external: u16,
    lease: u32,
) -> Result<(u16, u32)> {
    let mut request = vec![0, UDP, 0, 0];
    request.extend(internal.to_be_bytes());
    request.extend(external.to_be_bytes());
    request.extend(lease.to_be_bytes());
    let answer = exchange(gateway, waits, &request, 16)?;
    ensure!(
        answer[1] == 128 + UDP && u16::from_be_bytes([answer[8], answer[9]]) == internal,
        "Unexpected NAT-PMP answer"
    );
    Ok((
        u16::from_be_bytes([answer[10], answer[11]]),
        u32::from_be_bytes([answer[12], answer[13], answer[14], answer[15]]),
    ))
}

/// Send `request`, asking again after each of `waits` passes unanswered
/// ([`RFC_WAITS`] for a real router), and return a successful answer of
/// `len` bytes.
fn exchange(
    gateway: SocketAddrV4,
    waits: &[Duration],
    request: &[u8],
    len: usize,
) -> Result<Vec<u8>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.connect(gateway)?;
    let mut buffer = [0u8; 64];
    for &wait in waits {
        socket.send(request)?;
        socket.set_read_timeout(Some(wait))?;
        match socket.recv(&mut buffer) {
            Ok(n) if n >= len && buffer[0] == 0 => {
                let result = u16::from_be_bytes([buffer[2], buffer[3]]);
                if result != 0 {
                    bail!(
                        "The router refused the port mapping ({})",
                        result_text(result)
                    );
                }
                return Ok(buffer[..len].to_vec());
            }
            Ok(_) => continue,
            // Timed out, or the router has no NAT-PMP (ICMP port unreachable).
            Err(_) => continue,
        }
    }
    bail!("No NAT-PMP answer from the router at {}", gateway.ip())
}

fn result_text(code: u16) -> &'static str {
    match code {
        1 => "unsupported version",
        2 => "not authorized, port mapping is turned off on the router",
        3 => "network failure",
        4 => "out of resources",
        _ => "unsupported request",
    }
}

/// The default IPv4 gateway (normally the home router).
pub fn default_gateway() -> Result<Ipv4Addr> {
    platform_gateway().context("Could not find this computer's router")
}

#[cfg(windows)]
fn platform_gateway() -> Result<Ipv4Addr> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{GetBestRoute, MIB_IPFORWARDROW};
    // The route Windows would use to reach a public address.
    let destination = u32::from_ne_bytes([192, 0, 2, 1]);
    let mut row: MIB_IPFORWARDROW = unsafe { std::mem::zeroed() };
    // SAFETY: `row` is a valid, writable MIB_IPFORWARDROW for the call.
    let status = unsafe { GetBestRoute(destination, 0, &mut row) };
    ensure!(status == 0, "GetBestRoute failed ({status})");
    let hop = Ipv4Addr::from(row.dwForwardNextHop.to_ne_bytes());
    ensure!(!hop.is_unspecified(), "No default gateway");
    Ok(hop)
}

#[cfg(target_os = "linux")]
fn platform_gateway() -> Result<Ipv4Addr> {
    parse_proc_route(&std::fs::read_to_string("/proc/net/route")?)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn platform_gateway() -> Result<Ipv4Addr> {
    bail!("Default gateway lookup is not implemented on this platform")
}

/// `/proc/net/route`: the default route's gateway, stored little-endian hex.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_proc_route(table: &str) -> Result<Ipv4Addr> {
    for line in table.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() > 2 && fields[1] == "00000000" {
            let gateway = u32::from_str_radix(fields[2], 16)?;
            if gateway != 0 {
                return Ok(Ipv4Addr::from(gateway.to_le_bytes()));
            }
        }
    }
    bail!("No default route")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn default_route_gateway_is_read_from_the_route_table() {
        let table = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\n\
                     eth0\t0000A8C0\t00000000\t0001\t0\t0\t0\t00FFFFFF\n\
                     eth0\t00000000\t0101A8C0\t0003\t0\t0\t0\t00000000\n";
        assert_eq!(
            parse_proc_route(table).unwrap(),
            Ipv4Addr::new(192, 168, 1, 1)
        );
        assert!(parse_proc_route("Iface\tDestination\n").is_err());
    }

    /// A fake router on loopback answers the external address and mapping
    /// requests like RFC 6886 describes. It serves until the mapping is
    /// deleted, and the client waits for each answer rather than for the
    /// RFC's short retry timers, so a busy machine changes nothing.
    #[test]
    fn mappings_open_renew_and_close_against_a_local_router() {
        // One request each, answered whenever the router gets to it; the
        // bound only stops a broken run from hanging.
        const PATIENT: &[Duration] = &[Duration::from_secs(120)];
        let router = UdpSocket::bind("127.0.0.1:0").unwrap();
        let at = match router.local_addr().unwrap() {
            SocketAddr::V4(v4) => v4,
            _ => unreachable!(),
        };
        let serve = std::thread::spawn(move || {
            let mut seen = Vec::new();
            let mut buffer = [0u8; 64];
            router
                .set_read_timeout(Some(Duration::from_secs(120)))
                .unwrap();
            loop {
                let (n, from) = router.recv_from(&mut buffer).unwrap();
                let request = buffer[..n].to_vec();
                let mut answer = vec![0, 128 + request[1], 0, 0, 0, 0, 0, 9];
                if request[1] == 0 {
                    answer.extend([198, 51, 100, 7]);
                } else {
                    answer.extend(&request[4..6]);
                    // The router grants the requested external port, or the
                    // internal one when asked for none.
                    let external = if request[6..8] == [0, 0] && request[8..12] != [0; 4] {
                        request[4..6].to_vec()
                    } else {
                        request[6..8].to_vec()
                    };
                    answer.extend(external);
                    answer.extend(&request[8..12]);
                }
                router.send_to(&answer, from).unwrap();
                let deleted = request[1] == UDP && request[8..12] == [0; 4];
                seen.push(request);
                if deleted {
                    return seen;
                }
            }
        });
        let gateway = SocketAddrV4::new(*at.ip(), at.port());
        let mut mapping = Mapping::open_at(gateway, 28000, 3600, PATIENT).unwrap();
        assert_eq!(mapping.external_ip, Some(Ipv4Addr::new(198, 51, 100, 7)));
        assert_eq!((mapping.external_port, mapping.lease), (28000, 3600));
        mapping.renew().unwrap();
        drop(mapping);
        let seen = serve.join().unwrap();
        // External address, open, renew, delete: one request each.
        assert_eq!(seen.len(), 4, "{seen:?}");
        assert_eq!(seen[0], [0, 0], "the external address is asked first");
        assert_eq!(
            seen[1][1..],
            seen[2][1..],
            "renewing asks for the same mapping"
        );
        assert_eq!(seen[3][8..12], [0, 0, 0, 0], "dropping deletes the mapping");
    }

    #[test]
    fn refusals_name_the_reason() {
        assert!(result_text(2).contains("turned off"));
    }
}
