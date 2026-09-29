//! Chat links on the real UI pack: with the cursor toggled on, clicking a
//! link asks before opening the browser. No OS window or input is created.
use bri_ui::{
    api::*,
    binds::{BindMap, Platform},
    input::{InputEvent, MouseButton},
    pack::Pack,
    screens::{
        ScreenId,
        play::{chat_link_at, mouse_tip},
    },
    ui::{Ui, UiConfig},
};
use std::{path::Path, rc::Rc};

fn click(ui: &mut Ui, x: f32, y: f32) {
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
}

#[test]
fn clicking_a_chat_link_asks_before_opening_it() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-004");
    let Ok(pack) = Pack::load(&dir) else {
        return;
    };
    let pack = Rc::new(pack);
    let settings = Settings {
        binds: Some(BindMap::defaults(&pack.data.data, 2, 0, Platform::Windows).entries),
        mouse_type: 2,
        ..Default::default()
    };
    let mut ui = Ui::new(
        pack.clone(),
        UiConfig {
            size: (1024, 768),
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        settings,
    );
    ui.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "links".into(),
        max_players: 8,
        local: false,
        single_player: false,
        admin: false,
    }));
    ui.apply(UiUpdate::Chat {
        text: "\u{E007}\u{E003}Max\u{E007}\u{E006}: see <a:blockland.us/forum>blockland.us/forum</a>\u{E006} now"
            .into(),
    });
    ui.update(16);
    let hit = (0..1024)
        .step_by(2)
        .flat_map(|x| (0..80).step_by(2).map(move |y| (x, y)))
        .find(|&(x, y)| chat_link_at(&ui.core, x, y).is_some())
        .expect("the link is drawn in the chat");
    assert_eq!(
        chat_link_at(&ui.core, hit.0, hit.1).as_deref(),
        Some("blockland.us/forum")
    );

    // A shown link brings up v20's "TIP: Press M ..." under the chat.
    assert!(mouse_tip(&ui.core));

    // Without the cursor a click is gameplay input and asks nothing.
    click(&mut ui, hit.0 as f32 + 1.0, hit.1 as f32 + 1.0);
    ui.update(16);
    assert!(!ui.is_open(ScreenId::MessageBox));

    ui.core.run_command("toggleCursor", true);
    ui.update(16);
    click(&mut ui, hit.0 as f32 + 1.0, hit.1 as f32 + 1.0);
    ui.update(16);
    assert_eq!(ui.top_id(), ScreenId::MessageBox);
    assert!(
        ui.drain_actions()
            .iter()
            .all(|(_, a)| !matches!(a, UiAction::OpenUrl(_))),
        "nothing opens before the player confirms"
    );

    // Hostile schemes never become a web address.
    assert_eq!(bri_ui::ui::web_url("javascript:alert(1)"), None);
    assert_eq!(bri_ui::ui::web_url("file:///C:/x"), None);
    assert_eq!(
        bri_ui::ui::web_url("blockland.us/forum").as_deref(),
        Some("http://blockland.us/forum")
    );
}
