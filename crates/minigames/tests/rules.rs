use bri_minigames::*;

fn world(mode: PolicyMode) -> MinigamesWorld {
    let mut catalog = Catalog::minimal_vanilla();
    catalog.player_types.insert("v20.player.playernojet".into());
    catalog.items.insert(
        "v20.weapon.basketballitem".into(),
        Some("v20.image.basketballimage".into()),
    );
    MinigamesWorld::new(catalog, mode, true).unwrap()
}
fn connect(w: &mut MinigamesWorld, account: u64) -> PlayerId {
    let p = w
        .connect(AccountId(account), format!("Player {account}"), false)
        .unwrap();
    w.set_ready(p, true).unwrap();
    p
}
fn create(w: &mut MinigamesWorld, actor: PlayerId, color: u8) -> GameId {
    w.execute(Command::Create {
        actor,
        color,
        settings: Settings::default(),
    })
    .unwrap();
    w.player(actor).unwrap().game.unwrap()
}
fn life(w: &MinigamesWorld, p: PlayerId) -> LifeId {
    match w.player(p).unwrap().life {
        LifeState::Alive { life } | LifeState::Dead { life, .. } => life,
    }
}
fn obj(kind: ObjectKind, owner: PlayerId) -> Target {
    Target::Object {
        kind,
        owner: Some(owner.account),
        membership: Membership::Owner,
        spawn_brick: true,
    }
}
fn configure(
    w: &mut MinigamesWorld,
    owner: PlayerId,
    f: impl FnOnce(&mut Settings),
) -> Vec<Effect> {
    let mut s = w
        .game(w.player(owner).unwrap().game.unwrap())
        .unwrap()
        .settings
        .clone();
    f(&mut s);
    w.execute(Command::Configure {
        actor: owner,
        settings: s,
    })
    .unwrap()
}
fn steps(w: &mut MinigamesWorld, n: usize) {
    for _ in 0..n {
        w.step().unwrap();
    }
}

#[test]
fn actual_gui_defaults_and_timer_units() {
    let s = Settings::default();
    assert!(
        !s.invite_only
            && !s.use_all_players_bricks
            && !s.players_use_own_bricks
            && s.use_spawn_bricks
    );
    assert!(
        s.falling_damage && s.weapon_damage && s.self_damage && s.vehicle_damage && s.brick_damage
    );
    assert!(s.enable_building && s.enable_painting && !s.enable_wand);
    assert_eq!(
        (
            s.points_kill_player,
            s.points_kill_self,
            s.points_die,
            s.points_plant_brick,
            s.points_break_brick
        ),
        (1, -1, 0, 0, 0)
    );
    assert_eq!(
        (s.respawn_ms, s.vehicle_respawn_ms, s.brick_respawn_ms),
        (1000, 5000, 30000)
    );
    assert_eq!(s.lives, Lives::Unlimited);
    assert_eq!(s.loadout[2].as_deref(), Some("v20.weapon.printgun"));
    assert_eq!(
        s.loadout[4].as_deref(),
        Some("v20.weapon.rocketlauncheritem")
    );
    assert_eq!(ticks_for_ms(50), 6);
    assert_eq!(manual_respawn_ticks(1000), 121);
}

#[test]
fn owner_lifecycle_and_admin_is_not_owner() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let id = create(&mut w, a, 0);
    w.execute(Command::Join { actor: b, game: id }).unwrap();
    w.set_admin(b, true).unwrap();
    assert_eq!(
        w.execute(Command::Configure {
            actor: b,
            settings: Settings::default()
        }),
        Err(Error::NotOwner)
    );
    assert_eq!(w.execute(Command::End { actor: b }), Err(Error::NotOwner));
    let a_life = life(&w, a);
    let b_life = life(&w, b);
    let out = w.execute(Command::Leave { actor: a }).unwrap();
    assert!(
        out.iter()
            .any(|e| matches!(e,Effect::RestoreOwner { player,.. } if *player==a))
    );
    assert!(
        out.iter()
            .any(|e| matches!(e,Effect::Spawn { player,reason:SpawnReason::End,.. } if *player==b))
    );
    assert_ne!(life(&w, a), a_life);
    assert_ne!(life(&w, b), b_life);
    assert_eq!(w.game(id), Err(Error::StaleGame));
    assert_eq!(w.free_colors().len(), 10);
    assert!(w.player(b).unwrap().game.is_none());
}

