use bri_ui::{
    api::*,
    binds::Platform,
    pack::Pack,
    schema::{Control, UiPack},
    screens::ScreenId,
    ui::{Pending, Ui, UiConfig},
    view::{EventKind, ViewEvent},
};
use std::{path::PathBuf, rc::Rc};

fn ctl(class: &str, name: Option<&str>, command: Option<&str>, variable: Option<&str>) -> Control {
    Control {
        class: class.into(),
        name: name.map(str::to_owned),
        command: command.map(str::to_owned),
        variable: variable.map(str::to_owned),
        style: "GuiDefaultProfile".into(),
        position: [8, 8],
        extent: [120, 24],
        visible: true,
        ..Default::default()
    }
}
fn layout(children: Vec<Control>) -> Control {
    Control {
        class: "GuiControl".into(),
        name: Some("root".into()),
        extent: [640, 480],
        visible: true,
        children,
        ..Default::default()
    }
}
fn test_ui() -> Ui {
    let mut pack = UiPack::default();
    pack.layouts.insert(
        "joinMiniGameGui".into(),
        layout(vec![
            ctl(
                "GuiTextListCtrl",
                Some("JMG_List"),
                Some("JoinMiniGameGui.clickList();"),
                None,
            ),
            ctl(
                "GuiBitmapButtonCtrl",
                None,
                Some("JoinMiniGameGui.clickJoin();"),
                None,
            ),
            ctl(
                "GuiBitmapButtonCtrl",
                None,
                Some("JoinMiniGameGui.clickLeave();"),
                None,
            ),
            ctl(
                "GuiBitmapButtonCtrl",
                None,
                Some("JoinMiniGameGui.clickCreate();"),
                None,
            ),
        ]),
    );
    pack.layouts.insert(
        "CreateMiniGameGui".into(),
        layout(vec![
            ctl("GuiWindowCtrl", Some("CMG_Window"), None, None),
            ctl(
                "GuiBitmapButtonCtrl",
                Some("CMG_CreateButton"),
                Some("CreateMiniGameGui.clickCreate();"),
                None,
            ),
            ctl(
                "GuiBitmapButtonCtrl",
                None,
                Some("CreateMiniGameGui.clickReset();"),
                None,
            ),
            ctl(
                "GuiBitmapButtonCtrl",
                None,
                Some("CreateMiniGameGui.clickEnd();"),
                None,
            ),
            ctl(
                "GuiPopUpMenuCtrl",
                Some("CMG_ColorList"),
                Some("CreateMiniGameGui.clickColorList();"),
                None,
            ),
            ctl("GuiPopUpMenuCtrl", Some("CMG_PlayerDataBlock"), None, None),
            ctl("GuiPopUpMenuCtrl", Some("CMG_StartEquip0"), None, None),
            ctl("GuiPopUpMenuCtrl", Some("CMG_StartEquip1"), None, None),
            ctl("GuiPopUpMenuCtrl", Some("CMG_StartEquip2"), None, None),
            ctl("GuiPopUpMenuCtrl", Some("CMG_StartEquip3"), None, None),
            ctl("GuiPopUpMenuCtrl", Some("CMG_StartEquip4"), None, None),
            ctl("GuiSwatchCtrl", Some("CMG_Swatch"), None, None),
            ctl("GuiTextEditCtrl", None, None, Some("$MiniGame::Title")),
            ctl(
                "GuiTextEditCtrl",
                None,
                None,
                Some("$MiniGame::Points::BreakBrick"),
            ),
            ctl(
                "GuiTextEditCtrl",
                None,
                None,
                Some("$MiniGame::Points::PlantBrick"),
            ),
            ctl(
                "GuiTextEditCtrl",
                None,
                None,
                Some("$MiniGame::Points::KillPlayer"),
            ),
            ctl(
                "GuiTextEditCtrl",
                None,
                None,
                Some("$MiniGame::Points::KillSelf"),
            ),
            ctl(
                "GuiTextEditCtrl",
                None,
                None,
                Some("$MiniGame::Points::Die"),
            ),
            ctl(
                "GuiTextEditCtrl",
                None,
                None,
                Some("$MiniGame::RespawnTime"),
            ),
            ctl(
                "GuiTextEditCtrl",
                None,
                None,
                Some("$MiniGame::VehicleRespawnTime"),
            ),
            ctl(
                "GuiTextEditCtrl",
                None,
                None,
                Some("$MiniGame::BrickRespawnTime"),
            ),
            ctl("GuiCheckBoxCtrl", None, None, Some("$MiniGame::InviteOnly")),
            bool_ctl("$MiniGame::PlayersUseOwnBricks", false),
            bool_ctl("$MiniGame::UseAllPlayersBricks", false),
            bool_ctl("$MiniGame::UseSpawnBricks", true),
            bool_ctl("$MiniGame::FallingDamage", true),
            bool_ctl("$MiniGame::WeaponDamage", true),
            bool_ctl("$MiniGame::SelfDamage", true),
            bool_ctl("$MiniGame::VehicleDamage", true),
            bool_ctl("$MiniGame::BrickDamage", true),
            bool_ctl("$MiniGame::EnableWand", false),
            bool_ctl("$MiniGame::EnableBuilding", true),
            bool_ctl("$MiniGame::EnablePainting", true),
            Control {
                visible: false,
                ..ctl("GuiSwatchCtrl", Some("CMG_FavsHelper"), None, None)
            },
            ctl(
                "GuiBitmapButtonCtrl",
                None,
                Some("CreateMiniGameGui.ClickSetFavs();"),
                None,
            ),
            ctl(
                "GuiBitmapButtonCtrl",
                Some("BSD_FavButton3"),
                Some("CreateMiniGameGui.clickFav(3);"),
                None,
            ),
        ]),
    );
    pack.layouts.insert(
        "MiniGameInviteGui".into(),
        layout(vec![
            ctl("GuiMLTextCtrl", Some("MGI_Title"), None, None),
            ctl("GuiMLTextCtrl", Some("MGI_Name"), None, None),
            ctl("GuiMLTextCtrl", Some("MGI_BL_ID"), None, None),
            ctl(
                "GuiBitmapButtonCtrl",
                None,
                Some("MiniGameInviteGui.clickAccept();"),
                None,
            ),
            ctl(
                "GuiBitmapButtonCtrl",
                None,
                Some("MiniGameInviteGui.clickReject();"),
                None,
            ),
            ctl(
                "GuiBitmapButtonCtrl",
                None,
                Some("MiniGameInviteGui.clickIgnore();"),
                None,
            ),
        ]),
    );
    Ui::new(
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
    )
}
fn bool_ctl(variable: &str, value: bool) -> Control {
    let mut c = ctl("GuiCheckBoxCtrl", None, None, Some(variable));
    c.fields
        .insert("value".into(), if value { "1" } else { "0" }.into());
    c
}
fn game_state() -> MiniGameUiState {
    let rules = MiniGameRules::default();
    MiniGameUiState {
        addon_locked: vec![],
        teams_shown_when: None,
        ready: true,
        revision: 1,
        capabilities: MiniGameCapabilities {
            list: true,
            create: true,
            configure: true,
            join: true,
            leave: true,
            invite: true,
            respond_invite: true,
            remove_member: true,
            reset: true,
            respawn_all: true,
            end: true,
            scoreboard: true,
        },
        games: vec![MiniGameSummary {
            default: false,
            paint_color: None,
            members: vec![],
            id: MiniGameId(42),
            title: "Alpha Round".into(),
            owner: MiniGamePlayerId(900),
            owner_name: "Owner".into(),
            color: 0,
            member_count: 2,
            invite_only: false,
            rules,
            teams: vec![],
            addon_settings: Default::default(),
        }],
        colors: vec![MiniGameColor {
            index: 0,
            name: "Red".into(),
            rgb: [255, 0, 0],
        }],
        active_game: None,
        owns_active_game: false,
        local_player: Some(MiniGamePlayerId(7)),
        members: vec![],
        invitations: vec![],
        player_types: vec![MiniGameChoice {
            id: "v20.player.playerstandardarmor".into(),
            name: "Standard Player".into(),
        }],
        items: vec![
            "hammeritem",
            "wrenchitem",
            "printgun",
            "gunitem",
            "rocketlauncheritem",
        ]
        .into_iter()
        .map(|s| MiniGameChoice {
            id: format!("v20.weapon.{s}"),
            name: s.into(),
        })
        .collect(),
        status: String::new(),
        addon_settings: vec![],
        addon_editable: vec![],
        palette: vec![],
    }
}
fn click(ui: &mut Ui, screen: ScreenId, command: &str) {
    let node = ui
        .screen(screen)
        .unwrap()
        .view()
        .by_command(command)
        .unwrap();
    let ev = ViewEvent {
        node,
        kind: EventKind::Click,
    };
    let i = ui.dialogs.iter().rposition(|s| s.id() == screen).unwrap();
    let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
    dialogs[i].on_event(&ev, core);
}

