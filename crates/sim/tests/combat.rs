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
    // Minigame death messages go to members only, with the vanilla icon.
    let notices = s.take_private_notices();
    assert!(notices.iter().any(|(o, n)| *o == a
        && matches!(n, Notice::Chat(t) if t.contains("ci/skull") && t.contains("Bravo"))));
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
    assert!(chat.iter().any(|(_, n)| matches!(n, Notice::Chat(t) if t.contains("ci/crater"))));

    // Leaving the minigame (after respawning) makes the same fall harmless.
    steps(&mut s, 130);
    s.command(a, 2, Command::Respawn).unwrap();
    s.command(a, 3, Command::MiniGame(MiniGameRequest::End)).unwrap();
    assert!(s.minigame_views().is_empty());
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