#[test]
fn invitations_ignore_stale_and_cooldown() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let c = connect(&mut w, 3);
    let g = create(&mut w, a, 0);
    let h = create(&mut w, c, 1);
    configure(&mut w, a, |s| s.invite_only = true);
    assert_eq!(
        w.execute(Command::Join { actor: b, game: g }),
        Err(Error::InviteOnly)
    );
    w.execute(Command::Invite {
        actor: a,
        target: b,
    })
    .unwrap();
    assert_eq!(
        w.execute(Command::Invite {
            actor: c,
            target: b
        }),
        Err(Error::AlreadyInvited)
    );
    assert_eq!(
        w.execute(Command::Accept { actor: b, game: h }),
        Err(Error::NoInvitation)
    );
    w.execute(Command::Reject {
        actor: b,
        game: g,
        ignore_owner: true,
    })
    .unwrap();
    assert_eq!(
        w.execute(Command::Invite {
            actor: a,
            target: b
        }),
        Err(Error::Ignored)
    );
    w.execute(Command::Join { actor: b, game: h }).unwrap();
    w.execute(Command::Leave { actor: b }).unwrap();
    assert_eq!(
        w.execute(Command::Join { actor: b, game: h }),
        Err(Error::Cooldown)
    );
    w.execute(Command::Invite {
        actor: c,
        target: b,
    })
    .unwrap();
    w.execute(Command::Accept { actor: b, game: h }).unwrap(); // source acceptance bypasses public join throttle
    w.disconnect(b).unwrap();
    let new_b = connect(&mut w, 2);
    assert_ne!(new_b, b);
    assert_eq!(
        w.execute(Command::Leave { actor: b }),
        Err(Error::StalePlayer)
    );
    w.execute(Command::Invite {
        actor: c,
        target: new_b,
    })
    .unwrap();
    w.execute(Command::End { actor: c }).unwrap();
    assert_eq!(w.player(new_b).unwrap().invite, None);
    assert_eq!(
        w.execute(Command::Accept {
            actor: new_b,
            game: h
        }),
        Err(Error::StaleGame)
    );
}

#[test]
fn two_games_and_outside_damage_matrix() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let c = connect(&mut w, 3);
    let d = connect(&mut w, 4);
    let e = connect(&mut w, 5);
    let g = create(&mut w, a, 0);
    w.execute(Command::Join { actor: b, game: g }).unwrap();
    create(&mut w, c, 1);
    for actor in [a, b, c, d, e] {
        for victim in [a, b, c, d, e] {
            let actual = w.can_damage(
                DamageSource::Player(actor),
                w.target_for_player(victim).unwrap(),
            );
            let ga = w.player(actor).unwrap().game;
            let gv = w.player(victim).unwrap().game;
            let expected = if ga.is_none() && gv.is_none() {
                Decision::OutsideMinigames
            } else if ga == gv {
                Decision::Allow
            } else {
                Decision::Deny(Denial::DifferentGame)
            };
            assert_eq!(actual, expected, "{actor:?}->{victim:?}");
        }
    }
    configure(&mut w, a, |s| {
        s.self_damage = false;
        s.vehicle_damage = false;
        s.brick_damage = false;
    });
    assert_eq!(
        w.can_damage(DamageSource::Player(a), w.target_for_player(a).unwrap()),
        Decision::Deny(Denial::Disabled)
    );
    assert_eq!(
        w.can_damage(DamageSource::Player(a), w.target_for_player(b).unwrap()),
        Decision::Allow
    );
    for kind in [ObjectKind::Vehicle, ObjectKind::Bot, ObjectKind::Brick] {
        assert_eq!(
            w.can_damage(DamageSource::Player(b), obj(kind, a)),
            Decision::Deny(Denial::Disabled)
        );
    }
    configure(&mut w, a, |s| s.weapon_damage = false);
    assert_eq!(
        w.can_damage(DamageSource::Player(a), w.target_for_player(b).unwrap()),
        Decision::Deny(Denial::Disabled)
    );
}

