//! The dedicated server's session setup from a content root, shared by
//! `bri-server` and headless tests so both host the same game.
use crate::{
    content_identity,
    host_setup::{HostSetup, HostedAddOns, SessionContent},
    map_content::MapContent,
};
use anyhow::{Context, Result};
use bri_package::{environment::Environment, packages::PackageSet};
use bri_sim::session::{Session, ToolCatalog};
use glam::Vec3;
use std::path::Path;

/// Music-brick loop ids declared by the audio pack.
fn audio_music(audio: &std::path::Path) -> Result<Vec<String>> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(audio.join("manifest.json"))?)?;
    Ok(manifest["triggers"]
        .as_array()
        .context("Missing audio triggers")?
        .iter()
        .filter(|t| {
            t["key"]
                .as_str()
                .is_some_and(|k| k.starts_with("music-brick:"))
        })
        .filter_map(|t| t["sound"].as_str().map(str::to_string))
        .collect())
}
fn audio_event_sounds(audio: &std::path::Path) -> Result<Vec<String>> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(audio.join("manifest.json"))?)?;
    Ok(manifest["sounds"]
        .as_array()
        .context("Missing audio sounds")?
        .iter()
        .filter(|s| {
            s["lists"]
                .as_array()
                .is_some_and(|l| l.iter().any(|v| v == "event-param:Sound"))
        })
        .filter_map(|s| s["id"].as_str().map(str::to_string))
        .collect())
}

/// A session ready to serve, and what the host reports about it.
pub struct Dedicated {
    pub session: Session,
    /// How this host sets up a session; also what keeps the Add-Ons' state
    /// when the session ends.
    pub setup: std::sync::Arc<HostSetup>,
    /// Every package this host loaded, hashed: what joiners must match.
    pub environment: Environment,
    pub spawn_points: Vec<Vec3>,
    pub tool_summary: serde_json::Value,
    pub unresolved_items: usize,
    pub pending_objects: Vec<String>,
    /// What merging other packages' content skipped, per kind.
    pub merge_notes: Vec<String>,
    /// What a match recording needs to set this host up again.
    start: Start,
}

/// The world and Add-On state a [`Dedicated`] session started from.
struct Start {
    world: bri_world::World,
    /// The map id the session was hosted on.
    map: String,
    package_save: Option<Vec<u8>>,
    map_palette: Vec<[f32; 4]>,
}

/// Loads the packages `content_root/packages.json` lists (the base game's
/// and the installed default Add-Ons when absent) and builds the session
/// for `world`.
pub fn load(content_root: &Path, world: bri_world::World) -> Result<Dedicated> {
    load_packages(content_root, &PackageSet::load_root(content_root)?, world)
}

