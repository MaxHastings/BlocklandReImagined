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
        max_bots: 16,
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
        addon_settings: Default::default(),
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

/// Opens every admin dialog over a populated admin model and renders it
/// offscreen; screenshots go to `out` when given.
#[cfg(feature = "gpu")]
fn admin_screens_render_offscreen(
    pack: std::rc::Rc<bri_ui::pack::Pack>,
    out: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    use bri_ui::{
        api::Settings,
        binds::Platform,
        gpu::{Headless, UiRenderer},
        models::admin::{AdminBrickGroup, AdminConfirmation, AdminMap},
        screens::ScreenId,
        ui::{Ui, UiConfig},
    };

    let gpu = Headless::new()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);

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
        assert!(!draw.cmds.is_empty(), "{name} draws nothing");
        assert_eq!(pixels.len(), 1024 * 768 * 4);
        if let Some(out) = out {
            image::save_buffer(
                out.join(format!("{name}.png")),
                &pixels,
                1024,
                768,
                image::ColorType::Rgba8,
            )?;
        }
    }
    Ok(())
}

#[cfg(feature = "gpu")]
#[test]
fn admin_screens_render_offscreen_synthetic() -> anyhow::Result<()> {
    let mut data = bri_ui::schema::UiPack::default();
    bri_ui::testing::add_dialogs(&mut data);
    admin_screens_render_offscreen(bri_ui::testing::pack(data), None)
}

