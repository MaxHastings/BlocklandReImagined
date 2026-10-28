//! Actual native content, image decoding and both menus. All assets invented.
//! Admin DTOs supply menu rows only; this is not a server-authority test.
use anyhow::{Context, Result, ensure};
use bri_client::{
    content::ClientContent,
    testing::{content_root::ContentRoot, png, sha256},
};
use bri_ui::{
    api::{IconRef, MapInfo, Settings, UiUpdate},
    binds::Platform,
    draw::DrawCmd,
    input::{InputEvent, MouseButton},
    models::admin::*,
    pack::TexKey,
    screens::ScreenId,
    ui::{Ui, UiConfig},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn add_map(root: &Path, package: &str, name: &str, color: Option<[u8; 4]>) -> Result<PathBuf> {
    let id = format!("{package}:map/main");
    let directory = root.join(package).join("native");
    let mut rooms = bri_content::testing::map_bundle::rooms_for(&[&id]);
    rooms[0].name = name.into();
    bri_content::testing::map_bundle::write_bundle(&directory, &rooms)?;
    let file = directory.join("bundle.json");
    let mut bundle: Value = serde_json::from_slice(&std::fs::read(&file)?)?;
    if let Some(color) = color {
        let bytes = png(3, 2, |_, _| color)?;
        std::fs::write(directory.join("picture.png"), &bytes)?;
        bundle["maps"][0]["preview"] = json!({"file":"picture.png","source":"invented companion.png","width":3,"height":2,"sha256":sha256(&bytes)});
    }
    std::fs::write(&file, serde_json::to_vec(&bundle)?)?;
    std::fs::write(
        root.join(package).join("package.json"),
        serde_json::to_vec(&json!({
            "schema_version":1,"id":package,"version":"1.0.0","api":1,"name":name,"license":"CC0-1.0",
            "provides":[{"id":id,"kind":"map","file":"native/bundle.json"}]
        }))?,
    )?;
    let mut set: Value = serde_json::from_slice(&std::fs::read(root.join("packages.json"))?)?;
    set["packages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":package,"version":"1.0.0","side":"shared","dir":package}));
    std::fs::write(root.join("packages.json"), serde_json::to_vec(&set)?)?;
    Ok(file)
}
fn stock_preview(fixture: &ContentRoot) -> Result<()> {
    let packages = bri_package::packages::PackageSet::load_root(&fixture.root)?;
    let directory = packages.role_dir(&fixture.root, "ui_pack")?;
    let path = directory.join("ui-pack.json");
    let mut data: bri_ui::schema::UiPack = serde_json::from_slice(&std::fs::read(&path)?)?;
    let bytes = png(3, 2, |_, _| [70, 80, 90, 255])?;
    std::fs::write(directory.join("stock-preview.png"), &bytes)?;
    let id = "invented/stock-preview".to_string();
    data.images.insert(
        id.clone(),
        bri_ui::schema::ImageEntry {
            file: "stock-preview.png".into(),
            source: "invented stock.png".into(),
            sha256: sha256(&bytes),
            width: 3,
            height: 2,
        },
    );
    let placeholder = png(3, 2, |_, _| [100, 100, 100, 255])?;
    std::fs::write(directory.join("missing-preview.png"), &placeholder)?;
    data.images.insert(
        "base/data/missions/default".into(),
        bri_ui::schema::ImageEntry {
            file: "missing-preview.png".into(),
            source: "invented placeholder.png".into(),
            sha256: sha256(&placeholder),
            width: 3,
            height: 2,
        },
    );
    data.maps
        .iter_mut()
        .find(|m| m.mission == fixture.map.0.trim_start_matches("v20/"))
        .unwrap()
        .preview = Some(id);
    fn bitmap(c: &mut bri_ui::schema::Control) {
        if c.name.as_deref() == Some("SM_MapPreview") {
            c.bitmap = Some("base/data/missions/default".into());
        }
        for child in &mut c.children {
            bitmap(child);
        }
    }
    bitmap(data.layouts.get_mut("startMissionGui").unwrap());
    std::fs::write(path, serde_json::to_vec(&data)?)?;
    Ok(())
}
fn select(ui: &mut Ui, screen: ScreenId, list: &str, name: &str) -> Result<()> {
    let view = ui.screen(screen).context("screen")?.view();
    let node = view.id(list).context("list")?;
    let row = view
        .node(node)
        .state
        .items
        .iter()
        .position(|(text, _)| text == name)
        .context("map row")?;
    ui.screen_mut(screen)
        .unwrap()
        .view_mut()
        .reveal_row(node, row as i64);
    let n = ui.screen(screen).unwrap().view().node(node);
    let scale = ui.scale();
    let x = (n.rect.x + 6) as f32 * scale;
    let y = (n.rect.y + row as i32 * n.state.row_height + n.state.row_height / 2) as f32 * scale;
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
    ui.update(16);
    let v = ui.screen(screen).unwrap().view();
    ensure!(
        v.selected_text(node).as_deref() == Some(name),
        "Wrong selected map"
    );
    Ok(())
}
fn drawn(ui: &Ui, screen: ScreenId, control: &str, map: &MapInfo) -> Result<()> {
    let v = ui.screen(screen).context("screen")?.view();
    let n = v.node(v.id(control).context("preview control")?);
    let expected = match &map.preview {
        IconRef::Pack(id) => id.as_str(),
        IconRef::None => "base/data/missions/default",
        _ => anyhow::bail!("Unexpected external preview"),
    };
    ensure!(n.state.bitmap.as_deref().or(n.ctrl.bitmap.as_deref()) == Some(expected));
    let draw = ui.draw();
    ensure!(draw.cmds.iter().any(|cmd|matches!(cmd,DrawCmd::Image{tex:TexKey::Image(id),dst,..} if id==expected && *dst==[n.rect.x as f32,n.rect.y as f32,n.rect.w as f32,n.rect.h as f32])),"Preview must reach actual draw quad: {screen:?} {} tex={expected} rect={:?} matching={:?}",map.id,n.rect,draw.cmds.iter().filter(|cmd|matches!(cmd,DrawCmd::Image{tex:TexKey::Image(id),..} if id==expected)).collect::<Vec<_>>());
    Ok(())
}

