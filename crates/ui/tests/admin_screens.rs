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
#[ignore = "explicit offscreen source-skin inspection; requires content/ui-pack-004 and a headless GPU adapter"]
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
    let pack_dir = workspace.join("content/ui-pack-004");
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
        ("saved-ranks", ScreenId::AdminRanks),
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
        // The host hands out ranks, so the rank buttons show.
        ui.core
            .admin
            .snapshot
            .as_mut()
            .unwrap()
            .supported
            .insert(AdminFeature::Ranks);
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
        ui.core.admin.saved_ranks = vec![
            bri_ui::models::admin::AdminSavedRank {
                key: "ab".repeat(32),
                name: "Builder".into(),
                role: AdminRole::Admin,
            },
            bri_ui::models::admin::AdminSavedRank {
                key: "cd".repeat(32),
                name: "Co-host".into(),
                role: AdminRole::SuperAdmin,
            },
        ];
        ui.core.admin.selected_rank = Some("ab".repeat(32));
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

#[test]
fn picking_the_destructo_wand_closes_the_admin_and_escape_menus() {
    use bri_ui::{
        api::{ConnectionState, Settings, UiUpdate},
        binds::Platform,
        geom::Rect,
        input::{InputEvent, MouseButton},
        pack::Pack,
        schema::UiPack,
        screens::{ScreenId, ctrl},
        ui::{Ui, UiConfig},
    };
    use std::{path::PathBuf, rc::Rc};
    let mut pack = UiPack::default();
    for name in ["MainMenuGui", "PlayGui", "LoadingGui", "escapeMenu"] {
        pack.layouts.insert(
            name.into(),
            ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480)),
        );
    }
    let mut admin = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
    let mut wand = ctrl("GuiButtonCtrl", "GuiButtonProfile", Rect::new(100, 100, 140, 30));
    wand.text = Some("Destructo Wand".into());
    wand.command = Some("AdminGui_Wand();".into());
    admin.children.push(wand);
    pack.layouts.insert("adminGui".into(), admin);
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
        .apply(AdminUpdate::State(state(AdminRole::Admin, false)))
        .unwrap();
    ui.core.push(ScreenId::EscapeMenu);
    ui.core.push(ScreenId::Admin);
    ui.update(0);
    ui.drain_actions();
    ui.core.admin.pending.clear();
    assert_eq!(ui.top_id(), ScreenId::Admin);
    let (x, y) = ui
        .control_center(ScreenId::Admin, "AdminGui_Wand();")
        .unwrap();
    ui.handle_input(InputEvent::MouseMove { x, y });
    ui.handle_input(InputEvent::MouseDown {
        button: MouseButton::Left,
        x,
        y,
    });
    ui.handle_input(InputEvent::MouseUp {
        button: MouseButton::Left,
        x,
        y,
    });
    assert!(
        ui.core
            .admin
            .pending
            .values()
            .any(|a| matches!(a, AdminAction::Wand)),
        "the wand is requested"
    );
    ui.update(16);
    let stack = ui.stack();
    for id in [ScreenId::Admin, ScreenId::EscapeMenu] {
        assert!(!stack.contains(&id), "{id:?} stays open: {stack:?}");
    }
}

#[test]
fn only_super_admins_and_the_host_change_ranks() {
    let mut model = AdminModel::default();
    let make_admin = AdminAction::SetRole {
        target: 7,
        role: AdminRole::Admin,
    };
    model
        .apply(AdminUpdate::State(state(AdminRole::Admin, false)))
        .unwrap();
    assert!(!model.allowed(&make_admin));
    let mut snapshot = state(AdminRole::SuperAdmin, false);
    snapshot.supported.insert(AdminFeature::Ranks);
    snapshot.revision = 2;
    model.apply(AdminUpdate::State(snapshot.clone())).unwrap();
    assert!(model.allowed(&make_admin));
    // Asking for the rank already held, or touching the host, is no change.
    assert!(!model.allowed(&AdminAction::SetRole {
        target: 7,
        role: AdminRole::Player
    }));
    snapshot.players[0].owner = true;
    snapshot.revision = 3;
    model.apply(AdminUpdate::State(snapshot)).unwrap();
    assert!(!model.allowed(&make_admin));
}