#[test]
fn use_own_bricks_applies_to_use_not_damage() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let g = create(&mut w, a, 0);
    w.execute(Command::Join { actor: b, game: g }).unwrap();
    assert_eq!(
        w.can_use(b, obj(ObjectKind::Vehicle, b)),
        Decision::Deny(Denial::NotInGame)
    );
    assert_eq!(w.can_use(b, obj(ObjectKind::Vehicle, a)), Decision::Allow);
    configure(&mut w, a, |s| {
        s.use_all_players_bricks = true;
        s.players_use_own_bricks = true;
    });
    assert_eq!(
        w.can_use(b, obj(ObjectKind::Vehicle, a)),
        Decision::Deny(Denial::NotYours)
    );
    assert_eq!(w.can_use(b, obj(ObjectKind::Vehicle, b)), Decision::Allow);
    assert_eq!(
        w.can_damage(DamageSource::Player(b), obj(ObjectKind::Vehicle, a)),
        Decision::Allow
    );
    let loose = Target::Object {
        kind: ObjectKind::Item,
        owner: Some(a.account),
        membership: Membership::Owner,
        spawn_brick: false,
    };
    assert_eq!(w.can_use(b, loose), Decision::Allow);
    let own_outside = Target::Object {
        kind: ObjectKind::Vehicle,
        owner: Some(b.account),
        membership: Membership::Outside,
        spawn_brick: true,
    };
    assert_eq!(w.can_use(b, own_outside), Decision::Allow);
}

#[test]
fn legacy_lan_policy_is_explicit_and_radius_self_check_is_preserved() {
    let mut w = world(PolicyMode::LegacyLan);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    create(&mut w, a, 0);
    configure(&mut w, a, |s| s.self_damage = false);
    assert_eq!(
        w.can_use(a, w.target_for_player(b).unwrap()),
        Decision::Allow
    );
    assert_eq!(
        w.can_damage(DamageSource::Player(a), obj(ObjectKind::Vehicle, b)),
        Decision::Allow
    );
    assert_eq!(
        w.can_damage(DamageSource::Player(a), w.target_for_player(b).unwrap()),
        Decision::Deny(Denial::DifferentGame)
    );
    let t = w.target_for_player(a).unwrap();
    assert_eq!(w.can_damage(DamageSource::Player(a), t), Decision::Allow);
    assert_eq!(
        w.can_radius_damage(DamageSource::Player(a), t),
        Decision::Deny(Denial::Disabled)
    );
}

#[test]
fn source_generation_round_and_target_life_reject_stale_hits() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let g = create(&mut w, a, 0);
    w.execute(Command::Join { actor: b, game: g }).unwrap();
    let source = w.projectile_source(a).unwrap();
    let old_target = w.target_for_player(b).unwrap();
    w.execute(Command::Reset {
        game: g,
        authority: EventAuthority::Owner(a),
    })
    .unwrap();
    assert_eq!(
        w.can_damage(source, w.target_for_player(b).unwrap()),
        Decision::Deny(Denial::StaleIdentity)
    );
    assert_eq!(
        w.can_damage(DamageSource::Player(a), old_target),
        Decision::Deny(Denial::StaleIdentity)
    );
    let source = w.projectile_source(b).unwrap();
    w.disconnect(b).unwrap();
    connect(&mut w, 2);
    assert_eq!(
        w.can_damage(source, w.target_for_player(a).unwrap()),
        Decision::Deny(Denial::StaleIdentity)
    );
}

#[test]
fn death_scores_are_once_per_life_and_respawn_is_manual_strict_deadline() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let g = create(&mut w, a, 0);
    w.execute(Command::Join { actor: b, game: g }).unwrap();
    configure(&mut w, a, |s| s.points_die = -2);
    let old = life(&w, b);
    w.died(b, old, Some(a)).unwrap();
    assert_eq!(
        (w.player(a).unwrap().score, w.player(b).unwrap().score),
        (1, -2)
    );
    assert_eq!(w.died(b, old, Some(a)), Err(Error::StaleLife));
    steps(&mut w, 120);
    assert_eq!(
        w.execute(Command::Respawn { actor: b }),
        Err(Error::RespawnNotReady)
    );
    steps(&mut w, 1);
    assert!(matches!(w.player(b).unwrap().life, LifeState::Dead { .. }));
    w.execute(Command::Respawn { actor: b }).unwrap();
    let new = life(&w, b);
    assert_ne!(old, new);
    w.died(b, new, Some(b)).unwrap();
    assert_eq!(w.player(b).unwrap().score, -3); // suicide only, no additional Die score
    steps(&mut w, 121);
    w.execute(Command::Respawn { actor: b }).unwrap();
    let new = life(&w, b);
    w.died(b, new, None).unwrap();
    assert_eq!(w.player(b).unwrap().score, -5);
}

