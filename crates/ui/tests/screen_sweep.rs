//! Every menu and dialog through the real `Ui` at a short wide window, 720p
//! and 1440p (automatic UI scale, as the game picks it), rendered offscreen
//! and checked for controls cut off by the window, children spilling out of
//! their parents and text wider than its control.
//!
//! The synthetic variant runs on made-up assets (`bri_ui::testing`). The
//! content variant runs on the converted pack and saves screenshots:
//! cargo test -p bri-ui --test screen_sweep -- --ignored --nocapture
//! PNGs go to `BRI_SWEEP_OUT` (default `target/screen-sweep`); they contain
//! original artwork, so keep them out of git.
use bri_ui::{
    api::Settings,
    binds::Platform,
    draw::DrawList,
    geom::Rect,
    gpu::{Headless, UiRenderer},
    pack::Pack,
    screens::ScreenId,
    text::Font,
    ui::{Ui, UiConfig},
    view::View,
};
use std::{path::PathBuf, rc::Rc};

const SIZES: [(u32, u32); 3] = [(1999, 800), (1280, 720), (2560, 1440)];

fn screens() -> Vec<(ScreenId, &'static str)> {
    use bri_ui::api::{ChatChannel, WrenchVariant};
    vec![
        (ScreenId::MainMenu, "main-menu"),
        (ScreenId::DefaultControls, "default-controls"),
        (ScreenId::StartMission, "start-game"),
        (ScreenId::GameModes, "game-modes"),
        (ScreenId::JoinServer, "join-server"),
        (ScreenId::ManualJoin, "connect-to-ip"),
        (ScreenId::Connecting, "connecting"),
        (ScreenId::Loading, "loading"),
        (ScreenId::Play, "play"),
        (ScreenId::MessageInput(ChatChannel::Say), "chat-input"),
        (ScreenId::EscapeMenu, "escape-menu"),
        (ScreenId::Options, "options"),
        (ScreenId::Remap, "remap"),
        (ScreenId::PlayerList, "player-list"),
        (ScreenId::MiniGames, "minigames"),
        (ScreenId::MiniGameSettings, "minigame-settings"),
        (ScreenId::MiniGameInvitation, "minigame-invitation"),
        (ScreenId::TrustInvitation, "trust-invitation"),
        (ScreenId::Admin, "admin"),
        (ScreenId::AdminLogin, "admin-login"),
        (ScreenId::AdminBan, "admin-ban"),
        (ScreenId::AdminUnban, "admin-unban"),
        (ScreenId::AdminBricks, "admin-bricks"),
        (ScreenId::AdminMaps, "admin-maps"),
        (ScreenId::AdminOptions, "admin-options"),
        (ScreenId::AdminCredentials, "admin-credentials"),
        (ScreenId::AdminConfirm, "admin-confirm"),
        (ScreenId::BrickSelector, "brick-selector"),
        (ScreenId::PrintSelector, "print-selector"),
        (ScreenId::Wrench(WrenchVariant::Normal), "wrench"),
        (ScreenId::WrenchEvents, "wrench-events"),
        (ScreenId::Avatar, "avatar"),
        (ScreenId::SaveBricks, "save-bricks"),
        (ScreenId::LoadBricks, "load-bricks"),
        (ScreenId::About, "about"),
        (ScreenId::Console, "console"),
        (ScreenId::AddOns, "add-ons"),
        (ScreenId::PackageDownload, "add-on-download"),
        (ScreenId::AddOnMismatch, "add-on-mismatch"),
        (ScreenId::ServerConfig, "server-config"),
        (ScreenId::Help, "help"),
    ]
}

fn inside(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.w <= outer.x + outer.w
        && inner.y + inner.h <= outer.y + outer.h
}

fn shown(v: &View, mut n: usize) -> bool {
    loop {
        let node = v.node(n);
        if !node.state.visible || !node.ctrl.visible {
            return false;
        }
        match node.parent {
            Some(p) => n = p,
            None => return true,
        }
    }
}

/// Inside a scroll or list the content is meant to run past its frame.
fn scrolled(v: &View, n: usize) -> bool {
    let mut at = v.node(n).parent;
    while let Some(p) = at {
        let class = v.node(p).ctrl.class.to_ascii_lowercase();
        if class.contains("scroll") || class.contains("list") {
            return true;
        }
        at = v.node(p).parent;
    }
    false
}

