//! Bounded native-save transfer benchmark with actual QUIC loopback clients.
use anyhow::{Context, Result, ensure};
use bri_net::{
    client::Client,
    codec,
    protocol::{Checkpoint, Message, ResumeToken},
    server::{self, ServerOptions},
};
use bri_sim::{
    definitions::Definitions,
    session::{Command, Session},
    simulation::Simulation,
};
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
        "Usage: network_probe <catalog-dir> <native-content> <world-dir> <report.json>"
    );
    let source: serde_json::Value =
        serde_json::from_slice(&std::fs::read(args[2].join("report.json"))?)?;
    let mut reports = Vec::new();
    for name in ["Bedroom/Demo House.bls", "Slate/Golden Gate Bridge.bls"] {
        let entry = source["saves"]
            .as_array()
            .context("Missing source saves")?
            .iter()
            .find(|s| s["source"].as_str() == Some(name))
            .context("Missing reference save")?;
        let world = bri_world::persistence::load(
            &args[2].join(entry["file"].as_str().context("Missing native save")?),
        )?;
        let session = Session::new(Simulation::new(
            world,
            Definitions::load(&args[0], &args[1])?,
            vec![],
        )?);
        let checkpoint = Checkpoint::from_session(&session, 0);
        let count = checkpoint.world.bricks.len();
        let expected = checkpoint.world;
        let checkpoint = Checkpoint::from_session(&session, 0);
        let message = Message::Welcome {
            administrator: false,
            owner: 1,
            resume: ResumeToken([0; 32]),
            checkpoint,
        };
        let raw_bytes = serde_json::to_vec(&message)?.len();
        let compressed_bytes = codec::encode(&message)?.len();
        let server = server::start(
            session,
            ServerOptions {
                bind: "127.0.0.1:0".parse()?,
                content_id: "native-probe-only".into(),
                spawn_points: vec![
                    Vec3::new(2000.0, 2000.0, 2000.0),
                    Vec3::new(2003.0, 2000.0, 2000.0),
                ],
                certificate: None,
                map_loader: None,
            },
        )?;
        let start = Instant::now();
        let mut first = Client::connect(
            server.address,
            &server.certificate,
            "First".into(),
            "native-probe-only".into(),
            None,
        )
        .await?;
        let first_ms = start.elapsed().as_secs_f64() * 1000.0;
        ensure!(
            first.replica.world == expected,
            "Initial native world changed in transit"
        );
        let start = Instant::now();
        let mut second = Client::connect(
            server.address,
            &server.certificate,
            "Late join".into(),
            "native-probe-only".into(),
            None,
        )
        .await?;
        let late_ms = start.elapsed().as_secs_f64() * 1000.0;
        ensure!(
            second.replica.world == expected,
            "Late-join native world changed in transit"
        );
        first
            .command(Command::Chat("Transport workload check".into()))
            .await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            while second.replica.chat.is_empty() {
                second.receive().await?;
            }
            Result::<()>::Ok(())
        })
        .await??;
        drop(first);
        drop(second);
        let result = server.stop().await?;
        ensure!(
            result.final_world == expected,
            "Authority changed native world"
        );
        reports.push(serde_json::json!({"source":name,"bricks":count,"checkpoint_json_bytes":raw_bytes,"checkpoint_compressed_bytes":compressed_bytes,"first_join_ms":first_ms,"late_join_ms":late_ms,"joins":result.joins,"server_ticks":result.ticks,"world_equality":"exact public-state comparison; source provenance stays on server"}));
    }
    if let Some(parent) = args[3].parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &args[3],
        serde_json::to_vec_pretty(
            &serde_json::json!({"status":"passed","workloads":reports,"scope":"Release loopback QUIC/TLS native-world transfer and post-join chat on this Windows host. No WAN/loss throughput or frame-rate claims. World collision is loaded, map geometry is omitted, players spawn outside the build solely for this transport benchmark."}),
        )?,
    )?;
    println!("Two native-world QUIC transfer workloads passed");
    Ok(())
}