#[test]
fn public_join_uses_stable_game_selector_and_fails_closed_without_host_capability() {
    let mut ui = test_ui();
    ui.core.push(ScreenId::MiniGames);
    ui.update(0);
    let join = ui
        .screen(ScreenId::MiniGames)
        .unwrap()
        .view()
        .by_command("JoinMiniGameGui.clickJoin();")
        .unwrap();
    assert!(
        !ui.screen(ScreenId::MiniGames)
            .unwrap()
            .view()
            .node(join)
            .state
            .active
    );
    click(&mut ui, ScreenId::MiniGames, "JoinMiniGameGui.clickJoin();");
    assert!(ui.drain_actions().is_empty());
    ui.apply(UiUpdate::MiniGames(game_state()));
    ui.update(0);
    click(&mut ui, ScreenId::MiniGames, "JoinMiniGameGui.clickJoin();");
    let actions = ui.drain_actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].1,
        UiAction::JoinMiniGame {
            game: MiniGameId(42)
        }
    );
    assert_eq!(
        ui.core.pending.get(&actions[0].0),
        Some(&Pending::MiniGame(MiniGameOperation::Join))
    );
}

#[test]
fn stock_defaults_are_submitted_and_admin_does_not_unlock_owner_actions() {
    let mut ui = test_ui();
    let mut state = game_state();
    state.colors[0].index = 4;
    ui.core.minigames = state.clone();
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    click(
        &mut ui,
        ScreenId::MiniGameSettings,
        "CreateMiniGameGui.clickCreate();",
    );
    let (_, action) = ui.drain_actions().pop().unwrap();
    let UiAction::CreateMiniGame { color, rules } = action else {
        panic!("wrong action")
    };
    assert_eq!(color, 4);
    assert_eq!(rules, MiniGameRules::default());
    state.active_game = Some(MiniGameId(42));
    state.owns_active_game = false;
    state.capabilities.reset = true;
    state.capabilities.end = true;
    state.capabilities.configure = true;
    state.capabilities.create = true;
    state.games[0].id = MiniGameId(42);
    state.local_player = Some(MiniGamePlayerId(7));
    ui.core.minigames = state;
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    let view = ui.screen(ScreenId::MiniGameSettings).unwrap().view();
    assert!(
        !view
            .node(view.by_command("CreateMiniGameGui.clickReset();").unwrap())
            .state
            .active
    );
    assert!(
        !view
            .node(view.by_command("CreateMiniGameGui.clickEnd();").unwrap())
            .state
            .active
    );
}

#[test]
fn invitation_acceptance_keeps_stable_game_identity() {
    let mut ui = test_ui();
    let invite = MiniGameInvitation {
        game: MiniGameId(8),
        title: "Private Game".into(),
        owner: MiniGamePlayerId(90),
        owner_name: "Host".into(),
        owner_display_id: "LAN".into(),
    };
    ui.core.minigames = game_state();
    ui.core.minigames.invitations.push(invite.clone());
    ui.core.push(ScreenId::MiniGameInvitation);
    ui.update(0);
    click(
        &mut ui,
        ScreenId::MiniGameInvitation,
        "MiniGameInviteGui.clickAccept();",
    );
    assert_eq!(
        ui.drain_actions().pop().unwrap().1,
        UiAction::AcceptMiniGameInvite { game: invite.game }
    );
    assert_eq!(ui.core.minigames.invitations[0].owner_display_id, "LAN");
}

#[test]
fn editor_offers_reset_and_end_only_when_running_and_closes_after_acting() {
    let mut ui = test_ui();
    ui.apply(UiUpdate::MiniGames(game_state()));
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    let active = |ui: &Ui, command: &str| {
        let v = ui.screen(ScreenId::MiniGameSettings).unwrap().view();
        v.node(v.by_command(command).unwrap()).state.active
    };
    assert!(active(&ui, "CreateMiniGameGui.clickCreate();"));
    assert!(
        !active(&ui, "CreateMiniGameGui.clickReset();"),
        "nothing to reset yet"
    );
    assert!(
        !active(&ui, "CreateMiniGameGui.clickEnd();"),
        "nothing to end yet"
    );
    click(
        &mut ui,
        ScreenId::MiniGameSettings,
        "CreateMiniGameGui.clickCreate();",
    );
    let (id, _) = ui
        .drain_actions()
        .into_iter()
        .find(|(_, a)| matches!(a, UiAction::CreateMiniGame { .. }))
        .unwrap();
    ui.apply(UiUpdate::ActionResult { id, result: Ok(()) });
    assert!(
        !ui.is_open(ScreenId::MiniGameSettings),
        "creating closes the editor"
    );

    let mut running = game_state();
    running.active_game = Some(MiniGameId(42));
    running.owns_active_game = true;
    running.revision = 2;
    ui.apply(UiUpdate::MiniGames(running));
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    assert!(active(&ui, "CreateMiniGameGui.clickReset();"));
    assert!(active(&ui, "CreateMiniGameGui.clickEnd();"));
    click(
        &mut ui,
        ScreenId::MiniGameSettings,
        "CreateMiniGameGui.clickReset();",
    );
    let (id, _) = ui
        .drain_actions()
        .into_iter()
        .find(|(_, a)| matches!(a, UiAction::ResetMiniGame { .. }))
        .expect("v20 resets without a confirmation");
    ui.apply(UiUpdate::ActionResult { id, result: Ok(()) });
    assert!(
        !ui.is_open(ScreenId::MiniGameSettings),
        "resetting closes the editor"
    );
}

#[test]
fn set_favs_saves_the_form_to_a_slot_and_the_slot_fills_it_again() {
    let mut ui = test_ui();
    ui.apply(UiUpdate::MiniGames(game_state()));
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    let title = |ui: &mut Ui, text: Option<&str>| {
        let i = ui
            .dialogs
            .iter()
            .rposition(|s| s.id() == ScreenId::MiniGameSettings)
            .unwrap();
        let v = ui.dialogs[i].view_mut();
        let n = v
            .walk()
            .find(|&n| v.node(n).ctrl.variable.as_deref() == Some("$MiniGame::Title"))
            .unwrap();
        if let Some(t) = text {
            v.set_text(n, t);
        }
        v.edit_text(n)
    };
    title(&mut ui, Some("Rocket Arena"));
    click(
        &mut ui,
        ScreenId::MiniGameSettings,
        "CreateMiniGameGui.ClickSetFavs();",
    );
    click(
        &mut ui,
        ScreenId::MiniGameSettings,
        "CreateMiniGameGui.clickFav(3);",
    );
    assert_eq!(
        ui.core.settings.minigame_favorites[&3].rules.title,
        "Rocket Arena"
    );
    assert!(ui.drain_actions().iter().any(
        |(_, a)| matches!(a,UiAction::SaveSettings(s) if s.minigame_favorites.contains_key(&3))
    ));
    title(&mut ui, Some("Something Else"));
    click(
        &mut ui,
        ScreenId::MiniGameSettings,
        "CreateMiniGameGui.clickFav(3);",
    );
    assert_eq!(title(&mut ui, None), "Rocket Arena");
}

