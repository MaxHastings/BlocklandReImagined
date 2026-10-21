//! Offscreen renders of Options → Controls with a game's control profile:
//! a window-less look at the game selector, status line, list and buttons.
//! Does not create a window or send any operating-system input.
//!
//! `controls_probe [ui-pack] [out-dir]`
use anyhow::{Context, Result};
use bri_content::controls::{LayerDef, LayerWhen, ProfileDef};
use bri_ui::{
    api::*,
    binds::{BindMap, Platform},
    gpu::{Headless, UiRenderer},
    input::{InputEvent, Key, Modifiers, MouseButton},
    pack::Pack,
    profiles::Hides,
    screens::ScreenId,
    ui::{Ui, UiConfig},
};
use std::{collections::BTreeMap, path::PathBuf, rc::Rc};

fn bind(name: &str, command: &str, key: Option<&str>, mouse: Option<&str>) -> PackageBind {
    PackageBind {
        division: "Arena Assault".into(),
        name: name.into(),
        package: "aa-rules".into(),
        command: command.into(),
        key: key.map(Into::into),
        mouse: mouse.map(Into::into),
        profile: "aa".into(),
        stock: None,
        hold: false,
        screen: None,
        layer: String::new(),
    }
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

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let pack_path = PathBuf::from(args.first().map_or("content/ui-pack-004", String::as_str));
    let out = PathBuf::from(
        args.get(1)
            .map_or("artifacts/controls-probe", String::as_str),
    );
    std::fs::create_dir_all(&out)?;
    let pack = Rc::new(Pack::load(&pack_path)?);
    let gpu = Headless::new()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    let (size, scale) = ((1600u32, 1200u32), 2.0f32);
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
    ui.set_profile_defs(vec![
        ProfileDef {
            id: "aa".into(),
            name: "Arena Assault".into(),
            inherit: true,
            player_types: vec!["aa-rules:archetype/soldier".into()],
            layers: vec![LayerDef {
                id: "aircraft".into(),
                name: Some("In aircraft".into()),
                when: LayerWhen::MouseDriver,
                invert_mouse: Some(true),
                mouse_sensitivity: None,
            }],
            invert_mouse: None,
            vehicle_invert_mouse: None,
            mouse_sensitivity: Some(0.75),
        },
        ProfileDef {
            id: "halo".into(),
            name: "Halo".into(),
            inherit: true,
            player_types: vec!["halo-rules:archetype/spartan".into()],
            layers: vec![],
            invert_mouse: None,
            vehicle_invert_mouse: None,
            mouse_sensitivity: None,
        },
    ]);
    let mut stock = bind("Crouch", "crouch", Some("lcontrol"), None);
    stock.stock = Some("crouch".into());
    ui.set_package_binds(vec![
        bind("Sprint", "sprint", Some("lshift"), None),
        bind("Fire Mode", "firemode", Some("v"), None),
        bind("Reload", "reload", Some("r"), None),
        bind("Aim Down Sights", "aim", None, Some("button1")),
        bind("Throw Grenade", "grenade", Some("g"), None),
        stock,
        bind("Prone", "prone", None, None),
    ]);
    ui.set_profile_hides(BTreeMap::from([(
        "aa".to_string(),
        Hides {
            building: true,
            jet: true,
        },
    )]));
    ui.core.conn = ConnectionState::InGame {
        server_name: "Probe".into(),
        max_players: 8,
        local: true,
        single_player: true,
        admin: false,
    };
    ui.set_control_context("aa-rules:archetype/soldier", None);
    ui.core.push(ScreenId::Options);
    ui.update(0);
    click(&mut ui, ScreenId::Options, "optionsDlg.setPane(Controls);")?;
    ui.update(0);
    let shot = |ui: &Ui, name: &str, renderer: &mut UiRenderer| -> Result<()> {
        let dl = ui.draw();
        let px = gpu.render_rgba(renderer, &pack, &dl, size, scale, [0.16, 0.22, 0.3, 1.0])?;
        image::save_buffer(
            out.join(format!("{name}.png")),
            &px,
            size.0,
            size.1,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    };
    shot(&ui, "controls-aa-in-game", &mut renderer)?;
    click(&mut ui, ScreenId::Options, "OptGameMenu")?;
    ui.update(0);
    shot(&ui, "controls-game-menu-open", &mut renderer)?;
    // Pick Blockland from the open list: its full list and brick options.
    key(&mut ui, Key::Up);
    key(&mut ui, Key::Return);
    ui.update(0);
    anyhow::ensure!(
        ui.core.profiles.viewed == "blockland",
        "Blockland not shown"
    );
    shot(&ui, "controls-blockland-while-in-aa", &mut renderer)?;
    // Back to Arena Assault, then the mouse context menu.
    click(&mut ui, ScreenId::Options, "OptGameMenu")?;
    key(&mut ui, Key::Down);
    key(&mut ui, Key::Return);
    ui.update(0);
    anyhow::ensure!(ui.core.profiles.viewed == "aa", "Arena Assault not shown");
    click(&mut ui, ScreenId::Options, "OptLayerMenu")?;
    key(&mut ui, Key::Down);
    key(&mut ui, Key::Return);
    ui.update(0);
    shot(&ui, "controls-aa-aircraft-context", &mut renderer)?;
    // Clear All names the game.
    click(&mut ui, ScreenId::Options, "optionsDlg.clearAllBinds();")?;
    ui.update(0);
    shot(&ui, "controls-aa-clear-all-confirm", &mut renderer)?;
    key(&mut ui, Key::Escape);
    ui.update(0);
    // Defaults names the game too (Blockland's still opens its own chooser).
    click(
        &mut ui,
        ScreenId::Options,
        "canvas.pushDialog(DefaultControlsGui);",
    )?;
    ui.update(0);
    shot(&ui, "controls-aa-defaults-confirm", &mut renderer)?;
    println!("wrote {}", out.display());
    Ok(())
}
