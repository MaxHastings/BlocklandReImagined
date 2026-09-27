//! Stress Lab harness. The Stress Lab is a set of ordinary mod packages
//! (`packages/stresslab`); this crate only loads them into headless
//! sessions, loopback hosts and soak runs for tests and tools. Nothing here
//! is gameplay: that lives in the packages.
use anyhow::{Context, Result};
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    session::{PackageSave, Session},
    simulation::Simulation,
};
use glam::Vec3;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// The Stress Lab packages, in load order, with the side each runs on.
pub const PACKAGES: [(&str, Side); 5] = [
    ("stresslab-world", Side::Server),
    ("stresslab-creeper", Side::Server),
    ("stresslab-creeper-model", Side::Client),
    ("stresslab-economy", Side::Server),
    ("stresslab-hud", Side::Client),
];
/// The brick the Stress Lab world draws its voxels with.
pub const CUBE: &str = "v20/brick/brick4xcubedata";

/// `packages/stresslab` in this checkout.
pub fn packages_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/stresslab")
}
/// The package list entries a content root's `packages.json` adds for the
/// Stress Lab, with `dir` relative to `prefix`.
pub fn entries(prefix: &str) -> Vec<PackageEntry> {
    PACKAGES
        .iter()
        .map(|(id, side)| PackageEntry {
            id: (*id).into(),
            version: "1.0.0".into(),
            side: *side,
            dir: if prefix.is_empty() {
                (*id).into()
            } else {
                format!("{prefix}/{id}")
            },
            role: None,
        })
        .collect()
}
/// A package list with only the Stress Lab and the one base package it
/// names, for fixtures whose root is `packages/stresslab`.
pub fn fixture_set() -> PackageSet {
    let mut packages = vec![PackageEntry {
        id: "v20-bricks".into(),
        version: "4.0.0".into(),
        side: Side::Shared,
        dir: "unused".into(),
        role: Some("brick_catalog".into()),
    }];
    packages.extend(entries(""));
    PackageSet {
        schema_version: 1,
        packages,
    }
}
/// Load the Stress Lab as a server (`server`) or client would.
pub fn catalog(root: &Path, set: &PackageSet, server: bool) -> Result<Arc<Catalog>> {
    Catalog::load(root, set, server)
        .map(Arc::new)
        .map_err(|problems| {
            anyhow::Error::new(bri_package::diag::Rejected(bri_package::diag::Diagnostics(
                problems,
            )))
        })
}
/// Brick definitions with just the voxel cube, for tests without the
/// generated content packs.
pub fn fixture_definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: CUBE.into(),
        footprint_studs: [4, 4],
        height_plates: 10,
        attachment_rows: vec!["bbbb".into(); 4],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: CUBE.into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [2.0, 2.0, 2.0],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .expect("box collider")
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [(
            CUBE.into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
            },
        )]
        .into(),
    }
}
/// A headless Stress Lab session: an empty world with the packages enabled
/// and their world generated around the origin. Returns its spawn points.
pub fn session(
    definitions: Definitions,
    catalog: Arc<Catalog>,
    save: Option<PackageSave>,
) -> Result<(Session, Vec<Vec3>)> {
    let world = bri_world::World::new("Stress Lab".into(), "stresslab".into(), vec![[1.0; 4]]);
    let mut session = Session::new(Simulation::new(world, definitions, vec![])?);
    let spawns = session.install_packages(catalog, save)?;
    anyhow::ensure!(
        !spawns.is_empty(),
        "The world provider generated no ground to spawn on"
    );
    Ok((session, spawns))
}
/// The fixture session: synthetic cube definition, packages from this
/// checkout.
pub fn fixture_session(save: Option<PackageSave>) -> Result<(Session, Vec<Vec3>)> {
    let catalog = catalog(&packages_root(), &fixture_set(), true)
        .context("Loading the Stress Lab packages")?;
    session(fixture_definitions(), catalog, save)
}

/// Loopback helpers shared by the acceptance tests and the soak tool.
pub mod net {
    use anyhow::{Result, ensure};
    use bri_net::client::{Client, ClientEvent};
    use bri_sim::session::{ActionAim, Command, PackageArg, PackageCommand, Reply};
    use std::time::Duration;

    pub fn package(package: &str, command: &str, args: Vec<PackageArg>) -> Command {
        Command::Package(PackageCommand {
            package: package.into(),
            command: command.into(),
            args,
        })
    }
    /// Send a command with an aim and wait for its reply.
    pub async fn command(
        client: &mut Client,
        command: Command,
        aim: Option<ActionAim>,
    ) -> Result<Reply> {
        let sequence = client.request_with_aim(command, aim).await?;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let ClientEvent::Reply {
                    sequence: reply,
                    result,
                } = client.receive().await?
                {
                    ensure!(reply == sequence, "Unexpected reply sequence");
                    return result.map_err(|r| anyhow::anyhow!(r.message));
                }
            }
        })
        .await?
    }
    /// Receive until `predicate` holds, for at most `seconds`.
    pub async fn wait(
        client: &mut Client,
        seconds: u64,
        predicate: impl Fn(&Client) -> bool,
    ) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(seconds), async {
            while !predicate(client) {
                client.receive().await?;
            }
            Result::<()>::Ok(())
        })
        .await?
    }
    /// A public package value for this client's own player.
    pub fn own(client: &Client, package: &str, key: &str) -> Option<i64> {
        client
            .replica
            .package_state
            .packages
            .get(package)?
            .players
            .get(&client.owner)?
            .get(key)?
            .as_i64()
    }
    /// Look straight down (so aimed commands hit the ground underfoot).
    pub const DOWN: Option<ActionAim> = Some(ActionAim {
        yaw: 0.0,
        pitch: -1.5,
    });
}