/// Slayer-like settings: a mode picker, lives, a CTF setting shown only in
/// Capture the Flag, and a team setting.
fn addon_state() -> MiniGameUiState {
    let mut state = game_state();
    state.active_game = Some(MiniGameId(42));
    state.addon_editable = vec![MiniGameId(42)];
    state.palette = vec![[255, 0, 0], [0, 0, 255], [0, 255, 0]];
    let text = |s: &str| MiniGameSettingValue::Text(s.into());
    let setting = |key: &str, title: &str, team: bool, kind, default| MiniGameAddOnSetting {
        key: key.into(),
        add_on: "Slayer".into(),
        category: "Victory Method".into(),
        title: title.into(),
        team,
        server: false,
        restart: false,
        kind,
        default,
        help: String::new(),
        avatar: None,
        shown_when: None,
    };
    state.addon_settings = vec![
        setting(
            "slayer:mode",
            "Game Mode",
            false,
            MiniGameSettingKind::List {
                items: vec![
                    (text("dm"), "Deathmatch".into()),
                    (text("ctf"), "Capture the Flag".into()),
                ],
            },
            text("dm"),
        ),
        setting(
            "slayer:lives",
            "Lives",
            false,
            MiniGameSettingKind::Int { min: 0, max: 99 },
            MiniGameSettingValue::Int(0),
        ),
        MiniGameAddOnSetting {
            shown_when: Some(MiniGameShownWhen {
                setting: "slayer:mode".into(),
                is: vec![text("ctf")],
                is_not: vec![],
            }),
            ..setting(
                "ctf:capturepoints",
                "Capture Points",
                false,
                MiniGameSettingKind::Int { min: 0, max: 99 },
                MiniGameSettingValue::Int(1),
            )
        },
        setting(
            "slayer:teamlives",
            "Team Lives",
            true,
            MiniGameSettingKind::Int { min: -1, max: 99 },
            MiniGameSettingValue::Int(-1),
        ),
    ];
    state.games[0].teams = vec![MiniGameTeam {
        id: 1,
        name: "Red".into(),
        color: 0,
        settings: Default::default(),
    }];
    state
}
fn addon_view(ui: &mut Ui) -> &mut bri_ui::view::View {
    let i = ui
        .dialogs
        .iter()
        .rposition(|s| s.id() == ScreenId::MiniGameAddOns)
        .unwrap();
    ui.dialogs[i].view_mut()
}
fn addon_event(ui: &mut Ui, name: &str, kind: EventKind) {
    let node = addon_view(ui).id(name).unwrap();
    let ev = ViewEvent { node, kind };
    let i = ui
        .dialogs
        .iter()
        .rposition(|s| s.id() == ScreenId::MiniGameAddOns)
        .unwrap();
    let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
    dialogs[i].on_event(&ev, core);
}

fn pick_category(ui: &mut Ui, category: &str) {
    let n = addon_view(ui).id("AOS_Category").unwrap();
    let selected = addon_view(ui)
        .node(n)
        .state
        .items
        .iter()
        .find(|(label, _)| label.ends_with(category))
        .unwrap()
        .1;
    addon_view(ui).select(n, Some(selected));
    addon_event(ui, "AOS_Category", EventKind::Changed);
}

#[test]
fn addon_setting_shown_unless_the_mode_is_one_of_some_values() {
    let mut ui = test_ui();
    let mut state = addon_state();
    let base = state.addon_settings[1].clone();
    state.addon_settings.push(MiniGameAddOnSetting {
        key: "slayer:respawnpenalty".into(),
        title: "Respawn Penalty".into(),
        shown_when: Some(MiniGameShownWhen {
            setting: "slayer:mode".into(),
            is: vec![],
            is_not: vec![MiniGameSettingValue::Text("dm".into())],
        }),
        ..base
    });
    ui.apply(UiUpdate::MiniGames(state));
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    click(&mut ui, ScreenId::MiniGameSettings, "NativeMiniGameAddOns");
    ui.update(0);
    assert!(
        addon_view(&mut ui).id("AOS_S4").is_none(),
        "hidden in Deathmatch"
    );
    let mode = addon_view(&mut ui).id("AOS_S0").unwrap();
    addon_view(&mut ui).select(mode, Some(1));
    addon_event(&mut ui, "AOS_S0", EventKind::Changed);
    assert!(
        addon_view(&mut ui).id("AOS_S4").is_some(),
        "shown in any other mode"
    );
}

#[test]
fn a_team_colour_look_shows_the_teams_colour_and_stays_the_teams() {
    let mut ui = test_ui();
    let mut state = addon_state();
    let base = state.addon_settings[3].clone();
    state.addon_settings.push(MiniGameAddOnSetting {
        key: "slayer:uni_hatcolor".into(),
        title: "Hat Color".into(),
        category: "Uniform".into(),
        kind: MiniGameSettingKind::Text { max_length: 32 },
        default: MiniGameSettingValue::Text("TEAMCOLOR".into()),
        avatar: Some("HatColor".into()),
        ..base
    });
    ui.apply(UiUpdate::MiniGames(state));
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    addon_event(&mut ui, "AOS_Teams", EventKind::Click);
    pick_category(&mut ui, "Uniform");
    // Red is palette colour 0: the editor shows the hat red.
    addon_event(&mut ui, "AOS_T0_Look_Uniform", EventKind::Click);
    let value = ui
        .core
        .avatar_value
        .clone()
        .expect("the avatar editor opens on the look");
    assert_eq!(value.look.get("HatColor"), Some("1 0 0 1"));
    assert_eq!(value.default.get("HatColor"), Some("1 0 0 1"));
    // Done without changing it: still the team's colour, nothing to send.
    ui.core.avatar_value.as_mut().unwrap().done = Some(value.look.clone());
    let i = ui
        .dialogs
        .iter()
        .rposition(|s| s.id() == ScreenId::MiniGameAddOns)
        .unwrap();
    let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
    dialogs[i].on_update(core);
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    assert!(
        !ui.drain_actions()
            .iter()
            .any(|(_, a)| matches!(a, UiAction::EditMiniGameAddOns { .. })),
        "TEAMCOLOR is kept"
    );
}

