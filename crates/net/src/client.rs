use crate::{codec, protocol::*, replica::Replica, server::transport};
use anyhow::{Context, Result, ensure};
use bri_identity::ClientIdentity;
use bri_progress::{Progress, Stage, Unit};
use bri_sim::{
    player::MoveInput,
    session::{CameraView, Command, Reply, SeatSince},
};
use bri_world::OwnerId;
use sha2::Digest;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::sync::mpsc;
enum Incoming {
    Reliable(Box<Message>),
    Pose(Pose),
    Vehicle(bri_sim::session::VehiclePose),
    Orb(Orb),
    Closed(String),
}
#[derive(Debug)]
pub enum ClientEvent {
    Updated {
        world_changed: bool,
        /// Brick ids the delta added, replaced or removed.
        changed_bricks: Vec<u64>,
        palette_changed: bool,
    },
    Pose(OwnerId),
    Vehicle(u64),
    Orb(OwnerId),
    Reply {
        sequence: u64,
        result: Result<Reply, bri_sim::session::Rejection>,
    },
    AdminSnapshot(bri_sim::session::AdminSnapshot),
    Notice(bri_sim::session::Notice),
    /// The host changed maps and its bricks are streaming in (progress is
    /// reported separately). `MapChanged` follows once the replica holds the
    /// whole new world.
    MapChanging {
        map: String,
    },
    /// The replica now holds a new map.
    MapChanged,
}
/// A join refused because the client's shared packages differ from the
/// server's. Downcast a join error to this to offer the download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagesDiffer(pub Vec<bri_package::environment::Mismatch>);
impl std::fmt::Display for PackagesDiffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The Add-Ons screen reads the differing packages back from this.
        f.write_str(&bri_package::environment::refusal(&self.0))
    }
}
impl std::error::Error for PackagesDiffer {}
/// What a player who joined without some of the server's shared content
/// is told: which packages, and that what they add may be missing.
pub fn unavailable_notice(missing: &[bri_package::environment::Mismatch]) -> String {
    let names: Vec<String> = missing
        .iter()
        .map(|m| match m {
            bri_package::environment::Mismatch::Missing(p)
            | bri_package::environment::Mismatch::Different { server: p, .. } => {
                format!("{} {}", p.id, p.version)
            }
            bri_package::environment::Mismatch::Extra(p) => {
                format!("{} {} (yours, not the server's)", p.id, p.version)
            }
        })
        .collect();
    format!(
        "You joined without some of this server's content, which could not be downloaded, so what it adds may be missing or behave differently: {}",
        names.join(", ")
    )
}

