//! Every length is bounded before allocating. Server frames are compressed;
//! client requests are uncompressed frames with bounded JSON depth. Full native
//! event lists may be large; Hello remains small and server request admission
//! accounts for body bytes through command dispatch.
use anyhow::{Result, ensure};
use serde::{Serialize, de::DeserializeOwned};
use std::io::Read;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
pub const MAX_HELLO: usize = 64 * 1024;
/// Fits full native event lists and converted stock builds. Requests are not
/// compressed, so this is independent of the compressed server MAX_FRAME cap.
pub const MAX_REQUEST: usize = 64 * 1024 * 1024;
/// Shared queued/in-flight command body bytes, not a per-peer allowance.
pub const REQUEST_BODY_BUDGET: usize = 128 * 1024 * 1024;
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
pub const MAX_DECODED: usize = 128 * 1024 * 1024;
pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>> {
    let raw = encode_request(message, MAX_DECODED)?;
    ensure!(
        raw.len() <= MAX_DECODED,
        "Outgoing state exceeds checkpoint budget"
    );
    let packed = zstd::stream::encode_all(raw.as_slice(), 3)?;
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
    Ok(serde_json::from_slice(&decoded)?)
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
    Ok(serde_json::from_slice(
        &read_frame(stream, MAX_HELLO).await?,
    )?)
}
/// Reserve before allocating/reading the body. The caller retains the permit
/// alongside the parsed command until dispatch or rejection has completed.
pub async fn read_budgeted_request<T: DeserializeOwned>(
    stream: &mut quinn::RecvStream,
    budget: &Arc<Semaphore>,
) -> Result<(T, OwnedSemaphorePermit)> {
    let length = read_length(stream, MAX_REQUEST).await?;
    let permit = budget.clone().acquire_many_owned(length as u32).await?;
    let bytes = read_body(stream, length).await?;
    let request = serde_json::from_slice(&bytes)?;
    Ok((request, permit))
}
pub async fn read_request<T: DeserializeOwned>(stream: &mut quinn::RecvStream) -> Result<T> {
    Ok(serde_json::from_slice(
        &read_frame(stream, MAX_REQUEST).await?,
    )?)
}
pub async fn write_request<T: Serialize>(
    stream: &mut quinn::SendStream,
    request: &T,
) -> Result<()> {
    let bytes = encode_request(request, MAX_REQUEST)?;
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
    ensure!(limit <= MAX_DECODED, "Invalid serialization limit");
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
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
    };
    serde_json::to_writer(&mut writer, request)?;
    Ok(writer.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maximum_native_event_strings_fit_even_with_json_escaping() {
        let event = bri_world::Event {
            enabled: false,
            input: bri_world::Input::Activate,
            delay_ms: 300_000,
            target: bri_world::Target::Named("\u{1}".repeat(128)),
            action: bri_world::Action::Light(Some(bri_world::ContentRef::Resolved(
                "\u{1}".repeat(512),
            ))),
        };
        let request = crate::protocol::Request {
            sequence: u64::MAX,
            aim: Some(bri_sim::session::ActionAim {
                yaw: std::f32::consts::PI,
                pitch: -std::f32::consts::FRAC_PI_2,
            }),
            command: bri_sim::session::Command::Tool(bri_sim::session::ToolAction::SetEvents {
                brick: u64::MAX,
                events: vec![event; bri_world::MAX_EVENTS_PER_BRICK],
            }),
        };
        let bytes = encode_request(&request, MAX_REQUEST).unwrap();
        assert!(bytes.len() > 4 * 1024 * 1024);
        assert!(bytes.len() <= MAX_REQUEST && bytes.len() <= MAX_FRAME);
        eprintln!("Worst escaped native event request: {} bytes", bytes.len());
        let decoded: crate::protocol::Request = serde_json::from_slice(&bytes).unwrap();
        let bri_sim::session::Command::Tool(bri_sim::session::ToolAction::SetEvents {
            events, ..
        }) = decoded.command
        else {
            panic!()
        };
        assert_eq!(events.len(), bri_world::MAX_EVENTS_PER_BRICK);
    }
    #[test]
    fn serialization_limit_is_exact_and_applies_to_hello_separately() {
        // Includes JSON's surrounding quotes; reject before appending overflow.
        assert_eq!(encode_request(&"abcd", 6).unwrap(), b"\"abcd\"");
        assert!(encode_request(&"abcd", 5).is_err());
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
        write_frame(&mut send, b"null").await?;
        let (_reply, mut receive) = server_connection.accept_bi().await?;
        let budget = Arc::new(Semaphore::new(0));
        let task_budget = budget.clone();
        let task = tokio::spawn(async move {
            let result =
                read_budgeted_request::<serde_json::Value>(&mut receive, &task_budget).await;
            (receive, result)
        });
        // Body has arrived but cannot be allocated/parsed before admission.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(!task.is_finished());
        budget.add_permits(4);
        let (mut receive, result) =
            tokio::time::timeout(std::time::Duration::from_secs(2), task).await??;
        let (value, permit) = result?;
        assert_eq!(value, serde_json::Value::Null);
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
            read_budgeted_request::<serde_json::Value>(&mut receive, &budget),
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