#[test]
fn changed_respawn_setting_affects_already_dead_players() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    create(&mut w, a, 0);
    let l = life(&w, a);
    w.died(a, l, None).unwrap();
    steps(&mut w, 100);
    configure(&mut w, a, |s| s.respawn_ms = 2000);
    steps(&mut w, 140);
    assert_eq!(
        w.execute(Command::Respawn { actor: a }),
        Err(Error::RespawnNotReady)
    );
    steps(&mut w, 1);
    w.execute(Command::Respawn { actor: a }).unwrap();
}

#[test]
fn reset_event_authority_and_cleanup_order() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let c = connect(&mut w, 3);
    let g = create(&mut w, a, 0);
    w.execute(Command::Join { actor: b, game: g }).unwrap();
    assert_eq!(
        w.execute(Command::Reset {
            game: g,
            authority: EventAuthority::Owner(b)
        }),
        Err(Error::NotOwner)
    );
    assert_eq!(
        w.execute(Command::Reset {
            game: g,
            authority: EventAuthority::OwnerBrick {
                instigator: b,
                brick_owner: b.account
            }
        }),
        Err(Error::NotOwner)
    );
    assert_eq!(
        w.execute(Command::Reset {
            game: g,
            authority: EventAuthority::OwnerBrick {
                instigator: c,
                brick_owner: a.account
            }
        }),
        Err(Error::NotMember)
    );
    let out = w
        .execute(Command::Reset {
            game: g,
            authority: EventAuthority::OwnerBrick {
                instigator: b,
                brick_owner: a.account,
            },
        })
        .unwrap();
    assert!(matches!(&out[0],Effect::ResetBricks {owners,..} if owners==&vec![a.account]));
    for p in [a, b] {
        let cleanup = out
            .iter()
            .position(|e| matches!(e,Effect::Cleanup {player,..} if *player==p))
            .unwrap();
        let spawn = out
            .iter()
            .position(|e| matches!(e,Effect::Spawn {player,..} if *player==p))
            .unwrap();
        assert!(cleanup < spawn);
    }
    assert_eq!(
        w.execute(Command::Reset {
            game: g,
            authority: EventAuthority::Owner(a)
        }),
        Err(Error::Cooldown)
    );
    steps(&mut w, 600);
    configure(&mut w, a, |s| s.use_all_players_bricks = true);
    let out = w
        .execute(Command::Reset {
            game: g,
            authority: EventAuthority::Owner(a),
        })
        .unwrap();
    assert!(
        matches!(&out[0],Effect::ResetBricks {owners,..} if owners==&vec![a.account,b.account])
    );
}

#[test]
fn sports_equipment_player_type_build_and_paint_callbacks() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    create(&mut w, a, 0);
    let out = configure(&mut w, a, |s| {
        s.loadout[0] = Some("v20.weapon.basketballitem".into());
        s.loadout[1] = s.loadout[0].clone();
        s.player_type = "v20.player.playernojet".into();
        s.enable_building = false;
        s.enable_painting = false;
        s.enable_wand = true;
    });
    assert!(out.iter().any(|e|matches!(e,Effect::ApplyEquipment {equipment,change_player_type:true,cancel_building:true,unmount_paint:true,..}
        if equipment.tools[0].is_none() && equipment.tools[1].is_none() && equipment.start_ball.as_deref()==Some("v20.image.basketballimage"))));
    for _ in 0..5 {
        assert!(w.step().unwrap().is_empty());
    }
    assert!(
        matches!(&w.step().unwrap()[0],Effect::StartBall {player,only_if_hands_empty:true,..} if *player==a)
    );
    assert_eq!(
        w.can_build(a, BuildAction::Build).unwrap(),
        Decision::Deny(Denial::Disabled)
    );
    assert_eq!(
        w.can_build(a, BuildAction::Paint).unwrap(),
        Decision::Deny(Denial::Disabled)
    );
    assert_eq!(w.can_build(a, BuildAction::Wand).unwrap(), Decision::Allow);
    configure(&mut w, a, |s| s.loadout[0] = None);
    steps(&mut w, 5);
    assert!(w.step().unwrap().is_empty());
}

