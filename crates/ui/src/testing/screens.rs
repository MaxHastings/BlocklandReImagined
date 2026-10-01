//! Made-up layouts for the menu, host, admin, wrench and chat screens, so
//! the screens can be driven end to end without a converted UI pack.
//!
//! Every layout carries the control names, preference variables and
//! command strings the screens themselves look up (`SM_missionList`,
//! `$Pref::Server::Name`, `SM_StartMission();`, ...); every position, size,
//! label and colour is invented. Each window keeps its controls inside it
//! and clear of each other, so a click at a control's centre reaches it.
use super::{FONT, add_dialogs, dialog, font, label, named, style, text_button};
use crate::{
    geom::Rect,
    models::admin::{option_pairs, options_from_prefs},
    pack::Pack,
    prefs::Prefs,
    schema::{
        Control, EventTables, HSizing, HelpPage, InputEventDef, OutputEventDef, ParamSpec, UiPack,
        VSizing,
    },
    screens::{ctrl, swatch},
};
use std::rc::Rc;

/// The fonts the help pages' ML text switches between.
pub const HELP_FONTS: [(&str, u32); 2] = [("arial", 14), ("arial bold", 20)];

/// The profile Join Server's list draws with: outlined, so a list that
/// drew its rows with the profile's outline would draw each glyph five
/// times.
pub const SERVER_LIST_PROFILE: &str = "ServerListProfile";

/// Event inputs and their (target, class) choices, and brick and player
/// outputs, shaped like the stock tables: two outputs on the brick (a
/// paint colour, a list), one on the player with no parameters.
pub fn event_tables() -> EventTables {
    let targets = vec![
        ("Self".to_string(), "fxDTSBrick".to_string()),
        ("Player".to_string(), "Player".to_string()),
    ];
    let input = |name: &str| InputEventDef {
        class: "fxDTSBrick".into(),
        name: name.into(),
        targets: targets.clone(),
        source_line: 0,
    };
    let output = |class: &str, name: &str, params: Vec<ParamSpec>| OutputEventDef {
        class: class.into(),
        name: name.into(),
        params,
        append_client: true,
        source_line: 0,
    };
    EventTables {
        inputs: vec![input("onActivate"), input("onPlayerTouch")],
        outputs: vec![
            output(
                "fxDTSBrick",
                "setColor",
                vec![ParamSpec::PaintColor { default: 0 }],
            ),
            output(
                "fxDTSBrick",
                "setColorFX",
                vec![ParamSpec::List {
                    items: vec![("None".into(), 0), ("Pearl".into(), 1), ("Glow".into(), 2)],
                }],
            ),
            output("Player", "Kill", vec![]),
        ],
    }
}

/// [`add_dialogs`] and [`add_screens`] in an in-memory pack (see
/// [`super::pack`]).
pub fn screens_pack() -> Rc<Pack> {
    let mut data = UiPack::default();
    add_dialogs(&mut data);
    add_screens(&mut data);
    super::pack(data)
}

/// The converted pack `content/<id>` (for the content variants of tests
/// that also run on [`screens_pack`]).
pub fn content_pack(id: &str) -> Rc<Pack> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content")
        .join(id);
    Rc::new(Pack::load(&dir).unwrap_or_else(|e| panic!("loading {}: {e:#}", dir.display())))
}

// ------------------------------------------------------------ builders

fn sized(mut c: Control, h: HSizing, v: VSizing) -> Control {
    c.h_sizing = h;
    c.v_sizing = v;
    c
}

fn edit(name: &str, r: Rect) -> Control {
    named(ctrl("GuiTextEditCtrl", "BlockTextEditProfile", r), name)
}

/// An unnamed text box bound to `variable`.
fn var_edit(variable: &str, r: Rect) -> Control {
    let mut c = ctrl("GuiTextEditCtrl", "BlockTextEditProfile", r);
    c.variable = Some(variable.into());
    c
}

fn checkbox(r: Rect, text: &str) -> Control {
    let mut c = ctrl("GuiCheckBoxCtrl", "GuiCheckBoxProfile", r);
    c.text = Some(text.into());
    c
}

fn var_check(variable: &str, r: Rect, text: &str) -> Control {
    let mut c = checkbox(r, text);
    c.variable = Some(variable.into());
    c
}

fn radio(name: &str, group: i32, r: Rect, text: &str, command: &str) -> Control {
    let mut c = named(ctrl("GuiRadioCtrl", "GuiCheckBoxProfile", r), name);
    c.group = Some(group);
    c.text = Some(text.into());
    if !command.is_empty() {
        c.command = Some(command.into());
    }
    c
}

fn popup(name: &str, r: Rect) -> Control {
    named(ctrl("GuiPopUpMenuCtrl", "GuiPopUpMenuProfile", r), name)
}

fn ml(name: &str, r: Rect) -> Control {
    named(ctrl("GuiMLTextCtrl", "GuiMLTextProfile", r), name)
}

/// A scroll box at `r` holding the text list `list` (one row tall; it
/// grows with its rows).
fn scrolled_list(list: &str, r: Rect, style: &str) -> Control {
    let mut scroll = ctrl("GuiScrollCtrl", "BlockScrollProfile", r);
    scroll
        .fields
        .insert("hScrollBar".into(), "alwaysOff".into());
    scroll.fields.insert("vScrollBar".into(), "dynamic".into());
    scroll.children.push(named(
        ctrl("GuiTextListCtrl", style, Rect::new(0, 0, r.w - 18, 16)),
        list,
    ));
    scroll
}

/// A full-screen layout holding `children` directly (no window).
fn screen(children: Vec<Control>) -> Control {
    let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
    root.children = children;
    root
}

/// The window of a [`dialog`] layout.
fn window_of(layout: &mut Control) -> &mut Control {
    &mut layout.children[0]
}

// ------------------------------------------------------------- screens

