//! Stress Lab soak: a loopback host and several headless clients walk out
//! across the generated world in different directions, mining as they go.
//! Prints a JSON report of what the host and clients did.
//!
//! stresslab-soak [--clients N] [--seconds S] [--out report.json]
use anyhow::{Context, Result};
use bri_identity::ClientIdentity;
use bri_net::{
    client::Client,
    server::{self, ServerOptions},
};
use bri_sim::{player::MoveInput, session::ActionAim};
use bri_stresslab::net::{own, package};
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Serialize)]
struct ClientReport {
    name: String,
    mined: i64,
    bits: i64,
    commands: u64,
    rejected: u64,
    distance_walked: f32,
    bricks_in_replica: usize,
    entities_in_replica: usize,
}
#[derive(Serialize)]
struct Report {
    clients: usize,
    seconds: f32,
    host_ticks: u64,
    host_dropped_ticks: u64,
    host_commands: u64,
    host_rejected: u64,
    chunks_generated: usize,
    voxels_live: usize,
    voxels_removed: usize,
    world_bricks_at_end: usize,
    world_extent_chunks: [i64; 4],
    replicas_agree: bool,
    package_diagnostics: Vec<String>,
    per_client: Vec<ClientReport>,
    wall_seconds: f32,
}

fn arg(args: &[String], name: &str, default: &str) -> String {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| default.into())
}

