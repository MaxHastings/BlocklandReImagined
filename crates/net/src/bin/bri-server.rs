//! Headless native-content host; no renderer or window dependencies.
use anyhow::{Context, Result, ensure};
use bri_net::{
    dedicated,
    server::{self, ServerOptions},
};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
/// A crash loses at most this much play; the newest autosaves are kept.
const AUTOSAVE_EVERY: Duration = Duration::from_secs(60);
const AUTOSAVE_KEEP: usize = 3;
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 4 || args.len() == 5,
        "Usage: bri-server <content-root> <world.json> <state-dir> <listen-address> [run-seconds]
         The content root's packages.json lists the packages to load (the base game's list when absent)."
    );
    let content_root = PathBuf::from(&args[0]);
    let world_path = PathBuf::from(&args[1]);
    let state_dir = PathBuf::from(&args[2]);
    // Session log and crash reports beside the server binary (or in its state).
    if let Err(error) = bri_crash::install("bri-server", &bri_crash::default_directories(&state_dir)) {
        eprintln!("Crash capture unavailable: {error}");
    }
    let bind = args[3]
        .to_str()
        .context("Invalid listen address")?
        .parse()?;
    let seconds = if args.len() == 5 {
        let seconds: u64 = args[4].to_str().context("Invalid duration")?.parse()?;
        ensure!((1..=86400).contains(&seconds), "Duration out of range");
        Some(seconds)
    } else {
        None
    };
    let world = bri_world::persistence::load_startup(&world_path)?;
    let dedicated::Dedicated {
        session,
        environment,
        spawn_points,
        tool_summary,
        unresolved_items,
        pending_objects,
        merge_notes,
    } = dedicated::load(&content_root, world)?;
    for note in &merge_notes {
        eprintln!("{note}");
    }
    let content_id = environment.digest();
    let initial_static_items = session.weapon_view().static_items.len();
    let mut server = server::start_with_admin_store_and_limit(
        session,
        ServerOptions {
            bind,
            environment: environment.clone(),
            spawn_points,
            certificate: Some(server::HostCertificate::load_or_create(&state_dir)?),
            map_loader: None,
            autosave: Some(server::Autosave {
                every: AUTOSAVE_EVERY,
                save: {
                    let dir = state_dir.clone();
                    std::sync::Arc::new(move |world| {
                        bri_world::persistence::autosave(&dir, world, AUTOSAVE_KEEP).map(drop)
                    })
                },
            }),
        },
        64,
        state_dir.join("administration.json"),
    )?;
    std::fs::create_dir_all(&state_dir)?;
    std::fs::write(state_dir.join("server-cert.der"), &server.certificate)?;
    std::fs::write(
        state_dir.join("host.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"schema_version":1,"address":server.address.to_string(),"content_id":content_id,"environment":environment,"tool_catalog":tool_summary,"initial_static_items":initial_static_items,"unresolved_item_references":unresolved_items,"certificate":"server-cert.der","pending_map_objects":pending_objects}),
        )?,
    )?;
    println!(
        "Headless host listening on {}. Public connection metadata: {}",
        server.address,
        state_dir.join("host.json").display()
    );
    println!(
        "Autosaving the world every {} s to {} (autosave-*.world.json; pass the newest as <world.json> to resume after a crash)",
        AUTOSAVE_EVERY.as_secs(),
        state_dir.display()
    );
    // A host listening beyond this computer opens its port on the router
    // (when there is one) and says whether players can reach it, with the
    // invite to share.
    let bind_ip: std::net::IpAddr = server.address.ip();
    if !bind_ip.is_loopback() && seconds.is_none() {
        let port = server.address.port();
        server.open_to_internet(move |report| {
            println!("Hosting check (port {port}):");
            for line in report.lines() {
                println!("  {line}");
            }
        });
    }
    if let Some(seconds) = seconds {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
    } else {
        tokio::signal::ctrl_c().await?;
    }
    let report = server.stop().await?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let save = state_dir.join(format!("world-{timestamp}.json"));
    bri_world::persistence::save_new(&save, &report.native_world)?;
    std::fs::write(
        state_dir.join("last-run.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"ticks":report.ticks,"dropped_ticks":report.dropped_ticks,"dropped_cues":report.dropped_cues,"weapon_adapter_gaps":report.weapon_adapter_gaps,"joins":report.joins,"resumes":report.resumes,"commands":report.commands,"rejected":report.rejected,"notices":report.notices,"saved_world":save.file_name().unwrap().to_string_lossy()}),
        )?,
    )?;
    println!(
        "Saved native world at tick {} to {}",
        report.ticks,
        save.display()
    );
    Ok(())
}