/// Explicit offscreen source-skin inspection: writes the dialogs to
/// `artifacts/native-admin-ui`.
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires generated v20 content"]
fn source_admin_screens_render_offscreen() -> anyhow::Result<()> {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pack = std::rc::Rc::new(bri_ui::pack::Pack::load(&bri_package::testing::pack_dir(
        &workspace.join("content"),
        "ui_pack",
    ))?);
    let out = workspace.join("artifacts/native-admin-ui");
    std::fs::create_dir_all(&out)?;
    admin_screens_render_offscreen(pack, Some(&out))
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
    let mut wand = ctrl(
        "GuiButtonCtrl",
        "GuiButtonProfile",
        Rect::new(100, 100, 140, 30),
    );
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
        ui.core
            .admin
            .confirmation
            .as_ref()
            .map(|c| c.action.clone()),
        Some(AdminAction::SetRole {
            target: 7,
            role: AdminRole::SuperAdmin
        })
    );
    assert!(
        ui.core.admin.pending.is_empty(),
        "nothing is sent unconfirmed"
    );
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
    for name in [
        "MainMenuGui",
        "PlayGui",
        "LoadingGui",
        "escapeMenu",
        "adminGui",
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
    // The day/night cycle's check box turns it on (and back off), and
    // brings its sliders to life.
    let cycle = |ui: &Ui| ui.core.environment.draft.as_ref().unwrap().day_cycle;
    click(&mut ui, ScreenId::AdminEnvironment, "EnvDayCycleSimple");
    assert!(
        cycle(&ui).is_some(),
        "the check box did not turn the cycle on"
    );
    click(&mut ui, ScreenId::AdminEnvironment, "EnvS_TimeOfDay");
    let at = cycle(&ui).unwrap().time;
    assert!((at - 0.5).abs() <= 0.02, "{at}");
    click(&mut ui, ScreenId::AdminEnvironment, "EnvDayCycleSimple");
    assert_eq!(cycle(&ui), None);
    click(&mut ui, ScreenId::AdminEnvironment, "EnvDayCycleSimple");
    click(&mut ui, ScreenId::AdminEnvironment, "EnvApply");
    let sent = ui.core.admin.pending.values().find_map(|a| match a {
        AdminAction::SetEnvironment { settings } => settings.day_cycle,
        _ => None,
    });
    assert_eq!(
        sent.map(|d| d.length_seconds),
        Some(300.0),
        "Apply sends the cycle"
    );
    ui.core.admin.pending.clear();
    // The same on the Advanced tab, and Vignette Multiply beside it.
    click(&mut ui, ScreenId::AdminEnvironment, "EnvTabAdvanced");
    click(&mut ui, ScreenId::AdminEnvironment, "EnvDayCycleAdvanced");
    assert_eq!(cycle(&ui), None);
    click(&mut ui, ScreenId::AdminEnvironment, "EnvDayCycleAdvanced");
    assert!(cycle(&ui).is_some());
    // Vignette Multiply is the last row, below the page's fold: scroll to it.
    let (x, y) = ui
        .control_center(ScreenId::AdminEnvironment, "EnvAdvancedPage")
        .unwrap();
    ui.handle_input(InputEvent::MouseMove { x, y });
    ui.handle_input(InputEvent::Wheel { delta: -10.0 });
    ui.update(16);
    let (_, y) = ui
        .control_center(ScreenId::AdminEnvironment, "EnvVignetteMultiply")
        .unwrap();
    assert!(
        y < 386.0,
        "Vignette Multiply is still below the fold at {y}"
    );
    click(&mut ui, ScreenId::AdminEnvironment, "EnvVignetteMultiply");
    assert!(ui.core.environment.vignette_multiply());
    ui.core.environment.reset();
    ui.handle_input(InputEvent::Wheel { delta: 10.0 });
    ui.update(16);
    click(&mut ui, ScreenId::AdminEnvironment, "EnvTabSimple");
    // Advanced: the sun azimuth slider's middle is 180 degrees.
    click(&mut ui, ScreenId::AdminEnvironment, "EnvTabAdvanced");
    click(&mut ui, ScreenId::AdminEnvironment, "EnvA_SunAzimuth");
    let azimuth = ui
        .core
        .environment
        .draft
        .as_ref()
        .unwrap()
        .sun_azimuth
        .unwrap();
    assert!((azimuth - 180.0).abs() <= 3.0, "{azimuth}");
    // The direct light's picker: Done puts its colour in the draft.
    click(&mut ui, ScreenId::AdminEnvironment, "Env_DirectLight");
    assert_eq!(ui.top_id(), ScreenId::AdminColorPicker);
    click(&mut ui, ScreenId::AdminColorPicker, "EnvPick0");
    click(&mut ui, ScreenId::AdminColorPicker, "EnvPickDone");
    assert_eq!(ui.top_id(), ScreenId::AdminEnvironment);
    let sun = ui
        .core
        .environment
        .draft
        .as_ref()
        .unwrap()
        .direct_light
        .unwrap();
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

/// The Environment window applies a change by itself once the rows have
/// been still for a moment: a slider's steps become one request, a change
/// made while one is in flight goes once it is answered, and a rejected
/// change is not sent again until the rows change.
#[test]
fn environment_changes_go_to_the_host_live_once_they_settle() {
    use bri_content::atmosphere::{Authored, Settings, light_direction};
    use bri_ui::input::{InputEvent, MouseButton};
    use bri_ui::{
        api::{ConnectionState, Settings as UiSettings, UiUpdate},
        binds::Platform,
        geom::Rect,
        models::environment::{EnvironmentView, NumberField},
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
    let view = |settings: Settings| EnvironmentView {
        authored: Authored {
            sun_direction: light_direction(90.0, 45.0),
            direct_light: [0.6; 3],
            ambient_light: [0.3; 3],
            fog_start: 0.0,
            fog_end: 0.0,
            fog_color: [0.5; 3],
        },
        settings,
        tick: 0,
    };
    ui.apply(UiUpdate::Environment(view(Settings::default())));
    let mut snapshot = state(AdminRole::Admin, false);
    snapshot.supported.insert(AdminFeature::Environment);
    ui.apply(UiUpdate::Admin(AdminUpdate::State(snapshot)));
    ui.core.push(ScreenId::AdminEnvironment);
    ui.update(0);
    assert_eq!(ui.top_id(), ScreenId::AdminEnvironment);
    ui.drain_actions();
    let sent = |ui: &Ui| -> Vec<Settings> {
        ui.core
            .admin
            .pending
            .values()
            .filter_map(|a| match a {
                AdminAction::SetEnvironment { settings } => Some((**settings).clone()),
                _ => None,
            })
            .collect()
    };
    let request = |ui: &Ui| *ui.core.admin.pending.keys().next().unwrap();
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
    // Opening sends nothing, however long the window sits.
    ui.update(1000);
    assert!(sent(&ui).is_empty());
    // A slider's steps: nothing until the rows settle, then one request
    // with the last value.
    for azimuth in [100.0, 150.0, 200.0] {
        ui.core
            .environment
            .set_number(NumberField::SunAzimuth, azimuth);
        ui.update(100);
        assert!(sent(&ui).is_empty(), "sent while still moving at {azimuth}");
    }
    ui.update(200);
    let first = sent(&ui);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].sun_azimuth, Some(200.0));
    assert_eq!(ui.core.admin.status, "Waiting for host...");
    // A change while that is in flight waits for the answer.
    ui.core
        .environment
        .set_number(NumberField::SunElevation, 30.0);
    ui.update(1000);
    assert_eq!(sent(&ui).len(), 1, "a second request went out unanswered");
    let id = request(&ui);
    ui.apply(UiUpdate::ActionResult { id, result: Ok(()) });
    ui.apply(UiUpdate::Environment(view(first[0].clone())));
    // (The host's answer is a change to the rows too, so the delay runs
    // again from it.)
    ui.update(300);
    let second = sent(&ui);
    assert_eq!(second.len(), 1, "the waiting change goes once answered");
    assert_eq!(second[0].sun_elevation, Some(30.0));
    assert_eq!(second[0].sun_azimuth, Some(200.0));
    // The host rejects it: no retry until the rows change again.
    let id = request(&ui);
    ui.apply(UiUpdate::ActionResult {
        id,
        result: Err("no".into()),
    });
    ui.update(2000);
    assert!(sent(&ui).is_empty(), "a rejected change was sent again");
    assert!(ui.core.admin.status.starts_with("Rejected"));
    assert!(ui.core.environment.changed());
    ui.core
        .environment
        .set_number(NumberField::SunElevation, 40.0);
    ui.update(300);
    assert_eq!(sent(&ui).len(), 1);
    assert_eq!(sent(&ui)[0].sun_elevation, Some(40.0));
    // A colour picker goes live too: moving its sliders sends the colour
    // once they settle, with the picker still open.
    let id = request(&ui);
    ui.apply(UiUpdate::ActionResult { id, result: Ok(()) });
    ui.apply(UiUpdate::Environment(view(ui.core.environment.settings())));
    ui.update(300);
    assert!(sent(&ui).is_empty());
    click(&mut ui, ScreenId::AdminEnvironment, "EnvTabAdvanced");
    click(&mut ui, ScreenId::AdminEnvironment, "Env_ShadowColor");
    assert_eq!(ui.top_id(), ScreenId::AdminColorPicker);
    click(&mut ui, ScreenId::AdminColorPicker, "EnvPick0");
    ui.update(300);
    let live = sent(&ui);
    assert_eq!(live.len(), 1, "the picker's colour went live");
    let picked = live[0].shadow_color.expect("shadow colour sent");
    assert_eq!(ui.top_id(), ScreenId::AdminColorPicker);
    // Cancel sends the colour from before back.
    let id = request(&ui);
    ui.apply(UiUpdate::ActionResult { id, result: Ok(()) });
    ui.apply(UiUpdate::Environment(view(live[0].clone())));
    click(&mut ui, ScreenId::AdminColorPicker, "EnvPickCancel");
    assert_eq!(ui.top_id(), ScreenId::AdminEnvironment);
    ui.update(300);
    let back = sent(&ui);
    assert_eq!(back.len(), 1, "Cancel reverts the host");
    assert_ne!(back[0].shadow_color, Some(picked));
    assert_eq!(back[0].shadow_color, None);
}