#[test]
fn addon_previews_decode_under_distinct_identities_and_draw_in_both_menus() -> Result<()> {
    let fixture = ContentRoot::synthetic()?;
    stock_preview(&fixture)?;
    add_map(
        &fixture.root,
        "first-map",
        "AAA first",
        Some([180, 20, 40, 255]),
    )?;
    add_map(
        &fixture.root,
        "second-map",
        "BBB second",
        Some([20, 160, 90, 255]),
    )?;
    add_map(&fixture.root, "missing-map", "ZZZ missing", None)?;
    let content = ClientContent::load(&fixture.root)?;
    let ids = [
        "first-map:map/main",
        "second-map:map/main",
        fixture.map.0.as_str(),
        "missing-map:map/main",
    ];
    let maps: Vec<_> = ids
        .iter()
        .map(|id| content.maps.iter().find(|m| m.id == *id).unwrap().clone())
        .collect();
    let mut image_ids = std::collections::BTreeSet::new();
    for (map, color) in
        maps.iter()
            .take(3)
            .zip([[180, 20, 40, 255], [20, 160, 90, 255], [70, 80, 90, 255]])
    {
        let IconRef::Pack(id) = &map.preview else {
            anyhow::bail!("Missing preview for {}", map.id)
        };
        ensure!(
            image_ids.insert(id.clone()),
            "Map previews must not alias by basename"
        );
        let pixels = content
            .ui_pack
            .pixels(&TexKey::Image(id.clone()))
            .context("Decoded image")?;
        ensure!((pixels.width, pixels.height) == (3, 2) && pixels.rgba[..4] == color);
    }
    ensure!(maps[3].preview == IconRef::None);
    let mut ui = Ui::new(
        content.ui_pack.clone(),
        UiConfig {
            size: (800, 600),
            scale: Some(1.),
            platform: Platform::Windows,
        },
        Settings::default(),
    );
    ui.core.pop(ScreenId::DefaultControls);
    ui.apply(UiUpdate::Maps(maps.clone()));
    ui.core.push(ScreenId::StartMission);
    ui.update(16);
    for map in &maps {
        select(&mut ui, ScreenId::StartMission, "SM_missionList", &map.name)?;
        drawn(&ui, ScreenId::StartMission, "SM_MapPreview", map)?;
    }
    ui.core.pop(ScreenId::StartMission);
    ui.update(16);
    ui.apply(UiUpdate::Admin(AdminUpdate::State(AdminSnapshot {
        revision: 1,
        role: AdminRole::SuperAdmin,
        local_host: true,
        legacy_lan: false,
        supported: [AdminFeature::Maps].into(),
        voice_muted: Default::default(),
        players: vec![],
        options: None,
    })));
    let request = ui
        .core
        .admin_request(AdminAction::RequestMaps)
        .context("Map request")?;
    ui.apply(UiUpdate::Admin(AdminUpdate::Maps {
        request,
        revision: 1,
        rows: maps
            .iter()
            .map(|m| AdminMap {
                id: m.id.clone(),
                name: m.name.clone(),
            })
            .collect(),
    }));
    ui.core.push(ScreenId::AdminMaps);
    ui.update(16);
    for map in &maps {
        select(&mut ui, ScreenId::AdminMaps, "changeMapList", &map.name)?;
        ensure!(ui.core.admin.selected_map.as_deref() == Some(map.id.as_str()));
        drawn(&ui, ScreenId::AdminMaps, "NativeMapPreview", map)?;
    }
    Ok(())
}

