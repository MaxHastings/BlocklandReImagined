//! The one wire format. Every message is MessagePack with named struct
//! fields: binary (floats and integers at native width, about half of JSON's
//! size and much faster to encode) yet self-describing, so every serde
//! representation the shared types use, including adjacently tagged
//! commands, round-trips exactly. `protocol::VERSION` pins the schema.
//! Reliable frames are length-prefixed; server frames are also zstd
//! compressed. Unreliable datagrams are uncompressed and compact: fields by
//! position rather than by name, several state items packed into one
//! datagram (see `docs/audits/network-bandwidth.md`).
//! Every length is bounded before allocating, decoding rejects trailing
//! bytes, and server request admission accounts for body bytes through
//! command dispatch.
use anyhow::{Context, Result, ensure};
use serde::{Serialize, de::DeserializeOwned};
use std::io::Read;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
pub const MAX_HELLO: usize = 64 * 1024;
/// Fits full native event lists and converted stock builds. Requests are not
/// compressed, so this is independent of the compressed server MAX_FRAME cap.
pub const MAX_REQUEST: usize = 64 * 1024 * 1024;
/// Largest command a player without the bulk capability (build loading, held
/// by administrators) may send. The worst native event list is ~2.4 MB.
pub const PLAYER_MAX_REQUEST: usize = 4 * 1024 * 1024;
/// Each peer's own queued/in-flight command body bytes. One peer's traffic
/// only ever waits on this allowance, never on another peer's.
pub const PEER_REQUEST_BUDGET: usize = PLAYER_MAX_REQUEST;
/// Every command reserves at least this much of its peer's allowance, which
/// bounds a peer to 8 commands in flight however small they are.
pub const MIN_REQUEST_COST: usize = PEER_REQUEST_BUDGET / 8;
/// Shared queued/in-flight bytes of bulk commands larger than
/// [`PLAYER_MAX_REQUEST`], which only administrators may send.
pub const BULK_REQUEST_BUDGET: usize = 128 * 1024 * 1024;
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
pub const MAX_DECODED: usize = 128 * 1024 * 1024;
use crate::protocol::MAX_DATAGRAM;
/// Server frame: bounded MessagePack, then zstd.
pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>> {
    let raw = encode_request(message, MAX_DECODED).context("Outgoing state exceeds budget")?;
    let packed = zstd::stream::encode_all(raw.as_slice(), 1)?;
    ensure!(packed.len() <= MAX_FRAME, "Outgoing frame exceeds budget");
    Ok(packed)
}
pub fn decode<T: DeserializeOwned>(frame: &[u8]) -> Result<T> {
    ensure!(frame.len() <= MAX_FRAME, "Oversized compressed frame");
    let mut decoded = Vec::new();
    let mut decoder = zstd::stream::read::Decoder::new(frame)?;
    decoder.window_log_max(27)?;
    decoder
        .take(MAX_DECODED as u64 + 1)
        .read_to_end(&mut decoded)?;
    ensure!(
        decoded.len() <= MAX_DECODED,
        "Expanded frame exceeds budget"
    );
    from_bytes(&decoded)
}
/// One unreliable datagram in the compact form, bounded so it always fits a
/// QUIC datagram frame.
pub fn encode_datagram<T: Serialize>(message: &T) -> Result<Vec<u8>> {
    encode_with(message, MAX_DATAGRAM, false).context("Datagram exceeds budget")
}
/// Largest array header [`pack_datagrams`] writes before its items.
const ARRAY_HEADER: usize = 3;
/// Largest item [`pack_datagrams`] carries: an item and its batch's header
/// always fit one datagram.
pub const MAX_DATAGRAM_ITEM: usize = MAX_DATAGRAM - ARRAY_HEADER;
/// One item of a datagram batch in the compact form.
pub fn encode_datagram_item<T: Serialize>(item: &T) -> Result<Vec<u8>> {
    encode_with(item, MAX_DATAGRAM_ITEM, false).context("Datagram item exceeds budget")
}
/// Pack encoded items, in order, into as few datagrams as fit: each one a
/// MessagePack array of items, decoded with `decode_datagram::<Vec<T>>`.
/// One packet's header costs more than a pose, so this is most of the win.
pub fn pack_datagrams<'a>(items: impl IntoIterator<Item = &'a [u8]>) -> Vec<Vec<u8>> {
    fn finish(out: &mut Vec<Vec<u8>>, items: &mut Vec<&[u8]>) {
        if items.is_empty() {
            return;
        }
        let mut bytes = Vec::with_capacity(MAX_DATAGRAM);
        match items.len() {
            n @ 0..=15 => bytes.push(0x90 | n as u8),
            n => {
                bytes.push(0xdc);
                bytes.extend_from_slice(&(n as u16).to_be_bytes());
            }
        }
        for item in items.drain(..) {
            bytes.extend_from_slice(item);
        }
        out.push(bytes);
    }
    let mut out = Vec::new();
    let mut batch = Vec::new();
    let mut size = ARRAY_HEADER;
    for item in items {
        debug_assert!(item.len() <= MAX_DATAGRAM_ITEM);
        if size + item.len() > MAX_DATAGRAM {
            finish(&mut out, &mut batch);
            size = ARRAY_HEADER;
        }
        size += item.len();
        batch.push(item);
    }
    finish(&mut out, &mut batch);
    out
}
pub fn decode_datagram<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    ensure!(bytes.len() <= MAX_DATAGRAM, "Oversized datagram");
    from_bytes(bytes)
}
/// Strict decode: the whole buffer is exactly one message.
fn from_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value = rmp_serde::from_read(&mut cursor).context("Malformed message")?;
    ensure!(
        cursor.position() == bytes.len() as u64,
        "Trailing bytes after message"
    );
    Ok(value)
}
pub async fn write_frame(stream: &mut quinn::SendStream, bytes: &[u8]) -> Result<()> {
    ensure!(bytes.len() <= MAX_FRAME, "Oversized frame");
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .await?;
    stream.write_all(bytes).await?;
    Ok(())
}
pub async fn read_frame(stream: &mut quinn::RecvStream, limit: usize) -> Result<Vec<u8>> {
    let length = read_length(stream, limit).await?;
    read_body(stream, length).await
}
async fn read_length(stream: &mut quinn::RecvStream, limit: usize) -> Result<usize> {
    let mut length = [0; 4];
    stream.read_exact(&mut length).await?;
    let length = u32::from_le_bytes(length) as usize;
    ensure!(length > 0 && length <= limit, "Invalid frame length");
    Ok(length)
}
async fn read_body(stream: &mut quinn::RecvStream, length: usize) -> Result<Vec<u8>> {
    let mut bytes = vec![0; length];
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        stream.read_exact(&mut bytes),
    )
    .await??;
    Ok(bytes)
}
pub async fn read_small_request<T: DeserializeOwned>(stream: &mut quinn::RecvStream) -> Result<T> {
    from_bytes(&read_frame(stream, MAX_HELLO).await?)
}
/// Reserve before allocating/reading the body. `admit` maps the declared
/// length to the budget it draws on and the permits it reserves, or refuses
/// it before a byte of the body is read. The caller retains the permit
/// alongside the parsed command until dispatch or rejection has completed.
pub async fn read_budgeted_request<T: DeserializeOwned>(
    stream: &mut quinn::RecvStream,
    admit: impl FnOnce(usize) -> Result<(Arc<Semaphore>, u32)>,
) -> Result<(T, OwnedSemaphorePermit)> {
    let length = read_length(stream, MAX_REQUEST).await?;
    let (budget, cost) = admit(length)?;
    let permit = budget.acquire_many_owned(cost).await?;
    let bytes = read_body(stream, length).await?;
    let request = from_bytes(&bytes)?;
    Ok((request, permit))
}
pub async fn read_request<T: DeserializeOwned>(stream: &mut quinn::RecvStream) -> Result<T> {
    from_bytes(&read_frame(stream, MAX_REQUEST).await?)
}
pub async fn write_request<T: Serialize>(
    stream: &mut quinn::SendStream,
    request: &T,
    limit: usize,
) -> Result<()> {
    ensure!(limit <= MAX_REQUEST, "Invalid request limit");
    let bytes = encode_request(request, limit)?;
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .await?;
    stream.write_all(&bytes).await?;
    Ok(())
}
pub async fn write_small_request<T: Serialize>(
    stream: &mut quinn::SendStream,
    request: &T,
) -> Result<()> {
    write_frame(stream, &encode_request(request, MAX_HELLO)?).await
}
/// Bound the serialization buffer as it grows, before any stream bytes are
/// written. A rejected local request leaves the framed stream synchronized.
pub fn encode_request<T: Serialize>(request: &T, limit: usize) -> Result<Vec<u8>> {
    encode_with(request, limit, true)
}
/// Named (self-describing maps) or compact (positional arrays) MessagePack,
/// bounded as it grows.
fn encode_with<T: Serialize>(request: &T, limit: usize, named: bool) -> Result<Vec<u8>> {
    ensure!(limit <= MAX_DECODED, "Invalid serialization limit");
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
        overflowed: bool,
    }
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                self.overflowed = true;
                return Err(std::io::Error::other("Oversized request"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded {
        bytes: Vec::new(),
        limit,
        overflowed: false,
    };
    let result = if named {
        rmp_serde::encode::write_named(&mut writer, request)
    } else {
        rmp_serde::encode::write(&mut writer, request)
    };
    ensure!(!writer.overflowed, "Oversized request");
    result.context("Could not encode message")?;
    Ok(writer.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maximum_native_event_strings_fit_even_with_json_escaping() {
        let event = bri_world::EventRow {
            preserved: None,
            enabled: false,
            input: "\u{1}".repeat(128),
            delay_ms: 300_000,
            target: bri_world::EventTarget::Named("\u{1}".repeat(128)),
            output: "\u{1}".repeat(128),
            params: vec![bri_world::EventValue::Datablock(Some("\u{1}".repeat(256))); 4],
        };
        let preserved = bri_world::EventRow {
            preserved: Some(bri_events::PreservedRow {
                original: "\u{1}".repeat(2048),
                diagnostic: "\u{1}".repeat(1024),
            }),
            enabled: false,
            input: String::new(),
            delay_ms: 0,
            target: bri_world::EventTarget::Slot(bri_events::Slot::SelfBrick),
            output: String::new(),
            params: vec![],
        };
        let mut rows = vec![event; bri_world::MAX_EVENTS_PER_BRICK / 2];
        rows.resize(bri_world::MAX_EVENTS_PER_BRICK, preserved);
        let request = crate::protocol::Request {
            sequence: u64::MAX,
            aim: Some(bri_sim::session::ActionAim {
                yaw: std::f32::consts::PI,
                pitch: -std::f32::consts::FRAC_PI_2,
            }),
            command: bri_sim::session::Command::Tool(bri_sim::session::ToolAction::SetEvents {
                brick: u64::MAX,
                events: rows,
            }),
        };
        let bytes = encode_request(&request, MAX_REQUEST).unwrap();
        assert!(bytes.len() <= PLAYER_MAX_REQUEST && bytes.len() <= MAX_FRAME);
        eprintln!("Worst native event request: {} bytes", bytes.len());
        let decoded: crate::protocol::Request = from_bytes(&bytes).unwrap();
        let bri_sim::session::Command::Tool(bri_sim::session::ToolAction::SetEvents {
            events, ..
        }) = decoded.command
        else {
            panic!()
        };
        assert_eq!(events.len(), bri_world::MAX_EVENTS_PER_BRICK);
    }
    #[test]
    fn tagged_commands_round_trip_and_trailing_bytes_are_rejected() {
        use bri_sim::session::Command;
        for command in [
            Command::Activate,
            Command::Chat("hi".into()),
            Command::SwitchSeat(-1),
            Command::Suicide,
        ] {
            let request = crate::protocol::Request {
                sequence: 7,
                command,
                aim: None,
            };
            let mut bytes = encode_request(&request, MAX_HELLO).unwrap();
            let decoded: crate::protocol::Request = from_bytes(&bytes).unwrap();
            assert_eq!(format!("{decoded:?}"), format!("{request:?}"));
            bytes.push(0);
            assert!(from_bytes::<crate::protocol::Request>(&bytes).is_err());
        }
    }
    #[test]
    fn worst_case_datagrams_fit_the_datagram_budget() {
        use crate::protocol::*;
        let input = bri_sim::player::MoveInput {
            forward: -1.0,
            right: 1.0,
            yaw: -std::f32::consts::PI,
            pitch: std::f32::consts::FRAC_PI_2,
            head_yaw: 1.0,
            jump: true,
            crouch: true,
            jet: true,
        };
        let movement = Movement {
            version: VERSION,
            newest: u64::MAX,
            inputs: vec![input; MOVEMENT_REDUNDANCY],
            camera: Some(bri_sim::session::CameraView {
                eye: [-999_999.9; 3],
                yaw: -std::f32::consts::PI,
                pitch: -std::f32::consts::FRAC_PI_2,
            }),
        };
        let bytes = encode_datagram(&movement).unwrap();
        let pose = Datagram::Pose(Pose {
            tick: u64::MAX,
            acknowledged_input: u64::MAX,
            player: bri_sim::player::PlayerState {
                owner: u64::MAX,
                feet: [f32::MAX; 3],
                velocity: [f32::MAX; 3],
                yaw: 1.0,
                pitch: 1.0,
                head_yaw: 1.0,
                grounded: true,
                crouched: true,
                jetting: true,
                jump: Default::default(),
                archetype: bri_sim::archetype::ArchetypeId(u16::MAX),
                scale: f32::MAX,
                energy: f32::MAX,
                tick: bri_sim::player::TorqueTick {
                    feet: [f32::MAX; 3],
                    from: [f32::MAX; 3],
                    phase: u8::MAX,
                    jump: true,
                },
            },
        });
        let vehicle = Datagram::Vehicle(bri_sim::session::VehiclePose {
            id: u64::MAX,
            tick: u64::MAX,
            position: [1.0; 3],
            rotation: [1.0; 4],
            velocity: [1.0; 3],
            steering: 1.0,
            wheel_suspension: vec![1.0; 16],
            wheel_rotation: vec![1.0; 16],
            wheel_contact: vec![true; 16],
            turret_aim: [1.0; 2],
            jetting: true,
        });
        eprintln!(
            "Datagrams: movement {} bytes, pose {} bytes, vehicle {} bytes",
            bytes.len(),
            encode_datagram(&pose).unwrap().len(),
            encode_datagram(&vehicle).unwrap().len()
        );
        let orb = Datagram::Orb(Orb {
            tick: u64::MAX,
            owner: u64::MAX,
            eye: [f32::MAX; 3],
        });
        let items = [pose, vehicle, orb];
        let encoded: Vec<_> = items
            .iter()
            .map(|d| encode_datagram_item(d).unwrap())
            .collect();
        let packed = pack_datagrams(encoded.iter().map(Vec::as_slice));
        assert_eq!(packed.len(), 1);
        assert_eq!(decode_datagram::<Vec<Datagram>>(&packed[0]).unwrap(), items);
        // Many items split into full datagrams, in order.
        let many: Vec<_> = (0..40)
            .flat_map(|_| encoded.iter().map(Vec::as_slice))
            .collect();
        let packed = pack_datagrams(many.iter().copied());
        assert!(packed.len() > 1 && packed.iter().all(|d| d.len() <= MAX_DATAGRAM));
        let decoded: Vec<Datagram> = packed
            .iter()
            .flat_map(|d| decode_datagram::<Vec<Datagram>>(d).unwrap())
            .collect();
        assert_eq!(decoded.len(), 120);
        assert_eq!(decoded[..3], items);
        let decoded: Movement = decode_datagram(&bytes).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded.sequenced().last().unwrap().0, u64::MAX);
        let hostile = Movement {
            newest: u64::MAX,
            ..decoded
        };
        assert_eq!(hostile.sequenced().count(), MOVEMENT_REDUNDANCY);
    }
    #[test]
    fn serialization_limit_is_exact_and_applies_to_hello_separately() {
        // Includes the length prefix; reject before appending overflow.
        assert_eq!(encode_request(&"abcd", 5).unwrap(), b"\xa4abcd");
        assert!(encode_request(&"abcd", 4).is_err());
        let large = "a".repeat(MAX_HELLO);
        assert!(encode_request(&large, MAX_HELLO).is_err());
        assert!(encode_request(&large, MAX_REQUEST).is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quic_request_budget_is_reserved_before_body_and_released_by_dispatch_owner()
    -> Result<()> {
        let cert = rcgen::generate_simple_self_signed(vec!["blockland.local".into()])?;
        let certificate = cert.cert.der().clone();
        let key =
            quinn::rustls::pki_types::PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
        let config = quinn::ServerConfig::with_single_cert(vec![certificate.clone()], key.into())?;
        let server = quinn::Endpoint::server(config, "127.0.0.1:0".parse()?)?;
        let mut roots = quinn::rustls::RootCertStore::empty();
        roots.add(certificate)?;
        let mut client = quinn::Endpoint::client("127.0.0.1:0".parse()?)?;
        client.set_default_client_config(quinn::ClientConfig::with_root_certificates(Arc::new(
            roots,
        ))?);
        let connecting = client.connect(server.local_addr()?, "blockland.local")?;
        let incoming = server.accept().await.unwrap();
        let (client_connection, server_connection) =
            tokio::try_join!(connecting, incoming.into_future())?;
        let (mut send, _reply) = client_connection.open_bi().await?;
        write_frame(&mut send, &encode_request(&"abc", 4)?).await?; // 4 bytes.
        let (_reply, mut receive) = server_connection.accept_bi().await?;
        let budget = Arc::new(Semaphore::new(0));
        let task_budget = budget.clone();
        let task = tokio::spawn(async move {
            let result = read_budgeted_request::<String>(&mut receive, |length| {
                Ok((task_budget, length as u32))
            })
            .await;
            (receive, result)
        });
        // Body has arrived but cannot be allocated/parsed before admission.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(!task.is_finished());
        budget.add_permits(4);
        let (mut receive, result) =
            tokio::time::timeout(std::time::Duration::from_secs(2), task).await??;
        let (value, permit) = result?;
        assert_eq!(value, "abc");
        assert_eq!(budget.available_permits(), 0);
        drop(permit); // The server Event::Command owns this until dispatch.
        assert_eq!(budget.available_permits(), 4);
        // Only the oversized prefix is sent. Failure must be immediate without
        // waiting for its body or acquiring any of the now-zero body budget.
        let held = budget.clone().acquire_many_owned(4).await?;
        send.write_all(&((MAX_REQUEST + 1) as u32).to_le_bytes())
            .await?;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            read_budgeted_request::<String>(&mut receive, |length| {
                Ok((budget.clone(), length as u32))
            }),
        )
        .await?;
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid frame length")
        );
        assert_eq!(budget.available_permits(), 0);
        drop(held);
        client.close(0_u32.into(), b"test complete");
        server.close(0_u32.into(), b"test complete");
        Ok(())
    }
}
