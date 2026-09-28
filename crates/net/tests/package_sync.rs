//! Stress campaign category 3 (multiplayer distribution): a clean client
//! fetches a modded server's packages before joining. Every test builds its
//! packages on disk from the literal files below, so each run is replayable.
use anyhow::Result;
use bri_net::{
    codec,
    packages::{PackageShelf, fetch_missing},
    protocol::{DownloadReply, DownloadRequest, JoinBegin, Message, Purpose, VERSION},
    server::{self, ServerOptions},
};
use bri_package::{
    environment::{Environment, hash_dir},
    packages::PackageSet,
    sync::{Cache, Listing},
};
use bri_progress::{Progress, Stage};
use sha2::Digest;
use std::{path::Path, sync::Arc, time::Duration};

mod common;
use common as fixture;

/// A content root with four packages. `creeper` and `zombies` share a
/// texture; `rules` is server-only and must never be offered.
fn content(root: &Path) -> Result<(PackageSet, Environment)> {
    let files: &[(&str, &[u8])] = &[
        ("creeper/package.json", br#"{"id":"creeper"}"#),
        ("creeper/models/creeper.glb", &[7; 300_000]),
        ("creeper/textures/shared.png", &[9; 1_500_000]),
        ("zombies/package.json", br#"{"id":"zombies"}"#),
        ("zombies/textures/shared.png", &[9; 1_500_000]),
        ("hud/panels/wallet.json", br#"{"slot":"hud.top_right"}"#),
        ("rules/rules.rhai", b"// server-only behaviour"),
    ];
    for (path, bytes) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, bytes)?;
    }
    let set = PackageSet::parse(
        br#"{"schema_version":1,"packages":[
            {"id":"creeper","version":"1.0.0","side":"shared","dir":"creeper"},
            {"id":"zombies","version":"2.1.0","side":"shared","dir":"zombies"},
            {"id":"hud","version":"1.0.0","side":"client","dir":"hud"},
            {"id":"rules","version":"1.0.0","side":"server","dir":"rules"}
        ]}"#,
    )?;
    let environment = Environment::load(root, &set)?;
    Ok((set, environment))
}

fn modded_server(root: &Path) -> Result<(server::ServerHandle, Environment)> {
    let (set, environment) = content(root)?;
    let shelf = PackageShelf::new(root, &set, &environment)?;
    let server = server::start(
        fixture::session(),
        ServerOptions {
            packages: Some(Arc::new(shelf)),
            ..fixture::options()
        },
    )?;
    Ok((server, environment))
}

/// E6. A clean client downloads exactly the shared and client packages, the
/// shared texture once, reports progress, and installs packages that hash to
/// what the server loaded. A second fetch downloads nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_clean_client_fetches_verifies_and_reuses_the_servers_packages() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (server, environment) = modded_server(root.path())?;
    let cache_dir = tempfile::tempdir()?;
    let cache = Cache::open(cache_dir.path())?;
    let progress = Progress::default();
    let fetched = fetch_missing(server.address, &server.certificate, &cache, &progress).await?;
    let ids: Vec<_> = fetched.iter().map(|f| f.package.id.as_str()).collect();
    assert_eq!(
        ids,
        ["creeper", "zombies", "hud"],
        "server-only rules never offered"
    );
    for fetched in &fetched {
        assert_eq!(hash_dir(&fetched.dir)?.0, fetched.package.hash);
        assert!(environment.packages.contains(&fetched.package));
    }
    let total: u64 = fetched.iter().map(|f| f.downloaded).sum();
    let unique = 16 + 300_000 + 1_500_000 + 16 + 24;
    assert_eq!(total, unique as u64, "the shared texture downloads once");
    let snapshot = progress.snapshot();
    assert_eq!(snapshot.stage, Stage::DownloadingPackages);
    assert_eq!((snapshot.done, snapshot.total), (total, Some(total)));
    let again = fetch_missing(server.address, &server.certificate, &cache, &progress).await?;
    assert!(again.iter().all(|f| f.downloaded == 0), "cache reused");
    server.stop().await?;
    Ok(())
}

