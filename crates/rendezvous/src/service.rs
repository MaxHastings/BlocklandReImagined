//! The rendezvous service: hands out join codes, introduces players to
//! hosts so both routers open for each other (hole punching), and relays
//! when that fails. It keeps nothing on disk but its own certificate; a code
//! lives only while its host stays connected.
use crate::{
    proto::{self, Answer, Key, Request},
    relay,
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: SocketAddr,
    /// The address players and hosts reach this service at, for relay
    /// ports. None uses the address each client connected to.
    pub public_ip: Option<IpAddr>,
    /// Relay traffic when direct connection fails. Off makes this a
    /// rendezvous-only service.
    pub relay: bool,
    pub relay_limits: relay::Limits,
    pub max_hosts: usize,
    pub max_relays: usize,
    /// Codes one IP address may hold at once.
    pub hosts_per_ip: usize,
    /// Code lookups one IP address may make per minute (guessing codes).
    pub joins_per_minute: u32,
}
impl Config {
    pub fn new(listen: SocketAddr) -> Self {
        Self {
            listen,
            public_ip: None,
            relay: true,
            relay_limits: relay::Limits::default(),
            max_hosts: 5000,
            max_relays: 500,
            hosts_per_ip: 4,
            joins_per_minute: 20,
        }
    }
}

/// The service's own certificate, kept across restarts so the address
/// players have configured (with its key) stays valid.
#[derive(Clone)]
pub struct Identity {
    pub der: Vec<u8>,
    pub key: Vec<u8>,
}
impl Identity {
    pub fn generate() -> Result<Self> {
        let cert = rcgen::generate_simple_self_signed(vec!["rendezvous.blockland.local".into()])?;
        Ok(Self {
            der: cert.cert.der().to_vec(),
            key: cert.signing_key.serialize_der(),
        })
    }
    /// `rendezvous-cert.der` and `rendezvous-key.der` in `dir`, created on
    /// first use.
    pub fn load_or_create(dir: &std::path::Path) -> Result<Self> {
        let (cert, key) = (dir.join("rendezvous-cert.der"), dir.join("rendezvous-key.der"));
        match (std::fs::read(&cert), std::fs::read(&key)) {
            (Ok(der), Ok(key)) => Ok(Self { der, key }),
            (Err(_), Err(_)) => {
                let identity = Self::generate()?;
                std::fs::create_dir_all(dir)?;
                std::fs::write(&key, &identity.key)?;
                std::fs::write(&cert, &identity.der)?;
                Ok(identity)
            }
            _ => bail!("Only one of {} and {} exists; restore both", cert.display(), key.display()),
        }
    }
}

struct HostEntry {
    key: Key,
    name: String,
    observed: SocketAddr,
    locals: Vec<SocketAddr>,
    notify: mpsc::Sender<Answer>,
}

#[derive(Default)]
struct State {
    hosts: HashMap<String, HostEntry>,
    hosts_by_ip: HashMap<IpAddr, usize>,
    joins: HashMap<IpAddr, (Instant, u32)>,
}

pub struct Service {
    pub address: SocketAddr,
    /// The key players pin, published with the address.
    pub key: Key,
    pub relay_stats: Arc<relay::Stats>,
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Service {
    pub fn start(config: Config, identity: Identity) -> Result<Self> {
        let key = proto::key_of(&identity.der);
        let private = quinn::rustls::pki_types::PrivatePkcs8KeyDer::from(identity.key.clone());
        let mut server =
            quinn::ServerConfig::with_single_cert(vec![identity.der.clone().into()], private.into())?;
        server.transport_config(Arc::new(transport()));
        let endpoint = quinn::Endpoint::server(server, config.listen)?;
        let address = endpoint.local_addr()?;
        let relay_stats = Arc::new(relay::Stats::default());
        let (stop, stopped) = oneshot::channel();
        let state = Arc::new(Mutex::new(State::default()));
        let stats = relay_stats.clone();
        let task = tokio::spawn(async move {
            let mut stopped = stopped;
            loop {
                tokio::select! {
                    _ = &mut stopped => break,
                    incoming = endpoint.accept() => {
                        let Some(incoming) = incoming else { break };
                        let (state, config, stats) = (state.clone(), config.clone(), stats.clone());
                        tokio::spawn(async move {
                            if let Ok(Ok(connection)) = tokio::time::timeout(Duration::from_secs(10), incoming).await {
                                serve(connection, state, config, stats).await;
                            }
                        });
                    }
                }
            }
            endpoint.close(0u32.into(), b"Service stopped");
        });
        Ok(Self {
            address,
            key,
            relay_stats,
            stop: Some(stop),
            task,
        })
    }
    pub async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let _ = (&mut self.task).await;
    }
}

/// Service connections: kept alive so the host's router keeps the game
/// port's mapping to the service open.
pub fn transport() -> quinn::TransportConfig {
    let mut t = quinn::TransportConfig::default();
    t.max_concurrent_bidi_streams(4_u32.into())
        .max_concurrent_uni_streams(0_u32.into())
        .keep_alive_interval(Some(Duration::from_secs(10)))
        .max_idle_timeout(Some(Duration::from_secs(40).try_into().unwrap()));
    t
}

async fn serve(
    connection: quinn::Connection,
    state: Arc<Mutex<State>>,
    config: Config,
    stats: Arc<relay::Stats>,
) {
    while let Ok((send, receive)) = connection.accept_bi().await {
        let (connection, state, config, stats) =
            (connection.clone(), state.clone(), config.clone(), stats.clone());
        tokio::spawn(async move {
            let _ = stream(connection, send, receive, state, config, stats).await;
        });
    }
}

