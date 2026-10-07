//! Match recording files, and setting up a session to replay one.
//!
//! A host records a match when asked ([`Recording`]): the file holds how the
//! host set the game up ([`Header`]), then the session's calls and tick
//! digests ([`bri_sim::replay`]). Replaying needs the same content folder
//! (the same packages) and the same build; the session is rebuilt from the
//! header exactly as the host built it, then the frames are played into it.
//!
//! File layout: [`MAGIC`], the file schema as a little-endian `u32`, then
//! one zstd stream holding the header and the frames, each a little-endian
//! `u32` length and its MessagePack. The stream is flushed every second of
//! play, so a host that crashes leaves a file readable up to the last
//! second before.
use crate::{
    content_identity::WeaponContent,
    host_setup::{HostSetup, Hosted, HostedAddOns, SessionContent},
    map_content::MapContent,
};
use anyhow::{Context, Result, ensure};
use bri_package::{environment::Environment, packages::PackageSet};
use bri_sim::{
    bot_kind::tuning::Overrides,
    replay::{FrameReader, Recorder, Secrets},
    session::{CopyStore, MapListing, Session, StoreDone},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

/// A match recording file starts with these bytes.
pub const MAGIC: &[u8; 8] = b"BRIMATCH";
/// The layout and header this build writes and reads.
pub const FILE_SCHEMA: u32 = 1;
/// A match recording's file extension.
pub const EXTENSION: &str = "brimatch";
/// zstd's level for recordings: its fast end, since the host compresses on
/// its own tick.
const COMPRESSION_LEVEL: i32 = 1;
/// Largest header a reader accepts (the starting world dominates it).
const MAX_HEADER_BYTES: usize = 1 << 30;

/// How the host set the game up, captured as it does: enough to build the
/// same session again from the same content folder.
#[derive(Clone, Serialize, Deserialize)]
pub struct HostRecipe {
    /// The world the first map was loaded with, before any Add-On generated
    /// ground into it.
    pub world: bri_world::World,
    /// The map players chose ([`Hosted::map`]).
    pub map: String,
    /// Start Game's game mode; None is Custom.
    pub mode: Option<String>,
    /// Whether the host ran its server-side Add-Ons.
    pub add_ons: bool,
    pub lan: bool,
    pub content: SessionContent,
    pub maps: Vec<MapListing>,
    pub settings: Option<bri_admin::ServerSettings>,
    /// The administrator and super administrator passwords. In a file,
    /// these are stand-ins ([`Secrets`]).
    pub passwords: Option<(bri_admin::Secret, bri_admin::Secret)>,
    pub game_version: Option<String>,
    pub bot_tuning: Option<RecordedTuning>,
    /// Whether the host kept duplicator copies.
    pub copies: bool,
    /// The Add-On state the first map's session was set up with.
    pub package_save: Option<Vec<u8>>,
    /// The paint palette Change Map gives new worlds.
    pub map_palette: Vec<[f32; 4]>,
}
/// Where the host's bot tuning came from (`/botreload`, `/botsave`).
#[derive(Clone, Serialize, Deserialize)]
pub struct RecordedTuning {
    /// Whether `/botreload` read the kinds again.
    pub reload: bool,
    /// The file `/botsave` kept the overrides in.
    pub overrides: Option<PathBuf>,
}
impl HostSetup {
    /// What a recording needs of how this host set up `hosted`: `world` is
    /// the one its map was loaded with, `package_save` the Add-On state
    /// the session got ([`Self::package_save`]), `map_palette` Change Map's
    /// paint.
    pub fn recipe(
        &self,
        hosted: &Hosted,
        world: bri_world::World,
        package_save: Option<Vec<u8>>,
        map_palette: Vec<[f32; 4]>,
    ) -> HostRecipe {
        HostRecipe {
            world,
            map: hosted.map.clone(),
            mode: self.add_ons.as_ref().and_then(|a| a.mode.clone()),
            add_ons: self.add_ons.is_some(),
            lan: self.lan,
            content: self.content.clone(),
            maps: self.maps.clone(),
            settings: self.settings.clone(),
            passwords: self.passwords.clone(),
            game_version: self.game_version.clone(),
            bot_tuning: self.bot_tuning.as_ref().map(|t| RecordedTuning {
                reload: t.reload.is_some(),
                overrides: t.overrides.clone(),
            }),
            copies: self.copies.is_some(),
            package_save,
            map_palette,
        }
    }
}

/// What a recording file starts with.
#[derive(Clone, Serialize, Deserialize)]
pub struct Header {
    /// [`bri_sim::replay::SCHEMA_VERSION`] of the frames that follow.
    pub frames_schema: u32,
    /// The digest of every package the host loaded: the content folder a
    /// replay needs.
    pub environment: String,
    /// When recording started, in seconds since the Unix epoch.
    pub started: u64,
    pub host: HostRecipe,
    pub start: StartState,
}
/// The session's state as recording started, beyond its setup.
#[derive(Clone, Serialize, Deserialize)]
pub struct StartState {
    /// The administration state the host restored (bans, ranks).
    pub admin: Vec<u8>,
    /// A restarted host's mini-game, waiting for its player.
    pub held_minigame: Option<serde_json::Value>,
    /// The bot override dials in effect.
    pub bot_overrides: Overrides,
}

/// A host's request to record its match.
pub struct Recording {
    /// The file to write (made with its folder).
    pub path: PathBuf,
    pub host: HostRecipe,
}

/// Begin recording `session` (set up as `recording.host` says, about to be
/// served) to `recording.path`.
pub fn start(
    session: &mut Session,
    recording: Recording,
    environment: &Environment,
) -> Result<Recorder> {
    let Recording { path, mut host } = recording;
    let mut secrets = Secrets::default();
    host.passwords = host
        .passwords
        .map(|(admin, super_admin)| (secrets.redact(&admin), secrets.redact(&super_admin)));
    let mut admin = Vec::new();
    session.admin_durable_state().write(&mut admin)?;
    let header = Header {
        frames_schema: bri_sim::replay::SCHEMA_VERSION,
        environment: environment.digest(),
        started: bri_sim::replay::WallClock::now().0,
        host,
        start: StartState {
            admin,
            held_minigame: session.held_minigame().cloned(),
            bot_overrides: session.bot_overrides().clone(),
        },
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file =
        std::fs::File::create_new(&path).with_context(|| format!("Creating {}", path.display()))?;
    file.write_all(MAGIC)?;
    file.write_all(&FILE_SCHEMA.to_le_bytes())?;
    let mut out = zstd::stream::write::Encoder::new(file, COMPRESSION_LEVEL)?.auto_finish();
    let bytes = bri_sim::replay::encode(&header)?;
    out.write_all(&u32::try_from(bytes.len())?.to_le_bytes())?;
    out.write_all(&bytes)?;
    Recorder::start(session, secrets, Box::new(out))
}

/// An open recording: its header, and its frames to read.
pub struct Opened {
    pub header: Header,
    pub frames: FrameReader<Box<dyn Read>>,
}
/// Open the recording at `path`.
pub fn open(path: &Path) -> Result<Opened> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("Opening {}", path.display()))?;
    let mut magic = [0; 8];
    file.read_exact(&mut magic)?;
    ensure!(
        &magic == MAGIC,
        "{} is not a match recording",
        path.display()
    );
    let mut schema = [0; 4];
    file.read_exact(&mut schema)?;
    let schema = u32::from_le_bytes(schema);
    ensure!(
        schema == FILE_SCHEMA,
        "{} is a version {schema} recording; this build reads version {FILE_SCHEMA}",
        path.display()
    );
    let mut input: Box<dyn Read> = Box::new(zstd::stream::read::Decoder::new(file)?);
    let mut len = [0; 4];
    input
        .read_exact(&mut len)
        .context("Reading the recording's header")?;
    let len = u32::from_le_bytes(len) as usize;
    ensure!(
        len <= MAX_HEADER_BYTES,
        "The recording's header is too large"
    );
    let mut bytes = vec![0; len];
    input
        .read_exact(&mut bytes)
        .context("Reading the recording's header")?;
    let header: Header =
        bri_sim::replay::decode(&bytes).context("Reading the recording's header")?;
    ensure!(
        header.frames_schema == bri_sim::replay::SCHEMA_VERSION,
        "The recording's frames are version {}; this build reads version {}",
        header.frames_schema,
        bri_sim::replay::SCHEMA_VERSION
    );
    Ok(Opened {
        header,
        frames: FrameReader::new(input),
    })
}

