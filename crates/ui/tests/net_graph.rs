//! The net graph (Ctrl+N, v20's `toggleNetGraph`) and the performance
//! overlay (F3) through key binds, the remap list and saved controls.
use bri_ui::{
    api::*,
    binds::{BindMap, Platform},
    geom::Rect,
    input::*,
    models::perf::{FrameSample, NetSample, PerfMode},
    pack::Pack,
    schema::{DefaultBind, Device, RemapEntry, UiPack},
    screens::ctrl,
    ui::{Ui, UiConfig},
};
use std::{path::PathBuf, rc::Rc};

const CTRL: Modifiers = Modifiers {
    ctrl: true,
    ..Modifiers::NONE
};

fn remap(name: &str, command: &str) -> RemapEntry {
    RemapEntry {
        division: None,
        name: name.into(),
        command: command.into(),
    }
}

fn pack() -> Rc<Pack> {
    let mut p = UiPack::default();
    for name in ["MainMenuGui", "PlayGui", "LoadingGui", "defaultControlsGui"] {
        p.layouts.insert(
            name.into(),
            ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480)),
        );
    }
    // v20's Gui section around Toggle NetGraph, and its Ctrl+N default.
    p.data.remap = vec![
        remap("Show Player List", "showPlayerList"),
        remap("Toggle NetGraph", "toggleNetGraph"),
        remap("Toggle Player Names / Crosshair", "ToggleShapeNameHud"),
    ];
    p.data.default_binds = vec![DefaultBind {
        device: Device::Keyboard,
        key: "ctrl n".into(),
        command: "toggleNetGraph".into(),
        when: vec![],
        source_line: 15373,
    }];
    Rc::new(Pack::from_parts(p, PathBuf::new()))
}

fn ui(binds: Option<Vec<BindEntry>>) -> Ui {
    let mut u = Ui::new(
        pack(),
        UiConfig {
            size: (1280, 960),
            scale: Some(2.0),
            platform: Platform::Windows,
        },
        Settings {
            binds,
            ..Default::default()
        },
    );
    u.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: true,
        single_player: true,
        admin: true,
    }));
    u.drain_actions();
    u
}

fn press(u: &mut Ui, key: Key, mods: Modifiers) {
    u.handle_input(InputEvent::KeyDown {
        key,
        mods,
        repeat: false,
    });
    u.handle_input(InputEvent::KeyUp { key, mods });
}

fn chord(key: Key, mods: Modifiers) -> BindInput {
    BindInput::Key(Chord { key, mods })
}

fn defaults() -> Vec<BindEntry> {
    BindMap::defaults(&pack().data.data, 2, 0, Platform::Windows).entries
}

#[test]
fn ctrl_n_toggles_the_net_graph_like_v20() {
    let mut u = ui(Some(defaults()));
    assert!(u.core.net_graph.is_none());
    press(&mut u, Key::Letter('n'), CTRL);
    assert!(u.core.net_graph.is_some());
    u.apply(UiUpdate::NetSample(NetSample {
        interval_ms: 32.0,
        latency_ms: 42.0,
        ..Default::default()
    }));
    assert_eq!(
        u.core
            .net_graph
            .as_ref()
            .unwrap()
            .latest()
            .unwrap()
            .latency_ms,
        42.0
    );
    press(&mut u, Key::Letter('n'), CTRL);
    assert!(
        u.core.net_graph.is_none(),
        "toggling off removes it and its history"
    );
    // Samples while hidden are dropped.
    u.apply(UiUpdate::NetSample(NetSample::default()));
    assert!(u.core.net_graph.is_none());
}

