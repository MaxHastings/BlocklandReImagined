//! Offscreen renders of authored v20 layouts through the native UI stack
//! (pack → View layout/skins/fonts → DrawList → wgpu). Writes PNGs to
//! `artifacts/native-ui/` (git-ignored: they contain original artwork).
//!
//! Usage: ui_gallery [PACK_DIR] [OUT_DIR]

use bri_ui::draw::DrawList;
use bri_ui::geom::Rect;
use bri_ui::gpu::{Headless, UiRenderer};
use bri_ui::pack::Pack;
use bri_ui::view::View;
use std::path::PathBuf;

fn scene(pack: &Pack, name: &str, w: i32, h: i32) -> View {
    let mut v = View::new(&pack.data.layouts[name]);
    let set_text = |v: &mut View, n: &str, t: &str| {
        if let Some(id) = v.id(n) {
            v.set_text(id, t);
        }
    };
    match name {
        "MainMenuGui" => {
            set_text(&mut v, "MM_Version", "Version: 20");
            for hide in [
                "MM_AuthBar",
                "DemoBanner",
                "buyNowButton_W",
                "buyNowButton_B",
            ] {
                if let Some(id) = v.id(hide) {
                    v.set_visible(id, false);
                }
            }
            if let Some(id) = v.id("MM_BG") {
                v.state(id).bitmap = Some("screenshots/icepalace".into());
            }
        }
        "startMissionGui" => {
            if let Some(id) = v.id("SM_missionList") {
                let mut maps: Vec<_> = pack
                    .data
                    .maps
                    .iter()
                    .map(|m| m.display_name.clone())
                    .collect();
                maps.sort();
                v.state(id).items = maps
                    .into_iter()
                    .enumerate()
                    .map(|(i, m)| (m, i as i64))
                    .collect();
                let sel = v
                    .node(id)
                    .state
                    .items
                    .iter()
                    .find(|(t, _)| t == "Bedroom")
                    .map(|x| x.1);
                v.select(id, sel);
            }
            if let (Some(id), Some(m)) = (
                v.id("SM_MapPreview"),
                pack.data.maps.iter().find(|m| m.display_name == "Bedroom"),
            ) {
                v.state(id).bitmap = m.preview.clone();
                set_text(&mut v, "SM_MapDescription", &m.description);
            }
            if let Some(id) = v.id("SM_OptSinglePlayer") {
                v.select_radio(id);
            }
        }
        _ => {}
    }
    v.layout(w, h);
    v
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let pack_dir = PathBuf::from(
        args.get(1)
            .map_or("../../content/ui-pack-001", String::as_str),
    );
    let out = PathBuf::from(
        args.get(2)
            .map_or("../../artifacts/native-ui", String::as_str),
    );
    std::fs::create_dir_all(&out)?;
    let pack = Pack::load(&pack_dir)?;
    let gpu = Headless::new()?;
    eprintln!(
        "adapter: {} ({:?})",
        gpu.adapter_info.name, gpu.adapter_info.backend
    );
    let mut r = UiRenderer::new(&gpu.device, &gpu.queue);
    let screens = [
        "MainMenuGui",
        "defaultControlsGui",
        "startMissionGui",
        "JoinServerGui",
        "LoadingGui",
        "escapeMenu",
        "optionsDlg",
        "AvatarGui",
        "BrickSelectorDlg",
        "PrintSelectorDlg",
        "wrenchDlg",
        "wrenchSoundDlg",
        "wrenchVehicleSpawnDlg",
        "wrenchEventsDlg",
        "NewPlayerListGui",
        "saveBricksGui",
        "LoadBricksGui",
        "MessageBoxYesNoDlg",
    ];
    // (physical size, UI scale)
    let targets = [
        ((1024u32, 768u32), 1.0f32),
        ((1920, 1080), 1.0),
        ((1920, 1080), 2.0),
    ];
    let mut report = Vec::new();
    for name in screens {
        if !pack.data.layouts.contains_key(name) {
            report.push(format!("{name}: MISSING LAYOUT"));
            continue;
        }
        for ((pw, ph), scale) in targets {
            let (lw, lh) = ((pw as f32 / scale) as i32, (ph as f32 / scale) as i32);
            let v = scene(&pack, name, lw, lh);
            let mut dl = DrawList::new(Rect::new(0, 0, lw, lh));
            // Dialogs are drawn over the main menu like in v20.
            if name != "MainMenuGui" && name != "LoadingGui" {
                scene(&pack, "MainMenuGui", lw, lh).draw(&pack, &mut dl);
            }
            v.draw(&pack, &mut dl);
            let px = gpu.render_rgba(&mut r, &pack, &dl, (pw, ph), scale, [0.0, 0.0, 0.0, 1.0])?;
            let file = out.join(format!("{name}_{pw}x{ph}@{scale}x.png"));
            image::save_buffer(&file, &px, pw, ph, image::ColorType::Rgba8)?;
            report.push(format!(
                "{name} {pw}x{ph}@{scale}x: {} draw commands, {} glyphs",
                dl.cmds.len(),
                dl.glyph_count()
            ));
        }
    }
    let missing: Vec<String> = r.missing_textures().map(|k| format!("{k:?}")).collect();
    report.push(format!(
        "missing textures: {}",
        if missing.is_empty() {
            "none".into()
        } else {
            missing.join(", ")
        }
    ));
    std::fs::write(out.join("gallery-report.txt"), report.join("\n") + "\n")?;
    println!("{}", report.join("\n"));
    Ok(())
}
