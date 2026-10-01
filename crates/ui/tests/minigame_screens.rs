use bri_ui::{api::*, binds::Platform, pack::Pack, schema::{Control,UiPack}, screens::ScreenId, ui::{Pending,Ui,UiConfig}, view::{EventKind,ViewEvent}};
use std::{path::PathBuf,rc::Rc};

fn ctl(class:&str,name:Option<&str>,command:Option<&str>,variable:Option<&str>)->Control{
    Control{class:class.into(),name:name.map(str::to_owned),command:command.map(str::to_owned),variable:variable.map(str::to_owned),
        style:"GuiDefaultProfile".into(),position:[8,8],extent:[120,24],visible:true,..Default::default()}
}
fn layout(children:Vec<Control>)->Control{Control{class:"GuiControl".into(),name:Some("root".into()),extent:[640,480],visible:true,children,..Default::default()}}
fn test_ui()->Ui{
    let mut pack=UiPack::default();
    pack.layouts.insert("joinMiniGameGui".into(),layout(vec![
        ctl("GuiTextListCtrl",Some("JMG_List"),Some("JoinMiniGameGui.clickList();"),None),
        ctl("GuiBitmapButtonCtrl",None,Some("JoinMiniGameGui.clickJoin();"),None),
        ctl("GuiBitmapButtonCtrl",None,Some("JoinMiniGameGui.clickLeave();"),None),
        ctl("GuiBitmapButtonCtrl",None,Some("JoinMiniGameGui.clickCreate();"),None),
    ]));
    pack.layouts.insert("CreateMiniGameGui".into(),layout(vec![
        ctl("GuiWindowCtrl",Some("CMG_Window"),None,None),
        ctl("GuiBitmapButtonCtrl",Some("CMG_CreateButton"),Some("CreateMiniGameGui.clickCreate();"),None),
        ctl("GuiBitmapButtonCtrl",None,Some("CreateMiniGameGui.clickReset();"),None),
        ctl("GuiBitmapButtonCtrl",None,Some("CreateMiniGameGui.clickEnd();"),None),
        ctl("GuiPopUpMenuCtrl",Some("CMG_ColorList"),Some("CreateMiniGameGui.clickColorList();"),None),
        ctl("GuiPopUpMenuCtrl",Some("CMG_PlayerDataBlock"),None,None),
        ctl("GuiPopUpMenuCtrl",Some("CMG_StartEquip0"),None,None),
        ctl("GuiPopUpMenuCtrl",Some("CMG_StartEquip1"),None,None),
        ctl("GuiPopUpMenuCtrl",Some("CMG_StartEquip2"),None,None),
        ctl("GuiPopUpMenuCtrl",Some("CMG_StartEquip3"),None,None),
        ctl("GuiPopUpMenuCtrl",Some("CMG_StartEquip4"),None,None),
        ctl("GuiSwatchCtrl",Some("CMG_Swatch"),None,None),
        ctl("GuiTextEditCtrl",None,None,Some("$MiniGame::Title")),
        ctl("GuiTextEditCtrl",None,None,Some("$MiniGame::Points::BreakBrick")),
        ctl("GuiTextEditCtrl",None,None,Some("$MiniGame::Points::PlantBrick")),
        ctl("GuiTextEditCtrl",None,None,Some("$MiniGame::Points::KillPlayer")),
        ctl("GuiTextEditCtrl",None,None,Some("$MiniGame::Points::KillSelf")),
        ctl("GuiTextEditCtrl",None,None,Some("$MiniGame::Points::Die")),
        ctl("GuiTextEditCtrl",None,None,Some("$MiniGame::RespawnTime")),
        ctl("GuiTextEditCtrl",None,None,Some("$MiniGame::VehicleRespawnTime")),
        ctl("GuiTextEditCtrl",None,None,Some("$MiniGame::BrickRespawnTime")),
        ctl("GuiCheckBoxCtrl",None,None,Some("$MiniGame::InviteOnly")),
        bool_ctl("$MiniGame::PlayersUseOwnBricks",false),bool_ctl("$MiniGame::UseAllPlayersBricks",false),bool_ctl("$MiniGame::UseSpawnBricks",true),
        bool_ctl("$MiniGame::FallingDamage",true),bool_ctl("$MiniGame::WeaponDamage",true),bool_ctl("$MiniGame::SelfDamage",true),
        bool_ctl("$MiniGame::VehicleDamage",true),bool_ctl("$MiniGame::BrickDamage",true),bool_ctl("$MiniGame::EnableWand",false),
        bool_ctl("$MiniGame::EnableBuilding",true),bool_ctl("$MiniGame::EnablePainting",true),
        Control{visible:false,..ctl("GuiSwatchCtrl",Some("CMG_FavsHelper"),None,None)},
        ctl("GuiBitmapButtonCtrl",None,Some("CreateMiniGameGui.ClickSetFavs();"),None),
        ctl("GuiBitmapButtonCtrl",Some("BSD_FavButton3"),Some("CreateMiniGameGui.clickFav(3);"),None),
    ]));
    pack.layouts.insert("MiniGameInviteGui".into(),layout(vec![
        ctl("GuiMLTextCtrl",Some("MGI_Title"),None,None),ctl("GuiMLTextCtrl",Some("MGI_Name"),None,None),ctl("GuiMLTextCtrl",Some("MGI_BL_ID"),None,None),
        ctl("GuiBitmapButtonCtrl",None,Some("MiniGameInviteGui.clickAccept();"),None),
        ctl("GuiBitmapButtonCtrl",None,Some("MiniGameInviteGui.clickReject();"),None),
        ctl("GuiBitmapButtonCtrl",None,Some("MiniGameInviteGui.clickIgnore();"),None),
    ]));
    Ui::new(Rc::new(Pack::from_parts(pack,PathBuf::new())),UiConfig{size:(640,480),scale:Some(1.0),platform:Platform::Windows},Settings{binds:Some(vec![]),..Default::default()})
}
fn bool_ctl(variable:&str,value:bool)->Control{let mut c=ctl("GuiCheckBoxCtrl",None,None,Some(variable));c.fields.insert("value".into(),if value{"1"}else{"0"}.into());c}
fn game_state()->MiniGameUiState{
    let rules=MiniGameRules::default();
    MiniGameUiState{addon_locked:vec![],teams_shown_when:None,ready:true,revision:1,capabilities:MiniGameCapabilities{list:true,create:true,configure:true,join:true,leave:true,invite:true,respond_invite:true,remove_member:true,reset:true,respawn_all:true,end:true,scoreboard:true},
        games:vec![MiniGameSummary{default:false,paint_color:None,members:vec![],id:MiniGameId(42),title:"Alpha Round".into(),owner:MiniGamePlayerId(900),owner_name:"Owner".into(),color:0,member_count:2,invite_only:false,rules,teams:vec![],addon_settings:Default::default()}],
        colors:vec![MiniGameColor{index:0,name:"Red".into(),rgb:[255,0,0]}],
        active_game:None,owns_active_game:false,local_player:Some(MiniGamePlayerId(7)),members:vec![],invitations:vec![],
        player_types:vec![MiniGameChoice{id:"v20.player.playerstandardarmor".into(),name:"Standard Player".into()}],
        items:vec!["hammeritem","wrenchitem","printgun","gunitem","rocketlauncheritem"].into_iter().map(|s|MiniGameChoice{id:format!("v20.weapon.{s}"),name:s.into()}).collect(),status:String::new(),addon_settings:vec![],addon_editable:vec![],palette:vec![]}
}
fn click(ui:&mut Ui,screen:ScreenId,command:&str){
    let node=ui.screen(screen).unwrap().view().by_command(command).unwrap();
    let ev=ViewEvent{node,kind:EventKind::Click};
    let i=ui.dialogs.iter().rposition(|s|s.id()==screen).unwrap();
    let (dialogs,core)=(&mut ui.dialogs,&mut ui.core);
    dialogs[i].on_event(&ev,core);
}

