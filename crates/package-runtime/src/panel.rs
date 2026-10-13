//! Panels: windows an Add-On's rules show a player (`show_panel`), built
//! from the game's own v20 widgets: text, a list to pick from, text fields
//! and a row of buttons. A roleplay shop, a bank, a job board or a
//! Wiremod-style gate editor needs no client code: the rules describe the
//! window, and what the player does in it comes back to them
//! (`on_panel`).
//!
//! The engine owns the window and its widgets; the rules own what it
//! says and what its buttons mean. A player has one panel open at a time:
//! a new one replaces it.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most items a panel has.
pub const MAX_PANEL_ITEMS: usize = 24;
/// Most buttons in one row.
pub const MAX_PANEL_BUTTONS: usize = 6;
/// Most rows a list has.
pub const MAX_PANEL_ROWS: usize = 256;
/// Longest title, label, button or list row, in characters.
pub const MAX_PANEL_LINE: usize = 96;
/// Longest text item, in characters.
pub const MAX_PANEL_TEXT: usize = 1024;
/// Longest text a field may hold.
pub const MAX_PANEL_FIELD: u16 = 256;
/// Longest panel, item or button id.
pub const MAX_PANEL_ID: usize = 32;
/// The panel's size bounds, in the game's 640x480 screen units.
pub const PANEL_WIDTH: std::ops::RangeInclusive<u16> = 160..=600;
pub const PANEL_HEIGHT: std::ops::RangeInclusive<u16> = 100..=440;

/// A window the rules show a player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Panel {
    /// The rules' name for it, which every event carries and
    /// `close_panel` names.
    pub id: String,
    /// The window's title.
    #[serde(default)]
    pub title: String,
    #[serde(default = "default_width")]
    pub width: u16,
    #[serde(default = "default_height")]
    pub height: u16,
    /// Top to bottom.
    #[serde(default)]
    pub items: Vec<PanelItem>,
}
fn default_width() -> u16 {
    320
}
fn default_height() -> u16 {
    260
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    /// Lines of text (`text`).
    Text,
    /// Rows to pick one of (`rows`, `selected`), `height` tall; picking
    /// one is an event. With `drag`, a row can be dragged onto another
    /// list with `drag` (or elsewhere in its own), which is an event too.
    List,
    /// A line of text the player types (`label` beside it, `text` to
    /// start with, at most `max` characters).
    Field,
    /// A row of buttons (`buttons`); pressing one is an event.
    Buttons,
}

/// One item of a panel. Which fields mean anything depends on its `kind`;
/// the rest stay at their defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelItem {
    pub kind: PanelKind,
    /// A list's or field's name in events.
    #[serde(default)]
    pub id: String,
    /// A text item's lines, or a field's starting text.
    #[serde(default)]
    pub text: String,
    /// A field's label.
    #[serde(default)]
    pub label: String,
    /// A list's rows.
    #[serde(default)]
    pub rows: Vec<String>,
    /// A list's picked row.
    #[serde(default)]
    pub selected: Option<u16>,
    /// A list's height (default 120).
    #[serde(default)]
    pub height: Option<u16>,
    /// A field's most characters (default 64).
    #[serde(default)]
    pub max: Option<u16>,
    /// A list's rows can be dragged to another list that allows it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drag: bool,
    #[serde(default)]
    pub buttons: Vec<PanelButton>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelButton {
    pub id: String,
    pub label: String,
}

/// What the player did in a panel, sent back to its rules: the control
/// (a button, a list picked from, `close` for the window's close box)
/// and every field's text and list's pick as they stand.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PanelEvent {
    pub panel: String,
    pub control: String,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub lists: BTreeMap<String, Option<u16>>,
    /// A row dragged onto the list `control` names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop: Option<PanelDrop>,
}