/// Adds made-up layouts for the main menu, Start Game (with Advanced
/// Config and Music Files), Join Server, Connect to IP, the escape menu,
/// Options, Default Controls, About, Help, the colour warning, chat
/// input, Avatar, Choose Name, the brick selector, the console, the
/// mini-game list and editor, the wrench dialogs, the admin dialogs (with
/// the brick manager), the trust invitation and the play screen's print
/// dialogs;
/// the help pages' fonts, the chat profiles and [`event_tables`]. Layouts
/// `data` already has are replaced.
pub fn add_screens(data: &mut UiPack) {
    for (face, size) in HELP_FONTS {
        data.fonts
            .insert(format!("{face}_{size}"), font(face, size));
    }
    let mut outlined = style(FONT);
    outlined.font_outline = Some([0, 0, 0, 255]);
    data.styles.insert(SERVER_LIST_PROFILE.into(), outlined);
    for size in 0..=10 {
        for profile in [
            "BlockChatTextSize",
            "HUDChatTextEditSize",
            "BlockChatChannelSize",
        ] {
            data.styles
                .insert(format!("{profile}{size}Profile"), style(FONT));
        }
    }
    data.data.event_tables = event_tables();
    data.data.help = ["0. Credits", "1. Controls"]
        .map(|name| HelpPage {
            name: name.into(),
            text: format!("{name}: made-up help text."),
        })
        .to_vec();
    for (name, layout) in [
        ("MainMenuGui", main_menu()),
        ("startMissionGui", start_mission()),
        ("serverConfigGui", server_config()),
        ("MusicFilesGui", music_files()),
        ("JoinServerGui", join_server()),
        ("ManualJoin", manual_join()),
        ("escapeMenu", escape_menu()),
        ("defaultControlsGui", default_controls()),
        ("aboutDlg", about()),
        ("HelpDlg", help()),
        ("LoadBricksColorGui", color_warning()),
        ("newMessageHud", message_hud()),
        ("optionsDlg", options()),
        ("AvatarGui", avatar()),
        ("regNameGui", choose_name()),
        ("BrickSelectorDlg", brick_selector()),
        ("ConsoleDlg", console_layout()),
        ("joinMiniGameGui", join_minigame()),
        ("CreateMiniGameGui", create_minigame()),
        ("AdminLoginGui", admin_login()),
        ("adminGui", admin()),
        ("addBanGui", add_ban()),
        ("unBanGui", unban()),
        ("PlayGui", play()),
        ("BrickManGui", brick_manager()),
        ("TrustInviteGui", trust_invite()),
        ("changeMapGui", change_map()),
        ("wrenchEventsDlg", wrench_events()),
    ] {
        data.layouts.insert(name.into(), layout);
    }
    for (layout, prefix, fields) in WRENCHES {
        data.layouts
            .insert(layout.into(), wrench(layout, prefix, fields));
    }
}

fn main_menu() -> Control {
    let mut children = vec![named(
        crate::screens::text("GuiTextProfile", Rect::new(440, 455, 190, 18), ""),
        "MM_Version",
    )];
    // Each button carries the stock control name (its hover note keys on it).
    for (i, (name, text, command)) in [
        (
            "MM_StartButton",
            "Start Game",
            "canvas.pushDialog(startMissionGui);",
        ),
        (
            "MM_JoinButton",
            "Join Game",
            "canvas.pushDialog(JoinServerGui);",
        ),
        ("MM_PlayerButton", "Avatar", "canvas.pushDialog(AvatarGui);"),
        (
            "MM_OptionsButton",
            "Options",
            "canvas.pushDialog(optionsDlg);",
        ),
        ("MM_TutorialButton", "Tutorial", "MM_Tutorial();"),
        ("MM_CreditsButton", "Credits", "getHelp(\"1. Credits\");"),
        ("MM_AboutButton", "About", "canvas.pushDialog(aboutDlg);"),
        ("MM_QuitButton", "Quit", "quit();"),
    ]
    .into_iter()
    .enumerate()
    {
        children.push(named(
            text_button(Rect::new(20, 120 + i as i32 * 36, 140, 30), text, command),
            name,
        ));
    }
    screen(children)
}

fn start_mission() -> Control {
    let radio_row = |name: &str, x: i32, text: &str, command: &str| {
        radio(name, 1, Rect::new(x, 236, 110, 18), text, command)
    };
    dialog(
        "SM_Window",
        "Start Game",
        Rect::new(20, 20, 600, 440),
        vec![
            scrolled_list(
                "SM_missionList",
                Rect::new(10, 30, 250, 190),
                "GuiTextListProfile",
            ),
            named(
                crate::screens::text("GuiTextProfile", Rect::new(275, 30, 300, 18), ""),
                "SM_MapName",
            ),
            ml("SM_MapDescription", Rect::new(275, 52, 300, 60)),
            named(
                ctrl(
                    "GuiBitmapCtrl",
                    "GuiDefaultProfile",
                    Rect::new(275, 116, 160, 104),
                ),
                "SM_MapPreview",
            ),
            radio_row(
                "SM_OptSinglePlayer",
                10,
                "Single Player",
                "startMissionGui.clickSinglePlayer();",
            ),
            radio_row("SM_OptLAN", 130, "LAN", "startMissionGui.clickLAN();"),
            radio_row(
                "SM_OptInternet",
                250,
                "Internet",
                "startMissionGui.clickInternet();",
            ),
            label(Rect::new(10, 264, 120, 18), "Server Name:"),
            {
                let mut c = edit("TxtServerName", Rect::new(140, 264, 200, 18));
                c.variable = Some("$Pref::Server::Name".into());
                c
            },
            label(Rect::new(10, 288, 120, 18), "Admin Code:"),
            {
                let mut c = edit("TxtServerAdminPasswordCRAP", Rect::new(140, 288, 200, 18));
                c.variable = Some("$Pref::Server::AdminPassword".into());
                c
            },
            label(Rect::new(10, 312, 120, 18), "Super Admin Code:"),
            var_edit(
                "$Pref::Server::SuperAdminPassword",
                Rect::new(140, 312, 200, 18),
            ),
            label(Rect::new(10, 336, 120, 18), "Players:"),
            popup("SM_PlayerCountMenu", Rect::new(140, 336, 80, 18)),
            // Over the server options while single player is picked.
            named(
                swatch(Rect::new(5, 260, 345, 100), [40, 40, 40, 96]),
                "SM_OptionsBlocker",
            ),
            text_button(
                Rect::new(10, 396, 90, 24),
                "Advanced",
                "canvas.pushDialog(ServerconfigGui);",
            ),
            text_button(
                Rect::new(105, 396, 90, 24),
                "Music",
                "canvas.pushDialog(MusicFilesGui);",
            ),
            text_button(
                Rect::new(200, 396, 80, 24),
                "Add-Ons",
                "canvas.pushDialog(AddOnsGui);",
            ),
            text_button(
                Rect::new(450, 396, 65, 28),
                "Back",
                "canvas.popDialog(startMissionGui);",
            ),
            text_button(Rect::new(520, 396, 70, 28), "Start", "SM_StartMission();"),
        ],
    )
}

