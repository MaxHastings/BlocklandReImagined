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
    let audio = voice::encode_routed(9, Some([1.0, 2.0, 3.0]), 4, &[7; 60], 81);
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
    assert!(routes[&b].contains_key(&a), "Global includes dead speakers");
    assert!(
        routes[&a].contains_key(&b),
        "Global includes living speakers"
    );
    for (i, mode) in ["proximity", "team"].into_iter().enumerate() {
        cmd(
            &mut s,
            a,
            4 + i as u64,
            Command::MiniGame(MiniGameRequest::AddOnSettings {
                game,
                settings: vec![SettingEdit {
                    key: "voice_chat:mode".into(),
                    value: Some(SettingValue::Text(mode.into())),
                }],
                teams: None,
                quiet: true,
                reset: false,
            }),
        );
        let routes = s.voice_routes(&[a, b]);
        assert!(
            !routes.get(&b).is_some_and(|r| r.contains_key(&a)),
            "{mode} retains its dead-to-living restriction"
        );
        assert!(
            routes[&a].contains_key(&b),
            "dead listeners still hear living members in {mode}"
        );
    }
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

fn authored_voice_session(
    script: &str,
    base_voice: bool,
) -> (bri_sim::session::Session, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("custom-voice");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        serde_json::json!({
            "schema_version":1,"id":"custom-voice","version":"1.0.0","api":1,
            "name":"Voice parameter fixture","license":"CC0-1.0","capabilities":["voice"],
            "provides":[
                {"kind":"behaviour","id":"custom-voice:behaviour/rules","file":"behaviour.json"},
                {"kind":"script","id":"custom-voice:script/rules","file":"rules.rhai"}]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("behaviour.json"),
        r#"{"schema_version":1,"script":"rules.rhai","on_voice":true,
          "commands":[{"name":"fixture_channel","args":["int"],"while_dead":true}],
          "state":{"player":{"channel":{"default":1,"visible":"server"}}}}"#,
    )
    .unwrap();
    let command = r#"fn cmd_fixture_channel(id, value) { set_player(id, "channel", value); }"#;
    std::fs::write(dir.join("rules.rhai"), [command, script].join("\n")).unwrap();
    let mut packages = vec![PackageEntry {
        id: "custom-voice".into(),
        version: "1.0.0".into(),
        dir: "custom-voice".into(),
        side: Side::Server,
        role: None,
    }];
    if base_voice {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/voice_chat");
        let destination = tmp.path().join("voice_chat");
        std::fs::create_dir(&destination).unwrap();
        for file in std::fs::read_dir(base).unwrap() {
            let file = file.unwrap();
            if file.file_type().unwrap().is_file() {
                std::fs::copy(file.path(), destination.join(file.file_name())).unwrap();
            }
        }
        packages.push(PackageEntry {
            id: "voice_chat".into(),
            version: "1.0.0".into(),
            dir: "voice_chat".into(),
            side: Side::Server,
            role: None,
        });
    }
    let catalog = Catalog::load(
        tmp.path(),
        &PackageSet {
            schema_version: 1,
            packages,
        },
        true,
    )
    .unwrap();
    let mut s = common::session();
    s.install_packages(Arc::new(catalog), None).unwrap();
    (s, tmp)
}