/// A session rebuilt to replay a recording, and the setup that loads the
/// maps it changes to ([`load_map`]).
pub struct Rebuilt {
    pub session: Session,
    pub setup: Arc<HostSetup>,
}
/// The session for a map a recording changed to, with the Add-On state it
/// was set up with, its events prepared as the host's were.
pub fn load_map(setup: &HostSetup, map: &str, save: Option<&[u8]>) -> Result<Session> {
    let (mut session, _) = setup.load_map(map, Some(save))?;
    session.prepare_events();
    Ok(session)
}

/// A copy store that keeps nothing: a replay's answers come from the
/// recording.
struct NoCopies;
impl CopyStore for NoCopies {
    fn save(&self, _: u64, _: &str, _: bri_sim::blueprint::SavedCopy, _: bool) {}
    fn load(&self, _: u64, _: &str) {}
    fn list(&self, _: u64, _: &str) {}
    fn poll(&self) -> Vec<(u64, StoreDone)> {
        Vec::new()
    }
}

/// Build the session `header` recorded, from the content folder at
/// `content_root`, as its host built it.
pub fn rebuild(content_root: &Path, header: &Header) -> Result<Rebuilt> {
    let packages = PackageSet::load_root(content_root)?;
    packages.validate().into_result()?;
    let environment = Environment::load(content_root, &packages)?;
    ensure!(
        environment.digest() == header.environment,
        "The content folder's packages differ from the recording's; replay it with the same Add-Ons and content"
    );
    let role = |role: &str| packages.role_dir(content_root, role);
    let weapon_extras =
        crate::content_identity::kind_providers(content_root, &packages, "weapons.json")?;
    let weapons = WeaponContent::load_with(&role("weapons")?, &weapon_extras)?;
    let maps = MapContent::from_root(content_root, &packages, weapons)?;
    let host = &header.host;
    let add_ons = if host.add_ons {
        let (server, problems) =
            bri_package_runtime::Catalog::load_skipping(content_root, &packages, true);
        ensure!(
            problems.is_empty(),
            "Add-Ons failed to load: {}",
            problems
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        );
        Some(HostedAddOns {
            server: Arc::new(server),
            mode: host.mode.clone(),
            saves: None,
        })
    } else {
        None
    };
    let setup = HostSetup {
        lan: host.lan,
        content: host.content.clone(),
        maps: host.maps.clone(),
        settings: host.settings.clone(),
        passwords: host.passwords.clone(),
        add_ons,
        load_map: Some(maps.clone().loader(host.map_palette.clone())),
        copies: host
            .copies
            .then(|| Arc::new(NoCopies) as Arc<dyn CopyStore>),
        game_version: host.game_version.clone(),
        bot_tuning: host
            .bot_tuning
            .as_ref()
            .map(|t| bri_sim::session::BotTuning {
                // A replay's `/botreload` is answered from the recording.
                reload: t.reload.then(|| {
                    Arc::new(|| anyhow::bail!("A replay reads no bot kinds from disk"))
                        as bri_sim::session::BotReload
                }),
                overrides: t.overrides.clone(),
            }),
        bot_overrides: Some(header.start.bot_overrides.clone()),
    };
    let hosted = setup.hosted(&host.map)?;
    let map = maps.load(host.world.clone())?;
    let (mut session, _) =
        setup.session_with(&hosted, map.into_session(), host.package_save.as_deref())?;
    session.restore_admin_state(&header.start.admin)?;
    if let Some(minigame) = &header.start.held_minigame {
        session.hold_minigame(minigame.clone());
    }
    session.prepare_events();
    Ok(Rebuilt {
        session,
        setup: Arc::new(setup),
    })
}

