use bri_ui::api::RequestId;
use bri_ui::models::admin::{
    AdminAction, AdminBan, AdminFeature, AdminModel, AdminOptions, AdminPlayer, AdminQuotas,
    AdminRole, AdminSnapshot, AdminUpdate,
};
use std::collections::BTreeSet;

fn options() -> AdminOptions {
    AdminOptions {
        name: "Test Server".into(),
        port: 28000,
        max_players: 32,
        brick_limit: 10000,
        bricks_per_second: 20,
        max_chat_length: 120,
        physics_vehicles: 8,
        player_vehicles: 4,
        random_brick_color: false,
        chat_filter: false,
        falling_damage: false,
        public_domain_timeout_minutes: -1,
        too_far_distance: 100.0,
        per_player: AdminQuotas {
            schedules: 1,
            misc: 1,
            projectiles: 1,
            items: 1,
            environment: 1,
            players: 1,
            vehicles: 1,
        },
        lan: AdminQuotas {
            schedules: 1,
            misc: 1,
            projectiles: 1,
            items: 1,
            environment: 1,
            players: 1,
            vehicles: 1,
        },
    }
}

fn state(role: AdminRole, local_host: bool) -> AdminSnapshot {
    AdminSnapshot {
        revision: 1,
        role,
        local_host,
        legacy_lan: false,
        supported: [
            AdminFeature::Login,
            AdminFeature::Kick,
            AdminFeature::Ban,
            AdminFeature::Unban,
            AdminFeature::Spy,
            AdminFeature::Wand,
            AdminFeature::Maps,
            AdminFeature::ClearBricks,
            AdminFeature::HostOptions,
            AdminFeature::AdminPassword,
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
        players: vec![AdminPlayer {
            connection: 7,
            name: "Builder".into(),
            identity_label: "Native identity".into(),
            role: AdminRole::Player,
            owner: false,
            local: false,
            bot: false,
            persistent_identity: true,
        }],
        options: None,
    }
}

#[test]
fn role_and_host_capabilities_gate_admin_actions() {
    let mut model = AdminModel::default();
    model
        .apply(AdminUpdate::State(state(AdminRole::Player, false)))
        .unwrap();
    assert!(!model.allowed(&AdminAction::Kick { target: 7 }));
    assert!(!model.allowed(&AdminAction::ConfigureHost {
        options: Box::new(options())
    }));

    model
        .apply(AdminUpdate::State(state(AdminRole::Admin, false)))
        .unwrap();
    assert!(model.allowed(&AdminAction::Kick { target: 7 }));
    assert!(!model.allowed(&AdminAction::ConfigureHost {
        options: Box::new(options())
    }));

    model
        .apply(AdminUpdate::State(state(AdminRole::SuperAdmin, false)))
        .unwrap();
    assert!(model.allowed(&AdminAction::SetPassword {
        slot: bri_ui::models::admin::AdminPasswordSlot::Admin,
        password: bri_ui::models::admin::AdminSecret("secret".into()),
    }));
}

#[test]
fn confirmation_targets_are_rechecked_and_protected_players_cannot_be_kicked() {
    let mut model = AdminModel::default();
    let mut snapshot = state(AdminRole::Admin, false);
    snapshot.players[0].role = AdminRole::SuperAdmin;
    model.apply(AdminUpdate::State(snapshot)).unwrap();
    assert!(!model.allowed(&AdminAction::Kick { target: 7 }));

    let mut snapshot = state(AdminRole::Admin, false);
    snapshot.players[0].owner = true;
    model.apply(AdminUpdate::State(snapshot)).unwrap();
    assert!(!model.allowed(&AdminAction::Ban {
        target: 7,
        minutes: Some(10),
        reason: String::new(),
    }));
}

#[test]
fn ban_lists_correlate_to_the_matching_request_and_stable_ban_ids() {
    let mut model = AdminModel::default();
    model
        .apply(AdminUpdate::State(state(AdminRole::Admin, false)))
        .unwrap();
    let request: RequestId = 42;
    model.pending.insert(request, AdminAction::RequestBans);
    let rows = vec![AdminBan {
        id: 9,
        administrator: "Host".into(),
        name: "Builder".into(),
        identity_label: "verified key".into(),
        address: None,
        reason: "Repeated griefing".into(),
        remaining_minutes: None,
    }];

    assert!(
        model
            .apply(AdminUpdate::Bans {
                request: 41,
                revision: 1,
                rows: rows.clone(),
            })
            .is_err()
    );
    assert!(model.bans.is_empty());
    model
        .apply(AdminUpdate::Bans {
            request,
            revision: 1,
            rows,
        })
        .unwrap();
    assert!(model.allowed(&AdminAction::Unban { ban: 9 }));
    assert!(!model.allowed(&AdminAction::Unban { ban: 8 }));
}

#[test]
fn stale_state_cannot_replace_authoritative_roles_and_secrets_redact_debug() {
    let mut model = AdminModel::default();
    model
        .apply(AdminUpdate::State(state(AdminRole::Admin, false)))
        .unwrap();
    let mut stale = state(AdminRole::Player, false);
    stale.revision = 0;
    assert!(model.apply(AdminUpdate::State(stale)).is_err());
    assert!(model.is_admin());

    let secret = bri_ui::models::admin::AdminSecret("do-not-log".into());
    assert!(!format!("{secret:?}").contains("do-not-log"));
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "explicit offscreen source-skin inspection; requires content/ui-pack-003 and a headless GPU adapter"]
fn source_admin_screens_render_offscreen() -> anyhow::Result<()> {
    use bri_ui::{
        api::Settings,
        binds::Platform,
        gpu::{Headless, UiRenderer},
        models::admin::{AdminBrickGroup, AdminConfirmation, AdminMap},
        pack::Pack,
        screens::ScreenId,
        ui::{Ui, UiConfig},
    };
    use std::{path::PathBuf, rc::Rc};

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pack_dir = workspace.join("content/ui-pack-003");
    if !pack_dir.join("ui-pack.json").exists() {
        return Ok(());
    }
    let pack = Rc::new(Pack::load(&pack_dir)?);
    let gpu = Headless::new()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    let out = workspace.join("artifacts/native-admin-ui");
    std::fs::create_dir_all(&out)?;

    for (name, id) in [
        ("menu", ScreenId::Admin),
        ("login", ScreenId::AdminLogin),
        ("ban", ScreenId::AdminBan),
        ("unban", ScreenId::AdminUnban),
        ("bricks", ScreenId::AdminBricks),
        ("maps", ScreenId::AdminMaps),
        ("host-options", ScreenId::AdminOptions),
        ("credentials", ScreenId::AdminCredentials),
        ("confirm", ScreenId::AdminConfirm),
    ] {
        let mut ui = Ui::new(
            pack.clone(),
            UiConfig {
                size: (1024, 768),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        ui.core.admin.snapshot = Some(state(AdminRole::SuperAdmin, true));
        ui.core.admin.snapshot.as_mut().unwrap().options = Some(options());
        ui.core.admin.selected_player = Some(7);
        ui.core.admin.bans = vec![AdminBan {
            id: 9,
            administrator: "Host".into(),
            name: "Builder".into(),
            identity_label: "Native identity".into(),
            address: None,
            reason: "Repeated griefing".into(),
            remaining_minutes: Some(1439),
        }];
        ui.core.admin.groups = vec![AdminBrickGroup {
            id: 12,
            name: "Builder's Bricks".into(),
            identity_label: "Native identity".into(),
            bricks: 256,
        }];
        ui.core.admin.maps = vec![AdminMap {
            id: "bedroom".into(),
            name: "Bedroom".into(),
        }];
        if id == ScreenId::AdminConfirm {
            ui.core.admin.confirmation = Some(AdminConfirmation {
                title: "Kick Player?".into(),
                text: "Kick Builder (Native identity)?".into(),
                action: AdminAction::Kick { target: 7 },
            });
        }
        ui.core.push(id);
        ui.update(0);
        let draw = ui.draw();
        let pixels = gpu.render_rgba(
            &mut renderer,
            &pack,
            &draw,
            (1024, 768),
            1.0,
            [0.12, 0.16, 0.22, 1.0],
        )?;
        image::save_buffer(
            out.join(format!("{name}.png")),
            &pixels,
            1024,
            768,
            image::ColorType::Rgba8,
        )?;
    }
    Ok(())
}

#[test]
fn unowned_group_zero_is_valid_but_does_not_enable_unbound_highlighting() {
    use bri_ui::models::admin::AdminBrickGroup;
    let mut model = AdminModel::default();
    model
        .apply(AdminUpdate::State(state(AdminRole::Admin, false)))
        .unwrap();
    model.pending.insert(8, AdminAction::RequestBrickGroups);
    let row = AdminBrickGroup {
        id: 0,
        name: "Unowned".into(),
        identity_label: "Unavailable".into(),
        bricks: 2,
    };
    model
        .apply(AdminUpdate::BrickGroups {
            request: 8,
            revision: 1,
            rows: vec![row.clone()],
        })
        .unwrap();
    assert!(model.allowed(&AdminAction::ClearBrickGroup { group: 0 }));
    assert!(!model.allowed(&AdminAction::HighlightBrickGroup { group: 0 }));
    model.pending.insert(9, AdminAction::RequestBrickGroups);
    assert!(
        model
            .apply(AdminUpdate::BrickGroups {
                request: 9,
                revision: 1,
                rows: vec![row.clone(), row]
            })
            .is_err()
    );
    assert_eq!(model.groups.len(), 1);
}

#[test]
fn unban_waits_for_correlated_host_acceptance_and_clears_stale_selection() {
    let mut model = AdminModel::default();
    model
        .apply(AdminUpdate::State(state(AdminRole::Admin, false)))
        .unwrap();
    model.bans.push(AdminBan {
        id: 27,
        administrator: "Host".into(),
        name: "Builder".into(),
        identity_label: "Native identity".into(),
        address: None,
        reason: "Test".into(),
        remaining_minutes: Some(15),
    });
    model.selected_ban = Some(27);
    model.pending.insert(1, AdminAction::Unban { ban: 27 });
    assert!(!model.result(99, &Ok(())));
    assert!(model.result(1, &Err("Storage unavailable".into())));
    assert_eq!(model.bans.len(), 1);
    assert_eq!(model.selected_ban, Some(27));
    model.pending.insert(2, AdminAction::Unban { ban: 27 });
    assert!(model.result(2, &Ok(())));
    assert!(model.bans.is_empty());
    assert_eq!(model.selected_ban, None);
    // Expiry or another administrator can also remove a selected row.
    model.selected_ban = Some(27);
    model.pending.insert(3, AdminAction::RequestBans);
    model
        .apply(AdminUpdate::Bans {
            request: 3,
            revision: 1,
            rows: vec![],
        })
        .unwrap();
    assert_eq!(model.selected_ban, None);
}

#[test]
fn confirming_change_map_closes_the_map_and_admin_menus() {
    use bri_ui::{
        api::{ConnectionState, Settings, UiUpdate},
        binds::Platform,
        geom::Rect,
        input::{InputEvent, Key, Modifiers},
        models::admin::{AdminConfirmation, AdminMap},
        pack::Pack,
        schema::UiPack,
        screens::{ScreenId, ctrl},
        ui::{Ui, UiConfig},
    };
    use std::{path::PathBuf, rc::Rc};
    let mut pack = UiPack::default();
    for name in [
        "MainMenuGui",
        "PlayGui",
        "LoadingGui",
        "adminGui",
        "changeMapGui",
        "MessageBoxYesNoDlg",
    ] {
        pack.layouts.insert(
            name.into(),
            ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480)),
        );
    }
    let mut ui = Ui::new(
        Rc::new(Pack::from_parts(pack, PathBuf::new())),
        UiConfig {
            size: (640, 480),
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        Settings {
            binds: Some(vec![]),
            ..Default::default()
        },
    );
    ui.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: true,
        single_player: true,
        admin: true,
    }));
    ui.core
        .admin
        .apply(AdminUpdate::State(state(AdminRole::SuperAdmin, true)))
        .unwrap();
    ui.core.admin.maps = vec![AdminMap {
        id: "bedroom".into(),
        name: "Bedroom".into(),
    }];
    ui.core.admin.confirmation = Some(AdminConfirmation {
        title: "Change Map?".into(),
        text: "Change to Bedroom?".into(),
        action: AdminAction::ChangeMap {
            map: "bedroom".into(),
        },
    });
    for id in [ScreenId::Admin, ScreenId::AdminMaps, ScreenId::AdminConfirm] {
        ui.core.push(id);
    }
    ui.update(0);
    ui.drain_actions();
    // The host has answered the map list request by the time Max confirms.
    ui.core.admin.pending.clear();
    assert_eq!(ui.top_id(), ScreenId::AdminConfirm);
    ui.handle_input(InputEvent::KeyDown {
        key: Key::Return,
        mods: Modifiers::NONE,
        repeat: false,
    });
    assert!(
        ui.core
            .admin
            .pending
            .values()
            .any(|a| matches!(a, AdminAction::ChangeMap { .. })),
        "the confirmed map change is requested"
    );
    ui.update(16);
    let stack = ui.stack();
    for id in [ScreenId::Admin, ScreenId::AdminMaps, ScreenId::AdminConfirm] {
        assert!(!stack.contains(&id), "{id:?} stays open: {stack:?}");
    }
}

#[test]
fn logging_in_or_out_updates_every_admin_check() {
    use bri_ui::{
        api::{ConnectionState, Settings, UiUpdate},
        binds::Platform,
        pack::Pack,
        schema::UiPack,
        ui::{Ui, UiConfig},
    };
    use std::{path::PathBuf, rc::Rc};
    let mut ui = Ui::new(
        Rc::new(Pack::from_parts(UiPack::default(), PathBuf::new())),
        UiConfig {
            size: (640, 480),
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        Settings {
            binds: Some(vec![]),
            ..Default::default()
        },
    );
    ui.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: false,
        single_player: false,
        admin: false,
    }));
    assert!(!ui.core.is_admin());
    // Load Bricks and the admin window read the same live role.
    ui.core
        .admin
        .apply(AdminUpdate::State(state(AdminRole::Admin, false)))
        .unwrap();
    assert!(ui.core.is_admin());
    let mut demoted = state(AdminRole::Player, false);
    demoted.revision = 2;
    ui.core.admin.apply(AdminUpdate::State(demoted)).unwrap();
    assert!(!ui.core.is_admin());
}