/// The Advanced Config fields: one per host option, bound to its
/// `$Pref::Server::` preference (a checkbox for the on/off ones).
fn server_config() -> Control {
    let options = option_pairs(&options_from_prefs(&Prefs::default()));
    let mut children = vec![];
    for (i, (key, value)) in options.iter().enumerate() {
        let (col, row) = ((i % 2) as i32, (i / 2) as i32);
        let (x, y) = (10 + col * 240, 30 + row * 24);
        let variable = format!("$Pref::Server::{key}");
        if SWITCHES.contains(key) {
            children.push(var_check(&variable, Rect::new(x, y, 220, 18), key));
        } else {
            debug_assert!(value.parse::<f64>().is_ok(), "{key} is a number");
            children.push(label(Rect::new(x, y, 150, 18), key));
            children.push(var_edit(&variable, Rect::new(x + 155, y, 70, 18)));
        }
    }
    children.push(text_button(
        Rect::new(10, 370, 100, 28),
        "Defaults",
        "ServerConfigGui.clickDefaults();",
    ));
    children.push(text_button(
        Rect::new(380, 370, 100, 28),
        "Done",
        "canvas.popDialog(ServerConfigGui);",
    ));
    dialog(
        "ServerConfig_Window",
        "Advanced Config",
        Rect::new(75, 30, 490, 410),
        children,
    )
}

/// Host options shown as checkboxes.
const SWITCHES: [&str; 3] = ["RandomBrickColor", "ETardFilter", "FallingDamage"];

fn music_files() -> Control {
    let mut scroll = ctrl(
        "GuiScrollCtrl",
        "BlockScrollProfile",
        Rect::new(10, 30, 280, 250),
    );
    scroll.name = Some("MFG_Scroll".into());
    dialog(
        "MFG_Window",
        "Music Files",
        Rect::new(170, 70, 300, 330),
        vec![
            scroll,
            text_button(
                Rect::new(10, 290, 80, 28),
                "None",
                "MusicFilesGui.clickNone();",
            ),
            text_button(
                Rect::new(95, 290, 90, 28),
                "Defaults",
                "MusicFilesGui.clickDefaults();",
            ),
            text_button(
                Rect::new(210, 290, 80, 28),
                "Done",
                "canvas.popDialog(MusicFilesGui);",
            ),
        ],
    )
}

fn join_server() -> Control {
    let mut children = vec![];
    for (i, (text, command)) in [
        ("Name", "JS_sortList(10);"),
        ("Ping", "JS_sortNumList(3);"),
        ("Players", "JS_sortNumList(4, 1);"),
        ("Map", "JS_sortList(8);"),
    ]
    .into_iter()
    .enumerate()
    {
        children.push(text_button(
            Rect::new(10 + i as i32 * 100, 28, 96, 18),
            text,
            command,
        ));
    }
    let mut list = scrolled_list(
        "JS_serverList",
        Rect::new(10, 48, 480, 250),
        SERVER_LIST_PROFILE,
    );
    list.children[0].h_sizing = HSizing::Width;
    list.children[0]
        .fields
        .insert("columns".into(), "0 30 60 220 260 290 300 330 370".into());
    children.push(sized(list, HSizing::Width, VSizing::Height));
    let bottom = |c: Control| sized(c, HSizing::Right, VSizing::Top);
    children.extend([
        bottom(named(
            crate::screens::text("GuiTextProfile", Rect::new(10, 304, 200, 18), ""),
            "JS_statusText",
        )),
        bottom(named(
            ctrl(
                "GuiControl",
                "GuiDefaultProfile",
                Rect::new(215, 304, 60, 18),
            ),
            "JS_queryStatus",
        )),
        bottom(text_button(
            Rect::new(10, 326, 90, 28),
            "Back",
            "canvas.popDialog(JoinServerGui);",
        )),
        bottom(text_button(
            Rect::new(105, 326, 90, 28),
            "Query LAN",
            "JoinServerGui.queryLan();",
        )),
        bottom(text_button(
            Rect::new(200, 326, 90, 28),
            "Favorite",
            "JoinServerGui.queryWebMaster();",
        )),
        bottom(text_button(
            Rect::new(295, 326, 100, 28),
            "Connect to IP",
            "canvas.pushDialog(\"ManualJoin\");",
        )),
        bottom(text_button(
            Rect::new(400, 326, 90, 28),
            "Join",
            "JoinServerGui.join();",
        )),
    ]);
    let mut layout = dialog(
        "JS_Window",
        "Join Server",
        Rect::new(70, 60, 500, 362),
        children,
    );
    let window = window_of(&mut layout);
    for key in ["resizeWidth", "resizeHeight", "canMaximize"] {
        window.fields.insert(key.into(), "1".into());
    }
    layout
}

fn manual_join() -> Control {
    dialog(
        "MJ_Window",
        "Connect to IP",
        Rect::new(170, 180, 300, 110),
        vec![
            label(Rect::new(10, 32, 70, 18), "Address:"),
            edit("MJ_txtIP", Rect::new(85, 32, 200, 18)),
            text_button(
                Rect::new(10, 70, 90, 28),
                "Cancel",
                "canvas.popDialog(manualJoin);",
            ),
            text_button(Rect::new(195, 70, 90, 28), "Connect", "MJ_connect();"),
        ],
    )
}

fn escape_menu() -> Control {
    let mut children = vec![];
    for (i, (name, text, command)) in [
        ("EM_Save", "Save Bricks", "escapeMenu::clickSaveBricks();"),
        ("EM_Load", "Load Bricks", "escapeMenu::clickLoadBricks();"),
        ("EM_Admin", "Admin", "escapeMenu::clickAdmin();"),
        (
            "EM_Players",
            "Players",
            "canvas.pushDialog(NewPlayerListGui);canvas.popDialog(escapeMenu);",
        ),
        (
            "EM_MiniGames",
            "Mini-Games",
            "escapeMenu::clickMiniGames();",
        ),
        ("EM_Options", "Options", "canvas.pushDialog(optionsDlg);"),
        ("EM_Leave", "Leave Game", "escapeFromGame();"),
        ("EM_Quit", "Quit", "quitGame();"),
    ]
    .into_iter()
    .enumerate()
    {
        children.push(named(
            text_button(Rect::new(20, 30 + i as i32 * 32, 160, 28), text, command),
            name,
        ));
    }
    dialog("EM_Window", "Menu", Rect::new(220, 100, 200, 296), children)
}

