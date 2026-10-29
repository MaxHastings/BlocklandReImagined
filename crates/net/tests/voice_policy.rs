//! Real package policy composed against authoritative session membership.
mod common;
use bri_net::voice;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::session::{Command, MiniGameRequest, PackageArg, PackageCommand};
use glam::Vec3;
use std::{path::PathBuf, sync::Arc};

fn session(radio: bool) -> bri_sim::session::Session {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages");
    let mut packages = vec![PackageEntry {
        id: "voice_chat".into(),
        version: "1.0.0".into(),
        side: Side::Server,
        dir: "voice_chat".into(),
        role: None,
    }];
    if radio {
        packages.push(PackageEntry {
            id: "sample-radio".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "samples/sample-radio".into(),
            role: None,
        });
    }
    if radio {
        packages.push(PackageEntry {
            id: "sample-radio-controls".into(),
            version: "1.0.0".into(),
            side: Side::Client,
            dir: "samples/sample-radio-controls".into(),
            role: None,
        });
    }
    let catalog = Catalog::load(
        &root,
        &PackageSet {
            schema_version: 1,
            packages,
        },
        true,
    )
    .unwrap_or_else(|e| panic!("{e:#?}"));
    let mut s = common::session();
    s.install_packages(Arc::new(catalog), None).unwrap();
    s
}
fn cmd(s: &mut bri_sim::session::Session, id: u64, sequence: u64, c: Command) {
    s.command(id, sequence, c).unwrap();
}
fn radio(
    s: &mut bri_sim::session::Session,
    id: u64,
    sequence: u64,
    command: &str,
    arg: PackageArg,
) {
    cmd(
        s,
        id,
        sequence,
        Command::Package(PackageCommand {
            package: "sample-radio".into(),
            command: command.into(),
            args: vec![arg],
        }),
    );
}
#[test]
fn voice_default_groups_and_radio_compose_without_duplicate_or_cross_group_delivery() {
    let mut s = session(true);
    let a = s
        .join("A".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let b = s
        .join("B".into(), Vec3::new(95.0, 0.05, 0.0), false)
        .unwrap();
    let c = s
        .join("C".into(), Vec3::new(-95.0, 0.05, 0.0), false)
        .unwrap();
    let owners = [a, b, c];
    let routes = s.voice_routes(&owners);
    assert!(
        !routes[&b][&a].spatial,
        "outside voice is global, including distant builders"
    );
    cmd(
        &mut s,
        a,
        1,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Default::default(),
        }),
    );
    let game = s.minigame_views()[0].id;
    cmd(
        &mut s,
        b,
        1,
        Command::MiniGame(MiniGameRequest::Join { game }),
    );
    let routes = s.voice_routes(&owners);
    assert!(routes[&b][&a].spatial);
    assert!(
        !routes.get(&c).is_some_and(|r| r.contains_key(&a)),
        "outside listener is isolated"
    );
    assert!(
        !routes.get(&a).is_some_and(|r| r.contains_key(&c)),
        "outside speaker cannot broadcast into match"
    );
    radio(&mut s, a, 2, "radio_transmit", PackageArg::Bool(true));
    let routes = s.voice_routes(&owners);
    assert!(!routes[&b][&a].spatial);
    assert_eq!(routes[&b][&a].label, "Radio 1");
    assert_eq!(routes[&b].keys().filter(|id| **id == a).count(), 1);
    radio(&mut s, b, 2, "radio_channel", PackageArg::Int(2));
    assert!(
        !s.voice_routes(&owners)
            .get(&b)
            .is_some_and(|r| r.contains_key(&a)),
        "channel mismatch suppresses base route during radio transmission"
    );
    radio(&mut s, a, 3, "radio_transmit", PackageArg::Bool(false));
    assert!(s.voice_routes(&owners)[&b][&a].spatial);
    cmd(&mut s, b, 3, Command::MiniGame(MiniGameRequest::Leave));
    let routes = s.voice_routes(&owners);
    assert!(!routes.get(&b).is_some_and(|r| r.contains_key(&a)));
    assert!(!routes[&b][&c].spatial);
    assert!(
        s.take_package_problems().is_empty(),
        "default and radio scripts must run without fallback"
    );
}
#[test]
fn voice_revision_audio_and_activity_are_stamped_together() {
    let audio = voice::encode_routed(9, [1.0, 2.0, 3.0], 4, &[7; 60], 81);
    assert_eq!(voice::decode_downstream(&audio).unwrap().revision, 81);
    let activity = voice::encode_routed_activity(9, true, 81);
    assert_eq!(
        voice::decode_downstream_activity(&activity)
            .unwrap()
            .revision,
        81
    );
    assert!(voice::decode_downstream(&audio[..31]).is_none());
}

