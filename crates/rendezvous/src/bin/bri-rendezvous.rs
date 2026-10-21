//! `bri-rendezvous`: the join code service. Run it on any machine players
//! can reach over UDP; set the printed address in the game's
//! `$pref::Net::JoinCodeService`.
use anyhow::{Context, Result, ensure};
use bri_rendezvous::{
    proto::{DEFAULT_PORT, ServiceAddress},
    service::{Config, Identity, Service},
};
use std::{net::SocketAddr, path::PathBuf, sync::atomic::Ordering, time::Duration};

const USAGE: &str = "Usage: bri-rendezvous <state-dir> [--listen 0.0.0.0:28100] [--public-host HOST] [--relay-ports 28200-28999] [--no-relay]

Gives hosts short join codes, introduces players to hosts so both routers open
(hole punching), and relays the encrypted game traffic when that fails.
<state-dir> keeps the service's certificate; keep it across restarts, or every
player's configured service address stops working.
Open UDP --listen's port and the relay port range in the firewall.";

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let state = args
        .next()
        .filter(|a| !a.starts_with("--"))
        .context(USAGE)?;
    let mut config = Config::new(SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT)));
    let mut public_host = None;
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .with_context(|| format!("{flag} needs a value\n\n{USAGE}"))
        };
        match flag.as_str() {
            "--listen" => config.listen = value()?.parse().context("Invalid --listen address")?,
            "--public-host" => public_host = Some(value()?),
            "--relay-ports" => {
                let text = value()?;
                let (low, high) = text
                    .split_once('-')
                    .context("--relay-ports looks like 28200-28999")?;
                let (low, high): (u16, u16) = (low.parse()?, high.parse()?);
                ensure!(low > 0 && low <= high, "Invalid --relay-ports range");
                config.relay_limits.ports = Some((low, high));
            }
            "--no-relay" => config.relay = false,
            _ => anyhow::bail!("Unknown option {flag}\n\n{USAGE}"),
        }
    }
    if let Some(host) = &public_host
        && let Ok(ip) = host.parse()
    {
        config.public_ip = Some(ip);
    }
    let identity = Identity::load_or_create(&PathBuf::from(&state))?;
    let service = Service::start(config.clone(), identity)?;
    let shown = ServiceAddress {
        host: public_host.unwrap_or_else(|| service.address.ip().to_string()),
        port: service.address.port(),
        key: service.key,
    };
    println!("Join code service listening on {}", service.address);
    println!("Players set: {shown}");
    println!("Relay: {}", if config.relay { "on" } else { "off" });
    let stats = service.relay_stats.clone();
    let log = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            println!(
                "Relays active {}, forwarded {} MiB, dropped {} over budget",
                stats.active.load(Ordering::Relaxed),
                stats.forwarded_bytes.load(Ordering::Relaxed) >> 20,
                stats.dropped.load(Ordering::Relaxed),
            );
        }
    });
    tokio::signal::ctrl_c().await?;
    log.abort();
    service.stop().await;
    Ok(())
}