/// A row dragged from one list and let go over another (or its own).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDrop {
    /// The list the row came from.
    pub from: String,
    /// The row, counted from 0, in that list.
    pub row: u16,
    /// The row of the list it was let go over, if any (none: below its
    /// last row).
    #[serde(default)]
    pub at: Option<u16>,
}

/// An id rules may give: 1 to 32 of a-z, 0-9 and `_`.
pub fn is_panel_id(id: &str) -> bool {
    (1..=MAX_PANEL_ID).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn line(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.chars().any(char::is_control)
}

impl PanelItem {
    /// A field's most characters.
    pub fn max_chars(&self) -> u16 {
        self.max.unwrap_or(64)
    }
    /// What is wrong with this item, if anything.
    fn problem(&self) -> Option<String> {
        let unused =
            |what: &str, set: bool| set.then(|| format!("a {:?} item has no {what}", self.kind));
        let no_id = || (!self.id.is_empty()).then(|| format!("a {:?} item has no id", self.kind));
        match self.kind {
            PanelKind::Text => no_id()
                .or_else(|| unused("drag", self.drag))
                .or_else(|| unused("label", !self.label.is_empty()))
                .or_else(|| unused("rows", !self.rows.is_empty()))
                .or_else(|| unused("buttons", !self.buttons.is_empty()))
                .or_else(|| {
                    (self.text.chars().count() > MAX_PANEL_TEXT
                        || self.text.chars().any(|c| c.is_control() && c != '\n'))
                    .then(|| format!("text is at most {MAX_PANEL_TEXT} characters, no control characters but new lines"))
                }),
            PanelKind::List => (!is_panel_id(&self.id))
                .then(|| format!("a list's id is 1 to {MAX_PANEL_ID} of a-z, 0-9 and _"))
                .or_else(|| unused("text", !self.text.is_empty()))
                .or_else(|| unused("buttons", !self.buttons.is_empty()))
                .or_else(|| {
                    (self.rows.len() > MAX_PANEL_ROWS
                        || !self.rows.iter().all(|r| line(r, MAX_PANEL_LINE)))
                    .then(|| format!("a list has at most {MAX_PANEL_ROWS} rows of {MAX_PANEL_LINE} characters"))
                })
                .or_else(|| {
                    self.selected
                        .is_some_and(|s| usize::from(s) >= self.rows.len())
                        .then(|| "a list's selected row is one of its rows".into())
                })
                .or_else(|| {
                    self.height
                        .is_some_and(|h| !(40..=400).contains(&h))
                        .then(|| "a list is 40 to 400 tall".into())
                }),
            PanelKind::Field => (!is_panel_id(&self.id))
                .then(|| format!("a field's id is 1 to {MAX_PANEL_ID} of a-z, 0-9 and _"))
                .or_else(|| unused("drag", self.drag))
                .or_else(|| unused("rows", !self.rows.is_empty()))
                .or_else(|| unused("buttons", !self.buttons.is_empty()))
                .or_else(|| {
                    (!(1..=MAX_PANEL_FIELD).contains(&self.max_chars()))
                        .then(|| format!("a field holds 1 to {MAX_PANEL_FIELD} characters"))
                })
                .or_else(|| {
                    (!line(&self.label, MAX_PANEL_LINE)
                        || !line(&self.text, usize::from(self.max_chars())))
                    .then(|| "a field's label and text are single lines within its size".into())
                }),
            PanelKind::Buttons => no_id()
                .or_else(|| unused("drag", self.drag))
                .or_else(|| unused("rows", !self.rows.is_empty()))
                .or_else(|| unused("text", !self.text.is_empty()))
                .or_else(|| {
                    (self.buttons.is_empty() || self.buttons.len() > MAX_PANEL_BUTTONS)
                        .then(|| format!("a row has 1 to {MAX_PANEL_BUTTONS} buttons"))
                })
                .or_else(|| {
                    self.buttons
                        .iter()
                        .find(|b| !is_panel_id(&b.id) || b.id == "close" || !line(&b.label, MAX_PANEL_LINE))
                        .map(|b| format!("button {:?}: an id of a-z, 0-9 and _ (not `close`), a one-line label", b.id))
                }),
        }
    }
}

impl Panel {
    /// What is wrong with this panel, if anything: the first problem.
    pub fn problem(&self) -> Option<String> {
        if !is_panel_id(&self.id) {
            return Some(format!(
                "a panel's id is 1 to {MAX_PANEL_ID} of a-z, 0-9 and _"
            ));
        }
        if !line(&self.title, MAX_PANEL_LINE) {
            return Some(format!(
                "a title is one line of at most {MAX_PANEL_LINE} characters"
            ));
        }
        if !PANEL_WIDTH.contains(&self.width) || !PANEL_HEIGHT.contains(&self.height) {
            return Some(format!(
                "a panel is {} to {} wide and {} to {} tall",
                PANEL_WIDTH.start(),
                PANEL_WIDTH.end(),
                PANEL_HEIGHT.start(),
                PANEL_HEIGHT.end()
            ));
        }
        if self.items.len() > MAX_PANEL_ITEMS {
            return Some(format!("a panel has at most {MAX_PANEL_ITEMS} items"));
        }
        let mut names = std::collections::BTreeSet::new();
        for item in &self.items {
            if let Some(problem) = item.problem() {
                return Some(problem);
            }
            let ids = std::iter::once(item.id.as_str())
                .filter(|id| !id.is_empty())
                .chain(item.buttons.iter().map(|b| b.id.as_str()));
            for id in ids {
                if !names.insert(id) {
                    return Some(format!("two controls are named {id:?}"));
                }
            }
        }
        None
    }
    pub fn is_bounded(&self) -> bool {
        self.problem().is_none()
    }

    /// The item a control names, with the button when it is one.
    fn control(&self, id: &str) -> Option<(&PanelItem, Option<&PanelButton>)> {
        self.items.iter().find_map(|item| {
            if item.id == id {
                return Some((item, None));
            }
            item.buttons
                .iter()
                .find(|b| b.id == id)
                .map(|b| (item, Some(b)))
        })
    }

    /// Whether `event` is one this panel could send: a control it has
    /// (a button, a list, or `close`), and only its own fields and lists,
    /// within their sizes.
    pub fn accepts(&self, event: &PanelEvent) -> bool {
        let control = event.control == "close"
            || matches!(
                self.control(&event.control),
                Some((
                    PanelItem {
                        kind: PanelKind::List,
                        ..
                    },
                    None
                )) | Some((_, Some(_)))
            );
        let fields = event.fields.iter().all(|(id, text)| {
            matches!(self.control(id), Some((item, None)) if item.kind == PanelKind::Field
                && line(text, usize::from(item.max_chars())))
        });
        let lists = event.lists.iter().all(|(id, row)| {
            matches!(self.control(id), Some((item, None)) if item.kind == PanelKind::List
                && row.is_none_or(|r| usize::from(r) < item.rows.len()))
        });
        let list = |id: &str| match self.control(id) {
            Some((item, None)) if item.kind == PanelKind::List && item.drag => Some(item),
            _ => None,
        };
        let drop = event.drop.as_ref().is_none_or(|d| {
            let (Some(from), Some(to)) = (list(&d.from), list(&event.control)) else {
                return false;
            };
            usize::from(d.row) < from.rows.len()
                && d.at.is_none_or(|at| usize::from(at) < to.rows.len())
        });
        event.panel == self.id && control && fields && lists && drop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shop() -> Panel {
        serde_json::from_value(serde_json::json!({
            "id": "shop", "title": "Gun Shop",
            "items": [
                { "kind": "text", "text": "Pick a gun.\nPrices include tax." },
                { "kind": "list", "id": "stock", "rows": ["Pistol $5", "Rifle $20"], "selected": 0 },
                { "kind": "field", "id": "qty", "label": "How many", "text": "1", "max": 3 },
                { "kind": "buttons", "buttons": [{ "id": "buy", "label": "Buy" }] }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn a_shop_panel_is_bounded_and_accepts_only_its_own_controls() {
        let panel = shop();
        assert_eq!(panel.problem(), None);
        let buy = PanelEvent {
            panel: "shop".into(),
            control: "buy".into(),
            fields: [("qty".into(), "2".into())].into(),
            lists: [("stock".into(), Some(1))].into(),
            drop: None,
        };
        assert!(panel.accepts(&buy));
        assert!(panel.accepts(&PanelEvent {
            control: "stock".into(),
            ..buy.clone()
        }));
        assert!(panel.accepts(&PanelEvent {
            control: "close".into(),
            ..buy.clone()
        }));
        for bad in [
            PanelEvent {
                control: "qty".into(),
                ..buy.clone()
            },
            PanelEvent {
                control: "sell".into(),
                ..buy.clone()
            },
            PanelEvent {
                panel: "bank".into(),
                ..buy.clone()
            },
            PanelEvent {
                fields: [("qty".into(), "1234".into())].into(),
                ..buy.clone()
            },
            PanelEvent {
                lists: [("stock".into(), Some(2))].into(),
                ..buy.clone()
            },
            PanelEvent {
                fields: [("stock".into(), "x".into())].into(),
                ..buy.clone()
            },
        ] {
            assert!(!panel.accepts(&bad), "{bad:?}");
        }
    }

    #[test]
    fn misshapen_panels_say_what_is_wrong() {
        let mut p = shop();
        p.items[1].selected = Some(2);
        assert!(p.problem().unwrap().contains("selected"));
        let mut p = shop();
        p.items[3].buttons[0].id = "stock".into();
        assert!(p.problem().unwrap().contains("two controls"));
        let mut p = shop();
        p.items[0].rows = vec!["x".into()];
        assert!(p.problem().unwrap().contains("no rows"));
        let mut p = shop();
        p.width = 2000;
        assert!(p.problem().is_some());
        let mut p = shop();
        p.items[3].buttons[0].id = "close".into();
        assert!(p.problem().is_some());
        let mut p = shop();
        p.items[0].drag = true;
        assert!(p.problem().unwrap().contains("no drag"));
    }

    fn backpack_and_chest() -> Panel {
        serde_json::from_value(serde_json::json!({
            "id": "chest", "title": "Chest",
            "items": [
                { "kind": "list", "id": "pack", "rows": ["Bread", "Rope"], "drag": true },
                { "kind": "list", "id": "chest", "rows": ["Gold"], "drag": true },
                { "kind": "list", "id": "log", "rows": ["opened"] }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn a_row_drops_only_between_lists_that_drag_and_onto_rows_they_have() {
        let panel = backpack_and_chest();
        assert_eq!(panel.problem(), None);
        let drop = |control: &str, from: &str, row: u16, at: Option<u16>| PanelEvent {
            panel: "chest".into(),
            control: control.into(),
            drop: Some(PanelDrop {
                from: from.into(),
                row,
                at,
            }),
            ..PanelEvent::default()
        };
        assert!(panel.accepts(&drop("chest", "pack", 1, None)));
        assert!(panel.accepts(&drop("chest", "pack", 0, Some(0))));
        assert!(panel.accepts(&drop("pack", "chest", 0, Some(1))));
        assert!(panel.accepts(&drop("pack", "pack", 1, Some(0))));
        for bad in [
            drop("chest", "pack", 2, None),
            drop("chest", "pack", 0, Some(1)),
            drop("log", "pack", 0, None),
            drop("chest", "log", 0, None),
            drop("chest", "nowhere", 0, None),
        ] {
            assert!(!panel.accepts(&bad), "{bad:?}");
        }
    }
}