/// [`load`] with an explicit package list.
pub fn load_packages(
    content_root: &Path,
    packages: &PackageSet,
    world: bri_world::World,
) -> Result<Dedicated> {
    packages.validate().into_result()?;
    let role = |role: &str| packages.role_dir(content_root, role);
    let catalog_dir = role("brick_catalog")?;
    let materials_dir = role("brick_materials")?;
    let effects_dir = role("effects")?;
    let avatar_dir = role("avatar")?;
    let audio_dir = role("audio")?;
    let weapons_dir = role("weapons")?;
    let item_presentation_dir = role("item_presentation")?;
    let vehicles_dir = role("vehicles")?;
    let events_dir = role("events")?;
    // Hash before loading simulation so missing/corrupt packages fail before a
    // socket is opened or server state is written.
    let environment = Environment::load(content_root, packages)?;
    // Other packages providing weapons merge onto the base pack.
    let weapon_extras = content_identity::kind_providers(content_root, packages, "weapons.json")?;
    let weapons = content_identity::WeaponContent::load_with(&weapons_dir, &weapon_extras)?;
    let item_physics = content_identity::ItemPhysicsContent::load_with(
        &item_presentation_dir,
        &weapons,
        &weapon_extras,
    )?;
    let mut vehicle_parts = Vec::new();
    for (dir, abs) in content_identity::kind_providers(content_root, packages, "vehicles.json")? {
        let part = bri_vehicles::Pack::load(abs.join("vehicles.json"))?;
        part.verify_assets(&abs)?;
        vehicle_parts.push((dir, part));
    }
    let (vehicle_pack, vehicle_notes) =
        bri_vehicles::Pack::load(vehicles_dir.join("vehicles.json"))?.merge(vehicle_parts);
    vehicle_pack.validate()?;
    let brick_extras = content_identity::brick_catalog_providers(content_root, packages)?;
    // Add-On bricks are named, printed and wrenched like base ones.
    let catalog = bri_sim::definitions::catalog_with(&catalog_dir, &brick_extras)?;
    let materials =
        serde_json::from_slice(&std::fs::read(materials_dir.join("brick-materials.json"))?)?;
    let effects = serde_json::from_slice(&std::fs::read(effects_dir.join("effects.json"))?)?;
    let mut tools = ToolCatalog::from_native(&catalog, &effects, &materials)?;
    tools.install_items(weapons.item_choices.iter().map(|(id, _)| id.clone()))?;
    tools.install_effects(
        weapons.emitter_choices.iter().map(|(id, _)| id.clone()),
        weapons.light_choices.iter().map(|(id, _)| id.clone()),
    )?;
    let tool_summary = serde_json::json!({"items":tools.items.len(),"prints":tools.prints.len(),"printable_definitions":tools.brick_print_aspects.len(),"lights":tools.lights.len(),"emitters":tools.emitters.len(),"default_print":tools.default_print});
    let maps = MapContent::from_root(content_root, packages, weapons.clone())?;
    // Change Map paints new worlds with the colors this one starts with.
    let palette = world.palette.clone();
    // Package worlds grow their ground when the session is built, so a
    // recording keeps the world as given.
    let start_world = world.clone();
    let map = maps.load(world)?;
    let merge_notes = weapons
        .pack
        .diagnostics
        .iter()
        .filter(|d| d.starts_with("merge: "))
        .cloned()
        .chain(vehicle_notes.into_iter().map(|n| format!("merge: {n}")))
        .collect();
    let bot_kinds = content_identity::bot_kinds(content_root, packages)?;
    // What a Vehicle Spawn brick may hold, as the game's own host offers it.
    tools.install_special(
        audio_music(&audio_dir)?,
        vehicle_pack
            .definitions
            .iter()
            .filter(|d| d.family.spawnable())
            .map(|d| d.id.clone())
            .chain(bot_kinds.iter().map(|k| k.id.clone())),
    )?;
    // Add-On scripts run as they do in a game the client hosts; one broken
    // Add-On is left out and reported.
    let (server, problems) =
        bri_package_runtime::Catalog::load_skipping(content_root, packages, true);
    for problem in problems {
        eprintln!("Add-On left out: {problem}");
    }
    let avatar: bri_content::avatar::Package =
        serde_json::from_slice(&std::fs::read(avatar_dir.join("avatar.json"))?)?;
    avatar.validate()?;
    // The Blockhead's mount points (`mountObject`), from its rig.
    let bytes = std::fs::read(avatar_dir.join(&avatar.rig))?;
    anyhow::ensure!(
        format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&bytes)) == avatar.rig_sha256,
        "Avatar rig checksum mismatch"
    );
    let rig: bri_content::avatar::Rig = serde_json::from_slice(&bytes)?;
    rig.validate()?;
    let body_mounts = bri_sim::session::shape_mount_points(&rig.shape);
    let setup = HostSetup {
        lan: false,
        content: SessionContent {
            tool_catalog: tools,
            weapon_pack: weapons.pack,
            item_bounds: item_physics.bounds,
            avatar_catalog: avatar,
            body_mounts,
            vehicle_pack,
            bot_kinds,
            event_catalog: bri_events::Catalog::load(events_dir.join("catalog.json"))?,
            event_sounds: audio_event_sounds(&audio_dir)?,
        },
        maps: maps.maps()?,
        settings: None,
        passwords: None,
        add_ons: (!server.packages.is_empty()).then(|| HostedAddOns {
            server: std::sync::Arc::new(server),
            mode: None,
            saves: None,
        }),
        load_map: Some(maps.loader(palette.clone())),
        copies: None,
        game_version: None,
        // `/botreload` reads the Add-Ons' bots.json again; a dedicated
        // server keeps no user overrides.
        bot_tuning: Some(bri_sim::session::BotTuning {
            reload: Some({
                let (root, packages) = (content_root.to_path_buf(), packages.clone());
                std::sync::Arc::new(move || content_identity::bot_kinds(&root, &packages))
            }),
            overrides: None,
        }),
        bot_overrides: None,
    };
    let hosted = setup.hosted(&map.simulation.state().map_id)?;
    let unresolved_items = map.unresolved_items;
    let pending_objects = map.pending_objects.clone();
    let package_save = setup.package_save(&hosted)?;
    let (session, spawn_points) =
        setup.session_with(&hosted, map.into_session(), package_save.as_deref())?;
    Ok(Dedicated {
        session,
        setup: std::sync::Arc::new(setup),
        environment,
        spawn_points,
        tool_summary,
        unresolved_items,
        pending_objects,
        merge_notes,
        start: Start {
            world: start_world,
            map: hosted.map.clone(),
            package_save,
            map_palette: palette,
        },
    })
}

