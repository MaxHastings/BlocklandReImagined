//! Hostile hosts against the joining client (`docs/audits/red-team.md`):
//! each must end in a clear refusal within the wait the caller gave, never
//! a hang or a panic.
use bri_net::client::{HostPin, probe};
use bri_net::server::{HostCertificate, transport};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A QUIC host with a real certificate whose transport is `configure`d,
/// accepting connections and then doing whatever `serve` does with them.
async fn hostile_host<F, Fut>(
    configure: impl FnOnce(&mut quinn::TransportConfig),
    serve: F,
) -> SocketAddr
where
    F: Fn(quinn::Connection) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let identity = HostCertificate::generate().unwrap();
    let key = quinn::rustls::pki_types::PrivatePkcs8KeyDer::from(identity.key.clone());
    let mut config =
        quinn::ServerConfig::with_single_cert(vec![identity.der.clone().into()], key.into())
            .unwrap();
    let mut t = transport();
    configure(&mut t);
    config.transport_config(Arc::new(t));
    let endpoint = quinn::Endpoint::server(config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let address = endpoint.local_addr().unwrap();
    let serve = Arc::new(serve);
    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let serve = serve.clone();
            tokio::spawn(async move {
                if let Ok(connection) = incoming.await {
                    serve(connection).await;
                }
            });
        }
    });
    address
}

const WAIT: Duration = Duration::from_millis(1500);

async fn refused_in_time(address: SocketAddr) -> String {
    let started = Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(20),
        probe(address, &HostPin::FirstUse, WAIT),
    )
    .await
    .expect("the join hung");
    let error = outcome.err().expect("a hostile host was accepted");
    assert!(
        started.elapsed() < WAIT * 3,
        "took {:?} to refuse",
        started.elapsed()
    );
    format!("{error:#}")
}

/// A host that completes the handshake but allows no streams: the client
/// could never send its first request and waited forever.
#[tokio::test]
async fn a_host_that_allows_no_streams_is_refused_in_time() {
    let address = hostile_host(
        |t| {
            t.max_concurrent_bidi_streams(0_u32.into());
        },
        |connection| async move {
            let _ = connection.closed().await;
        },
    )
    .await;
    refused_in_time(address).await;
}

/// A host that accepts the stream and then says nothing.
#[tokio::test]
async fn a_host_that_never_answers_is_refused_in_time() {
    let address = hostile_host(
        |_| {},
        |connection| async move {
            let _streams = connection.accept_bi().await;
            let _ = connection.closed().await;
        },
    )
    .await;
    refused_in_time(address).await;
}

/// A host that answers the first request with garbage.
#[tokio::test]
async fn a_host_that_answers_garbage_is_refused() {
    let address = hostile_host(
        |_| {},
        |connection| async move {
            if let Ok((mut send, _receive)) = connection.accept_bi().await {
                let _ = send
                    .write_all(&[5, 0, 0, 0, 0xde, 0xad, 0xbe, 0xef, 0x00])
                    .await;
                let _ = send.write_all(&u32::MAX.to_le_bytes()).await;
            }
            let _ = connection.closed().await;
        },
    )
    .await;
    refused_in_time(address).await;
}

/// A port that answers every datagram with garbage is not a host.
#[tokio::test]
async fn a_port_that_answers_garbage_is_no_answer() {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buffer = [0u8; 2048];
        while let Ok((len, from)) = socket.recv_from(&mut buffer).await {
            let mut reply = buffer[..len].to_vec();
            reply.reverse();
            let _ = socket.send_to(&reply, from).await;
            let _ = socket.send_to(b"\xc0garbage", from).await;
        }
    });
    let error = refused_in_time(address).await;
    assert!(error.contains("No server answered"), "{error}");
}

/// Invites are typed and pasted by players and passed around in chat: no
/// text may panic the parser, and every refusal says what to type.
#[test]
fn no_invite_text_panics_the_parser() {
    use bri_net::invite::JoinTarget;
    let key = "a".repeat(26);
    let mut hostile = vec![
        String::new(),
        "bri://".into(),
        "bri:///".into(),
        format!("bri:///{key}"),
        format!("bri://:28000/{key}"),
        format!("bri://1.2.3.4:99999/{key}"),
        format!("bri://1.2.3.4:28000/{key}/{key}"),
        format!("bri://1.2.3.4:28000/{}", "7".repeat(26)),
        format!("bri://1.2.3.4:28000/{}", "a".repeat(10_000)),
        format!("bri://{}:28000/{key}", "a".repeat(100_000)),
        "BRI://é".into(),
        "bríéé://".into(),
        "[::1".into(),
        "::1]:28000".into(),
        "[]:28000".into(),
        "1.2.3.4:-1".into(),
        "\u{0}".into(),
        "é:28000".into(),
        "a.b.c.d.e.f.g:65535".into(),
    ];
    // Every prefix of a real invite, and every character swapped for one
    // that is out of place.
    let real = bri_net::invite::invite("203.0.113.10:28000".parse().unwrap(), b"cert");
    for end in 0..real.len() {
        hostile.push(real[..end].to_string());
    }
    for (i, _) in real.char_indices() {
        for c in ['/', ':', '[', 'é', '\u{0}', ' '] {
            let mut damaged = real.clone();
            damaged.replace_range(i..i + 1, &c.to_string());
            hostile.push(damaged);
        }
    }
    for text in &hostile {
        if let Ok(target) = JoinTarget::parse(text) {
            // Whatever parses round-trips to something that parses the same.
            assert_eq!(
                JoinTarget::parse(&target.to_string()).unwrap(),
                target,
                "{text:?}"
            );
        }
    }
}
