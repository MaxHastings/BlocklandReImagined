//! A whole made-up content root, as `App::load` and `ClientContent::load`
//! read it: `bri_net::testing`'s server packages, and on top of them the
//! client's own (UI, runtime effects, weather, foliage, weapon debris,
//! tutorial targets) and the client's fuller versions of the shared ones
//! (item icons pictured from their models, the effects showcase, the
//! audio pack with its clips), all named by one `packages.json`.
//!
//! [`ContentRoot`] is that root or the generated v20 one, for tests that
//! run on both (`synthetic_and_content!`). Every value is invented.
use super::{ScratchDir, png, sha256, write_file};
use crate::content::LOADABLE_MAPS;
use anyhow::Result;
use bri_net::testing as server;
use serde_json::json;
use std::path::{Path, PathBuf};

pub use bri_net::testing::MENU_BRICKS;

/// Every base role, in the base package list's order.
pub const ROLES: [&str; 18] = [
    "map_bundle",
    "brick_catalog",
    "geometry",
    "effects",
    "brick_materials",
    "avatar",
    "audio",
    "weapons",
    "item_presentation",
    "vehicles",
    "events",
    "ui_pack",
    "effects_runtime",
    "weather",
    "foliage",
    "weapon_debris",
    "worlds",
    "tutorial",
];

/// A content root a test runs on: the generated v20 content, or the
/// made-up one written into a scratch folder.
pub struct ContentRoot {
    pub root: PathBuf,
    /// Whether this is the generated v20 content.
    pub content: bool,
    /// A 2x2 brick the brick menu offers.
    pub brick: String,
    _scratch: Option<ScratchDir>,
}

impl ContentRoot {
    /// The generated v20 content: BRI_CONTENT, else the workspace `content/`.
    pub fn content() -> Result<Self> {
        Ok(Self {
            root: std::env::var_os("BRI_CONTENT").map_or_else(
                || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
                PathBuf::from,
            ),
            content: true,
            brick: "v20/brick/brick2x2data".into(),
            _scratch: None,
        })
    }

    /// The made-up root, written by [`write_root`].
    pub fn synthetic() -> Result<Self> {
        let dir = ScratchDir::new("content-root")?;
        write_root(dir.path())?;
        Ok(Self {
            root: dir.path().to_path_buf(),
            content: false,
            brick: MENU_BRICKS[2].0.into(),
            _scratch: Some(dir),
        })
    }

    /// A fresh, empty client state folder for an `App` on this root,
    /// removed when dropped.
    pub fn state(&self) -> Result<ScratchDir> {
        ScratchDir::new("client-state")
    }

    /// The folder for a test's regenerated evidence: `artifacts/<name>` on
    /// the generated content, a temporary folder on the made-up root.
    pub fn out(&self, name: &str) -> Result<PathBuf> {
        let out = if self.content {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../artifacts")
                .join(name)
        } else {
            std::env::temp_dir()
                .join("bri-client-tests")
                .join(format!("{name}-synthetic"))
        };
        std::fs::create_dir_all(&out)?;
        Ok(out)
    }
}

/// Write the whole made-up root into `root`: the server's packages (one
/// lit room per map in [`LOADABLE_MAPS`]), the client's, and a
/// `packages.json` naming all of them.
pub fn write_root(root: &Path) -> Result<()> {
    let written = server::write_root(root, LOADABLE_MAPS)?;
    let dir = |role: &str| server::role_dir(root, role);
    super::items::write(&dir("item_presentation")?, &dir("weapons")?)?;
    write_effects(&dir("effects")?)?;
    super::audio::write_pack(&dir("audio")?)?;
    let icons: Vec<String> = written
        .brick_icons
        .iter()
        .map(|(_, icon)| icon.clone())
        .chain(written.print_icons.iter().cloned())
        .collect();
    write_ui(&dir("ui_pack")?, &icons)?;
    write_effects_runtime(&dir("effects_runtime")?)?;
    write_weather(&dir("weather")?)?;
    bri_foliage::testing::write_pack(&dir("foliage")?)?;
    super::weapon_debris::write(&dir("weapon_debris")?)?;
    super::tutorial::write_targets(&dir("tutorial")?)?;
    server::write_packages(root, &ROLES)?;
    Ok(())
}

/// The effects catalog: `bri_fx_runtime::testing`'s showcase library, its
/// one texture written beside it.
fn write_effects(dir: &Path) -> Result<()> {
    let library = effects_library();
    for file in library.textures.values() {
        write_file(dir, file, &png(1, 1, |_, _| [120, 80, 20, 255])?)?;
    }
    write_file(dir, "effects.json", &serde_json::to_vec(&library)?)?;
    Ok(())
}