impl Dedicated {
    /// How to set this host up again to replay a match it records; call it
    /// after [`Self::configure`].
    pub fn recipe(&self) -> Result<crate::replay::HostRecipe> {
        let hosted = self.setup.hosted(&self.start.map)?;
        Ok(self.setup.recipe(
            &hosted,
            self.start.world.clone(),
            self.start.package_save.clone(),
            self.start.map_palette.clone(),
        ))
    }

    /// Applies the owner's `server.json` before anyone joins: the admin
    /// passwords and Start Game's Advanced Config.
    pub fn configure(&mut self, config: &ServerConfig) -> Result<()> {
        self.session.set_server_settings(config.settings.clone())?;
        self.session.set_admin_passwords(
            config.admin_password.clone(),
            config.super_admin_password.clone(),
        )?;
        let setup = std::sync::Arc::get_mut(&mut self.setup)
            .context("The server's setup is already shared")?;
        setup.settings = Some(config.settings.clone());
        setup.passwords = Some((
            config.admin_password.clone(),
            config.super_admin_password.clone(),
        ));
        Ok(())
    }
}

/// A dedicated server's own settings, `server.json` in its state directory:
/// v20's `$Pref::Server::AdminPassword`, `SuperAdminPassword` and the rest
/// of Start Game's Advanced Config. An empty password turns that login off.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub schema_version: u32,
    pub admin_password: bri_admin::Secret,
    pub super_admin_password: bri_admin::Secret,
    pub settings: bri_admin::ServerSettings,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            admin_password: bri_admin::Secret::new(String::new()).expect("empty password"),
            super_admin_password: bri_admin::Secret::new(String::new()).expect("empty password"),
            settings: bri_admin::ServerSettings::default(),
        }
    }
}

impl ServerConfig {
    pub const FILE: &str = "server.json";

    /// Reads `state_dir/server.json`, first writing the defaults there when
    /// it is missing so the owner has a file to edit.
    pub fn load_or_create(state_dir: &Path) -> Result<Self> {
        let path = state_dir.join(Self::FILE);
        if !path.exists() {
            std::fs::create_dir_all(state_dir)?;
            std::fs::write(&path, serde_json::to_vec_pretty(&Self::default())?)?;
        }
        let bytes = std::fs::read(&path)?;
        anyhow::ensure!(bytes.len() <= 1 << 20, "{} is too large", path.display());
        let config: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("Reading {}", path.display()))?;
        anyhow::ensure!(
            config.schema_version == 1,
            "Unsupported {} schema",
            path.display()
        );
        config.admin_password.validate()?;
        config.super_admin_password.validate()?;
        config.settings.validate()?;
        Ok(config)
    }
}

