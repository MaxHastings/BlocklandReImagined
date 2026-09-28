//! Two headless clients on one Internet host over loopback: name tags,
//! minigame listing, trust invitations and admin Change Map. Never creates a
//! window or OS input; the host binds UDP 28000/28050.
//! Run: cargo test -p bri-client --test multiplayer --release -- --ignored --nocapture
use anyhow::{Context, Result, bail, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
    models::admin::AdminAction,
    screens::ScreenId,
};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SIZE: (u32, u32) = (960, 720);
const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";
const SLATE: &str = "v20/add-ons/map_slate/slate.mis";

fn step(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        bail!("Connection failed: {reason}");
    }
    Ok(())
}

/// Step both apps until `ready` holds for them.
fn until(
    apps: &mut [&mut App],
    what: &str,
    timeout: Duration,
    ready: impl Fn(&[&mut App]) -> bool,
) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    loop {
        let now = Instant::now();
        for app in apps.iter_mut() {
            step(app, now.duration_since(previous))?;
        }
        previous = now;
        if ready(apps) {
            return Ok(());
        }
        ensure!(start.elapsed() < timeout, "Timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn request(app: &mut App, action: UiAction) -> Result<()> {
    app.ui.core.request(action);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    app.ui.update(0);
    Ok(())
}

fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}

fn capture(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("multiplayer capture"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    ensure!(
        app.render_scene(&mut RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder: &mut encoder,
            target: &view,
            format,
            size: SIZE,
            ui_renderer: renderer,
        })?,
        "No scene rendered"
    );
    renderer.render(
        &gpu.device,
        &gpu.queue,
        &mut encoder,
        &view,
        format,
        SIZE,
        app.ui.scale(),
        &app.ui.core.pack,
        &app.ui.draw(),
        None,
    );
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("multiplayer readback"),
        size: u64::from(row) * u64::from(SIZE.1),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIZE.1),
            },
        },
        extent,
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    rx.recv_timeout(Duration::from_secs(5))??;
    let mapped = readback
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((SIZE.0 * SIZE.1 * 4) as usize);
    for bytes in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&bytes[..SIZE.0 as usize * 4]);
    }
    drop(mapped);
    readback.unmap();
    Ok(pixels)
}

fn save(path: &Path, pixels: &[u8]) -> Result<()> {
    image::save_buffer(path, pixels, SIZE.0, SIZE.1, image::ColorType::Rgba8)?;
    Ok(())
}

fn player_row<'a>(app: &'a App, name: &str) -> Option<&'a PlayerRow> {
    app.ui.core.players.iter().find(|p| p.name == name)
}