#[test]
fn unsafe_missing_corrupt_and_oversized_optional_preview_does_not_remove_map() -> Result<()> {
    for case in 0..11 {
        let fixture = ContentRoot::synthetic()?;
        let file = add_map(
            &fixture.root,
            "first-map",
            "First",
            Some([180, 20, 40, 255]),
        )?;
        add_map(
            &fixture.root,
            "second-map",
            "Second",
            Some([20, 160, 90, 255]),
        )?;
        let directory = file.parent().unwrap();
        let mut bundle: Value = serde_json::from_slice(&std::fs::read(&file)?)?;
        match case {
            0 => bundle["maps"][0]["preview"]["file"] = json!("../outside.png"),
            1 => {
                bundle["maps"][0]["preview"]["file"] =
                    json!(fixture.root.join("outside.png").to_string_lossy())
            }
            2 => bundle["maps"][0]["preview"]["file"] = json!("missing.png"),
            3..=5 => {
                let bytes = match case {
                    3 => b"corrupt image".to_vec(),
                    4 => b"GIF89a unsupported".to_vec(),
                    _ => vec![0; 4 * 1024 * 1024 + 1],
                };
                std::fs::write(directory.join("picture.png"), &bytes)?;
                bundle["maps"][0]["preview"]["sha256"] = json!(sha256(&bytes));
            }
            6 => bundle["maps"][0]["preview"]["width"] = json!(8193),
            7 => bundle["maps"][0]["preview"]["width"] = json!(4),
            8 => bundle["maps"][0]["preview"]["sha256"] = json!("0".repeat(64)),
            9 => bundle["maps"][0]["preview"] = json!("malformed metadata"),
            _ => {
                std::fs::copy(directory.join("picture.png"), directory.join("wrong.jpg"))?;
                bundle["maps"][0]["preview"]["file"] = json!("wrong.jpg");
            }
        }
        std::fs::write(
            directory.parent().unwrap().join("outside.png"),
            png(3, 2, |_, _| [1, 2, 3, 255])?,
        )?;
        std::fs::write(&file, serde_json::to_vec(&bundle)?)?;
        let content = ClientContent::load(&fixture.root)?;
        ensure!(
            content
                .maps
                .iter()
                .find(|m| m.id == "first-map:map/main")
                .context("Map must remain")?
                .preview
                == IconRef::None,
            "case {case}"
        );
        ensure!(
            matches!(
                content
                    .maps
                    .iter()
                    .find(|m| m.id == "second-map:map/main")
                    .unwrap()
                    .preview,
                IconRef::Pack(_)
            ),
            "Other maps must remain healthy, case {case}"
        );
    }
    Ok(())
}
