//! Offscreen renders of the modder tools' screens, for layout review: the
//! Tune window over a body's parts, and an Add-On's details saying why it
//! is not in every game and that it changed during the game. Needs the
//! generated UI pack and an offscreen GPU; `BRI_UI_PACK` names the pack
//! folder (default: the content root's), `BRI_RENDER_OUT` where the PNGs
//! go (default `artifacts/modder-tools`).
use anyhow::Context;
use bri_ui::{
    api::*,
    binds::Platform,
    gpu::{Headless, UiRenderer},
    pack::Pack,
    screens::ScreenId,
    ui::{Ui, UiConfig},
    view::{EventKind, ViewEvent},
};
use std::{path::PathBuf, rc::Rc};

fn open_ui(pack: &Rc<Pack>, size: (u32, u32), scale: f32) -> Ui {
    Ui::new(
        pack.clone(),
        UiConfig {
            size,
            scale: Some(scale),
            platform: Platform::Windows,
        },
        Settings {
            binds: Some(vec![]),
            ..Default::default()
        },
    )
}

fn event(ui: &mut Ui, screen: ScreenId, name: &str, kind: EventKind) {
    let i = ui.dialogs.iter().rposition(|s| s.id() == screen).unwrap();
    let node = ui.dialogs[i].view().id(name).unwrap();
    let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
    dialogs[i].on_event(&ViewEvent { node, kind }, core);
}

fn select(ui: &mut Ui, screen: ScreenId, list: &str, label: &str) {
    let i = ui.dialogs.iter().rposition(|s| s.id() == screen).unwrap();
    let view = ui.dialogs[i].view_mut();
    let n = view.id(list).unwrap();
    let id = view
        .node(n)
        .state
        .items
        .iter()
        .find(|(l, _)| l.contains(label))
        .map(|(_, id)| *id)
        .with_context(|| format!("no {label} in {list}"))
        .unwrap();
    view.select(n, Some(id));
    event(ui, screen, list, EventKind::Changed);
}

/// A body as the Tune window shows it, from the motor's numbers.
fn runner() -> TuneView {
    let table = bri_sim::archetype::Archetypes::default();
    let mut body = table.resolve(Default::default()).clone();
    body.id = "playground-runner:archetype/runner".into();
    body.name = "Runner".into();
    let window = bri_client::tune::TuneWindow::open(&body, None);
    let mut table = table;
    table.add(body).unwrap();
    let mut view = window.view(&table, "").unwrap();
    // As after two changes from the sliders.
    view.unsaved = true;
    for f in view
        .fields
        .iter_mut()
        .filter(|f| f.group == "Movement")
        .take(2)
    {
        f.unsaved = true;
    }
    view
}

fn add_on_rows() -> AddOnsView {
    AddOnsView {
        rows: vec![
            AddOnRow {
                id: "playground-stomper".into(),
                name: "Playground Stomper".into(),
                version: "1.0.0".into(),
                category: "Player Types".into(),
                enabled: true,
                runs: "Server and players".into(),
                description: "A heavy body that shakes the ground when it lands.".into(),
                authors: "Playground".into(),
                license: "CC0-1.0".into(),
                provides: vec!["a player type".into(), "a movement controller".into()],
                plays_only: "Plays only in Mixed Add-On Playground games: a Custom game leaves out the rules and bodies a game mode uses. Pick Mixed Add-On Playground as the game mode in Start Game to play it.".into(),
                changed_in_game: true,
                ..Default::default()
            },
            AddOnRow {
                id: "playground-rifle".into(),
                name: "Playground Rifle".into(),
                version: "1.0.0".into(),
                category: "Weapons & Items".into(),
                enabled: true,
                ..Default::default()
            },
        ],
        notice: "You are in a game, which keeps the Add-Ons it started with: changes here apply the next time you host or join.".into(),
    }
}

#[test]
#[ignore = "requires the generated UI pack and an offscreen GPU"]
fn modder_screens_render_offscreen() -> anyhow::Result<()> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pack_dir = match std::env::var_os("BRI_UI_PACK") {
        Some(dir) => PathBuf::from(dir),
        None => bri_package::testing::pack_dir(&workspace.join("content"), "ui_pack"),
    };
    let pack = Rc::new(Pack::load(&pack_dir)?);
    let out = std::env::var_os("BRI_RENDER_OUT")
        .map_or_else(|| workspace.join("artifacts/modder-tools"), PathBuf::from);
    std::fs::create_dir_all(&out)?;
    let gpu = Headless::new()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    let mut save = |ui: &mut Ui, name: &str, size: (u32, u32)| -> anyhow::Result<()> {
        ui.update(16);
        let draw = ui.draw();
        let pixels = gpu.render_rgba(
            &mut renderer,
            &pack,
            &draw,
            size,
            ui.scale(),
            [0.12, 0.16, 0.22, 1.0],
        )?;
        let png = out.join(format!("{name}-{}x{}.png", size.0, size.1));
        image::save_buffer(&png, &pixels, size.0, size.1, image::ColorType::Rgba8)
            .with_context(|| format!("Writing {}", png.display()))?;
        Ok(())
    };
    for (size, scale) in [((1024, 768), 1.0), ((1920, 1080), 2.0)] {
        for part in ["Movement", "Hands", "Camera"] {
            let mut ui = open_ui(&pack, size, scale);
            ui.apply(UiUpdate::Tune(Some(runner())));
            ui.update(0);
            select(&mut ui, ScreenId::Tune, "TB_Group", part);
            save(&mut ui, &format!("tune-{}", part.to_lowercase()), size)?;
        }
        let mut ui = open_ui(&pack, size, scale);
        ui.apply(UiUpdate::Tune(Some(runner())));
        ui.update(0);
        event(&mut ui, ScreenId::Tune, "TB_Done", EventKind::Click);
        save(&mut ui, "tune-done-unsaved", size)?;
        let mut ui = open_ui(&pack, size, scale);
        let mut saved = runner();
        for f in &mut saved.fields {
            f.unsaved = false;
        }
        saved.unsaved = false;
        saved.status = "Saved to addons/playground-runner/runner.json.".into();
        ui.apply(UiUpdate::Tune(Some(saved)));
        ui.update(0);
        save(&mut ui, "tune-saved", size)?;
        let mut ui = open_ui(&pack, size, scale);
        ui.apply(UiUpdate::AddOns(add_on_rows()));
        ui.core.push(ScreenId::AddOns);
        ui.update(0);
        select(&mut ui, ScreenId::AddOns, "AO_List", "Playground Stomper");
        save(&mut ui, "add-ons-not-in-every-game", size)?;
    }
    println!("renders in {}", out.display());
    Ok(())
}