#[test]
fn public_join_uses_stable_game_selector_and_fails_closed_without_host_capability(){
    let mut ui=test_ui();
    ui.core.push(ScreenId::MiniGames);ui.update(0);
    let join=ui.screen(ScreenId::MiniGames).unwrap().view().by_command("JoinMiniGameGui.clickJoin();").unwrap();
    assert!(!ui.screen(ScreenId::MiniGames).unwrap().view().node(join).state.active);
    click(&mut ui,ScreenId::MiniGames,"JoinMiniGameGui.clickJoin();");
    assert!(ui.drain_actions().is_empty());
    ui.apply(UiUpdate::MiniGames(game_state()));ui.update(0);
    click(&mut ui,ScreenId::MiniGames,"JoinMiniGameGui.clickJoin();");
    let actions=ui.drain_actions();
    assert_eq!(actions.len(),1);
    assert_eq!(actions[0].1,UiAction::JoinMiniGame{game:MiniGameId(42)});
    assert_eq!(ui.core.pending.get(&actions[0].0),Some(&Pending::MiniGame(MiniGameOperation::Join)));
}

#[test]
fn stock_defaults_are_submitted_and_admin_does_not_unlock_owner_actions(){
    let mut ui=test_ui();
    let mut state=game_state();state.colors[0].index=4;ui.core.minigames=state.clone();
    ui.core.push(ScreenId::MiniGameSettings);ui.update(0);
    click(&mut ui,ScreenId::MiniGameSettings,"CreateMiniGameGui.clickCreate();");
    let (_,action)=ui.drain_actions().pop().unwrap();
    let UiAction::CreateMiniGame{color,rules}=action else{panic!("wrong action")};
    assert_eq!(color,4);assert_eq!(rules,MiniGameRules::default());
    state.active_game=Some(MiniGameId(42));state.owns_active_game=false;state.capabilities.reset=true;state.capabilities.end=true;
    state.capabilities.configure=true;state.capabilities.create=true;state.games[0].id=MiniGameId(42);state.local_player=Some(MiniGamePlayerId(7));
    ui.core.minigames=state;ui.core.push(ScreenId::MiniGameSettings);ui.update(0);
    let view=ui.screen(ScreenId::MiniGameSettings).unwrap().view();
    assert!(!view.node(view.by_command("CreateMiniGameGui.clickReset();").unwrap()).state.active);
    assert!(!view.node(view.by_command("CreateMiniGameGui.clickEnd();").unwrap()).state.active);
}