pub struct Client {
    endpoint: quinn::Endpoint,
    connection: quinn::Connection,
    send: quinn::SendStream,
    incoming: mpsc::Receiver<Incoming>,
    readers: Vec<tokio::task::JoinHandle<()>>,
    pub owner: OwnerId,
    pub administrator: bool,
    pub admin_snapshot: Option<bri_sim::session::AdminSnapshot>,
    pub resume: ResumeToken,
    /// The certificate the host presented, to pin for later joins.
    pub certificate: Vec<u8>,
    /// The host's listing from the handshake (its name for saved servers).
    pub listing: Listing,
    /// Shared packages the server runs that this client joined without:
    /// never offered (base game content), or their download or load failed.
    pub unavailable: Vec<bri_package::environment::PackageRef>,
    pub replica: Replica,
    /// A changed map whose bricks are still streaming in.
    changing_map: Option<WorldAssembly>,
    /// The joined world's distant bricks, still streaming in while the
    /// player plays.
    joining: Option<WorldRest>,
    /// Where joins and map changes report their world download.
    progress: Progress,
    sequence: u64,
    /// Replica updates applied (deltas and state datagrams), for the net graph.
    updates: Arc<std::sync::atomic::AtomicU64>,
    /// The newest host tick a pose datagram brought, as it arrived: where
    /// the host is, however far behind this client is in reading.
    heard: Arc<std::sync::atomic::AtomicU64>,
    /// Movement datagrams leave on a millisecond clock while connected.
    _timers: crate::timer_resolution::Guard,
}
/// Transport counters since the connection opened. Differences between two
/// samples give the net graph's rates.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LinkSample {
    pub rtt: Duration,
    pub sent_packets: u64,
    pub received_packets: u64,
    pub sent_bytes: u64,
    pub received_bytes: u64,
    /// Sent packets the transport declared lost.
    pub lost_packets: u64,
    /// Replica updates applied: world deltas and pose datagrams.
    pub updates: u64,
}
/// A cheap handle another thread samples the connection's counters through.
#[derive(Clone)]
pub struct LinkProbe {
    connection: quinn::Connection,
    updates: Arc<std::sync::atomic::AtomicU64>,
}
impl LinkProbe {
    pub fn sample(&self) -> LinkSample {
        let stats = self.connection.stats();
        LinkSample {
            rtt: self.connection.rtt(),
            sent_packets: stats.udp_tx.datagrams,
            received_packets: stats.udp_rx.datagrams,
            sent_bytes: stats.udp_tx.bytes,
            received_bytes: stats.udp_rx.bytes,
            lost_packets: stats.path.lost_packets,
            updates: self.updates.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
    /// Datagrams received from the host so far, acknowledgements included.
    pub fn received(&self) -> u64 {
        self.connection.stats().udp_rx.datagrams
    }
}
impl Client {
    /// The certificate is a trusted host pin. Never disable TLS verification.
    /// A future host browser/import flow supplies this trust decision.
    pub async fn connect(
        address: SocketAddr,
        certificate: &[u8],
        name: String,
        packages: Vec<bri_package::environment::PackageRef>,
        resume: Option<ResumeToken>,
    ) -> Result<Self> {
        Self::connect_with_host(address, certificate, name, packages, resume, None).await
    }
    pub async fn connect_with_host(
        address: SocketAddr,
        certificate: &[u8],
        name: String,
        packages: Vec<bri_package::environment::PackageRef>,
        resume: Option<ResumeToken>,
        host: Option<ResumeToken>,
    ) -> Result<Self> {
        Self::connect_inner(
            address,
            HostPin::from(certificate),
            name.into(),
            packages,
            resume,
            host,
            None,
            Progress::default(),
            false,
        )
        .await
    }
    pub async fn connect_with_identity(
        address: SocketAddr,
        certificate: &[u8],
        name: String,
        packages: Vec<bri_package::environment::PackageRef>,
        resume: Option<ResumeToken>,
        host: Option<ResumeToken>,
        identity: &ClientIdentity,
    ) -> Result<Self> {
        Self::connect_reporting(
            address,
            certificate,
            name.into(),
            packages,
            resume,
            host,
            identity,
            Progress::default(),
        )
        .await
    }
    /// Joins like [`Client::connect_reporting`], downloading whatever the
    /// server runs that this client does not, exactly (by content hash):
    /// all offered packages before admission (HUD panels, models), and when
    /// the server refuses because shared packages differ, every package it
    /// offers that the client lacks or has in another version, into
    /// `cache`, then joins once more. Shared packages the client runs and
    /// the server does not are left out of that join. `load` receives the
    /// fetched packages and the left-out ones, loads the server's set, and
    /// returns the package list the client now runs; the server checks
    /// that list again. Nothing asks the player: a join downloads what it
    /// needs, as v20 did. What the server does not offer, or what fails to
    /// download or load, is joined without ([`Client::unavailable`]); the
    /// server tells the player what is missing. `load` runs before admission
    /// and may run again for removed shared packages or load-error fallback.
    /// A loader may return [`JoinPreparationPending`] to suspend admission
    /// while its caller prepares the fetched content. This is not a load
    /// failure and never takes the joining-without fallback.
    /// Returns what was fetched and what was left out.
    #[allow(clippy::too_many_arguments)]
    pub async fn connect_fetching(
        address: SocketAddr,
        pin: HostPin,
        name: JoinName,
        packages: Vec<bri_package::environment::PackageRef>,
        host: Option<ResumeToken>,
        identity: &ClientIdentity,
        cache: &bri_package::sync::Cache,
        progress: Progress,
        load: impl FnMut(
            &[crate::packages::Fetched],
            &[bri_package::environment::PackageRef],
        ) -> Result<Vec<bri_package::environment::PackageRef>>,
    ) -> Result<(
        Self,
        Vec<crate::packages::Fetched>,
        Vec<bri_package::environment::PackageRef>,
    )> {
        Self::connect_fetching_resuming(
            address, pin, name, packages, None, host, identity, cache, progress, load,
        )
        .await
    }
    /// [`Client::connect_fetching`] back into the game a lost connection
    /// was in: `resume` is that connection's [`Client::resume`] ticket. The
    /// host gives the player their owner number back, and a connection of
    /// theirs it has not yet timed out is replaced. A host that no longer
    /// knows the ticket (it restarted) refuses it, and the join is made
    /// afresh.
    #[allow(clippy::too_many_arguments)]
    pub async fn connect_fetching_resuming(
        address: SocketAddr,
        pin: HostPin,
        name: JoinName,
        packages: Vec<bri_package::environment::PackageRef>,
        mut resume: Option<ResumeToken>,
        host: Option<ResumeToken>,
        identity: &ClientIdentity,
        cache: &bri_package::sync::Cache,
        progress: Progress,
        mut load: impl FnMut(
            &[crate::packages::Fetched],
            &[bri_package::environment::PackageRef],
        ) -> Result<Vec<bri_package::environment::PackageRef>>,
    ) -> Result<(
        Self,
        Vec<crate::packages::Fetched>,
        Vec<bri_package::environment::PackageRef>,
    )> {
        let have = packages.clone();
        let mut packages = packages;
        // Pin the same authenticated host for the download and game joins.
        // The existing probe stops before Hello, so no player is admitted.
        progress.begin(Stage::Connecting, Unit::Steps, None);
        let found = probe(address, &pin, Duration::from_secs(10)).await?;
        // The host's listing names its map, so the loading screen can show
        // it while the Add-Ons download, before the Welcome confirms it.
        if !found.listing.map.trim().is_empty() {
            progress.set_subject(&found.listing.map);
        }
        let pin = HostPin::Certificate(found.certificate);
        let (mut fetched, fetch_failure) =
            match crate::packages::fetch_missing_pinned(address, &pin, cache, &progress, &have)
                .await
            {
                Ok(fetched) => (fetched, None),
                Err(error) => (Vec::new(), Some(format!("{error:#}"))),
            };
        let offered_shared: Vec<_> = fetched
            .iter()
            .filter(|f| f.package.side == bri_package::packages::Side::Shared)
            .map(|f| f.package.clone())
            .collect();
        if !fetched.is_empty() {
            match load(&fetched, &[]) {
                Ok(prepared) => packages = prepared,
                Err(error) if error.is::<JoinPreparationPending>() => return Err(error),
                Err(error) => {
                    eprintln!("Joining without the server's Add-Ons: {error:#}");
                    fetched.clear();
                    packages = load(&[], &[])?;
                }
            }
        }
        let mut joined = Self::connect_pinned(
            address,
            pin.clone(),
            name.clone(),
            packages.clone(),
            resume.clone(),
            host.clone(),
            identity,
            progress.clone(),
        )
        .await;
        if resume.is_some()
            && let Err(error) = &joined
            && matches!(
                error.downcast_ref::<JoinError>(),
                Some(JoinError::Rejected(_))
            )
        {
            resume = None;
            joined = Self::connect_pinned(
                address,
                pin.clone(),
                name.clone(),
                packages,
                None,
                host.clone(),
                identity,
                progress.clone(),
            )
            .await;
        }
        let refused = match joined {
            Ok(client) => {
                // Every shared package matches. The caller has already
                // prepared client-only content before this admission.
                return Ok((client, fetched, Vec::new()));
            }
            Err(error) => error,
        };
        let Some(differ) = refused.downcast_ref::<PackagesDiffer>() else {
            return Err(refused);
        };
        if let Some(error) = fetch_failure {
            eprintln!("Joining without the server's Add-Ons: {error}");
        }
        // Shared Add-Ons only this client runs sit this game out.
        let dropped: Vec<_> = differ
            .0
            .iter()
            .filter_map(|m| match m {
                bri_package::environment::Mismatch::Extra(p) if m.blocks_join() => Some(p.clone()),
                _ => None,
            })
            .collect();
        // Whatever cannot be downloaded or loaded is joined without: the
        // server names it to the player once they are in.
        let (fetched, packages) = match load(&fetched, &dropped) {
            Ok(packages) => (fetched, packages),
            Err(error) if error.is::<JoinPreparationPending>() => return Err(error),
            Err(error) if !fetched.is_empty() => {
                eprintln!("Joining without the server's Add-Ons: {error:#}");
                (Vec::new(), load(&[], &dropped)?)
            }
            Err(error) => return Err(error),
        };
        let mut client = Self::connect_inner(
            address,
            pin,
            name,
            packages.clone(),
            resume,
            host,
            Some(identity),
            progress,
            true,
        )
        .await?;
        // A callback may have declined a downloaded shared package, or a
        // later load-error fallback may remove packages that matched the
        // first Hello. Report the final declared content, not just downloads.
        let mut unavailable: std::collections::BTreeMap<_, _> = offered_shared
            .into_iter()
            .filter(|package| !packages.contains(package))
            .map(|package| (package.id.clone(), package))
            .collect();
        for mismatch in &differ.0 {
            if let bri_package::environment::Mismatch::Missing(server)
            | bri_package::environment::Mismatch::Different { server, .. } = mismatch
                && mismatch.blocks_join()
                && !packages.contains(server)
            {
                unavailable.insert(server.id.clone(), server.clone());
            }
        }
        client.unavailable = unavailable.into_values().collect();
        Ok((client, fetched, dropped))
    }
    /// Connects like [`Client::connect_with_identity`], reporting the
    /// handshake and the world download into `progress`.
    #[allow(clippy::too_many_arguments)]
    pub async fn connect_reporting(
        address: SocketAddr,
        certificate: &[u8],
        name: JoinName,
        packages: Vec<bri_package::environment::PackageRef>,
        resume: Option<ResumeToken>,
        host: Option<ResumeToken>,
        identity: &ClientIdentity,
        progress: Progress,
    ) -> Result<Self> {
        Self::connect_pinned(
            address,
            HostPin::from(certificate),
            name,
            packages,
            resume,
            host,
            identity,
            progress,
        )
        .await
    }
    /// Connects to a host identified by `pin` (a saved certificate, an
    /// invite's key, or trust on first use).
    #[allow(clippy::too_many_arguments)]
    pub async fn connect_pinned(
        address: SocketAddr,
        pin: HostPin,
        name: JoinName,
        packages: Vec<bri_package::environment::PackageRef>,
        resume: Option<ResumeToken>,
        host: Option<ResumeToken>,
        identity: &ClientIdentity,
        progress: Progress,
    ) -> Result<Self> {
        Self::connect_inner(
            address,
            pin,
            name,
            packages,
            resume,
            host,
            Some(identity),
            progress,
            false,
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    async fn connect_inner(
        address: SocketAddr,
        pin: HostPin,
        name: JoinName,
        packages: Vec<bri_package::environment::PackageRef>,
        resume: Option<ResumeToken>,
        host: Option<ResumeToken>,
        identity: Option<&ClientIdentity>,
        progress: Progress,
        accept_differences: bool,
    ) -> Result<Self> {
        progress.begin(Stage::Connecting, Unit::Steps, None);
        let Opened {
            endpoint,
            connection,
            certificate,
            mut send,
            mut receive,
            nonce: challenge,
            listing,
        } = open(address, &pin, Duration::from_secs(10)).await?;
        let JoinName { name, clan } = name;
        let mut hello = Hello {
            version: VERSION,
            name,
            clan,
            packages,
            resume,
            host,
            identity: None,
            accept_differences,
        };
        if let Some(identity) = identity {
            let server_fingerprint: [u8; 32] = sha2::Sha256::digest(&certificate).into();
            let transcript = identity_transcript(&hello, &challenge, &server_fingerprint)?;
            hello.identity = Some(IdentityProof {
                public_key: *identity.public_key(),
                signature: identity.sign(&transcript)?.to_vec(),
            });
        }
        tokio::time::timeout(
            Duration::from_secs(10),
            codec::write_small_request(&mut send, &hello),
        )
        .await
        .map_err(|_| {
            JoinError::Connection(address, "the server stopped answering the join".into())
        })??;
        progress.begin(Stage::WaitingForServer, Unit::Steps, None);
        let welcome: Message = codec::decode(
            &tokio::time::timeout(
                Duration::from_secs(30),
                codec::read_frame(&mut receive, codec::MAX_FRAME),
            )
            .await??,
        )?;
        let (owner, administrator, resume, checkpoint) = match welcome {
            Message::Welcome {
                owner,
                administrator,
                resume,
                checkpoint,
            } => (owner, administrator, resume, checkpoint),
            Message::Rejected(reason) => return Err(JoinError::Rejected(reason).into()),
            Message::PackagesDiffer(differences) => {
                return Err(PackagesDiffer(differences).into());
            }
            _ => anyhow::bail!("Expected welcome"),
        };
        // The Welcome is small; the world streams after it in chunks.
        progress.set_subject(&checkpoint.world.map_id);
        progress.begin(
            Stage::ReceivingWorld,
            Unit::Bricks,
            Some(checkpoint.world_bricks),
        );
        // The bricks around this player arrive first; they play once those
        // are in, and the rest stream in behind them.
        let chunks = checkpoint.world_near_chunks;
        let mut world = WorldAssembly::new(checkpoint)?;
        // Frames are read as they arrive and decoded on blocking threads,
        // several at once, then added in order.
        let parallel = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
        let mut decoding = std::collections::VecDeque::new();
        let mut read = 0;
        while read < chunks || !decoding.is_empty() {
            while read < chunks && decoding.len() < parallel {
                let frame = tokio::time::timeout(
                    Duration::from_secs(30),
                    codec::read_frame(&mut receive, codec::MAX_FRAME),
                )
                .await??;
                read += 1;
                decoding.push_back(tokio::task::spawn_blocking(move || {
                    codec::decode::<Message>(&frame)
                }));
            }
            let Some(decoded) = decoding.pop_front() else {
                break;
            };
            match decoded.await?? {
                Message::WorldChunk(chunk) => {
                    let bricks = chunk.len() as u64;
                    world.add(chunk)?;
                    progress.advance(bricks);
                }
                Message::Rejected(reason) => anyhow::bail!("Join rejected: {reason}"),
                _ => anyhow::bail!("Expected world chunk"),
            }
        }
        let (checkpoint, rest) = world.split(read)?;
        let replica = Replica::new(checkpoint)?;
        ensure!(
            replica.names.contains_key(&owner),
            "Welcome has no local player"
        );
        let joining = (!rest.done()).then_some(rest);
        let (events, incoming) = mpsc::channel(128);
        let reliable_events = events.clone();
        let reader_connection = connection.clone();
        // Frames decode on blocking threads, several at once, and are handed
        // on in the order they arrived.
        let (decoded_tx, mut decoded_rx) =
            mpsc::channel::<tokio::task::JoinHandle<Result<Message>>>(parallel);
        let reader = tokio::spawn(async move {
            loop {
                let (handle, failed) = match codec::read_frame(&mut receive, codec::MAX_FRAME).await
                {
                    Ok(frame) => (
                        tokio::task::spawn_blocking(move || codec::decode::<Message>(&frame)),
                        false,
                    ),
                    Err(error) => (tokio::spawn(async move { Err(error) }), true),
                };
                if decoded_tx.send(handle).await.is_err() || failed {
                    break;
                }
            }
        });
        let forwarder = tokio::spawn(async move {
            while let Some(decoded) = decoded_rx.recv().await {
                let message = match decoded.await {
                    Ok(message) => message,
                    Err(error) => Err(error.into()),
                };
                match message {
                    Ok(message) => {
                        if reliable_events
                            .send(Incoming::Reliable(Box::new(message)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        // A read fails with a bare "connection lost"; the
                        // server's close frame (a kick's message, a
                        // shutdown) is the reason the player should see.
                        // A network drop (timeout, reset) is marked so the
                        // client can rejoin; a close the server chose is not.
                        let reason = match reader_connection.close_reason() {
                            Some(
                                quinn::ConnectionError::TimedOut | quinn::ConnectionError::Reset,
                            ) => CONNECTION_LOST.to_string(),
                            Some(reason) => reason.to_string(),
                            None => error.to_string(),
                        };
                        let _ = reliable_events.send(Incoming::Closed(reason)).await;
                        break;
                    }
                }
            }
        });
        let datagram_connection = connection.clone();
        let heard = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let heard_tick = heard.clone();
        let datagrams = tokio::spawn(async move {
            while let Ok(bytes) = datagram_connection.read_datagram().await {
                let Ok(items) = codec::decode_datagram::<Vec<Datagram>>(&bytes) else {
                    continue;
                };
                for item in items {
                    if let Datagram::Pose(Pose { tick, .. })
                    | Datagram::Remote(RemotePose { tick, .. }) = &item
                    {
                        heard_tick.fetch_max(*tick, std::sync::atomic::Ordering::Relaxed);
                    }
                    let _ = events.try_send(match item {
                        Datagram::Pose(pose) => Incoming::Pose(pose),
                        Datagram::Remote(pose) => Incoming::Pose(pose.into_pose()),
                        Datagram::Vehicle(pose) => Incoming::Vehicle(pose),
                        Datagram::Orb(orb) => Incoming::Orb(orb),
                    });
                }
            }
        });
        Ok(Self {
            endpoint,
            connection,
            send,
            incoming,
            readers: vec![reader, forwarder, datagrams],
            owner,
            administrator,
            admin_snapshot: None,
            resume,
            certificate,
            listing,
            unavailable: Vec::new(),
            replica,
            changing_map: None,
            joining,
            progress,
            sequence: 0,
            updates: Arc::default(),
            heard,
            _timers: crate::timer_resolution::Guard::acquire(),
        })
    }
    /// Whether the joined world has fully arrived (its distant bricks
    /// stream in after the join).
    pub fn world_complete(&self) -> bool {
        self.joining.is_none()
    }
    /// Receive until the joined world has fully arrived.
    pub async fn await_world(&mut self) -> Result<()> {
        while self.joining.is_some() {
            self.receive().await?;
        }
        Ok(())
    }
    /// Send the most recent prediction inputs, oldest first, ending at `newest`.
    /// The server ignores inputs it already received, so every datagram can
    /// repeat recent history and absorb isolated losses. A slow frame that ran
    /// more ticks than one datagram carries is split into several, oldest
    /// first, so no input is skipped.
    pub fn movement(
        &mut self,
        newest: u64,
        inputs: &[MoveInput],
        camera: Option<CameraView>,
        seat: Option<SeatSince>,
    ) -> Result<()> {
        ensure!(
            inputs.len() <= MAX_MOVEMENT_BATCH && newest >= inputs.len() as u64,
            "Invalid movement batch"
        );
        for input in inputs {
            input.validate()?;
        }
        let mut end = inputs.len();
        let mut datagrams = Vec::new();
        while end > 0 {
            let start = end.saturating_sub(MOVEMENT_REDUNDANCY);
            let movement = Movement {
                version: VERSION,
                newest: newest - (inputs.len() - end) as u64,
                inputs: inputs[start..end].to_vec(),
                camera,
                seat,
            };
            movement.validate()?;
            datagrams.push(codec::encode_datagram(&movement)?);
            end = start;
        }
        for bytes in datagrams.into_iter().rev() {
            self.connection.send_datagram(bytes.into())?;
        }
        Ok(())
    }
    /// Received messages waiting for `receive`. A caller that only needs the
    /// latest state can hold its work until it has handled this many more.
    pub fn queued(&self) -> usize {
        self.incoming.len()
    }
    pub async fn receive(&mut self) -> Result<ClientEvent> {
        loop {
            let incoming = self
                .incoming
                .recv()
                .await
                .context("Network receiver stopped")?;
            // The joined world's distant bricks arrive before any update.
            if let Some(rest) = self.joining.as_mut()
                && let Incoming::Reliable(message) = &incoming
                && matches!(**message, Message::WorldChunk(_))
            {
                let Incoming::Reliable(message) = incoming else {
                    unreachable!()
                };
                let Message::WorldChunk(chunk) = *message else {
                    unreachable!()
                };
                let ids = rest.add(&mut self.replica.world, chunk)?;
                self.progress.advance(ids.len() as u64);
                if rest.done() {
                    self.joining = None;
                }
                return Ok(ClientEvent::Updated {
                    world_changed: true,
                    changed_bricks: ids,
                    palette_changed: false,
                });
            }
            // A changing map's chunks arrive before anything that depends on
            // them; datagrams about the old or half-loaded map are dropped.
            if self.changing_map.is_some() {
                match incoming {
                    Incoming::Reliable(message) => match *message {
                        Message::WorldChunk(chunk) => {
                            let Some(world) = self.changing_map.as_mut() else {
                                unreachable!()
                            };
                            let bricks = chunk.len() as u64;
                            world.add(chunk)?;
                            self.progress.advance(bricks);
                            if let Some(event) = self.finish_map_change()? {
                                return Ok(event);
                            }
                        }
                        _ => anyhow::bail!("Unexpected message during a map transfer"),
                    },
                    Incoming::Pose(_) | Incoming::Vehicle(_) | Incoming::Orb(_) => {}
                    Incoming::Closed(reason) => anyhow::bail!("Connection closed: {reason}"),
                }
                continue;
            }
            if matches!(
                &incoming,
                Incoming::Pose(_) | Incoming::Vehicle(_) | Incoming::Orb(_)
            ) || matches!(&incoming, Incoming::Reliable(m) if matches!(**m, Message::Update(_)))
            {
                self.updates
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            return match incoming {
                Incoming::Reliable(message) => match *message {
                    Message::Update(delta) => {
                        let world_changed = !delta.bricks.is_empty() || delta.palette.is_some();
                        let changed_bricks = delta.bricks.keys().copied().collect();
                        let palette_changed = delta.palette.is_some();
                        self.replica.update(delta)?;
                        Ok(ClientEvent::Updated {
                            world_changed,
                            changed_bricks,
                            palette_changed,
                        })
                    }
                    Message::MapChanged(checkpoint) => {
                        let map = checkpoint.world.map_id.clone();
                        self.progress.set_subject(&map);
                        self.progress.begin(
                            Stage::ReceivingWorld,
                            Unit::Bricks,
                            Some(checkpoint.world_bricks),
                        );
                        self.changing_map = Some(WorldAssembly::new(checkpoint)?);
                        match self.finish_map_change()? {
                            Some(event) => Ok(event),
                            None => Ok(ClientEvent::MapChanging { map }),
                        }
                    }
                    Message::Reply { sequence, result } => {
                        Ok(ClientEvent::Reply { sequence, result })
                    }
                    Message::Notice(notice) => Ok(ClientEvent::Notice(notice)),
                    Message::PackageState(view) => {
                        self.replica.package_state(view)?;
                        Ok(ClientEvent::Updated {
                            world_changed: false,
                            changed_bricks: Vec::new(),
                            palette_changed: false,
                        })
                    }
                    Message::AdminSnapshot(snapshot) => {
                        self.administrator = snapshot.role.is_admin();
                        self.admin_snapshot = Some(snapshot.clone());
                        Ok(ClientEvent::AdminSnapshot(snapshot))
                    }
                    _ => anyhow::bail!("Unexpected message after welcome"),
                },
                Incoming::Pose(pose) => {
                    let id = pose.player.owner;
                    self.replica.pose(pose)?;
                    Ok(ClientEvent::Pose(id))
                }
                Incoming::Vehicle(pose) => {
                    let id = pose.id;
                    self.replica.vehicle_pose(pose)?;
                    Ok(ClientEvent::Vehicle(id))
                }
                Incoming::Orb(orb) => {
                    let owner = orb.owner;
                    self.replica.orb(orb)?;
                    Ok(ClientEvent::Orb(owner))
                }
                Incoming::Closed(reason) => anyhow::bail!("Connection closed: {reason}"),
            };
        }
    }
    /// Swap in the new map once every announced brick has arrived.
    fn finish_map_change(&mut self) -> Result<Option<ClientEvent>> {
        if !self
            .changing_map
            .as_ref()
            .is_some_and(WorldAssembly::complete)
        {
            return Ok(None);
        }
        let Some(world) = self.changing_map.take() else {
            return Ok(None);
        };
        self.replica = Replica::new(world.finish()?)?;
        Ok(Some(ClientEvent::MapChanged))
    }
    /// Send without waiting for its reply. The worker continues processing poses
    /// and correlates ClientEvent::Reply using the returned sequence.
    pub async fn request(&mut self, command: Command) -> Result<u64> {
        self.request_with_aim(command, None).await
    }
    pub async fn request_with_aim(
        &mut self,
        command: Command,
        aim: Option<bri_sim::session::ActionAim>,
    ) -> Result<u64> {
        if let Some(aim) = aim {
            aim.validate()?;
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .context("Command sequence exhausted")?;
        // The host disconnects a non-administrator whose frame exceeds the
        // player limit, so refuse it here with the stream still usable.
        let limit = if self.administrator {
            codec::MAX_REQUEST
        } else {
            codec::PLAYER_MAX_REQUEST
        };
        codec::write_request(
            &mut self.send,
            &Request::new(self.sequence, command, aim),
            limit,
        )
        .await?;
        Ok(self.sequence)
    }
    /// Sequential convenience helper for scripts/probes; interactive clients use
    /// request + receive so awaiting an edit cannot stall movement updates.
    ///
    /// It waits on the host's progress, not the wall clock: it fails when
    /// the host runs [`COMMAND_TICKS`] ticks past the request without the
    /// reply, or sends nothing at all for [`COMMAND_STALL`]. A busy machine
    /// slows the host's ticks with it, and a reply queued behind the rest of
    /// a joined world waits for those chunks, which keep arriving. The ticks
    /// count from where the host was as the request went (the newest pose
    /// datagram heard), not from the update this client last read: a client
    /// behind on reading has the host's earlier updates still to read before
    /// the reply, and those are not ticks the host spent on it.
    pub async fn command(&mut self, command: Command) -> Result<Reply> {
        let sequence = self.request(command).await?;
        let read = self.replica.tick;
        let sent = read.max(self.heard.load(std::sync::atomic::Ordering::Relaxed));
        let unread = self.queued();
        loop {
            let event = tokio::time::timeout(COMMAND_STALL, self.receive())
                .await
                .map_err(|_| {
                    anyhow::anyhow!(
                        "The server sent nothing for {} s while command {sequence} waited",
                        COMMAND_STALL.as_secs()
                    )
                })??;
            if let ClientEvent::Reply {
                sequence: reply,
                result,
            } = event
            {
                ensure!(reply == sequence, "Unexpected reply sequence");
                return result.map_err(anyhow::Error::msg);
            }
            let ran = self.replica.tick.saturating_sub(sent);
            ensure!(
                ran <= COMMAND_TICKS,
                "The server ran {ran} ticks without answering command {sequence} \
                 (sent at host tick {sent}, having read to tick {read}, {unread} events unread)"
            );
        }
    }
    /// Current QUIC round-trip estimate.
    pub fn rtt(&self) -> Duration {
        self.connection.rtt()
    }
    /// A handle the net graph samples this connection's counters with.
    pub fn link_probe(&self) -> LinkProbe {
        LinkProbe {
            connection: self.connection.clone(),
            updates: self.updates.clone(),
        }
    }
    pub fn close(&self) {
        self.connection.close(0_u32.into(), b"Client disconnect");
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.close();
        for reader in &self.readers {
            reader.abort();
        }
        self.endpoint.close(0_u32.into(), b"Client closed");
    }
}

/// A caller needs to prepare downloaded content before a game admission.
/// `connect_fetching` propagates this control flow without the load-failure
/// fallback. No Session player exists for this suspended join.
#[derive(Debug)]
pub struct JoinPreparationPending;
impl std::fmt::Display for JoinPreparationPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Loading the server's Add-Ons")
    }
}
impl std::error::Error for JoinPreparationPending {}

/// Why a join failed, in words a player can act on. Other failures stay as
/// they are.
#[derive(Debug)]
pub enum JoinError {
    /// Nothing answered: wrong address, host not running, or a firewall.
    NoAnswer(SocketAddr),
    /// The host answered with a different identity than the one pinned.
    IdentityChanged(SocketAddr),
    /// The host refused the join and said why.
    Rejected(String),
    /// The connection failed another way.
    Connection(SocketAddr, String),
}
impl JoinError {
    fn from_connection(address: SocketAddr, error: quinn::ConnectionError) -> Self {
        match &error {
            quinn::ConnectionError::TimedOut => Self::NoAnswer(address),
            quinn::ConnectionError::TransportError(e)
                if e.code.to_string().contains("CRYPTO") || e.reason.contains("certificate") =>
            {
                Self::IdentityChanged(address)
            }
            _ => Self::Connection(address, error.to_string()),
        }
    }
}
impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAnswer(address) => write!(
                f,
                "No server answered at {address}. Check the address, that the server is running, and that UDP port {} is open on the host's router and firewall.",
                address.port()
            ),
            Self::IdentityChanged(address) => write!(
                f,
                "The server at {address} has a different identity than when you last joined. If its host reinstalled the game, join again to trust the new identity."
            ),
            Self::Rejected(reason) => write!(f, "The server refused the join: {reason}"),
            Self::Connection(address, reason) => {
                write!(f, "Could not connect to {address}: {reason}")
            }
        }
    }
}
impl std::error::Error for JoinError {}

/// Which host a connection must reach. Never disables verification: the
/// handshake signature is always checked against the presented certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostPin {
    /// Accept the first certificate presented; the caller pins it from
    /// `Client::certificate` (trust on first use, like SSH).
    FirstUse,
    /// Exactly this certificate (a saved pin or a LAN listing's).
    Certificate(Vec<u8>),
    /// A certificate with this key (from an invite).
    Key(crate::invite::HostKey),
}
impl From<&[u8]> for HostPin {
    /// An empty certificate means first use.
    fn from(certificate: &[u8]) -> Self {
        if certificate.is_empty() {
            Self::FirstUse
        } else {
            Self::Certificate(certificate.to_vec())
        }
    }
}

/// What a probe learned about a server over its game port.
#[derive(Debug, Clone)]
pub struct Probe {
    pub listing: Listing,
    pub certificate: Vec<u8>,
    pub ping: Duration,
}

/// Ask a server for its listing over the game port without joining: the
/// start of a join, stopped after the host's first answer. Used by the join
/// list for saved servers and by a host checking it can be reached.
pub async fn probe(address: SocketAddr, pin: &HostPin, wait: Duration) -> Result<Probe> {
    let started = std::time::Instant::now();
    let opened = open(address, pin, wait).await?;
    let ping = opened.connection.rtt().min(started.elapsed());
    opened.connection.close(0_u32.into(), b"Probe");
    opened.endpoint.wait_idle().await;
    Ok(Probe {
        listing: opened.listing,
        certificate: opened.certificate,
        ping,
    })
}

/// A QUIC connection that has sent `JoinBegin` and read the challenge.
struct Opened {
    endpoint: quinn::Endpoint,
    connection: quinn::Connection,
    certificate: Vec<u8>,
    send: quinn::SendStream,
    receive: quinn::RecvStream,
    nonce: [u8; 32],
    listing: Listing,
}

async fn open(address: SocketAddr, pin: &HostPin, wait: Duration) -> Result<Opened> {
    let (endpoint, connection, certificate) = connect_quic(address, pin, wait).await?;
    // Every step after the handshake shares the caller's wait: a host that
    // allows no streams, grants no flow control or never answers would
    // otherwise hold the join forever.
    let (send, receive, frame) = tokio::time::timeout(wait, async {
        let (mut send, mut receive) = connection.open_bi().await?;
        codec::write_small_request(&mut send, &JoinBegin::join()).await?;
        let frame = codec::read_frame(&mut receive, codec::MAX_FRAME).await?;
        anyhow::Ok((send, receive, frame))
    })
    .await
    .map_err(|_| {
        JoinError::Connection(address, "the server stopped answering the join".into())
    })??;
    let (nonce, listing) = match codec::decode::<Message>(&frame)? {
        Message::Challenge { nonce, listing } => (nonce, listing),
        Message::Rejected(reason) => return Err(JoinError::Rejected(reason).into()),
        _ => anyhow::bail!("Expected identity challenge"),
    };
    listing.validate()?;
    Ok(Opened {
        endpoint,
        connection,
        certificate,
        send,
        receive,
        nonce,
        listing,
    })
}

/// A QUIC connection to the host `pin` names, and the certificate it
/// presented; nothing is sent yet (a join or a package download follows).
pub(crate) async fn connect_quic(
    address: SocketAddr,
    pin: &HostPin,
    wait: Duration,
) -> Result<(quinn::Endpoint, quinn::Connection, Vec<u8>)> {
    let mut config = quinn::ClientConfig::new(Arc::new(pinned_config(pin.clone())?));
    config.transport_config(Arc::new(transport()));
    let ip = if address.is_ipv4() {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    };
    let mut endpoint = quinn::Endpoint::client(SocketAddr::new(ip, 0))?;
    endpoint.set_default_client_config(config);
    let connection =
        match tokio::time::timeout(wait, endpoint.connect(address, "blockland.local")?).await {
            Ok(Ok(connection)) => connection,
            Ok(Err(error)) => return Err(JoinError::from_connection(address, error).into()),
            Err(_) => return Err(JoinError::NoAnswer(address).into()),
        };
    let certificate = connection
        .peer_identity()
        .and_then(|identity| {
            identity
                .downcast::<Vec<quinn::rustls::pki_types::CertificateDer<'static>>>()
                .ok()
        })
        .and_then(|chain| chain.first().map(|c| c.to_vec()))
        .context("The server presented no certificate")?;
    Ok((endpoint, connection, certificate))
}

/// TLS configuration that accepts the pinned host (or, for first use, any
/// host); the caller pins what it saw from `Client::certificate`.
fn pinned_config(pin: HostPin) -> Result<quinn::crypto::rustls::QuicClientConfig> {
    use quinn::rustls;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let crypto = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned { provider, pin }))
        .with_no_client_auth();
    Ok(quinn::crypto::rustls::QuicClientConfig::try_from(crypto)?)
}