#[test]
fn authorized_voice_effects_are_native_parameters_and_invalid_routes_report_problems() {
    use bri_sim::session::VoiceEffects;
    for (field, expected) in [
        ("", Some(VoiceEffects::default())),
        (
            "effects: #{ gain: 0.7, high_pass_hz: 300.0, low_pass_hz: 3000.0 },",
            Some(VoiceEffects {
                gain: 0.7,
                high_pass_hz: 300.0,
                low_pass_hz: 3000.0,
            }),
        ),
        ("effects: #{ gain: 2.1 },", None),
        ("effects: #{ high_pass_hz: 10.0 },", None),
        (
            "effects: #{ high_pass_hz: 3000.0, low_pass_hz: 1000.0 },",
            None,
        ),
        ("effects: #{ arbitrary_plugin: 1 },", None),
    ] {
        let script = r#"
            fn voice_routes(speaker, listeners) {
                let routes = [];
                for id in listeners {
                    if id != speaker {
                        routes.push(#{ listener:id, spatial:false, range:48.0,
                            label:"Authored channel", priority:10, EFFECTS });
                    }
                }
                routes
            }
        "#
        .replace("EFFECTS", field);
        let (mut s, _tmp) = authored_voice_session(&script, false);
        let a = s
            .join("Speaker".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        let b = s
            .join("Listener".into(), Vec3::new(4.0, 0.05, 0.0), false)
            .unwrap();
        let routes = s.voice_routes(&[a, b]);
        if let Some(expected) = expected {
            assert_eq!(routes[&b][&a].effects, expected, "{field}");
            assert!(s.package_diagnostics().is_empty());
        } else {
            assert!(routes.is_empty(), "{field}");
            assert!(
                s.package_diagnostics()
                    .iter()
                    .any(|p| p.code == "voice.routes" || p.code == "voice.playback"),
                "{field}"
            );
        }
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
    // Global frames carry no physical coordinates, despite both actors having bodies.
    a.voice_link().send(0, &[7; 60])?;
    let mut global = vec![];
    tokio::time::timeout(Duration::from_secs(3), async {
        while global.is_empty() {
            b.voice_link().take_heard(&mut global);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    assert!(
        global
            .iter()
            .all(|frame| frame.position.is_none() && frame.revision == first)
    );
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
    for sequence in 1..6 {
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

async fn fresh_voice_grant(
    client: &mut bri_net::client::Client,
    speaker: u64,
    previous: Option<u64>,
) -> anyhow::Result<u64> {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if let Some(route) = client.voice_link().routes().routes.get(&speaker)
                && Some(route.generation) != previous
            {
                return Ok(route.generation);
            }
            client.receive().await?;
        }
    })
    .await?
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn voice_identical_playback_gets_fresh_grants_when_either_conversation_changes()
-> anyhow::Result<()> {
    use bri_net::{client::Client, server};
    use bri_sim::session::TeamEdit;
    use std::time::Duration;
    let (s, _tmp) = authored_voice_session(
        r#"
        fn voice_routes(speaker, listeners) {
            let routes = [];
            for id in listeners {
                if id != speaker { routes.push(#{ listener:id, spatial:false, range:48.0,
                    label:"Same audio shape", priority:10 }); }
            }
            routes
        }
    "#,
        false,
    );
    let server = server::start(s, common::options())?;
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
    let c = Client::connect(
        server.address,
        &server.certificate,
        "C".into(),
        vec![],
        None,
    )
    .await?;
    let first = fresh_voice_grant(&mut b, a.owner, None).await?;
    let unrelated = fresh_voice_grant(&mut b, c.owner, None).await?;
    let playback = b.voice_link().routes().routes[&a.owner].playback.clone();
    a.command(Command::MiniGame(MiniGameRequest::Create {
        color: 0,
        settings: Default::default(),
    }))
    .await?;
    let after_speaker = fresh_voice_grant(&mut b, a.owner, Some(first)).await?;
    assert_eq!(b.voice_link().routes().routes[&a.owner].playback, playback);
    assert_eq!(
        b.voice_link().routes().routes[&c.owner].generation,
        unrelated,
        "one speaker changing scope cannot restart unrelated conversations"
    );
    let game = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(game) = b.replica.minigames.iter().find(|g| g.owner == a.owner) {
                return anyhow::Ok(game.id);
            }
            b.receive().await?;
        }
    })
    .await??;
    b.command(Command::MiniGame(MiniGameRequest::Join { game }))
        .await?;
    let after_listener = fresh_voice_grant(&mut b, a.owner, Some(after_speaker)).await?;
    assert_eq!(b.voice_link().routes().routes[&a.owner].playback, playback);
    a.command(Command::MiniGame(MiniGameRequest::AddOnSettings {
        game,
        settings: vec![],
        teams: Some(vec![
            TeamEdit {
                id: None,
                name: "Red".into(),
                color: 0,
                settings: vec![],
            },
            TeamEdit {
                id: None,
                name: "Blue".into(),
                color: 1,
                settings: vec![],
            },
        ]),
        quiet: true,
        reset: false,
    }))
    .await?;
    let teams = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(g) = b.replica.minigames.iter().find(|g| g.id == game)
                && g.teams.len() == 2
            {
                return anyhow::Ok([g.teams[0].id.0, g.teams[1].id.0]);
            }
            b.receive().await?;
        }
    })
    .await??;
    a.command(Command::MiniGame(MiniGameRequest::SetTeam {
        game,
        target: a.owner,
        team: Some(teams[0]),
    }))
    .await?;
    let after_speaker_team = fresh_voice_grant(&mut b, a.owner, Some(after_listener)).await?;
    a.command(Command::MiniGame(MiniGameRequest::SetTeam {
        game,
        target: b.owner,
        team: Some(teams[1]),
    }))
    .await?;
    fresh_voice_grant(&mut b, a.owner, Some(after_speaker_team)).await?;
    assert_eq!(b.voice_link().routes().routes[&a.owner].playback, playback);
    server.stop().await?;
    Ok(())
}