/// E7. What a client can ask a download connection for is exactly the
/// offered files: a server-only package, an arbitrary hash and a range past
/// the end are refused, and a host without a shelf refuses downloads.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn downloads_reach_only_offered_files() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (server, environment) = modded_server(root.path())?;
    let rules = environment
        .packages
        .iter()
        .find(|p| p.id == "rules")
        .unwrap();
    let texture = Listing::of(&root.path().join("creeper"), &environment.packages[0])?
        .files
        .into_iter()
        .find(|f| f.path.ends_with("shared.png"))
        .unwrap();
    let rules_file = Listing::of(&root.path().join("rules"), rules)?.files[0].clone();
    // Replay: each request on a fresh download connection.
    for request in [
        DownloadRequest::Listing {
            hash: rules.hash.clone(),
        },
        DownloadRequest::Object {
            sha256: rules_file.sha256.clone(),
            offset: 0,
            length: 1,
        },
        DownloadRequest::Object {
            sha256: "ab".repeat(32),
            offset: 0,
            length: 1,
        },
        DownloadRequest::Object {
            sha256: texture.sha256.clone(),
            offset: texture.size,
            length: 1,
        },
        DownloadRequest::Object {
            sha256: texture.sha256.clone(),
            offset: 0,
            length: bri_net::protocol::MAX_OBJECT_CHUNK + 1,
        },
    ] {
        let reply = raw_download(&server, &request).await?;
        assert!(
            matches!(reply, DownloadReply::Refused(_)),
            "{request:?} answered {reply:?}"
        );
    }
    let plain = server::start(fixture::session(), fixture::options())?;
    let cache_dir = tempfile::tempdir()?;
    let error = fetch_missing(
        plain.address,
        &plain.certificate,
        &Cache::open(cache_dir.path())?,
        &Progress::default(),
    )
    .await
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("does not offer package downloads"),
        "{error:#}"
    );
    plain.stop().await?;
    server.stop().await?;
    Ok(())
}

/// E8. Download connections are bounded per address like joins: a third
/// concurrent download from one address is refused with a reason.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn download_connections_are_bounded_per_address() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (server, _) = modded_server(root.path())?;
    let mut held = Vec::new();
    for _ in 0..2 {
        let (endpoint, connection, mut send, receive) = open_download(&server).await?;
        codec::write_small_request(&mut send, &DownloadRequest::Environment).await?;
        held.push((endpoint, connection, send, receive));
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let cache_dir = tempfile::tempdir()?;
    let error = fetch_missing(
        server.address,
        &server.certificate,
        &Cache::open(cache_dir.path())?,
        &Progress::default(),
    )
    .await
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("Too many package downloads"),
        "{error:#}"
    );
    drop(held);
    server.stop().await?;
    Ok(())
}

/// E9. A hostile or broken server: honest listings, but one object's bytes
/// are corrupted, or the connection dies mid-file. The fetch fails naming the
/// package, nothing unverified is installed, and a later fetch from an honest
/// server completes, reusing the files that did verify.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn corrupt_or_interrupted_downloads_fail_safely_and_resume() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (set, environment) = content(root.path())?;
    let shelf = Arc::new(PackageShelf::new(root.path(), &set, &environment)?);
    let cache_dir = tempfile::tempdir()?;
    let cache = Cache::open(cache_dir.path())?;
    for fault in [Fault::Corrupt, Fault::Interrupt] {
        let (address, certificate, task) = lying_server(root.path(), &set, &environment, fault)?;
        let error = fetch_missing(address, &certificate, &cache, &Progress::default())
            .await
            .unwrap_err();
        let text = format!("{error:#}");
        match fault {
            Fault::Corrupt => assert!(text.contains("do not match their hash"), "{text}"),
            Fault::Interrupt => assert!(!text.is_empty()),
        }
        for package in environment.client_packages() {
            if package.id == "creeper" {
                assert!(cache.installed(&package).is_none(), "{fault:?} installed");
            }
        }
        task.abort();
    }
    let server = server::start(
        fixture::session(),
        ServerOptions {
            packages: Some(shelf),
            ..fixture::options()
        },
    )?;
    let fetched = fetch_missing(
        server.address,
        &server.certificate,
        &cache,
        &Progress::default(),
    )
    .await?;
    let creeper = fetched.iter().find(|f| f.package.id == "creeper").unwrap();
    assert_eq!(hash_dir(&creeper.dir)?.0, creeper.package.hash);
    assert!(
        creeper.downloaded < creeper.package.size,
        "files that verified before the fault were kept"
    );
    server.stop().await?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum Fault {
    /// Flip the first byte of the creeper model's first chunk.
    Corrupt,
    /// Close the connection on the first chunk of the shared texture.
    Interrupt,
}

