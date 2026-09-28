//! Every visible authored button on the menu screens a player reaches does
//! something: none answers "Interface under construction". Runs against the
//! converted v20 UI pack, and skips when that local content is absent.
use bri_ui::{
    api::Settings,
    binds::Platform,
    pack::Pack,
    screens::ScreenId,
    ui::{StackCmd, Ui, UiConfig},
    view::{EventKind, View, ViewEvent},
};
use std::{path::Path, rc::Rc};

const SCREENS: [ScreenId; 19] = [
    ScreenId::MainMenu,
    ScreenId::StartMission,
    ScreenId::JoinServer,
    ScreenId::ManualJoin,
    ScreenId::EscapeMenu,
    ScreenId::Options,
    ScreenId::DefaultControls,
    ScreenId::About,
    ScreenId::Avatar,
    ScreenId::SaveBricks,
    ScreenId::LoadBricks,
    ScreenId::PlayerList,
    ScreenId::MiniGames,
    ScreenId::MiniGameSettings,
    ScreenId::BrickSelector,
    ScreenId::Admin,
    ScreenId::AdminBricks,
    ScreenId::AdminUnban,
    ScreenId::AdminMaps,
];
const UNBUILT: &str = "Interface under construction";

fn visible(v: &View, mut n: usize) -> bool {
    loop {
        let node = v.node(n);
        if !node.state.visible || !node.ctrl.visible {
            return false;
        }
        match node.parent {
            Some(p) => n = p,
            None => return true,
        }
    }
}

fn ui(pack: &Rc<Pack>) -> Ui {
    Ui::new(
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
    )
}

/// Commands on `screen` whose click answers "Interface under construction".
fn unbuilt(pack: &Rc<Pack>, screen: ScreenId) -> Vec<String> {
    let mut probe = ui(pack);
    probe.core.push(screen);
    probe.update(0);
    let Some(s) = probe.screen(screen) else {
        return vec![];
    };
    let v = s.view();
    let targets: Vec<(usize, String)> = v
        .walk()
        .filter(|&n| visible(v, n) && v.node(n).state.active)
        .filter_map(|n| v.node(n).ctrl.command.clone().map(|c| (n, c)))
        .collect();
    let mut out = vec![];
    for (node, command) in targets {
        let mut u = ui(pack);
        u.core.push(screen);
        u.update(0);
        u.core.cmds.clear();
        let i = u.dialogs.iter().rposition(|s| s.id() == screen).unwrap();
        let (dialogs, core) = (&mut u.dialogs, &mut u.core);
        dialogs[i].on_event(
            &ViewEvent {
                node,
                kind: EventKind::Click,
            },
            core,
        );
        if u.core
            .cmds
            .iter()
            .any(|c| matches!(c, StackCmd::Message(m) if m.title == UNBUILT))
        {
            out.push(command);
        }
    }
    out
}

#[test]
fn menu_buttons_are_all_built() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    if !dir.join("ui-pack.json").exists() {
        eprintln!("skipped: ui-pack-003 is not converted on this machine");
        return;
    }
    let pack = Rc::new(Pack::load(&dir).unwrap());
    let mut all = vec![];
    for screen in SCREENS {
        for command in unbuilt(&pack, screen) {
            all.push(format!("{screen:?}: {command}"));
        }
    }
    // Still to build; see docs/audits/v20-parity.md. Shrinks to empty.
    let known = [
        "MainMenu: getHelp(\"1. Credits\");",
        "StartMission: canvas.pushDialog(MusicFilesGui);",
    ];
    assert_eq!(all, known, "unbuilt buttons changed");
}

/// Diagnostic for audits: buttons whose click changes nothing visible and
/// asks the game for nothing. `cargo test -p bri-ui --test authored_buttons
/// -- --ignored --nocapture` lists them.
#[test]
#[ignore = "diagnostic listing, not a pass/fail check"]
fn list_inert_buttons() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else { return };
    let pack = Rc::new(pack);
    for screen in SCREENS {
        let mut probe = ui(&pack);
        probe.core.push(screen);
        probe.update(0);
        let Some(s) = probe.screen(screen) else {
            continue;
        };
        let v = s.view();
        let targets: Vec<(usize, String)> = v
            .walk()
            .filter(|&n| visible(v, n) && v.node(n).state.active)
            .filter_map(|n| v.node(n).ctrl.command.clone().map(|c| (n, c)))
            .collect();
        for (node, command) in targets {
            let mut u = ui(&pack);
            u.core.push(screen);
            u.update(0);
            u.core.cmds.clear();
            u.drain_actions();
            let snap = |u: &Ui| {
                let v = u.screen(screen).unwrap().view();
                v.walk()
                    .map(|n| format!("{:?}", v.node(n).state))
                    .collect::<Vec<_>>()
            };
            let before = snap(&u);
            let i = u.dialogs.iter().rposition(|s| s.id() == screen).unwrap();
            let (dialogs, core) = (&mut u.dialogs, &mut u.core);
            dialogs[i].on_event(
                &ViewEvent {
                    node,
                    kind: EventKind::Click,
                },
                core,
            );
            let quiet = u.core.cmds.is_empty() && u.drain_actions().is_empty();
            if quiet && snap(&u) == before {
                println!("INERT {screen:?}: {command}");
            }
        }
    }
}

#[test]
fn advanced_config_saves_the_next_hosts_settings() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-003");
    let Ok(pack) = Pack::load(&dir) else { return };
    let mut u = ui(&Rc::new(pack));
    u.core.push(ScreenId::StartMission);
    u.update(0);
    let click = |u: &mut Ui, screen: ScreenId, command: &str| {
        let node = u
            .screen(screen)
            .unwrap()
            .view()
            .by_command(command)
            .unwrap();
        let i = u.dialogs.iter().rposition(|s| s.id() == screen).unwrap();
        let (dialogs, core) = (&mut u.dialogs, &mut u.core);
        dialogs[i].on_event(
            &ViewEvent {
                node,
                kind: EventKind::Click,
            },
            core,
        );
        u.update(0);
    };
    click(
        &mut u,
        ScreenId::StartMission,
        "canvas.pushDialog(ServerconfigGui);",
    );
    assert_eq!(u.top_id(), ScreenId::ServerConfig);
    let i = u
        .dialogs
        .iter()
        .rposition(|s| s.id() == ScreenId::ServerConfig)
        .unwrap();
    let v = u.dialogs[i].view_mut();
    let n = v.id("AdminOption_maxchatlen").unwrap();
    assert_eq!(v.edit_text(n), "120");
    v.set_text(n, "64");
    click(
        &mut u,
        ScreenId::ServerConfig,
        "canvas.popDialog(ServerConfigGui);",
    );
    assert!(u.screen(ScreenId::ServerConfig).is_none());
    assert_eq!(u.core.prefs.get("$Pref::Server::MaxChatLen"), Some("64"));
}