#[test]
fn voice_conversation_keys_are_bounded_host_metadata_separate_from_playback() {
    for (key, valid) in [
        (String::new(), true),
        ("radio/one".to_string(), true),
        ("x".repeat(96), true),
        ("x".repeat(97), false),
        ("bad\nkey".to_string(), false),
    ] {
        let script = r#"
            fn voice_routes(speaker, listeners) {
                let routes = [];
                for id in listeners {
                    if id != speaker { routes.push(#{listener:id, spatial:false, range:48.0,
                        label:"Same", priority:10, conversation:CONVERSATION}); }
                }
                routes
            }
        "#
        .replace("CONVERSATION", &serde_json::to_string(&key).unwrap());
        let (mut s, _tmp) = authored_voice_session(&script, false);
        let a = s.join("A".into(), Vec3::new(0., 0.05, 0.), false).unwrap();
        let b = s.join("B".into(), Vec3::new(4., 0.05, 0.), false).unwrap();
        let selected = s.voice_selections(&[a, b]);
        if valid {
            let route = &selected[&b][&a];
            if key.is_empty() {
                assert!(
                    route.conversation.is_none(),
                    "an omitted key uses the native conversation"
                );
            } else {
                let conversation = route.conversation.as_ref().unwrap();
                assert_eq!(conversation.provider, "custom-voice");
                assert_eq!(conversation.key, key);
            }
            assert!(
                serde_json::to_value(&route.playback)
                    .unwrap()
                    .get("conversation")
                    .is_none()
            );
        } else {
            assert!(selected.is_empty());
            assert!(
                s.package_diagnostics()
                    .iter()
                    .any(|d| d.code == "voice.conversation")
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn voice_package_channel_rekeys_without_changing_the_audio_profile() -> anyhow::Result<()> {
    use bri_net::{client::Client, server};
    let (s, _tmp) = authored_voice_session(
        r#"
        fn voice_routes(speaker, listeners) {
            let routes = [];
            let key = "channel/" + get_player(speaker, "channel");
            for id in listeners {
                if id != speaker { routes.push(#{listener:id, spatial:false, range:48.0,
                    label:"Same audio shape", priority:10, conversation:key}); }
            }
            routes
        }
    "#,
        false,
    );
    let server = server::start(s, common::options())?;
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
    let first = fresh_voice_grant(&mut b, a.owner, None).await?;
    let profile = b.voice_link().routes().routes[&a.owner].playback.clone();
    let talk = a.voice_link();
    let source = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let epoch = talk.control().metadata.source_epoch;
            if epoch > 0 {
                break epoch;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await?;
    let incoming = talk.routes();
    a.command(Command::Package(PackageCommand {
        package: "custom-voice".into(),
        command: "fixture_channel".into(),
        args: vec![PackageArg::Int(2)],
    }))
    .await?;
    fresh_voice_grant(&mut b, a.owner, Some(first)).await?;
    assert_eq!(b.voice_link().routes().routes[&a.owner].playback, profile);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while talk.control().metadata.source_epoch == source {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await?;
    assert!(talk.control().metadata.source_epoch > 0);
    assert_eq!(
        talk.routes(),
        incoming,
        "outgoing channel changes rekey capture even with identical inbound grants"
    );
    server.stop().await?;
    Ok(())
}

#[test]
fn a_failed_voice_policy_cannot_fall_through_to_a_public_route() {
    for body in [
        "[]".to_string(),
        "throw \"private policy failed\";".to_string(),
        r#"let routes=[]; for id in listeners { if id != speaker {
            routes.push(#{listener:id, spatial:false, range:48.0, label:"Private",
                priority:10, effects:#{gain:9.0}}); } } routes"#
            .to_string(),
        r#"let routes=[]; for id in listeners { if id != speaker {
            routes.push(#{listener:id, spatial:false, range:48.0, label:"Private",
                priority:10, conversation:"KEY"}); } } routes"#
            .replace("KEY", &"x".repeat(97)),
    ] {
        let script = ["fn voice_routes(speaker, listeners) {", &body, "}"].join("\n");
        let (mut s, _tmp) = authored_voice_session(&script, true);
        let a = s.join("A".into(), Vec3::new(0., 0.05, 0.), false).unwrap();
        let b = s.join("B".into(), Vec3::new(4., 0.05, 0.), false).unwrap();
        let routes = s.voice_routes(&[a, b]);
        if body == "[]" {
            assert!(
                routes[&b].contains_key(&a),
                "valid abstention retains ordinary public voice"
            );
        } else {
            assert!(
                routes.is_empty(),
                "an undecided private policy cannot grant public fallback: {body}"
            );
            assert!(!s.package_diagnostics().is_empty());
        }
    }
}

#[test]
fn loading_connection_has_no_implicit_private_radio_channel() {
    let mut s = session(true);
    let a = s.join("A".into(), Vec3::new(0., 0.05, 0.), false).unwrap();
    let loading = s
        .arrive("Loading".into(), &Default::default(), false, None)
        .unwrap();
    assert!(s.voice_routes(&[a, loading])[&loading].contains_key(&a));
    radio(&mut s, a, 1, "radio_transmit", PackageArg::Bool(true));
    assert!(
        !s.voice_routes(&[a, loading])
            .get(&loading)
            .is_some_and(|r| r.contains_key(&a)),
        "unknown loading radio channel must deny, rather than falling back to public voice"
    );
    assert!(s.take_package_problems().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepted_loading_voice_is_bidirectional_moderated_and_rekeyed_on_entry()
-> anyhow::Result<()> {
    use bri_net::{
        client::{Arrival, Client, HostPin},
        server,
        voice::VoiceLink,
    };
    use std::time::Duration;
    async fn player_route(client: &mut Client, speaker: u64, allowed: bool) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let table = client.voice_link().routes();
                if table.revision > 0 && table.routes.contains_key(&speaker) == allowed {
                    break;
                }
                client.receive().await?;
            }
            anyhow::Ok(())
        })
        .await??;
        Ok(())
    }
    async fn loading_route(link: &VoiceLink, speaker: u64, allowed: bool) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let table = link.routes();
                if table.revision > 0 && table.routes.contains_key(&speaker) == allowed {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        Ok(())
    }
    async fn exchange(
        talk: &VoiceLink,
        listen: &VoiceLink,
        speaker: u64,
        seq: u16,
        value: u8,
    ) -> anyhow::Result<()> {
        let mut heard = vec![];
        listen.take_heard(&mut heard);
        heard.clear();
        for n in seq..seq + 3 {
            talk.send(n, &[value; 60])?;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                listen.take_heard(&mut heard);
                if heard
                    .iter()
                    .any(|f| f.speaker == speaker && f.payload == vec![value; 60])
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        let generation = listen.routes().routes[&speaker].generation;
        assert!(
            heard
                .iter()
                .filter(|f| f.speaker == speaker && f.payload == vec![value; 60])
                .all(|f| f.position.is_none() && f.revision == generation)
        );
        Ok(())
    }
    let server = server::start(session(false), common::options())?;
    let mut a = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Host".into(),
        vec![],
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    let root = tempfile::tempdir()?;
    let identity = bri_identity::ClientIdentity::load_or_create(root.path().join("b.identity"))?;
    let other_identity =
        bri_identity::ClientIdentity::load_or_create(root.path().join("c.identity"))?;
    let pin = HostPin::from(&server.certificate[..]);
    let b = Arrival::open(server.address, &pin, "Loading".into(), None, &identity).await?;
    let c = Arrival::open(
        server.address,
        &pin,
        "Other loading".into(),
        None,
        &other_identity,
    )
    .await?;
    let bl = b.voice_link();
    let cl = c.voice_link();
    player_route(&mut a, b.owner, true).await?;
    loading_route(&bl, a.owner, true).await?;
    loading_route(&cl, b.owner, true).await?;
    assert!(!a.replica.names.contains_key(&b.owner) && !a.replica.names.contains_key(&c.owner));
    exchange(&a.voice_link(), &bl, a.owner, 0, 11).await?;
    exchange(&bl, &a.voice_link(), b.owner, 0, 22).await?;
    exchange(&bl, &cl, b.owner, 3, 33).await?;
    bl.send_activity(true)?;
    let mut activity = vec![];
    tokio::time::timeout(Duration::from_secs(3), async {
        while !activity
            .iter()
            .any(|v: &voice::VoiceActivity| v.speaker == b.owner && v.active)
        {
            a.voice_link().take_activity(&mut activity);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    // An ordinary game change revokes loading/lobby eligibility in both directions.
    a.command(Command::MiniGame(MiniGameRequest::Create {
        color: 0,
        settings: Default::default(),
    }))
    .await?;
    player_route(&mut a, b.owner, false).await?;
    loading_route(&bl, a.owner, false).await?;
    let mut forbidden = vec![];
    a.voice_link().take_heard(&mut forbidden);
    forbidden.clear();
    bl.send(6, &[44; 60])?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    a.voice_link().take_heard(&mut forbidden);
    assert!(
        !forbidden
            .iter()
            .any(|f| f.speaker == b.owner && f.payload == vec![44; 60])
    );
    a.command(Command::MiniGame(MiniGameRequest::Leave)).await?;
    player_route(&mut a, b.owner, true).await?;
    loading_route(&bl, a.owner, true).await?;
    a.command(Command::Admin(bri_admin::Request::new(
        bri_admin::Action::VoiceMute {
            target: bri_admin::ConnectionId(b.owner),
        },
    )))
    .await?;
    player_route(&mut a, b.owner, false).await?;
    loading_route(&cl, b.owner, false).await?;
    a.command(Command::Admin(bri_admin::Request::new(
        bri_admin::Action::VoiceUnmute {
            target: bri_admin::ConnectionId(b.owner),
        },
    )))
    .await?;
    player_route(&mut a, b.owner, true).await?;
    loading_route(&cl, b.owner, true).await?;
    let old = a.voice_link().routes().routes[&b.owner].generation;
    let cache = bri_package::sync::Cache::open(&root.path().join("cache"))?;
    let (mut entered, _, _) = Client::connect_arriving(
        server.address,
        pin,
        "Loading".into(),
        vec![],
        None,
        None,
        None,
        &identity,
        &cache,
        bri_progress::Progress::default(),
        |_, _| Ok(vec![]),
        Some(b.ticket.clone()),
        async { Ok(()) },
        None,
    )
    .await?;
    assert_eq!(entered.owner, b.owner);
    player_route(&mut a, b.owner, true).await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while a
            .voice_link()
            .routes()
            .routes
            .get(&b.owner)
            .is_none_or(|r| r.generation == old)
        {
            a.receive().await?;
        }
        anyhow::Ok(())
    })
    .await??;
    player_route(&mut entered, a.owner, true).await?;
    exchange(&entered.voice_link(), &a.voice_link(), b.owner, 0, 55).await?;
    exchange(&a.voice_link(), &entered.voice_link(), a.owner, 3, 66).await?;
    // Retained authorized game membership is usable while reconnecting, with
    // actual QUIC isolation from the other loading participant still in lobby.
    a.command(Command::MiniGame(MiniGameRequest::Create {
        color: 0,
        settings: Default::default(),
    }))
    .await?;
    player_route(&mut a, b.owner, false).await?;
    let game = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(g) = a.replica.minigames.iter().find(|g| g.owner == a.owner) {
                return anyhow::Ok(g.id);
            }
            a.receive().await?;
        }
    })
    .await??;
    entered
        .command(Command::MiniGame(MiniGameRequest::Join { game }))
        .await?;
    player_route(&mut a, b.owner, true).await?;
    entered.close();
    player_route(&mut a, b.owner, false).await?;
    let returning = Arrival::open(
        server.address,
        &HostPin::from(&server.certificate[..]),
        "Loading".into(),
        None,
        &identity,
    )
    .await?;
    assert_eq!(returning.owner, b.owner);
    let returning_link = returning.voice_link();
    player_route(&mut a, b.owner, true).await?;
    loading_route(&returning_link, a.owner, true).await?;
    loading_route(&cl, b.owner, false).await?;
    assert!(!returning_link.routes().routes.contains_key(&c.owner));
    assert_eq!(
        returning_link
            .control()
            .metadata
            .conversation
            .as_ref()
            .map(|c| c.game),
        Some(Some(game)),
        "loading presentation must name its authorized retained game, not the lobby"
    );
    exchange(&a.voice_link(), &returning_link, a.owner, 6, 77).await?;
    exchange(&returning_link, &a.voice_link(), b.owner, 0, 88).await?;
    returning.close();
    player_route(&mut a, b.owner, false).await?;
    c.close();
    entered.close();
    a.close();
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loading_voice_routes_are_revoked_when_the_new_map_has_no_voice_policy()
-> anyhow::Result<()> {
    use bri_net::{
        client::{Arrival, Client, HostPin},
        server,
    };
    use bri_sim::session::MapListing;
    use std::time::Duration;
    let maps = vec![
        MapListing {
            id: "fixture".into(),
            name: "Fixture".into(),
        },
        MapListing {
            id: "quiet".into(),
            name: "Quiet".into(),
        },
    ];
    let mut initial = session(false);
    initial.set_map_list(maps.clone())?;
    let mut options = common::options();
    options.map_loader = Some(Arc::new(move |map: &str| {
        anyhow::ensure!(map == "quiet", "unexpected map");
        let mut next = common::session();
        next.set_map_list(maps.clone())?;
        Ok(next)
    }));
    let server = server::start(initial, options)?;
    let mut a = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Host".into(),
        vec![],
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    let root = tempfile::tempdir()?;
    let identity =
        bri_identity::ClientIdentity::load_or_create(root.path().join("loading.identity"))?;
    let loading = Arrival::open(
        server.address,
        &HostPin::from(&server.certificate[..]),
        "Loading".into(),
        None,
        &identity,
    )
    .await?;
    let link = loading.voice_link();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !link.routes().routes.contains_key(&a.owner) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    let old = link.routes().revision;
    a.command(Command::Admin(bri_admin::Request::new(
        bri_admin::Action::ChangeMap {
            map: "quiet".into(),
        },
    )))
    .await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let table = link.routes();
            if table.revision > old && table.routes.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    loading.close();
    a.close();
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn voice_control_teardown_preserves_the_authoritative_kick_reason() -> anyhow::Result<()> {
    use bri_net::{client::Client, server};
    use std::time::Duration;
    let server = server::start(session(false), common::options())?;
    let mut host = Client::connect_with_host(
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
    let link = guest.voice_link();
    tokio::time::timeout(Duration::from_secs(3), async {
        while link.control().revision == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    host.command(Command::Admin(bri_admin::Request::new(
        bri_admin::Action::Kick {
            target: bri_admin::ConnectionId(guest.owner),
        },
    )))
    .await?;
    let reason = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Err(error) = guest.receive().await {
                return error.to_string();
            }
        }
    })
    .await?;
    assert!(
        reason.to_ascii_lowercase().contains("kick"),
        "actual kick reason was lost: {reason}"
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while !link.is_closed()
            || link.control().metadata.enabled
            || !link.routes().routes.is_empty()
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    host.close();
    server.stop().await?;
    Ok(())
}