#[test]
fn saved_ranks_list_and_remove_only_what_the_host_listed() {
    use bri_ui::models::admin::AdminSavedRank;
    let mut model = AdminModel::default();
    let mut snapshot = state(AdminRole::SuperAdmin, false);
    snapshot.supported.insert(AdminFeature::Ranks);
    model.apply(AdminUpdate::State(snapshot)).unwrap();
    let key = "ab".repeat(32);
    assert!(!model.allowed(&AdminAction::ForgetRank { key: key.clone() }));
    model.pending.insert(3, AdminAction::RequestRanks);
    let row = AdminSavedRank {
        key: key.clone(),
        name: "Builder".into(),
        role: AdminRole::Admin,
    };
    // A malformed key or a "Player" rank is refused whole.
    for bad in [
        AdminSavedRank {
            key: "xy".into(),
            ..row.clone()
        },
        AdminSavedRank {
            role: AdminRole::Player,
            ..row.clone()
        },
    ] {
        assert!(
            model
                .apply(AdminUpdate::Ranks {
                    request: 3,
                    revision: 1,
                    rows: vec![bad],
                })
                .is_err()
        );
    }
    model
        .apply(AdminUpdate::Ranks {
            request: 3,
            revision: 1,
            rows: vec![row],
        })
        .unwrap();
    assert!(!model.busy());
    model.selected_rank = Some(key.clone());
    let forget = AdminAction::ForgetRank { key: key.clone() };
    assert!(model.allowed(&forget));
    model.pending.insert(4, forget);
    assert!(model.result(4, &Ok(())));
    assert!(model.saved_ranks.is_empty());
    assert_eq!(model.selected_rank, None);
    // An Admin does not see the list at all.
    let mut model = AdminModel::default();
    model
        .apply(AdminUpdate::State(state(AdminRole::Admin, false)))
        .unwrap();
    assert!(!model.allowed(&AdminAction::RequestRanks));
}

#[test]
fn the_admin_menu_rank_buttons_confirm_before_asking_the_host() {
    use bri_ui::{
        api::{ConnectionState, Settings, UiUpdate},
        binds::Platform,
        geom::Rect,
        input::{InputEvent, MouseButton},
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
        "escapeMenu",
        "adminGui",
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
        local: false,
        single_player: false,
        admin: true,
    }));
    let mut snapshot = state(AdminRole::SuperAdmin, false);
    snapshot.supported.insert(AdminFeature::Ranks);
    ui.core.admin.apply(AdminUpdate::State(snapshot)).unwrap();
    ui.core.admin.selected_player = Some(7);
    ui.core.push(ScreenId::EscapeMenu);
    ui.core.push(ScreenId::Admin);
    ui.update(0);
    ui.drain_actions();
    ui.core.admin.pending.clear();
    ui.update(16);
    let (x, y) = ui
        .control_center(ScreenId::Admin, "NativeMakeSuperAdmin")
        .unwrap();
    ui.handle_input(InputEvent::MouseMove { x, y });
    ui.handle_input(InputEvent::MouseDown {
        button: MouseButton::Left,
        x,
        y,
    });
    ui.handle_input(InputEvent::MouseUp {
        button: MouseButton::Left,
        x,
        y,
    });
    assert_eq!(ui.top_id(), ScreenId::AdminConfirm);
    assert_eq!(
        ui.core.admin.confirmation.as_ref().map(|c| c.action.clone()),
        Some(AdminAction::SetRole {
            target: 7,
            role: AdminRole::SuperAdmin
        })
    );
    assert!(ui.core.admin.pending.is_empty(), "nothing is sent unconfirmed");
}

