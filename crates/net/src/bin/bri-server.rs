//! Headless native-content host; no renderer or window dependencies.
use anyhow::{Context, Result, ensure};
use bri_net::{
    dedicated,
    server::{self, ServerOptions},
};
use std::{path::PathBuf, time::Duration};
/// mimalloc: the persistent world maps and replication allocate heavily.
/// On a 200k-brick world it cut world build 17%, wire decode 20%, JSON
/// load 16% and collider inserts 18% against the system allocator (Linux;
/// Windows' heap usually gains more).
/// `bri_net::allocator::tune` keeps its purges off the tick.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
/// Asks for the match to be recorded.
const RECORD_FLAG: &str = "--record";
#[tokio::main]
async fn main() -> Result<()> {
    bri_net::allocator::tune();
    // Watched from the start: a stop asked for while loading still saves.
    let mut shutdown = dedicated::Shutdown::watch()?;
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    let given = args.len();
    args.retain(|a| a != RECORD_FLAG);
    let record = args.len() < given;
    ensure!(
        args.len() == 4 || args.len() == 5,
        "Usage: bri-server [--record] <content-root> <map | world.json | resume> <state-dir> <listen-address> [run-seconds]
         <map> starts an empty world on a base map by name (slate, bedroom, kitchen, slopes, ...).
         `resume` continues from the newest world this server saved in <state-dir> when it last stopped
         (or, after a crash, the world it kept for recovery).
         <state-dir>/server.json holds the admin passwords and server settings (written with defaults on first start).
         <listen-address> is usually 0.0.0.0:28000 (UDP).
         --record writes the match to <state-dir>/recordings (the newest ten are kept) for bri-replay to check.
         The content root's packages.json lists the packages to load (the base game's list and the default Add-Ons when absent)."
    );
    let content_root = PathBuf::from(&args[0]);
    // A release's content has its default Add-Ons; a source checkout's is
    // left as it is unless BRI_INSTALL_DEFAULT_ADD_ONS=1 asks.
    if let Some(done) = bri_package::defaults::install_when_asked(&content_root)?
        && !done.is_empty()
    {
        println!("Installed the default Add-Ons {}.", done.ids().join(", "));
    }
    let state_dir = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&state_dir)?;
    // The last run did not stop cleanly: what it kept is that run's world.
    if let Some(world) = dedicated::adopt_recovery(&state_dir)? {
        println!(
            "The server did not stop cleanly last time; the world it kept is {} (`resume` continues from it).",
            world.display()
        );
    }
    let start = args[1].to_string_lossy();
    let world_path = if start == "resume" {
        let newest = bri_world::persistence::newest_world(&state_dir)
            .with_context(|| format!("Reading {}", state_dir.display()))?
            .with_context(|| format!("No saved world to resume in {}", state_dir.display()))?;
        println!("Resuming {}", newest.display());
        Some(newest)
    } else if start.ends_with(".json") {
        Some(PathBuf::from(&args[1]))
    } else {
        None
    };
    // Session log and crash reports beside the server binary (or in its state).
    if let Err(error) =
        bri_crash::install("bri-server", &bri_crash::default_directories(&state_dir))
    {
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
    let (world, minigame) = match &world_path {
        Some(path) => {
            let build = bri_world::persistence::load_startup(path)?;
            (build.world, build.minigame)
        }
        None => (dedicated::blank_world(&content_root, &start)?, None),
    };
    let config = dedicated::ServerConfig::load_or_create(&state_dir)?;
    let map_name = world
        .map_id
        .rsplit('/')
        .next()
        .and_then(|file| file.strip_suffix(".mis"))
        .unwrap_or(&world.map_id)
        .to_string();
    let mut host = dedicated::load(&content_root, world)?;
    host.configure(&config)?;
    // A recording that cannot start leaves the match unrecorded.
    let recording = if record {
        let dir = state_dir.join(bri_net::replay::RECORDINGS_DIR);
        match bri_net::replay::next_recording(&dir) {
            Ok(path) => Some(bri_net::replay::Recording {
                path,
                host: host.recipe()?,
            }),
            Err(error) => {
                eprintln!("Not recording this match ({}): {error:#}", dir.display());
                None
            }
        }
    } else {
        None
    };
    println!(
        "{}: up to {} players; settings and admin passwords in {}",
        config.settings.name,
        config.settings.max_players,
        state_dir.join(dedicated::ServerConfig::FILE).display()
    );
    let dedicated::Dedicated {
        session,
        setup,
        environment,
        spawn_points,
        tool_summary,
        unresolved_items,
        pending_objects,
        merge_notes,
        ..
    } = host;
    for note in &merge_notes {
        eprintln!("{note}");
    }
    let mut session = session;
    // The mini-game it was saved with comes back for whoever ran it.
    if let Some(minigame) = minigame {
        session.hold_minigame(minigame);
    }
    let content_id = environment.digest();
    let initial_static_items = session.weapon_view().static_items.len();
    let mut server = server::start_with_admin_store_limit_and_recording(
        session,
        ServerOptions {
            bind,
            environment: environment.clone(),
            spawn_points,
            certificate: Some(server::HostCertificate::load_or_create(&state_dir)?),
            // Cannot change maps yet; keeps the Add-Ons' state when stopping.
            map_loader: Some(setup),
            // Joiners download the Add-Ons this host runs.
            packages: Some(std::sync::Arc::new(bri_net::packages::PackageShelf::new(
                &content_root,
                &bri_package::packages::PackageSet::load_root(&content_root)?,
                &environment,
            )?)),
        },
        usize::from(config.settings.max_players),
        state_dir.join("administration.json"),
        recording,
    )?;
    // Connect to IP and the LAN list show the server's name and map.
    if seconds.is_none()
        && let Err(error) = server
            .advertise(
                config.settings.name.clone(),
                map_name,
                u32::from(config.settings.max_players),
                content_id.clone(),
            )
            .await
    {
        eprintln!("Not listed on the LAN: {error:#}");
    }
    server.keep_recovery(server::Recovery::new(dedicated::recovery_path(&state_dir)))?;
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
        // A host that stopped by itself (it kept failing) is saved too.
        let ended = async {
            while !server.is_finished() {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        };
        tokio::select! {
            why = shutdown.requested() => println!("Stopping ({why}); saving the world."),
            () = ended => println!("The server stopped by itself; saving the world."),
        }
    }
    // The final world is saved however the host ended, before any error.
    let report = server.finish().await?;
    let save = dedicated::save_stopped(&state_dir, &report);
    dedicated::shutdown_saved();
    let save = save?;
    // The saved world supersedes the recovery snapshot a failure kept.
    let _ = std::fs::remove_file(dedicated::recovery_path(&state_dir));
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
    if let Some(failure) = report.failure {
        anyhow::bail!("The server stopped by itself: {failure}");
    }
    Ok(())
}
