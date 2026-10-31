//! Real control delivery while the world's receive queue is demonstrably full.
use super::*;
use crate::server::{self, ServerOptions};
use bri_package::packages::{PackageEntry, PackageSet, Side};

fn voice_server() -> Result<server::ServerHandle> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages");
    let catalog = bri_package_runtime::Catalog::load(
        &root,
        &PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "voice_chat".into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: "voice_chat".into(),
                role: None,
            }],
        },
        true,
    )
    .map_err(|d| anyhow::anyhow!("{d:?}"))?;
    let mut session = bri_sim::session::Session::new(bri_sim::simulation::Simulation::new(
        bri_world::World::new("Control".into(), "fixture".into(), vec![[1.; 4]; 2]),
        bri_sim::testing::definitions(),
        vec![],
    )?);
    session.install_packages(Arc::new(catalog), None)?;
    server::start(
        session,
        ServerOptions {
            bind: "127.0.0.1:0".parse()?,
            environment: bri_package::environment::Environment::empty(),
            spawn_points: vec![glam::Vec3::new(0., 10., 0.), glam::Vec3::new(5., 10., 0.)],
            certificate: None,
            map_loader: None,
            packages: None,
        },
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn voice_control_survives_a_saturated_world_queue_without_ui_polling() -> Result<()> {
    let server = voice_server()?;
    let mut a = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Host".into(),
        vec![],
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    let b = Client::connect(
        server.address,
        &server.certificate,
        "Paused UI".into(),
        vec![],
        None,
    )
    .await?;
    let link = b.voice_link();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !link.routes().routes.contains_key(&a.owner) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    let control = link.control();
    assert!(control.metadata.enabled);
    assert_eq!(
        control.metadata.conversation.as_ref().unwrap().title,
        "Server Lobby"
    );
    assert_eq!(control.metadata.conversation.as_ref().unwrap().game, None);
    // Never call b.receive: real replicated poses fill its bounded world queue.
    tokio::time::timeout(Duration::from_secs(10), async {
        while b.incoming.capacity() > 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    assert_eq!(b.incoming.capacity(), 0);
    a.command(Command::MiniGame(
        bri_sim::session::MiniGameRequest::Create {
            color: 0,
            settings: Default::default(),
        },
    ))
    .await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while link.routes().routes.contains_key(&a.owner) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    assert!(link.control().revision > control.revision);
    assert_eq!(
        b.incoming.capacity(),
        0,
        "voice control must not drain or depend on world events"
    );
    a.command(Command::Admin(bri_admin::Request::new(
        bri_admin::Action::VoiceMute {
            target: bri_admin::ConnectionId(b.owner),
        },
    )))
    .await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while !link.control().metadata.muted.contains(&b.owner) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    a.command(Command::Admin(bri_admin::Request::new(
        bri_admin::Action::VoiceUnmute {
            target: bri_admin::ConnectionId(b.owner),
        },
    )))
    .await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while link.control().metadata.muted.contains(&b.owner) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    assert_eq!(b.incoming.capacity(), 0);
    b.close();
    a.close();
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn voice_ready_requires_admission_and_closes_a_join_rejected_by_its_observer() -> Result<()> {
    let session = bri_sim::session::Session::new(bri_sim::simulation::Simulation::new(
        bri_world::World::new("Early voice".into(), "fixture".into(), vec![[1.; 4]; 2]),
        bri_sim::testing::definitions(),
        vec![],
    )?);
    let server = server::start(
        session,
        ServerOptions {
            bind: "127.0.0.1:0".parse()?,
            environment: bri_package::environment::Environment::empty(),
            spawn_points: vec![glam::Vec3::new(0., 10., 0.), glam::Vec3::new(5., 10., 0.)],
            certificate: None,
            map_loader: None,
            packages: None,
        },
    )?;
    let tmp = tempfile::tempdir()?;
    let identity = ClientIdentity::load_or_create(tmp.path().join("observer.identity"))?;
    let observed = Arc::new(std::sync::Mutex::new(None));
    let output = observed.clone();
    let observer: VoiceReady = Arc::new(move |link| {
        *output.lock().unwrap() = Some(link);
        anyhow::bail!("observer rejected this join")
    });
    let refused = Client::connect_reporting(
        server.address,
        &server.certificate,
        "Probe".into(),
        vec![],
        None,
        Some(ResumeToken([0; 32])),
        &identity,
        Progress::default(),
        Some(observer.clone()),
    )
    .await
    .err()
    .context("invalid host credential must fail")?;
    assert!(refused.to_string().contains("host credential"));
    assert!(
        observed.lock().unwrap().is_none(),
        "no observer before authenticated admission"
    );
    let canceled = Client::connect_reporting(
        server.address,
        &server.certificate,
        "Probe".into(),
        vec![],
        None,
        None,
        &identity,
        Progress::default(),
        Some(observer),
    )
    .await
    .err()
    .context("observer must cancel")?;
    assert!(canceled.to_string().contains("observer rejected"));
    let link = observed
        .lock()
        .unwrap()
        .take()
        .context("accepted voice link was not published")?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while !link.is_closed() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    let inspector_id = ClientIdentity::load_or_create(tmp.path().join("inspector.identity"))?;
    let mut inspector = Arrival::open(
        server.address,
        &HostPin::from(&server.certificate[..]),
        "Inspector".into(),
        None,
        &inspector_id,
    )
    .await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(ArrivalEvent::Players(rows)) = inspector.next().await
                && rows.iter().all(|p| p.name != "Probe")
            {
                break;
            }
        }
    })
    .await?;
    inspector.close();
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn voice_capture_scope_rejects_old_audio_and_activity_after_membership_regrant() -> Result<()>
{
    use bri_sim::session::MiniGameRequest;
    let server = voice_server()?;
    let mut a = Client::connect(
        server.address,
        &server.certificate,
        "Speaker".into(),
        vec![],
        None,
    )
    .await?;
    let b = Client::connect(
        server.address,
        &server.certificate,
        "Listener".into(),
        vec![],
        None,
    )
    .await?;
    let talk = a.voice_link();
    let listen = b.voice_link();
    let wait_epoch = |previous: Option<u64>| {
        let talk = talk.clone();
        async move {
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let epoch = talk.control().metadata.source_epoch;
                    if epoch > 0 && Some(epoch) != previous {
                        break epoch;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
        }
    };
    let old = wait_epoch(None).await?;
    a.command(Command::MiniGame(MiniGameRequest::Create {
        color: 0,
        settings: Default::default(),
    }))
    .await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while talk.control().metadata.source_epoch != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    a.command(Command::MiniGame(MiniGameRequest::Leave)).await?;
    let new = wait_epoch(Some(old)).await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while !listen.routes().routes.contains_key(&a.owner) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    let mut heard = vec![];
    let mut activity = vec![];
    listen.take_heard(&mut heard);
    heard.clear();
    listen.take_activity(&mut activity);
    activity.clear();
    // Bypass VoiceLink's local stale-frame check to challenge HOST admission.
    for sequence in 0..3 {
        a.connection
            .send_datagram(crate::voice::encode_upstream(sequence, old, &[99; 60])?.into())?;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    a.connection
        .send_datagram(crate::voice::encode_upstream_activity(true, old).into())?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    listen.take_heard(&mut heard);
    listen.take_activity(&mut activity);
    assert!(
        heard.is_empty(),
        "old capture cannot be restamped into the regranted conversation"
    );
    assert!(
        activity.iter().all(|a| !a.active),
        "old capture cannot resurrect speech activity"
    );
    talk.send_captured(10, new, &[7; 60])?;
    talk.send_activity(true)?;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            listen.take_heard(&mut heard);
            listen.take_activity(&mut activity);
            if heard.iter().any(|f| f.payload == vec![7; 60]) && activity.iter().any(|a| a.active) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    assert!(heard.iter().all(|f| f.payload != vec![99; 60]));
    a.close();
    b.close();
    server.stop().await?;
    Ok(())
}
