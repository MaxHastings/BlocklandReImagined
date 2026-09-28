use anyhow::{Context, Result};
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_identity::ClientIdentity;
use bri_net::{
    client::Client,
    protocol::{
        Hello, IdentityProof, JoinBegin, Message, ResumeToken, VERSION, identity_transcript,
    },
    server::{self, ServerOptions},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{AdminData, Command, InspectMode, Reply, Session, ToolAction},
    simulation::Simulation,
};
use bri_world::{EventRow, EventTarget, EventValue, World};
use glam::Vec3;
use rapier3d::prelude::*;
use sha2::Digest;
use std::time::Duration;
fn session() -> Session {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let defs = Definitions {
        entries: [(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
            },
        )]
        .into(),
    };
    let mut session = Session::new(
        Simulation::new(
            World::new(
                "Loopback".into(),
                "fixture".into(),
                vec![[1.0; 4], [0.0; 4]],
            ),
            defs,
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    session
        .set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    session
}
/// The core tools as v20 images with the stock state layout (Activate, Ready,
/// PreFire, Fire running `onFire`, CheckFire, StopFire), without shapes or
/// effects, so tool swings cross QUIC without the generated weapons pack.
fn tool_pack() -> bri_weapons::Pack {
    let state = |name: &str, ticks, script: &str| bri_weapons::State {
        name: name.into(),
        ticks,
        wait: true,
        allow_change: true,
        script: script.into(),
        ..Default::default()
    };
    let mut items = std::collections::BTreeMap::new();
    let mut images = std::collections::BTreeMap::new();
    for (id, stem) in bri_weapons::CORE_TOOLS
        .into_iter()
        .zip(["hammer", "wrench", "printGun", "wand"])
    {
        let image = format!("v20.image.{}image", stem.to_ascii_lowercase());
        let states = vec![
            bri_weapons::State {
                timeout: Some(1),
                ..state("Activate", 0, "")
            },
            bri_weapons::State {
                down: Some(2),
                ..state("Ready", 0, "")
            },
            bri_weapons::State {
                timeout: Some(3),
                ..state("PreFire", 2, "onPreFire")
            },
            bri_weapons::State {
                timeout: Some(4),
                ..state("Fire", 24, "onFire")
            },
            bri_weapons::State {
                up: Some(5),
                ..state("CheckFire", 0, "")
            },
            bri_weapons::State {
                timeout: Some(1),
                ..state("StopFire", 2, "onStopFire")
            },
        ];
        images.insert(
            image.clone(),
            bri_weapons::Image {
                id: image.clone(),
                name: format!("{stem}Image"),
                model: String::new(),
                projectile: None,
                mount_point: 0,
                offset: [0.; 3],
                eye_offset: [0.; 3],
                source_rotation_degrees: [0.; 3],
                correct_muzzle: false,
                melee: true,
                color: [1.; 4],
                color_shift: false,
                arm_ready: true,
                casing: String::new(),
                min_shot_ticks: 0,
                states,
                command: None,
                commands: Default::default(),
                shot: None,
            },
        );
        items.insert(
            id.to_string(),
            bri_weapons::Item {
                id: id.into(),
                name: format!("{stem}Item"),
                ui_name: stem.into(),
                image,
                model: String::new(),
                icon: String::new(),
                can_drop: true,
                sport: false,
            },
        );
    }
    let pack = bri_weapons::Pack {
        schema_version: bri_weapons::SCHEMA,
        id: "test.tools".into(),
        items,
        images,
        projectiles: Default::default(),
        damage_types: Default::default(),
        explosions: Default::default(),
        definitions: vec![],
        resources: vec![],
        diagnostics: vec![],
    };
    pack.validate().unwrap();
    pack
}
fn tool_session() -> Session {
    let mut session = session();
    session.set_weapon_pack(tool_pack()).unwrap();
    session
}
fn color_row(target: EventTarget, color: u8) -> EventRow {
    EventRow {
        preserved: None,
        enabled: true,
        input: "onActivate".into(),
        delay_ms: 0,
        target,
        output: "setColor".into(),
        params: vec![EventValue::Color(color)],
    }
}

/// Add-On weapons use the platform id grammar (`namespace:weapon/name`), not
/// the base game's `v20.weapon.name`. Every peer validates every replicated
/// inventory with the same rule the host used to give the item, so holding
/// one never disconnects anyone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn add_on_weapon_ids_replicate_to_every_peer() -> Result<()> {
    const SHOTGUN: &str = "addon_shotgun:weapon/shotgunitem";
    let mut pack = tool_pack();
    let mut item = pack.items[bri_weapons::CORE_TOOLS[0]].clone();
    item.id = SHOTGUN.into();
    item.name = "shotgunItem".into();
    pack.items.insert(SHOTGUN.into(), item);
    pack.validate()?;
    let mut game = session();
    game.set_weapon_pack(pack)?;
    game.set_spawn_loadout(bri_sim::session::ToolInventory {
        slots: [Some(SHOTGUN.to_string()), None, None, None, None].into(),
        selected: None,
    })?;
    let server = server::start(game, options())?;
    let mut owner = Client::connect(
        server.address,
        &server.certificate,
        "Owner".into(),
        Vec::new(),
        None,
    )
    .await?;
    let mut observer = Client::connect(
        server.address,
        &server.certificate,
        "Observer".into(),
        Vec::new(),
        None,
    )
    .await?;
    let owner_id = owner.owner;
    assert_eq!(
        owner.replica.tools[&owner_id].slots[0].as_deref(),
        Some(SHOTGUN)
    );
    owner.command(Command::EquipTool { slot: Some(0) }).await?;
    wait(&mut observer, |client| {
        client
            .replica
            .tools
            .get(&owner_id)
            .is_some_and(|t| t.selected == Some(0))
    })
    .await?;
    let late = Client::connect(
        server.address,
        &server.certificate,
        "Late".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert_eq!(late.replica.tools[&owner_id].selected, Some(0));
    assert_eq!(
        late.replica.tools[&owner_id].slots[0].as_deref(),
        Some(SHOTGUN)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inventory_selection_replicates_to_peers_and_late_join_without_cross_owner_changes()
-> Result<()> {
    let mut game = session();
    game.set_item_bounds(
        bri_weapons::CORE_TOOLS
            .into_iter()
            .map(|id| {
                (
                    id.into(),
                    bri_weapons::ItemBounds {
                        min: [-0.1; 3],
                        max: [0.1; 3],
                    },
                )
            })
            .collect(),
    )?;
    let server = server::start(game, options())?;
    let mut owner = Client::connect(
        server.address,
        &server.certificate,
        "Owner".into(),
        Vec::new(),
        None,
    )
    .await?;
    let mut observer = Client::connect(
        server.address,
        &server.certificate,
        "Observer".into(),
        Vec::new(),
        None,
    )
    .await?;
    let owner_id = owner.owner;
    assert_eq!(
        owner.replica.tools[&owner_id],
        bri_sim::session::ToolInventory::default()
    );
    owner.command(Command::EquipTool { slot: Some(2) }).await?;
    wait(&mut observer, |client| {
        client
            .replica
            .tools
            .get(&owner_id)
            .is_some_and(|t| t.selected == Some(2))
    })
    .await?;
    assert_eq!(observer.replica.tools[&observer.owner].selected, None);
    let mut late = Client::connect(
        server.address,
        &server.certificate,
        "Late".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert_eq!(late.replica.tools[&owner_id].selected, Some(2));
    assert!(
        owner
            .command(Command::EquipTool { slot: Some(4) })
            .await
            .is_err()
    );
    owner.command(Command::EquipTool { slot: None }).await?;
    wait(&mut late, |client| {
        client
            .replica
            .tools
            .get(&owner_id)
            .is_some_and(|t| t.selected.is_none())
    })
    .await?;
    owner.command(Command::DropTool { slot: 0 }).await?;
    wait(&mut late, |client| {
        !client.replica.weapons.drops.is_empty()
            && client.replica.tools[&owner_id].slots[0].is_none()
    })
    .await?;
    let drop = late.replica.weapons.drops[0].clone();
    assert_eq!(drop.source.0, owner_id);
    assert_eq!(drop.scale, 1.);
    assert!((drop.rotation.length_squared() - 1.).abs() < 0.00001);
    let observer_id = observer.owner;
    observer.close();
    wait(&mut late, |client| {
        !client.replica.names.contains_key(&observer_id)
    })
    .await?;
    let fourth = Client::connect(
        server.address,
        &server.certificate,
        "Drop observer".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert!(fourth.replica.weapons.drops.iter().any(|d| d.id == drop.id));
    assert!(fourth.replica.tools[&owner_id].slots[0].is_none());
    fourth.close();
    owner.close();
    observer.close();
    late.close();
    let report = server.stop().await?;
    assert!(report.rejected >= 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_event_list_crosses_real_quic_replication_and_native_save_atomically() -> Result<()> {
    let server = server::start(tool_session(), options())?;
    let mut owner = Client::connect(
        server.address,
        &server.certificate,
        "Event builder".into(),
        Vec::new(),
        None,
    )
    .await?;
    let mut observer = Client::connect(
        server.address,
        &server.certificate,
        "Observer".into(),
        Vec::new(),
        None,
    )
    .await?;
    let Reply::Planted(id) = owner
        .command(Command::Plant {
            definition: "plate".into(),
            position: [0.5, 0.1, -3.25],
            quarter_turns: 0,
            color: 0,
        })
        .await?
    else {
        panic!("Expected plant")
    };
    aim(&mut owner).await?;
    let inspect = Command::Tool(ToolAction::Inspect {
        mode: InspectMode::Events,
    });
    // The wrench hit opens the brick; Events is the dialog nested inside it.
    let (hit, _, mode) = swing(&mut owner, 1).await?.expect("wrench opens the brick");
    assert_eq!((hit, mode), (id, InspectMode::Wrench));
    assert!(
        matches!(owner.command(inspect.clone()).await?, Reply::Inspected { brick_id, mode: InspectMode::Events, .. } if brick_id == id)
    );
    let events: Vec<_> = (0..bri_world::MAX_EVENTS_PER_BRICK)
        // Unique names make ordering loss observable without triggering these events.
        .map(|index| {
            color_row(
                EventTarget::Named(format!("event-order-{index:04}")),
                (index % 2) as u8,
            )
        })
        .collect();
    let command = Command::Tool(ToolAction::SetEvents {
        brick: id,
        events: events.clone(),
    });
    let size = serde_json::to_vec(&bri_net::protocol::Request {
        sequence: 3,
        aim: None,
        command: command.clone(),
    })?
    .len();
    assert!(size > 64 * 1024 && size <= bri_net::codec::MAX_REQUEST);
    assert_eq!(owner.command(command).await?, Reply::Accepted);
    wait(&mut owner, |client| {
        client
            .replica
            .world
            .bricks
            .get(&id)
            .is_some_and(|b| b.events.len() == bri_world::MAX_EVENTS_PER_BRICK)
    })
    .await?;
    assert_eq!(owner.replica.world.bricks[&id].events, events);
    wait(&mut observer, |client| {
        client
            .replica
            .world
            .bricks
            .get(&id)
            .is_some_and(|b| b.events.len() == bri_world::MAX_EVENTS_PER_BRICK)
    })
    .await?;
    assert_eq!(observer.replica.world.bricks[&id].events, events);
    // A malformed over-limit edit is delivered and rejected by authority,
    // retaining the complete prior list; failure is not a transport disconnect.
    owner.command(inspect.clone()).await?;
    let mut excess = events.clone();
    excess.push(events[0].clone());
    let error = owner
        .command(Command::Tool(ToolAction::SetEvents {
            brick: id,
            events: excess,
        }))
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("event admission limit"),
        "{error}"
    );
    let Reply::Inspected { brick, .. } = owner.command(inspect).await? else {
        panic!()
    };
    assert_eq!(brick.events, events);
    // Local frame rejection occurs before writing any framing bytes. The same
    // reliable stream must remain usable for the subsequent ordinary command.
    let oversized = "x".repeat(bri_net::codec::MAX_REQUEST);
    assert!(
        owner
            .command(Command::Chat(oversized))
            .await
            .unwrap_err()
            .to_string()
            .contains("Oversized request")
    );
    owner
        .command(Command::Chat("Still connected after rejection".into()))
        .await?;
    wait(&mut observer, |client| !client.replica.chat.is_empty()).await?;
    let late = Client::connect(
        server.address,
        &server.certificate,
        "Late observer".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert_eq!(late.replica.world.bricks[&id].events, events);
    drop(owner);
    drop(observer);
    drop(late);
    let report = server.stop().await?;
    assert_eq!(report.native_world.bricks[&id].events, events);
    let directory = std::env::temp_dir().join(format!(
        "bri-network-events-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir(&directory)?;
    let path = directory.join("events.world.json");
    bri_world::persistence::save_new(&path, &report.native_world)?;
    let restored = bri_world::persistence::load(&path)?;
    assert_eq!(restored.bricks[&id].events, events);
    std::fs::remove_file(path)?;
    std::fs::remove_dir(directory)?;
    eprintln!(
        "{} ordered zero-delay rows: {size} request bytes, two live replicas + late join + native save; over-limit edit rejected atomically",
        events.len()
    );
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires native weapons pack; headless QUIC only"]
async fn native_projectiles_and_equipped_images_survive_real_quic_late_join() -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-009/weapons.json");
    let mut game = session();
    game.set_weapon_pack(bri_weapons::Pack::from_json(&std::fs::read(root)?)?)?;
    let mut loadout = bri_sim::session::ToolInventory::default();
    loadout.slots[3] = Some("v20.weapon.gunitem".into());
    game.set_spawn_loadout(loadout)?;
    let server = server::start(game, options())?;
    let mut first = Client::connect(
        server.address,
        &server.certificate,
        "First".into(),
        Vec::new(),
        None,
    )
    .await?;
    let shooter = first.owner;
    first.command(Command::EquipTool { slot: Some(3) }).await?;
    wait(&mut first, |client| {
        client
            .replica
            .weapons
            .images
            .get(&shooter)
            .is_some_and(|images| images.iter().any(|i| i.state == "Ready"))
    })
    .await?;
    let mut cues = first.replica.take_cues();
    send_inputs(&mut first, &[MoveInput::default()])?;
    first.command(Command::WeaponTrigger { down: true }).await?;
    first
        .command(Command::WeaponTrigger { down: false })
        .await?;
    // What each update brought, for the failure message below.
    let mut timeline = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let fresh = first.replica.take_cues();
            timeline.push(format!(
                "tick {} images {:?} projectiles {:?} cues {:?}",
                first.replica.tick,
                first
                    .replica
                    .weapons
                    .images
                    .get(&shooter)
                    .map(|images| { images.iter().map(|i| i.state.clone()).collect::<Vec<_>>() }),
                first
                    .replica
                    .weapons
                    .projectiles
                    .iter()
                    .map(|p| (p.id, p.age))
                    .collect::<Vec<_>>(),
                fresh.iter().map(|c| (c.tick, &c.kind)).collect::<Vec<_>>()
            ));
            cues.extend(fresh);
            if !first.replica.weapons.projectiles.is_empty() {
                break;
            }
            first.receive().await?;
        }
        Result::<()>::Ok(())
    })
    .await
    .context("waiting for the shot's projectile")??;
    let projectile = first.replica.weapons.projectiles[0].clone();
    assert_eq!(projectile.source.0, shooter);
    assert_eq!(first.replica.tools[&shooter].selected, Some(3));
    assert_eq!(
        first.replica.weapons.images[&shooter][0].image,
        "v20.image.gunimage"
    );
    cues.extend(first.replica.take_cues());
    assert!(
        cues.iter().any(|c| matches!(&c.kind, bri_sim::presentation::CueKind::WeaponSound { profile } if profile == "gunShot1Sound")),
        "no gunShot1Sound cue by the projectile's first update; cues: {:?}\n{}",
        cues.iter().map(|c| (c.tick, &c.kind)).collect::<Vec<_>>(),
        timeline.join("\n")
    );
    let mut second = Client::connect(
        server.address,
        &server.certificate,
        "Late".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert!(
        second
            .replica
            .weapons
            .projectiles
            .iter()
            .any(|p| p.id == projectile.id)
    );
    assert!(second.replica.take_cues().is_empty()); // no replay before join
    wait(&mut first, |client| {
        client
            .replica
            .weapons
            .projectiles
            .iter()
            .any(|p| p.id == projectile.id && p.age > projectile.age)
    })
    .await?;
    first.close();
    second.close();
    server.stop().await?;
    Ok(())
}

fn options() -> ServerOptions {
    ServerOptions {
        bind: "127.0.0.1:0".parse().unwrap(),
        environment: bri_package::environment::Environment::empty(),
        spawn_points: vec![
            Vec3::new(0.0, 0.05, 0.0),
            Vec3::new(3.0, 0.05, 0.0),
            Vec3::new(-3.0, 0.05, 0.0),
        ],
        certificate: None,
        map_loader: None,
        autosave: None,
        packages: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_join_needs_only_the_game_port_and_errors_are_plain() -> Result<()> {
    use bri_net::client::JoinError;
    let server = server::start(session(), options())?;
    // No certificate yet: the first certificate is trusted and handed back
    // for pinning, with no discovery port involved.
    let first = Client::connect(server.address, &[], "First".into(), Vec::new(), None).await?;
    assert_eq!(first.certificate, server.certificate);
    first.close();
    // A pinned identity that no longer matches is named as such.
    let other = server::HostCertificate::generate()?;
    let error = Client::connect(
        server.address,
        &other.der,
        "Pinned".into(),
        Vec::new(),
        None,
    )
    .await
    .err()
    .context("Wrong pin accepted")?;
    assert!(
        matches!(
            error.downcast_ref::<JoinError>(),
            Some(JoinError::IdentityChanged(_))
        ),
        "{error:#}"
    );
    assert!(format!("{error}").contains("different identity"), "{error}");
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invites_pin_the_host_key_and_probes_read_the_listing() -> Result<()> {
    use bri_net::{
        client::{HostPin, JoinError, probe},
        invite::{JoinTarget, host_key, invite},
    };
    let mut server = server::start(session(), options())?;
    let lan = server
        .advertise_on(0, "Max's Server".into(), "Slate".into(), 12, "id".into())
        .await?;
    assert_ne!(lan, 0);
    // A probe over the game port reads the listing and the certificate.
    let seen = probe(server.address, &HostPin::FirstUse, Duration::from_secs(5)).await?;
    assert_eq!(seen.listing.name, "Max's Server");
    assert_eq!(seen.listing.map, "Slate");
    assert_eq!((seen.listing.players, seen.listing.max_players), (0, 12));
    assert_eq!(seen.certificate, server.certificate);
    // An invite's key is enough to join a host never seen before.
    let text = invite(server.address, &server.certificate);
    let route = JoinTarget::parse(&text)?.resolve().await?;
    assert_eq!(route.key, Some(host_key(&server.certificate)));
    let dir = tempfile::tempdir()?;
    let identity = ClientIdentity::load_or_create(dir.path().join("client.identity"))?;
    let joined = Client::connect_pinned(
        route.address,
        HostPin::Key(route.key.context("key")?),
        "Invited".into(),
        Vec::new(),
        None,
        None,
        &identity,
        Default::default(),
    )
    .await?;
    assert_eq!(joined.certificate, server.certificate);
    assert_eq!(joined.listing.name, "Max's Server");
    // The live count reaches later probes.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let busy = probe(
        server.address,
        &HostPin::Key(host_key(&server.certificate)),
        Duration::from_secs(5),
    )
    .await?;
    assert_eq!(busy.listing.players, 1);
    joined.close();
    // An invite for another host is refused as a changed identity.
    let stranger = server::HostCertificate::generate()?;
    let error = probe(
        server.address,
        &HostPin::Key(host_key(&stranger.der)),
        Duration::from_secs(5),
    )
    .await
    .err()
    .context("Wrong key accepted")?;
    assert!(
        matches!(
            error.downcast_ref::<JoinError>(),
            Some(JoinError::IdentityChanged(_))
        ),
        "{error:#}"
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_different_version_is_told_which_side_to_update() -> Result<()> {
    let server = server::start(session(), options())?;
    let mut config = quinn::ClientConfig::with_root_certificates(std::sync::Arc::new({
        let mut roots = quinn::rustls::RootCertStore::empty();
        roots.add(server.certificate.clone().into())?;
        roots
    }))?;
    config.transport_config(std::sync::Arc::new(server::transport()));
    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse()?)?;
    endpoint.set_default_client_config(config);
    let connection = endpoint.connect(server.address, "blockland.local")?.await?;
    let (mut send, mut receive) = connection.open_bi().await?;
    bri_net::codec::write_small_request(
        &mut send,
        &JoinBegin {
            version: VERSION - 1,
            ..JoinBegin::join()
        },
    )
    .await?;
    let answer = bri_net::codec::decode::<Message>(
        &bri_net::codec::read_frame(&mut receive, bri_net::codec::MAX_FRAME).await?,
    )?;
    let Message::Rejected(reason) = answer else {
        anyhow::bail!("Expected a refusal")
    };
    assert!(
        reason.contains("newer version") && reason.contains("Update your game"),
        "{reason}"
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn join_refusal_names_each_differing_shared_package() -> Result<()> {
    use bri_package::{
        environment::{Environment, PackageRef},
        packages::Side,
    };
    let package = |id: &str, side: Side, hash: u8| PackageRef {
        id: id.into(),
        version: "1.0.0".into(),
        side,
        hash: format!("{hash:02x}").repeat(32),
        size: 1,
    };
    let mut environment = Environment::empty();
    environment.packages = vec![
        package("v20-weapons", Side::Shared, 1),
        package("v20-ui", Side::Client, 2),
        package("v20-worlds", Side::Server, 3),
    ];
    let server = server::start(
        session(),
        ServerOptions {
            environment: environment.clone(),
            ..options()
        },
    )?;
    let connect = |packages: Vec<PackageRef>| {
        Client::connect(
            server.address,
            &server.certificate,
            "Joiner".into(),
            packages,
            None,
        )
    };
    // Different weapons and a mod the server lacks: refused, both named.
    let mut theirs = environment.client_packages();
    theirs[0].hash = "aa".repeat(32);
    theirs.push(package("creeper", Side::Shared, 9));
    let error = connect(theirs)
        .await
        .err()
        .context("Mismatched join accepted")?;
    let text = format!("{error:#}");
    assert!(text.contains("server has v20-weapons 1.0.0"), "{text}");
    assert!(text.contains("you have creeper 1.0.0"), "{text}");
    assert!(
        !text.contains("v20-ui") && !text.contains("v20-worlds"),
        "{text}"
    );
    // A different presentation package still joins; server-only packages
    // are never asked for.
    let mut cosmetic = environment.client_packages();
    cosmetic[1].hash = "bb".repeat(32);
    let mut client = connect(cosmetic).await?;
    // v20's "Welcome to Blockland" line arrives too; wait for the package one.
    let told = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let bri_net::client::ClientEvent::Notice(bri_sim::session::Notice::Chat(text)) =
                client.receive().await?
                && text.contains("server has")
            {
                return anyhow::Ok(text);
            }
        }
    })
    .await
    .context("waiting for the differing-package notice")??;
    assert!(told.contains("server has v20-ui 1.0.0"), "{told}");
    client.close();
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reliable_actions_capture_distinct_aim_without_movement_datagrams() -> Result<()> {
    use bri_sim::session::ActionAim;
    let server = server::start(session(), options())?;
    let mut client = Client::connect(
        server.address,
        &server.certificate,
        "Builder".into(),
        Vec::new(),
        None,
    )
    .await?;
    let mut ids = vec![];
    for z in [-3.25, 3.25] {
        let Reply::Planted(id) = client
            .command(Command::Plant {
                definition: "plate".into(),
                position: [0.5, 0.1, z],
                quarter_turns: 0,
                color: 0,
            })
            .await?
        else {
            panic!()
        };
        ids.push(id);
    }
    wait(&mut client, |c| c.replica.poses[&c.owner].player.grounded).await?;
    let eye = client.replica.poses[&client.owner]
        .player
        .eye(&bri_sim::player::PlayerTuning::default());
    let mut sequences = vec![];
    for z in [-3.25, 3.25] {
        let d = Vec3::new(0.5, 0.1, z) - eye;
        let aim = ActionAim {
            yaw: d.x.atan2(-d.z),
            pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
        };
        sequences.push(
            client
                .request_with_aim(Command::Activate, Some(aim))
                .await?,
        );
    }
    let mut replies = vec![];
    tokio::time::timeout(Duration::from_secs(5), async {
        while replies.len() < 2 {
            if let bri_net::client::ClientEvent::Reply { sequence, result } =
                client.receive().await?
            {
                let Reply::Activated(Some(brick_id)) = result.map_err(anyhow::Error::msg)? else {
                    panic!()
                };
                replies.push((sequence, brick_id));
            }
        }
        anyhow::Ok(())
    })
    .await
    .context("waiting for both activation replies")??;
    assert_eq!(
        replies,
        vec![(sequences[0], ids[0]), (sequences[1], ids[1])]
    );
    // No movement was sent: action snapshots did not become movement input.
    assert_eq!(client.replica.poses[&client.owner].acknowledged_input, 0);
    assert_eq!(client.replica.poses[&client.owner].player.yaw, 0.0);
    client.close();
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admin_roles_are_transport_authenticated_protected_and_reconnect_clean() -> Result<()> {
    use bri_admin::{Action, PasswordSlot, Request, Role, Secret};
    let server = server::start(session(), options())?;
    let mut host = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Host".into(),
        Vec::new(),
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    wait(&mut host, |c| c.admin_snapshot.is_some()).await?;
    assert_eq!(host.admin_snapshot.as_ref().unwrap().role, Role::SuperAdmin);

    // Password login needs a durable identity (failed guesses follow it).
    let guest_dir = tempfile::tempdir()?;
    let guest_identity = ClientIdentity::load_or_create(guest_dir.path().join("guest.identity"))?;
    let mut guest = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Guest".into(),
        Vec::new(),
        None,
        None,
        &guest_identity,
    )
    .await?;
    wait(&mut guest, |c| c.admin_snapshot.is_some()).await?;
    assert_eq!(guest.admin_snapshot.as_ref().unwrap().role, Role::Player);
    let guest_ticket = guest.resume.clone();

    let Reply::Planted(_) = guest
        .command(Command::Plant {
            definition: "plate".into(),
            position: [0.5, 0.1, -3.25],
            quarter_turns: 0,
            color: 0,
        })
        .await?
    else {
        panic!("fixture brick should plant")
    };
    let Reply::Admin(groups) = host
        .command(Command::Admin(Request::new(Action::RequestBrickGroups)))
        .await?
    else {
        panic!("group query returns typed rows")
    };
    assert!(matches!(
        groups.data,
        bri_sim::session::AdminData::BrickGroups(ref rows)
            if rows.iter().any(|row| row.id == guest.owner && row.bricks == 1)
    ));
    // Highlight flashes the group with Glow, then restores it.
    let Reply::Admin(_) = host
        .command(Command::Admin(Request::new(Action::HighlightBrickGroup {
            group: guest.owner,
        })))
        .await?
    else {
        panic!("authorized highlight is acknowledged")
    };
    wait(&mut host, |c| {
        c.replica.world.bricks.values().all(|b| b.color_effect == 3)
    })
    .await?;
    wait(&mut host, |c| {
        c.replica
            .world
            .bricks
            .values()
            .all(|b| b.color_effect == 0 && b.color == 0)
    })
    .await?;
    let Reply::Admin(_) = host
        .command(Command::Admin(Request::new(Action::ClearBrickGroup {
            group: guest.owner,
        })))
        .await?
    else {
        panic!("authorized group clear is acknowledged")
    };
    wait(&mut host, |c| c.replica.world.bricks.is_empty()).await?;

    let Reply::Admin(_) = host
        .command(Command::Admin(Request::new(Action::HostSetPassword {
            slot: PasswordSlot::Admin,
            password: Secret::new("admin-pass".into())?,
        })))
        .await?
    else {
        panic!("password change returns authoritative admin state")
    };

    let host_id = host
        .admin_snapshot
        .as_ref()
        .unwrap()
        .players
        .iter()
        .find(|p| p.owner)
        .unwrap()
        .connection;
    let denied = guest
        .command(Command::Admin(Request::new(Action::Kick {
            target: bri_admin::ConnectionId(host_id),
        })))
        .await
        .unwrap_err();
    assert!(denied.to_string().contains("administrator permission"));
    assert!(
        guest
            .command(Command::Admin(Request::new(Action::HostSetRole {
                target: bri_admin::ConnectionId(
                    guest
                        .admin_snapshot
                        .as_ref()
                        .unwrap()
                        .players
                        .iter()
                        .find(|p| p.connection != host_id)
                        .unwrap()
                        .connection
                ),
                role: Role::SuperAdmin,
            })))
            .await
            .is_err()
    );

    let Reply::Admin(login) = guest
        .command(Command::Admin(Request::new(Action::Login {
            password: Secret::new("admin-pass".into())?,
        })))
        .await?
    else {
        panic!("login returns authoritative admin state")
    };
    assert_eq!(login.snapshot.role, Role::Admin);
    wait(&mut guest, |c| c.administrator).await?;
    wait(&mut host, |c| {
        c.admin_snapshot.as_ref().is_some_and(|s| {
            s.players
                .iter()
                .any(|p| p.name == "Guest" && p.role == Role::Admin)
        })
    })
    .await?;

    let denied = guest
        .command(Command::Admin(Request::new(Action::Kick {
            target: bri_admin::ConnectionId(host_id),
        })))
        .await
        .unwrap_err();
    assert!(
        denied
            .to_string()
            .to_ascii_lowercase()
            .contains("protected")
    );
    let denied = guest
        .command(Command::Admin(Request::new(Action::Ban {
            target: bri_admin::ConnectionId(host_id),
            duration: bri_admin::BanDuration::Minutes(10),
            reason: "No trusted identity".into(),
        })))
        .await
        .unwrap_err();
    assert!(
        denied
            .to_string()
            .to_ascii_lowercase()
            .contains("protected")
    );

    let mut target = Client::connect(
        server.address,
        &server.certificate,
        "Target".into(),
        Vec::new(),
        None,
    )
    .await?;
    wait(&mut target, |c| {
        c.admin_snapshot
            .as_ref()
            .is_some_and(|s| s.players.iter().any(|p| p.name == "Target"))
    })
    .await?;
    let target_id = target
        .admin_snapshot
        .as_ref()
        .unwrap()
        .players
        .iter()
        .find(|p| p.name == "Target")
        .unwrap()
        .connection;
    let Reply::Admin(_) = guest
        .command(Command::Admin(Request::new(Action::Kick {
            target: bri_admin::ConnectionId(target_id),
        })))
        .await?
    else {
        panic!("authorized kick returns its confirmation")
    };
    // The kicked player learns why: the server's close frame, not a bare
    // "connection lost".
    let closed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Err(error) = target.receive().await {
                break format!("{error:#}");
            }
        }
    })
    .await
    .context("waiting for the kicked player's close")?;
    assert!(
        closed.contains("closed by peer: You were kicked"),
        "{closed}"
    );

    guest.close();
    wait(&mut host, |c| {
        c.admin_snapshot
            .as_ref()
            .is_some_and(|s| !s.players.iter().any(|p| p.name == "Guest"))
    })
    .await?;
    let mut resumed = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Guest".into(),
        Vec::new(),
        Some(guest_ticket),
        None,
        &guest_identity,
    )
    .await?;
    wait(&mut resumed, |c| c.admin_snapshot.is_some()).await?;
    assert_eq!(resumed.admin_snapshot.as_ref().unwrap().role, Role::Player);
    assert!(!resumed.administrator);

    host.close();
    resumed.close();
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fourth_failed_admin_password_closes_the_authenticated_connection() -> Result<()> {
    use bri_admin::{Action, PasswordSlot, Request, Secret};
    let server = server::start(session(), options())?;
    let mut host = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Host".into(),
        Vec::new(),
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    wait(&mut host, |c| c.admin_snapshot.is_some()).await?;
    host.command(Command::Admin(Request::new(Action::HostSetPassword {
        slot: PasswordSlot::Admin,
        password: Secret::new("correct".into())?,
    })))
    .await?;
    // Password login needs a durable identity (failed guesses follow it).
    let guest_dir = tempfile::tempdir()?;
    let guest_identity = ClientIdentity::load_or_create(guest_dir.path().join("guest.identity"))?;
    let mut guest = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Guessing client".into(),
        Vec::new(),
        None,
        None,
        &guest_identity,
    )
    .await?;
    wait(&mut guest, |c| c.admin_snapshot.is_some()).await?;
    for attempts in 1..=3 {
        let Reply::Admin(reply) = guest
            .command(Command::Admin(Request::new(Action::Login {
                password: Secret::new("wrong".into())?,
            })))
            .await?
        else {
            panic!("failed login should return its attempt count")
        };
        assert_eq!(
            reply.data,
            bri_sim::session::AdminData::LoginRejected { attempts }
        );
    }
    let _ = guest
        .request(Command::Admin(Request::new(Action::Login {
            password: Secret::new("wrong".into())?,
        })))
        .await?;
    wait(&mut host, |c| {
        c.admin_snapshot
            .as_ref()
            .is_some_and(|s| !s.players.iter().any(|p| p.name == "Guessing client"))
    })
    .await?;
    host.close();
    guest.close();
    server.stop().await?;
    Ok(())
}
/// Receive until `predicate` holds; a timeout names the waiting line.
#[track_caller]
fn wait<'a>(
    client: &'a mut Client,
    predicate: impl Fn(&Client) -> bool + 'a,
) -> impl std::future::Future<Output = Result<()>> + 'a {
    let at = std::panic::Location::caller();
    async move {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !predicate(client) {
                client.receive().await?;
            }
            Result::<()>::Ok(())
        })
        .await
        .with_context(|| format!("waiting at {at}"))?
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_capability_bulk_load_save_palette_late_join_and_resume() -> Result<()> {
    use bri_world::{Brick, ContentRef, build::SavedBuild};
    let server = server::start(session(), options())?;
    let mut guest = Client::connect(
        server.address,
        &server.certificate,
        "First local peer".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert!(
        !guest.administrator,
        "Local/first join must not confer host authority"
    );
    assert!(
        Client::connect_with_host(
            server.address,
            &server.certificate,
            "Bad host".into(),
            Vec::new(),
            None,
            Some(ResumeToken([42; 32]))
        )
        .await
        .is_err()
    );
    let mut host = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Host".into(),
        Vec::new(),
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    assert!(host.administrator);
    let mut source = World::new(
        "Loaded".into(),
        "source-map".into(),
        vec![[0.25, 0.5, 0.75, 1.0]],
    );
    let mut brick = Brick::new(
        ContentRef::Resolved("plate".into()),
        [0.5, 0.1, -3.25],
        guest.owner,
    );
    brick.name = Some("button".into());
    brick
        .events
        .push(color_row(EventTarget::Named("button".into()), 0));
    source.bricks.insert(99, brick);
    source.next_brick_id = 100;
    let build = SavedBuild::capture(&source, true, true)?;
    let load = |build: SavedBuild| Command::LoadBuild {
        build: Box::new(build),
        ownership: true,
    };
    assert!(guest.command(load(build.clone())).await.is_err());
    assert_eq!(
        host.command(load(build)).await?,
        Reply::Loaded { bricks: 1 }
    );
    wait(&mut guest, |c| c.replica.world.bricks.len() == 1).await?;
    let first = guest.replica.world.bricks[&1].clone();
    assert_ne!(first.owner, guest.owner);
    assert_ne!(first.owner, host.owner);
    assert_eq!(
        guest.replica.world.palette[first.color as usize],
        source.palette[0]
    );
    assert_eq!(first.events[0].params, vec![EventValue::Color(first.color)]);
    let Reply::Saved(saved) = guest
        .command(Command::SaveBuild {
            events: true,
            ownership: true,
        })
        .await?
    else {
        panic!("Missing build")
    };
    assert_eq!(saved.world.bricks[&1], first);
    assert!(
        saved.world.owners.is_empty(),
        "Imported owners stay unclaimed"
    );
    // Loaded beside the first copy: one on top of it would be skipped as
    // overlapping, as in v20.
    let mut saved = *saved;
    saved.world.bricks.get_mut(&1).unwrap().position[0] += 1.0;
    assert_eq!(
        host.command(load(saved)).await?,
        Reply::Loaded { bricks: 1 }
    );
    wait(&mut host, |c| c.replica.world.bricks.len() == 2).await?;
    wait(&mut guest, |c| c.replica.world.bricks.len() == 2).await?;
    // Owners without a principal have no identity to return to, so a
    // reload gives them a fresh, unclaimed number.
    let reloaded = host.replica.world.bricks[&2].clone();
    assert_ne!(reloaded.owner, first.owner);
    assert_eq!(
        Brick {
            owner: first.owner,
            position: first.position,
            ..reloaded
        },
        first
    );
    let late = Client::connect(
        server.address,
        &server.certificate,
        "Late".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert_ne!(late.owner, first.owner);
    assert_eq!(late.replica.world, guest.replica.world);
    let owner = host.owner;
    let token = host.resume.clone();
    host.close();
    drop(host);
    wait(&mut guest, |c| !c.replica.names.contains_key(&owner)).await?;
    let resumed = Client::connect(
        server.address,
        &server.certificate,
        "Resume".into(),
        Vec::new(),
        Some(token),
    )
    .await?;
    assert!(resumed.administrator);
    assert_eq!(resumed.owner, owner);
    drop(resumed);
    drop(late);
    drop(guest);
    let report = server.stop().await?;
    assert_eq!(report.native_world.bricks.len(), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn build_request_larger_than_old_frame_limit_crosses_real_quic() -> Result<()> {
    use bri_world::{Brick, ContentRef, SourceRecord, build::SavedBuild};
    let server = server::start(session(), options())?;
    let mut host = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Host".into(),
        Vec::new(),
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    let mut world = World::new("Large build".into(), "fixture".into(), vec![[1.0; 4]]);
    let mut brick = Brick::new(ContentRef::Resolved("plate".into()), [0.5, 0.1, -3.25], 1);
    // Preserved opaque data exercises the large native payload without thousands
    // of colliders hiding transport failures behind expensive simulation setup.
    brick.source_records = (0..270)
        .map(|line| SourceRecord {
            line,
            text: "x".repeat(65536),
            diagnostic: None,
        })
        .collect();
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    let build = SavedBuild::capture(&world, true, true)?;
    assert!(bri_world::build::encode(&build)?.len() > 16 * 1024 * 1024);
    assert_eq!(
        host.command(Command::LoadBuild {
            build: Box::new(build),
            ownership: false
        })
        .await?,
        Reply::Loaded { bricks: 1 }
    );
    // Loaded bricks are published on the next tick.
    wait(&mut host, |c| c.replica.world.bricks.len() == 1).await?;
    let Reply::Saved(saved) = host
        .command(Command::SaveBuild {
            events: true,
            ownership: true,
        })
        .await?
    else {
        panic!()
    };
    assert_eq!(
        saved.world.bricks[&1].source_records,
        world.bricks[&1].source_records
    );
    drop(host);
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires converted original avatar catalog"]
async fn original_avatar_changes_replicate_late_join_reject_invalid_and_resume() -> Result<()> {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
    let package: bri_content::avatar::Package =
        serde_json::from_slice(&std::fs::read(root.join("avatar.json"))?)?;
    let mut session = session();
    session.set_avatar_catalog(package.clone())?;
    let server = server::start(session, options())?;
    let mut a = Client::connect(
        server.address,
        &server.certificate,
        "A".into(),
        Vec::new(),
        None,
    )
    .await?;
    let mut b = Client::connect(
        server.address,
        &server.certificate,
        "B".into(),
        Vec::new(),
        None,
    )
    .await?;
    let owner = a.owner;
    let token = a.resume.clone();
    let mut appearance = package.defaults.clone();
    appearance
        .parts
        .insert("hat".into(), package.parts["hat"][1].clone());
    appearance.parts.insert("accent".into(), "visor".into());
    appearance.parts.insert("hip".into(), "skirthip".into());
    appearance.face = package.faces[1].clone();
    appearance
        .colors
        .insert("torso".into(), [0.2, 0.6, 0.8, 1.0]);
    a.command(Command::Avatar(appearance.clone())).await?;
    wait(&mut b, |c| {
        c.replica.avatars.get(&owner) == Some(&appearance)
    })
    .await?;
    assert_eq!(b.replica.avatars[&b.owner], package.defaults);
    let late = Client::connect(
        server.address,
        &server.certificate,
        "Late".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert_eq!(late.replica.avatars[&owner], appearance);
    let mut invalid = appearance.clone();
    invalid.face = "../../outside.png".into();
    assert!(a.command(Command::Avatar(invalid)).await.is_err());
    let mut invalid = appearance.clone();
    invalid.parts.insert("hat".into(), "nosuchhat".into());
    assert!(a.command(Command::Avatar(invalid)).await.is_err());
    let mut invalid = appearance.clone();
    invalid.colors.insert("lleg".into(), [1.1, 0.0, 0.0, 1.0]);
    assert!(a.command(Command::Avatar(invalid)).await.is_err());
    a.command(Command::Chat("Avatar edits rejected atomically".into()))
        .await?;
    assert_eq!(a.replica.avatars[&owner], appearance);
    drop(a);
    wait(&mut b, |c| !c.replica.names.contains_key(&owner)).await?;
    assert!(!b.replica.avatars.contains_key(&owner));
    let resumed = Client::connect(
        server.address,
        &server.certificate,
        "Ignored".into(),
        Vec::new(),
        Some(token),
    )
    .await?;
    assert_eq!(resumed.owner, owner);
    assert_eq!(resumed.replica.avatars[&owner], appearance);
    drop(resumed);
    drop(late);
    drop(b);
    server.stop().await?;
    Ok(())
}
async fn aim(client: &mut Client) -> Result<()> {
    wait(client, |c| c.replica.poses[&c.owner].player.grounded).await?;
    let p = &client.replica.poses[&client.owner].player;
    let direction = Vec3::new(0.5, 0.1, -3.25) - (Vec3::from(p.feet) + Vec3::Y * 2.4);
    let sequence = send_inputs(
        client,
        &[MoveInput {
            yaw: direction.x.atan2(-direction.z),
            pitch: direction
                .y
                .atan2(Vec3::new(direction.x, 0.0, direction.z).length()),
            ..Default::default()
        }],
    )?;
    wait(client, |c| {
        c.replica.poses[&c.owner].acknowledged_input >= sequence
    })
    .await
}
/// Equip a tool slot and swing it once at the current look, returning the
/// dialog the host opened (wrench or printer). The trigger is released after.
async fn swing(
    client: &mut Client,
    slot: usize,
) -> Result<Option<(u64, Box<bri_world::Brick>, InspectMode)>> {
    use bri_net::client::ClientEvent;
    use bri_sim::session::Notice;
    client
        .command(Command::EquipTool { slot: Some(slot) })
        .await?;
    // Triggers need a live movement stream; renew it at the current look.
    let p = &client.replica.poses[&client.owner].player;
    let look = MoveInput {
        yaw: p.yaw,
        pitch: p.pitch,
        ..Default::default()
    };
    send_inputs(client, &[look])?;
    let press = client
        .request(Command::WeaponTrigger { down: true })
        .await?;
    let opened = tokio::time::timeout(Duration::from_millis(750), async {
        loop {
            match client.receive().await? {
                ClientEvent::Reply {
                    sequence,
                    result: Err(error),
                } if sequence == press => anyhow::bail!("{error}"),
                ClientEvent::Notice(Notice::Inspected {
                    brick_id,
                    brick,
                    mode,
                }) => return Ok((brick_id, brick, mode)),
                _ => {}
            }
        }
    })
    .await
    .ok()
    .transpose()?;
    client
        .command(Command::WeaponTrigger { down: false })
        .await?;
    Ok(opened)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authoritative_cues_reach_two_peers_once_and_late_join_only_hears_new_actions() -> Result<()>
{
    use bri_sim::presentation::CueKind;
    let server = server::start(session(), options())?;
    let mut a = Client::connect(
        server.address,
        &server.certificate,
        "Cue builder".into(),
        Vec::new(),
        None,
    )
    .await?;
    let mut b = Client::connect(
        server.address,
        &server.certificate,
        "Cue listener".into(),
        Vec::new(),
        None,
    )
    .await?;
    let plant = |x| Command::Plant {
        definition: "plate".into(),
        position: [x, 0.1, -3.25],
        quarter_turns: 0,
        color: 0,
    };
    a.command(plant(0.5)).await?;
    // Planting sounds the plant cue and plays the builder's thread-3 `plant`.
    wait(&mut a, |c| c.replica.cue_cursor == 2).await?;
    wait(&mut b, |c| c.replica.cue_cursor == 2).await?;
    let events = a.replica.take_cues();
    assert_eq!(events, b.replica.take_cues());
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, CueKind::Plant);
    assert!(matches!(
        &events[1].kind,
        CueKind::WeaponAnimation { thread: 3, sequence, .. } if sequence == "plant"
    ));
    assert!(a.command(plant(0.5)).await.is_err());
    let mut late = Client::connect(
        server.address,
        &server.certificate,
        "Late listener".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert_eq!(late.replica.cue_cursor, 2);
    assert!(late.replica.take_cues().is_empty());
    a.command(plant(2.5)).await?;
    wait(&mut a, |c| c.replica.cue_cursor == 4).await?;
    wait(&mut b, |c| c.replica.cue_cursor == 4).await?;
    wait(&mut late, |c| c.replica.cue_cursor == 4).await?;
    let events = a.replica.take_cues();
    assert_eq!(events, b.replica.take_cues());
    assert_eq!(events, late.replica.take_cues());
    assert_eq!(events.len(), 2);
    // v20 refuses a jump tapped in the tick the feet land (canJump reads
    // contact from before the move), so wait until the jumper can jump.
    wait(&mut a, |c| {
        let p = &c.replica.poses[&c.owner].player;
        p.grounded && p.jump.delay == 0 && p.jump.since_contact == 0
    })
    .await?;
    // Movement is unreliable, so a real client repeats every input in each
    // frame's datagram until the host acknowledges it. Resend the jump
    // (always the same input, so the host takes it once) each frame.
    let jump = [MoveInput {
        jump: true,
        ..Default::default()
    }];
    let sequence = a.replica.poses[&a.owner].acknowledged_input + 1;
    // The jumper's state as it changes, and every cue either side hears,
    // for the failure message.
    let mut timeline = Vec::new();
    let mut last = None;
    let heard = tokio::time::timeout(Duration::from_secs(5), async {
        while b.replica.cue_cursor < 5 {
            if a.replica.poses[&a.owner].acknowledged_input < sequence {
                a.movement(sequence, &jump, None)?;
            }
            tokio::select! {
                received = b.receive() => { received?; }
                received = a.receive() => { received?; }
                () = tokio::time::sleep(Duration::from_millis(16)) => {}
            }
            let pose = &a.replica.poses[&a.owner];
            let p = &pose.player;
            let now = format!(
                "ack {} feet {:?} velocity {:?} grounded {} jump {:?} motor {:?}",
                pose.acknowledged_input, p.feet, p.velocity, p.grounded, p.jump, p.tick
            );
            if last.as_ref() != Some(&now) {
                timeline.push(format!("tick {} {now}", a.replica.tick));
                last = Some(now);
            }
            for (side, client) in [("jumper", &mut a), ("listener", &mut b)] {
                for cue in client.replica.take_cues() {
                    timeline.push(format!("{side} heard {:?} at tick {}", cue.kind, cue.tick));
                }
            }
        }
        anyhow::Ok(())
    })
    .await;
    heard
        .with_context(|| {
            format!(
                "waiting for the listener to hear the jump (input {sequence}):\n{}",
                timeline.join("\n")
            )
        })??;
    assert_eq!(b.replica.cue_cursor, 5);
    let jumps: Vec<_> = timeline
        .iter()
        .filter(|line| line.starts_with("listener heard"))
        .collect();
    assert_eq!(jumps.len(), 1, "{timeline:#?}");
    assert!(jumps[0].starts_with("listener heard Jump"), "{timeline:#?}");
    drop((a, b, late));
    server.stop().await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_quic_clients_build_late_join_and_resume_owned_bricks() -> Result<()> {
    let server = server::start(tool_session(), options())?;
    let mut a = Client::connect(
        server.address,
        &server.certificate,
        "A".into(),
        Vec::new(),
        None,
    )
    .await?;
    let mut b = Client::connect(
        server.address,
        &server.certificate,
        "B".into(),
        Vec::new(),
        None,
    )
    .await?;
    let owner = a.owner;
    let token = a.resume.clone();
    let Reply::Planted(id) = a
        .command(Command::Plant {
            definition: "plate".into(),
            position: [0.5, 0.1, -3.25],
            quarter_turns: 0,
            color: 0,
        })
        .await?
    else {
        panic!()
    };
    wait(&mut b, |c| c.replica.world.bricks.contains_key(&id)).await?;
    aim(&mut a).await?;
    aim(&mut b).await?;
    // B has no trust: its wrench opens nothing and its hammer leaves A's brick.
    assert!(swing(&mut b, 1).await?.is_none());
    assert!(swing(&mut b, 0).await?.is_none());
    let (opened, _, _) = swing(&mut a, 1)
        .await?
        .expect("owner's wrench opens the brick");
    assert_eq!(opened, id);
    a.command(Command::Chat("Native multiplayer".into()))
        .await?;
    wait(&mut a, |c| {
        c.replica.world.bricks.contains_key(&id) && c.replica.chat.len() == 1
    })
    .await?;
    wait(&mut b, |c| {
        c.replica.world.bricks.contains_key(&id) && c.replica.chat.len() == 1
    })
    .await?;
    let late = Client::connect(
        server.address,
        &server.certificate,
        "Late".into(),
        Vec::new(),
        None,
    )
    .await?;
    assert_eq!(late.replica.world, a.replica.world);
    assert_eq!(b.replica.world, a.replica.world);
    assert_eq!(late.replica.chat, a.replica.chat);
    assert!(
        Client::connect(
            server.address,
            &server.certificate,
            "Forged".into(),
            Vec::new(),
            Some(ResumeToken([0; 32]))
        )
        .await
        .is_err()
    );
    assert!(
        Client::connect(
            server.address,
            &server.certificate,
            "Mismatch".into(),
            vec![bri_package::environment::PackageRef {
                id: "other-content".into(),
                version: "1.0.0".into(),
                side: bri_package::packages::Side::Shared,
                hash: "cd".repeat(32),
                size: 1,
            }],
            None
        )
        .await
        .is_err()
    );
    a.close();
    drop(a);
    wait(&mut b, |c| !c.replica.names.contains_key(&owner)).await?;
    let mut resumed = Client::connect(
        server.address,
        &server.certificate,
        "Cannot rename via resume".into(),
        Vec::new(),
        Some(token),
    )
    .await?;
    assert_eq!(resumed.owner, owner);
    assert_eq!(resumed.replica.names[&owner], "A");
    aim(&mut resumed).await?;
    // The resumed owner still owns the brick, so the hammer breaks it.
    assert!(swing(&mut resumed, 0).await?.is_none());
    wait(&mut resumed, |c| !c.replica.world.bricks.contains_key(&id)).await?;
    drop(resumed);
    drop(b);
    drop(late);
    let report = server.stop().await?;
    assert_eq!(report.joins, 3);
    assert_eq!(report.resumes, 1);
    assert!(report.final_world.bricks.is_empty());
    // Forged resume and content mismatch; untrusted swings are misses, not rejections.
    assert!(report.rejected >= 2);
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_host_certificate_is_rejected_and_idle_input_stops() -> Result<()> {
    let server = server::start(session(), options())?;
    let unrelated = rcgen::generate_simple_self_signed(vec!["blockland.local".into()])?;
    assert!(
        Client::connect(
            server.address,
            unrelated.cert.der(),
            "Untrusted".into(),
            Vec::new(),
            None
        )
        .await
        .is_err()
    );
    let mut client = Client::connect(
        server.address,
        &server.certificate,
        "Mover".into(),
        Vec::new(),
        None,
    )
    .await?;
    wait(&mut client, |c| c.replica.poses[&c.owner].player.grounded).await?;
    // Half a second of held forward input, one input per prediction tick,
    // paced below the server's input burst budget.
    for _ in 0..2 {
        let sequence = send_inputs(
            &mut client,
            &[MoveInput {
                forward: 1.0,
                ..Default::default()
            }; 30],
        )?;
        wait(&mut client, |c| {
            c.replica.poses[&c.owner].acknowledged_input >= sequence
        })
        .await?;
    }
    let tick = client.replica.poses[&client.owner].tick;
    wait(&mut client, |c| c.replica.poses[&c.owner].tick > tick + 150).await?;
    let player = &client.replica.poses[&client.owner].player;
    assert!(player.feet[2] < -2.0 && player.feet[2] > -5.0);
    assert!(Vec3::from(player.velocity).length() < 0.01);
    drop(client);
    server.stop().await?;
    Ok(())
}

/// Send consecutive inputs after the last acknowledged one, in redundant-size
/// datagrams. Returns the final sequence.
fn send_inputs(client: &mut Client, inputs: &[MoveInput]) -> Result<u64> {
    let first = client.replica.poses[&client.owner].acknowledged_input + 1;
    for (i, chunk) in inputs
        .chunks(bri_net::protocol::MOVEMENT_REDUNDANCY)
        .enumerate()
    {
        let newest = first + (i * bri_net::protocol::MOVEMENT_REDUNDANCY + chunk.len()) as u64 - 1;
        client.movement(newest, chunk, None)?;
    }
    Ok(first + inputs.len() as u64 - 1)
}

async fn raw_identity_challenge(
    address: std::net::SocketAddr,
    certificate: &[u8],
) -> Result<(
    quinn::Endpoint,
    quinn::Connection,
    quinn::SendStream,
    quinn::RecvStream,
    [u8; 32],
)> {
    let mut roots = quinn::rustls::RootCertStore::empty();
    roots.add(certificate.to_vec().into())?;
    let mut config = quinn::ClientConfig::with_root_certificates(std::sync::Arc::new(roots))?;
    config.transport_config(std::sync::Arc::new(server::transport()));
    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse()?)?;
    endpoint.set_default_client_config(config);
    let connection = endpoint.connect(address, "blockland.local")?.await?;
    let (mut send, mut receive) = connection.open_bi().await?;
    bri_net::codec::write_small_request(&mut send, &JoinBegin::join()).await?;
    let challenge = bri_net::codec::decode::<Message>(
        &bri_net::codec::read_frame(&mut receive, bri_net::codec::MAX_FRAME).await?,
    )?;
    let Message::Challenge { nonce, .. } = challenge else {
        anyhow::bail!("Expected server identity challenge")
    };
    Ok((endpoint, connection, send, receive, nonce))
}

fn hello_for_proof(name: &str) -> Hello {
    Hello {
        version: VERSION,
        name: name.into(),
        packages: Vec::new(),
        resume: None,
        host: None,
        identity: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn identity_proofs_reject_forgery_and_replayed_challenges() -> Result<()> {
    let host = server::start(session(), options())?;
    let identity = ClientIdentity::load_or_create(
        std::env::temp_dir().join(format!("bri-proof-{}.key", std::process::id())),
    )?;
    let server_fingerprint: [u8; 32] = sha2::Sha256::digest(&host.certificate).into();

    let (endpoint1, connection1, mut send1, mut receive1, nonce1) =
        raw_identity_challenge(host.address, &host.certificate).await?;
    let mut forged = hello_for_proof("Forgery");
    let transcript1 = identity_transcript(&forged, &nonce1, &server_fingerprint)?;
    let mut signature = identity.sign(&transcript1)?.to_vec();
    signature[0] ^= 0x80;
    forged.identity = Some(IdentityProof {
        public_key: *identity.public_key(),
        signature,
    });
    bri_net::codec::write_small_request(&mut send1, &forged).await?;
    let rejected = bri_net::codec::decode::<Message>(
        &bri_net::codec::read_frame(&mut receive1, bri_net::codec::MAX_FRAME).await?,
    )?;
    assert!(matches!(rejected, Message::Rejected(message) if message.contains("proof failed")));
    drop((send1, receive1, connection1, endpoint1));

    let (endpoint2, connection2, mut send2, mut receive2, nonce2) =
        raw_identity_challenge(host.address, &host.certificate).await?;
    assert_ne!(nonce1, nonce2);
    let mut replay = hello_for_proof("Replay");
    let replay_transcript = identity_transcript(&replay, &nonce1, &server_fingerprint)?;
    replay.identity = Some(IdentityProof {
        public_key: *identity.public_key(),
        signature: identity.sign(&replay_transcript)?.to_vec(),
    });
    bri_net::codec::write_small_request(&mut send2, &replay).await?;
    let rejected = bri_net::codec::decode::<Message>(
        &bri_net::codec::read_frame(&mut receive2, bri_net::codec::MAX_FRAME).await?,
    )?;
    assert!(matches!(rejected, Message::Rejected(message) if message.contains("proof failed")));
    drop((send2, receive2, connection2, endpoint2));
    host.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn persistent_bans_bind_keys_survive_restart_and_unban() -> Result<()> {
    use bri_admin::{Action as AdminAction, BanDuration, BanId, DurableState, Request};

    let state_dir = tempfile::tempdir()?;
    let admin_file = state_dir.path().join("admin.json");
    let host_key = ClientIdentity::load_or_create(state_dir.path().join("host.identity"))?;
    let victim_key = ClientIdentity::load_or_create(state_dir.path().join("victim.identity"))?;
    let other_key = ClientIdentity::load_or_create(state_dir.path().join("other.identity"))?;

    let server = server::start_with_admin_store_and_limit(session(), options(), 7, &admin_file)?;
    assert!(
        Client::connect(
            server.address,
            &server.certificate,
            "Anonymous".into(),
            Vec::new(),
            None,
        )
        .await
        .is_err()
    );
    let mut host = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Host".into(),
        Vec::new(),
        None,
        Some(server.host_token.clone()),
        &host_key,
    )
    .await?;
    let victim = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Copied Name".into(),
        Vec::new(),
        None,
        None,
        &victim_key,
    )
    .await?;
    wait(&mut host, |client| {
        client.admin_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .players
                .iter()
                .any(|player| player.name == "Copied Name")
        })
    })
    .await?;
    let victim_owner = victim.owner;
    let victim_ticket = victim.resume.clone();
    drop(victim);
    wait(&mut host, |client| {
        !client.replica.names.contains_key(&victim_owner)
    })
    .await?;
    let revision_before_resume = host.admin_snapshot.as_ref().unwrap().revision;
    let victim = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Copied Name".into(),
        Vec::new(),
        Some(victim_ticket.clone()),
        None,
        &victim_key,
    )
    .await?;
    assert_eq!(victim.owner, victim_owner);
    wait(&mut host, |client| {
        client.admin_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.revision > revision_before_resume
                && snapshot
                    .players
                    .iter()
                    .filter(|player| player.name == "Copied Name")
                    .count()
                    == 1
        })
    })
    .await?;
    let victim_connection = host
        .admin_snapshot
        .as_ref()
        .unwrap()
        .players
        .iter()
        .find(|player| player.name == "Copied Name")
        .unwrap()
        .connection;

    let host_connection = host
        .admin_snapshot
        .as_ref()
        .unwrap()
        .players
        .iter()
        .find(|player| player.owner)
        .unwrap()
        .connection;
    let protected = host
        .command(Command::Admin(Request::new(AdminAction::Ban {
            target: bri_admin::ConnectionId(host_connection),
            duration: BanDuration::Forever,
            reason: "must not ban host".into(),
        })))
        .await
        .unwrap_err();
    assert!(protected.to_string().contains("protected"));

    let Reply::Admin(ban_reply) = host
        .command(Command::Admin(Request::new(AdminAction::Ban {
            target: bri_admin::ConnectionId(victim_connection),
            duration: BanDuration::Forever,
            reason: "verified target".into(),
        })))
        .await?
    else {
        panic!("ban must return admin reply")
    };
    assert!(matches!(ban_reply.data, AdminData::None));
    wait(&mut host, |client| {
        !client.replica.names.contains_key(&victim_owner)
    })
    .await?;
    let stored = DurableState::read(std::fs::File::open(&admin_file)?)?;
    assert_eq!(stored.bans.len(), 1);
    let expected_principal: [u8; 32] = sha2::Sha256::digest(victim_key.public_key()).into();
    assert_eq!(stored.bans[0].principal.0, expected_principal);
    let ban_id = stored.bans[0].id;

    let stolen_ticket = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Copied Name".into(),
        Vec::new(),
        Some(victim_ticket.clone()),
        None,
        &other_key,
    )
    .await;
    assert!(stolen_ticket.is_err());
    let banned_reconnect = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Renamed".into(),
        Vec::new(),
        Some(victim_ticket.clone()),
        None,
        &victim_key,
    )
    .await;
    assert!(banned_reconnect.is_err());

    // A copied name does not inherit the ban; only possession of the key does.
    let other = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Copied Name".into(),
        Vec::new(),
        None,
        None,
        &other_key,
    )
    .await?;
    drop(other);
    drop(host);
    drop(victim);
    server.stop().await?;

    let server = server::start_with_admin_store_and_limit(session(), options(), 7, &admin_file)?;
    let mut host = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Host".into(),
        Vec::new(),
        None,
        Some(server.host_token.clone()),
        &host_key,
    )
    .await?;
    let banned_after_restart = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Renamed".into(),
        Vec::new(),
        None,
        None,
        &victim_key,
    )
    .await;
    assert!(banned_after_restart.is_err());
    let Reply::Admin(list) = host
        .command(Command::Admin(Request::new(AdminAction::RequestBanList)))
        .await?
    else {
        panic!("ban list must return admin reply")
    };
    assert!(
        matches!(list.data, AdminData::BanList { rows, .. } if rows.len() == 1 && rows[0].id == ban_id)
    );
    let Reply::Admin(unban) = host
        .command(Command::Admin(Request::new(AdminAction::Unban {
            ban: BanId(ban_id.0),
        })))
        .await?
    else {
        panic!("unban must return admin reply")
    };
    assert!(matches!(unban.data, AdminData::None));
    assert!(
        DurableState::read(std::fs::File::open(&admin_file)?)?
            .bans
            .is_empty()
    );
    drop(host);
    server.stop().await?;

    let mut expired = DurableState {
        next_ban_id: 2,
        ..Default::default()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    expired.bans.push(bri_admin::BanRecord {
        id: BanId(1),
        principal: bri_admin::Principal(expected_principal),
        victim_name: "Previous name".into(),
        issued_by: "Host".into(),
        reason: "expired fixture".into(),
        created_unix_seconds: now.saturating_sub(120),
        expires_unix_seconds: Some(now.saturating_sub(60)),
    });
    let mut expired_bytes = Vec::new();
    expired.write(&mut expired_bytes)?;
    std::fs::write(&admin_file, expired_bytes)?;

    let server = server::start_with_admin_store_and_limit(session(), options(), 7, &admin_file)?;
    let mut host = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Host".into(),
        Vec::new(),
        None,
        Some(server.host_token.clone()),
        &host_key,
    )
    .await?;
    let Reply::Admin(expired_list) = host
        .command(Command::Admin(Request::new(AdminAction::RequestBanList)))
        .await?
    else {
        panic!("expired ban list must return admin reply")
    };
    assert!(matches!(expired_list.data, AdminData::BanList { rows, .. } if rows.is_empty()));
    let recovered = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Renamed".into(),
        Vec::new(),
        None,
        None,
        &victim_key,
    )
    .await?;
    drop(recovered);
    drop(host);
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lan_discovery_advertises_listing_and_joinable_certificate() -> Result<()> {
    let mut server = server::start(session(), options())?;
    // A free port, so a running host on the real discovery port cannot collide.
    let port = server
        .advertise_on(
            0,
            "LAN Host".into(),
            "Fixture".into(),
            8,
            "fixture-v1".into(),
        )
        .await?;
    let responder = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let found = bri_net::discovery::query(&[responder], Duration::from_millis(1500)).await?;
    let (address, beacon) = found
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no LAN reply"))?;
    assert_eq!(address.port(), server.address.port());
    assert_eq!(beacon.name, "LAN Host");
    assert_eq!(beacon.max_players, 8);
    let certificate = beacon.certificate_der()?;
    assert_eq!(certificate, server.certificate);
    // The advertised certificate is enough to join.
    let mut client =
        Client::connect(address, &certificate, "Finder".into(), Vec::new(), None).await?;
    client.command(Command::Chat("found you".into())).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let found = bri_net::discovery::query(&[responder], Duration::from_millis(1500)).await?;
    assert_eq!(found[0].1.players, 1, "listing reports connected players");
    drop(client);
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn minigame_listing_replicates_to_other_clients_and_late_joiners() -> Result<()> {
    use bri_sim::session::MiniGameRequest;
    let server = server::start(session(), options())?;
    let connect = |name: &str| {
        Client::connect(
            server.address,
            &server.certificate,
            name.into(),
            Vec::new(),
            None,
        )
    };
    let mut owner = connect("Owner").await?;
    let mut other = connect("Other").await?;
    let settings = bri_minigames::Settings {
        title: "Deathmatch".into(),
        ..Default::default()
    };
    owner
        .command(Command::MiniGame(MiniGameRequest::Create {
            color: 2,
            settings,
        }))
        .await?;
    wait(&mut other, |c| c.replica.minigames.len() == 1).await?;
    assert_eq!(other.replica.minigames[0].owner, owner.owner);
    let late = connect("Late").await?;
    assert_eq!(late.replica.minigames.len(), 1);
    other
        .command(Command::MiniGame(MiniGameRequest::Join {
            game: other.replica.minigames[0].id,
        }))
        .await?;
    let other_id = other.owner;
    wait(&mut owner, |c| {
        c.replica
            .minigames
            .first()
            .is_some_and(|g| g.members.contains(&other_id))
    })
    .await?;
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admin_change_map_moves_every_client_to_the_new_world() -> Result<()> {
    use bri_admin::{Action, Request};
    use bri_sim::session::{MapListing, Reply};
    let maps = vec![
        MapListing {
            id: "fixture".into(),
            name: "Fixture".into(),
        },
        MapListing {
            id: "other".into(),
            name: "Other".into(),
        },
    ];
    let mut game = session();
    game.set_spawn_points(options().spawn_points)?;
    game.set_map_list(maps.clone())?;
    let saved = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log = saved.clone();
    let mut opts = options();
    // The timer never fires in this test; only the map change saves.
    opts.autosave = Some(server::Autosave {
        every: Duration::from_secs(3600),
        save: std::sync::Arc::new(move |world: &World| {
            log.lock().unwrap().push(world.name.clone());
            Ok(())
        }),
    });
    opts.map_loader = Some(std::sync::Arc::new(move |map: &str| {
        let mut next = session();
        next.set_spawn_points(vec![
            Vec3::new(10.0, 0.05, 10.0),
            Vec3::new(13.0, 0.05, 10.0),
        ])?;
        next.set_map_list(maps.clone())?;
        anyhow::ensure!(map == "other", "unexpected map");
        Ok(next)
    }));
    let server = server::start(game, opts)?;
    let mut admin = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Admin".into(),
        Vec::new(),
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    let mut guest = Client::connect(
        server.address,
        &server.certificate,
        "Guest".into(),
        Vec::new(),
        None,
    )
    .await?;
    let guest_id = guest.owner;
    guest.command(Command::Chat("before".into())).await?;
    match admin
        .command(Command::Admin(Request::new(Action::RequestMaps)))
        .await?
    {
        Reply::Admin(reply) => assert!(matches!(
            reply.data,
            AdminData::Maps(ref rows) if rows.len() == 2
        )),
        other => anyhow::bail!("unexpected reply {other:?}"),
    }
    admin
        .command(Command::Admin(Request::new(Action::ChangeMap {
            map: "other".into(),
        })))
        .await?;
    let moved = |c: &Client| {
        c.replica
            .chat
            .iter()
            .any(|l| l.text.contains("changed the map to"))
            && c.replica.names.len() == 2
    };
    wait(&mut guest, moved).await?;
    wait(&mut admin, moved).await?;
    // The world being left was saved before the new one replaced it.
    assert_eq!(*saved.lock().unwrap(), ["Loopback".to_string()]);
    assert_eq!(guest.owner, guest_id);
    // Players keep their identity and chat history and can act on the new map.
    assert!(guest.replica.chat.iter().any(|l| l.text == "before"));
    guest.command(Command::Chat("after".into())).await?;
    wait(&mut admin, |c| {
        c.replica.chat.iter().any(|l| l.text == "after")
    })
    .await?;
    wait(&mut admin, |c| {
        c.replica
            .poses
            .get(&guest_id)
            .is_some_and(|p| p.player.feet[2] > 5.0)
    })
    .await?;
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn join_reports_the_world_download_in_bricks() -> Result<()> {
    use bri_progress::{Progress, Stage, Unit};
    let state_dir = tempfile::tempdir()?;
    let key = ClientIdentity::load_or_create(state_dir.path().join("joiner.identity"))?;
    let server = server::start_with_admin_store_and_limit(
        session(),
        options(),
        4,
        state_dir.path().join("admin.json"),
    )?;
    let progress = Progress::new();
    let _client = Client::connect_reporting(
        server.address,
        &server.certificate,
        "Joiner".into(),
        Vec::new(),
        None,
        None,
        &key,
        progress.clone(),
    )
    .await?;
    let seen = progress.snapshot();
    assert_eq!(
        (seen.stage, seen.unit),
        (Stage::ReceivingWorld, Unit::Bricks)
    );
    assert_eq!(Some(seen.done), seen.total);
    assert_eq!(seen.fraction(), 1.0);
    Ok(())
}

/// A client that is slow to drain its events (a busy frame thread, a test
/// between steps) must not be disconnected by unreliable pose traffic
/// crowding out reliable messages. Regression for the flaky
/// `fourth_failed_admin_password_closes_the_authenticated_connection`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unread_pose_datagrams_never_block_reliable_delivery() -> Result<()> {
    let server = server::start(session(), options())?;
    let mut first = Client::connect(
        server.address,
        &server.certificate,
        "First".into(),
        Vec::new(),
        None,
    )
    .await?;
    let _second = Client::connect(
        server.address,
        &server.certificate,
        "Second".into(),
        Vec::new(),
        None,
    )
    .await?;
    // Two players' poses at 40 Hz each outrun any event queue in seconds.
    tokio::time::sleep(Duration::from_secs(6)).await;
    let reply = first.command(Command::Chat("still here".into())).await;
    assert!(reply.is_ok(), "{reply:?}");
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_autosaves_on_its_timer_and_returns_its_final_world() -> Result<()> {
    let saved = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u64>::new()));
    let log = saved.clone();
    let server = server::start(
        session(),
        ServerOptions {
            autosave: Some(server::Autosave {
                every: Duration::from_secs(1),
                save: std::sync::Arc::new(move |world: &World| {
                    log.lock().unwrap().push(world.revision);
                    Ok(())
                }),
            }),
            ..options()
        },
    )?;
    tokio::time::sleep(Duration::from_millis(2300)).await;
    let report = server.stop().await?;
    let saves = saved.lock().unwrap().len() as u64;
    assert!(saves >= 1, "the timer autosaved");
    assert_eq!(report.autosaves, saves);
    assert_eq!(report.autosave_failures, 0);
    // A clean stop hands back the world for the caller to keep.
    assert_eq!(
        report.native_world.map_id,
        session().simulation().state().map_id
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ghost_bricks_replicate_to_other_players_and_leave_with_the_bricks() -> Result<()> {
    use bri_sim::session::{BrickHand, GhostBrick};
    let server = server::start(session(), options())?;
    let connect = |name: &str| {
        Client::connect(
            server.address,
            &server.certificate,
            name.into(),
            Vec::new(),
            None,
        )
    };
    let mut builder = connect("Builder").await?;
    let mut other = connect("Other").await?;
    let id = builder.owner;
    let hand = |equipped: bool, ghost: bool| {
        Command::BrickHand(BrickHand {
            stocked: true,
            equipped,
            ghost,
        })
    };
    let ghost = |x: f32, quarter_turns: u8| GhostBrick {
        definition: "plate".into(),
        position: [x, 0.1, 2.0],
        quarter_turns,
        color: 1,
        print: None,
    };
    builder.command(hand(true, true)).await?;
    builder
        .command(Command::GhostBrick(Some(ghost(1.0, 0))))
        .await?;
    wait(&mut other, |c| {
        c.replica.vitals.get(&id).and_then(|v| v.ghost.as_ref()) == Some(&ghost(1.0, 0))
    })
    .await?;
    // Moves and turns follow; a late joiner sees where it is now.
    builder
        .command(Command::GhostBrick(Some(ghost(1.5, 3))))
        .await?;
    wait(&mut other, |c| {
        c.replica.vitals.get(&id).and_then(|v| v.ghost.as_ref()) == Some(&ghost(1.5, 3))
    })
    .await?;
    let late = connect("Late").await?;
    assert_eq!(late.replica.vitals[&id].ghost, Some(ghost(1.5, 3)));
    // Unknown bricks and bad positions are refused and change nothing.
    let mut unknown = ghost(0.0, 0);
    unknown.definition = "no-such-brick".into();
    assert!(
        builder
            .command(Command::GhostBrick(Some(unknown)))
            .await
            .is_err()
    );
    let mut far = ghost(0.0, 0);
    far.position[0] = f32::NAN;
    assert!(
        builder
            .command(Command::GhostBrick(Some(far)))
            .await
            .is_err()
    );
    // Putting the bricks away takes the ghost with them.
    builder.command(hand(false, true)).await?;
    wait(&mut other, |c| {
        c.replica.vitals.get(&id).is_some_and(|v| v.ghost.is_none())
    })
    .await?;
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn look_pitch_and_head_turn_reach_other_players() -> Result<()> {
    let server = server::start(session(), options())?;
    let connect = |name: &str| {
        Client::connect(
            server.address,
            &server.certificate,
            name.into(),
            Vec::new(),
            None,
        )
    };
    let mut looker = connect("Looker").await?;
    let mut other = connect("Other").await?;
    let id = looker.owner;
    let input = MoveInput {
        yaw: 0.4,
        pitch: -0.7,
        head_yaw: 1.2,
        ..Default::default()
    };
    send_inputs(&mut looker, &[input; 4])?;
    wait(&mut other, |c| {
        c.replica.poses.get(&id).is_some_and(|p| {
            (p.player.pitch + 0.7).abs() < 1e-4 && (p.player.head_yaw - 1.2).abs() < 1e-4
        })
    })
    .await?;
    server.stop().await?;
    Ok(())
}