async fn stream(
    connection: quinn::Connection,
    mut send: quinn::SendStream,
    mut receive: quinn::RecvStream,
    state: Arc<Mutex<State>>,
    config: Config,
    stats: Arc<relay::Stats>,
) -> Result<()> {
    let observed = connection.remote_address();
    let request: Request = tokio::time::timeout(Duration::from_secs(10), proto::read(&mut receive))
        .await??
        .context("Empty request")?;
    let refuse = |text: &str| Answer::Refused(text.to_string());
    match request {
        Request::Host {
            version,
            key,
            locals,
            name,
        } => {
            let checked = (|| {
                ensure!(version == proto::VERSION, "This game is too old or too new for this join code service.");
                ensure!(locals.len() <= proto::MAX_LOCALS && name.len() <= 128, "Invalid request");
                Ok(())
            })();
            if let Err(error) = checked {
                proto::write(&mut send, &refuse(&error.to_string())).await?;
                return Ok(());
            }
            let (notify, mut events) = mpsc::channel(16);
            let code = {
                let mut state = state.lock().unwrap();
                let held = state.hosts_by_ip.get(&observed.ip()).copied().unwrap_or(0);
                if state.hosts.len() >= config.max_hosts || held >= config.hosts_per_ip {
                    None
                } else {
                    let mut code = proto::new_code()?;
                    while state.hosts.contains_key(&code) {
                        code = proto::new_code()?;
                    }
                    state.hosts.insert(
                        code.clone(),
                        HostEntry {
                            key,
                            name,
                            observed,
                            locals,
                            notify,
                        },
                    );
                    *state.hosts_by_ip.entry(observed.ip()).or_default() += 1;
                    Some(code)
                }
            };
            let Some(code) = code else {
                proto::write(&mut send, &refuse("The join code service is full right now. Try again later, or share your address instead.")).await?;
                return Ok(());
            };
            let result = async {
                proto::write(&mut send, &Answer::Hosting { code: code.clone(), observed }).await?;
                let mut probe = [0u8; 1];
                loop {
                    tokio::select! {
                        event = events.recv() => match event {
                            Some(event) => proto::write(&mut send, &event).await?,
                            None => break,
                        },
                        // The host finishing or dropping the stream ends the code.
                        _ = receive.read(&mut probe) => break,
                    }
                }
                anyhow::Ok(())
            }
            .await;
            let mut state = state.lock().unwrap();
            state.hosts.remove(&code);
            if let Some(held) = state.hosts_by_ip.get_mut(&observed.ip()) {
                *held -= 1;
                if *held == 0 {
                    state.hosts_by_ip.remove(&observed.ip());
                }
            }
            result
        }
        Request::Join {
            version,
            code,
            locals,
        } => {
            ensure!(locals.len() <= proto::MAX_LOCALS, "Invalid request");
            if version != proto::VERSION {
                proto::write(&mut send, &refuse("This game is too old or too new for this join code service.")).await?;
                return Ok(());
            }
            let found = {
                let mut state = state.lock().unwrap();
                let now = Instant::now();
                if state.joins.len() > 100_000 {
                    state.joins.retain(|_, (at, _)| now.duration_since(*at) < Duration::from_secs(60));
                }
                let attempts = state.joins.entry(observed.ip()).or_insert((now, 0));
                if now.duration_since(attempts.0) > Duration::from_secs(60) {
                    *attempts = (now, 0);
                }
                attempts.1 += 1;
                if attempts.1 > config.joins_per_minute {
                    Err("Too many join attempts. Wait a minute and try again.")
                } else {
                    let code = proto::parse_code(&code).unwrap_or_default();
                    state
                        .hosts
                        .get(&code)
                        .map(|h| (h.key, h.name.clone(), h.observed, h.locals.clone(), h.notify.clone()))
                        .ok_or("No game has that code. Check it, or ask the host for the code again.")
                }
            };
            let (key, name, host_observed, host_locals, notify) = match found {
                Ok(found) => found,
                Err(text) => {
                    proto::write(&mut send, &refuse(text)).await?;
                    return Ok(());
                }
            };
            let public_ip = config
                .public_ip
                .or_else(|| connection.local_ip())
                .unwrap_or(config.listen.ip());
            let relay = if config.relay
                && stats.active.load(std::sync::atomic::Ordering::Relaxed) < config.max_relays
            {
                relay::open(
                    config.listen.ip(),
                    host_observed.ip(),
                    observed.ip(),
                    config.relay_limits,
                    stats.clone(),
                )
                .await
                .ok()
            } else {
                None
            };
            let _ = notify
                .send(Answer::Joiner {
                    observed,
                    locals,
                    relay: relay.map(|(host_port, _)| SocketAddr::new(public_ip, host_port)),
                })
                .await;
            let mut candidates = vec![host_observed];
            candidates.extend(host_locals);
            proto::write(
                &mut send,
                &Answer::Found {
                    key,
                    name,
                    candidates,
                    relay: relay.map(|(_, joiner_port)| SocketAddr::new(public_ip, joiner_port)),
                    observed,
                },
            )
            .await?;
            send.finish()?;
            let _ = tokio::time::timeout(Duration::from_secs(2), send.stopped()).await;
            Ok(())
        }
    }
}
