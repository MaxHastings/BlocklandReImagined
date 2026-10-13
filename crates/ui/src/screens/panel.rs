//! An Add-On's panel (`show_panel`): a window its rules describe, laid
//! out top to bottom from the game's own widgets in the Report window's
//! style: plain text, a list to pick from, labelled text fields and a row
//! of buttons. Pressing a button, picking a row or closing the window goes
//! to the rules with every field's text and list's pick
//! (`UiAction::PanelEvent`); the rules decide what it means.
use super::*;
use crate::api::{PanelItemView, PanelView, UiAction};
use crate::view::EventKind;

const MARGIN: i32 = 12;
const GAP: i32 = 8;
const LINE: i32 = 22;
const BUTTON: (i32, i32) = (98, 28);
const LABEL_WIDTH: i32 = 110;
const LIST: &str = "Panel_List_";
const FIELD: &str = "Panel_Field_";
const BUTTON_NAME: &str = "Panel_Button_";

fn named(mut c: Control, name: String) -> Control {
    c.name = Some(name);
    c
}

/// Text as it reads in ML text: no markup, lines kept.
fn plain_lines(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            line.chars()
                .filter(|c| !c.is_control())
                .map(|c| match c {
                    '<' => '‹',
                    '>' => '›',
                    _ => c,
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

pub struct AddOnPanel {
    view: View,
    /// The panel shown: its package, id and controls, for events.
    shown: PanelView,
}

impl AddOnPanel {
    pub fn new(core: &Core) -> Self {
        let shown = core.panel.clone().unwrap_or_default();
        let (w, h) = (
            i32::from(shown.width).clamp(160, 600),
            i32::from(shown.height).clamp(100, 440),
        );
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut win = ctrl(
            "GuiWindowCtrl",
            "BlockWindowProfile",
            Rect::new((640 - w) / 2, (480 - h) / 2, w, h),
        );
        win.text = Some(if shown.title.is_empty() {
            "Panel".into()
        } else {
            shown.title.clone()
        });
        win.h_sizing = HSizing::Center;
        win.v_sizing = VSizing::Center;
        let inner = w - 2 * MARGIN;
        let mut y = 34;
        for item in &shown.items {
            match item {
                PanelItemView::Text(t) => {
                    let markup = plain_lines(t);
                    let height =
                        View::ml_height(&core.pack, "GuiMLTextProfile", &markup, inner).max(16);
                    let mut c = text("GuiMLTextProfile", Rect::new(MARGIN, y, inner, height), "");
                    c.class = "GuiMLTextCtrl".into();
                    c.text = Some(markup);
                    win.children.push(c);
                    y += height + GAP;
                }
                PanelItemView::List { id, height, .. } => {
                    let height = i32::from(*height);
                    let mut scroll = ctrl(
                        "GuiScrollCtrl",
                        "BlockScrollProfile",
                        Rect::new(MARGIN, y, inner, height),
                    );
                    scroll
                        .fields
                        .insert("hScrollBar".into(), "alwaysOff".into());
                    scroll.fields.insert("vScrollBar".into(), "dynamic".into());
                    let list = ctrl(
                        "GuiTextListCtrl",
                        "GuiTextListProfile",
                        Rect::new(0, 0, inner - 16, 16),
                    );
                    scroll.children.push(named(list, format!("{LIST}{id}")));
                    win.children.push(scroll);
                    y += height + GAP;
                }
                PanelItemView::Field {
                    id,
                    label,
                    text: start,
                    max,
                } => {
                    let label_width = if label.is_empty() { 0 } else { LABEL_WIDTH };
                    if label_width > 0 {
                        win.children.push(text(
                            "GuiTextProfile",
                            Rect::new(MARGIN, y, label_width - GAP, LINE),
                            label,
                        ));
                    }
                    let mut edit = ctrl(
                        "GuiTextEditCtrl",
                        "GuiTextEditProfile",
                        Rect::new(MARGIN + label_width, y, inner - label_width, LINE),
                    );
                    edit.text = Some(start.clone());
                    edit.fields.insert("maxLength".into(), max.to_string());
                    win.children.push(named(edit, format!("{FIELD}{id}")));
                    y += LINE + GAP;
                }
                PanelItemView::Buttons(buttons) => {
                    for (i, (id, label)) in buttons.iter().enumerate() {
                        let x = MARGIN + i as i32 * (BUTTON.0 + GAP);
                        let name = format!("{BUTTON_NAME}{id}");
                        win.children.push(named(
                            button(
                                "BlockButtonProfile",
                                Rect::new(x, y, BUTTON.0, BUTTON.1),
                                "base/client/ui/button1",
                                label,
                                &name,
                            ),
                            name.clone(),
                        ));
                    }
                    y += BUTTON.1 + GAP;
                }
            }
        }
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        for item in &shown.items {
            if let PanelItemView::List {
                id, rows, selected, ..
            } = item
                && let Some(n) = view.id(&format!("{LIST}{id}"))
            {
                view.state(n).items = rows
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (r.clone(), i as i64))
                    .collect();
                view.select(n, selected.map(i64::from));
            }
        }
        view.relayout();
        Self { view, shown }
    }

    /// Every field's text and list's pick, as the panel stands.
    fn values(&self) -> (Vec<(String, String)>, Vec<(String, Option<u16>)>) {
        let mut fields = Vec::new();
        let mut lists = Vec::new();
        for item in &self.shown.items {
            match item {
                PanelItemView::Field { id, max, .. } => {
                    if let Some(n) = self.view.id(&format!("{FIELD}{id}")) {
                        let text: String = self
                            .view
                            .edit_text(n)
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(usize::from(*max))
                            .collect();
                        fields.push((id.clone(), text));
                    }
                }
                PanelItemView::List { id, rows, .. } => {
                    if let Some(n) = self.view.id(&format!("{LIST}{id}")) {
                        let row = self
                            .view
                            .selected(n)
                            .and_then(|r| u16::try_from(r).ok())
                            .filter(|r| usize::from(*r) < rows.len());
                        lists.push((id.clone(), row));
                    }
                }
                _ => {}
            }
        }
        (fields, lists)
    }

    fn send(&self, control: &str, core: &mut Core) {
        let (fields, lists) = self.values();
        core.request(UiAction::PanelEvent {
            package: self.shown.package.clone(),
            panel: self.shown.id.clone(),
            control: control.into(),
            fields,
            lists,
        });
    }

    fn close(&mut self, core: &mut Core) {
        self.send("close", core);
        core.panel = None;
        core.pop(self.id());
    }
}

impl Screen for AddOnPanel {
    fn id(&self) -> ScreenId {
        ScreenId::AddOnPanel
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            self.close(core);
            return true;
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        let name = self
            .view
            .node(ev.node)
            .ctrl
            .name
            .clone()
            .unwrap_or_default();
        match ev.kind {
            EventKind::Close => self.close(core),
            EventKind::Click => {
                if let Some(id) = name.strip_prefix(BUTTON_NAME) {
                    self.send(id, core);
                }
            }
            EventKind::Changed => {
                if let Some(id) = name.strip_prefix(LIST) {
                    self.send(id, core);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_text_never_becomes_markup_and_keeps_its_lines() {
        assert_eq!(
            plain_lines("<b>Shop</b>\nPick one"),
            "‹b›Shop‹/b›<br>Pick one"
        );
    }
}
