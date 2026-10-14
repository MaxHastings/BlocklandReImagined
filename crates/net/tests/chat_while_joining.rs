//! Joining as v20 does: the host accepts the player and tells everyone they
//! connected before the player downloads the server's Add-Ons and loads the
//! map. They chat with the players in the game the whole time, and are told
//! as spawned only when their game join puts them in the world.
use anyhow::Result;
use bri_net::{
    client::{Arrival, ArrivalEvent, Client, ClientEvent, HostPin},
    packages::PackageShelf,
    server::{self, ServerOptions},
};
use bri_package::{environment::Environment, packages::PackageSet, sync::Cache};
use bri_progress::Progress;
use bri_sim::session::{Command, Notice};
use std::{path::Path, sync::Arc, time::Duration};

mod common;
use common as fixture;

/// A server running one shared Add-On big enough to take a moment to fetch.
fn modded_server(root: &Path) -> Result<(server::ServerHandle, Environment)> {
    let files: &[(&str, &[u8])] = &[
        ("creeper/package.json", br#"{"id":"creeper"}"#),
        ("creeper/textures/skin.png", &[9; 1_500_000]),
    ];
    for (path, bytes) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, bytes)?;
    }
    let set = PackageSet::parse(
        br#"{"schema_version":1,"packages":[
            {"id":"creeper","version":"1.0.0","side":"shared","dir":"creeper"}
        ]}"#,
    )?;
    let environment = Environment::load(root, &set)?;
    let shelf = PackageShelf::new(root, &set, &environment)?;
    let server = server::start(
        fixture::session(),
        ServerOptions {
            environment: environment.clone(),
            packages: Some(Arc::new(shelf)),
            ..fixture::options()
        },
    )?;
    Ok((server, environment))
}

fn identity(dir: &Path, name: &str) -> Result<bri_identity::ClientIdentity> {
    bri_identity::ClientIdentity::load_or_create(dir.join(format!("{name}.identity")))
}

/// Read `client` until a server line ending in `ending` arrives, keeping
/// every server line read on the way.
async fn notice(client: &mut Client, ending: &str, seen: &mut Vec<String>) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let ClientEvent::Notice(Notice::Chat(text)) = client.receive().await? {
                let found = text.ends_with(ending);
                seen.push(text);
                if found {
                    return anyhow::Ok(());
                }
            }
        }
    })
    .await?
}

/// Read `client` until a chat line from `name` saying `text` is in its world.
async fn chat_line(client: &mut Client, name: &str, text: &str) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if client
                .replica
                .chat
                .iter()
                .any(|l| l.name == name && l.text == text)
            {
                return anyhow::Ok(());
            }
            client.receive().await?;
        }
    })
    .await?
}

/// Read `client` until its list of loading players is `wanted(loading)`.
async fn loading_until(
    client: &mut Client,
    wanted: impl Fn(&std::collections::BTreeMap<u64, String>) -> bool,
) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !wanted(&client.replica.loading) {
            client.receive().await?;
        }
        anyhow::Ok(())
    })
    .await?
}