#[test]
fn spawn_selection_counts_bricks_not_groups() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let c = connect(&mut w, 3);
    let g = create(&mut w, a, 0);
    w.execute(Command::Join { actor: b, game: g }).unwrap();
    let points = [
        SpawnPoint {
            id: 1,
            owner: a.account,
        },
        SpawnPoint {
            id: 2,
            owner: b.account,
        },
        SpawnPoint {
            id: 3,
            owner: b.account,
        },
        SpawnPoint {
            id: 4,
            owner: c.account,
        },
    ];
    assert_eq!(w.pick_spawn(b, &points, u64::MAX).unwrap(), Some(1));
    configure(&mut w, a, |s| s.use_all_players_bricks = true);
    assert_eq!(w.pick_spawn(b, &points, 0).unwrap(), Some(1));
    assert_eq!(w.pick_spawn(b, &points, u64::MAX / 2).unwrap(), Some(2));
    assert_eq!(w.pick_spawn(b, &points, u64::MAX).unwrap(), Some(3));
    configure(&mut w, a, |s| s.players_use_own_bricks = true);
    assert_eq!(w.pick_spawn(b, &points, 0).unwrap(), Some(2));
    configure(&mut w, a, |s| s.use_spawn_bricks = false);
    assert_eq!(w.pick_spawn(b, &points, 0).unwrap(), None);
}

#[test]
fn environment_flags_and_respawn_resource_timing() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let g = create(&mut w, a, 0);
    configure(&mut w, a, |s| {
        s.falling_damage = false;
        s.weapon_damage = false;
        s.self_damage = false;
        s.vehicle_respawn_ms = 0;
        s.brick_respawn_ms = 2000;
    });
    let target = w.target_for_player(a).unwrap();
    assert_eq!(
        w.can_damage(
            DamageSource::Environment(EnvironmentDamage::Falling),
            target
        ),
        Decision::Deny(Denial::Disabled)
    );
    assert_eq!(
        w.can_damage(DamageSource::Environment(EnvironmentDamage::Lava), target),
        Decision::Allow
    );
    assert_eq!(
        w.can_damage(
            DamageSource::Environment(EnvironmentDamage::Suicide),
            target
        ),
        Decision::Allow
    );
    assert_eq!(w.respawn_delay(Some(g), RespawnObject::Vehicle).unwrap(), 0);
    assert_eq!(w.respawn_delay(Some(g), RespawnObject::Brick).unwrap(), 240);
    assert_eq!(w.respawn_delay(Some(g), RespawnObject::Item).unwrap(), 480);
}

#[test]
fn message_events_and_respawn_all_preserve_score() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let g = create(&mut w, a, 0);
    w.execute(Command::Join { actor: b, game: g }).unwrap();
    w.event_score(b, 12, false).unwrap();
    let out = w
        .execute(Command::Message {
            game: g,
            authority: EventAuthority::OwnerBrick {
                instigator: b,
                brick_owner: a.account,
            },
            kind: MessageKind::Chat,
            text: "%1 has %2 points".into(),
        })
        .unwrap();
    assert!(
        matches!(&out[0],Effect::Message {recipients,text,..} if recipients==&vec![a,b] && text=="Player 2 has 12 points")
    );
    let center = w
        .execute(Command::Message {
            game: g,
            authority: EventAuthority::Owner(a),
            kind: MessageKind::Center { seconds: 3 },
            text: "%1 %2".into(),
        })
        .unwrap();
    assert!(matches!(&center[0], Effect::Message { text,.. } if text == "Player 1 %2"));
    let old = life(&w, b);
    w.execute(Command::RespawnAll {
        game: g,
        authority: EventAuthority::System,
    })
    .unwrap();
    assert_ne!(old, life(&w, b));
    assert_eq!(w.player(b).unwrap().score, 12);
}