/// A download-only server that follows the protocol but misbehaves once.
fn lying_server(
    root: &Path,
    set: &PackageSet,
    environment: &Environment,
    fault: Fault,
) -> Result<(std::net::SocketAddr, Vec<u8>, tokio::task::JoinHandle<()>)> {
    let shelf = Arc::new(PackageShelf::new(root, set, environment)?);
    let identity = server::HostCertificate::generate()?;
    let key = quinn::rustls::pki_types::PrivatePkcs8KeyDer::from(identity.key.clone());
    let mut config =
        quinn::ServerConfig::with_single_cert(vec![identity.der.clone().into()], key.into())?;
    config.transport_config(Arc::new(server::transport()));
    let endpoint = quinn::Endpoint::server(config, "127.0.0.1:0".parse()?)?;
    let address = endpoint.local_addr()?;
    let listings: Vec<Listing> = environment
        .client_packages()
        .iter()
        .map(|p| Listing::of(&root.join(&p.id), p))
        .collect::<Result<_>>()?;
    let model = listings[0]
        .files
        .iter()
        .find(|f| f.path.ends_with(".glb"))
        .unwrap()
        .sha256
        .clone();
    let texture = listings[0]
        .files
        .iter()
        .find(|f| f.path.ends_with(".png"))
        .unwrap()
        .sha256
        .clone();
    let root = root.to_path_buf();
    let task = tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let Ok(connection) = incoming.await else {
                continue;
            };
            let Ok((mut send, mut receive)) = connection.accept_bi().await else {
                continue;
            };
            let _: Result<JoinBegin> = codec::read_small_request(&mut receive).await;
            while let Ok(request) = codec::read_small_request::<DownloadRequest>(&mut receive).await
            {
                let reply = match &request {
                    DownloadRequest::Environment => DownloadReply::Environment(
                        listings.iter().map(|l| l.package.clone()).collect(),
                    ),
                    DownloadRequest::Listing { hash } => DownloadReply::Listing(Box::new(
                        listings
                            .iter()
                            .find(|l| &l.package.hash == hash)
                            .unwrap()
                            .clone(),
                    )),
                    DownloadRequest::Object {
                        sha256,
                        offset,
                        length,
                    } => {
                        if matches!(fault, Fault::Interrupt) && *sha256 == texture {
                            connection.close(0_u32.into(), b"gone");
                            break;
                        }
                        let path = listings
                            .iter()
                            .flat_map(|l| l.files.iter().map(move |f| (l, f)))
                            .find(|(_, f)| &f.sha256 == sha256)
                            .map(|(l, f)| root.join(&l.package.id).join(&f.path))
                            .unwrap();
                        let mut bytes =
                            bri_package::sync::read_range(&path, *offset, *length as usize)
                                .unwrap();
                        if matches!(fault, Fault::Corrupt) && *sha256 == model && *offset == 0 {
                            bytes[0] ^= 0xff;
                        }
                        DownloadReply::Object(bytes)
                    }
                };
                if codec::write_frame(&mut send, &codec::encode(&reply).unwrap())
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    });
    let _ = shelf;
    Ok((address, identity.der, task))
}

async fn open_download(
    server: &server::ServerHandle,
) -> Result<(
    quinn::Endpoint,
    quinn::Connection,
    quinn::SendStream,
    quinn::RecvStream,
)> {
    let mut roots = quinn::rustls::RootCertStore::empty();
    roots.add(server.certificate.clone().into())?;
    let mut config = quinn::ClientConfig::with_root_certificates(Arc::new(roots))?;
    config.transport_config(Arc::new(server::transport()));
    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse()?)?;
    endpoint.set_default_client_config(config);
    let connection = endpoint.connect(server.address, "blockland.local")?.await?;
    let (mut send, receive) = connection.open_bi().await?;
    codec::write_small_request(
        &mut send,
        &JoinBegin {
            version: VERSION,
            purpose: Purpose::Download,
        },
    )
    .await?;
    Ok((endpoint, connection, send, receive))
}

