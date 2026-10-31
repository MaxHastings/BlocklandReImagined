//! Real control delivery while the world's receive queue is demonstrably full.
use super::*;
use crate::server::{self, ServerOptions};
use bri_package::packages::{PackageEntry, PackageSet, Side};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn voice_control_survives_a_saturated_world_queue_without_ui_polling() -> Result<()> {
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
