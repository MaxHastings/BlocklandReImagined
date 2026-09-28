//! Health, death, respawn, falling damage and minigame membership through the
//! authoritative session.
use bri_minigames::Settings;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{Command, MiniGameRequest, Notice, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;

fn session() -> Session {
    let mut s = Session::new(
        Simulation::new(
            World::new("Combat".into(), "test".into(), vec![[1.0; 4]]),
            Definitions::default(),
            vec![ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0))],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s
}
fn steps(s: &mut Session, n: usize) {
    for _ in 0..n {
        s.step().unwrap();
    }
}

#[test]
fn suicide_death_message_score_and_click_respawn() {
    let mut s = session();
    let a = s.join("Alpha".into(), Vec3::new(-3.0, 0.05, 0.0), false).unwrap();
    let b = s.join("Bravo".into(), Vec3::new(3.0, 0.05, 0.0), false).unwrap();
    s.command(
        a,
        1,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings::default(),
        }),
    )
    .unwrap();
    let game = s.minigame_views()[0].id;
    s.command(b, 1, Command::MiniGame(MiniGameRequest::Join { game })).unwrap();
    assert_eq!(s.minigame_views()[0].members.len(), 2);
    assert_eq!(s.vitals()[&b].minigame, Some(game));
    // The owner hears about the new member; the joiner does not.
    let notices = s.take_private_notices();
    assert!(notices.iter().any(|(o, n)| *o == a
        && matches!(n, Notice::Chat(t) if t.contains("Bravo joined the mini-game"))));

    s.command(b, 2, Command::Suicide).unwrap();
    let vitals = s.vitals();
    assert!(!vitals[&b].alive);
    assert_eq!(vitals[&b].health, 0.0);
    assert_eq!(vitals[&b].score, -1);
    // Minigame death messages go to members only. Icons come from the
    // weapon pack's damage types, which this core-only session lacks.
    let notices = s.take_private_notices();
    assert!(
        notices
            .iter()
            .any(|(o, n)| *o == a && matches!(n, Notice::Chat(t) if t == "Bravo"))
    );
    assert!(s.command(b, 3, Command::Suicide).is_err());
    assert!(s.command(b, 4, Command::Respawn).is_err(), "respawn delay");
    steps(&mut s, 125);
    s.command(b, 5, Command::Respawn).unwrap();
    let vitals = s.vitals();
    assert!(vitals[&b].alive);
    assert_eq!(vitals[&b].health, 100.0);
    // Respawn relocates to the map drop point.
    let feet = s
        .motion_states()
        .into_iter()
        .find(|(p, _)| p.owner == b)
        .unwrap()
        .0
        .feet;
    assert!(Vec3::from(feet).distance(Vec3::new(0.0, 0.05, 0.0)) < 0.5);
}

#[test]
fn players_outside_minigames_cannot_be_hurt_and_falls_follow_rules() {
    let mut s = session();
    let a = s.join("Alpha".into(), Vec3::new(0.0, 0.05, 0.0), false).unwrap();
    // Outside a minigame, a long fall is harmless (sandbox falling damage off).
    s.set_spawn_points(vec![Vec3::new(0.0, 80.0, 0.0)]).unwrap();
    s.command(
        a,
        1,
        Command::MiniGame(MiniGameRequest::Create {
            color: 3,
            settings: Settings::default(),
        }),
    )
    .unwrap();
    // Creating the game respawned Alpha at the high drop point. Feed idle
    // inputs so the motor runs every tick like a connected client.
    for sequence in 1..=600 {
        s.movement(a, sequence, MoveInput::default()).unwrap();
        s.step().unwrap();
        if !s.is_alive(a) {
            break;
        }
    }
    assert!(!s.is_alive(a), "an 80 unit fall kills inside a minigame");
    let chat = s.take_private_notices();
    assert!(chat.iter().any(|(_, n)| matches!(n, Notice::Chat(t) if t == "Alpha")));

    // Outside a mini-game the host's Falling Damage decides; with it off,
    // leaving the minigame (after respawning) makes the same fall harmless.
    s.set_server_settings(bri_admin::ServerSettings {
        falling_damage: false,
        ..Default::default()
    })
    .unwrap();
    steps(&mut s, 130);
    s.command(a, 2, Command::Respawn).unwrap();
    s.take_private_notices();
    s.command(a, 3, Command::MiniGame(MiniGameRequest::End)).unwrap();
    assert!(s.minigame_views().is_empty());
    assert!(s.take_private_notices().iter().any(|(o, n)| *o == a
        && matches!(n, Notice::Chat(t) if t.ends_with("The mini-game ended."))));
    let mut sequence = 1000;
    for _ in 0..600 {
        sequence += 1;
        s.movement(a, sequence, MoveInput::default()).unwrap();
        s.step().unwrap();
    }
    assert!(s.is_alive(a));
    assert_eq!(s.vitals()[&a].health, 100.0);
}