async fn raw_download(
    server: &server::ServerHandle,
    request: &DownloadRequest,
) -> Result<DownloadReply> {
    let (_endpoint, _connection, mut send, mut receive) = open_download(server).await?;
    codec::write_small_request(&mut send, request).await?;
    let frame = codec::read_frame(&mut receive, codec::MAX_FRAME).await?;
    match codec::decode::<DownloadReply>(&frame) {
        Ok(reply) => Ok(reply),
        Err(_) => match codec::decode::<Message>(&frame)? {
            Message::Rejected(reason) => Ok(DownloadReply::Refused(reason)),
            other => anyhow::bail!("unexpected {other:?}"),
        },
    }
}

/// E10 (category 8, lifecycle). Two fetches into one cache at once, as when
/// a player opens a second server while the first is still downloading.
/// Both finish with verified packages and the cache holds only good objects.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_fetches_into_one_cache_both_verify() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (server, _) = modded_server(root.path())?;
    let cache_dir = tempfile::tempdir()?;
    for round in 0..3 {
        let cache = Arc::new(Cache::open(&cache_dir.path().join(round.to_string()))?);
        let fetches: Vec<_> = (0..2)
            .map(|_| {
                let cache = cache.clone();
                let (address, certificate) = (server.address, server.certificate.clone());
                tokio::spawn(async move {
                    fetch_missing(address, &certificate, &cache, &Progress::default()).await
                })
            })
            .collect();
        for fetch in fetches {
            for fetched in fetch.await?? {
                assert_eq!(hash_dir(&fetched.dir)?.0, fetched.package.hash);
            }
        }
        for object in std::fs::read_dir(cache_dir.path().join(round.to_string()).join("objects"))? {
            let object = object?;
            let bytes = std::fs::read(object.path())?;
            assert_eq!(
                hex(&sha2::Sha256::digest(&bytes)),
                object.file_name().to_string_lossy(),
                "an object in the cache does not match its name"
            );
        }
    }
    server.stop().await?;
    Ok(())
}

/// E11 (category 8, lifecycle). The cache is damaged at rest: an object is
/// truncated and a file inside an installed package is edited (a crash, a
/// disk fault, a curious player). The next fetch notices, repairs what it
/// needs from the server and returns packages that hash to their identity.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_damaged_cache_is_detected_and_repaired_by_the_next_fetch() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (server, _) = modded_server(root.path())?;
    let cache_dir = tempfile::tempdir()?;
    let cache = Cache::open(cache_dir.path())?;
    let fetched = fetch_missing(
        server.address,
        &server.certificate,
        &cache,
        &Progress::default(),
    )
    .await?;
    let creeper = fetched.iter().find(|f| f.package.id == "creeper").unwrap();
    std::fs::write(creeper.dir.join("models/creeper.glb"), b"edited")?;
    let texture = hex(&sha2::Sha256::digest([9_u8; 1_500_000]));
    std::fs::write(cache_dir.path().join("objects").join(&texture), b"trunc")?;
    std::fs::remove_dir_all(
        fetched
            .iter()
            .find(|f| f.package.id == "zombies")
            .unwrap()
            .dir
            .clone(),
    )?;
    let again = fetch_missing(
        server.address,
        &server.certificate,
        &cache,
        &Progress::default(),
    )
    .await?;
    for fetched in &again {
        assert_eq!(
            hash_dir(&fetched.dir)?.0,
            fetched.package.hash,
            "{} was returned damaged",
            fetched.package
        );
    }
    server.stop().await?;
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// E12 (category 8, lifecycle). The host edits a package file while hosting,
/// the usual way a mod is iterated on. A client fetching afterwards is told
/// the package changed on the host, rather than failing a hash check that
/// reads as a hostile server, and nothing unverified is installed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_package_edited_while_hosting_is_reported_not_served() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (server, environment) = modded_server(root.path())?;
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(
        root.path().join("creeper/models/creeper.glb"),
        [8_u8; 300_000],
    )?;
    let cache_dir = tempfile::tempdir()?;
    let cache = Cache::open(cache_dir.path())?;
    let error = fetch_missing(
        server.address,
        &server.certificate,
        &cache,
        &Progress::default(),
    )
    .await
    .unwrap_err();
    let text = format!("{error:#}");
    assert!(
        text.contains("Package `creeper` changed on the host"),
        "{text}"
    );
    let creeper = environment
        .packages
        .iter()
        .find(|p| p.id == "creeper")
        .unwrap();
    assert!(cache.installed(creeper).is_none());
    server.stop().await?;
    Ok(())
}