/// The folder in a host's state directory that holds its recordings.
pub const RECORDINGS_DIR: &str = "recordings";
/// How many recordings a host keeps; it deletes the oldest past this.
pub const KEPT_RECORDINGS: usize = 10;

/// The file for a new recording in `dir`, after deleting the oldest so
/// that with it `dir` holds [`KEPT_RECORDINGS`]. Recordings are numbered
/// in the order they start (`match-<n>.brimatch`), so the newest is never
/// mistaken for an old one, whatever the clock does.
pub fn next_recording(dir: &Path) -> Result<PathBuf> {
    let found = recordings(dir)?;
    let next = found.last().map_or(1, |(number, _)| number + 1);
    let excess = found.len().saturating_sub(KEPT_RECORDINGS - 1);
    for (_, old) in &found[..excess] {
        std::fs::remove_file(old).with_context(|| format!("Removing {}", old.display()))?;
    }
    Ok(dir.join(format!("match-{next}.{EXTENSION}")))
}

/// The numbered recordings in `dir`, oldest first. Other files are left
/// out.
fn recordings(dir: &Path) -> Result<Vec<(u64, PathBuf)>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut found: Vec<(u64, PathBuf)> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == EXTENSION))
        .filter_map(|p| {
            let number = p
                .file_stem()?
                .to_str()?
                .strip_prefix("match-")?
                .parse()
                .ok()?;
            Some((number, p))
        })
        .collect();
    found.sort();
    Ok(found)
}
