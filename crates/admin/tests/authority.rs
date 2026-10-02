use bri_admin::*;
fn id(n: u64) -> ConnectionId {
    ConnectionId(n)
}
fn principal(n: u8) -> Principal {
    Principal([n; 32])
}
fn connection(n: u64) -> TrustedConnection {
    TrustedConnection {
        id: id(n),
        display_name: format!("Player {n}"),
        principal: Some(principal(n as u8)),
        is_owner: n == 1,
        is_local: n == 1,
        is_bot: false,
    }
}
fn run(s: &mut Administration, actor: u64, action: Action) -> Result<Vec<Effect>, Error> {
    s.handle(
        Origin::Connection(id(actor)),
        Request::new(action),
        1000,
        |_| None,
    )
}
fn setup() -> Administration {
    let mut s = Administration::default();
    for n in 1..=5 {
        s.connect(connection(n), 0).unwrap();
    }
    run(
        &mut s,
        1,
        Action::HostSetRole {
            target: id(2),
            role: Role::Admin,
        },
    )
    .unwrap();
    run(
        &mut s,
        1,
        Action::HostSetRole {
            target: id(3),
            role: Role::SuperAdmin,
        },
    )
    .unwrap();
    s
}
#[test]
fn transport_actor_cannot_be_supplied_by_request_and_stale_ids_do_not_rebind() {
    assert!(
        Request::decode(br#"{"schema_version":1,"actor":1,"action":{"kind":"ClearAllBricks"}}"#)
            .is_err()
    );
    assert!(Request::decode(br#"{"schema_version":1,"action":{"kind":"Kick","value":{"target":4,"role":"SuperAdmin"}}}"#).is_err());
    let mut s = setup();
    assert!(matches!(
        run(
            &mut s,
            4,
            Action::HostSetRole {
                target: id(4),
                role: Role::SuperAdmin
            }
        ),
        Err(Error::Denied)
    ));
    assert_eq!(s.role(id(4)), Some(Role::Player));
    s.disconnect(id(2));
    assert!(matches!(s.connect(connection(2), 0), Err(Error::Duplicate)));
    assert!(matches!(
        run(&mut s, 2, Action::ClearAllBricks),
        Err(Error::UnknownConnection)
    ));
}
#[test]
fn source_permission_matrix_and_protected_targets() {
    let actions = [
        Action::RequestMaps,
        Action::RequestBrickGroups,
        Action::RequestBanList,
        Action::DestructoWand,
        Action::ClearAllBricks,
        Action::ClearVehicles,
        Action::ClearBots,
        Action::Warp,
        Action::Spy { target: id(4) },
        Action::Fetch { target: id(4) },
        Action::Find { target: id(4) },
        Action::ReturnToPreviousPosition,
        Action::DropPlayerAtCamera,
        Action::DropCameraAtPlayer,
        Action::RealBrickCount,
        Action::ResetVehicles,
        Action::CancelAllEvents,
        Action::TimeScale { scale: 3.0 },
        Action::ChangeMap {
            map: "v20.map.bedroom".into(),
        },
    ];
    for action in actions {
        for actor in 1..=4 {
            let mut s = setup();
            assert_eq!(
                run(&mut s, actor, action.clone()).is_ok(),
                actor <= 3,
                "{actor}: {action:?}"
            );
        }
    }
    for actor in 1..=3 {
        for target in [1, 3] {
            let mut s = setup();
            assert!(matches!(
                run(&mut s, actor, Action::Kick { target: id(target) }),
                Err(Error::Protected)
            ));
            assert!(matches!(
                run(
                    &mut s,
                    actor,
                    Action::Ban {
                        target: id(target),
                        duration: BanDuration::Forever,
                        reason: String::new()
                    }
                ),
                Err(Error::Protected)
            ));
        }
    }
    let mut s = setup();
    assert!(
        s.handle(
            Origin::HostConsole,
            Request::new(Action::Kick { target: id(3) }),
            0,
            |_| None
        )
        .is_err()
    );
    assert!(run(&mut s, 2, Action::Kick { target: id(2) }).is_ok()); // no invented peer-Admin protection
    assert!(matches!(
        run(
            &mut s,
            2,
            Action::SetAdminPassword {
                password: Secret::new("x".into()).unwrap()
            }
        ),
        Err(Error::Denied)
    ));
    assert!(
        run(
            &mut s,
            3,
            Action::SetAdminPassword {
                password: Secret::new("x".into()).unwrap()
            }
        )
        .is_ok()
    );
    assert!(matches!(
        run(
            &mut s,
            3,
            Action::HostConfigure {
                settings: ServerSettings::default()
            }
        ),
        Err(Error::HostOnly)
    ));
}

#[test]
fn bounded_capacity_and_deadline_overflow_do_not_mutate_state() {
    let mut s = setup();
    let before = serde_json::to_vec(s.durable()).unwrap();
    assert!(matches!(
        s.handle(
            Origin::Connection(id(2)),
            Request::new(Action::Ban {
                target: id(4),
                duration: BanDuration::Minutes(1),
                reason: String::new()
            }),
            u64::MAX,
            |_| None
        ),
        Err(Error::InvalidTime)
    ));
    assert_eq!(serde_json::to_vec(s.durable()).unwrap(), before);
    assert!(matches!(
        Request::decode(&vec![b' '; MAX_REQUEST_BYTES + 1]),
        Err(Error::Budget)
    ));
    let mut state = DurableState::default();
    for n in 1..=MAX_BANS {
        let mut p = [0; 32];
        p[..8].copy_from_slice(&(n as u64).to_le_bytes());
        state.bans.push(BanRecord {
            id: BanId(n as u64),
            principal: Principal(p),
            victim_name: "Fixture".into(),
            issued_by: "Host".into(),
            reason: String::new(),
            created_unix_seconds: 0,
            expires_unix_seconds: None,
        });
    }
    state.next_ban_id = MAX_BANS as u64 + 1;
    state.validate().unwrap();
    let bytes = serde_json::to_vec(&state).unwrap();
    s.restore(bytes.as_slice()).unwrap();
    assert!(matches!(
        run(
            &mut s,
            2,
            Action::Ban {
                target: id(4),
                duration: BanDuration::Forever,
                reason: String::new()
            }
        ),
        Err(Error::Budget)
    ));
    assert_eq!(serde_json::to_vec(s.durable()).unwrap(), bytes);
    state.bans.push(state.bans[0].clone());
    assert!(matches!(state.validate(), Err(Error::Budget)));
}
#[test]
fn passwords_fourth_failure_disconnects_and_do_not_serialize_into_state() {
    let mut s = setup();
    let login = |pw: &str| {
        Request::new(Action::Login {
            password: Secret::new(pw.into()).unwrap(),
        })
    };
    assert_eq!(
        s.handle(Origin::Connection(id(4)), login(""), 0, |_| panic!(
            "empty password verifier"
        ))
        .unwrap(),
        vec![Effect::LoginIgnored]
    );
    for attempt in 1..=4 {
        let effects = s
            .handle(Origin::Connection(id(4)), login("secret-text"), 0, |_| None)
            .unwrap();
        assert_eq!(
            effects[0],
            Effect::LoginRejected {
                attempts: attempt,
                disconnect: attempt == 4
            }
        );
        assert_eq!(effects.len(), if attempt == 4 { 2 } else { 1 });
    }
    assert!(matches!(
        s.handle(Origin::Connection(id(4)), login("valid"), 0, |_| Some(
            Role::SuperAdmin
        )),
        Err(Error::UnknownConnection)
    ));
    assert!(!format!("{:?}", login("secret-text")).contains("secret-text"));
    let mut bytes = Vec::new();
    s.durable().write(&mut bytes).unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains("secret-text"));
    s.handle(Origin::Connection(id(3)), login("admin"), 0, |_| {
        Some(Role::Admin)
    })
    .unwrap();
    assert_eq!(s.role(id(3)), Some(Role::Admin)); // source login can lower SA
}
#[test]
fn bans_survive_restore_expire_replace_and_unban_by_stable_id() {
    let mut s = setup();
    let effects = run(
        &mut s,
        2,
        Action::Ban {
            target: id(4),
            duration: BanDuration::Minutes(2),
            reason: "Repeated destruction".into(),
        },
    )
    .unwrap();
    assert_eq!(
        effects[1],
        Effect::Disconnect {
            target: id(4),
            reason: DisconnectReason::Banned(BanId(1))
        }
    );
    assert!(s.is_banned(principal(4), 1119));
    assert!(!s.is_banned(principal(4), 1120));
    let mut bytes = Vec::new();
    s.durable().write(&mut bytes).unwrap();
    let mut restored = Administration::default();
    restored.restore(bytes.as_slice()).unwrap();
    assert!(restored.is_banned(principal(4), 1050));
    let mut reconnect = connection(6);
    reconnect.principal = Some(principal(4));
    assert!(matches!(
        restored.connect(reconnect.clone(), 1050),
        Err(Error::Banned)
    ));
    assert!(restored.connect(reconnect, 1120).is_ok());
    run(
        &mut s,
        2,
        Action::Ban {
            target: id(4),
            duration: BanDuration::Forever,
            reason: "again".into(),
        },
    )
    .unwrap();
    assert_eq!(s.durable().bans.len(), 1);
    assert_eq!(s.durable().bans[0].id, BanId(2));
    assert!(matches!(
        run(&mut s, 2, Action::Unban { ban: BanId(1) }),
        Err(Error::UnknownBan)
    ));
    run(&mut s, 2, Action::Unban { ban: BanId(2) }).unwrap();
    assert!(!s.is_banned(principal(4), u64::MAX));
}
#[test]
fn ban_principal_is_host_bound_and_all_connections_are_protected() {
    let mut s = setup();
    let mut same = connection(6);
    same.principal = Some(principal(3));
    s.connect(same, 0).unwrap();
    assert!(matches!(
        run(
            &mut s,
            2,
            Action::Ban {
                target: id(6),
                duration: BanDuration::Forever,
                reason: String::new()
            }
        ),
        Err(Error::Protected)
    ));
    let mut anonymous = connection(7);
    anonymous.principal = None;
    s.connect(anonymous, 0).unwrap();
    assert!(matches!(
        run(
            &mut s,
            2,
            Action::Ban {
                target: id(7),
                duration: BanDuration::Forever,
                reason: String::new()
            }
        ),
        Err(Error::IdentityUnavailable)
    ));
    assert!(s.durable().bans.is_empty());
    assert!(run(&mut s, 2, Action::Kick { target: id(7) }).is_ok());
}
#[test]
fn durable_corruption_is_atomic_and_reads_are_bounded() {
    let mut s = setup();
    run(
        &mut s,
        1,
        Action::HostSetAutoRole {
            principal: principal(6),
            role: Role::SuperAdmin,
        },
    )
    .unwrap();
    run(
        &mut s,
        2,
        Action::Ban {
            target: id(4),
            duration: BanDuration::Forever,
            reason: String::new(),
        },
    )
    .unwrap();
    let baseline = serde_json::to_vec(s.durable()).unwrap();
    let mut corruptions = Vec::new();
    let mut bad = s.durable().clone();
    bad.bans.push(bad.bans[0].clone());
    corruptions.push(bad);
    let mut bad = s.durable().clone();
    bad.schema_version = 2;
    corruptions.push(bad);
    let mut bad = s.durable().clone();
    bad.next_ban_id = 1;
    corruptions.push(bad);
    let mut bad = s.durable().clone();
    bad.bans[0].expires_unix_seconds = Some(1);
    corruptions.push(bad);
    let mut bad = s.durable().clone();
    bad.auto_roles.push(bad.auto_roles[0].clone());
    corruptions.push(bad);
    let mut bad = s.durable().clone();
    bad.bans[0].reason = "x".repeat(513);
    corruptions.push(bad);
    for bad in corruptions {
        assert!(
            s.restore(serde_json::to_vec(&bad).unwrap().as_slice())
                .is_err()
        );
        assert_eq!(serde_json::to_vec(s.durable()).unwrap(), baseline);
    }
    assert!(matches!(
        s.restore(std::io::repeat(b' ')),
        Err(Error::Budget)
    ));
    assert_eq!(serde_json::to_vec(s.durable()).unwrap(), baseline);
}
#[test]
fn auto_roles_need_a_super_admin_and_do_not_demote_online_clients() {
    let mut s = setup();
    // Admins (2) and players (4) can neither read nor change the saved list.
    for actor in [2, 4] {
        for action in [
            Action::HostSetAutoRole {
                principal: principal(6),
                role: Role::Admin,
            },
            Action::RequestAutoRoles,
        ] {
            assert!(matches!(run(&mut s, actor, action), Err(Error::Denied)));
        }
    }
    // A Super Admin (3) reads it: the ranks given in `setup`, with names.
    let Ok(effects) = run(&mut s, 3, Action::RequestAutoRoles) else {
        panic!("Super Admin could not read the saved ranks")
    };
    let [Effect::AutoRoleList(rows)] = effects.as_slice() else {
        panic!("{effects:?}")
    };
    let names: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.role)).collect();
    assert_eq!(
        names,
        [("Player 2", Role::Admin), ("Player 3", Role::SuperAdmin)]
    );
    run(
        &mut s,
        1,
        Action::HostSetAutoRole {
            principal: principal(6),
            role: Role::Admin,
        },
    )
    .unwrap();
    run(
        &mut s,
        1,
        Action::HostSetAutoRole {
            principal: principal(6),
            role: Role::SuperAdmin,
        },
    )
    .unwrap();
    // Players 2 and 3 were ranked in `setup`, so they are saved too.
    assert_eq!(s.durable().auto_roles.len(), 3);
    assert_eq!(s.connect(connection(6), 0).unwrap(), Role::SuperAdmin);
    run(
        &mut s,
        1,
        Action::HostSetAutoRole {
            principal: principal(6),
            role: Role::Player,
        },
    )
    .unwrap();
    assert_eq!(s.role(id(6)), Some(Role::SuperAdmin));
    run(
        &mut s,
        1,
        Action::HostSetRole {
            target: id(6),
            role: Role::Player,
        },
    )
    .unwrap();
    assert_eq!(s.role(id(6)), Some(Role::Player));
    assert!(matches!(
        run(
            &mut s,
            1,
            Action::HostSetRole {
                target: id(1),
                role: Role::Player
            }
        ),
        Err(Error::Protected)
    ));
}
#[test]
fn typed_command_validation_clamps_authored_time_scale_and_keeps_minigame_owner_gate() {
    let mut s = setup();
    assert_eq!(
        run(&mut s, 2, Action::TimeScale { scale: 30. }).unwrap(),
        vec![Effect::Gameplay {
            actor: Some(id(2)),
            command: GameplayCommand::TimeScale(2.)
        }]
    );
    for bad in [f32::NAN, f32::INFINITY] {
        assert!(run(&mut s, 2, Action::TimeScale { scale: bad }).is_err());
    }
    assert!(
        run(
            &mut s,
            2,
            Action::ChangeMap {
                map: "../x.mis".into()
            }
        )
        .is_err()
    );
    assert!(
        run(
            &mut s,
            2,
            Action::Ban {
                target: id(4),
                duration: BanDuration::Minutes(0),
                reason: String::new()
            }
        )
        .is_err()
    );
    assert!(!owns_minigame(id(3), id(4)));
    assert!(owns_minigame(id(4), id(4)));
    let mut settings = ServerSettings {
        port: 1,
        ..Default::default()
    };
    assert!(
        run(
            &mut s,
            1,
            Action::HostConfigure {
                settings: settings.clone()
            }
        )
        .is_err()
    );
    settings.port = 28000;
    assert!(run(&mut s, 1, Action::HostConfigure { settings }).is_ok());
}
#[test]
fn failed_logins_follow_the_identity_across_reconnects_and_expire() {
    let mut s = setup();
    let login = |s: &mut Administration, n: u64, now: u64, pw: &str| {
        s.handle(
            Origin::Connection(id(n)),
            Request::new(Action::Login {
                password: Secret::new(pw.into()).unwrap(),
            }),
            now,
            |attempt| (attempt == "valid").then_some(Role::Admin),
        )
    };
    for _ in 0..4 {
        login(&mut s, 4, 100, "wrong").unwrap();
    }
    // Player 4 reconnects under the same identity: no fresh guesses, and even
    // the right password is refused while the strikes last.
    s.disconnect(id(4));
    let mut again = connection(4);
    again.id = id(6);
    s.connect(again, 200).unwrap();
    let effects = login(&mut s, 6, 200, "valid").unwrap();
    assert_eq!(
        effects[0],
        Effect::LoginRejected {
            attempts: 5,
            disconnect: true
        }
    );
    assert_eq!(s.role(id(6)), Some(Role::Player));
    // After the window the identity starts over and may log in.
    s.disconnect(id(6));
    let mut later = connection(4);
    later.id = id(7);
    s.connect(later, 200 + bri_admin::LOGIN_STRIKE_SECONDS)
        .unwrap();
    login(&mut s, 7, 200 + bri_admin::LOGIN_STRIKE_SECONDS, "valid").unwrap();
    assert_eq!(s.role(id(7)), Some(Role::Admin));
    // An anonymous connection cannot guess at all.
    let mut anonymous = connection(8);
    anonymous.principal = None;
    s.connect(anonymous, 0).unwrap();
    assert!(matches!(login(&mut s, 8, 0, "valid"), Err(Error::Denied)));
}
#[test]
fn super_admins_grant_and_revoke_ranks_that_return_on_rejoin() {
    let mut s = setup();
    // An Admin cannot hand out ranks; a Super Admin can.
    assert!(matches!(
        run(
            &mut s,
            2,
            Action::HostSetRole {
                target: id(4),
                role: Role::Admin
            }
        ),
        Err(Error::Denied)
    ));
    let effects = run(
        &mut s,
        3,
        Action::HostSetRole {
            target: id(4),
            role: Role::SuperAdmin,
        },
    )
    .unwrap();
    assert!(effects.contains(&Effect::AutoRolesChanged));
    assert_eq!(s.role(id(4)), Some(Role::SuperAdmin));
    let saved = s
        .durable()
        .auto_roles
        .iter()
        .find(|a| a.principal == principal(4))
        .unwrap();
    assert_eq!(
        (saved.role, saved.name.as_str()),
        (Role::SuperAdmin, "Player 4")
    );
    // The saved rank follows the key back in, whatever name it now uses.
    s.disconnect(id(4));
    let mut back = connection(9);
    back.principal = Some(principal(4));
    back.display_name = "Renamed".into();
    assert_eq!(s.connect(back, 0).unwrap(), Role::SuperAdmin);
    // Someone else under the old name gets nothing.
    let mut imposter = connection(10);
    imposter.display_name = "Player 4".into();
    assert_eq!(s.connect(imposter, 0).unwrap(), Role::Player);
    // A Super Admin can take another Super Admin's rank away, but never the host's.
    run(
        &mut s,
        3,
        Action::HostSetRole {
            target: id(9),
            role: Role::Player,
        },
    )
    .unwrap();
    assert_eq!(s.role(id(9)), Some(Role::Player));
    assert!(
        s.durable()
            .auto_roles
            .iter()
            .all(|a| a.principal != principal(4))
    );
    assert!(matches!(
        run(
            &mut s,
            3,
            Action::HostSetRole {
                target: id(1),
                role: Role::Player
            }
        ),
        Err(Error::Protected)
    ));
    // Without a verified key a rank lasts only for the visit.
    let mut anonymous = connection(11);
    anonymous.principal = None;
    s.connect(anonymous, 0).unwrap();
    let before = s.durable().auto_roles.len();
    let effects = run(
        &mut s,
        1,
        Action::HostSetRole {
            target: id(11),
            role: Role::Admin,
        },
    )
    .unwrap();
    assert_eq!(
        effects,
        vec![Effect::RoleChanged {
            target: id(11),
            role: Role::Admin
        }]
    );
    assert_eq!(s.durable().auto_roles.len(), before);
}
#[test]
fn saved_lists_without_names_still_load() {
    let old = br#"{"schema_version":1,"next_ban_id":1,"bans":[],"auto_roles":[{"principal":[7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7],"role":"Admin"}]}"#;
    let state = DurableState::read(&old[..]).unwrap();
    assert_eq!(state.auto_roles[0].name, "");
}