fn problems(pack: &Pack, v: &View, screen: Rect) -> Vec<String> {
    let mut out = Vec::new();
    for n in v.walk() {
        if !shown(v, n) {
            continue;
        }
        let node = v.node(n);
        let r = node.rect;
        if r.w <= 0 || r.h <= 0 {
            continue;
        }
        let label = node
            .ctrl
            .name
            .clone()
            .unwrap_or_else(|| format!("{}@{}", node.ctrl.class, node.ctrl.source_line));
        if !inside(screen, r) && !scrolled(v, n) {
            out.push(format!("{label}: cut off by the window at {r:?}"));
            continue;
        }
        if let Some(p) = node.parent
            && !scrolled(v, n)
            && !inside(v.node(p).rect, r)
        {
            out.push(format!(
                "{label}: spills out of its parent {:?} at {r:?}",
                v.node(p).rect
            ));
        }
        let class = node.ctrl.class.to_ascii_lowercase();
        if (class == "guitextctrl" || class.contains("button"))
            && !scrolled(v, n)
            && let Some(text) = node.state.text.as_deref().or(node.ctrl.text.as_deref())
            && let Some(font) = pack
                .data
                .styles
                .get(&node.ctrl.style)
                .and_then(|s| s.font.as_deref())
                .and_then(|f| Font::get(pack, f))
        {
            let width = font.width(text);
            if width > r.w + 2 {
                out.push(format!("{label}: text {text:?} is {width}px in {}px", r.w));
            }
        }
    }
    out
}

/// Opens every screen at each size, renders it offscreen and reports
/// problems; screenshots and the report go to `out` when given.
fn every_screen_fits(pack: Rc<Pack>, out: Option<&std::path::Path>) -> anyhow::Result<()> {
    let gpu = Headless::new()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    let mut report = Vec::new();
    let mut windows_checked = 0;
    for size in SIZES {
        for (id, file) in screens() {
            let mut ui = Ui::new(
                pack.clone(),
                UiConfig {
                    size,
                    scale: None,
                    platform: Platform::Windows,
                },
                Settings {
                    binds: Some(vec![]),
                    ..Default::default()
                },
            );
            ui.core.push(id);
            ui.update(0);
            ui.update(16);
            let (w, h) = ui.logical_size();
            let screen = Rect::new(0, 0, w, h);
            let Some(s) = ui.screen(id) else {
                report.push(format!("{file} {}x{}: did not open", size.0, size.1));
                continue;
            };
            if s.view()
                .walk()
                .any(|n| s.view().node(n).ctrl.class == "GuiWindowCtrl")
            {
                windows_checked += 1;
            }
            for p in problems(&pack, s.view(), screen) {
                report.push(format!(
                    "{file} {}x{} (logical {w}x{h}): {p}",
                    size.0, size.1
                ));
            }
            let dl: DrawList = ui.draw();
            let pixels = gpu.render_rgba(
                &mut renderer,
                &pack,
                &dl,
                size,
                ui.scale(),
                [0., 0., 0., 1.],
            )?;
            assert_eq!(pixels.len(), (size.0 * size.1 * 4) as usize);
            if let Some(out) = out {
                image::save_buffer(
                    out.join(format!("{file}-{}x{}.png", size.0, size.1)),
                    &pixels,
                    size.0,
                    size.1,
                    image::ColorType::Rgba8,
                )?;
            }
        }
    }
    println!("{}", report.join("\n"));
    if let Some(out) = out {
        std::fs::write(out.join("report.txt"), report.join("\n") + "\n")?;
        println!(
            "{} findings; screenshots in {}",
            report.len(),
            out.display()
        );
    }
    // A dialog's own window must always fit: its title bar and buttons
    // are what the player needs.
    let windows: Vec<_> = report
        .iter()
        .filter(|l| l.contains("GuiWindowCtrl") && l.contains("cut off"))
        .collect();
    assert!(windows.is_empty(), "{windows:#?}");
    assert!(windows_checked > 0, "no screen had a window to check");
    Ok(())
}

#[test]
fn every_screen_fits_a_short_wide_window_720p_and_1440p_synthetic() -> anyhow::Result<()> {
    let mut data = bri_ui::schema::UiPack::default();
    bri_ui::testing::add_dialogs(&mut data);
    every_screen_fits(bri_ui::testing::pack(data), None)
}

#[test]
#[ignore = "requires generated v20 content"]
fn every_screen_fits_a_short_wide_window_720p_and_1440p() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pack = Rc::new(Pack::load(&root.join("content/ui-pack-004"))?);
    let out = std::env::var_os("BRI_SWEEP_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/screen-sweep"));
    std::fs::create_dir_all(&out)?;
    every_screen_fits(pack, Some(&out))
}