/// A new empty world on the base map `name` (`slate`, or a full map id),
/// with the game's default paint palette, like choosing a map in Start Game.
pub fn blank_world(content_root: &Path, name: &str) -> Result<bri_world::World> {
    let packages = PackageSet::load_root(content_root)?;
    crate::map_content::blank_world(
        &packages.role_dir(content_root, "map_bundle")?,
        crate::map_content::ui_palette(&packages.role_dir(content_root, "ui_pack")?)?,
        name,
    )
}

/// The dedicated server's crash-recovery slot in its state folder
/// ([`crate::recovery`]).
pub fn recovery_path(state_dir: &Path) -> std::path::PathBuf {
    state_dir.join("recovery.json")
}

/// A recovery snapshot left by a run that did not stop cleanly (a crash, a
/// killed process, a power cut) becomes the world save of that run, the
/// one `resume` continues from, named by when it was written. Returns it.
pub fn adopt_recovery(state_dir: &Path) -> Result<Option<std::path::PathBuf>> {
    let slot = recovery_path(state_dir);
    let written = match std::fs::metadata(&slot) {
        Ok(meta) => meta.modified()?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let millis = written
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    // A shutdown save of the same millisecond is never replaced.
    let world = (millis..)
        .map(|m| state_dir.join(format!("world-{m}.json")))
        .find(|p| !p.exists())
        .context("No free world save name")?;
    std::fs::rename(&slot, &world)?;
    Ok(Some(world))
}

/// Save the stopped host's final world, with its mini-game, as a new
/// `world-<unix millis>.json` in `state_dir`, the newest world `resume`
/// continues from.
pub fn save_stopped(
    state_dir: &Path,
    report: &crate::server::ServerReport,
) -> Result<std::path::PathBuf> {
    let bytes = bri_world::build::encode(&report.saved_build()?)?;
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let path = state_dir.join(format!("world-{millis}.json"));
    bri_files::create_new(&path, &bytes)?;
    Ok(path)
}

/// What asks the server to stop: Ctrl+C; SIGTERM (a service manager,
/// `kill`, a container stopping) or SIGHUP (its terminal closed) on Unix;
/// the console window closing, the user logging off or Windows shutting
/// down on Windows. Watched from [`Shutdown::watch`] on, so a request that
/// comes while the server is still starting is not missed.
pub struct Shutdown {
    #[cfg(unix)]
    signals: [(tokio::signal::unix::Signal, &'static str); 3],
    #[cfg(windows)]
    signals: (
        tokio::signal::windows::CtrlC,
        tokio::sync::oneshot::Receiver<()>,
    ),
}
impl Shutdown {
    pub fn watch() -> Result<Self> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            Ok(Self {
                signals: [
                    (signal(SignalKind::interrupt())?, "Ctrl+C"),
                    (signal(SignalKind::terminate())?, "SIGTERM"),
                    (signal(SignalKind::hangup())?, "SIGHUP"),
                ],
            })
        }
        #[cfg(windows)]
        {
            Ok(Self {
                signals: (tokio::signal::windows::ctrl_c()?, console::install()?),
            })
        }
    }
    /// Resolves when the server is asked to stop, saying how.
    pub async fn requested(&mut self) -> &'static str {
        #[cfg(unix)]
        {
            let [(interrupt, a), (terminate, b), (hangup, c)] = &mut self.signals;
            tokio::select! {
                _ = interrupt.recv() => a,
                _ = terminate.recv() => b,
                _ = hangup.recv() => c,
            }
        }
        #[cfg(windows)]
        {
            let (ctrl_c, closing) = &mut self.signals;
            tokio::select! {
                _ = ctrl_c.recv() => "Ctrl+C",
                _ = closing => "the console closing",
            }
        }
    }
}

/// Call once the world is saved after [`Shutdown::requested`]: Windows
/// ends a closing console's process as soon as its close handler returns,
/// so the handler waits for this (up to the few seconds Windows allows).
pub fn shutdown_saved() {
    #[cfg(windows)]
    console::saved();
}

