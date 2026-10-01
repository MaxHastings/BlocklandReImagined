//! Teams: the mechanism an Add-On such as Slayer sets policy on.
use bri_minigames::*;

fn world() -> MinigamesWorld {
    MinigamesWorld::new(Catalog::minimal_vanilla(), PolicyMode::Internet, true).unwrap()
}
fn connect(w: &mut MinigamesWorld, account: u64) -> PlayerId {
    let p = w
        .connect(AccountId(account), format!("Player {account}"), false)
        .unwrap();
    w.set_ready(p, true).unwrap();
    p
}
fn spec(name: &str, color: u8) -> TeamSpec {
    TeamSpec {
        id: None,
        name: name.into(),
        color,
    }
}
fn hit(w: &MinigamesWorld, by: PlayerId, victim: PlayerId) -> Decision {
    w.can_damage(
        w.projectile_source(by).unwrap(),
        w.target_for_player(victim).unwrap(),
    )
}

/// Owner plus two members in one game, with Red and Blue.
fn red_blue() -> (MinigamesWorld, GameId, [PlayerId; 3], [TeamId; 2]) {
    let mut w = world();
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let c = connect(&mut w, 3);
    w.execute(Command::Create {
        actor: a,
        color: 0,
        settings: Settings::default(),
    })
    .unwrap();
    let game = w.player(a).unwrap().game.unwrap();
    for p in [b, c] {
        w.execute(Command::Join { actor: p, game }).unwrap();
    }
    let (ids, out) = w
        .set_teams(game, vec![spec("Red", 0), spec("Blue", 3)], false, false)
        .unwrap();
    assert_eq!(out, vec![Effect::TeamsConfigured { game }]);
    (w, game, [a, b, c], [ids[0], ids[1]])
}

#[test]
fn friendly_fire_off_spares_teammates_only() {
    let (mut w, game, [a, b, c], [red, blue]) = red_blue();
    for (p, t) in [(a, red), (b, red), (c, blue)] {
        assert_eq!(
            w.assign_team(p, Some(t)).unwrap(),
            vec![Effect::TeamChanged {
                player: p,
                game,
                team: Some(t)
            }]
        );
    }
    assert!(w.allied(a, b) && !w.allied(a, c));
    assert_eq!(hit(&w, a, b), Decision::Deny(Denial::Teammate));
    assert_eq!(hit(&w, a, c), Decision::Allow);
    // Self-damage is the game's own setting, not the team's.
    assert_eq!(hit(&w, a, a), Decision::Allow);
    // Assigning the same team again changes nothing.
    assert!(w.assign_team(a, Some(red)).unwrap().is_empty());

    w.set_teams(
        game,
        vec![
            TeamSpec { id: Some(red), name: "Red".into(), color: 0 },
            TeamSpec { id: Some(blue), name: "Blue".into(), color: 3 },
        ],
        true,
        false,
    )
    .unwrap();
    assert_eq!(hit(&w, a, b), Decision::Allow);
}

#[test]
fn same_colour_allies_and_removed_teams_free_their_members() {
    let (mut w, game, [a, b, c], [red, blue]) = red_blue();
    w.assign_team(a, Some(red)).unwrap();
    w.assign_team(c, Some(blue)).unwrap();
    // A second red team: allies only when the game says so.
    let (ids, _) = w
        .set_teams(
            game,
            vec![
                TeamSpec { id: Some(red), name: "Red".into(), color: 0 },
                TeamSpec { id: Some(blue), name: "Blue".into(), color: 3 },
                spec("Crimson", 0),
            ],
            false,
            true,
        )
        .unwrap();
    let crimson = ids[2];
    assert!(crimson != red && crimson != blue);
    w.assign_team(b, Some(crimson)).unwrap();
    assert!(w.allied(a, b));
    assert_eq!(hit(&w, b, a), Decision::Deny(Denial::Teammate));

    // Dropping Blue leaves its member with no team.
    let (_, out) = w
        .set_teams(
            game,
            vec![TeamSpec { id: Some(red), name: "Reds".into(), color: 0 }],
            false,
            false,
        )
        .unwrap();
    assert!(out.contains(&Effect::TeamChanged { player: c, game, team: None }));
    assert!(out.contains(&Effect::TeamChanged { player: b, game, team: None }));
    assert_eq!(w.team_of(a), Some(red));
    assert_eq!(w.game(game).unwrap().teams.get(red).unwrap().name, "Reds");
    assert_eq!(w.assign_team(c, Some(blue)), Err(Error::StaleTeam));
}

#[test]
fn leaving_or_ending_the_game_drops_the_team() {
    let (mut w, game, [a, b, c], [red, blue]) = red_blue();
    w.assign_team(a, Some(red)).unwrap();
    w.assign_team(b, Some(blue)).unwrap();
    w.assign_team(c, Some(blue)).unwrap();
    let out = w.execute(Command::Leave { actor: b }).unwrap();
    assert!(out.contains(&Effect::TeamChanged { player: b, game, team: None }));
    assert_eq!(w.team_of(b), None);
    // Outside a game nobody can be put on a team.
    assert_eq!(w.assign_team(b, Some(blue)), Err(Error::NotMember));
    let out = w.execute(Command::End { actor: a }).unwrap();
    assert!(out.contains(&Effect::TeamChanged { player: c, game, team: None }));
    assert_eq!(w.team_of(a), None);
}

#[test]
fn teams_are_checked_and_survive_a_save() {
    let (mut w, game, [a, _, c], [red, blue]) = red_blue();
    assert_eq!(
        w.set_teams(game, vec![spec("", 1)], false, false).map(|_| ()),
        Err(Error::InvalidSettings)
    );
    assert_eq!(
        w.set_teams(game, vec![spec(&"x".repeat(MAX_TEAM_NAME + 1), 1)], false, false)
            .map(|_| ()),
        Err(Error::InvalidSettings)
    );
    assert_eq!(
        w.set_teams(game, (0..=MAX_TEAMS).map(|i| spec(&format!("T{i}"), 0)).collect(), false, false)
            .map(|_| ()),
        Err(Error::Capacity)
    );
    assert_eq!(
        w.set_teams(
            game,
            vec![
                TeamSpec { id: Some(red), name: "A".into(), color: 0 },
                TeamSpec { id: Some(red), name: "B".into(), color: 0 },
            ],
            false,
            false
        )
        .map(|_| ()),
        Err(Error::StaleTeam)
    );
    w.assign_team(a, Some(red)).unwrap();
    w.assign_team(c, Some(blue)).unwrap();
    let saved = w.save().unwrap();
    let back = MinigamesWorld::restore(&saved, Catalog::minimal_vanilla()).unwrap();
    assert_eq!(back.team_of(a), Some(red));
    assert_eq!(back.game(game).unwrap().teams, w.game(game).unwrap().teams);
    assert_eq!(hit(&back, a, c), Decision::Allow);

    // A saved player on a team their game does not have is refused.
    let text = String::from_utf8(saved).unwrap();
    let broken = text.replacen(&format!("\"team\": {}", blue.0), "\"team\": 99", 1);
    assert_ne!(broken, text);
    assert!(MinigamesWorld::restore(broken.as_bytes(), Catalog::minimal_vanilla()).is_err());
}