fn default_controls() -> Control {
    let mut children = vec![];
    for i in 0..4 {
        children.push(radio(
            &format!("OPT_Mouse{i}"),
            1,
            Rect::new(10, 30 + i * 22, 140, 18),
            &format!("Mouse layout {i}"),
            "",
        ));
    }
    for i in 0..2 {
        children.push(radio(
            &format!("OPT_Keyboard{i}"),
            2,
            Rect::new(160, 30 + i * 22, 130, 18),
            &format!("Keyboard layout {i}"),
            "",
        ));
    }
    children.push(text_button(
        Rect::new(10, 130, 90, 28),
        "Cancel",
        "canvas.popDialog(defaultControlsGui);",
    ));
    children.push(named(
        ctrl(
            "GuiControl",
            "GuiDefaultProfile",
            Rect::new(5, 125, 100, 38),
        ),
        "DefaultControls_CancelBlocker",
    ));
    children.push(text_button(
        Rect::new(200, 130, 90, 28),
        "OK",
        "DefaultControlsGui.apply();",
    ));
    dialog(
        "DefaultControls_Window",
        "Controls",
        Rect::new(170, 150, 300, 170),
        children,
    )
}

fn about() -> Control {
    dialog(
        "About_Window",
        "About",
        Rect::new(170, 140, 300, 200),
        vec![
            ml("aboutText", Rect::new(10, 30, 280, 120)),
            text_button(
                Rect::new(105, 160, 90, 28),
                "OK",
                "canvas.popDialog(aboutDlg);",
            ),
        ],
    )
}

fn help() -> Control {
    let mut text = ctrl(
        "GuiScrollCtrl",
        "BlockScrollProfile",
        Rect::new(170, 30, 340, 330),
    );
    text.children.push(ml("HelpText", Rect::new(0, 0, 320, 16)));
    let mut layout = dialog(
        "HelpDlg_Window",
        "Help",
        Rect::new(60, 50, 520, 370),
        vec![
            scrolled_list(
                "HelpFileList",
                Rect::new(10, 30, 150, 330),
                "GuiTextListProfile",
            ),
            text,
        ],
    );
    window_of(&mut layout)
        .fields
        .insert("canMinimize".into(), "1".into());
    // Authored above the window; the window's close box closes it.
    layout.children.push(text_button(
        Rect::new(540, 20, 40, 24),
        "X",
        "Canvas.popDialog(HelpDlg);",
    ));
    layout
}

fn color_warning() -> Control {
    let mut children = vec![ml("ColorWarning_Text", Rect::new(10, 30, 280, 30))];
    for (i, (text, command)) in [
        ("Nearest Match", "ColorWarning_ClickMatch();"),
        ("Replace Colors", "ColorWarning_ClickReplace();"),
        ("Add More Colors", "ColorWarning_ClickAppend();"),
        ("Cancel", "ColorWarning_ClickCancel();"),
    ]
    .into_iter()
    .enumerate()
    {
        children.push(text_button(
            Rect::new(50, 66 + i as i32 * 36, 200, 30),
            text,
            command,
        ));
    }
    dialog(
        "ColorWarning_Window",
        "Colors Differ",
        Rect::new(170, 120, 300, 220),
        children,
    )
}

fn message_hud() -> Control {
    let mut bx = named(
        ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 20)),
        "NMH_Box",
    );
    bx.h_sizing = HSizing::Width;
    bx.children = vec![
        named(
            crate::screens::text("BlockChatChannelSize4Profile", Rect::new(2, 0, 40, 18), ""),
            "NMH_Channel",
        ),
        named(
            ctrl(
                "GuiTextEditCtrl",
                "HUDChatTextEditSize4Profile",
                Rect::new(44, 0, 590, 18),
            ),
            "NMH_Type",
        ),
    ];
    screen(vec![bx])
}

/// An option section: a swatch with a title bar swatch at (2, 2) and its
/// title, holding `children` below.
fn section(title: &str, r: Rect, children: Vec<Control>) -> Control {
    let mut s = swatch(r, [200, 200, 200, 255]);
    s.children
        .push(swatch(Rect::new(2, 2, r.w - 4, 14), [90, 90, 140, 255]));
    s.children.push(crate::screens::text(
        "GuiTextProfile",
        Rect::new(4, 0, 150, 14),
        title,
    ));
    s.children.extend(children);
    s
}

fn pane(name: &str, visible: bool, children: Vec<Control>) -> Control {
    let mut p = named(
        ctrl(
            "GuiControl",
            "GuiDefaultProfile",
            Rect::new(5, 50, 610, 370),
        ),
        name,
    );
    p.visible = visible;
    p.children = children;
    p
}

fn slider(name: &str, r: Rect) -> Control {
    let mut c = named(ctrl("GuiSliderCtrl", "GuiDefaultProfile", r), name);
    c.fields.insert("range".into(), "0 1".into());
    c
}