/// E31 (multiplayer/distribution). A clean client joins a modded server:
/// the join is refused naming the shared packages it lacks, the client
/// downloads them over the download connection, loads them and joins with
/// the server's package list. A client running a shared package the server
/// does not is refused without downloading anything.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_join_downloads_the_missing_packages_and_joins() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (set, environment) = content(root.path())?;
    let shelf = PackageShelf::new(root.path(), &set, &environment)?;
    let server = server::start(
        fixture::session(),
        ServerOptions {
            environment: environment.clone(),
            packages: Some(Arc::new(shelf)),
            ..fixture::options()
        },
    )?;
    let identity_dir = tempfile::tempdir()?;
    let identity =
        bri_identity::ClientIdentity::load_or_create(identity_dir.path().join("client.identity"))?;
    let cache_dir = tempfile::tempdir()?;
    let cache = Cache::open(cache_dir.path())?;

    // Without downloading, the refusal names what differs.
    let refused = bri_net::client::Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Plain".into(),
        Vec::new(),
        None,
        None,
        &identity,
    )
    .await
    .err()
    .unwrap();
    let differ = refused
        .downcast_ref::<bri_net::client::PackagesDiffer>()
        .expect("a typed package refusal");
    let missing: Vec<_> = differ.0.iter().map(|m| m.to_string()).collect();
    assert_eq!(missing.len(), 2, "{missing:?}");
    assert!(missing.iter().all(|m| m.starts_with("server has")));

    let mut loaded = Vec::new();
    let (client, fetched, _) = bri_net::client::Client::connect_fetching(
        server.address,
        bri_net::client::HostPin::from(&server.certificate[..]),
        "Fetcher".into(),
        Vec::new(),
        None,
        &identity,
        &cache,
        Progress::default(),
        |fetched, dropped| {
            assert!(dropped.is_empty());
            loaded = fetched.iter().map(|f| f.dir.clone()).collect();
            Ok(fetched.iter().map(|f| f.package.clone()).collect())
        },
    )
    .await?;
    assert!(client.owner > 0);
    assert_eq!(fetched.len(), 3, "shared and client packages");
    assert!(loaded.iter().all(|dir| dir.is_dir()));
    drop(client);

    // A shared package only the client runs sits the game out: the join
    // leaves it out rather than refusing.
    let mut extra = environment.client_packages();
    let mymod = bri_package::environment::PackageRef {
        id: "mymod".into(),
        version: "1.0.0".into(),
        side: bri_package::packages::Side::Shared,
        hash: "ab".repeat(32),
        size: 1,
    };
    extra.push(mymod.clone());
    let (client, _, dropped) = bri_net::client::Client::connect_fetching(
        server.address,
        bri_net::client::HostPin::from(&server.certificate[..]),
        "Extra".into(),
        extra.clone(),
        None,
        &identity,
        &cache,
        Progress::default(),
        |fetched, dropped| {
            Ok(extra
                .iter()
                .filter(|p| !dropped.contains(p) && !fetched.iter().any(|f| f.package.id == p.id))
                .cloned()
                .chain(fetched.iter().map(|f| f.package.clone()))
                .collect())
        },
    )
    .await?;
    assert!(client.owner > 0);
    assert_eq!(dropped, [mymod]);
    drop(client);
    server.stop().await?;
    Ok(())
}

/// What a join runs with: the packages the client loaded, with every one
/// the server sent in place of the client's copy of the same id and the
/// left-out ones removed (`bri_client::mods::load_fetched` does this).
fn joined(
    have: &[bri_package::environment::PackageRef],
    fetched: &[bri_net::packages::Fetched],
    dropped: &[bri_package::environment::PackageRef],
) -> Vec<bri_package::environment::PackageRef> {
    have.iter()
        .filter(|p| !dropped.contains(p) && !fetched.iter().any(|f| f.package.id == p.id))
        .cloned()
        .chain(fetched.iter().map(|f| f.package.clone()))
        .collect()
}

