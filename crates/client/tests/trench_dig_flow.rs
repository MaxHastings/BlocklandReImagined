//! Trench Digging as a player gets it: a stand-in for the original (its
//! dirt bricks and tools, CC0) imported into the content root the way the
//! game's Import Add-On installs it, beside its host rules, with
//! `packages.json` naming only the Add-On, as a list written before its
//! rules were installed does (Max's v0.1.11 test build). The host loads the
//! content and its Add-Ons the way Start Game does, and in the host's
//! mini-game the Trench Shovel digs a piece out of the dirt brick it hits.
//! No window, GPU or network.
use anyhow::{Context, Result};
use bri_client::content::ClientContent;
use bri_net::host_setup::{HostSetup, HostedAddOns, SessionContent};
use bri_package::packages::{PACKAGES_FILE, PackageEntry, PackageSet, Side};
use bri_sim::{
    player::MoveInput,
    session::{Command, MiniGameRequest, Reply, Session},
};
use bri_world::OwnerId;
use glam::Vec3;
use std::path::Path;

#[macro_use]
mod support;
use support::content_root::ContentRoot;

const NS: &str = "gamemode_trenchdigging";
const RULES: &str = "gamemode_trenchdigging-rules";
const SHOVEL: &str = "gamemode_trenchdigging:weapon/trenchshovelitem";
const DIRT: &str = "gamemode_trenchdigging:weapon/trenchdirtitem";

/// The importer's port fixture (the scripts the port reads) with the dirt
/// cubes the shovel splits an 8x cube into: `n`x Cube Dirt is `n` studs a
/// side and as tall.
fn stand_in(dir: &Path) -> Result<()> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../addon-import/tests/fixtures/ports/Gamemode_TrenchDigging");
    std::fs::create_dir_all(dir.join("Bricks"))?;
    for file in ["description.txt", "LICENSE.txt"] {
        std::fs::copy(fixture.join(file), dir.join(file))?;
    }
    let mut server = std::fs::read_to_string(fixture.join("server.cs"))?;
    for n in [2u32, 4, 8] {
        let file = format!("{n}x Cube Dirt.blb");
        std::fs::write(
            dir.join("Bricks").join(&file),
            format!("{n} {n} {}\nBRICK\n", n * 5 / 2),
        )?;
        server.push_str(&format!(
            "\ndatablock fxDTSBrickData(brick{n}xCubeDirtData)\n{{\n   brickFile = \"./Bricks/{file}\";\n   \
             category = \"Dirt\";\n   subCategory = \"Cube\";\n   uiName = \"{n}x Cube Dirt\";\n   isTrenchDirt = 1;\n}};\n"
        ));
    }
    std::fs::write(dir.join("server.cs"), server)?;
    Ok(())
}

struct Host {
    s: Session,
    seq: u64,
    moves: u64,
    look: MoveInput,
}
impl Host {
    fn cmd(&mut self, owner: OwnerId, command: Command) -> Result<Reply> {
        self.seq += 1;
        self.s.command(owner, self.seq, command)
    }
    fn steps(&mut self, owner: OwnerId, n: usize) -> Result<()> {
        for _ in 0..n {
            self.moves += 1;
            let _ = self.s.movement(owner, self.moves, self.look);
            self.s.step()?;
        }
        Ok(())
    }
    fn until(&mut self, owner: OwnerId, state: &str) -> Result<()> {
        for _ in 0..600 {
            let images = self.s.weapon_view().images.get(&owner).cloned();
            if images.is_some_and(|i| i.iter().any(|i| i.hand == 0 && i.state == state)) {
                return Ok(());
            }
            self.steps(owner, 1)?;
        }
        anyhow::bail!("the shovel never reached {state}")
    }
    fn dirt(&self, owner: OwnerId) -> Option<i64> {
        self.s
            .package_value(RULES, owner, "dirt")
            .and_then(|v| v.as_i64())
    }
}

