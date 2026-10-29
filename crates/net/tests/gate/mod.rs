//! What the v0.3.0 release gates share: a host built the way a real one is
//! (`bri_net::dedicated::load_packages`) on a made-up content root with an
//! open room big enough to drive in, the Add-Ons it runs staged into that
//! root for the test's length, and the way a test finds the private Add-Ons
//! (`BRI_PRIVATE_ADDONS`). Nothing private is ever in the repository: the
//! private folders are copied into a scratch root that is removed after.
#![allow(dead_code)]
use anyhow::{Context, Result};
use bri_content::testing::{ScratchDir, map_bundle};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use std::path::{Path, PathBuf};

/// The map every gate plays on: the made-up root's map id, rewritten as
/// one open room `HALF` across each way with its floor at y = 0.
pub const MAP: &str = bri_net::testing::MAP;
pub const HALF: f32 = 72.0;

/// The folder holding the private Add-Ons (`mario/`, `mario-rules/` and the
/// Halo ones, either flat or under `halo/`), from `BRI_PRIVATE_ADDONS`.
/// None skips the private gates, as content-gated tests do elsewhere.
pub fn private_add_ons() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("BRI_PRIVATE_ADDONS")?);
    root.is_dir().then_some(root)
}

/// Where `id` sits under the private folder: `<root>/<id>`, or, as the
/// shared project folder keeps them, `<root>/halo/<id>`.
pub fn find_private(root: &Path, id: &str) -> Result<PathBuf> {
    [root.join(id), root.join("halo").join(id)]
        .into_iter()
        .find(|p| p.join("package.json").is_file())
        .with_context(|| format!("no {id}/package.json under {}", root.display()))
}

/// The repository's own Add-On folder `dir` (under `packages/`).
pub fn repository(dir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages")
        .join(dir)
}

/// One Add-On a gate host runs: its id, the folder it comes from and its
/// side, as the Add-Ons list names it.
pub struct AddOn {
    pub id: String,
    pub from: PathBuf,
    pub side: Side,
}
impl AddOn {
    pub fn new(id: &str, from: PathBuf, side: Side) -> Self {
        Self {
            id: id.into(),
            from,
            side,
        }
    }
}

/// The Playground's own Add-Ons, every pack of it, from the repository.
pub fn playground() -> Vec<AddOn> {
    [
        (
            "playground-runner",
            "playground/playground-runner",
            Side::Server,
        ),
        (
            "playground-runner-look",
            "playground/playground-runner-look",
            Side::Client,
        ),
        (
            "playground-runner-hud",
            "playground/playground-runner-hud",
            Side::Client,
        ),
        ("sample-trooper", "samples/sample-trooper", Side::Server),
        (
            "playground-rifle",
            "playground/playground-rifle",
            Side::Shared,
        ),
        (
            "playground-buggy",
            "playground/playground-buggy",
            Side::Shared,
        ),
        ("blockhead_bot", "blockhead_bot", Side::Shared),
        (
            "playground-stomper",
            "playground/playground-stomper",
            Side::Shared,
        ),
        (
            "playground-stomper-look",
            "playground/playground-stomper-look",
            Side::Client,
        ),
        (
            "playground-stomper-rules",
            "playground/playground-stomper-rules",
            Side::Server,
        ),
        (
            "playground-fizzer",
            "playground/playground-fizzer",
            Side::Shared,
        ),
        (
            "playground-fizzer-look",
            "playground/playground-fizzer-look",
            Side::Client,
        ),
        (
            "playground-fizzer-rules",
            "playground/playground-fizzer-rules",
            Side::Server,
        ),
    ]
    .into_iter()
    .map(|(id, dir, side)| AddOn::new(id, repository(dir), side))
    .collect()
}

