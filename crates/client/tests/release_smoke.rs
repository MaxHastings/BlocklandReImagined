//! Smoke test of a packaged release, offscreen: no window, no input.
//! BRI_RELEASE_DIR names a scratch copy of the release folder (the test adds
//! an imported add-on to it). Covers Start Game's mode picker (Slate and the
//! Stress Lab), the Add-Ons screen, Import Add-On with the shipped importer,
//! and a second client joining over loopback.
use anyhow::{Context, Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
    screens::ScreenId,
};
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const SIZE: (u32, u32) = (960, 720);
const SLATE: &str = "v20/add-ons/map_slate/slate.mis";
const LEGACY: &str = "Weapon_Shotgun";

fn release() -> Option<PathBuf> {
    std::env::var_os("BRI_RELEASE_DIR").map(PathBuf::from)
}

fn step(apps: &mut [&mut App], elapsed: Duration) -> Result<()> {
    for app in apps.iter_mut() {
        app.tick(elapsed)?;
        app.ui.update(elapsed.as_millis() as u64);
        ensure!(app.pump()?.is_empty(), "unexpected window command");
        if let ConnectionState::Failed { reason } = &app.ui.core.conn {
            bail!("connection failed: {reason}");
        }
    }
    Ok(())
}

fn until(apps: &mut [&mut App], what: &str, limit: Duration, ready: impl Fn(&[&mut App]) -> bool) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    loop {
        let now = Instant::now();
        step(apps, now - previous)?;
        previous = now;
        if ready(apps) {
            return Ok(());
        }
        ensure!(start.elapsed() < limit, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn in_game(app: &App) -> bool {
    app.network_view().is_some_and(|v| v.poses.contains_key(&v.owner))
}

fn load(root: &Path, name: &str) -> Result<App> {
    let state = std::env::temp_dir().join(format!("bri-release-smoke-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let mut app = App::load(&root.join("content"), &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.core.settings.avatar.lan_name = name.into();
    Ok(app)
}

fn host(app: &mut App, map: &str, game_mode: Option<String>, mode: ServerMode) -> Result<()> {
    app.ui.core.request(UiAction::HostGame {
        map: map.into(),
        mode,
        game_mode,
        max_players: 4,
        server_name: "Release smoke".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(&mut [app], "the host to enter its game", Duration::from_secs(120), |a| in_game(a[0]))
}

fn leave(app: &mut App) -> Result<()> {
    app.ui.core.request(UiAction::Disconnect);
    until(&mut [app], "leaving the game", Duration::from_secs(30), |a| a[0].network_view().is_none())
}

fn lit_pixels(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<usize> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("release smoke"),
        size: wgpu::Extent3d { width: SIZE.0, height: SIZE.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.render(&gpu.device, &gpu.queue, &mut encoder, &view, format, SIZE, app.ui.scale(),
        &app.ui.core.pack, &app.ui.draw(), Some(wgpu::Color::BLACK));
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (row * SIZE.1) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(SIZE.1) },
        },
        wgpu::Extent3d { width: SIZE.0, height: SIZE.1, depth_or_array_layers: 1 },
    );
    gpu.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None })?;
    let data = buffer.slice(..).get_mapped_range()?;
    Ok(data.chunks(4).filter(|p| p[0] > 16 || p[1] > 16 || p[2] > 16).count())
}


/// Host and discovery ports for this test process, away from the game's
/// 28000/28050 so a real game on this machine never collides with it.
fn use_test_ports() -> u16 {
    let port = std::net::UdpSocket::bind("127.0.0.1:0")
        .and_then(|s| s.local_addr())
        .map(|a| a.port())
        .expect("a free UDP port");
    // SAFETY: set before any host or join starts; this binary runs one test.
    unsafe {
        std::env::set_var("BRI_TEST_HOST_PORT", port.to_string());
        std::env::set_var("BRI_TEST_DISCOVERY_PORT", "0");
    }
    port
}

/// The standalone `BlocklandReImagined.exe` (BRI_STANDALONE_EXE): it unpacks
/// into a per-user folder (a scratch one here), reuses that install on the
/// next start, and the game it runs passes startup validation there.
#[test]
#[ignore = "needs BRI_STANDALONE_EXE: a packaged standalone exe"]
fn standalone_exe_unpacks_per_user_and_starts_the_game() -> Result<()> {
    let Some(exe) = std::env::var_os("BRI_STANDALONE_EXE").map(PathBuf::from) else {
        eprintln!("BRI_STANDALONE_EXE is not set; nothing to check");
        return Ok(());
    };
    let root = std::env::temp_dir().join(format!("bri-standalone-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let run = |args: &[&str]| -> Result<std::process::Output> {
        let out = std::process::Command::new(&exe)
            .args(args)
            .env("BRI_STANDALONE_ROOT", &root)
            .env("BRI_NO_DIALOGS", "1")
            .output()
            .with_context(|| format!("running {}", exe.display()))?;
        ensure!(out.status.success(), "{} {args:?} failed: {}", exe.display(), String::from_utf8_lossy(&out.stderr));
        Ok(out)
    };
    let started = Instant::now();
    let out = run(&["--extract-only"])?;
    let game = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    eprintln!("unpacked into {} in {:?}", game.display(), started.elapsed());
    ensure!(game == root.join("Game"), "the game unpacks into the per-user folder, not {}", game.display());
    for file in ["bri-client.exe", "bri-import-addon.exe", "MANIFEST.json", "content/packages.json"] {
        ensure!(game.join(file).is_file(), "the install has {file}");
    }
    // Something the player adds survives the next start, which reuses the install.
    let drop = game.join("content").join(bri_package::library::DROP_DIR);
    std::fs::create_dir_all(&drop)?;
    std::fs::write(drop.join("Keep_Me.zip"), b"PK")?;
    let started = Instant::now();
    run(&["--extract-only"])?;
    ensure!(started.elapsed() < Duration::from_secs(5), "the second start reuses the install");
    ensure!(drop.join("Keep_Me.zip").is_file(), "the player's Add-On is still there");
    let out = run(&["--check"])?;
    let said = String::from_utf8_lossy(&out.stdout);
    ensure!(said.contains("Startup validation passed"), "the game starts from the install: {said}");
    ensure!(!exe.parent().is_some_and(|d| d.join("content").exists()), "nothing is written beside the exe");
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
#[ignore = "needs BRI_RELEASE_DIR: a scratch copy of a packaged release; offscreen GPU, loopback UDP"]
fn release_hosts_modes_lists_and_imports_add_ons_and_accepts_a_loopback_join() -> Result<()> {
    let port = use_test_ports();
    let Some(root) = release() else {
        eprintln!("BRI_RELEASE_DIR is not set; nothing to check");
        return Ok(());
    };
    let gpu = Headless::new().context("offscreen adapter")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);

    // Import Add-On: drop an old add-on in the Add-Ons folder and convert it
    // with the importer shipped next to the game, exactly as the button does.
    let archive = std::env::var("BRI_ADDON_ARCHIVE")
        .unwrap_or("C:/Users/Maxwell/Documents/_Blockland_Maxwell_1588_Archive/Addons".into());
    let zip = Path::new(&archive).join(format!("{LEGACY}.zip"));
    ensure!(zip.is_file(), "missing {}", zip.display());
    let content = root.join("content");
    let drop = content.join(bri_package::library::DROP_DIR);
    std::fs::create_dir_all(&drop)?;
    std::fs::copy(&zip, drop.join(format!("{LEGACY}.zip")))?;
    let library = bri_package::library::Library::scan(&content)?;
    ensure!(library.legacy.iter().any(|l| l.name == LEGACY), "the Add-Ons folder lists {LEGACY}");
    let out = content.join(library.import_dir(LEGACY));
    let importer = root.join("bri-import-addon.exe");
    let run = std::process::Command::new(&importer)
        .arg(drop.join(format!("{LEGACY}.zip")))
        .arg(&out)
        .arg("--json")
        .output()
        .with_context(|| format!("running {}", importer.display()))?;
    ensure!(run.status.success(), "import failed: {}", String::from_utf8_lossy(&run.stderr));
    eprintln!("imported {LEGACY} into {}", out.display());

    // The Add-Ons screen lists the base game, the Stress Lab and the import.
    let mut app = load(&root, "Hosty")?;
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    app.ui.core.push(ScreenId::AddOns);
    app.ui.core.request(UiAction::RequestAddOns);
    step(&mut [&mut app], Duration::from_millis(16))?;
    let rows = app.ui.core.add_ons.rows.clone();
    eprintln!("Add-Ons rows: {}", rows.iter().map(|r| format!("{} ({}{})", r.name, r.category, if r.enabled { ", on" } else { "" })).collect::<Vec<_>>().join("; "));
    ensure!(rows.iter().any(|r| r.locked), "base game rows");
    ensure!(rows.iter().any(|r| r.id.starts_with("stresslab")), "Stress Lab rows");
    ensure!(rows.iter().any(|r| r.name.to_ascii_lowercase().contains("shotgun")), "the imported shotgun row");
    let lit = lit_pixels(&mut app, &gpu, &mut renderer)?;
    ensure!(lit > 10_000, "the Add-Ons screen draws ({lit} lit pixels)");
    app.ui.core.pop(ScreenId::AddOns);

    // Start Game's mode picker: Custom on Slate, then the Stress Lab mode.
    let modes = app.ui.core.game_modes.clone();
    eprintln!("game modes: {:?}", modes.iter().map(|m| (&m.id, &m.map)).collect::<Vec<_>>());
    host(&mut app, SLATE, None, ServerMode::SinglePlayer)?;
    eprintln!("hosted Slate: {} bricks", app.network_view().map_or(0, |v| v.world.bricks.len()));
    leave(&mut app)?;
    let stress = modes.iter().find(|m| m.id.starts_with("stresslab")).context("Stress Lab game mode")?;
    let map = stress.map.clone().unwrap_or("stresslab-world:world/strata".into());
    host(&mut app, &map, Some(stress.id.clone()), ServerMode::SinglePlayer)?;
    until(&mut [&mut app], "the Stress Lab world and miner panel", Duration::from_secs(60), |a| {
        a[0].network_view().is_some_and(|v| v.world.bricks.len() > 5_000)
            && a[0].ui.core.package_panels.iter().any(|p| p.title.contains("STRESS LAB"))
    })?;
    eprintln!("hosted Stress Lab: {} bricks", app.network_view().map_or(0, |v| v.world.bricks.len()));
    leave(&mut app)?;

    // A second client joins a LAN-visible host over loopback. Another
    // session may hold the game ports: BRI_SMOKE_NO_JOIN skips this part.
    if std::env::var_os("BRI_SMOKE_NO_JOIN").is_some() {
        eprintln!("loopback join skipped (BRI_SMOKE_NO_JOIN)");
        return Ok(());
    }
    host(&mut app, SLATE, None, ServerMode::Internet)?;
    let mut guest = load(&root, "Guesty")?;
    guest.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    guest.ui.core.request(UiAction::JoinServer { address: format!("127.0.0.1:{port}"), password: String::new() });
    until(&mut [&mut app, &mut guest], "the guest to join and both to list two players", Duration::from_secs(120), |a| {
        in_game(a[1]) && a.iter().all(|x| x.ui.core.players.len() == 2)
    })?;
    eprintln!("loopback join: host lists {:?}", app.ui.core.players.iter().map(|p| &p.name).collect::<Vec<_>>());
    leave(&mut guest)?;
    leave(&mut app)?;
    Ok(())
}