/// Read the loading connection until a chat line from `name` saying `text`.
async fn arrival_line(arrival: &mut Arrival, name: &str, text: &str) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match arrival.next().await {
                Some(ArrivalEvent::Chat(lines))
                    if lines.iter().any(|l| l.name == name && l.text == text) =>
                {
                    return anyhow::Ok(());
                }
                Some(ArrivalEvent::Closed(reason)) => anyhow::bail!("closed: {reason}"),
                None => anyhow::bail!("the loading connection ended"),
                Some(_) => {}
            }
        }
    })
    .await?
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_joiner_is_connected_and_chats_while_downloading_then_spawns() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (server, environment) = modded_server(root.path())?;
    let ids = tempfile::tempdir()?;
    let mut ann = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Ann".into(),
        environment.client_packages(),
        None,
        None,
        &identity(ids.path(), "ann")?,
    )
    .await?;
    let bob_identity = identity(ids.path(), "bob")?;
    let pin = HostPin::from(&server.certificate[..]);

    // Bob's connection is accepted before anything downloads: Ann is told
    // he connected, and he has no body in her world yet.
    let mut bob = Arrival::open(server.address, &pin, "Bob".into(), None, &bob_identity).await?;
    assert_eq!(bob.name, "Bob");
    let mut said = Vec::new();
    notice(&mut ann, "Bob connected.", &mut said).await?;
    assert!(
        !said.iter().any(|s| s.ends_with("Bob spawned.")),
        "{said:?}"
    );
    assert!(!ann.replica.names.contains_key(&bob.owner));
    // He is on her player list already, as loading.
    let bob_owner = bob.owner;
    loading_until(&mut ann, |l| l.get(&bob_owner).is_some_and(|n| n == "Bob")).await?;

    // Both ways while he waits.
    bob.say("hi, still loading".into())?;
    chat_line(&mut ann, "Bob", "hi, still loading").await?;
    ann.command(Command::Chat("welcome Bob".into())).await?;
    arrival_line(&mut bob, "Ann", "welcome Bob").await?;

    // Bob downloads the server's Add-On and loads; he and Ann keep
    // chatting meanwhile, and he does not join until they have.
    let cache_dir = tempfile::tempdir()?;
    let cache = Cache::open(cache_dir.path())?;
    let mut downloaded = 0;
    let (chatted, chat_done) = tokio::sync::oneshot::channel::<()>();
    let (ticket, certificate, owner) = (bob.ticket.clone(), bob.certificate.clone(), bob.owner);
    let connect = Client::connect_arriving(
        server.address,
        HostPin::Certificate(certificate),
        "Bob".into(),
        Vec::new(),
        None,
        None,
        None,
        &bob_identity,
        &cache,
        Progress::default(),
        |fetched, _| {
            downloaded += fetched.len();
            Ok(fetched.iter().map(|f| f.package.clone()).collect())
        },
        Some(ticket),
        async {
            chat_done.await?;
            Ok(())
        },
    );
    let chat = async {
        bob.say("downloading your Add-Ons".into())?;
        chat_line(&mut ann, "Bob", "downloading your Add-Ons").await?;
        assert!(
            !ann.replica.names.contains_key(&owner),
            "Bob chatted before he spawned"
        );
        ann.command(Command::Chat("almost there".into())).await?;
        arrival_line(&mut bob, "Ann", "almost there").await?;
        let _ = chatted.send(());
        anyhow::Ok(())
    };
    let (joined, chat) = tokio::join!(connect, chat);
    chat?;
    let (joined, _, _) = joined?;
    assert_eq!(
        downloaded, 1,
        "the Add-On downloaded while Bob was connected"
    );
    assert_eq!(joined.owner, bob.owner, "the announced player spawned");
    notice(&mut ann, "Bob spawned.", &mut said).await?;
    assert_eq!(
        said.iter()
            .filter(|s| s.ends_with("Bob connected."))
            .count(),
        1,
        "{said:?}"
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        while !ann.replica.names.contains_key(&bob.owner) {
            ann.receive().await?;
        }
        anyhow::Ok(())
    })
    .await??;
    loading_until(&mut ann, |l| !l.contains_key(&bob_owner)).await?;
    // The game join ends the loading connection without a departure.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match bob.next().await {
                Some(ArrivalEvent::Closed(_)) | None => return,
                Some(_) => {}
            }
        }
    })
    .await?;
    drop(bob);
    joined.close();
    notice(&mut ann, "Bob has left the game.", &mut said).await?;
    assert_eq!(
        said.iter()
            .filter(|s| s.ends_with("Bob has left the game."))
            .count(),
        1,
        "{said:?}"
    );
    ann.close();
    let report = server.stop().await?;
    assert_eq!(report.joins, 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_joiner_who_leaves_while_loading_never_spawns() -> Result<()> {
    let server = server::start(fixture::session(), fixture::options())?;
    let ids = tempfile::tempdir()?;
    let mut ann = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Ann".into(),
        Vec::new(),
        None,
        None,
        &identity(ids.path(), "ann")?,
    )
    .await?;
    let bob = Arrival::open(
        server.address,
        &HostPin::from(&server.certificate[..]),
        "Bob".into(),
        None,
        &identity(ids.path(), "bob")?,
    )
    .await?;
    let mut said = Vec::new();
    notice(&mut ann, "Bob connected.", &mut said).await?;
    let bob_owner = bob.owner;
    loading_until(&mut ann, |l| l.contains_key(&bob_owner)).await?;
    drop(bob);
    notice(&mut ann, "Bob has left the game.", &mut said).await?;
    loading_until(&mut ann, |l| !l.contains_key(&bob_owner)).await?;
    assert!(
        !said.iter().any(|s| s.ends_with("Bob spawned.")),
        "{said:?}"
    );
    ann.close();
    let report = server.stop().await?;
    assert_eq!(report.joins, 1);
    Ok(())
}

/// A join that names a loading connection must come from the same player.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_player_cannot_spawn_as_a_loading_player() -> Result<()> {
    let server = server::start(fixture::session(), fixture::options())?;
    let ids = tempfile::tempdir()?;
    let pin = HostPin::from(&server.certificate[..]);
    let bob = Arrival::open(
        server.address,
        &pin,
        "Bob".into(),
        None,
        &identity(ids.path(), "bob")?,
    )
    .await?;
    let cache_dir = tempfile::tempdir()?;
    let cache = Cache::open(cache_dir.path())?;
    let refused = Client::connect_arriving(
        server.address,
        pin,
        "Mallory".into(),
        Vec::new(),
        None,
        None,
        None,
        &identity(ids.path(), "mallory")?,
        &cache,
        Progress::default(),
        |_, _| Ok(Vec::new()),
        Some(bob.ticket.clone()),
        async { Ok(()) },
    )
    .await
    .err()
    .expect("a stranger's join is refused");
    assert!(
        format!("{refused:#}").contains("does not match"),
        "{refused:#}"
    );
    drop(bob);
    let report = server.stop().await?;
    assert_eq!(report.joins, 0);
    Ok(())
}