/// A made-up content root holding the base game's server packages, the
/// open room, and `add_ons` staged under `add-ons/<id>`; and the package
/// list naming all of them. Removed when dropped.
pub struct Root {
    pub scratch: ScratchDir,
    pub set: PackageSet,
}
impl Root {
    pub fn new(label: &str, add_ons: &[AddOn]) -> Result<Self> {
        let scratch = ScratchDir::new(label)?;
        let root = scratch.path();
        bri_net::testing::write_root(root, &[MAP])?;
        // One open room, floor at y = 0, big enough to drive a Warthog in.
        let mut room = map_bundle::rooms_for(&[MAP]).remove(0);
        room.origin = glam::Vec3::ZERO;
        room.floor = 0.0;
        room.half = HALF;
        room.height = 40.0;
        room.skylight = true;
        room.spawn = glam::Vec3::new(0.0, 0.05, 20.0);
        room.extra_spawns = vec![glam::Vec3::new(4.0, 0.0, 0.0)];
        room.props.clear();
        map_bundle::write_bundle(&bri_net::testing::role_dir(root, "map_bundle")?, &[room])?;
        // The made-up catalog's six-by-six vehicle spawn plate is a
        // Vehicle Spawn brick, as v20's is: the wrench offers it vehicles
        // and bots.
        let catalog = bri_net::testing::role_dir(root, "brick_catalog")?.join("stock-catalog.json");
        let mut bricks: serde_json::Value = serde_json::from_slice(&std::fs::read(&catalog)?)?;
        for brick in bricks["bricks"].as_array_mut().context("no bricks")? {
            if brick["id"] == bri_sim::testing::VEHICLE_SPAWN {
                brick["special_kind"] = "VehicleSpawn".into();
            }
        }
        std::fs::write(&catalog, serde_json::to_vec_pretty(&bricks)?)?;
        let mut set = PackageSet::load_root(root)?;
        for add_on in add_ons {
            let dir = format!("add-ons/{}", add_on.id);
            copy_dir(&add_on.from, &root.join(&dir))
                .with_context(|| format!("staging {}", add_on.from.display()))?;
            let manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(add_on.from.join("package.json"))?)?;
            set.packages.push(PackageEntry {
                id: add_on.id.clone(),
                version: manifest["version"]
                    .as_str()
                    .context("no version in package.json")?
                    .into(),
                side: add_on.side,
                dir,
                role: None,
            });
        }
        set.validate().into_result()?;
        Ok(Self { scratch, set })
    }
    pub fn path(&self) -> &Path {
        self.scratch.path()
    }
    /// The real host's session over `world` (on [`MAP`]), every Add-On on.
    pub fn host(&self, world: bri_world::World) -> Result<bri_net::dedicated::Dedicated> {
        bri_net::dedicated::load_packages(self.path(), &self.set, world)
    }
    /// What a joining player already has: the base game's packages.
    pub fn base_packages(
        &self,
        host: &bri_net::dedicated::Dedicated,
    ) -> Vec<bri_package::environment::PackageRef> {
        let base: Vec<&str> = self
            .set
            .packages
            .iter()
            .filter(|p| p.role.is_some())
            .map(|p| p.id.as_str())
            .collect();
        host.environment
            .packages
            .iter()
            .filter(|p| base.contains(&p.id.as_str()))
            .cloned()
            .collect()
    }
}

/// An empty world on [`MAP`].
pub fn world(name: &str) -> bri_world::World {
    let palette = (0..16)
        .map(|i| {
            let f = i as f32 / 15.0;
            [f, 1.0 - f, 0.5, 1.0]
        })
        .collect();
    bri_world::World::new(name.into(), MAP.into(), palette)
}

pub fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// A whole-number environment variable, or `default`.
pub fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// An Add-On in the repository, with the side its manifest names (Shared
/// when it names none).
pub fn repository_add_on(id: &str, dir: &str) -> AddOn {
    let from = repository(dir);
    let side = bri_package::library::package_info(&from)
        .and_then(|info| info.side())
        .unwrap_or(Side::Shared);
    AddOn::new(id, from, side)
}

/// A player's join the way the game makes one: connect, download what the
/// host offers that `have` lacks into the empty cache `cache`, load it as
/// the game would (every downloaded Add-On must load) and join declaring
/// it. Returns the client and what it downloaded (id, bytes).
#[allow(clippy::too_many_arguments)]
pub async fn fetching_join(
    address: std::net::SocketAddr,
    certificate: &[u8],
    name: &str,
    have: Vec<bri_package::environment::PackageRef>,
    host_token: Option<bri_net::protocol::ResumeToken>,
    identity: &bri_identity::ClientIdentity,
    cache: &bri_package::sync::Cache,
    progress: bri_progress::Progress,
) -> Result<(bri_net::client::Client, Vec<(String, u64)>)> {
    let (client, fetched, _) = bri_net::client::Client::connect_fetching(
        address,
        bri_net::client::HostPin::from(certificate),
        name.into(),
        have.clone(),
        host_token,
        identity,
        cache,
        progress,
        |fetched, _| {
            let mut dirs: Vec<_> = fetched
                .iter()
                .map(|f| {
                    (
                        f.dir.clone(),
                        PackageEntry {
                            id: f.package.id.clone(),
                            version: f.package.version.clone(),
                            side: f.package.side,
                            dir: f.dir.to_string_lossy().into_owned(),
                            role: None,
                        },
                    )
                })
                .collect();
            bri_net::packages::declare_host_references(&mut dirs, fetched);
            let (_, problems) = bri_package_runtime::Catalog::load_dirs_skipping(&dirs, false);
            anyhow::ensure!(
                problems.is_empty(),
                "a downloaded Add-On does not load: {problems:?}"
            );
            Ok(have
                .iter()
                .cloned()
                .chain(fetched.iter().map(|f| f.package.clone()))
                .collect())
        },
    )
    .await?;
    let mut got: Vec<(String, u64)> = fetched
        .iter()
        .map(|f| (f.package.id.clone(), f.downloaded))
        .collect();
    got.sort();
    Ok((client, got))
}