#[test]
fn an_addon_favourite_keeps_the_games_rules_and_apply_sends_them() {
    let addon_click = |ui: &mut Ui, command: &str| {
        let node = addon_view(ui).by_command(command).unwrap();
        let ev = ViewEvent {
            node,
            kind: EventKind::Click,
        };
        let i = ui
            .dialogs
            .iter()
            .rposition(|s| s.id() == ScreenId::MiniGameAddOns)
            .unwrap();
        let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
        dialogs[i].on_event(&ev, core);
    };
    let mut ui = test_ui();
    let mut state = addon_state();
    state.games[0].rules.title = "Alpha Round".into();
    ui.apply(UiUpdate::MiniGames(state));
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    addon_click(&mut ui, "AOS_FavSave");
    let saved = ui.core.settings.addon_favorites[&0].clone();
    assert_eq!(
        saved.rules.as_ref().map(|r| r.title.as_str()),
        Some("Alpha Round")
    );
    ui.drain_actions();
    // A favourite from another game: its rules go with Apply.
    let rules = MiniGameRules {
        title: "Favourite Arena".into(),
        ..Default::default()
    };
    ui.core.settings.addon_favorites.insert(
        0,
        AddOnFavorite {
            rules: Some(rules.clone()),
            ..saved
        },
    );
    addon_click(&mut ui, "AOS_FavLoad");
    addon_click(&mut ui, "AOS_Apply");
    let sent: Vec<_> = ui.drain_actions().into_iter().map(|(_, a)| a).collect();
    assert!(
        sent.contains(&UiAction::ConfigureMiniGame {
            game: MiniGameId(42),
            rules
        }),
        "{sent:?}"
    );
    // A favourite saved before rules were kept still loads.
    let old: AddOnFavorite = serde_json::from_str(r#"{"settings":{},"teams":[]}"#).unwrap();
    assert_eq!(old.rules, None);
}

#[test]
fn addon_settings_window_edits_settings_and_teams_and_sends_only_changes() {
    let mut ui = test_ui();
    ui.apply(UiUpdate::MiniGames(addon_state()));
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    // The editor offers the window for the game being edited.
    click(&mut ui, ScreenId::MiniGameSettings, "NativeMiniGameAddOns");
    ui.update(0);
    assert!(ui.is_open(ScreenId::MiniGameAddOns));
    // CTF's setting hides until Capture the Flag is picked.
    assert!(addon_view(&mut ui).id("AOS_S2").is_none());
    let mode = addon_view(&mut ui).id("AOS_S0").unwrap();
    addon_view(&mut ui).select(mode, Some(1));
    addon_event(&mut ui, "AOS_S0", EventKind::Changed);
    assert!(addon_view(&mut ui).id("AOS_S2").is_some(), "shown in CTF");
    let lives = addon_view(&mut ui).id("AOS_S1").unwrap();
    addon_view(&mut ui).set_text(lives, "3");
    // A second team, with its own lives.
    addon_event(&mut ui, "AOS_Teams", EventKind::Click);
    addon_event(&mut ui, "AOS_AddTeam", EventKind::Click);
    let name = addon_view(&mut ui).id("AOS_T1_Name").unwrap();
    addon_view(&mut ui).set_text(name, "Blue");
    let team_lives = addon_view(&mut ui).id("AOS_T1_S3").unwrap();
    addon_view(&mut ui).set_text(team_lives, "5");
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    let (_, action) = ui
        .drain_actions()
        .into_iter()
        .find(|(_, a)| matches!(a, UiAction::EditMiniGameAddOns { .. }))
        .expect("Apply sends the changes");
    let UiAction::EditMiniGameAddOns {
        game,
        settings,
        teams,
        ..
    } = action
    else {
        unreachable!()
    };
    assert_eq!(game, MiniGameId(42));
    assert_eq!(
        settings,
        vec![
            (
                "slayer:lives".to_string(),
                Some(MiniGameSettingValue::Int(3))
            ),
            (
                "slayer:mode".to_string(),
                Some(MiniGameSettingValue::Text("ctf".into()))
            ),
        ],
        "unchanged settings (Capture Points at its default) are not sent"
    );
    let teams = teams.expect("the team list changed");
    assert_eq!(teams.len(), 2);
    assert_eq!((teams[0].id, teams[0].name.as_str()), (Some(1), "Red"));
    assert!(teams[0].settings.is_empty());
    assert_eq!(
        (teams[1].id, teams[1].name.as_str(), teams[1].color),
        (None, "Blue", 1)
    );
    assert_eq!(
        teams[1].settings,
        vec![(
            "slayer:teamlives".to_string(),
            Some(MiniGameSettingValue::Int(5))
        )]
    );
}

#[test]
fn addon_settings_window_refuses_bad_numbers_and_is_read_only_for_others() {
    let mut ui = test_ui();
    let mut state = addon_state();
    ui.apply(UiUpdate::MiniGames(state.clone()));
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    let lives = addon_view(&mut ui).id("AOS_S1").unwrap();
    addon_view(&mut ui).set_text(lives, "100");
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    assert!(ui.drain_actions().is_empty(), "100 lives is out of range");
    ui.core.pop(ScreenId::MiniGameAddOns);
    state.addon_editable.clear();
    state.revision += 1;
    ui.apply(UiUpdate::MiniGames(state));
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    let v = addon_view(&mut ui);
    let lives = v.id("AOS_S1").unwrap();
    assert!(!v.node(lives).state.active);
    assert!(!v.node(v.id("AOS_AddTeam").unwrap()).state.visible);
    let apply = v.id("AOS_Apply").unwrap();
    assert!(!v.node(apply).state.visible);
}

#[test]
fn the_list_gains_a_default_column_while_a_game_is_the_default() {
    let mut ui = test_ui();
    ui.core.push(ScreenId::MiniGames);
    ui.apply(UiUpdate::MiniGames(game_state()));
    ui.update(0);
    let row = |ui: &Ui| {
        let view = ui.screen(ScreenId::MiniGames).unwrap().view();
        let list = view.id("JMG_List").unwrap();
        let header = view.id("JMG_DefaultHeader").unwrap();
        (
            view.node(list).state.items[0].0.split('\t').count(),
            view.node(list).ctrl.field("columns").map(str::to_owned),
            view.node(header).state.visible,
            view.node(list).state.items[0].0.clone(),
        )
    };
    let (fields, _, header, _) = row(&ui);
    assert_eq!(
        (fields, header),
        (4, false),
        "no default game: v20's four columns"
    );
    let mut state = game_state();
    state.games[0].default = true;
    state.revision += 1;
    ui.apply(UiUpdate::MiniGames(state));
    ui.update(0);
    let (fields, columns, header, line) = row(&ui);
    assert_eq!((fields, header), (5, true));
    assert!(line.ends_with("\tYes"), "{line}");
    assert_eq!(columns.as_deref(), Some("0 102 165 490 450"));
}

#[test]
fn server_addon_settings_open_from_the_admin_menu_for_the_host_and_go_with_host_options() {
    use bri_ui::models::admin::{
        AdminAction, AdminFeature, AdminOptions, AdminSnapshot, AdminUpdate,
    };
    let mut ui = test_ui();
    let mut state = addon_state();
    state.addon_settings.push(MiniGameAddOnSetting {
        key: "tier:tt_ammo".into(),
        add_on: "Tier+Tactical".into(),
        category: "Ammo".into(),
        title: "Ammo System".into(),
        team: false,
        server: true,
        restart: false,
        kind: MiniGameSettingKind::Int { min: 0, max: 3 },
        default: MiniGameSettingValue::Int(0),
        help: String::new(),
        avatar: None,
        shown_when: None,
    });
    // One the game reads only as it starts is marked, with the note.
    state.addon_settings.push(MiniGameAddOnSetting {
        key: "tier:tt_disableammoitems".into(),
        add_on: "Tier+Tactical".into(),
        category: "Ammo".into(),
        title: "Disable Pickups".into(),
        team: false,
        server: true,
        restart: true,
        kind: MiniGameSettingKind::Bool,
        default: MiniGameSettingValue::Bool(false),
        help: String::new(),
        avatar: None,
        shown_when: None,
    });
    ui.apply(UiUpdate::MiniGames(state));
    let mut options = AdminOptions::default();
    options
        .addon_settings
        .insert("other:kept".into(), MiniGameSettingValue::Bool(true));
    ui.apply(UiUpdate::Admin(AdminUpdate::State(AdminSnapshot {
        revision: 1,
        role: bri_ui::models::admin::AdminRole::SuperAdmin,
        local_host: true,
        legacy_lan: false,
        supported: [AdminFeature::HostOptions].into_iter().collect(),
        players: Vec::new(),
        options: Some(options),
    })));
    // A mini-game's window leaves the server's settings out.
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    assert!(addon_view(&mut ui).id("AOS_S4").is_none());
    ui.core.pop(ScreenId::MiniGameAddOns);
    ui.core.minigame_addons = None;
    ui.core.server_addon_settings = true;
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    assert!(
        addon_view(&mut ui).id("AOS_S0").is_none(),
        "only the server's"
    );
    let texts: Vec<String> = {
        let view = addon_view(&mut ui);
        (0..view.nodes.len()).map(|n| view.text_of(n)).collect()
    };
    assert!(texts.iter().any(|t| t == "Disable Pickups *"), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|t| t == bri_ui::screens::minigame_addons::RESTART_NOTE),
        "{texts:?}"
    );
    let ammo = addon_view(&mut ui)
        .id("AOS_S4")
        .expect("the server's setting");
    assert!(
        addon_view(&mut ui).node(ammo).state.active,
        "the host may change it"
    );
    addon_view(&mut ui).set_text(ammo, "2");
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    let (_, action) = ui
        .drain_actions()
        .into_iter()
        .find(|(_, a)| matches!(a, UiAction::Admin(AdminAction::ConfigureHost { .. })))
        .expect("Apply sends the host's settings");
    let UiAction::Admin(AdminAction::ConfigureHost { options }) = action else {
        unreachable!()
    };
    assert_eq!(
        options.addon_settings.get("tier:tt_ammo"),
        Some(&MiniGameSettingValue::Int(2))
    );
    assert_eq!(
        options.addon_settings.get("other:kept"),
        Some(&MiniGameSettingValue::Bool(true)),
        "an Add-On not running now keeps its value"
    );
    assert_eq!(
        ui.core.prefs.get("$Pref::Server::AddOn::tier::tt_ammo"),
        Some("2"),
        "saved with the host's other server prefs"
    );
}