/// A finite emitter that emits at once and often ([`effects_library`]).
pub const FLASH_EMITTER: &str = "flash";

/// `bri_fx_runtime::testing`'s showcase library and a short, quick
/// [`FLASH_EMITTER`] of its sparks.
pub fn effects_library() -> bri_content::effects::Library {
    let mut library = bri_fx_runtime::testing::showcase_pack().library.clone();
    let mut flash = bri_fx_runtime::testing::emitter(FLASH_EMITTER, "Flash", &["spark"]);
    flash.period = 0.02;
    flash.lifetime = 0.4;
    library.emitters.push(flash);
    library
}

/// The runtime effects pack: the same library with its manifest (texture
/// records, composites) and checksums.
fn write_effects_runtime(dir: &Path) -> Result<()> {
    let pack = bri_fx_runtime::testing::showcase_pack();
    let effects = effects_library();
    let library = serde_json::to_vec(&effects)?;
    let mut manifest = pack.manifest.clone();
    manifest.library_sha256 = sha256(&library);
    manifest.textures.clear();
    for (id, file) in &effects.textures {
        let bytes = png(1, 1, |_, _| [120, 80, 20, 255])?;
        manifest.textures.insert(
            id.clone(),
            bri_fx_runtime::pack::TextureRecord {
                file: file.clone(),
                sha256: write_file(dir, file, &bytes)?,
                width: 1,
                height: 1,
            },
        );
    }
    write_file(dir, "effects.json", &library)?;
    write_file(dir, "manifest.json", &serde_json::to_vec(&manifest)?)?;
    Ok(())
}

/// No precipitation anywhere.
fn write_weather(dir: &Path) -> Result<()> {
    write_file(
        dir,
        "weather.json",
        &serde_json::to_vec(&json!({
            "schema_version": 1, "legacy_tick_seconds": 0.032, "definitions": [],
            "placements": [], "textures": {}, "sources": [], "assumptions": [],
        }))?,
    )?;
    Ok(())
}

/// v20-shaped screen layouts with the control names the screens look up:
/// the Start Game window (`startMissionGui`) with its map list.
fn add_screens(data: &mut bri_ui::schema::UiPack) {
    use bri_ui::geom::Rect;
    use bri_ui::{
        screens::ctrl,
        testing::{dialog, label, named, text_button},
    };
    data.layouts.insert(
        "startMissionGui".into(),
        dialog(
            "SM_Window",
            "Start Game",
            Rect::new(20, 10, 600, 460),
            vec![
                named(
                    ctrl(
                        "GuiTextListCtrl",
                        "GuiTextListProfile",
                        Rect::new(10, 30, 260, 400),
                    ),
                    "SM_missionList",
                ),
                named(
                    ctrl("GuiTextCtrl", "GuiTextProfile", Rect::new(290, 30, 290, 18)),
                    "SM_MapName",
                ),
                label(Rect::new(290, 60, 120, 18), "Players:"),
                named(
                    ctrl(
                        "GuiPopUpMenuCtrl",
                        "GuiPopUpMenuProfile",
                        Rect::new(420, 60, 80, 20),
                    ),
                    "SM_PlayerCountMenu",
                ),
                text_button(
                    Rect::new(480, 420, 100, 28),
                    "Start",
                    "startMissionGui.startMission();",
                ),
            ],
        ),
    );
}