fn options() -> Control {
    let graphics = pane(
        "OptGraphicsPane",
        true,
        vec![
            section(
                "Display Settings",
                Rect::new(0, 0, 300, 120),
                vec![
                    label(Rect::new(10, 24, 70, 18), "Resolution:"),
                    popup("OptGraphicsResolutionMenu", Rect::new(80, 24, 90, 18)),
                    var_check(
                        "$pref::Video::fullScreen",
                        Rect::new(185, 24, 100, 18),
                        "Fullscreen",
                    ),
                    var_check(
                        "$pref::Video::disableVerticalSync",
                        Rect::new(185, 46, 100, 18),
                        "No VSync",
                    ),
                ],
            ),
            section(
                "Gui Settings",
                Rect::new(305, 0, 300, 120),
                vec![
                    var_check(
                        "$pref::HUD::HidePaintBox",
                        Rect::new(10, 24, 200, 18),
                        "Hide paint box",
                    ),
                    var_check(
                        "$pref::HUD::HideToolBox",
                        Rect::new(10, 46, 200, 18),
                        "Hide tool box",
                    ),
                    var_check(
                        "$pref::precipitationOn",
                        Rect::new(10, 68, 200, 18),
                        "Precipitation",
                    ),
                ],
            ),
            section(
                "Shadow Quality",
                Rect::new(0, 125, 145, 100),
                (0..4)
                    .map(|i| {
                        radio(
                            &format!("OPT_ShadowQuality{i}"),
                            1,
                            Rect::new(10, 22 + i * 18, 120, 16),
                            &format!("Shadows {i}"),
                            &format!("optionsDlg.setShadowQuality({i});"),
                        )
                    })
                    .collect(),
            ),
            section(
                "Physics Quality",
                Rect::new(150, 125, 150, 110),
                (0..5)
                    .map(|i| {
                        radio(
                            &format!("OPT_PhysicsQuality{i}"),
                            2,
                            Rect::new(10, 22 + i * 17, 120, 16),
                            &format!("Physics {i}"),
                            &format!("optionsDlg.setPhysicsQuality({i});"),
                        )
                    })
                    .collect(),
            ),
        ],
    );
    let volume_row = |i: i32, name: &str, text: &str| {
        [
            label(Rect::new(10, 24 + i * 30, 110, 18), text),
            slider(name, Rect::new(125, 24 + i * 30, 160, 18)),
        ]
    };
    let audio = pane(
        "OptAudioPane",
        false,
        vec![
            section(
                "Volume",
                Rect::new(0, 0, 300, 200),
                [
                    volume_row(0, "OptAudioVolumeMaster", "Master Volume:"),
                    volume_row(1, "OptAudioVolumeShell", "Shell Volume:"),
                    volume_row(2, "OptAudioVolumeSim", "Sim Volume:"),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
            section(
                "Audio Options",
                Rect::new(305, 0, 300, 200),
                [
                    ("$Pref::Audio::PlayMusic", "Play music"),
                    ("$Pref::Audio::MenuSounds", "Menu sounds"),
                    ("$Pref::Audio::PlayBrickPlantSound", "Brick plant sound"),
                    ("$Pref::Audio::PlayBrickMoveSound", "Brick move sound"),
                ]
                .iter()
                .enumerate()
                .map(|(i, (var, text))| {
                    var_check(var, Rect::new(10, 24 + i as i32 * 22, 220, 18), text)
                })
                .collect(),
            ),
        ],
    );
    let controls = pane(
        "OptControlsPane",
        false,
        vec![
            section(
                "Mouse",
                Rect::new(0, 0, 300, 90),
                vec![
                    label(Rect::new(10, 24, 100, 18), "Sensitivity:"),
                    slider(
                        "SliderControlsMouseSensitivity",
                        Rect::new(115, 24, 170, 18),
                    ),
                    var_check(
                        "$pref::Input::MouseInvert",
                        Rect::new(10, 50, 200, 18),
                        "Invert mouse",
                    ),
                ],
            ),
            section(
                "Options",
                Rect::new(0, 95, 300, 120),
                vec![
                    var_check(
                        "$pref::Input::noobjet",
                        Rect::new(10, 24, 200, 18),
                        "Jet with jump",
                    ),
                    var_check(
                        "$Pref::Input::VehicleMouseInvert",
                        Rect::new(10, 46, 220, 18),
                        "Invert mouse in vehicles",
                    ),
                ],
            ),
            section(
                "Keys",
                Rect::new(305, 0, 300, 340),
                vec![scrolled_list(
                    "OptRemapList",
                    Rect::new(6, 22, 288, 310),
                    "GuiTextListProfile",
                )],
            ),
        ],
    );
    let mut page = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 580, 300));
    page.children = vec![
        section(
            "Gui Options",
            Rect::new(0, 0, 580, 70),
            vec![
                var_check(
                    "$pref::HUD::showToolTips",
                    Rect::new(10, 24, 200, 18),
                    "Show tooltips",
                ),
                var_check(
                    "$Pref::Gui::ColorEscapeMenu",
                    Rect::new(10, 46, 200, 18),
                    "Colour menu",
                ),
            ],
        ),
        section(
            "Chat",
            Rect::new(0, 75, 580, 80),
            vec![
                label(Rect::new(10, 24, 110, 18), "Line time:"),
                edit("Opt_ChatLineTime", Rect::new(125, 24, 70, 18)),
                label(Rect::new(10, 48, 110, 18), "Lines shown:"),
                edit("Opt_MaxChatLines", Rect::new(125, 48, 70, 18)),
            ],
        ),
    ];
    let mut scroll = ctrl(
        "GuiScrollCtrl",
        "BlockScrollProfile",
        Rect::new(0, 0, 605, 340),
    );
    scroll.children.push(page);
    let advanced = pane("OptAdvGraphicsPane", false, vec![scroll]);
    let mut children = vec![];
    for (i, pane) in ["Graphics", "Audio", "Controls", "AdvGraphics"]
        .into_iter()
        .enumerate()
    {
        children.push(text_button(
            Rect::new(12 + i as i32 * 90, 26, 86, 22),
            pane,
            &format!("optionsDlg.setPane({pane});"),
        ));
    }
    children.extend([graphics, audio, controls, advanced]);
    let mut done = named(
        text_button(
            Rect::new(515, 425, 90, 28),
            "Done",
            "Canvas.popDialog(optionsDlg);",
        ),
        "done",
    );
    done.accelerator = Some("escape".into());
    children.push(done);
    dialog(
        "OPT_Window",
        "Options",
        Rect::new(10, 10, 620, 460),
        children,
    )
}

fn avatar() -> Control {
    let mut children = vec![named(
        ctrl(
            "GuiObjectView",
            "GuiDefaultProfile",
            Rect::new(300, 30, 230, 330),
        ),
        "Avatar_Preview",
    )];
    for (i, (name, text)) in [
        ("Avatar_Name", "Name:"),
        ("Avatar_Prefix", "Clan prefix:"),
        ("Avatar_Suffix", "Clan suffix:"),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 30 + i as i32 * 26;
        children.push(label(Rect::new(10, y, 90, 18), text));
        children.push(edit(name, Rect::new(105, y, 170, 18)));
    }
    children.push(named(
        checkbox(Rect::new(10, 110, 150, 18), "Symmetry"),
        "Avatar_SymmetryCheckbox",
    ));
    children.push(text_button(
        Rect::new(440, 370, 90, 28),
        "Done",
        "Avatar_Done();",
    ));
    dialog(
        "Avatar_Window",
        "Avatar",
        Rect::new(50, 30, 540, 410),
        children,
    )
}

