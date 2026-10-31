//! Real imported Bot_Town geometry through native placement and replication.
//! The private source archive and imported assets are not committed.
use bri_net::{
    client::Client,
    server::{self, ServerOptions},
};
use bri_sim::{
    definitions::Definitions,
    session::{Command, Reply, Session},
    simulation::Simulation,
};
use glam::Vec3;
use rapier3d::prelude::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a private Bot_Town import; set BRI_BOT_TOWN_OUTPUT"]
async fn imported_town_bricks_plant_collide_save_and_reach_a_late_joiner() -> anyhow::Result<()> {
    let root = std::path::PathBuf::from(
        std::env::var_os("BRI_BOT_TOWN_OUTPUT").expect("BRI_BOT_TOWN_OUTPUT"),
    );
    let dir = root.join("assets/brick-catalog");
    let imported = Definitions::load(&dir, &dir)?;
    assert_eq!(imported.entries.len(), 8);
    let synthetic = bri_chaos::fixture::synthetic_simulation(&[])?;
    let mut definitions = synthetic.definitions.clone();
    let ids: Vec<_> = imported.entries.keys().cloned().collect();
    definitions.entries.extend(imported.entries);
    let mut simulation = bri_chaos::fixture::synthetic_simulation(&[])?;
    simulation.definitions = definitions.clone();
    let mut session = Session::new(simulation);
    session.set_lan_host(true);
    let package_root = root.parent().expect("import directory has a parent");
    let import_dir = root.file_name().unwrap().to_str().unwrap();
    let packages = bri_package::packages::PackageSet {
        schema_version: 1,
        packages: vec![
            bri_package::packages::PackageEntry {
                id: "bot_hole".into(),
                version: "1.0.0".into(),
                side: bri_package::packages::Side::Shared,
                dir: "bot_hole".into(),
                role: None,
            },
            bri_package::packages::PackageEntry {
                id: "bot_town".into(),
                version: "1.0.0".into(),
                side: bri_package::packages::Side::Shared,
                dir: import_dir.into(),
                role: None,
            },
        ],
    };
    let environment = bri_package::environment::Environment::load(package_root, &packages)?;
    let providers = bri_net::content_identity::brick_catalog_providers(package_root, &packages)?;
    let canonical_dir = dir.canonicalize()?;
    assert!(
        providers
            .iter()
            .any(|(_, catalog)| catalog.canonicalize().ok().as_ref() == Some(&canonical_dir)),
        "host content discovery must expose the imported menu catalog"
    );
    let menu: bri_content::brick::Catalog =
        serde_json::from_slice(&std::fs::read(dir.join("stock-catalog.json"))?)?;
    let icons: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("brick-icons.json"))?)?;
    for entry in &menu.bricks {
        let icon = &icons["icons"][entry.icon_source.to_ascii_lowercase()];
        let file = icon["file"].as_str().expect("each menu entry has its icon");
        let bytes =
            std::fs::read(bri_package::path::inside(&dir, file).map_err(anyhow::Error::msg)?)?;
        assert_eq!(
            bri_package::environment::hash_bytes(&bytes),
            icon["sha256"].as_str().unwrap()
        );
    }
    let client_packages = environment.client_packages();
    assert_eq!(
        client_packages.len(),
        2,
        "both real add-ons participate in the join identity"
    );
    let shelf = std::sync::Arc::new(bri_net::packages::PackageShelf::new(
        package_root,
        &packages,
        &environment,
    )?);
    let server = server::start(
        session,
        ServerOptions {
            bind: "127.0.0.1:0".parse()?,
            environment,
            spawn_points: vec![Vec3::new(0.0, 0.05, 0.0), Vec3::new(4.0, 0.05, 0.0)],
            certificate: None,
            map_loader: None,
            packages: Some(shelf),
        },
    )?;
    let mut owner = Client::connect(
        server.address,
        &server.certificate,
        "Builder".into(),
        client_packages.clone(),
        None,
    )
    .await?;
    let mut placed = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let height = definitions.entries[id].mesh.height_plates as f32 * 0.1;
        let position = [-14.0 + i as f32 * 4.0, height, 20.0];
        let Reply::Planted(brick) = owner
            .command(Command::Plant {
                definition: id.clone(),
                position,
                quarter_turns: (i % 4) as u8,
                color: 0,
            })
            .await?
        else {
            panic!("plant {id} returned a different reply")
        };
        placed.push((brick, id.clone(), position));
    }
    let local = tempfile::tempdir()?;
    let cache = bri_package::sync::Cache::open(&local.path().join("cache"))?;
    let identity =
        bri_identity::ClientIdentity::load_or_create(local.path().join("late.identity"))?;
    let mut loaded_count = 0;
    let (late, fetched, dropped) = Client::connect_fetching(
        server.address,
        bri_net::client::HostPin::from(&server.certificate[..]),
        "Late observer".into(),
        Vec::new(),
        None,
        &identity,
        &cache,
        bri_progress::Progress::default(),
        |fetched, _| {
            let town = fetched
                .iter()
                .find(|p| p.package.id == "bot_town")
                .expect("the actual Bot_Town package was fetched");
            let catalog = town.dir.join("assets/brick-catalog");
            loaded_count = Definitions::load(&catalog, &catalog)?.entries.len();
            Ok(fetched.iter().map(|p| p.package.clone()).collect())
        },
    )
    .await?;
    assert!(dropped.is_empty());
    assert_eq!(
        loaded_count, 8,
        "the downloaded package loads all eight definitions"
    );
    assert!(
        fetched
            .iter()
            .any(|p| p.package.id == "bot_town" && p.downloaded > 0)
    );
    assert!(
        fetched
            .iter()
            .any(|p| p.package.id == "bot_hole" && p.downloaded > 0)
    );
    for (brick, definition, _) in &placed {
        let b = &late.replica.world.bricks[brick];
        assert_eq!(
            b.definition,
            bri_world::ContentRef::Resolved(definition.clone())
        );
    }
    drop((owner, late));
    let report = server.stop().await?;
    let path = std::env::temp_dir().join(format!(
        "bri-bot-town-{}-{}.world.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    bri_world::persistence::save_new(&path, &report.native_world)?;
    let world = bri_world::persistence::load(&path)?;
    std::fs::remove_file(path)?;
    assert_eq!(world.bricks.len(), 8);
    let map =
        vec![ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0))];
    let sim = Simulation::new(world, definitions, map)?;
    assert!(sim.state().unloaded.is_empty());
    let query = sim
        .physics
        .query_pipeline_with_filter(QueryFilter::only_fixed());
    for (_, id, p) in placed {
        let ray = Ray::new(Vector::new(p[0], 2.0, p[2]), Vector::NEG_Y);
        let hit = query.cast_ray(&ray, 1.9, true);
        assert!(hit.is_some(), "{id} did not collide after save and reload");
    }
    Ok(())
}