async fn drive(mut client: Client, yaw: f32, seconds: f32) -> Result<(Client, u64, u64, f32)> {
    let start = Instant::now();
    let mut sequence = client
        .replica
        .poses
        .get(&client.owner)
        .map_or(0, |p| p.acknowledged_input)
        + 1;
    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / 120.0));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let (mut commands, mut rejected) = (0_u64, 0_u64);
    let mut history: Vec<MoveInput> = Vec::new();
    let mut tick = 0_u64;
    let mut pending: Option<u64> = None;
    let mut last = client
        .replica
        .poses
        .get(&client.owner)
        .map(|p| p.player.feet);
    let mut walked = 0.0;
    while start.elapsed().as_secs_f32() < seconds {
        tokio::select! {
            _ = ticker.tick() => {
                tick += 1;
                let stuck = client.replica.poses.get(&client.owner).is_some_and(|p| {
                    let v = p.player.velocity;
                    v[0] * v[0] + v[2] * v[2] < 1.0
                });
                history.push(MoveInput { forward: 1.0, yaw, pitch: -0.5, jump: stuck && tick % 30 < 3, ..Default::default() });
                if history.len() > bri_net::protocol::MOVEMENT_REDUNDANCY {
                    history.remove(0);
                }
                client.movement(sequence, &history, None)?;
                sequence += 1;
                if pending.is_none() && tick.is_multiple_of(24) {
                    let aim = Some(ActionAim { yaw, pitch: -0.9 });
                    pending = Some(client.request_with_aim(package("stresslab-economy", "mine", vec![]), aim).await?);
                    commands += 1;
                }
                if tick.is_multiple_of(600) {
                    client.request(package("stresslab-economy", "sell_all", vec![])).await?;
                    commands += 1;
                }
            }
            event = client.receive() => {
                if let bri_net::client::ClientEvent::Reply { sequence, result } = event? {
                    if Some(sequence) == pending {
                        pending = None;
                    }
                    if result.is_err() {
                        rejected += 1;
                    }
                }
                if let Some(p) = client.replica.poses.get(&client.owner) {
                    if let Some(l) = last {
                        walked += glam::Vec3::from(p.player.feet).distance(glam::Vec3::from(l));
                    }
                    last = Some(p.player.feet);
                }
            }
        }
    }
    Ok((client, commands, rejected, walked))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let clients: usize = arg(&args, "--clients", "6").parse().context("--clients")?;
    let seconds: f32 = arg(&args, "--seconds", "60").parse().context("--seconds")?;
    let out = arg(&args, "--out", "");
    let wall = Instant::now();
    let dir = std::env::temp_dir().join(format!("bri-stresslab-soak-{}", std::process::id()));
    let (session, spawns) = bri_stresslab::fixture_session(None)?;
    let server = server::start(
        session,
        ServerOptions {
            bind: "127.0.0.1:0".parse()?,
            environment: bri_stresslab::environment(&bri_stresslab::packages_root()).unwrap(),
            spawn_points: spawns,
            certificate: None,
            map_loader: None,
            packages: None,
        },
    )?;
    let mut tasks = Vec::new();
    for i in 0..clients {
        let identity = ClientIdentity::load_or_create(dir.join(format!("soak{i}.key")))?;
        let client = Client::connect_with_identity(
            server.address,
            &server.certificate,
            format!("Soak{i}"),
            bri_stresslab::environment(&bri_stresslab::packages_root())?.client_packages(),
            None,
            None,
            &identity,
        )
        .await?;
        let yaw = (i as f32 / clients as f32) * std::f32::consts::TAU - std::f32::consts::PI;
        tasks.push(tokio::spawn(drive(client, yaw, seconds)));
    }
    let mut finished = Vec::new();
    for task in tasks {
        finished.push(task.await??);
    }
    // Let the last updates arrive, then compare every replica's world.
    let settle = Instant::now();
    let mut agree = false;
    while settle.elapsed() < Duration::from_secs(5) {
        for (client, ..) in &mut finished {
            let _ = tokio::time::timeout(Duration::from_millis(50), client.receive()).await;
        }
        let first = &finished[0].0.replica.world.bricks;
        if finished
            .iter()
            .all(|(c, ..)| &c.replica.world.bricks == first)
        {
            agree = true;
            break;
        }
    }
    let per_client: Vec<ClientReport> = finished
        .iter()
        .map(|(c, commands, rejected, walked)| ClientReport {
            name: c.replica.names.get(&c.owner).cloned().unwrap_or_default(),
            mined: own(c, "stresslab-economy", "mined").unwrap_or(0),
            bits: own(c, "stresslab-economy", "bits").unwrap_or(0),
            commands: *commands,
            rejected: *rejected,
            distance_walked: *walked,
            bricks_in_replica: c.replica.world.bricks.len(),
            entities_in_replica: c.replica.entities.len(),
        })
        .collect();
    for (client, ..) in &finished {
        client.close();
    }
    drop(finished);
    let report = server.stop().await?;
    let save = report.packages.clone().context("package save")?;
    let removed = save.world.as_ref().map_or(0, |w| w.removed.len());
    let mut extent = [0_i64; 4];
    for v in report.native_world.bricks.values() {
        let (cx, cz) = (
            (v.position[0] / 16.0).floor() as i64,
            (v.position[2] / 16.0).floor() as i64,
        );
        extent = [
            extent[0].min(cx),
            extent[1].max(cx),
            extent[2].min(cz),
            extent[3].max(cz),
        ];
    }
    let chunks = ((extent[1] - extent[0] + 1) * (extent[3] - extent[2] + 1)) as usize;
    let result = Report {
        clients,
        seconds,
        host_ticks: report.ticks,
        host_dropped_ticks: report.dropped_ticks,
        host_commands: report.commands,
        host_rejected: report.rejected,
        chunks_generated: chunks,
        voxels_live: report.package_stats.voxels,
        voxels_removed: removed,
        world_bricks_at_end: report.native_world.bricks.len(),
        world_extent_chunks: extent,
        replicas_agree: agree,
        package_diagnostics: report
            .package_diagnostics
            .iter()
            .map(ToString::to_string)
            .collect(),
        per_client,
        wall_seconds: wall.elapsed().as_secs_f32(),
    };
    let json = serde_json::to_string_pretty(&result)?;
    println!("{json}");
    if !out.is_empty() {
        std::fs::write(&out, &json)?;
    }
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
