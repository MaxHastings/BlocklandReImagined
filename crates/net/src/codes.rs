//! Join codes. A provider gives a host a short code and tells a player how
//! to reach the host behind it. [`Rendezvous`] is ours (`bri-rendezvous`,
//! self-hostable); another provider (Steam, Epic Online Services) slots in
//! by implementing [`JoinCodes`], and a provider with its own transport
//! does so through quinn's `AsyncUdpSocket`, the seam the router simulator
//! uses in tests.
use crate::{client::HostPin, invite::HostKey};
use anyhow::{Context, Result, bail};
use bri_rendezvous::proto::{self, Answer, Request, ServiceAddress};
use std::{future::Future, net::SocketAddr, pin::Pin, sync::Arc, time::Duration};

/// The join code service a fresh install uses (`$pref::Net::JoinCodeService`
/// overrides it). None until a service is deployed: the game then offers
/// addresses and invites only.
pub const DEFAULT_SERVICE: Option<&str> = None;

/// The configured provider: the preference when set, else the default.
pub fn configured(preference: &str) -> Option<Result<Rendezvous>> {
    let text = if preference.trim().is_empty() {
        DEFAULT_SERVICE?
    } else {
        preference
    };
    Some(ServiceAddress::parse(text).map(Rendezvous::new))
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where a code's host can be tried.
#[derive(Debug, Clone)]
pub struct Found {
    pub key: HostKey,
    pub name: String,
    /// Direct addresses, most likely first.
    pub candidates: Vec<SocketAddr>,
    /// A relay to fall back on.
    pub relay: Option<SocketAddr>,
}

/// A code held for a running host. Dropping it gives the code up.
pub struct HostCode {
    pub code: String,
    /// The game port's address as the provider saw it.
    pub observed: Option<SocketAddr>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for HostCode {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub trait JoinCodes: Send + Sync {
    /// Get a code for the host listening on `endpoint` (the game port) and
    /// open the host's router for each player who looks it up.
    fn host(
        &self,
        endpoint: quinn::Endpoint,
        key: HostKey,
        locals: Vec<SocketAddr>,
        name: String,
    ) -> BoxFuture<'_, Result<HostCode>>;
    /// Where to reach the host with `code`, asked from the player's
    /// `endpoint` (the provider may use it to open the way).
    fn find<'a>(
        &'a self,
        endpoint: &'a quinn::Endpoint,
        code: &'a str,
    ) -> BoxFuture<'a, Result<Found>>;
}

/// The join code service at a configured address.
pub struct Rendezvous {
    pub service: ServiceAddress,
}

impl Rendezvous {
    pub fn new(service: ServiceAddress) -> Self {
        Self { service }
    }
    async fn connect(&self, endpoint: &quinn::Endpoint) -> Result<quinn::Connection> {
        let address = tokio::net::lookup_host((self.service.host.as_str(), self.service.port))
            .await
            .ok()
            .and_then(|mut found| found.find(SocketAddr::is_ipv4))
            .with_context(|| {
                format!("Could not find the join code service {}", self.service.host)
            })?;
        let mut config = quinn::ClientConfig::new(Arc::new(crate::client::pinned_config(
            HostPin::Key(self.service.key),
        )?));
        config.transport_config(Arc::new(bri_rendezvous::service::transport()));
        tokio::time::timeout(
            Duration::from_secs(5),
            endpoint.connect_with(config, address, "rendezvous.blockland.local")?,
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "The join code service did not answer. Try again, or share your address instead."
            )
        })?
        .context("Could not reach the join code service")
    }
}