fn choose_name() -> Control {
    dialog(
        "regName_Window",
        "Register Name",
        Rect::new(170, 180, 300, 110),
        vec![
            label(Rect::new(10, 32, 80, 18), "New Name:"),
            edit("regName_NewName", Rect::new(95, 32, 190, 18)),
            text_button(
                Rect::new(10, 70, 90, 28),
                "<< Cancel",
                "canvas.popDialog(regNameGui);",
            ),
            text_button(
                Rect::new(195, 70, 90, 28),
                "Register >>",
                "regNameGui::register();",
            ),
        ],
    )
}

fn brick_selector() -> Control {
    dialog("BSD_Window", "Bricks", Rect::new(0, 0, 640, 480), vec![])
}

/// ConsoleDlg: the log in a scroll box over a one-line entry, in a
/// window.
pub fn console_layout() -> Control {
    let mut log = ctrl("GuiConsole", "GuiConsoleProfile", Rect::new(1, 1, 8, 2));
    log.name = Some("testArrayCtrl".into());
    let mut scroll = ctrl(
        "GuiScrollCtrl",
        "GuiScrollProfile",
        Rect::new(0, 0, 640, 350),
    );
    scroll.children = vec![log];
    let mut entry = ctrl(
        "GuiConsoleEditCtrl",
        "GuiTextEditProfile",
        Rect::new(0, 350, 640, 18),
    );
    entry.name = Some("ConsoleEntry".into());
    let mut window = ctrl(
        "GuiWindowCtrl",
        "GuiWindowProfile",
        Rect::new(0, 0, 640, 370),
    );
    window.text = Some("Console".into());
    window.children = vec![scroll, entry];
    screen(vec![window])
}

fn join_minigame() -> Control {
    let mut list = scrolled_list(
        "JMG_List",
        Rect::new(10, 30, 400, 220),
        "GuiTextListProfile",
    );
    list.children[0].command = Some("JoinMiniGameGui.clickList();".into());
    dialog(
        "JMG_Window",
        "Mini-Games",
        Rect::new(110, 90, 420, 300),
        vec![
            list,
            text_button(
                Rect::new(10, 260, 90, 28),
                "Leave",
                "JoinMiniGameGui.clickLeave();",
            ),
            text_button(
                Rect::new(110, 260, 90, 28),
                "Create",
                "JoinMiniGameGui.clickCreate();",
            ),
            text_button(
                Rect::new(210, 260, 90, 28),
                "Back",
                "canvas.popDialog(JoinMiniGameGui);",
            ),
            text_button(
                Rect::new(320, 260, 90, 28),
                "Join",
                "JoinMiniGameGui.clickJoin();",
            ),
        ],
    )
}

fn create_minigame() -> Control {
    let mut children = vec![
        label(Rect::new(10, 30, 60, 18), "Title:"),
        var_edit("$MiniGame::Title", Rect::new(75, 30, 200, 18)),
        label(Rect::new(290, 30, 50, 18), "Color:"),
        {
            let mut c = popup("CMG_ColorList", Rect::new(345, 30, 100, 18));
            c.command = Some("CreateMiniGameGui.clickColorList();".into());
            c
        },
        named(swatch(Rect::new(450, 30, 18, 18), [255; 4]), "CMG_Swatch"),
    ];
    for (i, var) in [
        "$MiniGame::Points::BreakBrick",
        "$MiniGame::Points::PlantBrick",
        "$MiniGame::Points::KillPlayer",
        "$MiniGame::Points::KillSelf",
        "$MiniGame::Points::Die",
        "$MiniGame::RespawnTime",
        "$MiniGame::VehicleRespawnTime",
        "$MiniGame::BrickRespawnTime",
    ]
    .into_iter()
    .enumerate()
    {
        let y = 56 + i as i32 * 22;
        children.push(label(Rect::new(10, y, 170, 18), var));
        children.push(var_edit(var, Rect::new(185, y, 50, 18)));
    }
    for (i, var) in [
        "$MiniGame::InviteOnly",
        "$MiniGame::UseAllPlayersBricks",
        "$MiniGame::PlayersUseOwnBricks",
        "$MiniGame::UseSpawnBricks",
        "$MiniGame::FallingDamage",
        "$MiniGame::WeaponDamage",
        "$MiniGame::SelfDamage",
        "$MiniGame::VehicleDamage",
        "$MiniGame::BrickDamage",
        "$MiniGame::EnableWand",
        "$MiniGame::EnableBuilding",
        "$MiniGame::EnablePainting",
    ]
    .into_iter()
    .enumerate()
    {
        let y = 56 + i as i32 * 22;
        children.push(var_check(var, Rect::new(255, y, 200, 18), var));
    }
    children.push(label(Rect::new(10, 240, 90, 18), "Player type:"));
    children.push(popup("CMG_PlayerDataBlock", Rect::new(105, 240, 130, 18)));
    for i in 0..5 {
        children.push(popup(
            &format!("CMG_StartEquip{i}"),
            Rect::new(10 + (i % 3) * 120, 322 + (i / 3) * 24, 110, 18),
        ));
    }
    children.extend([
        text_button(
            Rect::new(10, 380, 80, 28),
            "Reset",
            "CreateMiniGameGui.clickReset();",
        ),
        text_button(
            Rect::new(95, 380, 80, 28),
            "End",
            "CreateMiniGameGui.clickEnd();",
        ),
        named(
            ctrl(
                "GuiControl",
                "GuiDefaultProfile",
                Rect::new(95, 380, 80, 28),
            ),
            "CMG_EndBlocker",
        ),
        text_button(
            Rect::new(270, 380, 80, 28),
            "Back",
            "canvas.popDialog(CreateMiniGameGui);",
        ),
        named(
            text_button(
                Rect::new(360, 380, 100, 28),
                "Create >>",
                "CreateMiniGameGui.clickCreate();",
            ),
            "CMG_CreateButton",
        ),
    ]);
    dialog(
        "CMG_Window",
        "Create Mini-Game",
        Rect::new(85, 25, 470, 420),
        children,
    )
}