/// Joining downloads whatever the server runs and plays, with no question
/// however large: a client with no Add-Ons at all, and one with an older
/// copy of the server's Add-On, both end up in the game running exactly
/// the server's Add-Ons, matched by content hash (a cached copy of another
/// version is never used).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_join_downloads_exactly_the_servers_add_ons_without_asking() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (set, environment) = content(root.path())?;
    let shelf = PackageShelf::new(root.path(), &set, &environment)?;
    let server = server::start(
        fixture::session(),
        ServerOptions {
            environment: environment.clone(),
            packages: Some(Arc::new(shelf)),
            ..fixture::options()
        },
    )?;
    let identity_dir = tempfile::tempdir()?;
    let identity =
        bri_identity::ClientIdentity::load_or_create(identity_dir.path().join("client.identity"))?;
    let expected = environment.client_packages();
    let join = async |have: Vec<bri_package::environment::PackageRef>, cache: &Cache| {
        let mut ran = Vec::new();
        let (client, fetched, dropped) = bri_net::client::Client::connect_fetching(
            server.address,
            bri_net::client::HostPin::from(&server.certificate[..]),
            "Joiner".into(),
            have.clone(),
            None,
            &identity,
            cache,
            Progress::default(),
            |fetched, dropped| {
                ran = joined(&have, fetched, dropped);
                Ok(ran.clone())
            },
        )
        .await?;
        anyhow::ensure!(client.owner > 0, "not in the game");
        drop(client);
        anyhow::Ok((ran, fetched, dropped))
    };

    // No Add-Ons at all.
    let clean = tempfile::tempdir()?;
    let (ran, fetched, dropped) = join(Vec::new(), &Cache::open(clean.path())?).await?;
    assert!(dropped.is_empty());
    assert!(fetched.iter().any(|f| f.downloaded > 0));
    let mut ran_sorted = ran.clone();
    ran_sorted.sort_by(|a, b| a.id.cmp(&b.id));
    let mut want = expected.clone();
    want.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(ran_sorted, want, "the server's exact Add-Ons");

    // An older copy of `creeper`, already in this client's cache too.
    let stale_root = tempfile::tempdir()?;
    std::fs::create_dir_all(stale_root.path().join("creeper"))?;
    std::fs::write(stale_root.path().join("creeper/package.json"), br#"{"id":"creeper"}"#)?;
    std::fs::write(stale_root.path().join("creeper/old.glb"), [1; 64])?;
    let (hash, size) = hash_dir(&stale_root.path().join("creeper"))?;
    let creeper = expected.iter().find(|p| p.id == "creeper").unwrap();
    let stale = bri_package::environment::PackageRef {
        hash,
        size,
        version: "0.9.0".into(),
        ..creeper.clone()
    };
    assert_ne!(stale.hash, creeper.hash);
    let (ran, fetched, _) = join(vec![stale.clone()], &Cache::open(clean.path())?).await?;
    assert!(!ran.contains(&stale), "the stale copy is not run");
    let got = fetched.iter().find(|f| f.package.id == "creeper").unwrap();
    assert_eq!(got.package, *creeper);
    assert_eq!(hash_dir(&got.dir)?.0, creeper.hash, "the server's bytes");
    let mut ran_sorted = ran;
    ran_sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(ran_sorted, want, "the server's exact Add-Ons");
    server.stop().await?;
    Ok(())
}

/// Base game content (an entry with a role) is never offered: joiners on
/// the same build have it, and others cannot run it. Only Add-Ons are.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_add_ons_are_offered_never_base_game_content() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (_, _) = content(root.path())?;
    std::fs::create_dir_all(root.path().join("base-ui"))?;
    std::fs::write(root.path().join("base-ui/ui-pack.json"), b"{}")?;
    let set = PackageSet::parse(
        br#"{"schema_version":1,"packages":[
            {"id":"v20-ui","version":"3.0.0","side":"client","dir":"base-ui","role":"ui_pack"},
            {"id":"hud","version":"1.0.0","side":"client","dir":"hud"}
        ]}"#,
    )?;
    let environment = Environment::load(root.path(), &set)?;
    let shelf = PackageShelf::new(root.path(), &set, &environment)?;
    let server = server::start(
        fixture::session(),
        ServerOptions {
            environment: environment.clone(),
            packages: Some(Arc::new(shelf)),
            ..fixture::options()
        },
    )?;
    let DownloadReply::Environment(offered) =
        raw_download(&server, &DownloadRequest::Environment).await?
    else {
        anyhow::bail!("expected the offered list");
    };
    let ids: Vec<_> = offered.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["hud"]);
    server.stop().await?;
    Ok(())
}
