//! How a host sets up the session for each map it plays: the one recipe
//! Start Game, Change Map and the dedicated server share, so no host path
//! can leave out content (the Blockhead Bot's kinds, the Add-On scripts) that
//! another path installs.
use anyhow::{Context, Result};
use bri_package_runtime::Catalog;
use bri_sim::{
    bot_kind::BotKind,
    map::Breakable,
    session::{CopyStore, MapListing, PackageSave, Session, ToolCatalog},
    simulation::Simulation,
    tutorial::TutorialMap,
};
use glam::Vec3;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

/// The content every session of a host is built from, whatever the map.
/// Every field is required: a new kind of content is added here once and
/// the compiler then asks every host for it.
#[derive(Clone)]
pub struct SessionContent {
    pub tool_catalog: ToolCatalog,
    pub weapon_pack: bri_weapons::Pack,
    pub item_bounds: BTreeMap<String, bri_weapons::ItemBounds>,
    pub avatar_catalog: bri_content::avatar::Package,
    /// The Blockhead's mount points (`mountObject`), from its rig.
    pub body_mounts: Vec<bri_sim::archetype::MountPoint>,
    pub vehicle_pack: bri_vehicles::Pack,
    pub bot_kinds: Vec<BotKind>,
    pub event_catalog: bri_events::Catalog,
    pub event_sounds: Vec<String>,
}

/// The Blockhead's model id (`m.dts`).
pub const BLOCKHEAD_MODEL: &str = "v20.shape.m";

/// One map loaded for play: its simulation and the rules its scene brings.
pub struct MapSession {
    pub simulation: Simulation,
    pub spawn_points: Vec<Vec3>,
    pub breakables: Vec<Breakable>,
    pub tutorial: Option<TutorialMap>,
}

/// Loads the map with the given id (its native bundle) for a new session.
pub type LoadMap = Arc<dyn Fn(&str) -> Result<MapSession> + Send + Sync>;

/// What a hosted game runs: the packages, the map players see chosen, the
/// base map it stands on and the key its package state is saved under.
#[derive(Debug)]
pub struct Hosted {
    pub catalog: Option<Arc<Catalog>>,
    pub map: String,
    pub base_map: String,
    pub save_key: String,
}