#[test]
fn invitation_acceptance_keeps_stable_game_identity(){
    let mut ui=test_ui();let invite=MiniGameInvitation{game:MiniGameId(8),title:"Private Game".into(),owner:MiniGamePlayerId(90),owner_name:"Host".into(),owner_display_id:"LAN".into()};
    ui.core.minigames=game_state();ui.core.minigames.invitations.push(invite.clone());ui.core.push(ScreenId::MiniGameInvitation);ui.update(0);
    click(&mut ui,ScreenId::MiniGameInvitation,"MiniGameInviteGui.clickAccept();");
    assert_eq!(ui.drain_actions().pop().unwrap().1,UiAction::AcceptMiniGameInvite{game:invite.game});
    assert_eq!(ui.core.minigames.invitations[0].owner_display_id,"LAN");
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
    assert!(!active(&ui, "CreateMiniGameGui.clickReset();"), "nothing to reset yet");
    assert!(!active(&ui, "CreateMiniGameGui.clickEnd();"), "nothing to end yet");
    click(&mut ui, ScreenId::MiniGameSettings, "CreateMiniGameGui.clickCreate();");
    let (id, _) = ui
        .drain_actions()
        .into_iter()
        .find(|(_, a)| matches!(a, UiAction::CreateMiniGame { .. }))
        .unwrap();
    ui.apply(UiUpdate::ActionResult { id, result: Ok(()) });
    assert!(!ui.is_open(ScreenId::MiniGameSettings), "creating closes the editor");

    let mut running = game_state();
    running.active_game = Some(MiniGameId(42));
    running.owns_active_game = true;
    running.revision = 2;
    ui.apply(UiUpdate::MiniGames(running));
    ui.core.push(ScreenId::MiniGameSettings);
    ui.update(0);
    assert!(active(&ui, "CreateMiniGameGui.clickReset();"));
    assert!(active(&ui, "CreateMiniGameGui.clickEnd();"));
    click(&mut ui, ScreenId::MiniGameSettings, "CreateMiniGameGui.clickReset();");
    let (id, _) = ui
        .drain_actions()
        .into_iter()
        .find(|(_, a)| matches!(a, UiAction::ResetMiniGame { .. }))
        .expect("v20 resets without a confirmation");
    ui.apply(UiUpdate::ActionResult { id, result: Ok(()) });
    assert!(!ui.is_open(ScreenId::MiniGameSettings), "resetting closes the editor");
}