#[test]
fn the_environment_window_applies_a_draft_through_the_host() {
    use bri_content::atmosphere::{Authored, Settings, light_direction};
    use bri_ui::{
        api::{ConnectionState, Settings as UiSettings, UiUpdate},
        binds::Platform,
        geom::Rect,
        input::{InputEvent, MouseButton},
        models::environment::EnvironmentView,
        pack::Pack,
        schema::UiPack,
        screens::{ScreenId, ctrl},
        ui::{Ui, UiConfig},
    };
    use std::{path::PathBuf, rc::Rc};
    let mut pack = UiPack::default();
    for name in ["MainMenuGui", "PlayGui", "LoadingGui", "escapeMenu", "adminGui"] {
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
        UiSettings {
            binds: Some(vec![]),
            ..Default::default()
        },
    );
    ui.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: false,
        single_player: false,
        admin: true,
    }));
    ui.apply(UiUpdate::Environment(EnvironmentView {
        authored: Authored {
            sun_direction: light_direction(90.0, 45.0),
            direct_light: [0.6; 3],
            ambient_light: [0.3; 3],
            fog_start: 0.0,
            fog_end: 0.0,
            fog_color: [0.5; 3],
        },
        settings: Settings::default(),
        tick: 0,
    }));
    let click = |ui: &mut Ui, screen: ScreenId, key: &str| {
        let (x, y) = ui.control_center(screen, key).unwrap();
        ui.handle_input(InputEvent::MouseMove { x, y });
        ui.handle_input(InputEvent::MouseDown {
            button: MouseButton::Left,
            x,
            y,
        });
        ui.handle_input(InputEvent::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        });
        ui.update(16);
    };
    // Plain players never see the button.
    ui.core
        .admin
        .apply(AdminUpdate::State(state(AdminRole::Admin, false)))
        .unwrap();
    ui.core.push(ScreenId::Admin);
    ui.update(0);
    click(&mut ui, ScreenId::Admin, "NativeEnvironment");
    assert_eq!(ui.top_id(), ScreenId::Admin);
    let mut snapshot = state(AdminRole::Admin, false);
    snapshot.supported.insert(AdminFeature::Environment);
    snapshot.revision = 2;
    ui.apply(UiUpdate::Admin(AdminUpdate::State(snapshot)));
    ui.update(16);
    ui.drain_actions();
    ui.core.admin.pending.clear();
    click(&mut ui, ScreenId::Admin, "NativeEnvironment");
    assert_eq!(ui.top_id(), ScreenId::AdminEnvironment);
    // Nothing changed yet: nothing to apply.
    click(&mut ui, ScreenId::AdminEnvironment, "EnvApply");
    assert!(ui.core.admin.pending.is_empty());
    // Advanced: the sun azimuth slider's middle is 180 degrees.
    click(&mut ui, ScreenId::AdminEnvironment, "EnvTabAdvanced");
    click(&mut ui, ScreenId::AdminEnvironment, "EnvA_SunAzimuth");
    let azimuth = ui.core.environment.draft.as_ref().unwrap().sun_azimuth.unwrap();
    assert!((azimuth - 180.0).abs() <= 3.0, "{azimuth}");
    // The direct light's picker: Done puts its colour in the draft.
    click(&mut ui, ScreenId::AdminEnvironment, "Env_DirectLight");
    assert_eq!(ui.top_id(), ScreenId::AdminColorPicker);
    click(&mut ui, ScreenId::AdminColorPicker, "EnvPick0");
    click(&mut ui, ScreenId::AdminColorPicker, "EnvPickDone");
    assert_eq!(ui.top_id(), ScreenId::AdminEnvironment);
    let sun = ui.core.environment.draft.as_ref().unwrap().direct_light.unwrap();
    assert!((sun[0] - 0.5).abs() < 0.03 && sun[1] == 0.6, "{sun:?}");
    click(&mut ui, ScreenId::AdminEnvironment, "EnvApply");
    let sent = ui.core.admin.pending.values().find_map(|a| match a {
        AdminAction::SetEnvironment { settings } => Some(settings.clone()),
        _ => None,
    });
    let sent = sent.expect("Apply asks the host");
    assert_eq!(sent.sun_azimuth, Some(azimuth));
    assert_eq!(sent.direct_light, Some(sun));
    // Losing the rank closes it.
    let mut snapshot = state(AdminRole::Player, false);
    snapshot.revision = 3;
    ui.apply(UiUpdate::Admin(AdminUpdate::State(snapshot)));
    ui.update(16);
    assert!(!ui.stack().contains(&ScreenId::AdminEnvironment));
}
