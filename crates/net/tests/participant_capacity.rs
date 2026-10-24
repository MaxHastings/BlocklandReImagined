//! The combined bot/human roster must survive ordinary network join and updates.
use anyhow::Result;
use bri_net::{client::Client, server};
use bri_sim::session::MAX_PARTICIPANTS;
use glam::Vec3;
use std::time::Duration;

mod common;

#[test]
fn combined_admission_remains_bounded() -> Result<()> {
    let mut session = common::session();
    for i in 0..MAX_PARTICIPANTS {
        session.join(
            format!("Participant {i}"),
            Vec3::new((i % 16) as f32 * 4. - 32., 0.05, (i / 16) as f32 * 4. + 20.),
            false,
        )?;
    }
    assert!(session.join("Overflow".into(), Vec3::ZERO, false).is_err());
    assert_eq!(session.names().len(), MAX_PARTICIPANTS);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_clients_accept_checkpoint_and_join_delta_above_64_participants() -> Result<()> {
    let mut session = common::session();
    // Existing participants use ordinary Session admission. The bot gauntlet
    // separately exercises 64 actual bots plus its two human builders.
    for i in 0..64 {
        session.join(
            format!("Existing {i}"),
            Vec3::new((i % 16) as f32 * 4. - 32., 0.05, (i / 16) as f32 * 4. + 20.),
            false,
        )?;
    }
    let server = server::start(session, common::options())?;
    let mut first = Client::connect(
        server.address,
        &server.certificate,
        "First observer".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert_eq!(first.replica.names.len(), 65);
    let second = Client::connect(
        server.address,
        &server.certificate,
        "Second observer".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert_eq!(second.replica.names.len(), 66);
    tokio::time::timeout(Duration::from_secs(15), async {
        while first.replica.names.len() != 66 {
            first.receive().await?;
        }
        anyhow::Ok(())
    })
    .await??;
    assert_eq!(first.replica.names, second.replica.names);
    assert_eq!(
        first.replica.tools.keys().collect::<Vec<_>>(),
        second.replica.tools.keys().collect::<Vec<_>>()
    );
    first.close();
    second.close();
    server.stop().await?;
    Ok(())
}
