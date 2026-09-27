use crate::{codec, protocol::*, replica::Replica, server::transport};
use anyhow::{Context, Result, ensure};
use bri_sim::{
    player::MoveInput,
    session::{Command, Reply},
};
use bri_identity::ClientIdentity;
use bri_world::OwnerId;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::sync::mpsc;
use sha2::Digest;
enum Incoming {
    Reliable(Box<Message>),
    Pose(Pose),
    Vehicle(bri_sim::session::VehiclePose),
    Closed(String),
}
#[derive(Debug)]
pub enum ClientEvent {
    Updated {
        world_changed: bool,
    },
    Pose(OwnerId),
    Vehicle(u64),
    Reply {
        sequence: u64,
        result: Result<Reply, bri_sim::session::Rejection>,
    },
    AdminSnapshot(bri_sim::session::AdminSnapshot),
    Notice(bri_sim::session::Notice),
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
    pub replica: Replica,
    sequence: u64,
}
impl Client {
    /// The certificate is a trusted host pin. Never disable TLS verification.
    /// A future host browser/import flow supplies this trust decision.
    pub async fn connect(
        address: SocketAddr,
        certificate: &[u8],
        name: String,
        content_id: String,
        resume: Option<ResumeToken>,
    ) -> Result<Self> {
        Self::connect_with_host(address, certificate, name, content_id, resume, None).await
    }
    pub async fn connect_with_host(
        address: SocketAddr,
        certificate: &[u8],
        name: String,
        content_id: String,
        resume: Option<ResumeToken>,
        host: Option<ResumeToken>,
    ) -> Result<Self> {
        Self::connect_inner(address, certificate, name, content_id, resume, host, None).await
    }
    pub async fn connect_with_identity(
        address: SocketAddr,
        certificate: &[u8],
        name: String,
        content_id: String,
        resume: Option<ResumeToken>,
        host: Option<ResumeToken>,
        identity: &ClientIdentity,
    ) -> Result<Self> {
        Self::connect_inner(
            address,
            certificate,
            name,
            content_id,
            resume,
            host,
            Some(identity),
        )
        .await
    }
    async fn connect_inner(
        address: SocketAddr,
        certificate: &[u8],
        name: String,
        content_id: String,
        resume: Option<ResumeToken>,
        host: Option<ResumeToken>,
        identity: Option<&ClientIdentity>,
    ) -> Result<Self> {
        let mut roots = quinn::rustls::RootCertStore::empty();
        roots.add(certificate.to_vec().into())?;
        let mut config = quinn::ClientConfig::with_root_certificates(Arc::new(roots))?;
        config.transport_config(Arc::new(transport()));
        let ip = if address.is_ipv4() {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        };
        let mut endpoint = quinn::Endpoint::client(SocketAddr::new(ip, 0))?;
        endpoint.set_default_client_config(config);
        let connection = tokio::time::timeout(
            Duration::from_secs(10),
            endpoint.connect(address, "blockland.local")?,
        )
        .await??;
        let (mut send, mut receive) = connection.open_bi().await?;
        codec::write_small_request(&mut send, &JoinBegin { version: VERSION }).await?;
        let challenge = match codec::decode::<Message>(
            &tokio::time::timeout(
                Duration::from_secs(10),
                codec::read_frame(&mut receive, codec::MAX_FRAME),
            )
            .await??,
        )? {
            Message::Challenge { nonce } => nonce,
            Message::Rejected(reason) => anyhow::bail!("Join rejected: {reason}"),
            _ => anyhow::bail!("Expected identity challenge"),
        };
        let mut hello = Hello {
            version: VERSION,
            name,
            content_id,
            resume,
            host,
            identity: None,
        };
        if let Some(identity) = identity {
            let server_fingerprint: [u8; 32] = sha2::Sha256::digest(certificate).into();
            let transcript = identity_transcript(&hello, &challenge, &server_fingerprint)?;
            hello.identity = Some(IdentityProof {
                public_key: *identity.public_key(),
                signature: identity.sign(&transcript)?.to_vec(),
            });
        }
        codec::write_small_request(
            &mut send,
            &hello,
        )
        .await?;
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
            Message::Rejected(reason) => anyhow::bail!("Join rejected: {reason}"),
            _ => anyhow::bail!("Expected welcome"),
        };
        let replica = Replica::new(checkpoint)?;
        ensure!(
            replica.names.contains_key(&owner),
            "Welcome has no local player"
        );
        let (events, incoming) = mpsc::channel(128);
        let reliable_events = events.clone();
        let reader = tokio::spawn(async move {
            loop {
                let message = async {
                    codec::decode::<Message>(
                        &codec::read_frame(&mut receive, codec::MAX_FRAME).await?,
                    )
                }
                .await;
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
                        let _ = reliable_events
                            .send(Incoming::Closed(error.to_string()))
                            .await;
                        break;
                    }
                }
            }
        });
        let datagram_connection = connection.clone();
        let datagrams = tokio::spawn(async move {
            while let Ok(bytes) = datagram_connection.read_datagram().await {
                if bytes.len() <= MAX_DATAGRAM
                    && let Ok(datagram) = serde_json::from_slice::<Datagram>(&bytes)
                {
                    let _ = events.try_send(match datagram {
                        Datagram::Pose(pose) => Incoming::Pose(pose),
                        Datagram::Vehicle(pose) => Incoming::Vehicle(pose),
                    });
                }
            }
        });
        Ok(Self {
            endpoint,
            connection,
            send,
            incoming,
            readers: vec![reader, datagrams],
            owner,
            administrator,
            admin_snapshot: None,
            resume,
            replica,
            sequence: 0,
        })
    }
    /// Send the most recent prediction inputs, oldest first, ending at `newest`.
    /// The server ignores inputs it already received, so every datagram can
    /// repeat recent history and absorb isolated losses.
    pub fn movement(&mut self, newest: u64, inputs: &[MoveInput]) -> Result<()> {
        for input in inputs {
            input.validate()?;
        }
        let movement = Movement {
            version: VERSION,
            newest,
            inputs: inputs.to_vec(),
        };
        movement.validate()?;
        let bytes = serde_json::to_vec(&movement)?;
        ensure!(bytes.len() <= MAX_DATAGRAM, "Movement exceeds datagram budget");
        self.connection.send_datagram(bytes.into())?;
        Ok(())
    }
    pub async fn receive(&mut self) -> Result<ClientEvent> {
        match self
            .incoming
            .recv()
            .await
            .context("Network receiver stopped")?
        {
            Incoming::Reliable(message) => match *message {
                Message::Update(delta) => {
                    let world_changed = !delta.bricks.is_empty() || delta.palette.is_some();
                    self.replica.update(delta)?;
                    Ok(ClientEvent::Updated { world_changed })
                }
                Message::Reply { sequence, result } => Ok(ClientEvent::Reply { sequence, result }),
                Message::Notice(notice) => Ok(ClientEvent::Notice(notice)),
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
            Incoming::Closed(reason) => anyhow::bail!("Connection closed: {reason}"),
        }
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
        codec::write_request(
            &mut self.send,
            &Request {
                sequence: self.sequence,
                command,
                aim,
            },
        )
        .await?;
        Ok(self.sequence)
    }
    /// Sequential convenience helper for scripts/probes; interactive clients use
    /// request + receive so awaiting an edit cannot stall movement updates.
    pub async fn command(&mut self, command: Command) -> Result<Reply> {
        let sequence = self.request(command).await?;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let ClientEvent::Reply {
                    sequence: reply,
                    result,
                } = self.receive().await?
                {
                    ensure!(reply == sequence, "Unexpected reply sequence");
                    return result.map_err(anyhow::Error::msg);
                }
            }
        })
        .await?
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