#[test]
fn f3_cycles_the_overlay_and_ctrl_f3_asks_for_a_capture() {
    let mut u = ui(Some(defaults()));
    assert_eq!(u.core.perf.mode, PerfMode::Off);
    u.apply(UiUpdate::PerfFrame(FrameSample {
        frame_ms: 16.0,
        ..Default::default()
    }));
    assert_eq!(
        u.core.perf.frames().count(),
        0,
        "nothing is kept while hidden"
    );
    press(&mut u, Key::F(3), Modifiers::NONE);
    assert_eq!(u.core.perf.mode, PerfMode::Compact);
    press(&mut u, Key::F(3), Modifiers::NONE);
    assert_eq!(u.core.perf.mode, PerfMode::Expanded);
    press(&mut u, Key::F(3), CTRL);
    assert_eq!(
        u.drain_actions()
            .into_iter()
            .map(|(_, a)| a)
            .collect::<Vec<_>>(),
        vec![UiAction::Game(GameAction::SavePerfCapture)]
    );
    press(&mut u, Key::F(3), Modifiers::NONE);
    assert_eq!(u.core.perf.mode, PerfMode::Off);
}

#[test]
fn overlay_commands_follow_toggle_netgraph_in_the_remap_list() {
    let u = ui(Some(defaults()));
    let names: Vec<&str> = u.core.remap.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Show Player List",
            "Toggle NetGraph",
            "Toggle Performance Overlay",
            "Save Performance Capture",
            "Toggle Player Names / Crosshair",
        ]
    );
    assert_eq!(u.core.remap_commands.len(), names.len());
    assert_eq!(u.core.binds.display("togglePerfOverlay"), "F3");
    assert_eq!(u.core.binds.display("savePerfCapture"), "CTRL F3");
    assert_eq!(u.core.binds.display("toggleNetGraph"), "CTRL N");
}

#[test]
fn saved_controls_gain_the_overlay_keys_only_where_free() {
    // Controls saved before the overlay existed gain its keys.
    let old = vec![BindEntry {
        command: "toggleNetGraph".into(),
        input: chord(Key::Letter('n'), CTRL),
    }];
    let u = ui(Some(old));
    assert_eq!(
        u.core.binds.command_for(&chord(Key::F(3), Modifiers::NONE)),
        Some("togglePerfOverlay")
    );
    // A player who put something else on F3 keeps it; the overlay stays
    // unbound until they choose a key in Options.
    let taken = vec![BindEntry {
        command: "toggleNetGraph".into(),
        input: chord(Key::F(3), Modifiers::NONE),
    }];
    let mut u = ui(Some(taken));
    assert_eq!(u.core.binds.binding_of("togglePerfOverlay"), None);
    assert_eq!(
        u.core.binds.command_for(&chord(Key::F(3), CTRL)),
        Some("savePerfCapture")
    );
    press(&mut u, Key::F(3), Modifiers::NONE);
    assert!(
        u.core.net_graph.is_some(),
        "F3 is the player's net graph key"
    );
    // Rebinding is the usual Options remap.
    u.core
        .binds
        .force_remap("togglePerfOverlay", chord(Key::F(4), Modifiers::NONE));
    press(&mut u, Key::F(4), Modifiers::NONE);
    assert_eq!(u.core.perf.mode, PerfMode::Compact);
}

#[test]
fn overlays_draw_above_every_screen_and_nothing_when_hidden() {
    let mut u = ui(Some(defaults()));
    let base = u.draw().cmds.len();
    u.core.toggle_net_graph();
    u.apply(UiUpdate::NetSample(NetSample {
        interval_ms: 32.0,
        latency_ms: 30.0,
        ..Default::default()
    }));
    assert!(u.draw().cmds.len() > base);
    u.core.toggle_net_graph();
    assert_eq!(u.draw().cmds.len(), base);
}