#[test]
fn voice_minigame_settings_and_death_policy_use_existing_authority() {
    use bri_package::setting::SettingValue;
    use bri_sim::session::SettingEdit;
    let mut s = session(false);
    let a = s
        .join("Owner".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let b = s
        .join("Member".into(), Vec3::new(4.0, 0.05, 0.0), false)
        .unwrap();
    cmd(
        &mut s,
        a,
        1,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Default::default(),
        }),
    );
    let game = s.minigame_views()[0].id;
    cmd(
        &mut s,
        b,
        1,
        Command::MiniGame(MiniGameRequest::Join { game }),
    );
    cmd(
        &mut s,
        a,
        2,
        Command::MiniGame(MiniGameRequest::AddOnSettings {
            game,
            settings: vec![
                SettingEdit {
                    key: "voice_chat:mode".into(),
                    value: Some(SettingValue::Text("global".into())),
                },
                SettingEdit {
                    key: "voice_chat:range".into(),
                    value: Some(SettingValue::Int(128)),
                },
            ],
            teams: None,
            quiet: true,
            reset: false,
        }),
    );
    let routes = s.voice_routes(&[a, b]);
    assert!(!routes[&b][&a].spatial);
    assert_eq!(routes[&b][&a].range, 128.0);
    cmd(&mut s, a, 3, Command::Suicide);
    let routes = s.voice_routes(&[a, b]);
    assert!(
        !routes.get(&b).is_some_and(|r| r.contains_key(&a)),
        "dead members do not broadcast to living members"
    );
    assert!(
        routes[&a].contains_key(&b),
        "dead members still hear living teammates"
    );
}

#[test]
fn voice_policy_requires_authorization_and_queries_cannot_write() {
    use serde_json::json;
    for authorized in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("custom-voice");
        std::fs::create_dir(&dir).unwrap();
        let manifest = json!({"schema_version":1,"id":"custom-voice","version":"1.0.0","api":1,"name":"Custom Voice","license":"CC0-1.0","capabilities":if authorized {vec!["voice"]} else {vec![]},"provides":[
            {"kind":"behaviour","id":"custom-voice:behaviour/rules","file":"behaviour.json"},
            {"kind":"script","id":"custom-voice:script/rules","file":"rules.rhai"}]});
        std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
        std::fs::write(dir.join("behaviour.json"),r#"{"schema_version":1,"script":"rules.rhai","on_voice":true,"state":{"global":{"changed":{"default":false,"visible":"server"}}}}"#).unwrap();
        std::fs::write(
            dir.join("rules.rhai"),
            r#"fn voice_routes(speaker,listeners) { set("changed",true); [] }"#,
        )
        .unwrap();
        let set = PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "custom-voice".into(),
                version: "1.0.0".into(),
                dir: "custom-voice".into(),
                side: Side::Server,
                role: None,
            }],
        };
        let catalog = Catalog::load(tmp.path(), &set, true).unwrap();
        let mut s = common::session();
        let installed = s.install_packages(Arc::new(catalog), None);
        if !authorized {
            assert!(installed.is_err());
            continue;
        }
        installed.unwrap();
        assert!(s.voice_chat(), "voice enablement has no special package ID");
        let a = s
            .join("A".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        assert!(s.voice_routes(&[a]).is_empty());
        assert_eq!(
            s.package_store()
                .unwrap()
                .global("custom-voice", "changed")
                .unwrap()
                .clone(),
            json!(false)
        );
        let problems = s.package_diagnostics();
        assert!(
            problems.iter().any(|d| d.code == "script.read_only"),
            "{problems:#?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn voice_real_network_revokes_and_regrants_private_routes_on_membership_change()
-> anyhow::Result<()> {
    use bri_net::{client::Client, server};
    use std::time::Duration;
    async fn wait(client: &mut Client, speaker: u64, allowed: bool) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let table = client.voice_link().routes();
                if table.revision > 0 && table.routes.contains_key(&speaker) == allowed {
                    return anyhow::Ok(());
                }
                client.receive().await?;
            }
        })
        .await??;
        Ok(())
    }
    let server = server::start(session(false), common::options())?;
    let mut a = Client::connect(
        server.address,
        &server.certificate,
        "A".into(),
        vec![],
        None,
    )
    .await?;
    let mut b = Client::connect(
        server.address,
        &server.certificate,
        "B".into(),
        vec![],
        None,
    )
    .await?;
    wait(&mut b, a.owner, true).await?;
    let first = b.voice_link().routes().routes[&a.owner].generation;
    a.command(Command::MiniGame(MiniGameRequest::Create {
        color: 0,
        settings: Default::default(),
    }))
    .await?;
    wait(&mut b, a.owner, false).await?;
    let listen = b.voice_link();
    let mut heard = vec![];
    listen.take_heard(&mut heard);
    heard.clear();
    for sequence in 0..5 {
        a.voice_link().send(sequence, &[7; 60])?;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    listen.take_heard(&mut heard);
    assert!(
        heard.is_empty(),
        "outside player receives no minigame audio"
    );
    a.command(Command::MiniGame(MiniGameRequest::Leave)).await?;
    wait(&mut b, a.owner, true).await?;
    assert_ne!(
        b.voice_link().routes().routes[&a.owner].generation,
        first,
        "regrant cannot accept old datagrams"
    );
    server.stop().await?;
    Ok(())
}

#[test]
fn voice_roster_is_independent_of_the_concurrent_mixer_limit() {
    let mut s = session(false);
    let owners: Vec<_> = (0..33)
        .map(|i| {
            s.join(
                format!("Player{i}"),
                Vec3::new(i as f32 * 3.0, 0.05, 0.0),
                false,
            )
            .unwrap()
        })
        .collect();
    let routes = s.voice_routes(&owners);
    assert_eq!(routes.len(), 33);
    assert!(routes.values().all(|r| r.len() == 32));
    assert!(s.package_diagnostics().is_empty());
}
