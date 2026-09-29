//! v20's HelpDlg: the help pages (`base/help/*.hfl`) listed on the left and
//! the chosen one on the right. The main menu's Credits button opens it on
//! the credits (`getHelp`), and F1 (`contextHelp`) opens or closes it.
use super::*;
use crate::view::EventKind;

const LIST: &str = "HelpFileList";
const TEXT: &str = "HelpText";

pub struct Help {
    view: View,
}

impl Help {
    pub fn new(core: &Core) -> Self {
        let mut view = layout_view(core, "HelpDlg");
        // The authored close button sits above the window; the window's own
        // close box closes it.
        if let Some(n) = view.by_command("Canvas.popDialog(HelpDlg);") {
            view.set_visible(n, false);
        }
        let pages = &core.pack.data.data.help;
        if let Some(n) = view.id(LIST) {
            view.state(n).items = pages
                .iter()
                .enumerate()
                .map(|(i, p)| (p.name.clone(), i as i64))
                .collect();
            // `getHelp(name)`: that page, else the first (v20's Credits
            // button asks for "1. Credits" while the file is "0. Credits").
            let wanted = core.help_page.as_deref().and_then(|want| {
                let want = want.to_ascii_lowercase();
                let topic = |name: &str| {
                    name.split_once(". ")
                        .map_or(name, |(_, t)| t)
                        .to_ascii_lowercase()
                };
                pages
                    .iter()
                    .position(|p| p.name.to_ascii_lowercase() == want)
                    .or_else(|| pages.iter().position(|p| topic(&p.name) == topic(&want)))
            });
            view.select(n, (!pages.is_empty()).then(|| wanted.unwrap_or(0) as i64));
        }
        let mut s = Self { view };
        s.show(core);
        s
    }

    /// `HelpFileList::onSelect`: the page's text, scrolled to its top.
    fn show(&mut self, core: &Core) {
        let page = self
            .view
            .id(LIST)
            .and_then(|n| self.view.selected(n))
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| core.pack.data.data.help.get(i));
        let text = page.map_or_else(String::new, |p| p.text.clone());
        let Some(n) = self.view.id(TEXT) else { return };
        let c = &self.view.node(n).ctrl;
        let h = View::ml_height_ctrl(&core.pack, c, &text, c.extent[0]).max(16);
        self.view.nodes[n].ctrl.extent[1] = h;
        self.view.set_text(n, text);
        if let Some(scroll) = self.view.node(n).parent {
            self.view.scroll_to(scroll, 0);
        }
        self.view.relayout();
    }
}

impl Screen for Help {
    fn id(&self) -> ScreenId {
        ScreenId::Help
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_wake(&mut self, core: &mut Core) {
        core.help_open = true;
    }
    fn on_sleep(&mut self, core: &mut Core) {
        core.help_open = false;
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.pop(ScreenId::Help);
            return true;
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        match ev.kind {
            EventKind::Close => core.pop(ScreenId::Help),
            EventKind::Changed | EventKind::Click
                if self.view.node(ev.node).ctrl.name.as_deref() == Some(LIST) =>
            {
                self.show(core)
            }
            _ => {}
        }
    }
}
