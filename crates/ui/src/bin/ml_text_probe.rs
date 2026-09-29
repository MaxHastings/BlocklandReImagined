//! Offscreen captures of server-supplied ML text on the real UI pack: center
//! and bottom prints, chat, a message box, a Tutorial prompt and hostile
//! markup. Does not create a window or send any operating-system input.
use anyhow::Result;
use bri_ui::{
    api::*,
    binds::{BindMap, Platform},
    gpu::{Headless, UiRenderer},
    pack::Pack,
    ui::{Ui, UiConfig},
};
use std::{path::PathBuf, rc::Rc};

/// Server text as the client receives it: `\cN` escapes decoded, then
/// bounded by the same sanitizer the client uses.
fn server(text: &str) -> String {
    bri_ui::ml::sanitize(&bri_ui::text::markup_colors(text))
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let pack_path = PathBuf::from(args.first().map_or("content/ui-pack-004", String::as_str));
    let out = PathBuf::from(args.get(1).map_or("artifacts/ml-text", String::as_str));
    std::fs::create_dir_all(&out)?;
    let pack = Rc::new(Pack::load(&pack_path)?);
    let gpu = Headless::new()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    let size = (1024, 768);
    let settings = Settings {
        binds: Some(BindMap::defaults(&pack.data.data, 2, 0, Platform::Windows).entries),
        mouse_type: 2,
        ..Default::default()
    };
    let mut ui = Ui::new(
        pack.clone(),
        UiConfig {
            size,
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        settings,
    );
    ui.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "ML text verification".into(),
        max_players: 8,
        local: true,
        single_player: false,
        admin: true,
    }));
    ui.update(16);
    let mut render = |ui: &Ui, name: &str| -> Result<()> {
        let dl = ui.draw();
        let px = gpu.render_rgba(
            &mut renderer,
            &pack,
            &dl,
            size,
            1.0,
            [0.45, 0.62, 0.78, 1.0],
        )?;
        image::save_buffer(
            out.join(format!("{name}.png")),
            &px,
            size.0,
            size.1,
            image::ColorType::Rgba8,
        )?;
        println!("{name}: {} draw commands", dl.cmds.len());
        Ok(())
    };

    // The event from the Badspot's Birthday save (GameConnection::CenterPrint).
    ui.apply(UiUpdate::CenterPrint {
        text: server(
            "<color:FFFFFF>It's no longer Badspot's' Birthday.<br>Attempts to butter Badspot by making presents will go ignored from now.",
        ),
        seconds: 5.0,
    });
    ui.apply(UiUpdate::BottomPrint {
        text: server("<bitmap:base/client/ui/CI/trophy>\\c3 Goal Completed! - Look - Time: 0:12"),
        seconds: 5.0,
        hide_bar: false,
    });
    ui.update(16);
    render(&ui, "prints-event")?;

    // Tutorial prompts (Map_Tutorial tutorial.cs:654 and :1182).
    ui.apply(UiUpdate::CenterPrint {
        text: server(
            "Shoot a Printable Brick to set its Print\nYou can press the letter or number on your keyboard instead of selecting it\n\n<color:FFFFFF>Complete the Puzzle!",
        ),
        seconds: 5.0,
    });
    ui.apply(UiUpdate::BottomPrint {
        text: server("Press \\c3W A S D\\c0 to move"),
        seconds: 5.0,
        hide_bar: false,
    });
    ui.update(16);
    render(&ui, "prints-tutorial")?;

    // Styled markup: fonts, justification, shadows, margins, links, tabs.
    ui.apply(UiUpdate::CenterPrint {
        text: server(
            "<font:Arial Bold:26><color:ffff00>Round 3<br><font:Palatino Linotype:18><shadowcolor:00000080><shadow:2:2><color:ffffff>Shadowed white<spush><color:ff4040> red inside push<spop> back to white<br><just:right><lmargin:40><rmargin:40>\\c4right justified with margins<br><just:left>left<tab:200,400>\ttab 200\ttab 400<br><a:example.com>a link</a> after link <unknown:tag>dropped",
        ),
        seconds: 5.0,
    });
    ui.update(16);
    render(&ui, "prints-styled")?;
    ui.apply(UiUpdate::ClearPrints);

    // Chat: server markup, a death icon and plain player chat.
    for line in [
        // A player line as the client formats it (tags in player text are
        // shown literally).
        "\\c7\\c3Max\\c7\\c6: hello ‹color:ff0000› stays literal",
        "<color:00ff00>Server says green<br>and no line break in chat",
        "\\c3Max <bitmap:base/client/ui/ci/skull> \\c3Bot",
        "Plain server message (v20 draws uncoloured chat in the profile colour)",
        "<spush><color:ff00ff>pushed<spop> restored \\c2green",
    ] {
        ui.apply(UiUpdate::Chat { text: server(line) });
    }
    ui.update(16);
    render(&ui, "chat")?;

    // Hostile markup: unbounded pushes, huge fonts, remote bitmaps, junk tags.
    let mut hostile = String::new();
    for _ in 0..200 {
        hostile.push_str("<spush><font:Impact:999>");
    }
    hostile
        .push_str("<bitmap:../../secret><bitmap:http://evil/x.png><a:javascript:alert(1)>link</a>");
    hostile.push_str(&"<zz>".repeat(2000));
    hostile.push_str("Still readable <3 and a < b > c");
    ui.apply(UiUpdate::ClearPrints);
    ui.apply(UiUpdate::CenterPrint {
        text: hostile,
        seconds: 5.0,
    });
    ui.update(16);
    render(&ui, "prints-hostile")?;
    ui.apply(UiUpdate::ClearPrints);

    // Message box text is an ML control too (MBOKText, allowColorChars = 0).
    ui.core.message_ok(
        "Request Rejected",
        &server("<just:center><color:ff0000>Too many bricks!<br><color:000000>\\c3colour codes stay off here"),
    );
    ui.update(16);
    render(&ui, "message-box")?;
    Ok(())
}