/// Resolve Start Game's choice of `map` and game `mode` (None: Custom).
/// Custom on a package world runs every enabled Add-On except those needing
/// another world; Custom on a base map runs the enabled Add-Ons that need
/// no package world and belong to no game mode (the Duplicator, say), as
/// v20 ran its enabled Add-Ons in every game. A mode runs its own Add-Ons,
/// on its own map when it names one.
pub fn hosted(server: Option<&Arc<Catalog>>, map: &str, mode: Option<&str>) -> Result<Hosted> {
    let problems = |p: Vec<bri_package::diag::Diagnostic>| {
        anyhow::anyhow!(
            p.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    let (catalog, map, save_key) = match (server, mode) {
        (None, Some(mode)) => anyhow::bail!("The game mode {mode} is not turned on in Add-Ons"),
        (None, None) => (None, map.to_owned(), map.to_owned()),
        (Some(server), Some(mode)) => {
            let def = server
                .modes()
                .find(|(id, _)| id.as_str() == mode)
                .map(|(_, m)| m.clone())
                .with_context(|| format!("The game mode {mode} is not turned on in Add-Ons"))?;
            let catalog = server.for_mode(mode).map_err(problems)?;
            let map = def.map.clone().unwrap_or_else(|| map.to_owned());
            if let Some((_, world, _)) = catalog.world() {
                anyhow::ensure!(*world == map, "{} plays on its own world", def.name);
            } else {
                anyhow::ensure!(
                    !map.contains(':'),
                    "{} does not bring that world; pick a map",
                    def.name
                );
            }
            let key = format!("{mode}-{map}");
            (Some(catalog), map, key)
        }
        (Some(server), None) if server.packages.values().any(|p| p.worlds.contains_key(map)) => (
            Some(server.for_world(map).map_err(problems)?),
            map.to_owned(),
            map.to_owned(),
        ),
        (Some(_), None) if map.contains(':') => {
            anyhow::bail!("No Add-On that is turned on provides {map}")
        }
        (Some(server), None) => (
            Some(server.for_base_map().map_err(problems)?),
            map.to_owned(),
            map.to_owned(),
        ),
    };
    // Nothing to run: host the plain base game.
    let catalog = catalog.filter(|c| c.world().is_some() || c.behaviours().next().is_some());
    let base_map = catalog
        .as_ref()
        .and_then(|c| c.world().map(|(_, _, w)| w.environment.clone()))
        .unwrap_or_else(|| map.clone());
    Ok(Hosted {
        catalog: catalog.map(Arc::new),
        map,
        base_map,
        save_key,
    })
}

/// The host's Add-On choice, applied to every map it plays.
#[derive(Clone)]
pub struct HostedAddOns {
    /// Every server-side package the host runs.
    pub server: Arc<Catalog>,
    /// Start Game's game mode; None is Custom.
    pub mode: Option<String>,
    /// Where each map's package state is kept between games; None keeps
    /// nothing.
    pub saves: Option<PathBuf>,
}

impl HostedAddOns {
    fn save_path(&self, save_key: &str) -> Option<PathBuf> {
        self.saves
            .as_ref()
            .map(|dir| dir.join(format!("{}.save.json", save_key.replace([':', '/'], "-"))))
    }
}

/// Everything a host installs in each map's session.
#[derive(Clone)]
pub struct HostSetup {
    /// v20 `$Server::LAN`: single-player and LAN hosts keep the looser
    /// brick-damage rule; internet hosts use miniGameCanDamage.
    pub lan: bool,
    pub content: SessionContent,
    /// Admin Change Map choices; empty offers none.
    pub maps: Vec<MapListing>,
    /// Start Game's Advanced Config (v20's `$Pref::Server::*`).
    pub settings: Option<bri_admin::ServerSettings>,
    /// Administrator and super administrator passwords.
    pub passwords: Option<(bri_admin::Secret, bri_admin::Secret)>,
    pub add_ons: Option<HostedAddOns>,
    /// Loads another map for Change Map; None cannot change maps.
    pub load_map: Option<LoadMap>,
    /// Where duplicators' saved copies are kept; None keeps none.
    pub copies: Option<Arc<dyn CopyStore>>,
    /// The game version players are told (the New Duplicator's
    /// `/DupVersion`); None leaves the session's own.
    pub game_version: Option<String>,
    /// Where `/botreload` reads bot kinds again and the user's bot dial
    /// overrides are kept (`/botset`, `/botsave`); applied as each session
    /// starts. None: the shipped kinds only.
    pub bot_tuning: Option<bri_sim::session::BotTuning>,
}

impl HostSetup {
    /// What runs when `map` (a base map or a package world) is played.
    pub fn hosted(&self, map: &str) -> Result<Hosted> {
        match &self.add_ons {
            Some(add_ons) => hosted(Some(&add_ons.server), map, add_ons.mode.as_deref()),
            None => hosted(None, map, None),
        }
    }

    /// The session for `map` (resolved by [`Self::hosted`]) on its loaded
    /// base map, with everything this host runs: content, rules, Add-On
    /// scripts and their saved state. Also the spawn points: a package world
    /// generates its own ground.
    pub fn session(&self, hosted: &Hosted, map: MapSession) -> Result<(Session, Vec<Vec3>)> {
        let content = &self.content;
        let mut session = Session::new(map.simulation);
        session.set_lan_host(self.lan);
        session.set_tool_catalog(content.tool_catalog.clone())?;
        session.set_weapon_pack(content.weapon_pack.clone())?;
        session.set_item_bounds(content.item_bounds.clone())?;
        session.set_avatar_catalog(content.avatar_catalog.clone())?;
        session.set_body_mount_points(BLOCKHEAD_MODEL, content.body_mounts.clone())?;
        session.set_vehicle_pack(content.vehicle_pack.clone(), content.bot_kinds.clone())?;
        if let Some(tuning) = &self.bot_tuning {
            for problem in session.set_bot_tuning(tuning.clone())? {
                bri_console::warn(format!("Bot override left out: {problem}"));
            }
        }
        session.set_event_catalog(content.event_catalog.clone(), content.event_sounds.clone())?;
        session.set_spawn_points(map.spawn_points.clone())?;
        session.set_breakables(map.breakables)?;
        session.set_map_list(self.maps.clone())?;
        if let Some(copies) = &self.copies {
            session.set_copy_store(copies.clone());
        }
        if let Some(version) = &self.game_version {
            session.set_game_version(version.clone());
        }
        if let Some(tutorial) = map.tutorial {
            session.set_tutorial(tutorial)?;
        }
        if let Some(settings) = &self.settings {
            session.set_server_settings(settings.clone())?;
        }
        if let Some((admin, super_admin)) = &self.passwords {
            session.set_admin_passwords(admin.clone(), super_admin.clone())?;
        }
        let mut spawn_points = map.spawn_points;
        if let Some(catalog) = &hosted.catalog {
            let save = match self
                .add_ons
                .as_ref()
                .and_then(|a| a.save_path(&hosted.save_key))
                .map(std::fs::read)
            {
                Some(Ok(bytes)) => Some(PackageSave::decode(&bytes)?),
                _ => None,
            };
            let world = catalog.world().is_some();
            let generated = session.install_packages(catalog.clone(), save)?;
            if world {
                anyhow::ensure!(
                    !generated.is_empty(),
                    "The package world generated no ground to stand on"
                );
                spawn_points = generated;
            }
        }
        Ok((session, spawn_points))
    }

    /// Keep the package state `session` ends with under its map's key, for
    /// the next game on that map.
    pub fn keep(&self, session: &Session) -> Result<()> {
        let (Some(add_ons), Some(save)) = (&self.add_ons, session.package_save()) else {
            return Ok(());
        };
        // A package world's provider is the map players chose; otherwise the
        // session stands on the chosen base map.
        let map = save.world.as_ref().map_or_else(
            || session.simulation().state().map_id.clone(),
            |w| w.provider.clone(),
        );
        let hosted = hosted(Some(&add_ons.server), &map, add_ons.mode.as_deref())?;
        let Some(path) = add_ons.save_path(&hosted.save_key) else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        bri_files::replace(&path, &save.encode()?)?;
        Ok(())
    }
}

impl crate::server::MapHost for HostSetup {
    fn load(&self, map: &str) -> Result<Session> {
        let load_map = self
            .load_map
            .as_ref()
            .context("This host cannot change maps")?;
        let hosted = self.hosted(map)?;
        let (session, _) = self.session(&hosted, load_map(&hosted.base_map)?)?;
        Ok(session)
    }
    fn outgoing(&self, session: &Session) {
        if let Err(error) = self.keep(session) {
            eprintln!("Could not save the Add-Ons' state: {error:#}");
        }
    }
}