fn admin_login() -> Control {
    let mut pass = edit("txtAdminPass", Rect::new(90, 32, 195, 18));
    pass.fields.insert("password".into(), "1".into());
    pass.alt_command = Some("SAD(txtAdminPass.getValue());".into());
    dialog(
        "AdminLogin_Window",
        "Admin Login",
        Rect::new(170, 180, 300, 100),
        vec![
            label(Rect::new(10, 32, 75, 18), "Password:"),
            pass,
            text_button(
                Rect::new(10, 62, 90, 28),
                "Cancel",
                "canvas.popDialog(AdminLoginGui);",
            ),
            text_button(
                Rect::new(195, 62, 90, 28),
                "Login",
                "SAD(txtAdminPass.getValue());",
            ),
        ],
    )
}

fn admin() -> Control {
    let mut children = vec![
        text_button(
            Rect::new(14, 32, 90, 18),
            "Name",
            "sortList(lstAdminPlayerList, 0);",
        ),
        text_button(
            Rect::new(108, 32, 90, 18),
            "Identity",
            "sortList(lstAdminPlayerList, 1);",
        ),
        {
            let mut s = scrolled_list(
                "lstAdminPlayerList",
                Rect::new(14, 52, 186, 360),
                "GuiTextListProfile",
            );
            s.children[0]
                .fields
                .insert("columns".into(), "0 100".into());
            s
        },
    ];
    for (i, (text, command)) in [
        ("Kick", "AdminGui_KickPlayer();"),
        ("Ban", "AdminGui_BanPlayer();"),
        ("Spy", "adminGui::spy();"),
        ("Wand", "AdminGui_Wand();"),
        ("Un-Ban", "canvas.pushDialog(unBanGui);"),
        ("Change Map", "canvas.pushdialog(changeMapGui);"),
        ("Clear Bricks", "AdminGui.ClickClearBricks();"),
    ]
    .into_iter()
    .enumerate()
    {
        let (col, row) = ((i % 2) as i32, (i / 2) as i32);
        children.push(text_button(
            Rect::new(310 + col * 100, 52 + row * 34, 96, 28),
            text,
            command,
        ));
    }
    children.push(text_button(
        Rect::new(410, 425, 96, 28),
        "Close",
        "canvas.popDialog(adminGui);",
    ));
    dialog(
        "adminGui_Window",
        "Admin",
        Rect::new(60, 0, 520, 470),
        children,
    )
}

fn add_ban() -> Control {
    dialog(
        "addBan_Window",
        "BAN",
        Rect::new(150, 150, 340, 170),
        vec![
            popup("AddBan_Days", Rect::new(10, 32, 95, 18)),
            popup("AddBan_Hours", Rect::new(115, 32, 95, 18)),
            popup("AddBan_Minutes", Rect::new(220, 32, 95, 18)),
            named(
                ctrl("GuiControl", "GuiDefaultProfile", Rect::new(5, 28, 315, 26)),
                "AddBan_TimeBlocker",
            ),
            named(
                checkbox(Rect::new(10, 60, 120, 18), "Forever"),
                "AddBan_Forever",
            ),
            label(Rect::new(10, 88, 60, 18), "Reason:"),
            edit("addBan_reason", Rect::new(75, 88, 250, 18)),
            text_button(
                Rect::new(10, 128, 90, 28),
                "Cancel",
                "canvas.popDialog(addBanGui);",
            ),
            text_button(Rect::new(235, 128, 90, 28), "Ban", "addBanGui.ban();"),
        ],
    )
}

/// The play screen: only the center and bottom print dialogs (hidden until
/// a print arrives), each holding its ML text.
fn play() -> Control {
    let print = |dialog: &str, text: &str, r: Rect| {
        let mut d = named(ctrl("GuiControl", "GuiDefaultProfile", r), dialog);
        d.visible = false;
        d.children.push(ml(text, Rect::new(10, 10, r.w - 20, 20)));
        d
    };
    screen(vec![
        print(
            "centerPrintDlg",
            "CenterPrintText",
            Rect::new(80, 150, 480, 100),
        ),
        print(
            "bottomPrintDlg",
            "BottomPrintText",
            Rect::new(80, 400, 480, 60),
        ),
    ])
}

/// Admin's brick manager: the owners list and its Clear, Highlight and
/// Clear All buttons.
fn brick_manager() -> Control {
    let mut children = vec![scrolled_list(
        "BrickMan_list",
        Rect::new(10, 30, 560, 290),
        "GuiTextListProfile",
    )];
    for (i, (text, command)) in [
        ("Clear", "BrickManGui.clickClear();"),
        ("Highlight", "BrickManGui.clickHilight();"),
        ("Clear All", "BrickManGui.clickClearAll();"),
        ("Close", "canvas.popDialog(BrickManGui);"),
    ]
    .into_iter()
    .enumerate()
    {
        children.push(text_button(
            Rect::new(10 + i as i32 * 110, 330, 100, 28),
            text,
            command,
        ));
    }
    dialog(
        "BrickMan_Window",
        "Bricks",
        Rect::new(30, 40, 580, 370),
        children,
    )
}

/// A trust invitation: who asks, and Accept, Reject and Ignore.
fn trust_invite() -> Control {
    let mut children = vec![];
    for (i, name) in ["TI_Name", "TI_BL_ID"].into_iter().enumerate() {
        children.push(named(
            crate::screens::text(
                "GuiTextProfile",
                Rect::new(10, 30 + i as i32 * 24, 320, 18),
                "",
            ),
            name,
        ));
    }
    for (i, (text, command)) in [
        ("Accept", "TrustInviteGui.clickAccept();"),
        ("Reject", "TrustInviteGui.clickReject();"),
        ("Ignore", "TrustInviteGui.clickIgnore();"),
    ]
    .into_iter()
    .enumerate()
    {
        children.push(text_button(
            Rect::new(10 + i as i32 * 110, 90, 100, 28),
            text,
            command,
        ));
    }
    dialog(
        "TI_Window",
        "Trust Invitation",
        Rect::new(150, 150, 340, 130),
        children,
    )
}

fn unban() -> Control {
    dialog(
        "unBan_Window",
        "Un-Ban",
        Rect::new(20, 60, 600, 330),
        vec![
            scrolled_list(
                "unBan_list",
                Rect::new(10, 30, 580, 240),
                "GuiTextListProfile",
            ),
            text_button(
                Rect::new(10, 280, 90, 28),
                "Close",
                "canvas.popDialog(unBanGui);",
            ),
            text_button(
                Rect::new(500, 280, 90, 28),
                "Un-Ban",
                "unBanGui.clickUnBan();",
            ),
        ],
    )
}

