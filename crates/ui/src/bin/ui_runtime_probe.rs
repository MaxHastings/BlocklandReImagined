//! Actual-pack headless screen flow and offscreen renderer evidence.
//! Does not create a window or send any operating-system input.
use anyhow::{Context, Result, ensure};
use bri_ui::{
    api::*,
    binds::{BindMap, Platform},
    draw::{DrawList, Filter},
    geom::Rect,
    gpu::{Headless, UiRenderer},
    input::{InputEvent, Key, Modifiers, MouseButton},
    pack::{Pack, TexKey},
    screens::ScreenId,
    ui::{Ui, UiConfig},
};
use serde::Deserialize;
use std::{path::PathBuf, rc::Rc};
#[derive(Deserialize)]
struct Catalog {
    bricks: Vec<Brick>,
}
#[derive(Deserialize)]
struct Brick {
    id: String,
    display_name: String,
    category: String,
    subcategory: String,
    icon_source: String,
}
fn key(ui: &mut Ui, key: Key) {
    ui.handle_input(InputEvent::KeyDown {
        key,
        mods: Modifiers::NONE,
        repeat: false,
    });
    ui.handle_input(InputEvent::KeyUp {
        key,
        mods: Modifiers::NONE,
    });
}
fn click(ui: &mut Ui, screen: ScreenId, name: &str) -> Result<()> {
    let (x, y) = ui
        .control_center(screen, name)
        .with_context(|| format!("missing {screen:?}/{name}"))?;
    ui.handle_input(InputEvent::MouseMove { x, y });
    ui.handle_input(InputEvent::MouseDown {
        button: MouseButton::Left,
        x,
        y,
    });
    ui.handle_input(InputEvent::MouseUp {
        button: MouseButton::Left,
        x,
        y,
    });
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let pack_path = PathBuf::from(args.first().map_or("content/ui-pack-004", String::as_str));
    let out = PathBuf::from(
        args.get(1)
            .map_or("artifacts/native-ui-runtime", String::as_str),
    );
    std::fs::create_dir_all(&out)?;
    let pack = Rc::new(Pack::load(&pack_path)?);
    let catalog: Catalog = serde_json::from_slice(&std::fs::read(
        "content/stock-catalog-004/stock-catalog.json",
    )?)?;
    let bricks: Vec<_> = catalog
        .bricks
        .into_iter()
        .filter(|b| {
            !b.display_name.is_empty() && !b.category.is_empty() && !b.subcategory.is_empty()
        })
        .map(|b| {
            let icon = b.icon_source.to_lowercase();
            BrickInfo {
                id: b.id,
                ui_name: b.display_name,
                category: b.category,
                subcategory: b.subcategory,
                icon: if pack.has_image(&icon) {
                    IconRef::Pack(icon)
                } else {
                    IconRef::None
                },
            }
        })
        .collect();
    let missing_icons = bricks.iter().filter(|b| b.icon == IconRef::None).count();
    let gpu = Headless::new()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    // A deliberately nonsquare texture verifies that External UVs sample the
    // whole image instead of its upper-left pixel, including scissor clipping.
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("UI external UV test"),
        size: wgpu::Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        tex.as_image_copy(),
        &[255, 0, 0, 255, 0, 0, 255, 255],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(1),
        },
        tex.size(),
    );
    renderer.set_external(17, tex.create_view(&Default::default()), (2, 1));
    let mut dl = DrawList::new(Rect::new(0, 0, 64, 32));
    dl.image(
        TexKey::External(17),
        [0.0, 0.0, 1.0, 1.0],
        [0.0, 0.0, 64.0, 32.0],
        [255; 4],
        Filter::Nearest,
    );
    let pixels = gpu.render_rgba(
        &mut renderer,
        &pack,
        &dl,
        (64, 32),
        1.0,
        [0.0, 0.0, 0.0, 1.0],
    )?;
    ensure!(
        pixels[4 * 8..4 * 8 + 4] == [255, 0, 0, 255]
            && pixels[4 * 56..4 * 56 + 4] == [0, 0, 255, 255],
        "external texture must sample both halves"
    );
    let mut report = Vec::new();
    for (size, scale) in [((1024, 768), 1.0), ((1920, 1080), 1.0), ((1920, 1080), 2.0)] {
        let settings = Settings {
            binds: Some(BindMap::defaults(&pack.data.data, 2, 0, Platform::Windows).entries),
            mouse_type: 2,
            ..Default::default()
        };
        let mut ui = Ui::new(
            pack.clone(),
            UiConfig {
                size,
                scale: Some(scale),
                platform: Platform::Windows,
            },
            settings,
        );
        let render = |ui: &Ui,
                      name: &str,
                      renderer: &mut UiRenderer,
                      report: &mut Vec<serde_json::Value>|
         -> Result<()> {
            let dl = ui.draw();
            let px = gpu.render_rgba(renderer, &pack, &dl, size, scale, [0.16, 0.22, 0.3, 1.0])?;
            let file = format!("{name}_{}x{}@{scale}x.png", size.0, size.1);
            image::save_buffer(
                out.join(&file),
                &px,
                size.0,
                size.1,
                image::ColorType::Rgba8,
            )?;
            report.push(serde_json::json!({"file":file,"draw_commands":dl.cmds.len(),"glyphs":dl.glyph_count()}));
            Ok(())
        };
        render(&ui, "main-menu", &mut renderer, &mut report)?;
        // `~` console over the main menu: echo, help, a warning and an error.
        ui.core.toggle_console();
        ui.update(0);
        bri_console::warn("Example warning line");
        bri_console::error("Example error line");
        for line in ["help", "volume"] {
            for c in line.chars() {
                ui.handle_input(InputEvent::Char(c));
            }
            key(&mut ui, Key::Return);
        }
        for c in "conn".chars() {
            ui.handle_input(InputEvent::Char(c));
        }
        ui.update(16);
        ensure!(ui.top_id() == ScreenId::Console, "console must open");
        render(&ui, "console", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        ui.update(200);
        ui.core.push(ScreenId::JoinServer);
        ui.apply(UiUpdate::LanServers {
            servers: vec![ServerInfo {
                address: "192.168.1.20:28000".into(),
                name: "Max's Server".into(),
                password: false,
                dedicated: false,
                ping_ms: Some(12),
                players: 2,
                max_players: 8,
                bricks: 1534,
                map: "Bedroom".into(),
                favorite: true,
            }],
            querying: false,
        });
        ui.update(0);
        render(&ui, "join-server", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        ui.apply(UiUpdate::Maps(vec![MapInfo {
            id: "bedroom".into(),
            name: "Bedroom".into(),
            description: "Converted Bedroom".into(),
            preview: pack
                .data
                .maps
                .iter()
                .find(|m| m.display_name == "Bedroom")
                .and_then(|m| m.preview.clone())
                .map_or(IconRef::None, IconRef::Pack),
        }]));
        click(&mut ui, ScreenId::MainMenu, "MM_StartButton")?;
        ensure!(ui.top_id() == ScreenId::StartMission, "start click routing");
        render(&ui, "start-mission", &mut renderer, &mut report)?;
        click(&mut ui, ScreenId::StartMission, "SM_StartMission();")?;
        ensure!(
            ui.drain_actions()
                .iter()
                .any(|(_, a)| matches!(a,UiAction::HostGame{map,..} if map=="bedroom")),
            "authored host button dispatch"
        );
        ui.apply(UiUpdate::Connection(ConnectionState::Connecting {
            text: "Connecting to Local Host...".into(),
        }));
        render(&ui, "connecting", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        ensure!(
            ui.drain_actions()
                .iter()
                .any(|(_, a)| matches!(a, UiAction::CancelConnect)),
            "connect cancel"
        );
        ui.apply(UiUpdate::Connection(ConnectionState::Loading {
            map: "Bedroom".into(),
            preview: IconRef::None,
            status: "RECEIVING WORLD".into(),
            progress: 0.6,
        }));
        render(&ui, "loading", &mut renderer, &mut report)?;
        ui.apply(UiUpdate::Bricks(bricks.clone()));
        ui.apply(UiUpdate::Colorset(
            pack.data
                .data
                .brick_colorset
                .iter()
                .map(|d| PaintDivision {
                    name: d.name.clone(),
                    colors: d.colors.clone(),
                })
                .collect(),
        ));
        ui.apply(UiUpdate::Connection(ConnectionState::InGame {
            server_name: "Native UI verification".into(),
            max_players: 8,
            local: true,
            single_player: false,
            admin: true,
        }));
        ui.apply(UiUpdate::BrickInventory(
            bricks.iter().take(10).map(|b| Some(b.id.clone())).collect(),
        ));
        ui.apply(UiUpdate::Tools(vec![
            Some(ToolInfo {
                id: "tool/hammer".into(),
                name: "Hammer".into(),
                icon: IconRef::Pack("base/client/ui/itemicons/hammer".into()),
                tint: None,
            }),
            None,
            None,
            None,
            None,
        ]));
        ui.apply(UiUpdate::Chat {
            text: "Native UI: original cached fonts and art".into(),
        });
        ui.core.run_command("useBricks", true);
        ui.update(120);
        render(&ui, "hud-bricks", &mut renderer, &mut report)?;
        ui.core.run_command("toggleSuperShift", true);
        ui.core.run_command("toggleSuperShift", false);
        ui.apply(UiUpdate::PlantError(PlantError::Overlap));
        ui.update(16);
        render(&ui, "hud-super-shift", &mut renderer, &mut report)?;
        ui.apply(UiUpdate::CenterPrint {
            text: "\u{E005}Respawning in 3 seconds...\n\u{E003}Second line".into(),
            seconds: 2.0,
        });
        ui.apply(UiUpdate::BottomPrint {
            text: "\u{E006}Bottom print".into(),
            seconds: 2.0,
            hide_bar: false,
        });
        ui.update(16);
        render(&ui, "hud-prints", &mut renderer, &mut report)?;
        ui.apply(UiUpdate::ClearPrints);
        ui.core.run_command("toggleSuperShift", true);
        ui.core.run_command("toggleSuperShift", false);
        ui.core.run_command("useSprayCan", true);
        ui.update(120);
        render(&ui, "hud-paint", &mut renderer, &mut report)?;
        ui.core.run_command("useTools", true);
        ui.update(120);
        render(&ui, "hud-tools", &mut renderer, &mut report)?;
        // A v20-shaped chat history: player chat, server messages and the
        // save/load messages, then the Say and Team boxes, then a page up.
        for text in [
            "\u{E003}Blockhead\u{E006}: hello",
            "\u{E003}Max\u{E000} cleared all bricks.",
            "Loading bricks. Please wait.",
            "412 / 412 bricks created in 3 seconds",
            "\u{E001}Blockhead \u{E006}spawned the \u{E003}Jeep",
            "\u{E003}Max\u{E006}: nice build",
        ] {
            ui.apply(UiUpdate::Chat { text: text.into() });
        }
        ui.apply(UiUpdate::Talking(vec!["Blockhead".into(), "Max".into()]));
        ui.core.run_command("useTools", true);
        ui.core.run_command("globalChat", true);
        ui.update(16);
        render(&ui, "hud-chat-say", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        ui.core.run_command("teamChat", true);
        ui.update(16);
        render(&ui, "hud-chat-team", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        ui.apply(UiUpdate::Talking(vec![]));
        for i in 0..12 {
            ui.apply(UiUpdate::Chat {
                text: format!("\u{E003}Blockhead\u{E006}: line {i}"),
            });
        }
        ui.core.run_command("pageUpNewChatHud", true);
        ui.core.run_command("pageUpNewChatHud", true);
        ui.update(16);
        render(&ui, "hud-chat-scrolled", &mut renderer, &mut report)?;
        for _ in 0..3 {
            ui.core.run_command("pageDownNewChatHud", true);
        }
        ui.core.push(ScreenId::MiniGameSettings);
        ui.update(0);
        render(&ui, "minigame-settings", &mut renderer, &mut report)?;
        let (x, y) = ui
            .control_center(ScreenId::MiniGameSettings, "CMG_Scroll")
            .context("missing CMG_Scroll")?;
        ui.handle_input(InputEvent::MouseMove { x, y });
        for _ in 0..4 {
            ui.handle_input(InputEvent::Wheel { delta: -1.0 });
        }
        render(
            &ui,
            "minigame-settings-scrolled",
            &mut renderer,
            &mut report,
        )?;
        key(&mut ui, Key::Escape);
        ui.core.push(ScreenId::EscapeMenu);
        ui.update(0);
        render(&ui, "escape-menu", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        ui.core.push(ScreenId::BrickSelector);
        ui.update(0);
        render(&ui, "brick-selector", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        ui.core.push(ScreenId::Options);
        ui.update(0);
        render(&ui, "options", &mut renderer, &mut report)?;
        click(&mut ui, ScreenId::Options, "OptGraphicsResolutionMenu")?;
        render(&ui, "options-resolution-menu", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        for pane in ["Audio", "Controls", "AdvGraphics"] {
            click(
                &mut ui,
                ScreenId::Options,
                &format!("optionsDlg.setPane({pane});"),
            )?;
            render(
                &ui,
                &format!("options-{}", pane.to_lowercase()),
                &mut renderer,
                &mut report,
            )?;
        }
        key(&mut ui, Key::Escape);
        ui.apply(UiUpdate::Players {
            rows: vec![PlayerRow {
                id: 1,
                name: "Blockhead".into(),
                score: 0,
                admin: true,
                super_admin: false,
                bl_id: None,
                trust: "You".into(),
                ignoring: false,
            }],
            server_name: "Native UI verification".into(),
            max_players: 8,
        });
        ui.core.push(ScreenId::PlayerList);
        ui.update(0);
        render(&ui, "player-list", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        ui.apply(UiUpdate::SaveContext {
            map: "Bedroom".into(),
            preview: IconRef::None,
        });
        ui.apply(UiUpdate::SaveFiles {
            maps: vec!["Bedroom".into()],
            files: vec![SaveFileInfo {
                name: "Demo.world.json".into(),
                map: "Bedroom".into(),
                modified: "2026-09-26".into(),
                description: "Original Demo build".into(),
                brick_count: Some(150),
                damaged: false,
            }],
        });
        ui.core.push(ScreenId::SaveBricks);
        ui.update(0);
        render(&ui, "save", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        ui.core.push(ScreenId::LoadBricks);
        ui.update(0);
        render(&ui, "load", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        let effects: serde_json::Value =
            serde_json::from_slice(&std::fs::read("content/effects-pass-004/effects.json")?)?;
        let mut menus = DatablockMenus::new();
        for (class, field) in [
            ("FxLightData", "lights"),
            ("ParticleEmitterData", "emitters"),
        ] {
            let choices = effects[field]
                .as_array()
                .context("effect definitions")?
                .iter()
                .filter_map(|v| {
                    let name = v["name"].as_str()?;
                    if name.is_empty() {
                        return None;
                    }
                    Some(Choice {
                        id: v["id"].as_str()?.into(),
                        name: name.into(),
                    })
                })
                .collect();
            menus.insert(class.into(), choices);
        }
        ui.apply(UiUpdate::Datablocks(menus));
        ui.apply(UiUpdate::Events(EventCatalog::from_tables(
            &pack.data.data.event_tables,
            CURRENT_BRICK_EVENT_INPUTS,
            CURRENT_BRICK_EVENT_OUTPUTS,
        )));
        ui.apply(UiUpdate::OpenWrench {
            brick: 1,
            variant: WrenchVariant::Normal,
            owner: "Blockhead".into(),
            data: WrenchData {
                colliding: true,
                rendering: true,
                raycasting: true,
                item_dir: 2,
                ..Default::default()
            },
            admin_override: false,
            events_allowed: true,
        });
        render(&ui, "wrench", &mut renderer, &mut report)?;
        ui.apply(UiUpdate::OpenEvents {
            brick: 1,
            builder: None,
            builder_name: None,
            rows: vec![
                EventRow::Editable(EventLine {
                    conditions: vec![],
                    enabled: true,
                    delay_ms: 500,
                    input: "onActivate".into(),
                    target: "Self".into(),
                    named_target: None,
                    output: "setColor".into(),
                    params: vec![ParamValue::PaintColor(3)],
                }),
                EventRow::Preserved {
                    enabled: true,
                    text: "Unimplemented imported event".into(),
                    token: "probe-preserved-row".into(),
                },
            ],
            named_targets: vec!["door".into()],
            allow_named: true,
        });
        render(&ui, "events", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        key(&mut ui, Key::Escape);
        ui.core.push(ScreenId::Avatar);
        ui.update(0);
        render(&ui, "avatar", &mut renderer, &mut report)?;
        key(&mut ui, Key::Escape);
        let prints: Vec<_> = pack
            .data
            .images
            .keys()
            .filter(|k| k.starts_with("add-ons/print_letters_default/icons/"))
            .map(|k| PrintInfo {
                id: k.clone(),
                name: k.rsplit('/').next().unwrap().into(),
                icon: IconRef::Pack(k.clone()),
            })
            .collect();
        ui.apply(UiUpdate::Prints {
            aspect: "Letters".into(),
            prints,
        });
        ui.apply(UiUpdate::OpenPrintSelector {
            aspect: "Letters".into(),
            current: None,
        });
        render(&ui, "print-selector", &mut renderer, &mut report)?;
    }
    let missing: Vec<_> = renderer
        .missing_textures()
        .map(|k| format!("{k:?}"))
        .collect();
    std::fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"adapter":format!("{} {:?}",gpu.adapter_info.name,gpu.adapter_info.backend),"external_uv_check":"passed","stock_bricks":bricks.len(),"stock_bricks_without_ui_pack_icon":missing_icons,"rendered":report,"missing_textures":missing,"scope":"Headless UI routing/rendering only; no gameplay or server adapter exercised."}),
        )?,
    )?;
    ensure!(missing.is_empty(), "missing rendered textures: {missing:?}");
    println!(
        "Rendered {} native screen frames; host/cancel flows and external UV check passed.",
        report.len()
    );
    Ok(())
}