/// Live apply with a day cycle on: the settled change carries the cycle,
/// the host's answer (its own anchor tick) shows as applied and is not
/// sent again, a favourite of it loads anchored at the host's tick and
/// goes live, and a later change keeps the running cycle rather than
/// restarting it.
#[test]
fn environment_day_cycle_goes_live_and_keeps_turning_through_a_favorite() {
    use bri_content::atmosphere::{Authored, DayCycle, Settings, light_direction};
    use bri_ui::{
        api::{ConnectionState, Settings as UiSettings, UiUpdate},
        binds::Platform,
        geom::Rect,
        input::{InputEvent, MouseButton},
        models::environment::{EnvironmentView, NumberField},
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
    let view = |settings: Settings, tick: u64| EnvironmentView {
        authored: Authored {
            sun_direction: light_direction(90.0, 45.0),
            direct_light: [0.6; 3],
            ambient_light: [0.3; 3],
            fog_start: 0.0,
            fog_end: 0.0,
            fog_color: [0.5; 3],
        },
        settings,
        tick,
    };
    ui.apply(UiUpdate::Environment(view(Settings::default(), 1000)));
    let mut snapshot = state(AdminRole::Admin, false);
    snapshot.supported.insert(AdminFeature::Environment);
    ui.apply(UiUpdate::Admin(AdminUpdate::State(snapshot)));
    ui.core.push(ScreenId::AdminEnvironment);
    ui.update(0);
    ui.drain_actions();
    let click = |ui: &mut Ui, key: &str| {
        let (x, y) = ui.control_center(ScreenId::AdminEnvironment, key).unwrap();
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
    let sent = |ui: &Ui| -> Vec<Settings> {
        ui.core
            .admin
            .pending
            .values()
            .filter_map(|a| match a {
                AdminAction::SetEnvironment { settings } => Some((**settings).clone()),
                _ => None,
            })
            .collect()
    };
    // The host answers a request: accepted, and its view of the settings
    // with the cycle stamped at its own tick.
    let answer = |ui: &mut Ui, tick: u64| -> Settings {
        let id = *ui.core.admin.pending.keys().next().unwrap();
        let mut applied = sent(ui).remove(0);
        if let Some(d) = applied.day_cycle.as_mut() {
            d.anchor_tick = tick;
        }
        ui.apply(UiUpdate::ActionResult { id, result: Ok(()) });
        ui.apply(UiUpdate::Environment(view(applied.clone(), tick)));
        ui.update(300);
        applied
    };
    // Cycle on, 18:00, a ten minute day: one request once it settles.
    click(&mut ui, "EnvDayCycleSimple");
    ui.core.environment.set_number(NumberField::TimeOfDay, 18.0);
    ui.core
        .environment
        .set_number(NumberField::DayLength, 600.0);
    ui.update(300);
    let first = sent(&ui);
    assert_eq!(first.len(), 1);
    assert_eq!(
        first[0].day_cycle,
        Some(DayCycle {
            length_seconds: 600.0,
            time: 0.75,
            anchor_tick: 1000,
        })
    );
    let applied = answer(&mut ui, 1300);
    assert!(sent(&ui).is_empty(), "the host's answer was sent back");
    assert!(
        !ui.core.environment.changed(),
        "the window still shows the cycle as not applied"
    );
    // Store it, Reset (which goes live too), then Load: the favourite's
    // cycle is anchored at the host's tick with the saved time of day.
    click(&mut ui, "EnvFavSave");
    let saved = ui.core.settings.environment_favorites[&0].clone();
    assert_eq!(saved.day_cycle.map(|d| d.anchor_tick), Some(0));
    click(&mut ui, "EnvReset");
    ui.update(300);
    assert_eq!(sent(&ui), vec![Settings::default()]);
    answer(&mut ui, 1600);
    click(&mut ui, "EnvFavLoad");
    ui.update(300);
    let loaded = sent(&ui);
    assert_eq!(loaded.len(), 1);
    let cycle = loaded[0].day_cycle.unwrap();
    assert_eq!(cycle.anchor_tick, 1600);
    assert_eq!(cycle.length_seconds, 600.0);
    assert!((cycle.time - applied.day_cycle.unwrap().time).abs() < 0.01);
    let running = answer(&mut ui, 1900);
    assert!(!ui.core.environment.changed());
    // A later sun change rides along without restarting the cycle.
    ui.core
        .environment
        .set_number(NumberField::SunAzimuth, 90.0);
    ui.update(300);
    let next = sent(&ui);
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].sun_azimuth, Some(90.0));
    assert_eq!(next[0].day_cycle, running.day_cycle, "the cycle restarted");
}

