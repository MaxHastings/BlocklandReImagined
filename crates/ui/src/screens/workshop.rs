//! Branch-only example picker. Buttons invoke the existing rulelab commands.
use super::*;
use crate::api::{RequestId, UiAction};
use crate::view::EventKind;

const EXAMPLES: [(&str, &str, &str); 9] = [
    (
        "switch",
        "Switch & door",
        "Open a named panel for two seconds",
    ),
    ("teamdoor", "Team door", "One team opens and closes a door"),
    (
        "puzzle",
        "Switch puzzle",
        "Three switches in order unlock a gate",
    ),
    (
        "race",
        "Checkpoints",
        "An ordered course for players or vehicles",
    ),
    (
        "hill",
        "King of the hill",
        "Score while the region is uncontested",
    ),
    (
        "slayer",
        "Kill scoring",
        "Five credited kills win the round",
    ),
    (
        "soccer",
        "Ball goals",
        "Steel ball, opposing goals and teams",
    ),
    (
        "sandbox",
        "State & physics",
        "Charged launcher, bounce pad and timer",
    ),
    (
        "addon",
        "Add-On switch",
        "Red, Green, Blue from Workshop Toys",
    ),
];

pub struct Workshop {
    view: View,
    request: Option<RequestId>,
    error: Option<String>,
}
impl Workshop {
    pub fn new(core: &Core) -> Self {
        let mut s = Self {
            view: View::new(&ctrl(
                "GuiControl",
                "GuiDefaultProfile",
                Rect::new(0, 0, 640, 480),
            )),
            request: None,
            error: None,
        };
        s.build(core);
        s
    }
    fn build(&mut self, core: &Core) {
        let w = (core.logical.0 - 20).clamp(360, 600);
        let h = (core.logical.1 - 20).clamp(280, 450);
        let mut window = ctrl(
            "GuiWindowCtrl",
            "GuiWindowProfile",
            Rect::new((core.logical.0 - w) / 2, (core.logical.1 - h) / 2, w, h),
        );
        window.name = Some("Workshop_Window".into());
        window.text = Some("Rule Workshop - Examples".into());
        let mut root = ctrl(
            "GuiControl",
            "GuiDefaultProfile",
            Rect::new(0, 0, core.logical.0, core.logical.1),
        );
        window.children.push(text(
            "GuiDefaultProfile",
            Rect::new(14, 28, w - 28, 20),
            "Place editable example bricks",
        ));
        window.children.push(text(
            "GuiDefaultProfile",
            Rect::new(14, 49, w - 28, 20),
            "Placed nearby. Choose a clear area.",
        ));
        let mut scroll = ctrl(
            "GuiScrollCtrl",
            "ColorScrollProfile",
            Rect::new(12, 76, w - 24, h - 144),
        );
        scroll.name = Some("Workshop_Scroll".into());
        let columns = if w >= 540 { 2 } else { 1 };
        let cw = (w - 44) / columns;
        let mut body = ctrl(
            "GuiSwatchCtrl",
            "GuiDefaultProfile",
            Rect::new(
                0,
                0,
                w - 42,
                ((EXAMPLES.len() as i32 + columns - 1) / columns) * 60,
            ),
        );
        for (i, (mode, title, subtitle)) in EXAMPLES.iter().enumerate() {
            let x = (i as i32 % columns) * cw;
            let y = (i as i32 / columns) * 60;
            let mut b = button(
                "BlockButtonProfile",
                Rect::new(x, y, cw - 8, 30),
                "base/client/ui/button1",
                title,
                &format!("workshop.{mode}"),
            );
            b.name = Some(format!("Workshop_{mode}"));
            body.children.push(b);
            body.children.push(text(
                "GuiDefaultProfile",
                Rect::new(x + 3, y + 32, cw - 11, 22),
                subtitle,
            ));
        }
        scroll.children.push(body);
        window.children.push(scroll);
        let mut close = button(
            "BlockButtonProfile",
            Rect::new(12, h - 45, 94, 30),
            "base/client/ui/button1",
            "Close",
            "workshop.close",
        );
        close.name = Some("Workshop_Close".into());
        window.children.push(close);
        let mut settings = button(
            "BlockButtonProfile",
            Rect::new(w - 188, h - 45, 174, 30),
            "base/client/ui/button1",
            "MiniGame settings",
            "workshop.settings",
        );
        settings.name = Some("Workshop_Settings".into());
        window.children.push(settings);
        let status = self.error.as_deref().unwrap_or(if self.request.is_some() {
            "Placing example..."
        } else if core.in_game() && core.is_admin() {
            ""
        } else {
            "Host/admin only"
        });
        window.children.push(text(
            "GuiDefaultProfile",
            Rect::new(14, h - 69, w - 28, 22),
            status,
        ));
        root.children.push(window);
        self.view = View::new(&root);
        self.view.measure(&core.pack);
        self.view.layout(core.logical.0, core.logical.1);
        for (mode, _, _) in EXAMPLES {
            if let Some(n) = self.view.id(&format!("Workshop_{mode}")) {
                self.view.set_active(
                    n,
                    core.in_game() && core.is_admin() && self.request.is_none(),
                );
            }
        }
    }
}
impl Screen for Workshop {
    fn id(&self) -> ScreenId {
        ScreenId::RuleWorkshop
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if let Some(id) = self.request.take() {
            core.abandon(id);
        }
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.pop(self.id());
            true
        } else {
            false
        }
    }
    fn layout(&mut self, _w: i32, _h: i32, core: &mut Core) {
        self.build(core);
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        let command = command_of(&self.view, ev.node);
        if ev.kind == EventKind::Close || command == "workshop.close" && ev.kind == EventKind::Click
        {
            core.pop(self.id());
            return;
        }
        if ev.kind != EventKind::Click || self.request.is_some() {
            return;
        }
        if command == "workshop.settings" {
            core.push(ScreenId::MiniGameSettings);
            return;
        }
        let Some(mode) = command
            .strip_prefix("workshop.")
            .filter(|m| EXAMPLES.iter().any(|(a, _, _)| a == m))
        else {
            return;
        };
        if !core.in_game() || !core.is_admin() {
            return;
        }
        self.error = None;
        self.request = Some(core.request_pending(
            UiAction::ChatCommand {
                name: "rulelab".into(),
                args: vec![mode.into()],
            },
            Pending::Other,
        ));
        self.build(core);
    }
    fn on_result(
        &mut self,
        id: RequestId,
        _kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        if self.request != Some(id) {
            return false;
        }
        self.request = None;
        match result {
            Ok(()) => {
                core.pop(self.id());
                core.pop(ScreenId::EscapeMenu);
            }
            Err(e) => {
                self.error = Some("Example not placed".into());
                self.build(core);
                core.message_ok("Example not placed", e);
            }
        }
        true
    }
}