#[test]
fn snapshot_and_preset_roundtrip_keep_deadline_invitation_and_session_identity() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let g = create(&mut w, a, 0);
    w.execute(Command::Invite {
        actor: a,
        target: b,
    })
    .unwrap();
    let l = life(&w, a);
    w.died(a, l, None).unwrap();
    steps(&mut w, 60);
    let data = w.save().unwrap();
    let mut restored = MinigamesWorld::restore(&data, w.catalog().clone()).unwrap();
    assert_eq!(data, restored.save().unwrap());
    steps(&mut restored, 61);
    restored.execute(Command::Respawn { actor: a }).unwrap();
    restored
        .execute(Command::Accept { actor: b, game: g })
        .unwrap();
    assert_eq!(restored.player(b).unwrap().game, Some(g));
    let preset = Preset::new(Settings::default(), w.catalog()).unwrap();
    let json = preset.to_json(w.catalog()).unwrap();
    assert_eq!(Preset::from_json(&json, w.catalog()).unwrap(), preset);
    let mut malformed: serde_json::Value = serde_json::from_slice(&data).unwrap();
    malformed["players"][0]["id"]["session"] = 999.into();
    assert!(
        MinigamesWorld::restore(
            &serde_json::to_vec(&malformed).unwrap(),
            w.catalog().clone()
        )
        .is_err()
    );
}

#[test]
fn invalid_commands_are_atomic_and_inputs_bounded() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let g = create(&mut w, a, 0);
    let before = w.save().unwrap();
    for invalid in [
        Settings {
            title: "x".repeat(36),
            ..Settings::default()
        },
        Settings {
            respawn_ms: 999,
            ..Settings::default()
        },
        Settings {
            player_type: "v20.player.notreal".into(),
            ..Settings::default()
        },
    ] {
        assert!(
            w.execute(Command::Configure {
                actor: a,
                settings: invalid
            })
            .is_err()
        );
        assert_eq!(before, w.save().unwrap());
    }
    assert_eq!(
        w.execute(Command::Message {
            game: g,
            authority: EventAuthority::Owner(a),
            kind: MessageKind::Center { seconds: 0 },
            text: "hello".into()
        }),
        Err(Error::InvalidEvent)
    );
    assert!(Preset::from_json(&vec![b' '; 65537], w.catalog()).is_err());
    assert_eq!(
        w.pick_spawn(
            a,
            &vec![
                SpawnPoint {
                    id: 1,
                    owner: a.account
                };
                65537
            ],
            0
        ),
        Err(Error::Capacity)
    );
    assert!(
        MinigamesWorld::restore(&vec![b' '; MAX_SNAPSHOT_BYTES + 1], w.catalog().clone()).is_err()
    );
    assert_eq!(before, w.save().unwrap());
}

#[test]
fn colors_limit_games_and_owner_disconnect_cleans_invites() {
    let mut w = world(PolicyMode::Internet);
    let mut owners = vec![];
    for i in 0..10 {
        let p = connect(&mut w, i + 1);
        create(&mut w, p, i as u8);
        owners.push(p);
    }
    let outside = connect(&mut w, 11);
    assert_eq!(
        w.execute(Command::Create {
            actor: outside,
            color: 0,
            settings: Settings::default()
        }),
        Err(Error::ColorUnavailable)
    );
    w.execute(Command::Invite {
        actor: owners[0],
        target: outside,
    })
    .unwrap();
    w.disconnect(owners[0]).unwrap();
    assert_eq!(w.player(outside).unwrap().invite, None);
    assert_eq!(w.free_colors(), vec![0]);
    create(&mut w, outside, 0);
}

#[test]
fn brick_scoring_resource_lifecycle_and_outside_admin_respawn() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let g = create(&mut w, a, 0);
    configure(&mut w, a, |s| {
        s.points_plant_brick = 3;
        s.points_break_brick = -2;
    });
    w.brick_score(a, true).unwrap();
    w.brick_score(a, false).unwrap();
    assert_eq!(w.player(a).unwrap().score, 1);
    assert_eq!(w.wheeled_destroy_respawn_delay(Some(g), 6000).unwrap(), 732);
    assert_eq!(w.wheeled_destroy_respawn_delay(Some(g), 1000).unwrap(), 612);
    assert_eq!(w.wheeled_destroy_respawn_delay(None, 1000).unwrap(), 132);
    assert_eq!(w.respawn_delay(None, RespawnObject::Vehicle).unwrap(), 0);
    w.execute(Command::End { actor: a }).unwrap();
    w.set_admin(a, true).unwrap();
    let current = life(&w, a);
    w.died(a, current, None).unwrap();
    w.execute(Command::Respawn { actor: a }).unwrap(); // outside minigame admin exception
    create(&mut w, a, 0);
    let current = life(&w, a);
    w.died(a, current, None).unwrap();
    assert_eq!(
        w.execute(Command::Respawn { actor: a }),
        Err(Error::RespawnNotReady)
    );
}