#[test]
#[ignore = "converted native content, loopback UDP 28000/28050 and an offscreen GPU; no window"]
fn two_clients_see_names_minigames_trust_and_follow_a_map_change() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifact = workspace.join("artifacts/native-multiplayer");
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let load = |name: &str| -> Result<App> {
        let state = artifact.join(format!("state-{name}-{stamp}"));
        std::fs::create_dir_all(&state)?;
        let mut settings = App::load(&workspace.join("content"), &state, SIZE)?;
        settings.ui.core.pop(ScreenId::DefaultControls);
        settings.ui.core.settings.avatar.lan_name = name.into();
        Ok(settings)
    };
    let mut host = load("Hosty")?;
    let mut guest = load("Guesty")?;
    let gpu = Headless::new().context("offscreen adapter")?;
    let mut ui_renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    host.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    guest.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;

    host.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::Internet, game_mode: None,
        max_players: 8,
        server_name: "Multiplayer probe".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(&mut [&mut host], "host in game", Duration::from_secs(90), |a| {
        in_game(a[0])
    })?;
    request(
        &mut guest,
        UiAction::JoinServer {
            address: "127.0.0.1".into(),
            password: String::new(),
        },
    )?;
    until(
        &mut [&mut host, &mut guest],
        "guest in game and both listed",
        Duration::from_secs(90),
        |a| in_game(a[1]) && a.iter().all(|app| app.ui.core.players.len() == 2),
    )?;
    let host_id = host.network_view().unwrap().owner;
    let guest_id = guest.network_view().unwrap().owner;
    until(
        &mut [&mut host, &mut guest],
        "join announcement",
        Duration::from_secs(5),
        |a| a[0].ui.core.chat.lines.iter().any(|l| l.text.contains("Guesty connected.")),
    )
    .with_context(|| {
        format!(
            "host chat {:?}, players {:?}",
            host.ui.core.chat.lines.iter().map(|l| &l.text).collect::<Vec<_>>(),
            host.ui.core.players.iter().map(|p| &p.name).collect::<Vec<_>>()
        )
    })?;

    // Name tags: face the guest toward the host and render with the HUD.
    let (from, to) = {
        let view = guest.network_view().unwrap();
        (view.poses[&guest_id].player.feet, view.poses[&host_id].player.feet)
    };
    let yaw = (to[0] - from[0]).atan2(-(to[2] - from[2]));
    request(&mut guest, UiAction::Game(GameAction::Look { yaw, pitch: 0.0 }))?;
    until(
        &mut [&mut host, &mut guest],
        "guest scene ready",
        Duration::from_secs(30),
        |a| a[1].world_render_ready(),
    )?;
    let _ = capture(&mut guest, &gpu, &mut ui_renderer)?;
    let tags = guest.ui.core.name_tags.clone();
    let frame = capture(&mut guest, &gpu, &mut ui_renderer)?;
    save(&artifact.join("guest-name-tag.png"), &frame)?;
    ensure!(
        tags.iter().any(|t| t.text == "Hosty"),
        "Guest sees no name tag for Hosty: {tags:?}"
    );
    ensure!(!tags.iter().any(|t| t.text == "Guesty"), "Own name shown");

    // Minigame listing reaches the other client's Join Mini-Game list.
    request(
        &mut host,
        UiAction::CreateMiniGame {
            color: 2,
            rules: MiniGameRules {
                title: "Probe Deathmatch".into(),
                respawn_seconds: 5,
                brick_respawn_seconds: 30,
                ..Default::default()
            },
        },
    )?;
    until(
        &mut [&mut host, &mut guest],
        "minigame in guest's list",
        Duration::from_secs(10),
        |a| {
            a[1].ui
                .core
                .minigames
                .games
                .iter()
                .any(|g| g.title == "Probe Deathmatch" && g.owner_name == "Hosty")
        },
    )?;
    guest.ui.core.push(ScreenId::MiniGames);
    guest.ui.update(0);
    let frame = capture(&mut guest, &gpu, &mut ui_renderer)?;
    save(&artifact.join("guest-join-minigame.png"), &frame)?;
    guest.ui.core.pop(ScreenId::MiniGames);

    // Trust: the host invites the guest to full trust; the guest accepts.
    until(
        &mut [&mut host, &mut guest],
        "trust rows",
        Duration::from_secs(10),
        |a| player_row(a[0], "Guesty").is_some_and(|p| p.trust == "-" && p.bl_id.is_some()),
    )?;
    request(&mut host, UiAction::TrustInvite { target: guest_id, level: 2 })?;
    until(
        &mut [&mut host, &mut guest],
        "trust invitation",
        Duration::from_secs(10),
        |a| !a[1].ui.core.trust_invites.is_empty(),
    )?;
    let frame = capture(&mut guest, &gpu, &mut ui_renderer)?;
    save(&artifact.join("guest-trust-invite.png"), &frame)?;
    request(
        &mut guest,
        UiAction::AnswerTrustInvite {
            from: host_id,
            answer: TrustAnswer::Accept,
        },
    )?;
    guest.ui.core.pop(ScreenId::TrustInvitation);
    until(
        &mut [&mut host, &mut guest],
        "mutual full trust",
        Duration::from_secs(10),
        |a| {
            player_row(a[0], "Guesty").is_some_and(|p| p.trust == "Full")
                && player_row(a[1], "Hosty").is_some_and(|p| p.trust == "Full")
        },
    )?;
    host.ui.core.pop(ScreenId::MessageBox);
    guest.ui.core.pop(ScreenId::MessageBox);
    guest.ui.core.push(ScreenId::PlayerList);
    guest.ui.update(0);
    let frame = capture(&mut guest, &gpu, &mut ui_renderer)?;
    save(&artifact.join("guest-player-list.png"), &frame)?;
    guest.ui.core.pop(ScreenId::PlayerList);

    // Change Map: the host administrator moves everyone to Slate.
    host.ui.core.push(ScreenId::AdminMaps);
    host.ui.update(0);
    until(
        &mut [&mut host, &mut guest],
        "map list",
        Duration::from_secs(10),
        |a| a[0].ui.core.admin.maps.iter().any(|m| m.id == SLATE),
    )?;
    host.ui.core.pop(ScreenId::AdminMaps);
    request(&mut host, UiAction::Admin(AdminAction::ChangeMap { map: SLATE.into() }))?;
    until(
        &mut [&mut host, &mut guest],
        "guest shows the loading screen",
        Duration::from_secs(20),
        |a| matches!(a[1].ui.core.conn, ConnectionState::Loading { .. }),
    )?;
    let frame = capture(&mut guest, &gpu, &mut ui_renderer)?;
    save(&artifact.join("guest-loading-new-map.png"), &frame)?;
    until(
        &mut [&mut host, &mut guest],
        "both clients on Slate",
        Duration::from_secs(120),
        |a| {
            a.iter().all(|app| {
                app.network_view().is_some_and(|v| v.world.map_id == SLATE)
                    && app.scene_map() == Some(SLATE)
                    && in_game(app)
                    && app.world_render_ready()
                    && app.ui.core.players.len() == 2
            })
        },
    )?;
    until(
        &mut [&mut host, &mut guest],
        "trust survives the map change",
        Duration::from_secs(10),
        |a| player_row(a[1], "Hosty").is_some_and(|p| p.trust == "Full"),
    )?;
    until(
        &mut [&mut host, &mut guest],
        "guest standing on Slate",
        Duration::from_secs(20),
        |a| {
            let view = a[1].network_view().unwrap();
            view.poses[&view.owner].player.grounded
        },
    )
    .with_context(|| {
        let view = guest.network_view().unwrap();
        format!("guest pose {:?}", view.poses[&view.owner].player)
    })?;
    let frame = capture(&mut guest, &gpu, &mut ui_renderer)?;
    save(&artifact.join("guest-after-change-map.png"), &frame)?;
    println!(
        "guest on slate at {:?}, presented {:?}",
        guest.network_view().unwrap().poses[&guest_id].player.feet,
        guest.presented_local().map(|p| p.feet)
    );
    let moved = guest
        .ui
        .core
        .chat
        .lines
        .iter()
        .any(|l| l.text.contains("changed the map to"));
    ensure!(moved, "Guest chat lacks the change-map line");
    // A fresh join on the new map renders the same world.
    request(&mut guest, UiAction::Disconnect)?;
    request(
        &mut guest,
        UiAction::JoinServer {
            address: "127.0.0.1".into(),
            password: String::new(),
        },
    )?;
    until(
        &mut [&mut host, &mut guest],
        "guest rejoined on Slate",
        Duration::from_secs(90),
        |a| in_game(a[1]) && a[1].world_render_ready(),
    )?;
    until(
        &mut [&mut host, &mut guest],
        "guest standing after rejoin",
        Duration::from_secs(20),
        |a| {
            let view = a[1].network_view().unwrap();
            view.poses[&view.owner].player.grounded
        },
    )?;
    let frame = capture(&mut guest, &gpu, &mut ui_renderer)?;
    save(&artifact.join("guest-rejoined-slate.png"), &frame)?;
    println!("rejoined presented {:?}", guest.presented_local().map(|p| p.feet));
    println!("artifacts: {}", artifact.display());
    Ok(())
}