impl JoinCodes for Rendezvous {
    fn host(
        &self,
        endpoint: quinn::Endpoint,
        key: HostKey,
        locals: Vec<SocketAddr>,
        name: String,
    ) -> BoxFuture<'_, Result<HostCode>> {
        Box::pin(async move {
            // From the game port itself, so the service sees (and the router
            // keeps open) the mapping players will use.
            let connection = self.connect(&endpoint).await?;
            let (mut send, mut receive) = connection.open_bi().await?;
            let locals = locals.into_iter().take(proto::MAX_LOCALS).collect();
            proto::write(
                &mut send,
                &Request::Host {
                    version: proto::VERSION,
                    key,
                    locals,
                    name,
                },
            )
            .await?;
            let answer = tokio::time::timeout(Duration::from_secs(5), proto::read(&mut receive))
                .await
                .map_err(|_| anyhow::anyhow!("The join code service did not give a code"))??;
            let (code, observed) = match answer {
                Some(Answer::Hosting { code, observed }) => (code, observed),
                Some(Answer::Refused(reason)) => bail!("{reason}"),
                _ => bail!("The join code service gave no code"),
            };
            let task = tokio::spawn(async move {
                // `send` stays open: finishing it gives the code up.
                let _send = send;
                let _connection = connection;
                while let Ok(Some(answer)) = proto::read::<Answer>(&mut receive).await {
                    if let Answer::Joiner {
                        observed,
                        locals,
                        relay,
                    } = answer
                    {
                        for target in std::iter::once(observed).chain(locals).chain(relay) {
                            punch(&endpoint, target);
                        }
                    }
                }
            });
            Ok(HostCode {
                code,
                observed: Some(observed),
                task,
            })
        })
    }
    fn find<'a>(
        &'a self,
        endpoint: &'a quinn::Endpoint,
        code: &'a str,
    ) -> BoxFuture<'a, Result<Found>> {
        Box::pin(async move {
            let code = proto::parse_code(code).context("A join code looks like K7Q-4MZ")?;
            let connection = self.connect(endpoint).await?;
            let (mut send, mut receive) = connection.open_bi().await?;
            proto::write(
                &mut send,
                &Request::Join {
                    version: proto::VERSION,
                    code,
                    locals: crate::reach::local_ip()
                        .map(|ip| {
                            SocketAddr::new(ip, endpoint.local_addr().map_or(0, |a| a.port()))
                        })
                        .into_iter()
                        .filter(|a| a.port() != 0)
                        .collect(),
                },
            )
            .await?;
            send.finish()?;
            let answer =
                tokio::time::timeout(Duration::from_secs(5), proto::read(&mut receive)).await??;
            connection.close(0u32.into(), b"Found");
            match answer {
                Some(Answer::Found {
                    key,
                    name,
                    candidates,
                    relay,
                    ..
                }) => Ok(Found {
                    key,
                    name,
                    candidates,
                    relay,
                }),
                Some(Answer::Refused(reason)) => bail!("{reason}"),
                _ => bail!("The join code service did not answer"),
            }
        })
    }
}

/// Send one QUIC packet from the game port to `target`, so this router
/// lets `target`'s packets in. Nothing listens for an answer; the attempt
/// is dropped after a few seconds.
pub fn punch(endpoint: &quinn::Endpoint, target: SocketAddr) {
    let Ok(crypto) = crate::client::pinned_config(HostPin::FirstUse) else {
        return;
    };
    let config = quinn::ClientConfig::new(Arc::new(crypto));
    if let Ok(connecting) = endpoint.connect_with(config, target, "punch") {
        tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_secs(3), connecting).await;
        });
    }
}

/// How long direct routes get before the relay is tried as well.
pub const RELAY_AFTER: Duration = Duration::from_millis(1500);
/// How long a code join may take before giving up.
pub const JOIN_WAIT: Duration = Duration::from_secs(10);

/// Try every route at once (the relay after [`RELAY_AFTER`]) and keep the
/// first handshake that completes. Returns the connection and whether it
/// runs through the relay.
pub async fn race(
    endpoint: &quinn::Endpoint,
    found: &Found,
    pin: &HostPin,
) -> Result<(quinn::Connection, bool)> {
    let mut attempts = tokio::task::JoinSet::new();
    let routes = found
        .candidates
        .iter()
        .map(|&a| (a, Duration::ZERO, false))
        .chain(found.relay.map(|a| (a, RELAY_AFTER, true)));
    for (address, delay, relayed) in routes {
        let (endpoint, pin) = (endpoint.clone(), pin.clone());
        attempts.spawn(async move {
            tokio::time::sleep(delay).await;
            crate::client::handshake(&endpoint, address, &pin, JOIN_WAIT - delay)
                .await
                .map(|connection| (connection, relayed))
        });
    }
    let mut last = None;
    while let Some(done) = attempts.join_next().await {
        match done {
            Ok(Ok(won)) => {
                attempts.abort_all();
                return Ok(won);
            }
            Ok(Err(error)) => last = Some(error),
            Err(_) => {}
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("The join code gave no way to reach the host")))
        .context("Could not reach the host behind that code")
}

/// A client endpoint on `socket` (a simulated router in tests) or a fresh
/// OS socket, optionally at a fixed local address.
pub fn endpoint(
    socket: Option<Arc<dyn quinn::AsyncUdpSocket>>,
    bind: Option<SocketAddr>,
) -> Result<quinn::Endpoint> {
    Ok(match socket {
        Some(socket) => quinn::Endpoint::new_with_abstract_socket(
            quinn::EndpointConfig::default(),
            None,
            socket,
            Arc::new(quinn::TokioRuntime),
        )?,
        None => quinn::Endpoint::client(bind.unwrap_or(SocketAddr::from(([0, 0, 0, 0], 0))))?,
    })
}