#[test]
fn the_trench_shovel_digs_dirt_in_a_mini_game_once_the_add_on_is_on() -> Result<()> {
    let f = ContentRoot::synthetic()?;
    let root = &f.root;
    // Import Add-On: the import at addons/<id>, its rules beside it.
    let source = tempfile::tempdir()?;
    let input = source.path().join("Gamemode_TrenchDigging");
    stand_in(&input)?;
    let report = bri_addon_import::import(&bri_addon_import::Options {
        input,
        out: root.join("addons").join(NS),
        installed: Some(root.clone()),
        ..Default::default()
    })?;
    assert!(report.ports.iter().any(|p| p.applied), "{:?}", report.ports);
    assert!(
        root.join("addons")
            .join(RULES)
            .join("package.json")
            .is_file()
    );
    // Turned on in the Add-Ons list before its rules were there: the list
    // names the Add-On alone.
    let mut list = PackageSet::load_root(root)?;
    list.packages.retain(|p| p.id != RULES);
    list.packages.push(PackageEntry {
        id: NS.into(),
        version: "1.0.0".into(),
        side: Side::Shared,
        dir: format!("addons/{NS}"),
        role: None,
    });
    std::fs::write(root.join(PACKAGES_FILE), serde_json::to_vec_pretty(&list)?)?;

    // Start Game: the content and the Add-Ons a host runs.
    let content = ClientContent::load(root)?;
    let dirt_brick = content
        .bricks
        .iter()
        .find(|b| b.ui_name == "8x Cube Dirt")
        .context("8x Cube Dirt in the brick menu")?;
    assert_eq!(dirt_brick.category, "Dirt");
    let kind = dirt_brick.id.clone();
    let (server, problems) = bri_client::packages::load_server(root);
    let server = server.context("no Add-Ons run")?;
    assert!(problems.is_empty(), "{problems:?}");
    assert!(
        server.packages.contains_key(RULES),
        "the rules run with the Add-On"
    );
    let map = &f.map.0;
    let avatar = bri_client::avatar::AvatarAssets::load(&content.paths.avatar)?;
    let setup = HostSetup {
        lan: true,
        content: SessionContent {
            tool_catalog: Default::default(),
            weapon_pack: content.weapons.pack.clone(),
            item_bounds: content.item_physics.bounds.clone(),
            body_mounts: bri_sim::session::shape_mount_points(&avatar.rig.shape),
            avatar_catalog: avatar.package,
            vehicle_pack: content.paths.vehicle_pack()?,
            bot_kinds: vec![],
            event_catalog: content.events.clone(),
            event_sounds: vec![],
        },
        maps: vec![],
        settings: None,
        passwords: None,
        add_ons: Some(HostedAddOns {
            server,
            mode: None,
            saves: None,
        }),
        load_map: None,
        copies: None,
        game_version: None,
    };
    let hosted = setup.hosted(map)?;
    let (s, spawn) = setup.session(&hosted, content.paths.load_map(map, None)?.into_session())?;
    let mut h = Host {
        s,
        seq: 0,
        moves: 0,
        look: MoveInput::default(),
    };
    let start = spawn.first().copied().unwrap_or(Vec3::ZERO);
    let me = h.s.join("Max".into(), start, true)?;
    // Landed on the floor.
    h.steps(me, 120)?;
    let feet = Vec3::from(
        h.s.snapshot()
            .players
            .iter()
            .find(|p| p.owner == me)
            .context("player")?
            .feet,
    );
    // An 8x cube of dirt (4 units a side) on the floor, its near face 1.5
    // units in front (yaw 0 looks along -z).
    let centre = [
        (feet.x * 2.0).round() / 2.0,
        (feet.y / 0.2).floor() * 0.2 + 2.0,
        (feet.z * 2.0).round() / 2.0 - 3.5,
    ];
    match h.cmd(
        me,
        Command::Plant {
            definition: kind.clone(),
            position: centre,
            quarter_turns: 0,
            color: 1,
        },
    )? {
        Reply::Planted(_) => {}
        other => anyhow::bail!("planting the dirt: {other:?}"),
    }
    // The host's mini-game hands out the shovel; joining it respawns the
    // host, here, in front of the dirt.
    h.s.set_spawn_points(vec![feet])?;
    let settings = bri_minigames::Settings {
        loadout: [Some(SHOVEL.into()), Some(DIRT.into()), None, None, None],
        ..Default::default()
    };
    h.cmd(
        me,
        Command::MiniGame(MiniGameRequest::Create { color: 1, settings }),
    )?;
    h.steps(me, 30)?;
    let tools = h.s.tool_inventories()[&me].clone();
    let slot = tools
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(SHOVEL))
        .context("the shovel in the loadout")?;
    // Looking straight ahead at the dirt.
    h.look = MoveInput::default();
    h.steps(me, 10)?;
    let before = h.s.snapshot().world.bricks.len();
    h.cmd(me, Command::EquipTool { slot: Some(slot) })?;
    h.until(me, "Ready")?;
    h.cmd(me, Command::WeaponTrigger { down: true })?;
    h.until(me, "Fire")?;
    h.s.release_trigger(me)?;
    h.until(me, "Ready")?;
    assert!(
        h.s.package_diagnostics().is_empty(),
        "{:#?}",
        h.s.package_diagnostics()
    );
    assert_eq!(h.dirt(me), Some(1), "a piece of dirt in the pocket");
    // The 8x cube split into 4x cubes, the nearest of them into 2x cubes,
    // one of which was taken: 7 + 7.
    assert_eq!(h.s.snapshot().world.bricks.len(), before - 1 + 14);
    Ok(())
}