/// The Environment window's favourites: Store keeps the rows in a slot
/// (saved with the player's settings), Load fills the rows from one, and
/// nothing reaches the host until Apply.
#[test]
fn environment_favorites_store_and_load_the_rows_without_applying() {
    use bri_content::atmosphere::{Authored, Settings, light_direction};
    use bri_ui::{
        api::{ConnectionState, Settings as UiSettings, UiAction, UiUpdate},
        binds::Platform,
        geom::Rect,
        input::{InputEvent, MouseButton},
        models::environment::{EnvironmentView, NumberField},
        pack::Pack,
        schema::UiPack,
        screens::{ScreenId, ctrl},
        ui::{Ui, UiConfig},
        view::EventKind,
    };
    use std::{path::PathBuf, rc::Rc};
    let mut pack = UiPack::default();
    for name in [
        "MainMenuGui",
        "PlayGui",
        "LoadingGui",
        "escapeMenu",
        "adminGui",
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
        tick: 2400,
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
    let mut snapshot = state(AdminRole::Admin, false);
    snapshot.supported.insert(AdminFeature::Environment);
    ui.apply(UiUpdate::Admin(AdminUpdate::State(snapshot)));
    ui.core.push(ScreenId::Admin);
    ui.update(0);
    click(&mut ui, ScreenId::Admin, "NativeEnvironment");
    assert_eq!(ui.top_id(), ScreenId::AdminEnvironment);
    ui.drain_actions();
    fn env(ui: &mut Ui) -> &mut bri_ui::view::View {
        let i = ui
            .dialogs
            .iter()
            .rposition(|s| s.id() == ScreenId::AdminEnvironment)
            .unwrap();
        ui.dialogs[i].view_mut()
    }
    // Every slot starts empty.
    let favs = env(&mut ui).id("EnvFavs").unwrap();
    assert_eq!(env(&mut ui).state(favs).items.len(), 10);
    assert_eq!(env(&mut ui).state(favs).items[2].0, "Slot 3 (empty)");
    // Loading an empty slot changes nothing and says so.
    click(&mut ui, ScreenId::AdminEnvironment, "EnvFavLoad");
    assert!(!ui.core.environment.changed());
    assert_eq!(ui.core.admin.status, "Slot 1 is empty.");
    // Set a look up: a day cycle at 18:00 and a sun azimuth.
    click(&mut ui, ScreenId::AdminEnvironment, "EnvDayCycleSimple");
    ui.core.environment.set_number(NumberField::TimeOfDay, 18.0);
    ui.core
        .environment
        .set_number(NumberField::SunAzimuth, 200.0);
    ui.update(16);
    // Store it in slot 3, picked from the list.
    env(&mut ui).select(favs, Some(2));
    {
        let node = favs;
        let ev = bri_ui::view::ViewEvent {
            node,
            kind: EventKind::Changed,
        };
        let i = ui
            .dialogs
            .iter()
            .rposition(|s| s.id() == ScreenId::AdminEnvironment)
            .unwrap();
        let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
        dialogs[i].on_event(&ev, core);
    }
    click(&mut ui, ScreenId::AdminEnvironment, "EnvFavSave");
    let saved = ui.core.settings.environment_favorites[&2].clone();
    assert_eq!(saved.sun_azimuth, Some(200.0));
    let cycle = saved.day_cycle.expect("the cycle is kept");
    assert_eq!(cycle.anchor_tick, 0, "a favourite has no server tick");
    assert!((cycle.time - 0.75).abs() < 0.001, "{}", cycle.time);
    assert_eq!(env(&mut ui).state(favs).items[2].0, "Slot 3");
    assert_eq!(ui.core.admin.status, "Saved in slot 3.");
    assert!(
        ui.drain_actions()
            .iter()
            .any(|(_, a)| matches!(a, UiAction::SaveSettings(s) if s.environment_favorites.contains_key(&2))),
        "Store saves the player's settings"
    );
    // Nothing went to the host.
    assert!(ui.core.admin.pending.is_empty());
    // Reset the rows, then Load brings the look back, anchored at the
    // server's tick, still not applied.
    click(&mut ui, ScreenId::AdminEnvironment, "EnvReset");
    assert_eq!(ui.core.environment.settings().sun_azimuth, None);
    click(&mut ui, ScreenId::AdminEnvironment, "EnvFavLoad");
    let draft = ui.core.environment.settings();
    assert_eq!(draft.sun_azimuth, Some(200.0));
    assert_eq!(draft.day_cycle.unwrap().anchor_tick, 2400);
    assert_eq!(draft.day_cycle.unwrap().time, cycle.time);
    assert!(ui.core.environment.changed());
    assert_eq!(ui.core.admin.status, "Loaded slot 3.");
    assert!(ui.core.admin.pending.is_empty());
    click(&mut ui, ScreenId::AdminEnvironment, "EnvApply");
    let sent = ui.core.admin.pending.values().find_map(|a| match a {
        AdminAction::SetEnvironment { settings } => Some(settings.clone()),
        _ => None,
    });
    assert_eq!(sent.map(|s| s.sun_azimuth), Some(Some(200.0)));
    // The favourites come back with the settings on the next run.
    let json = serde_json::to_string(&ui.core.settings).unwrap();
    let back: UiSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(back.environment_favorites[&2], saved);
}

/// Change Map shows the picked map's own picture over changeMapGui's
/// "UNKNOWN MAP" placeholder, and the placeholder again for a map without one.
fn change_map_shows_the_picked_maps_picture(pack: std::rc::Rc<bri_ui::pack::Pack>) {
    use bri_ui::{
        api::{ConnectionState, IconRef, MapInfo, Settings, UiUpdate},
        binds::Platform,
        input::{InputEvent, MouseButton},
        models::admin::AdminMap,
        screens::ScreenId,
        ui::{Ui, UiConfig},
    };
    let mut ui = Ui::new(
        pack,
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
    ui.core
        .admin
        .apply(AdminUpdate::State(state(AdminRole::SuperAdmin, false)))
        .unwrap();
    // The host lists maps by id; this client's content has their pictures.
    ui.core.maps = vec![
        MapInfo {
            id: "v20/kitchen".into(),
            name: "Kitchen".into(),
            description: String::new(),
            preview: IconRef::Pack("fixture/maps/kitchen".into()),
        },
        MapInfo {
            id: "v20/bare".into(),
            name: "Bare".into(),
            description: String::new(),
            preview: IconRef::None,
        },
    ];
    ui.core.admin.maps = ["Kitchen", "Bare"]
        .map(|name| AdminMap {
            id: format!("v20/{}", name.to_lowercase()),
            name: name.into(),
        })
        .to_vec();
    ui.core.push(ScreenId::AdminMaps);
    ui.update(0);
    let placeholder = |ui: &Ui| {
        let v = ui.screen(ScreenId::AdminMaps).expect("Change Map").view();
        let n = v
            .walk()
            .find(|&n| {
                v.node(n).ctrl.class == "GuiBitmapCtrl"
                    && v.node(n).ctrl.bitmap.as_deref() == Some("base/data/missions/default")
            })
            .expect("changeMapGui's map picture");
        v.node(n).state.bitmap.clone()
    };
    // Click a row of the map list, as Max does.
    let pick = |ui: &mut Ui, row: i32| {
        let v = ui.screen(ScreenId::AdminMaps).unwrap().view();
        let list = v.id("changeMapList").expect("map list");
        let (r, h) = (v.node(list).rect, v.node(list).state.row_height.max(1));
        let (x, y) = ((r.x + 4) as f32, (r.y + h * row + h / 2) as f32);
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
    assert_eq!(placeholder(&ui), None, "nothing picked: UNKNOWN MAP");
    pick(&mut ui, 0);
    assert_eq!(ui.core.admin.selected_map.as_deref(), Some("v20/kitchen"));
    assert_eq!(
        placeholder(&ui).as_deref(),
        Some("fixture/maps/kitchen"),
        "Kitchen's own picture"
    );
    pick(&mut ui, 1);
    assert_eq!(ui.core.admin.selected_map.as_deref(), Some("v20/bare"));
    assert_eq!(placeholder(&ui), None, "no picture: UNKNOWN MAP again");
}

#[test]
fn change_map_shows_the_picked_maps_picture_synthetic() {
    change_map_shows_the_picked_maps_picture(bri_ui::testing::screens_pack());
}

#[test]
#[ignore = "requires generated v20 content"]
fn change_map_shows_the_picked_maps_picture_content() {
    change_map_shows_the_picked_maps_picture(bri_ui::testing::content_pack(
        &bri_package::testing::pack_dir(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
            "ui_pack",
        ),
    ));
}

/// The Simple tab's Sky button shows the picked picture and opens a grid of
/// the map's skies, the map's own first; a pick goes into the rows and on to
/// the host like any change.
#[test]
fn the_sky_picker_picks_from_pictures() {
    use bri_content::atmosphere::{Authored, Settings, light_direction};
    use bri_ui::{
        api::{ConnectionState, IconRef, Settings as UiSettings, UiUpdate},
        binds::Platform,
        geom::Rect,
        input::{InputEvent, MouseButton},
        models::environment::{Choice, ChoiceKind, Choices, EnvironmentView},
        pack::Pack,
        schema::UiPack,
        screens::{ScreenId, ctrl, find},
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
    let choice = |id: &str, label: &str, icon| Choice {
        id: id.into(),
        label: label.into(),
        icon: IconRef::External(icon),
        look: Settings::default(),
    };
    let choices = Choices {
        skies: vec![
            choice("base/data/skies/sky_blue2/resource.dml", "Blue2", 101),
            choice("plain", "Plain", 102),
        ],
        grounds: vec![choice("add-ons/ground_tt/grass.ground", "Grass", 103)],
        waters: vec![
            choice("none", "None", 0),
            Choice {
                look: Settings {
                    water_height: Some(5.0),
                    ..Default::default()
                },
                ..choice("add-ons/water_lava/lava.water", "Lava", 104)
            },
        ],
        map_sky: Some("base/data/skies/sky_blue2/resource.dml".into()),
        flat: false,
    };
    ui.core.environment.set_choices(choices);
    let click = |ui: &mut Ui, screen: ScreenId, key: &str| {
        let (x, y) = ui
            .control_center(screen, key)
            .unwrap_or_else(|| panic!("{key} is not on {screen:?}"));
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
    // (picture, name, can be clicked) of a picker button.
    let shown = |ui: &Ui, screen: ScreenId, name: &str| {
        let view = ui.screen(screen).unwrap().view();
        let button = find(view, name).unwrap();
        let icon = find(view, &format!("{name}_Icon")).unwrap();
        let label = find(view, &format!("{name}_Name")).unwrap();
        (
            view.node(icon).state.external_texture,
            view.text_of(label),
            view.node(button).state.active,
        )
    };
    let mut snapshot = state(AdminRole::Admin, false);
    snapshot.supported.insert(AdminFeature::Environment);
    ui.core.admin.apply(AdminUpdate::State(snapshot)).unwrap();
    ui.core.push(ScreenId::AdminEnvironment);
    ui.update(16);
    let env = ScreenId::AdminEnvironment;
    let picker = ScreenId::AdminEnvironmentChoice;
    // The map's own sky's picture.
    assert_eq!(
        shown(&ui, env, "EnvSky"),
        (Some(101), "Map's own".into(), true)
    );
    // A ground or water needs a flat map.
    for name in ["EnvGround", "EnvWater"] {
        assert_eq!(
            shown(&ui, env, name),
            (None, "Flat maps only".into(), false)
        );
    }
    // The sky picker: the map's own first (drawn now, so not pickable).
    click(&mut ui, env, "EnvSky");
    assert_eq!(ui.top_id(), picker);
    assert_eq!(
        shown(&ui, picker, "EnvChoice0"),
        (Some(101), "Map's own".into(), false)
    );
    assert_eq!(
        shown(&ui, picker, "EnvChoice2"),
        (Some(102), "Plain".into(), true)
    );
    click(&mut ui, picker, "EnvChoice2");
    assert_eq!(ui.top_id(), env);
    assert_eq!(ui.core.environment.choice_label(ChoiceKind::Sky), "Plain");
    assert_eq!(shown(&ui, env, "EnvSky"), (Some(102), "Plain".into(), true));
    // The change goes to the host.
    ui.core.admin.pending.clear();
    click(&mut ui, env, "EnvApply");
    let sent = ui
        .core
        .admin
        .pending
        .values()
        .find_map(|a| match a {
            AdminAction::SetEnvironment { settings } => Some(settings.clone()),
            _ => None,
        })
        .expect("Apply asks the host");
    assert_eq!(sent.sky.as_deref(), Some("plain"));
    // Escape closes the picker and leaves the rows as they are.
    let before = ui.core.environment.choice(ChoiceKind::Sky);
    click(&mut ui, env, "EnvSky");
    ui.handle_input(InputEvent::KeyDown {
        key: bri_ui::input::Key::Escape,
        mods: Default::default(),
        repeat: false,
    });
    ui.update(16);
    assert_eq!(ui.top_id(), env);
    assert_eq!(ui.core.environment.choice(ChoiceKind::Sky), before);
    // On a flat map: the water picker, Map's own then None then Lava.
    let mut flat = ui.core.environment.choices.clone();
    flat.flat = true;
    ui.core.environment.set_choices(flat);
    ui.update(16);
    assert_eq!(
        shown(&ui, env, "EnvWater"),
        (None, "Map's own".into(), true)
    );
    click(&mut ui, env, "EnvWater");
    assert_eq!(ui.top_id(), picker);
    assert_eq!(shown(&ui, picker, "EnvChoice1").1, "None");
    click(&mut ui, picker, "EnvChoice2");
    assert_eq!(
        shown(&ui, env, "EnvWater"),
        (Some(104), "Lava".into(), true)
    );
    // Its Advanced row reads the lava's own height.
    click(&mut ui, env, "EnvTabAdvanced");
    let view = ui.screen(env).unwrap().view();
    let height = find(view, "EnvA_WaterHeight_Value").unwrap();
    assert_eq!(view.text_of(height), "5.0");
    let ground = find(view, "Env_GroundColor").unwrap();
    assert!(!view.node(ground).state.active, "no ground picked");
}