#[cfg(windows)]
mod console {
    use std::sync::{Condvar, Mutex, OnceLock};
    use windows_sys::Win32::System::Console::{
        CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT, SetConsoleCtrlHandler,
    };
    struct State {
        notify: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
        saved: (Mutex<bool>, Condvar),
    }
    static STATE: OnceLock<State> = OnceLock::new();
    /// Windows runs this on a thread of its own and ends the process when it
    /// returns, so it waits for the save.
    unsafe extern "system" fn handler(event: u32) -> windows_sys::core::BOOL {
        if ![CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT].contains(&event) {
            return 0; // Ctrl+C and Ctrl+Break go to tokio's handler.
        }
        let Some(state) = STATE.get() else { return 0 };
        if let Some(notify) = state
            .notify
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = notify.send(());
        }
        let (lock, done) = &state.saved;
        let saved = lock.lock().unwrap_or_else(|e| e.into_inner());
        let _ = done.wait_timeout_while(saved, std::time::Duration::from_secs(30), |s| !*s);
        1
    }
    pub(super) fn install() -> anyhow::Result<tokio::sync::oneshot::Receiver<()>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let state = STATE.get_or_init(|| State {
            notify: Mutex::new(None),
            saved: (Mutex::new(false), Condvar::new()),
        });
        *state.notify.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        // SAFETY: `handler` is a plain function that stays valid for the
        // life of the process.
        anyhow::ensure!(
            unsafe { SetConsoleCtrlHandler(Some(handler), 1) } != 0,
            "Could not watch for the console closing"
        );
        Ok(rx)
    }
    pub(super) fn saved() {
        if let Some(state) = STATE.get() {
            let (lock, done) = &state.saved;
            *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
            done.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Admin menu's Change Map works on a dedicated server as it does
    /// in a game the client hosts: it lists the maps and loads one.
    #[test]
    fn a_dedicated_server_changes_maps() -> Result<()> {
        use bri_sim::map::LOADABLE_MAPS;
        let scratch = bri_content::testing::ScratchDir::new("dedicated-change-map")?;
        let (slate, bedroom) = (LOADABLE_MAPS[3], LOADABLE_MAPS[0]);
        crate::testing::write_root(scratch.path(), &[slate, bedroom])?;
        let set = PackageSet::load_root(scratch.path())?;
        let world = crate::map_content::blank_world(
            &set.role_dir(scratch.path(), "map_bundle")?,
            vec![[1.0; 4]],
            "slate",
        )?;
        let host = load_packages(scratch.path(), &set, world)?;
        let listed: Vec<_> = host.setup.maps.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(listed, [slate, bedroom]);
        let next = crate::server::MapHost::load(&*host.setup, bedroom)?;
        assert_eq!(next.simulation().state().map_id, bedroom);
        assert_eq!(next.simulation().state().palette, vec![[1.0; 4]]);
        Ok(())
    }

    #[test]
    fn server_json_is_written_with_defaults_then_read_back() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let first = ServerConfig::load_or_create(dir.path())?;
        assert!(dir.path().join(ServerConfig::FILE).is_file());
        assert_eq!(first.settings, bri_admin::ServerSettings::default());
        assert_eq!(first.super_admin_password.expose(), "");
        let mut edited = first.clone();
        edited.super_admin_password = bri_admin::Secret::new("hunter2".into())?;
        edited.settings.name = "Friend's VPS".into();
        edited.settings.max_players = 16;
        std::fs::write(
            dir.path().join(ServerConfig::FILE),
            serde_json::to_vec(&edited)?,
        )?;
        let read = ServerConfig::load_or_create(dir.path())?;
        assert_eq!(read.super_admin_password.expose(), "hunter2");
        assert_eq!(read.settings.max_players, 16);
        std::fs::write(
            dir.path().join(ServerConfig::FILE),
            br#"{"schema_version":1}"#,
        )?;
        assert!(ServerConfig::load_or_create(dir.path()).is_err());
        Ok(())
    }
}