/// Both overlays, filled with samples, rendered offscreen at three sizes;
/// PNGs go to `out` when given.
#[cfg(feature = "gpu")]
fn overlays_render_offscreen(pack: Rc<Pack>, out: Option<&std::path::Path>) {
    use bri_ui::gpu::{Headless, UiRenderer};
    use bri_ui::models::perf::{PerfStats, ServerStats};
    let gpu = Headless::new().unwrap();
    let mut r = UiRenderer::new(&gpu.device, &gpu.queue);
    for (pw, ph, scale) in [
        (1024u32, 768u32, 1.0f32),
        (1920, 1080, 2.0),
        (3840, 2160, 4.0),
    ] {
        let mut u = Ui::new(
            pack.clone(),
            UiConfig {
                size: (pw, ph),
                scale: Some(scale),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        u.core.toggle_net_graph();
        u.core.perf.cycle();
        u.core.perf.cycle();
        for i in 0..240 {
            let t = i as f32;
            u.apply(UiUpdate::NetSample(NetSample {
                interval_ms: 32.0,
                ghosts_active: 12.0,
                ghost_updates: (t * 0.7).sin().abs() * 6.0,
                bits_sent: 2400.0 + (t * 0.3).sin() * 800.0,
                bits_received: 9000.0 + (t * 0.2).cos() * 3000.0,
                latency_ms: 38.0 + (t * 0.1).sin() * 6.0,
                packet_loss: if i % 60 < 5 { 2.0 } else { 0.0 },
                packets_sent: 1.0,
                packets_received: 4.0,
            }));
            u.apply(UiUpdate::PerfFrame(FrameSample {
                frame_ms: if i % 50 == 0 {
                    40.0
                } else {
                    7.0 + (t * 0.5).sin()
                },
                cpu_ms: 3.5,
                wait_ms: 3.0,
                gpu_ms: Some(4.2),
            }));
        }
        u.apply(UiUpdate::PerfStats(PerfStats {
            bricks: Some(18_342),
            players: Some(4),
            vehicles: Some(2),
            entities: Some(9),
            memory_bytes: Some(734 << 20),
            private_bytes: Some(912 << 20),
            server: Some(ServerStats {
                ticks_per_second: 120.0,
                tick_ms_mean: 0.84,
                tick_ms_max: 2.31,
                script_ms: vec![
                    ("Gamemode_Slayer".into(), 0.21),
                    ("Server_Voxels".into(), 0.05),
                ],
            }),
            remote_server: false,
            gpu: "NVIDIA GeForce RTX 3070".into(),
            gpu_passes: vec![
                ("sun shadows".into(), 1.2),
                ("lamp shadows".into(), 0.3),
                ("world".into(), 2.4),
                ("effects".into(), 0.1),
            ],
        }));
        let dl = u.draw();
        let px = gpu
            .render_rgba(&mut r, &pack, &dl, (pw, ph), scale, [0.2, 0.25, 0.3, 1.0])
            .unwrap();
        assert_eq!(px.len(), (pw * ph * 4) as usize);
        assert!(dl.glyph_count() > 0, "the overlays print their numbers");
        if let Some(out) = out {
            image::save_buffer(
                out.join(format!("overlays_{pw}x{ph}.png")),
                &px,
                pw,
                ph,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}

#[cfg(feature = "gpu")]
#[test]
fn overlays_render_offscreen_synthetic() {
    overlays_render_offscreen(bri_ui::testing::pack(UiPack::default()), None);
}

/// Offscreen look check with the converted v20 UI pack: writes PNGs of both
/// overlays over a black frame to `artifacts/perf-overlay/` (git-ignored).
/// `BRI_UI_PACK` names the pack when it is not in this checkout.
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires generated v20 content"]
fn overlays_render_offscreen_content() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::var_os("BRI_UI_PACK")
        .map(PathBuf::from)
        .unwrap_or_else(|| bri_package::testing::pack_dir(&root.join("content"), "ui_pack"));
    let out = root.join("artifacts/perf-overlay");
    std::fs::create_dir_all(&out).unwrap();
    overlays_render_offscreen(Rc::new(Pack::load(&dir).unwrap()), Some(&out));
}