/// A small view of the existing bounded trace. No execution/debugger machinery.
pub struct Explain {
    view: View,
    brick: u64,
    cursor: usize,
    lines: Vec<String>,
    request: Option<RequestId>,
}
impl Explain {
    pub fn new(core: &Core, brick: u64) -> Self {
        let mut s = Self {
            view: View::new(&ctrl(
                "GuiControl",
                "GuiDefaultProfile",
                Rect::new(0, 0, 640, 480),
            )),
            brick,
            cursor: core.chat.line_cursor(),
            lines: vec![],
            request: None,
        };
        s.build(core);
        s
    }
    fn fetch(&mut self, core: &mut Core) {
        self.cursor = core.chat.line_cursor();
        self.lines.clear();
        self.request = Some(core.request_pending(
            UiAction::ChatCommand {
                name: "ruleexplain".into(),
                args: vec![self.brick.to_string()],
            },
            Pending::Other,
        ));
        self.build(core);
    }
    fn build(&mut self, core: &Core) {
        let w = (core.logical.0 - 20).clamp(360, 620);
        let h = (core.logical.1 - 20).clamp(280, 430);
        let mut win = ctrl(
            "GuiWindowCtrl",
            "GuiWindowProfile",
            Rect::new((core.logical.0 - w) / 2, (core.logical.1 - h) / 2, w, h),
        );
        win.text = Some(format!("Explain saved events - Brick {}", self.brick));
        let mut scroll = ctrl(
            "GuiScrollCtrl",
            "ColorScrollProfile",
            Rect::new(12, 30, w - 24, h - 94),
        );
        let mut body = ctrl(
            "GuiSwatchCtrl",
            "GuiDefaultProfile",
            Rect::new(0, 0, w - 42, 1),
        );
        let mut y = 0;
        let lines = if self.lines.is_empty() {
            vec![if self.request.is_some() {
                "Reading saved events...".into()
            } else {
                "Try the event, then Refresh.".into()
            }]
        } else {
            self.lines.clone()
        };
        for line in &lines {
            let height = ((line.len() as i32 * 6 / (w - 52)) + 1) * 16 + 6;
            let mut t = text("GuiDefaultProfile", Rect::new(4, y, w - 50, height), line);
            t.class = "GuiMLTextCtrl".into();
            body.children.push(t);
            y += height;
        }
        body.extent[1] = y.max(1);
        scroll.children.push(body);
        win.children.push(scroll);
        for (x, width, label, command) in [
            (12, 85, "Close", "explain.close"),
            (103, 132, "Back to game", "explain.play"),
            (w - 104, 90, "Refresh", "explain.refresh"),
        ] {
            let mut b = button(
                "BlockButtonProfile",
                Rect::new(x, h - 46, width, 30),
                "base/client/ui/button1",
                label,
                command,
            );
            b.name = Some(command.into());
            win.children.push(b);
        }
        let mut root = ctrl(
            "GuiControl",
            "GuiDefaultProfile",
            Rect::new(0, 0, core.logical.0, core.logical.1),
        );
        root.children.push(win);
        self.view = View::new(&root);
        self.view.measure(&core.pack);
        self.view.layout(core.logical.0, core.logical.1);
        if let Some(n) = self.view.id("explain.refresh") {
            self.view.set_active(n, self.request.is_none());
        }
    }
}
impl Screen for Explain {
    fn id(&self) -> ScreenId {
        ScreenId::RuleExplain(self.brick)
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_wake(&mut self, core: &mut Core) {
        self.fetch(core);
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.pop(self.id());
            true
        } else {
            false
        }
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if let Some(id) = self.request.take() {
            core.abandon(id);
        }
    }
    fn layout(&mut self, _w: i32, _h: i32, core: &mut Core) {
        self.build(core);
    }
    fn on_update(&mut self, core: &mut Core) {
        let prefix = format!("[Events {}] ", self.brick);
        let lines: Vec<_> = core
            .chat
            .lines_since(self.cursor)
            .iter()
            .filter_map(|l| l.text.strip_prefix(&prefix).map(str::to_string))
            .take(16)
            .collect();
        if !lines.is_empty() && lines != self.lines {
            self.lines = lines;
            self.build(core);
        }
    }
    fn on_result(
        &mut self,
        id: RequestId,
        _kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        if self.request != Some(id) {
            return false;
        }
        self.request = None;
        if let Err(e) = result {
            self.lines = vec![e.clone()];
        }
        self.build(core);
        true
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        let command = command_of(&self.view, ev.node);
        if ev.kind == EventKind::Close || command == "explain.close" && ev.kind == EventKind::Click
        {
            core.pop(self.id());
            return;
        }
        if ev.kind != EventKind::Click {
            return;
        }
        if command == "explain.refresh" && self.request.is_none() {
            self.fetch(core);
        }
        if command == "explain.play" {
            core.pop(self.id());
            core.pop(ScreenId::WrenchEvents);
            for v in [
                WrenchVariant::Normal,
                WrenchVariant::Sound,
                WrenchVariant::VehicleSpawn,
            ] {
                core.pop(ScreenId::Wrench(v));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::{ConnectionState, Settings},
        binds::Platform,
        schema::UiPack,
        ui::{Ui, UiConfig},
        view::ViewEvent,
    };
    use std::rc::Rc;
    fn fixture() -> Ui {
        let mut ui = Ui::new(
            crate::testing::pack(UiPack::default()),
            UiConfig {
                size: (640, 480),
                scale: Some(1.),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        ui.core.conn = ConnectionState::InGame {
            server_name: "Workshop".into(),
            max_players: 8,
            single_player: true,
            local: true,
            admin: true,
        };
        ui
    }
    #[test]
    fn example_buttons_use_the_existing_command_and_reject_double_submission() {
        let mut ui = fixture();
        let mut s = Workshop::new(&ui.core);
        let n = s.view.id("Workshop_soccer").unwrap();
        let ev = ViewEvent {
            node: n,
            kind: EventKind::Click,
        };
        s.on_event(&ev, &mut ui.core);
        assert!(ui.drain_actions().iter().any(|(_,a)|matches!(a,UiAction::ChatCommand{name,args} if name=="rulelab"&&args==&["soccer"])));
        ui.drain_actions();
        s.on_event(&ev, &mut ui.core);
        assert!(ui.drain_actions().is_empty());
    }
    #[test]
    fn remote_non_admin_cannot_place_examples() {
        let mut ui = fixture();
        ui.core.conn = ConnectionState::InGame {
            server_name: "Workshop".into(),
            max_players: 8,
            single_player: true,
            local: false,
            admin: false,
        };
        let mut s = Workshop::new(&ui.core);
        let n = s.view.id("Workshop_puzzle").unwrap();
        s.on_event(
            &ViewEvent {
                node: n,
                kind: EventKind::Click,
            },
            &mut ui.core,
        );
        assert!(s.request.is_none());
    }
    #[test]
    fn explain_reads_only_new_results_for_this_brick_and_keeps_errors_visible() {
        let mut ui = fixture();
        ui.core.chat.add("[Events 7] old result", 0);
        let mut s = Explain::new(&ui.core, 7);
        s.on_wake(&mut ui.core);
        let request = s.request.unwrap();
        ui.core.chat.add("[Events 8] unrelated brick", 1);
        ui.core.chat.add("ordinary chat", 1);
        ui.core
            .chat
            .add("[Events 7] Row 1: setColor -> Self: ran", 1);
        s.on_update(&mut ui.core);
        assert_eq!(s.lines, ["Row 1: setColor -> Self: ran"]);
        s.on_result(request, None, &Ok(()), &mut ui.core);
        s.fetch(&mut ui.core);
        assert!(s.lines.is_empty());
        s.on_result(
            s.request.unwrap(),
            None,
            &Err("Brick no longer exists".into()),
            &mut ui.core,
        );
        s.on_update(&mut ui.core);
        assert_eq!(s.lines, ["Brick no longer exists"]);
    }
    #[cfg(feature = "gpu")]
    #[test]
    #[ignore = "requires generated v20 content"]
    fn workshop_offscreen() -> anyhow::Result<()> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = Rc::new(Pack::load(&root.join("content/ui-pack-004"))?);
        let gpu = crate::gpu::Headless::new()?;
        let mut renderer = crate::gpu::UiRenderer::new(&gpu.device, &gpu.queue);
        let out = root.join("artifacts/workshop-ui");
        std::fs::create_dir_all(&out)?;
        for size in [(640, 480), (400, 300), (853, 480)] {
            let mut ui = fixture();
            ui.core.pack = pack.clone();
            ui.core.logical = size;
            let mut s = Workshop::new(&ui.core);
            s.layout(size.0, size.1, &mut ui.core);
            let mut dl = DrawList::new(Rect::new(0, 0, size.0, size.1));
            s.draw(&pack, &mut dl, &ui.core);
            let rgba = gpu.render_rgba(
                &mut renderer,
                &pack,
                &dl,
                (size.0 as u32, size.1 as u32),
                1.,
                [0.15, 0.15, 0.18, 1.],
            )?;
            assert!(renderer.missing_textures().next().is_none());
            image::save_buffer(
                out.join(format!("Examples-{}x{}.png", size.0, size.1)),
                &rgba,
                size.0 as u32,
                size.1 as u32,
                image::ColorType::Rgba8,
            )?;
        }
        let mut ui = fixture();
        ui.core.pack = pack.clone();
        ui.core.logical = (640, 480);
        ui.core.minigames.ready = true;
        ui.core.minigames.palette = vec![[0, 0, 255], [255, 0, 0], [0, 255, 0]];
        ui.core.minigames.owns_active_game = true;
        ui.core.minigames.active_game = Some(crate::api::MiniGameId(1));
        ui.core.minigames.addon_editable = vec![crate::api::MiniGameId(1)];
        ui.core.minigame_addons = Some(crate::api::MiniGameId(1));
        ui.core.minigames.games.push(crate::api::MiniGameSummary {
            id: crate::api::MiniGameId(1),
            title: "Workshop".into(),
            owner: crate::api::MiniGamePlayerId(1),
            owner_name: "Max".into(),
            color: 0,
            member_count: 1,
            invite_only: false,
            rules: crate::api::MiniGameRules::default(),
            teams: vec![
                crate::api::MiniGameTeam {
                    id: 1,
                    name: "Blue".into(),
                    color: 0,
                    settings: Default::default(),
                },
                crate::api::MiniGameTeam {
                    id: 2,
                    name: "Red".into(),
                    color: 1,
                    settings: Default::default(),
                },
            ],
            addon_settings: Default::default(),
            default: false,
            paint_color: None,
            members: vec![crate::api::MiniGameTeamMember {
                id: crate::api::MiniGamePlayerId(1),
                name: "Max".into(),
                team: Some(1),
            }],
        });
        for (id, name) in [
            (ScreenId::EscapeMenu, "Pause"),
            (ScreenId::MiniGameSettings, "MiniGame"),
            (ScreenId::MiniGameAddOns, "Teams"),
        ] {
            let mut s = crate::screens::make(id, &mut ui.core);
            s.layout(640, 480, &mut ui.core);
            let mut dl = DrawList::new(Rect::new(0, 0, 640, 480));
            s.draw(&pack, &mut dl, &ui.core);
            let rgba = gpu.render_rgba(
                &mut renderer,
                &pack,
                &dl,
                (640, 480),
                1.,
                [0.15, 0.15, 0.18, 1.],
            )?;
            image::save_buffer(
                out.join(format!("{name}.png")),
                &rgba,
                640,
                480,
                image::ColorType::Rgba8,
            )?;
        }
        ui.core.logical = (400, 300);
        let mut small = crate::screens::make(ScreenId::MiniGameAddOns, &mut ui.core);
        small.layout(400, 300, &mut ui.core);
        let mut dl = DrawList::new(Rect::new(0, 0, 400, 300));
        small.draw(&pack, &mut dl, &ui.core);
        let rgba = gpu.render_rgba(
            &mut renderer,
            &pack,
            &dl,
            (400, 300),
            1.,
            [0.15, 0.15, 0.18, 1.],
        )?;
        image::save_buffer(
            out.join("Teams-400x300.png"),
            &rgba,
            400,
            300,
            image::ColorType::Rgba8,
        )?;
        ui.core.logical = (640, 480);
        let mut s = Explain::new(&ui.core, 17);
        s.lines = vec![
            "3 saved rows".into(),
            "Region: 4 x 4 x 4; 2 inside".into(),
            "Row 1: IF 1: Player Team = Blue (current: Red) - skipped".into(),
            "Row 2: addScore -> Client #4 after 1000ms: ran".into(),
        ];
        s.build(&ui.core);
        let mut dl = DrawList::new(Rect::new(0, 0, 640, 480));
        s.draw(&pack, &mut dl, &ui.core);
        let rgba = gpu.render_rgba(
            &mut renderer,
            &pack,
            &dl,
            (640, 480),
            1.,
            [0.15, 0.15, 0.18, 1.],
        )?;
        image::save_buffer(
            out.join("Explain.png"),
            &rgba,
            640,
            480,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    }
}
