//! An Add-On's panel (`show_panel`): a window its rules describe, laid
//! out top to bottom from the game's own widgets in the Report window's
//! style: plain text, a list to pick from, labelled text fields and a row
//! of buttons. Pressing a button, picking a row or closing the window goes
//! to the rules with every field's text and list's pick
//! (`UiAction::PanelEvent`); the rules decide what it means. A list the
//! rules let drag (`drag`) gives its rows to another such list, or
//! reorders its own: the drop goes to the rules too, which move the row
//! (or refuse) by showing the panel again.
use super::*;
use crate::api::{PanelDropView, PanelItemView, PanelPicture, PanelView, UiAction};
use crate::view::EventKind;

const MARGIN: i32 = 12;
const GAP: i32 = 8;
const LINE: i32 = 22;
const BUTTON: (i32, i32) = (98, 28);
const LABEL_WIDTH: i32 = 110;
const LIST: &str = "Panel_List_";
const FIELD: &str = "Panel_Field_";
const BUTTON_NAME: &str = "Panel_Button_";
const BACKGROUND: &str = "Panel_Background";
const IMAGE: &str = "Panel_Image_";

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

/// A node by name, its picture, its hover picture and its tint.
type Pictured = (String, PanelPicture, Option<PanelPicture>, Option<[f32; 4]>);
/// Every field's text, by id.
type Fields = Vec<(String, String)>;
/// Every list's picked row, by id.
type Lists = Vec<(String, Option<u16>)>;

pub struct AddOnPanel {
    view: View,
    /// The panel shown: its package, id and controls, for events.
    shown: PanelView,
}

impl AddOnPanel {
    pub fn new(core: &Core) -> Self {
        let shown = core.panel.clone().unwrap_or_default();
        let (w, h) = (
            i32::from(shown.width).clamp(160, 640),
            i32::from(shown.height).clamp(100, 480),
        );
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let place = Rect::new((640 - w) / 2, (480 - h) / 2, w, h);
        let mut win = if shown.border {
            ctrl("GuiWindowCtrl", "BlockWindowProfile", place)
        } else {
            ctrl("GuiControl", "GuiDefaultProfile", place)
        };
        if shown.border {
            win.text = Some(if shown.title.is_empty() {
                "Panel".into()
            } else {
                shown.title.clone()
            });
        }
        win.h_sizing = HSizing::Center;
        win.v_sizing = VSizing::Center;
        if shown.background.is_some() {
            win.children.push(named(
                ctrl("GuiBitmapCtrl", "HUDBitmapProfile", Rect::new(0, 0, w, h)),
                BACKGROUND.into(),
            ));
        }
        let inner = w - 2 * MARGIN;
        let mut y = if shown.border { 34 } else { MARGIN };
        // Pictures to set once the view exists: node name, picture, and the
        // hover picture and tint of a button.
        let mut pictures: Vec<Pictured> = Vec::new();
        for (n, item) in shown.items.iter().enumerate() {
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
                PanelItemView::List {
                    id, height, drag, ..
                } => {
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
                    let mut list = ctrl(
                        "GuiTextListCtrl",
                        "GuiTextListProfile",
                        Rect::new(0, 0, inner - 16, 16),
                    );
                    if *drag {
                        list.fields.insert("dragRows".into(), "1".into());
                    }
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
                    let mut flowed = false;
                    for (i, b) in buttons.iter().enumerate() {
                        let name = format!("{BUTTON_NAME}{}", b.id);
                        let rect = match b.rect {
                            Some([bx, by, bw, bh]) => Rect::new(bx, by, bw, bh),
                            None => {
                                flowed = true;
                                let x = MARGIN + i as i32 * (BUTTON.0 + GAP);
                                Rect::new(x, y, BUTTON.0, BUTTON.1)
                            }
                        };
                        let mut c = named(
                            button(
                                "BlockButtonProfile",
                                rect,
                                "base/client/ui/button1",
                                &b.label,
                                &name,
                            ),
                            name.clone(),
                        );
                        if let Some(colour) = b.color {
                            c.color = Some(crate::screens::rgba(colour));
                        }
                        win.children.push(c);
                        if let Some(p) = b.picture {
                            pictures.push((name, p, b.hover_picture, None));
                        }
                    }
                    if flowed {
                        y += BUTTON.1 + GAP;
                    }
                }
                PanelItemView::Image {
                    picture,
                    rect: [ix, iy, iw, ih],
                    tint,
                } => {
                    let name = format!("{IMAGE}{n}");
                    win.children.push(named(
                        ctrl(
                            "GuiBitmapCtrl",
                            "HUDBitmapProfile",
                            Rect::new(*ix, *iy, *iw, *ih),
                        ),
                        name.clone(),
                    ));
                    if let Some(p) = picture {
                        pictures.push((name, *p, None, Some(*tint)));
                    }
                }
            }
        }
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        if let Some(background) = &shown.background
            && let Some(p) = background.picture
            && let Some(n) = view.id(BACKGROUND)
        {
            let state = view.state(n);
            state.external_texture = Some(p.key);
            state.external_size = Some(p.size);
            state.slice = background.slice.map(|s| s.map(i32::from));
            state.tint = Some(crate::screens::rgba(background.tint));
        }
        for (name, picture, hover, tint) in &pictures {
            if let Some(n) = view.id(name) {
                let state = view.state(n);
                state.external_texture = Some(picture.key);
                state.external_size = Some(picture.size);
                state.hover_texture = hover.map(|h| h.key);
                if let Some(t) = tint {
                    state.tint = Some(crate::screens::rgba(*t));
                }
            }
        }
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
    fn values(&self) -> (Fields, Lists) {
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
        self.send_drop(control, None, core);
    }

    fn send_drop(&self, control: &str, drop: Option<PanelDropView>, core: &mut Core) {
        let (fields, lists) = self.values();
        core.request(UiAction::PanelEvent {
            package: self.shown.package.clone(),
            panel: self.shown.id.clone(),
            control: control.into(),
            fields,
            lists,
            drop,
        });
    }

    /// The list id a panel list's node stands for.
    fn list_id(&self, node: crate::view::NodeId) -> Option<String> {
        self.view
            .node(node)
            .ctrl
            .name
            .as_deref()?
            .strip_prefix(LIST)
            .map(str::to_owned)
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
    fn modal(&self) -> bool {
        self.shown.modal
    }
    fn wants_mouse(&self) -> bool {
        !self.shown.modal
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
            EventKind::Drop => {
                let Some(dropped) = self.view.take_drop() else {
                    return;
                };
                let row = |item: i64| u16::try_from(item).ok();
                if let (Some(to), Some(from), Some(r)) = (
                    name.strip_prefix(LIST),
                    self.list_id(dropped.from),
                    row(dropped.item),
                ) {
                    let drop = PanelDropView {
                        from,
                        row: r,
                        at: dropped.at.and_then(row),
                    };
                    self.send_drop(to, Some(drop), core);
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