#[test]
fn set_favs_saves_the_form_to_a_slot_and_the_slot_fills_it_again(){
    let mut ui=test_ui();
    ui.apply(UiUpdate::MiniGames(game_state()));
    ui.core.push(ScreenId::MiniGameSettings);ui.update(0);
    let title=|ui:&mut Ui,text:Option<&str>|{
        let i=ui.dialogs.iter().rposition(|s|s.id()==ScreenId::MiniGameSettings).unwrap();
        let v=ui.dialogs[i].view_mut();
        let n=v.walk().find(|&n|v.node(n).ctrl.variable.as_deref()==Some("$MiniGame::Title")).unwrap();
        if let Some(t)=text{v.set_text(n,t);}
        v.edit_text(n)
    };
    title(&mut ui,Some("Rocket Arena"));
    click(&mut ui,ScreenId::MiniGameSettings,"CreateMiniGameGui.ClickSetFavs();");
    click(&mut ui,ScreenId::MiniGameSettings,"CreateMiniGameGui.clickFav(3);");
    assert_eq!(ui.core.settings.minigame_favorites[&3].rules.title,"Rocket Arena");
    assert!(ui.drain_actions().iter().any(|(_,a)|matches!(a,UiAction::SaveSettings(s) if s.minigame_favorites.contains_key(&3))));
    title(&mut ui,Some("Something Else"));
    click(&mut ui,ScreenId::MiniGameSettings,"CreateMiniGameGui.clickFav(3);");
    assert_eq!(title(&mut ui,None),"Rocket Arena");
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
                items: vec![(text("dm"), "Deathmatch".into()), (text("ctf"), "Capture the Flag".into())],
            },
            text("dm"),
        ),
        setting("slayer:lives", "Lives", false, MiniGameSettingKind::Int { min: 0, max: 99 }, MiniGameSettingValue::Int(0)),
        MiniGameAddOnSetting {
            shown_when: Some(MiniGameShownWhen { setting: "slayer:mode".into(), is: vec![text("ctf")], is_not: vec![] }),
            ..setting("ctf:capturepoints", "Capture Points", false, MiniGameSettingKind::Int { min: 0, max: 99 }, MiniGameSettingValue::Int(1))
        },
        setting("slayer:teamlives", "Team Lives", true, MiniGameSettingKind::Int { min: -1, max: 99 }, MiniGameSettingValue::Int(-1)),
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
    let i = ui.dialogs.iter().rposition(|s| s.id() == ScreenId::MiniGameAddOns).unwrap();
    ui.dialogs[i].view_mut()
}
fn addon_event(ui: &mut Ui, name: &str, kind: EventKind) {
    let node = addon_view(ui).id(name).unwrap();
    let ev = ViewEvent { node, kind };
    let i = ui.dialogs.iter().rposition(|s| s.id() == ScreenId::MiniGameAddOns).unwrap();
    let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
    dialogs[i].on_event(&ev, core);
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
    assert!(addon_view(&mut ui).id("AOS_S4").is_none(), "hidden in Deathmatch");
    let mode = addon_view(&mut ui).id("AOS_S0").unwrap();
    addon_view(&mut ui).select(mode, Some(1));
    addon_event(&mut ui, "AOS_S0", EventKind::Changed);
    assert!(addon_view(&mut ui).id("AOS_S4").is_some(), "shown in any other mode");
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
    // Red is palette colour 0: the editor shows the hat red.
    addon_event(&mut ui, "AOS_T0_Look_Uniform", EventKind::Click);
    let value = ui.core.avatar_value.clone().expect("the avatar editor opens on the look");
    assert_eq!(value.look.get("HatColor"), Some("1 0 0 1"));
    assert_eq!(value.default.get("HatColor"), Some("1 0 0 1"));
    // Done without changing it: still the team's colour, nothing to send.
    ui.core.avatar_value.as_mut().unwrap().done = Some(value.look.clone());
    let i = ui.dialogs.iter().rposition(|s| s.id() == ScreenId::MiniGameAddOns).unwrap();
    let (dialogs, core) = (&mut ui.dialogs, &mut ui.core);
    dialogs[i].on_update(core);
    addon_event(&mut ui, "AOS_Apply", EventKind::Click);
    assert!(
        !ui.drain_actions().iter().any(|(_, a)| matches!(a, UiAction::EditMiniGameAddOns { .. })),
        "TEAMCOLOR is kept"
    );
}

