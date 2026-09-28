//! Package agreement at join, with Stress Lab packages: a loopback host and
//! clients whose package copies differ. Shared differences refuse the join
//! and name the package; presentation differences join with a chat line
//! naming it.
use anyhow::Result;
use bri_identity::ClientIdentity;
use bri_net::{
    client::{Client, ClientEvent},
    server::{self, ServerOptions},
};
use bri_package::{
    environment::Environment,
    packages::{PackageSet, Side},
};
use std::{fs, path::Path, time::Duration};

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}
/// The Stress Lab list, with the creeper model marked `shared` so a
/// difference in it must refuse the join (a model a server simulates with).
fn set() -> PackageSet {
    let mut set = bri_stresslab::stresslab_set();
    for p in &mut set.packages {
        if p.id == "stresslab-creeper-model" {
            p.side = Side::Shared;
        }
    }
    set
}
async fn join(
    server: &server::ServerHandle,
    name: &str,
    root: &Path,
    dir: &Path,
) -> Result<Client> {
    let identity = ClientIdentity::load_or_create(dir.join(format!("{name}.key")))?;
    let packages = Environment::load(root, &set())?.client_packages();
    Client::connect_with_identity(
        server.address,
        &server.certificate,
        name.into(),
        packages,
        None,
        None,
        &identity,
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn differing_packages_are_named_at_join() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let host_root = dir.path().join("host");
    copy(&bri_stresslab::packages_root(), &host_root);
    let (session, spawns) = bri_stresslab::fixture_session(None)?;
    let server = server::start(
        session,
        ServerOptions {
            bind: "127.0.0.1:0".parse()?,
            environment: Environment::load(&host_root, &set())?,
            spawn_points: spawns,
            certificate: None,
            map_loader: None,
            autosave: None,
        },
    )?;

    // Same packages: joins.
    let same = join(&server, "Same", &host_root, dir.path()).await?;

    // A changed client-side HUD: joins, and is told which package differs.
    let hud = dir.path().join("hud");
    copy(&host_root, &hud);
    fs::write(
        hud.join("stresslab-hud/miner.json"),
        fs::read_to_string(host_root.join("stresslab-hud/miner.json"))?
            .replace("STRESS LAB MINER", "MY OWN MINER"),
    )?;
    let mut cosmetic = join(&server, "Cosmetic", &hud, dir.path()).await?;
    let mut notices = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(ClientEvent::Notice(bri_sim::session::Notice::Chat(text))) =
                cosmetic.receive().await
            {
                let found = text.contains("stresslab-hud");
                notices.push(text);
                if found {
                    break;
                }
            }
        }
    })
    .await;
    assert!(
        notices.iter().any(|n| n.contains("stresslab-hud")),
        "{notices:?}"
    );

    // A changed shared model: refused, naming the package.
    let model = dir.path().join("model");
    copy(&host_root, &model);
    fs::write(
        model.join("stresslab-creeper-model/models/creeper.json"),
        fs::read_to_string(host_root.join("stresslab-creeper-model/models/creeper.json"))?
            .replace("0.35, 0.72", "0.95, 0.12"),
    )?;
    let refused = join(&server, "Changed", &model, dir.path())
        .await
        .err()
        .expect("a changed shared package refuses the join");
    let text = format!("{refused:#}");
    assert!(text.contains("stresslab-creeper-model"), "{text}");
    assert!(
        !text.contains("stresslab-hud"),
        "only the blocking difference: {text}"
    );

    // A missing shared package: refused, naming it.
    let missing = dir.path().join("missing");
    copy(&host_root, &missing);
    let mut without = set();
    without
        .packages
        .retain(|p| p.id != "stresslab-creeper-model");
    let identity = ClientIdentity::load_or_create(dir.path().join("missing.key"))?;
    let packages = Environment::load(&missing, &without)?.client_packages();
    let refused = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Missing".into(),
        packages,
        None,
        None,
        &identity,
    )
    .await
    .err()
    .expect("a missing shared package refuses the join");
    assert!(
        format!("{refused:#}").contains("stresslab-creeper-model"),
        "{refused:#}"
    );

    drop((same, cosmetic));
    server.stop().await?;
    Ok(())
}
