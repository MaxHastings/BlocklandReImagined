//! Native bulk loading through a live QUIC server, without a window or OS input.
use anyhow::{Context, Result, ensure};
use bri_net::{
    client::Client,
    protocol::public_brick,
    server::{self, ServerOptions},
};
use bri_sim::{
    definitions::Definitions,
    session::{Command, Reply, Session},
    simulation::Simulation,
};
use bri_world::{World, build::SavedBuild};
use glam::Vec3;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 4,
        "Usage: build_load_probe <catalog-dir> <native-content> <world-dir> <report.json>"
    );
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(args[2].join("report.json"))?)?;
    let mut results = vec![];
    for name in ["Bedroom/Demo House.bls", "Slate/Golden Gate Bridge.bls"] {
        let entry = index["saves"]
            .as_array()
            .context("Missing saves")?
            .iter()
            .find(|s| s["source"].as_str() == Some(name))
            .context("Missing reference save")?;
        let original = bri_world::persistence::load(
            &args[2].join(entry["file"].as_str().context("Missing file")?),
        )?;
        let count = original.bricks.len();
        let build = SavedBuild::capture(&original, true, true)?;
        let bytes = bri_world::build::encode(&build)?.len();
        let empty = World::new(
            "Live load".into(),
            original.map_id.clone(),
            original.palette.clone(),
        );
        let session = Session::new(Simulation::new(
            empty,
            Definitions::load(&args[0], &args[1])?,
            vec![],
        )?);
        let server = server::start(
            session,
            ServerOptions {
                bind: "127.0.0.1:0".parse()?,
                environment: bri_package::environment::Environment::empty(),
                spawn_points: vec![Vec3::splat(2000.0), Vec3::splat(2003.0)],
                certificate: None,
                map_loader: None,
                autosave: None,
            },
        )?;
        let mut host = Client::connect_with_host(
            server.address,
            &server.certificate,
            "Host".into(),
            Vec::new(),
            None,
            Some(server.host_token.clone()),
        )
        .await?;
        let start = Instant::now();
        ensure!(
            host.command(Command::LoadBuild {
                build: Box::new(build),
                ownership: false
            })
            .await?
                == Reply::Loaded { bricks: count },
            "Unexpected load reply"
        );
        let accepted_ms = start.elapsed().as_secs_f64() * 1000.0;
        tokio::time::timeout(Duration::from_secs(15), async {
            while host.replica.world.bricks.len() != count {
                host.receive().await?;
            }
            Result::<()>::Ok(())
        })
        .await??;
        let replicated_ms = start.elapsed().as_secs_f64() * 1000.0;
        let late = Client::connect(
            server.address,
            &server.certificate,
            "Late".into(),
            Vec::new(),
            None,
        )
        .await?;
        ensure!(
            late.replica.world == host.replica.world,
            "Late join differs from loaded replica"
        );
        let Reply::Saved(saved) = host
            .command(Command::SaveBuild {
                events: true,
                ownership: true,
            })
            .await?
        else {
            anyhow::bail!("Missing save reply")
        };
        for (loaded, source) in saved.world.bricks.values().zip(original.bricks.values()) {
            ensure!(
                loaded.source_records == source.source_records
                    && loaded.print == source.print
                    && loaded.events.len() == source.events.len(),
                "Original records/print/events were lost"
            );
            ensure!(
                loaded.owner == host.owner
                    && loaded.position == source.position
                    && loaded.definition == source.definition
                    && saved.world.palette[loaded.color as usize]
                        == original.palette[source.color as usize],
                "Loaded identity/geometry/color differs"
            );
        }
        for (id, brick) in &saved.world.bricks {
            ensure!(
                public_brick(brick) == host.replica.world.bricks[id],
                "Snapshot differs from replicated brick"
            );
        }
        drop(late);
        drop(host);
        let report = server.stop().await?;
        results.push(serde_json::json!({"source":name,"bricks":count,"build_bytes":bytes,"accepted_ms":accepted_ms,"replicated_ms":replicated_ms,"late_join_matches":true,"source_records_preserved":true,"ticks":report.ticks,"dropped_ticks":report.dropped_ticks}));
    }
    if let Some(parent) = args[3].parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &args[3],
        serde_json::to_vec_pretty(
            &serde_json::json!({"status":"passed","workloads":results,"scope":"Release Windows loopback bulk append, authored brick collision, late join and native export. No map geometry, renderer, WAN or interactive testing. Players spawn away from the build. Serialization/planning remain on the authority loop; dropped ticks must not be presented as smooth live loading."}),
        )?,
    )?;
    println!("Build load probe passed: {}", args[3].display());
    Ok(())
}