/// The UI pack: `bri_ui::testing`'s fonts, profiles and dialogs, an icon
/// for every brick in the menu, a two-division paint palette, an avatar
/// colour set, a key binding for every action the tests press, and an
/// entry for every map, each image and font sheet written as a file.
fn write_ui(dir: &Path, icons: &[String]) -> Result<()> {
    use bri_ui::schema::*;
    let mut data = UiPack {
        schema_version: PACK_SCHEMA_VERSION,
        generator: "fixture".into(),
        ..Default::default()
    };
    bri_ui::testing::add_dialogs(&mut data);
    bri_ui::testing::add_assets(&mut data);
    add_screens(&mut data);
    for icon in icons {
        bri_ui::testing::add_image(&mut data, icon, 32, 32);
    }
    data.data.brick_colorset = vec![
        ColorDivision {
            name: "Solid".into(),
            colors: vec![
                [0.8, 0.2, 0.2, 1.0],
                [0.2, 0.6, 0.3, 1.0],
                [0.2, 0.3, 0.8, 1.0],
                [0.9, 0.9, 0.9, 1.0],
                [0.1, 0.1, 0.1, 1.0],
            ],
        },
        ColorDivision {
            name: "Clear".into(),
            colors: vec![[0.6, 0.8, 1.0, 0.5], [1.0, 0.9, 0.3, 0.6]],
        },
    ];
    // Favorite set 1, bought on first spawn: every menu brick.
    data.data.favorites = [(1, MENU_BRICKS.map(|(_, name, ..)| name.to_string()).into())].into();
    data.data.avatar_colors = vec![
        [0.9, 0.8, 0.3, 1.0],
        [0.2, 0.4, 0.7, 1.0],
        [0.1, 0.1, 0.1, 1.0],
        [0.8, 0.1, 0.1, 1.0],
    ];
    let key = |key: &str, command: &str| DefaultBind {
        device: Device::Keyboard,
        key: key.into(),
        command: command.into(),
        when: vec![],
        source_line: 0,
    };
    let mouse = |button: &str, command: &str| DefaultBind {
        device: Device::Mouse,
        ..key(button, command)
    };
    let slots = [
        "useFirstSlot",
        "useSecondSlot",
        "useThirdSlot",
        "useFourthSlot",
        "useFifthSlot",
        "useSixthSlot",
        "useSeventhSlot",
        "useEighthSlot",
        "useNinthSlot",
        "useTenthSlot",
    ];
    let mut binds = vec![
        key("w", "moveforward"),
        key("s", "movebackward"),
        key("a", "moveleft"),
        key("d", "moveright"),
        key("space", "jump"),
        key("lshift", "crouch"),
        key("b", "openBSD"),
        key("q", "useTools"),
        key("e", "useSprayCan"),
        key("t", "GlobalChat"),
        key("y", "TeamChat"),
        key("tab", "toggleFirstPerson"),
        key("f", "useLight"),
        key("ctrl z", "undoBrick"),
        key("ctrl k", "suicide"),
        key("numpad8", "shiftBrickAway"),
        key("numpad2", "shiftBrickTowards"),
        key("numpad4", "shiftBrickLeft"),
        key("numpad6", "shiftBrickRight"),
        key("numpad5", "shiftBrickUp"),
        key("numpad0", "shiftBrickDown"),
        key("numpad9", "rotateBrickCW"),
        key("numpad7", "rotateBrickCCW"),
        key("numpadenter", "plantBrick"),
        key("numpaddecimal", "cancelBrick"),
        mouse("button0", "mouseFire"),
        mouse("button1", "Jet"),
        mouse("zaxis", "scrollInventory"),
    ];
    for (i, command) in slots.iter().enumerate() {
        binds.push(key(&((i + 1) % 10).to_string(), command));
    }
    data.data.default_binds = binds;
    data.data.global_binds = vec![key("tilde", "toggleConsole")];
    data.data.remap = data
        .data
        .default_binds
        .iter()
        .map(|b| RemapEntry {
            division: None,
            name: b.command.clone(),
            command: b.command.clone(),
        })
        .collect();
    data.maps = LOADABLE_MAPS
        .iter()
        .enumerate()
        .map(|(i, id)| MapEntry {
            mission: id.strip_prefix("v20/").unwrap_or(id).into(),
            display_name: format!("Fixture Room {}", i + 1),
            description: format!("A made-up room, number {}.", i + 1),
            preview: None,
        })
        .collect();
    // Every image and font sheet as a file the pack names.
    for (id, image) in data.images.iter_mut() {
        let pixels = bri_ui::testing::image_pixels(id, image.width, image.height);
        let bytes = png(pixels.width, pixels.height, |x, y| {
            let i = ((y * pixels.width + x) * 4) as usize;
            [
                pixels.rgba[i],
                pixels.rgba[i + 1],
                pixels.rgba[i + 2],
                pixels.rgba[i + 3],
            ]
        })?;
        image.sha256 = write_file(dir, &image.file, &bytes)?;
    }
    for font in data.fonts.values() {
        for (i, sheet) in font.sheets.iter().enumerate() {
            let pixels = bri_ui::testing::font_sheet(font, i as u16);
            let luma = image::GrayImage::from_fn(pixels.width, pixels.height, |x, y| {
                image::Luma([pixels.rgba[((y * pixels.width + x) * 4 + 3) as usize]])
            });
            let mut bytes = Vec::new();
            luma.write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )?;
            write_file(dir, sheet, &bytes)?;
        }
    }
    write_file(dir, "ui-pack.json", &serde_json::to_vec(&data)?)?;
    Ok(())
}