#[test]
fn teams_work_without_addon_settings_and_remain_editable_in_a_small_window() {
    let mut ui = test_ui();
    let mut state = addon_state();
    state.addon_settings.clear();
    ui.apply(UiUpdate::MiniGames(state));
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    click(&mut ui, ScreenId::MiniGameSettings, "NativeMiniGameAddOns");
    ui.update(0);
    assert!(ui.is_open(ScreenId::MiniGameAddOns));
    let name = addon_view(&mut ui).id("AOS_T0_Name").unwrap();
    addon_view(&mut ui).set_text(name, "Blue");
    ui.resize((400, 300), Some(1.));
    let view = addon_view(&mut ui);
    let n = view.id("AOS_Window").unwrap();
    let r = view.node(n).rect;
    assert!(
        r.x >= 0 && r.y >= 0 && r.x + r.w <= 400 && r.y + r.h <= 300,
        "{r:?}"
    );
    let name = view.id("AOS_T0_Name").unwrap();
    assert_eq!(view.edit_text(name), "Blue");
    addon_event(&mut ui, "AOS_AddTeam", EventKind::Click);
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    let (_, action) = ui
        .drain_actions()
        .into_iter()
        .find(|(_, a)| matches!(a, UiAction::EditMiniGameAddOns { .. }))
        .expect("core teams send through the existing request");
    let UiAction::EditMiniGameAddOns {
        settings, teams, ..
    } = action
    else {
        unreachable!()
    };
    assert!(settings.is_empty());
    let teams = teams.unwrap();
    assert_eq!(teams.len(), 2);
    assert_eq!(teams[0].name, "Blue");
}

#[test]
fn asynchronous_listings_preserve_valid_and_partial_typed_team_names() {
    for name in ["Blue", ""] {
        let mut ui = test_ui();
        let mut state = addon_state();
        ui.apply(UiUpdate::MiniGames(state.clone()));
        ui.core.minigame_addons = Some(MiniGameId(42));
        ui.core.push(ScreenId::MiniGameAddOns);
        ui.update(0);
        addon_event(&mut ui, "AOS_Teams", EventKind::Click);
        let n = addon_view(&mut ui).id("AOS_T0_Name").unwrap();
        addon_view(&mut ui).set_text(n, name);
        addon_event(&mut ui, "AOS_T0_Name", EventKind::Changed);
        state.revision += 1;
        ui.apply(UiUpdate::MiniGames(state));
        ui.update(0);
        let n = addon_view(&mut ui).id("AOS_T0_Name").unwrap();
        assert_eq!(addon_view(&mut ui).edit_text(n), name);
        let n = addon_view(&mut ui).id("AOS_Status").unwrap();
        assert_eq!(addon_view(&mut ui).text_of(n), "Not applied yet.");
        if !name.is_empty() {
            addon_event(&mut ui, "AOS_Apply", EventKind::Click);
            let (_, action) = ui
                .drain_actions()
                .into_iter()
                .find(|(_, a)| matches!(a, UiAction::EditMiniGameAddOns { .. }))
                .unwrap();
            let UiAction::EditMiniGameAddOns { teams, .. } = action else {
                unreachable!()
            };
            assert_eq!(teams.unwrap()[0].name, "Blue");
        }
    }
}

fn journey_state() -> MiniGameUiState {
    let mut state = addon_state();
    state.games[0].teams.clear();
    for setting in &mut state.addon_settings {
        setting.add_on = "Orbit Games".into();
    }
    state.addon_settings[1].category = "Rounds".into();
    let game = state.addon_settings[1].clone();
    let team = state.addon_settings[3].clone();
    let equipment = MiniGameSettingKind::Item {
        items: vec![
            (MiniGameSettingValue::Text("none".into()), "None".into()),
            (MiniGameSettingValue::Text("gun".into()), "Gun".into()),
            (MiniGameSettingValue::Text("bow".into()), "Bow".into()),
        ],
    };
    state.addon_settings.extend([
        MiniGameAddOnSetting {
            key: "orbit:equipment".into(),
            category: "Equipment".into(),
            title: "Starting Equipment".into(),
            kind: equipment.clone(),
            default: MiniGameSettingValue::Text("none".into()),
            ..game.clone()
        },
        MiniGameAddOnSetting {
            key: "orbit:team_equipment".into(),
            category: "Equipment".into(),
            title: "Starting Equipment".into(),
            kind: equipment,
            default: MiniGameSettingValue::Text("none".into()),
            ..team
        },
        MiniGameAddOnSetting {
            key: "orbit:score_limit".into(),
            category: "Rounds".into(),
            title: "Score Limit".into(),
            default: MiniGameSettingValue::Int(10),
            ..game
        },
    ]);
    state
}

fn type_addon(ui: &mut Ui, name: &str, text: &str) {
    let n = addon_view(ui).id(name).unwrap();
    addon_view(ui).set_text(n, text);
    addon_event(ui, name, EventKind::Changed);
}
fn select_addon(ui: &mut Ui, name: &str, value: i64) {
    let n = addon_view(ui).id(name).unwrap();
    addon_view(ui).select(n, Some(value));
    addon_event(ui, name, EventKind::Changed);
}