fn change_map() -> Control {
    dialog(
        "changeMap_Window",
        "Change Map",
        Rect::new(120, 80, 400, 300),
        vec![
            scrolled_list(
                "changeMapList",
                Rect::new(10, 30, 180, 220),
                "GuiTextListProfile",
            ),
            named(
                crate::screens::text("GuiTextProfile", Rect::new(200, 30, 190, 18), ""),
                "changeMapName",
            ),
            ml("changeMapDescription", Rect::new(200, 52, 190, 60)),
            text_button(
                Rect::new(10, 260, 90, 28),
                "Back",
                "canvas.popDialog(changeMapGui);",
            ),
            named(
                text_button(
                    Rect::new(300, 260, 90, 28),
                    "Change",
                    "changeMapButton.click();",
                ),
                "changeMapButton",
            ),
        ],
    )
}

/// One wrench field: its suffix, and how it is drawn.
#[derive(Clone, Copy)]
enum Field {
    Text(&'static str),
    Menu(&'static str),
    Check(&'static str),
    /// Radio buttons `<suffix><i>` for each listed `i`.
    Radios(&'static str, &'static [u8]),
}

/// The wrench layouts: name, control prefix, fields.
const WRENCHES: [(&str, &str, &[Field]); 3] = [
    (
        "wrenchDlg",
        "Wrench",
        &[
            Field::Text("Name"),
            Field::Menu("Lights"),
            Field::Menu("Emitters"),
            Field::Radios("EmitterDir", &[0, 1, 2, 3, 4, 5]),
            Field::Menu("Items"),
            Field::Radios("ItemPos", &[0, 1, 2, 3, 4, 5]),
            Field::Radios("ItemDir", &[2, 3, 4, 5]),
            Field::Text("ItemRespawnTime"),
            Field::Check("RayCasting"),
            Field::Check("Collision"),
            Field::Check("Rendering"),
        ],
    ),
    (
        "wrenchSoundDlg",
        "WrenchSound",
        &[Field::Text("Name"), Field::Menu("Sounds")],
    ),
    (
        "wrenchVehicleSpawnDlg",
        "WrenchVehicleSpawn",
        &[
            Field::Text("Name"),
            Field::Menu("Vehicles"),
            Field::Check("ReColorVehicle"),
            Field::Check("RayCasting"),
            Field::Check("Collision"),
            Field::Check("Rendering"),
        ],
    ),
];

/// A wrench window: each field on its own row with its Copy box at the
/// right, and the buttons along the bottom.
fn wrench(layout: &str, prefix: &str, fields: &[Field]) -> Control {
    let mut children = vec![];
    let mut group = 1;
    for (row, &f) in fields.iter().enumerate() {
        let y = 30 + row as i32 * 26;
        let suffix = match f {
            Field::Text(s) | Field::Menu(s) | Field::Check(s) | Field::Radios(s, _) => s,
        };
        match f {
            Field::Text(_) => {
                children.push(label(Rect::new(10, y, 110, 18), suffix));
                children.push(edit(
                    &format!("{prefix}_{suffix}"),
                    Rect::new(125, y, 150, 18),
                ));
            }
            Field::Menu(_) => {
                children.push(label(Rect::new(10, y, 110, 18), suffix));
                children.push(popup(
                    &format!("{prefix}_{suffix}"),
                    Rect::new(125, y, 150, 18),
                ));
            }
            Field::Check(_) => children.push(named(
                checkbox(Rect::new(10, y, 200, 18), suffix),
                &format!("{prefix}_{suffix}"),
            )),
            Field::Radios(_, which) => {
                children.push(label(Rect::new(10, y, 80, 18), suffix));
                for (i, &n) in which.iter().enumerate() {
                    children.push(radio(
                        &format!("{prefix}_{suffix}{n}"),
                        group,
                        Rect::new(95 + i as i32 * 30, y, 26, 18),
                        "",
                        "",
                    ));
                }
                group += 1;
            }
        }
        children.push(named(
            checkbox(Rect::new(300, y, 60, 18), "Copy"),
            &format!("{prefix}Lock_{suffix}"),
        ));
    }
    let y = 40 + fields.len() as i32 * 26;
    for (i, (text, command)) in [
        ("Cancel", format!("canvas.popDialog({layout});")),
        ("Events", "canvas.pushDialog(WrenchEventsDlg);".into()),
        ("Send", format!("{layout}.send();")),
    ]
    .into_iter()
    .enumerate()
    {
        children.push(text_button(
            Rect::new(10 + i as i32 * 120, y, 100, 28),
            text,
            &command,
        ));
    }
    named(
        dialog(
            &format!("{prefix}_Window"),
            "Wrench",
            Rect::new(130, 40, 380, y + 40),
            children,
        ),
        layout,
    )
}

fn wrench_events() -> Control {
    let mut copy = checkbox(Rect::new(0, 0, 42, 18), "Copy");
    copy.name = Some("WrenchLock_Events".into());
    let mut layout = dialog(
        "WrenchEvents_Window",
        "Events",
        Rect::new(0, 0, 640, 480),
        vec![
            text_button(Rect::new(0, 0, 91, 30), "Send", "wrenchEventsDlg.send();"),
            text_button(
                Rect::new(0, 0, 91, 30),
                "Cancel",
                "canvas.popDialog(wrenchEventsDlg);",
            ),
            text_button(Rect::new(0, 0, 91, 30), "Clear", "wrenchEventsDlg.clear();"),
            copy,
        ],
    );
    layout.name = Some("wrenchEventsDlg".into());
    layout
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_option_and_wrench_field_is_named_once() {
        let pack = screens_pack();
        let options = option_pairs(&options_from_prefs(&Prefs::default()));
        let config = &pack.data.layouts["serverConfigGui"];
        for (key, _) in options {
            assert!(
                field_with_variable(config, &format!("$Pref::Server::{key}")),
                "{key}"
            );
        }
    }

    fn field_with_variable(c: &Control, variable: &str) -> bool {
        c.variable.as_deref() == Some(variable)
            || c.children.iter().any(|k| field_with_variable(k, variable))
    }
}