#[test]
fn falls_outside_minigames_follow_the_hosts_falling_damage() {
    // v20's server/defaults.cs turns $Pref::Server::FallingDamage on.
    for (on, survives) in [(true, false), (false, true)] {
        let mut s = session();
        s.set_server_settings(bri_admin::ServerSettings {
            falling_damage: on,
            ..Default::default()
        })
        .unwrap();
        let a = s
            .join("Alpha".into(), Vec3::new(0.0, 80.0, 0.0), false)
            .unwrap();
        for sequence in 1..=600 {
            s.movement(a, sequence, MoveInput::default()).unwrap();
            s.step().unwrap();
        }
        assert_eq!(s.is_alive(a), survives, "Falling Damage {on}");
    }
}

#[test]
fn horses_take_no_falling_damage_below_their_min_impact_speed() {
    // HorseArmor's `minImpactSpeed` is 250, so the engine never raises
    // `onImpact` for a fall that kills a Standard Player.
    let mut s = session();
    let a = s.join("Alpha".into(), Vec3::new(0.0, 0.05, 0.0), false).unwrap();
    s.set_spawn_points(vec![Vec3::new(0.0, 80.0, 0.0)]).unwrap();
    let settings = Settings {
        player_type: bri_sim::player_types::PlayerType::Horse.id().into(),
        ..Settings::default()
    };
    s.command(a, 1, Command::MiniGame(MiniGameRequest::Create { color: 3, settings }))
        .unwrap();
    for sequence in 1..=600 {
        s.movement(a, sequence, MoveInput::default()).unwrap();
        s.step().unwrap();
    }
    assert!(s.is_alive(a));
    assert_eq!(s.vitals()[&a].health, 250.0);
}