#[test]
fn direct_teams_two_team_equipment_and_round_setup_saves_one_coherent_draft() {
    let mut ui = test_ui();
    ui.apply(UiUpdate::MiniGames(journey_state()));
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    click(&mut ui, ScreenId::MiniGameSettings, "NativeMiniGameTeams");
    ui.update(0);
    assert!(!ui.core.minigame_addons_teams, "opening consumes the route");
    let add = addon_view(&mut ui).id("AOS_AddTeam").unwrap();
    assert!(addon_view(&mut ui).node(add).state.visible);
    assert!(
        addon_view(&mut ui).id("AOS_S0").is_none(),
        "Teams skips game settings"
    );
    addon_event(&mut ui, "AOS_AddTeam", EventKind::Click);
    type_addon(&mut ui, "AOS_T0_Name", "Red");
    pick_category(&mut ui, "Equipment");
    select_addon(&mut ui, "AOS_T0_S5", 1);
    addon_event(&mut ui, "AOS_AddTeam", EventKind::Click);
    type_addon(&mut ui, "AOS_T1_Name", "Blue");
    select_addon(&mut ui, "AOS_T1_S5", 2);
    assert!(
        addon_view(&mut ui).id("AOS_T0_Name").is_none(),
        "one team's details at a time"
    );
    addon_event(&mut ui, "AOS_Setup", EventKind::Click);
    pick_category(&mut ui, "Rounds");
    type_addon(&mut ui, "AOS_S6", "25");
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    let action = ui
        .drain_actions()
        .into_iter()
        .find_map(|(_, a)| {
            if let UiAction::EditMiniGameAddOns {
                settings,
                teams: Some(teams),
                ..
            } = a
            {
                Some((settings, teams))
            } else {
                None
            }
        })
        .expect("Save sends the complete setup");
    assert_eq!(
        action.0,
        vec![(
            "orbit:score_limit".into(),
            Some(MiniGameSettingValue::Int(25))
        )]
    );
    assert_eq!(
        action
            .1
            .iter()
            .map(|t| (t.name.as_str(), t.color))
            .collect::<Vec<_>>(),
        vec![("Red", 0), ("Blue", 1)]
    );
    assert_eq!(
        action.1[0].settings,
        vec![
            (
                "orbit:team_equipment".into(),
                Some(MiniGameSettingValue::Text("gun".into()))
            ),
            ("slayer:teamlives".into(), None)
        ]
    );
    assert_eq!(
        action.1[1].settings,
        vec![
            (
                "orbit:team_equipment".into(),
                Some(MiniGameSettingValue::Text("bow".into()))
            ),
            ("slayer:teamlives".into(), None)
        ]
    );
}

#[test]
fn partial_hidden_fields_survive_navigation_resize_and_updates_but_block_save() {
    let mut ui = test_ui();
    let mut state = journey_state();
    state.games[0].teams.push(MiniGameTeam {
        id: 1,
        name: "Red".into(),
        color: 0,
        settings: Default::default(),
    });
    ui.apply(UiUpdate::MiniGames(state.clone()));
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    pick_category(&mut ui, "Rounds");
    type_addon(&mut ui, "AOS_S6", "-");
    addon_event(&mut ui, "AOS_Teams", EventKind::Click);
    type_addon(&mut ui, "AOS_T0_Name", "");
    addon_event(&mut ui, "AOS_AddTeam", EventKind::Click);
    type_addon(&mut ui, "AOS_T1_Name", "Blue");
    ui.resize((400, 300), Some(1.));
    state.revision += 1;
    ui.apply(UiUpdate::MiniGames(state));
    ui.update(0);
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    assert!(
        ui.drain_actions().is_empty(),
        "hidden invalid name/number cannot be silently defaulted"
    );
    select_addon(&mut ui, "AOS_Team", 0);
    let name = addon_view(&mut ui).id("AOS_T0_Name").unwrap();
    assert_eq!(addon_view(&mut ui).edit_text(name), "");
    type_addon(&mut ui, "AOS_T0_Name", "Red");
    addon_event(&mut ui, "AOS_Setup", EventKind::Click);
    pick_category(&mut ui, "Rounds");
    let score = addon_view(&mut ui).id("AOS_S6").unwrap();
    assert_eq!(addon_view(&mut ui).edit_text(score), "-");
    type_addon(&mut ui, "AOS_S6", "30");
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    assert!(
        ui.drain_actions()
            .iter()
            .any(|(_, a)| matches!(a, UiAction::EditMiniGameAddOns { .. }))
    );
}

#[test]
fn changing_team_or_removing_it_keeps_the_other_teams_draft_identity() {
    let mut ui = test_ui();
    ui.apply(UiUpdate::MiniGames(journey_state()));
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.minigame_addons_teams = true;
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    for (t, name) in [(0, "Red"), (1, "Blue"), (2, "Green")] {
        addon_event(&mut ui, "AOS_AddTeam", EventKind::Click);
        type_addon(&mut ui, &format!("AOS_T{t}_Name"), name);
    }
    select_addon(&mut ui, "AOS_Team", 1);
    type_addon(&mut ui, "AOS_T1_Name", "Blue revised");
    select_addon(&mut ui, "AOS_Team", 0);
    addon_event(&mut ui, "AOS_T0_Remove", EventKind::Click);
    let name = addon_view(&mut ui).id("AOS_T0_Name").unwrap();
    assert_eq!(addon_view(&mut ui).edit_text(name), "Blue revised");
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    let teams = ui
        .drain_actions()
        .into_iter()
        .find_map(|(_, a)| match a {
            UiAction::EditMiniGameAddOns { teams, .. } => teams,
            _ => None,
        })
        .unwrap();
    assert_eq!(
        teams.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        vec!["Blue revised", "Green"]
    );
}

#[test]
fn teams_and_add_team_stay_reachable_without_scrolling_unrelated_settings() {
    for size in [(400, 300), (640, 480), (1024, 768)] {
        let mut ui = test_ui();
        let mut state = journey_state();
        let base = state.addon_settings[1].clone();
        for i in 0..100 {
            state.addon_settings.push(MiniGameAddOnSetting {
                key: format!("unknown:setting{i}"),
                title: format!("Extra {i}"),
                category: "Unrelated".into(),
                add_on: "Unfamiliar Add-On".into(),
                ..base.clone()
            });
        }
        ui.apply(UiUpdate::MiniGames(state));
        ui.core.minigame_addons = Some(MiniGameId(42));
        ui.core.minigame_addons_teams = true;
        ui.core.push(ScreenId::MiniGameAddOns);
        ui.update(0);
        ui.resize(size, Some(1.));
        let view = addon_view(&mut ui);
        let add = view.id("AOS_AddTeam").unwrap();
        let scroll = view.id("AOS_Scroll").unwrap();
        let r = view.node(add).rect;
        assert!(
            r.y + r.h <= view.node(scroll).rect.y,
            "fixed Add Team is above details: {r:?}"
        );
        assert!(r.x >= 0 && r.y >= 0 && r.x + r.w <= size.0 as i32 && r.y + r.h <= size.1 as i32);
        assert_eq!(view.hit(r.x + r.w / 2, r.y + r.h / 2), Some(add));
        view.state(scroll).scroll_y = 5000;
        view.relayout();
        assert_eq!(view.node(add).rect, r, "detail scroll cannot move Add Team");
        assert_eq!(view.hit(r.x + r.w / 2, r.y + r.h / 2), Some(add));
    }
}

#[test]
fn permissions_and_mode_visibility_refresh_without_discarding_the_draft() {
    let mut ui = test_ui();
    let mut state = addon_state();
    state.teams_shown_when = Some(MiniGameShownWhen {
        setting: "slayer:mode".into(),
        is: vec![MiniGameSettingValue::Text("ctf".into())],
        is_not: vec![],
    });
    ui.apply(UiUpdate::MiniGames(state.clone()));
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    let teams = addon_view(&mut ui).id("AOS_Teams").unwrap();
    assert!(!addon_view(&mut ui).node(teams).state.active);
    select_addon(&mut ui, "AOS_S0", 1);
    addon_event(&mut ui, "AOS_Teams", EventKind::Click);
    type_addon(&mut ui, "AOS_T0_Name", "Red revised");
    state.revision += 1;
    state.addon_editable.clear();
    ui.apply(UiUpdate::MiniGames(state));
    ui.update(0);
    let name = addon_view(&mut ui).id("AOS_T0_Name").unwrap();
    assert_eq!(addon_view(&mut ui).edit_text(name), "Red revised");
    assert!(!addon_view(&mut ui).node(name).state.active);
    let save = addon_view(&mut ui).id("AOS_Apply").unwrap();
    assert!(!addon_view(&mut ui).node(save).state.visible);
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    assert!(ui.drain_actions().is_empty());
    addon_event(&mut ui, "AOS_Close", EventKind::Click);
    assert!(ui.drain_actions().is_empty(), "Cancel sends no settings");
}