#[test]
fn an_addon_favourite_keeps_the_games_rules_and_apply_sends_them() {
    let addon_click = |ui: &mut Ui, command: &str| {
        let node = addon_view(ui).by_command(command).unwrap();
        let ev = ViewEvent { node, kind: EventKind::Click };
        let i = ui.dialogs.iter().rposition(|s| s.id() == ScreenId::MiniGameAddOns).unwrap();
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
    assert_eq!(saved.rules.as_ref().map(|r| r.title.as_str()), Some("Alpha Round"));
    ui.drain_actions();
    // A favourite from another game: its rules go with Apply.
    let rules = MiniGameRules { title: "Favourite Arena".into(), ..Default::default() };
    ui.core.settings.addon_favorites.insert(0, AddOnFavorite { rules: Some(rules.clone()), ..saved });
    addon_click(&mut ui, "AOS_FavLoad");
    addon_click(&mut ui, "AOS_Apply");
    let sent: Vec<_> = ui.drain_actions().into_iter().map(|(_, a)| a).collect();
    assert!(
        sent.contains(&UiAction::ConfigureMiniGame { game: MiniGameId(42), rules }),
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
    let UiAction::EditMiniGameAddOns { game, settings, teams, .. } = action else {
        unreachable!()
    };
    assert_eq!(game, MiniGameId(42));
    assert_eq!(
        settings,
        vec![
            ("slayer:lives".to_string(), Some(MiniGameSettingValue::Int(3))),
            ("slayer:mode".to_string(), Some(MiniGameSettingValue::Text("ctf".into()))),
        ],
        "unchanged settings (Capture Points at its default) are not sent"
    );
    let teams = teams.expect("the team list changed");
    assert_eq!(teams.len(), 2);
    assert_eq!((teams[0].id, teams[0].name.as_str()), (Some(1), "Red"));
    assert!(teams[0].settings.is_empty());
    assert_eq!((teams[1].id, teams[1].name.as_str(), teams[1].color), (None, "Blue", 1));
    assert_eq!(
        teams[1].settings,
        vec![("slayer:teamlives".to_string(), Some(MiniGameSettingValue::Int(5)))]
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
    assert!(v.id("AOS_AddTeam").is_none());
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
    assert_eq!((fields, header), (4, false), "no default game: v20's four columns");
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