#[derive(Debug)]
struct Pinned {
    provider: Arc<quinn::rustls::crypto::CryptoProvider>,
    pin: HostPin,
}
impl quinn::rustls::client::danger::ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &quinn::rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[quinn::rustls::pki_types::CertificateDer<'_>],
        _server_name: &quinn::rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: quinn::rustls::pki_types::UnixTime,
    ) -> Result<quinn::rustls::client::danger::ServerCertVerified, quinn::rustls::Error> {
        let matches = match &self.pin {
            HostPin::FirstUse => true,
            HostPin::Certificate(der) => der.as_slice() == end_entity.as_ref(),
            HostPin::Key(key) => crate::invite::host_key(end_entity) == *key,
        };
        if matches {
            Ok(quinn::rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(quinn::rustls::Error::InvalidCertificate(
                quinn::rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &quinn::rustls::pki_types::CertificateDer<'_>,
        dss: &quinn::rustls::DigitallySignedStruct,
    ) -> Result<quinn::rustls::client::danger::HandshakeSignatureValid, quinn::rustls::Error> {
        quinn::rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &quinn::rustls::pki_types::CertificateDer<'_>,
        dss: &quinn::rustls::DigitallySignedStruct,
    ) -> Result<quinn::rustls::client::danger::HandshakeSignatureValid, quinn::rustls::Error> {
        quinn::rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<quinn::rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// The close reason when the network dropped (no answer, reset), as
/// opposed to the server ending the connection on purpose. A client may
/// rejoin after this one.
/// Host ticks [`Client::command`] lets pass without its reply: ten seconds
/// of game time, the wall-clock wait it replaces on an idle machine.
pub const COMMAND_TICKS: u64 = 1200;
/// How long [`Client::command`] waits with nothing at all from the host.
pub const COMMAND_STALL: Duration = Duration::from_secs(60);
pub const CONNECTION_LOST: &str = "Lost the connection to the server";