#[test]
fn players_have_direct_access_and_assign_only_authoritative_team_ids() {
    let mut ui = test_ui();
    let mut state = addon_state();
    state.games[0].members.push(MiniGameTeamMember {
        id: MiniGamePlayerId(7),
        name: "Builder".into(),
        team: None,
    });
    ui.apply(UiUpdate::MiniGames(state));
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.minigame_addons_teams = true;
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    addon_event(&mut ui, "AOS_AddTeam", EventKind::Click);
    addon_event(&mut ui, "AOS_Players", EventKind::Click);
    let pick = addon_view(&mut ui).id("AOS_P7_Team").unwrap();
    assert_eq!(
        addon_view(&mut ui).node(pick).state.items,
        vec![("No team".into(), -1), ("Red".into(), 1)]
    );
    let view = addon_view(&mut ui);
    assert!(
        view.walk()
            .any(|n| view.text_of(n) == "Save new teams before assigning players.")
    );
    select_addon(&mut ui, "AOS_P7_Team", 1);
    assert!(ui.drain_actions().iter().any(|(_, a)| matches!(
        a,
        UiAction::SetMiniGameTeam {
            game: MiniGameId(42),
            target: MiniGamePlayerId(7),
            team: Some(1)
        }
    )));
}

#[test]
fn dependent_hidden_values_and_focused_text_are_preserved_on_listing_refresh() {
    let mut ui = test_ui();
    let mut state = addon_state();
    ui.apply(UiUpdate::MiniGames(state.clone()));
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    select_addon(&mut ui, "AOS_S0", 1);
    type_addon(&mut ui, "AOS_S2", "7");
    select_addon(&mut ui, "AOS_S0", 0);
    assert!(addon_view(&mut ui).id("AOS_S2").is_none());
    type_addon(&mut ui, "AOS_S1", "12");
    let lives = addon_view(&mut ui).id("AOS_S1").unwrap();
    addon_view(&mut ui).focus = Some(lives);
    addon_view(&mut ui).state(lives).cursor = 1;
    state.revision += 1;
    ui.apply(UiUpdate::MiniGames(state));
    ui.update(0);
    let lives = addon_view(&mut ui).id("AOS_S1").unwrap();
    assert_eq!(addon_view(&mut ui).focus, Some(lives));
    assert_eq!(addon_view(&mut ui).node(lives).state.cursor, 1);
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    let settings = ui
        .drain_actions()
        .into_iter()
        .find_map(|(_, a)| match a {
            UiAction::EditMiniGameAddOns { settings, .. } => Some(settings),
            _ => None,
        })
        .unwrap();
    assert!(settings.contains(&(
        "ctf:capturepoints".into(),
        Some(MiniGameSettingValue::Int(7))
    )));
    assert!(settings.contains(&("slayer:lives".into(), Some(MiniGameSettingValue::Int(12)))));
}