#[test]
fn registry_capacity_and_ready_gate_are_explicit() {
    let mut w = world(PolicyMode::Internet);
    let a = w.connect(AccountId(1), "Loading".into(), false).unwrap();
    assert_eq!(
        w.execute(Command::Create {
            actor: a,
            color: 0,
            settings: Settings::default()
        }),
        Err(Error::NotReady)
    );
    assert_eq!(
        w.connect(AccountId(1), "Duplicate".into(), false),
        Err(Error::InvalidSettings)
    );
    for i in 2..=MAX_PLAYERS as u64 {
        connect(&mut w, i);
    }
    assert_eq!(
        w.connect(AccountId(2000), "Overflow".into(), false),
        Err(Error::Capacity)
    );
}

#[test]
fn malformed_snapshot_deadline_and_membership_cannot_panic_later() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    create(&mut w, a, 0);
    let current = life(&w, a);
    w.died(a, current, None).unwrap();
    let bytes = w.save().unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["players"][0]["life"]["Dead"]["ready_at"] = 0.into();
    assert!(
        MinigamesWorld::restore(&serde_json::to_vec(&value).unwrap(), w.catalog().clone()).is_err()
    );
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["games"][0]["members"] = serde_json::json!([]);
    assert!(
        MinigamesWorld::restore(&serde_json::to_vec(&value).unwrap(), w.catalog().clone()).is_err()
    );
}

/// Add-On items and their sports images use the platform id grammar; the
/// catalog accepts them by the same rule as base items instead of refusing
/// the whole catalog.
#[test]
fn catalog_accepts_add_on_content_ids() {
    let mut catalog = Catalog::minimal_vanilla();
    catalog
        .items
        .insert("addon_shotgun:weapon/shotgunitem".into(), None);
    catalog.items.insert(
        "addon_balls:weapon/beachballitem".into(),
        Some("addon_balls:image/beachballimage".into()),
    );
    assert!(MinigamesWorld::new(catalog.clone(), PolicyMode::Internet, true).is_ok());
    catalog.items.insert("Not An Id".into(), None);
    assert!(MinigamesWorld::new(catalog, PolicyMode::Internet, true).is_err());
}

#[test]
fn a_game_modes_minigame_belongs_to_the_server_and_holds_everyone() {
    let mut w = world(PolicyMode::Internet);
    let a = connect(&mut w, 1);
    let b = connect(&mut w, 2);
    let game = w.host_create(3, Settings::default()).unwrap();
    assert!(w.game(game).unwrap().is_server());
    assert_eq!(w.server_game(), Some(game));
    // Only one runs, and it takes its colour.
    assert_eq!(
        w.host_create(4, Settings::default()),
        Err(Error::ServerGame)
    );
    assert!(!w.free_colors().contains(&3));
    for p in [a, b] {
        let effects = w.host_place(p, Some(game)).unwrap();
        assert!(effects.iter().any(|e| matches!(e, Effect::Spawn { .. })));
        assert_eq!(w.player(p).unwrap().game, Some(game));
    }
    // Nobody starts, joins or leaves another; nobody owns it.
    assert_eq!(
        w.execute(Command::Leave { actor: a }),
        Err(Error::ServerGame)
    );
    assert_eq!(
        w.execute(Command::Create {
            actor: a,
            color: 0,
            settings: Settings::default()
        }),
        Err(Error::ServerGame)
    );
    assert_eq!(w.execute(Command::End { actor: a }), Err(Error::NotOwner));
    assert_eq!(
        w.can_damage(DamageSource::Player(a), w.target_for_player(b).unwrap()),
        Decision::Allow
    );
    // The world's own bricks (owner 0) are the game's bricks.
    let world_brick = Target::Object {
        kind: ObjectKind::Brick,
        owner: Some(SERVER.account),
        membership: Membership::Owner,
        spawn_brick: false,
    };
    assert_eq!(
        w.can_damage(DamageSource::Player(a), world_brick),
        Decision::Allow
    );
    // A member leaving the server leaves the game running.
    w.disconnect(a).unwrap();
    assert_eq!(w.server_game(), Some(game));
    assert_eq!(w.game(game).unwrap().members.len(), 1);
    // It saves and restores like any other.
    let restored = MinigamesWorld::restore(&w.save().unwrap(), w.catalog().clone()).unwrap();
    assert_eq!(restored.server_game(), Some(game));
}
