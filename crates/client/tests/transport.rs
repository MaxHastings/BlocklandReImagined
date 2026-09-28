use anyhow::{Result, ensure};
use bri_client::network::{Connected, Event, Worker};
use bri_net::{
    client::Client,
    server::{self, ServerOptions},
};
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{Command, Reply, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ui_transport_pipelines_replies_while_motion_advances_and_cancel_stops_host() -> Result<()>
{
    tokio::time::timeout(Duration::from_secs(15), async {
        let session = Session::new(Simulation::new(
            World::new("Test".into(), "fixture".into(), vec![[1.0; 4]]),
            Definitions::default(),
            vec![],
        )?);
        let host = server::start_with_limit(
            session,
            ServerOptions {
                bind: "127.0.0.1:0".parse()?,
                environment: bri_package::environment::Environment::empty(),
                spawn_points: vec![Vec3::new(0.0, 100.0, 0.0)],
                certificate: None,
                map_loader: None,
                autosave: None,
            },
            1,
        )?;
        let address = host.address;
        let certificate = host.certificate.clone();
        let pin = certificate.clone();
        let kept = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let keep = kept.clone();
        let keep_world: server::SaveWorld = std::sync::Arc::new(move |world: &World| {
            keep.lock().unwrap().push(world.map_id.clone());
            Ok(())
        });
        let mut worker = Worker::start(&tokio::runtime::Handle::current(), async move {
            let client =
                Client::connect(address, &pin, "Builder".into(), Vec::new(), None).await?;
            Ok(Connected {
                client,
                host: Some(host),
                package_save: None,
                keep_world: Some(keep_world),
            })
        });
        ensure!(
            matches!(worker.events.recv().await, Some(Event::Ready)),
            "No ready event"
        );
        let initial = worker.view.borrow().clone().unwrap();
        assert!(
            Client::connect(
                address,
                &certificate,
                "Extra".into(),
                Vec::new(),
                None
            )
            .await
            .is_err()
        );
        worker.movement(
            6,
            vec![
                MoveInput {
                    forward: 1.0,
                    ..Default::default()
                };
                6
            ],
        )?;
        worker.request(101, Command::Chat("one".into()))?;
        worker.request(102, Command::Chat("two".into()))?;
        let mut replies = Vec::new();
        while replies.len() < 2 {
            match worker.events.recv().await.context("Worker closed")? {
                Event::Reply { request, result } => {
                    ensure!(matches!(result, Ok(Reply::Accepted)), "Rejected chat");
                    replies.push(request);
                }
                Event::Failed(e) => anyhow::bail!(e),
                Event::Ready => anyhow::bail!("Duplicate ready"),
                Event::MapChanged(map) => anyhow::bail!("Unexpected map change to {map}"),
                Event::Notice(_) => {}
                Event::Presentation { cues, dropped } => {
                    assert!(cues.is_empty());
                    assert_eq!(dropped, 0);
                }
            }
        }
        assert_eq!(replies, vec![101, 102]);
        loop {
            worker.view.changed().await?;
            let current = worker.view.borrow().clone().unwrap();
            if current.tick > initial.tick + 12 && current.chat.len() == 2 {
                assert!(
                    current.poses[&current.owner].player.feet[2]
                        < initial.poses[&initial.owner].player.feet[2]
                );
                // Motion-only updates retain the world Arc rather than copying all bricks.
                assert!(std::sync::Arc::ptr_eq(&initial.world, &current.world));
                break;
            }
        }
        let task = worker.finish().context("Transport task")?;
        while let Some(event) = worker.events.recv().await {
            if let Event::Failed(e) = event {
                anyhow::bail!(e);
            }
        }
        // Stopping the host keeps the world it ended with.
        task.await?;
        assert_eq!(*kept.lock().unwrap(), vec!["fixture".to_string()]);
        Ok(())
    })
    .await?
}
use anyhow::Context;