/// Native art/font inspection only; this never opens or drives a game window.
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires generated native UI content and an offscreen GPU"]
fn source_minigame_tasks_render_offscreen() -> anyhow::Result<()> {
    use bri_ui::gpu::{Headless, UiRenderer};
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pack = Rc::new(Pack::load(&bri_package::testing::pack_dir(
        &workspace.join("content"),
        "ui_pack",
    ))?);
    let out = workspace.join("artifacts/v022-minigame-ui");
    std::fs::create_dir_all(&out)?;
    let gpu = Headless::new()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    for (width, height, scale) in [
        (400, 300, 1.0),
        (1024, 768, 1.0),
        (1024, 768, 2.0),
        (1920, 1080, 2.0),
    ] {
        for page in ["Setup", "Teams", "Players"] {
            let mut ui = Ui::new(
                pack.clone(),
                UiConfig {
                    size: (width, height),
                    scale: Some(scale),
                    platform: Platform::Windows,
                },
                Settings {
                    binds: Some(vec![]),
                    ..Default::default()
                },
            );
            let mut state = journey_state();
            state.games[0].teams = vec![
                MiniGameTeam {
                    id: 1,
                    name: "Red".into(),
                    color: 0,
                    settings: Default::default(),
                },
                MiniGameTeam {
                    id: 2,
                    name: "Blue".into(),
                    color: 1,
                    settings: Default::default(),
                },
            ];
            state.games[0].members.push(MiniGameTeamMember {
                id: MiniGamePlayerId(7),
                name: "Builder".into(),
                team: Some(1),
            });
            ui.apply(UiUpdate::MiniGames(state));
            ui.core.minigame_addons = Some(MiniGameId(42));
            ui.core.push(ScreenId::MiniGameAddOns);
            ui.update(0);
            addon_event(&mut ui, &format!("AOS_{page}"), EventKind::Click);
            if page == "Setup" {
                pick_category(&mut ui, "Rounds");
            }
            if page == "Teams" {
                pick_category(&mut ui, "Equipment");
            }
            let draw = ui.draw();
            let pixels = gpu.render_rgba(
                &mut renderer,
                &pack,
                &draw,
                (width, height),
                ui.scale(),
                [0.12, 0.16, 0.22, 1.0],
            )?;
            image::save_buffer(
                out.join(format!("{page}-{width}x{height}-{scale}x.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )?;
            assert!(!draw.cmds.is_empty());
            assert_eq!(pixels.len(), (width * height * 4) as usize);
        }
    }
    // Read generated Slayer definitions directly. This test-only adapter
    // mirrors client minigame_ui's typed metadata mapping, without a runtime
    // dependency or a product-side Add-On-name branch.
    let metadata: serde_json::Value = serde_json::from_slice(&std::fs::read(
        workspace.join("content/addons/gamemode_slayer-rules/behaviour.json"),
    )?)?;
    let value = |v: &serde_json::Value| {
        if let Some(v) = v.as_bool() {
            MiniGameSettingValue::Bool(v)
        } else if let Some(v) = v.as_i64() {
            MiniGameSettingValue::Int(v)
        } else {
            MiniGameSettingValue::Text(v.as_str().unwrap().into())
        }
    };
    let condition = |w: &serde_json::Value| MiniGameShownWhen {
        setting: if w["setting"].as_str().unwrap().contains(':') {
            w["setting"].as_str().unwrap().into()
        } else {
            format!("gamemode_slayer-rules:{}", w["setting"].as_str().unwrap())
        },
        is: w["is"]
            .as_array()
            .map(|v| v.iter().map(value).collect())
            .unwrap_or_default(),
        is_not: w["is_not"]
            .as_array()
            .map(|v| v.iter().map(value).collect())
            .unwrap_or_default(),
    };
    let mut actual = addon_state();
    actual.games[0].addon_settings.clear();
    actual.addon_settings =
        metadata["settings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let kind =
                    match s["type"].as_str().unwrap() {
                        "bool" => MiniGameSettingKind::Bool,
                        "int" => MiniGameSettingKind::Int {
                            min: s["min"].as_i64().unwrap(),
                            max: s["max"].as_i64().unwrap(),
                        },
                        "text" => MiniGameSettingKind::Text {
                            max_length: s["max_length"].as_u64().unwrap() as u32,
                        },
                        "paint_color" => MiniGameSettingKind::PaintColor {
                            min: s["min"].as_i64().unwrap_or(0),
                            max: s["max"].as_i64().unwrap_or(63),
                        },
                        "list" => MiniGameSettingKind::List {
                            items: s["items"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|i| (value(&i["value"]), i["name"].as_str().unwrap().into()))
                                .collect(),
                        },
                        "item" => MiniGameSettingKind::Item {
                            items: std::iter::once((
                                MiniGameSettingValue::Text(String::new()),
                                "NONE".into(),
                            ))
                            .chain(actual.items.iter().map(|i| {
                                (MiniGameSettingValue::Text(i.id.clone()), i.name.clone())
                            }))
                            .collect(),
                        },
                        "player_type" => MiniGameSettingKind::PlayerType {
                            items: actual
                                .player_types
                                .iter()
                                .map(|i| (MiniGameSettingValue::Text(i.id.clone()), i.name.clone()))
                                .collect(),
                        },
                        other => panic!("unknown generated setting kind {other}"),
                    };
                MiniGameAddOnSetting {
                    key: format!("gamemode_slayer-rules:{}", s["key"].as_str().unwrap()),
                    add_on: "Slayer".into(),
                    category: s["category"].as_str().unwrap_or("").into(),
                    title: s["title"].as_str().unwrap().into(),
                    team: s["scope"] == "team",
                    server: s["scope"] == "server",
                    restart: s["restart"].as_bool().unwrap_or(false),
                    kind,
                    default: value(&s["default"]),
                    shown_when: s.get("shown_when").map(condition),
                    help: s["help"].as_str().unwrap_or("").into(),
                    avatar: s["avatar"].as_str().map(String::from),
                }
            })
            .collect();
    assert_eq!(
        actual
            .addon_settings
            .iter()
            .filter(|s| !s.team && !s.server)
            .count(),
        65
    );
    assert_eq!(actual.addon_settings.iter().filter(|s| s.team).count(), 54);
    actual.teams_shown_when = metadata.get("teams_shown_when").map(condition);
    for (width, height) in [(400, 300), (1024, 768)] {
        for page in ["Setup", "Teams"] {
            let mut state = actual.clone();
            if page == "Teams" {
                state.games[0].addon_settings.insert(
                    "gamemode_slayer-rules:mode".into(),
                    MiniGameSettingValue::Text("Slayer_TeamDeathmatch".into()),
                );
                state.games[0].teams.push(MiniGameTeam {
                    id: 2,
                    name: "Blue".into(),
                    color: 1,
                    settings: Default::default(),
                });
            }
            let mut ui = Ui::new(
                pack.clone(),
                UiConfig {
                    size: (width, height),
                    scale: Some(1.0),
                    platform: Platform::Windows,
                },
                Settings {
                    binds: Some(vec![]),
                    ..Default::default()
                },
            );
            ui.apply(UiUpdate::MiniGames(state));
            ui.core.minigame_addons = Some(MiniGameId(42));
            ui.core.push(ScreenId::MiniGameAddOns);
            ui.update(0);
            addon_event(&mut ui, &format!("AOS_{page}"), EventKind::Click);
            let draw = ui.draw();
            let pixels = gpu.render_rgba(
                &mut renderer,
                &pack,
                &draw,
                (width, height),
                ui.scale(),
                [0.12, 0.16, 0.22, 1.0],
            )?;
            image::save_buffer(
                out.join(format!("Slayer-{page}-{width}x{height}-1x.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )?;
            assert!(!draw.cmds.is_empty());
        }
    }
    Ok(())
}

#[test]
fn stock_partial_rules_and_loadout_survive_teams_navigation_and_listing_refresh() {
    let mut ui = test_ui();
    let mut state = addon_state();
    state.owns_active_game = true;
    ui.apply(UiUpdate::MiniGames(state.clone()));
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    let i = ui
        .dialogs
        .iter()
        .rposition(|s| s.id() == ScreenId::MiniGameSettings)
        .unwrap();
    let v = ui.dialogs[i].view_mut();
    let number = v
        .walk()
        .find(|&n| v.node(n).ctrl.variable.as_deref() == Some("$MiniGame::RespawnTime"))
        .unwrap();
    v.set_text(number, "-");
    let equip = v.id("CMG_StartEquip0").unwrap();
    v.select(equip, Some(1));
    let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
    dialogs[i].on_event(
        &ViewEvent {
            node: number,
            kind: EventKind::Changed,
        },
        core,
    );
    click(&mut ui, ScreenId::MiniGameSettings, "NativeMiniGameTeams");
    ui.update(0);
    state.revision += 1;
    state.games[0].rules.respawn_seconds = 8;
    ui.apply(UiUpdate::MiniGames(state.clone()));
    ui.update(0);
    addon_event(&mut ui, "AOS_Close", EventKind::Click);
    ui.update(0);
    let view = ui.screen(ScreenId::MiniGameSettings).unwrap().view();
    assert_eq!(view.edit_text(number), "-");
    assert_eq!(view.selected(equip), Some(1));
    state.revision += 1;
    state.capabilities.configure = false;
    ui.apply(UiUpdate::MiniGames(state));
    ui.update(0);
    let view = ui.screen(ScreenId::MiniGameSettings).unwrap().view();
    let save = view.by_command("CreateMiniGameGui.clickCreate();").unwrap();
    assert!(
        !view.node(save).state.active,
        "preserved draft cannot bypass new permissions"
    );
}

#[test]
fn semantic_team_loadout_and_body_choices_precede_deeper_authored_settings() {
    let mut ui = test_ui();
    let mut state = journey_state();
    state.addon_settings[5].category = state.addon_settings[3].category.clone();
    let base = state.addon_settings[3].clone();
    for i in 0..20 {
        state.addon_settings.push(MiniGameAddOnSetting {
            key: format!("orbit:detail_{i}"),
            title: format!("Other rule {i}"),
            ..base.clone()
        });
    }
    let body_index = state.addon_settings.len();
    state.addon_settings.push(MiniGameAddOnSetting {
        key: "orbit:body".into(),
        title: "Body".into(),
        kind: MiniGameSettingKind::PlayerType {
            items: vec![
                (
                    MiniGameSettingValue::Text("standard".into()),
                    "Standard".into(),
                ),
                (MiniGameSettingValue::Text("runner".into()), "Runner".into()),
            ],
        },
        default: MiniGameSettingValue::Text("standard".into()),
        ..base
    });
    ui.apply(UiUpdate::MiniGames(state));
    ui.core.minigame_addons = Some(MiniGameId(42));
    ui.core.minigame_addons_teams = true;
    ui.core.push(ScreenId::MiniGameAddOns);
    ui.update(0);
    addon_event(&mut ui, "AOS_AddTeam", EventKind::Click);
    let view = addon_view(&mut ui);
    let loadout = view.id("AOS_T0_S5").unwrap();
    let body = view.id(&format!("AOS_T0_S{body_index}")).unwrap();
    let lives = view.id("AOS_T0_S3").unwrap();
    assert!(view.node(loadout).ctrl.position[1] < view.node(lives).ctrl.position[1]);
    assert!(view.node(body).ctrl.position[1] < view.node(lives).ctrl.position[1]);
    assert_eq!(view.selected(loadout), Some(0));
    assert_eq!(view.selected(body), Some(0));
    select_addon(&mut ui, "AOS_T0_S5", 1);
    select_addon(&mut ui, &format!("AOS_T0_S{body_index}"), 1);
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    assert!(ui.drain_actions().iter().any(|(_, a)| matches!(a,UiAction::EditMiniGameAddOns {teams:Some(teams),..} if teams[0].settings.contains(&("orbit:body".into(),Some(MiniGameSettingValue::Text("runner".into())))) && teams[0].settings.contains(&("orbit:team_equipment".into(),Some(MiniGameSettingValue::Text("gun".into())))))));
}