#[test]
fn minigame_owner_controls_and_invitations() {
    let mut s = session();
    let a = s.join("Alpha".into(), Vec3::new(-3.0, 0.05, 0.0), false).unwrap();
    let b = s.join("Bravo".into(), Vec3::new(3.0, 0.05, 0.0), false).unwrap();
    let settings = Settings {
        invite_only: true,
        ..Settings::default()
    };
    s.command(a, 1, Command::MiniGame(MiniGameRequest::Create { color: 1, settings }))
        .unwrap();
    let game = s.minigame_views()[0].id;
    let err = s
        .command(b, 1, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap_err();
    assert!(err.to_string().contains("invite only"));
    s.command(a, 2, Command::MiniGame(MiniGameRequest::Invite { target: b }))
        .unwrap();
    assert_eq!(s.vitals()[&b].invite, Some(game));
    assert!(s.take_private_notices().iter().any(|(o, n)| *o == b
        && matches!(n, Notice::Invite { owner_name, .. } if owner_name == "Alpha")));
    s.command(b, 2, Command::MiniGame(MiniGameRequest::Accept { game }))
        .unwrap();
    assert_eq!(s.minigame_views()[0].members.len(), 2);
    // Only the owner can reset; the member cannot.
    assert!(s.command(b, 3, Command::MiniGame(MiniGameRequest::Reset)).is_err());
    s.command(a, 3, Command::MiniGame(MiniGameRequest::Kick { target: b }))
        .unwrap();
    assert_eq!(s.vitals()[&b].minigame, None);
    s.command(a, 4, Command::ToggleLight).unwrap();
    assert!(s.vitals()[&a].light);
    s.command(a, 5, Command::Emote("love".into())).unwrap();
    assert!(s.command(a, 6, Command::Emote("dance".into())).is_err());
}

#[test]
fn minigame_loadout_may_repeat_an_item_like_v20() {
    let mut s = session();
    let a = s.join("Alpha".into(), Vec3::new(0.0, 0.05, 0.0), false).unwrap();
    let hammer = Some("v20.weapon.hammeritem".to_string());
    let settings = Settings {
        loadout: [hammer.clone(), hammer.clone(), None, None, None],
        ..Settings::default()
    };
    s.command(a, 1, Command::MiniGame(MiniGameRequest::Create { color: 0, settings }))
        .unwrap();
    s.command(a, 2, Command::Suicide).unwrap();
    steps(&mut s, 200);
    s.command(a, 3, Command::Respawn).unwrap();
    assert!(s.vitals()[&a].alive);
    let slots = &s.tool_inventories()[&a].slots;
    assert_eq!(slots[0], hammer);
    assert_eq!(slots[1], hammer);
}

#[test]
fn add_on_blasts_obey_the_minigame_like_weapon_blasts() {
    let mut s = session();
    let a = s
        .join("Alpha".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let b = s
        .join("Bravo".into(), Vec3::new(20.0, 0.05, 0.0), true)
        .unwrap();
    steps(&mut s, 30);
    s.command(
        a,
        1,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings::default(),
        }),
    )
    .unwrap();
    // Past spawn protection.
    steps(&mut s, 320);
    let at = |s: &Session, owner| {
        let (state, _) = s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap();
        Vec3::from(state.feet) + Vec3::Y
    };
    // Alpha's minigame keeps Bravo, outside it, out of Alpha's blasts.
    s.explode(at(&s, b), 4.0, 50.0, 0.0, "test", Some(a))
        .unwrap();
    assert_eq!(s.vitals()[&b].health, 100.0);
    // A blast nobody set off still hurts.
    s.explode(at(&s, b), 4.0, 50.0, 0.0, "test", None).unwrap();
    assert!(s.vitals()[&b].health < 100.0);
}

#[test]
fn admin_drop_at_camera_costs_a_point_and_respawns_at_once_in_minigames() {
    let mut s = session();
    let a = s.join("Admin".into(), Vec3::new(0.0, 0.05, 0.0), true).unwrap();
    s.command(
        a,
        1,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings::default(),
        }),
    )
    .unwrap();
    steps(&mut s, 10);
    s.command(a, 2, Command::DropPlayerAtCamera(None)).unwrap();
    assert_eq!(s.vitals()[&a].score, -1, "serverCmdDropPlayerAtCamera: incScore(-1)");
    s.command(a, 3, Command::Suicide).unwrap();
    assert!(!s.is_alive(a));
    // `spawnPlayer` directly: no waiting out the minigame respawn time, and
    // no point lost for the respawn itself.
    let score = s.vitals()[&a].score;
    s.command(a, 4, Command::DropPlayerAtCamera(None)).unwrap();
    assert!(s.is_alive(a));
    assert_eq!(s.vitals()[&a].score, score);
}

#[test]
fn anyone_may_ask_for_the_brick_count() {
    let mut s = session();
    let a = s
        .join("Alpha".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    s.take_private_notices();
    // `/brickCount` names no Add-On and no Add-On declares it.
    let typed = Command::Package(bri_sim::session::PackageCommand {
        package: String::new(),
        command: "brickCount".into(),
        args: vec![],
    });
    s.command(a, 1, typed).unwrap();
    let told = s.take_private_notices();
    assert!(
        told.iter()
            .any(|(o, n)| *o == a && matches!(n, Notice::Chat(t) if t == "0 bricks")),
        "{told:?}"
    );
}
