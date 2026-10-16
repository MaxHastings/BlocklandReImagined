//! Ordinary admin map changes through native Add-On providers and HostSetup.
mod common;
use anyhow::{Context, Result, ensure};
use bri_admin::{Action, Request};
use bri_net::{client::Client, dedicated, server};
use bri_package::packages::PackageSet;
use bri_sim::session::{Command, Session};
use bri_world::World;
use serde_json::json;
use std::{path::Path, time::Duration};

const FIRST: &str = "first-map:map/main";
const SECOND: &str = "second-map:map/main";
const SLATE: &str = "v20/add-ons/map_slate/slate.mis";
const DESERT: &str = "v20/add-ons/map_slate_desert/slatedesert.mis";

fn root() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    bri_net::testing::write_root(root.path(), &[SLATE, DESERT])?;
    let mut packages = serde_json::to_value(PackageSet::load_root(root.path())?)?;
    for (package, map) in [("first-map", FIRST), ("second-map", SECOND)] {
        let dir = root.path().join(package);
        let mut rooms = bri_content::testing::map_bundle::rooms_for(&[map]);
        rooms[0].name = package.into();
        if map == SECOND {
            rooms[0].origin.x = 80.;
        }
        bri_content::testing::map_bundle::write_bundle(&dir.join("native"), &rooms)?;
        std::fs::write(
            dir.join("package.json"),
            serde_json::to_vec(&json!({
                "schema_version":1,"id":package,"version":"1.0.0","api":1,
                "name":package,"license":"MIT",
                "provides":[{"id":map,"kind":"map","file":"native/bundle.json"}]
            }))?,
        )?;
        packages["packages"].as_array_mut().unwrap().push(json!({
            "id":package,"version":"1.0.0","side":"shared","dir":package
        }));
    }
    std::fs::write(
        root.path().join("packages.json"),
        serde_json::to_vec(&packages)?,
    )?;
    Ok(root)
}
fn host(root: &Path, map: &str) -> Result<dedicated::Dedicated> {
    dedicated::load(
        root,
        World::new("Initial".into(), map.into(), vec![[1.; 4]]),
    )
}
fn change(session: &mut Session, owner: u64, sequence: u64, map: &str) -> Result<()> {
    session
        .command(
            owner,
            sequence,
            Command::Admin(Request::new(Action::ChangeMap { map: map.into() })),
        )
        .with_context(|| format!("ordinary Session ChangeMap to {map}"))?;
    ensure!(
        session.take_map_change() == Some((owner, map.into())),
        "map request was not queued"
    );
    Ok(())
}

#[test]
fn listed_native_addon_map_reaches_the_ordinary_session_map_request() -> Result<()> {
    let root = root()?;
    let mut host = host(root.path(), FIRST)?;
    assert!(host.setup.maps.iter().any(|m| m.id == SECOND));
    // The actual native provider loads, independently of command validation.
    assert_eq!(
        host.setup
            .load_map(SECOND, Some(None))?
            .0
            .simulation()
            .state()
            .map_id,
        SECOND
    );
    let owner = host
        .session
        .join("Host".into(), host.spawn_points[0], true)?;
    assert!(
        host.session
            .command(
                owner,
                1,
                Command::Admin(Request::new(Action::ChangeMap {
                    map: "missing-map:map/main".into()
                }))
            )
            .is_err()
    );
    assert!(host.session.take_map_change().is_none());
    change(&mut host.session, owner, 2, DESERT)?;
    change(&mut host.session, owner, 3, SECOND)?;
    Ok(())
}

async fn wait_for_map(client: &mut Client, map: &str) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(15), async {
        while client.replica.world.map_id != map {
            client.receive().await?;
        }
        Result::<()>::Ok(())
    })
    .await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosted_map_change_crosses_distinct_addon_providers_and_stock_maps() -> Result<()> {
    let root = root()?;
    let host = host(root.path(), FIRST)?;
    let mut options = common::options();
    options.spawn_points = host.spawn_points;
    options.map_loader = Some(host.setup);
    let server = server::start(host.session, options)?;
    let mut admin = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Host".into(),
        vec![],
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    let mut guest = Client::connect(
        server.address,
        &server.certificate,
        "Guest".into(),
        vec![],
        None,
    )
    .await?;
    let owner = admin.owner;
    assert!(
        guest
            .command(Command::Admin(Request::new(Action::ChangeMap {
                map: SECOND.into()
            })))
            .await
            .is_err()
    );
    for bad in [
        "missing-map:map/main",
        "first-map:weapon/main",
        "second-map:map/../main",
        "C:/maps/main",
        "",
    ] {
        assert!(
            admin
                .command(Command::Admin(Request::new(Action::ChangeMap {
                    map: bad.into()
                })))
                .await
                .is_err(),
            "accepted {bad}"
        );
        assert_eq!(admin.replica.world.map_id, FIRST);
    }
    for target in [SLATE, DESERT, SECOND, FIRST, SECOND] {
        admin
            .command(Command::Admin(Request::new(Action::ChangeMap {
                map: target.into(),
            })))
            .await
            .with_context(|| format!("ordinary network ChangeMap to {target}"))?;
        wait_for_map(&mut admin, target).await?;
        wait_for_map(&mut guest, target).await?;
        assert_eq!(admin.owner, owner);
        assert_eq!(admin.replica.world.map_id, target);
        assert_eq!(guest.replica.world.map_id, target);
    }
    server.stop().await?;
    Ok(())
}
