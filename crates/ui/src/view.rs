//! Retained control tree built from an authored layout (`schema::Control`),
//! laid out with Torque's resize rules, drawn with the original skins, and
//! driven by input. Screens own a `View` and react to its `ViewEvent`s.

use crate::draw::{DrawList, Filter};
use crate::geom::{self, Rect, Rgba, WHITE};
use crate::input::{Chord, Key, Modifiers, MouseButton};
use crate::pack::{Pack, TexKey};
use crate::schema::{Control, HSizing, Justify, Style, VSizing};
use crate::text::{self, Font};
use std::collections::HashMap;

pub type NodeId = usize;

/// Window skin piece indices (Torque GuiWindowCtrl bitmap array).
mod win {
    pub const CLOSE: usize = 0;
    pub const MAXIMIZE: usize = 3;
    pub const NORMAL: usize = 6;
    pub const MINIMIZE: usize = 9;
    pub const TOP_LEFT: usize = 12;
    pub const TOP_RIGHT: usize = 13;
    pub const TOP: usize = 14;
    pub const LEFT: usize = 18;
    pub const RIGHT: usize = 19;
    pub const BOTTOM_LEFT: usize = 20;
    pub const BOTTOM: usize = 21;
    pub const BOTTOM_RIGHT: usize = 22;
}
/// Scroll skin parts × 3 states (Torque GuiScrollCtrl bitmap array).
mod scroll {
    pub const UP: usize = 0;
    pub const DOWN: usize = 1;
    pub const THUMB_TOP: usize = 2;
    pub const THUMB: usize = 3;
    pub const THUMB_BOTTOM: usize = 4;
    pub const PAGE: usize = 5;
    pub const STATES: usize = 3;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    Num(f32),
    Text(String),
    /// Selected item id (popups, lists); `None` = nothing selected.
    Selected(Option<i64>),
}

#[derive(Debug, Clone)]
pub struct NodeState {
    pub visible: bool,
    pub active: bool,
    pub text: Option<String>,
    pub bitmap: Option<String>,
    /// Host texture, sampled over its complete normalized UV rectangle.
    pub external_texture: Option<u64>,
    /// mColor for bitmap buttons, colour for swatches, setColor tint for bitmaps.
    pub tint: Option<Rgba>,
    pub value: Value,
    /// Popup/list items: (text, id). List text uses `\t` to separate columns.
    pub items: Vec<(String, i64)>,
    /// Vertical scroll offset (scroll controls).
    pub scroll_y: i32,
    pub cursor: usize,
    /// Animation frame (animated bitmaps).
    pub frame: usize,
    /// Text-list row height from the profile font (`View::measure`).
    pub row_height: i32,
    /// How far the player dragged this window from its laid-out place.
    pub moved: (i32, i32),
    /// How much the player widened and heightened this window.
    pub resized: (i32, i32),
    /// Maximized to fill its parent (`canMaximize`).
    pub maximized: bool,
    /// Minimized to its title bar of this height (`canMinimize`).
    pub minimized: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub ctrl: Control,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// Absolute rectangle in logical pixels after layout.
    pub rect: Rect,
    pub state: NodeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Click,
    RightClick,
    DoubleClick,
    /// Value changed by the user (checkbox, radio, slider, popup, list, edit).
    Changed,
    /// Enter pressed in a text edit (`altCommand`) or list double-click.
    Submit,
    /// Mouse entered the control (menu hover sounds).
    Hover,
    /// Window close button.
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewEvent {
    pub node: NodeId,
    pub kind: EventKind,
}

/// An open GuiPopUpMenuCtrl list (GuiPopupTextListCtrl in Torque).
#[derive(Debug, Clone, Copy)]
struct Popup {
    node: NodeId,
    /// List rectangle including its 1px frame.
    rect: Rect,
    row_h: i32,
    /// Rows visible at once; fewer than the item count adds a scroll bar.
    rows: usize,
    /// First visible item.
    scroll: usize,
    /// Highlighted item (mouse hover or keyboard cursor).
    hover: Option<usize>,
    /// Opened by the current press: releasing over a row selects it.
    dragging: bool,
}

impl Popup {
    fn scroll_bar(&self, items: usize) -> Option<Rect> {
        (items > self.rows).then(|| {
            Rect::new(
                self.rect.right() - 1 - POPUP_BAR,
                self.rect.y + 1,
                POPUP_BAR,
                self.rect.h - 2,
            )
        })
    }
    fn row_at(&self, x: i32, y: i32, items: usize) -> Option<usize> {
        let in_bar = self.scroll_bar(items).is_some_and(|b| b.contains(x, y));
        (self.rect.contains(x, y) && !in_bar)
            .then(|| self.scroll + ((y - self.rect.y - 1).max(0) / self.row_h.max(1)) as usize)
            .filter(|&i| i < items && i < self.scroll + self.rows)
    }
    fn scroll_to(&mut self, i: usize, items: usize) {
        if i < self.scroll {
            self.scroll = i;
        } else if i >= self.scroll + self.rows {
            self.scroll = i + 1 - self.rows;
        }
        self.scroll = self.scroll.min(items.saturating_sub(self.rows));
    }
}

/// Thumb (y, height) on a scroll track showing `visible` of `content`
/// pixels scrolled by `offset`, or `None` when everything fits.
fn thumb(
    track_y: i32,
    track_h: i32,
    content: i32,
    visible: i32,
    offset: i32,
) -> Option<(i32, i32)> {
    let content = content.max(1);
    if content <= visible || track_h <= 12 {
        return None;
    }
    let th = ((track_h as i64 * visible as i64) / content as i64).max(16) as i32;
    let max_scroll = content - visible;
    let ty = track_y + ((track_h - th) as i64 * offset as i64 / max_scroll.max(1) as i64) as i32;
    Some((ty, th))
}

/// Lowercased, trimmed item text the type-to-filter search matches against.
fn search_key(text: &str) -> String {
    text.trim().to_lowercase()
}

/// "NONE" (the Wrench's datablock lists) and "-" (the event editor) stay at
/// the top of a filtered list so clearing a choice is always one row away.
fn pinned_key(key: &str) -> bool {
    key == "none" || key == "-"
}

/// Type-to-filter order for `keys` (from `search_key`) under a lowercase
/// `query`: every row when the query is empty, otherwise the pinned rows,
/// then rows starting with the query, then rows merely containing it, each
/// group in list order.
fn filter_keys(keys: &[String], query: &str, out: &mut Vec<usize>) {
    out.clear();
    if query.is_empty() {
        out.extend(0..keys.len());
        return;
    }
    out.extend((0..keys.len()).filter(|&i| pinned_key(&keys[i])));
    out.extend((0..keys.len()).filter(|&i| !pinned_key(&keys[i]) && keys[i].starts_with(query)));
    out.extend((0..keys.len()).filter(|&i| {
        !pinned_key(&keys[i]) && !keys[i].starts_with(query) && keys[i].contains(query)
    }));
}

/// The rows (item indices, in display order) a dropdown shows for `query`.
pub fn filter_popup_items(items: &[(String, i64)], query: &str) -> Vec<usize> {
    let keys: Vec<String> = items.iter().map(|(t, _)| search_key(t)).collect();
    let mut out = Vec::new();
    filter_keys(&keys, &query.to_lowercase(), &mut out);
    out
}

/// Popup list scroll bar width (the blockscroll arrow pieces are 14px wide).
const POPUP_BAR: i32 = 14;
const SCROLL_SKIN: &str = "base/client/ui/blockscroll";

#[derive(Debug, Clone)]
pub struct View {
    pub nodes: Vec<Node>,
    pub root: NodeId,
    pub names: HashMap<String, NodeId>,
    pub hover: Option<NodeId>,
    pub pressed: Option<(NodeId, MouseButton)>,
    pub focus: Option<NodeId>,
    popup: Option<Popup>,
    /// What the player typed into the open dropdown (type-to-filter).
    popup_query: String,
    /// `search_key` of each item of the open dropdown.
    popup_keys: Vec<String>,
    /// Item indices the open dropdown shows, in order; `Popup::hover` and
    /// `Popup::scroll` index this.
    popup_shown: Vec<usize>,
    /// Scroll control whose thumb is being dragged: (control, grab offset,
    /// up arrow height, down arrow height).
    scroll_drag: Option<(NodeId, i32, i32, i32)>,
    /// Window being dragged by its title bar: (window, last mouse x, y).
    window_drag: Option<(NodeId, i32, i32)>,
    /// Window being resized by its right and/or bottom edge: (window, last
    /// mouse x, y, width, height).
    window_resize: Option<(NodeId, i32, i32, bool, bool)>,
    last_click: Option<(NodeId, u64)>,
    pub time_ms: u64,
    canvas: (i32, i32),
    /// Last mouse position (logical pixels).
    pub mouse: (i32, i32),
    close_hot: bool,
}

/// How close to a resizable window's right or bottom edge a press resizes it.
const RESIZE_EDGE: i32 = 6;

/// `r` moved as little as possible to lie inside `within` (its top-left
/// corner stays visible when it is the larger).
fn keep_inside(r: Rect, within: Rect) -> Rect {
    let x = r.x.min(within.right() - r.w).max(within.x);
    let y = r.y.min(within.bottom() - r.h).max(within.y);
    Rect::new(x, y, r.w, r.h)
}

fn authored_rect(c: &Control) -> Rect {
    Rect::new(c.position[0], c.position[1], c.extent[0], c.extent[1])
}

fn initial_value(c: &Control) -> Value {
    match c.class.as_str() {
        "GuiCheckBoxCtrl" | "GuiRadioCtrl" => {
            Value::Bool(c.field("value").is_some_and(|v| v == "1"))
        }
        "GuiSliderCtrl" => Value::Num(c.field("value").and_then(|v| v.parse().ok()).unwrap_or(0.0)),
        "GuiTextEditCtrl" | "GuiMLTextEditCtrl" => Value::Text(c.text.clone().unwrap_or_default()),
        "GuiPopUpMenuCtrl" | "GuiTextListCtrl" => Value::Selected(None),
        "GuiProgressCtrl" | "GuiHealthBarHud" => Value::Num(0.0),
        _ => Value::None,
    }
}

impl View {
    pub fn new(layout: &Control) -> View {
        let mut v = View {
            nodes: Vec::new(),
            root: 0,
            names: HashMap::new(),
            hover: None,
            pressed: None,
            focus: None,
            popup: None,
            popup_query: String::new(),
            popup_keys: Vec::new(),
            popup_shown: Vec::new(),
            scroll_drag: None,
            window_drag: None,
            window_resize: None,
            last_click: None,
            time_ms: 0,
            canvas: (640, 480),
            mouse: (-1, -1),
            close_hot: false,
        };
        v.root = v.insert(layout, None);
        v
    }

    fn insert(&mut self, c: &Control, parent: Option<NodeId>) -> NodeId {
        let id = self.nodes.len();
        let mut ctrl = c.clone();
        ctrl.children = Vec::new();
        self.nodes.push(Node {
            state: NodeState {
                visible: c.visible,
                active: true,
                text: None,
                bitmap: None,
                external_texture: None,
                tint: None,
                value: initial_value(c),
                items: Vec::new(),
                scroll_y: 0,
                cursor: 0,
                frame: 0,
                row_height: 16,
                moved: (0, 0),
                resized: (0, 0),
                maximized: false,
                minimized: None,
            },
            ctrl,
            parent,
            children: Vec::new(),
            rect: authored_rect(c),
        });
        if let Some(n) = &c.name {
            self.names.entry(n.clone()).or_insert(id);
        }
        for ch in &c.children {
            let cid = self.insert(ch, Some(id));
            self.nodes[id].children.push(cid);
        }
        id
    }

    /// Add a runtime-created control (script-built HUD, brick tiles, …).
    pub fn add(&mut self, parent: NodeId, c: Control) -> NodeId {
        let id = self.insert(&c, Some(parent));
        self.nodes[parent].children.push(id);
        id
    }

    /// Remove all children of `parent` (nodes stay allocated but detached).
    pub fn clear_children(&mut self, parent: NodeId) {
        let kids = std::mem::take(&mut self.nodes[parent].children);
        for k in kids {
            self.detach(k);
        }
    }

    fn detach(&mut self, id: NodeId) {
        let kids = std::mem::take(&mut self.nodes[id].children);
        for k in kids {
            self.detach(k);
        }
        self.nodes[id].parent = None;
        self.nodes[id].state.visible = false;
        if let Some(n) = self.nodes[id].ctrl.name.clone()
            && self.names.get(&n) == Some(&id)
        {
            self.names.remove(&n);
        }
        if self.focus == Some(id) {
            self.focus = None;
        }
        if self.hover == Some(id) {
            self.hover = None;
        }
    }

    /// Move a child to the end of its parent's list (drawn last / on top),
    /// like Torque `pushToBack`.
    pub fn push_to_back(&mut self, id: NodeId) {
        if let Some(p) = self.nodes[id].parent {
            let kids = &mut self.nodes[p].children;
            kids.retain(|k| *k != id);
            kids.push(id);
        }
    }

    pub fn id(&self, name: &str) -> Option<NodeId> {
        self.names.get(name).copied()
    }
    /// Find a control by its original command string.
    pub fn by_command(&self, command: &str) -> Option<NodeId> {
        self.walk()
            .find(|&n| self.nodes[n].ctrl.command.as_deref() == Some(command))
    }
    /// Find a control by displayed/authored text within a class.
    pub fn by_text(&self, class: &str, text: &str) -> Option<NodeId> {
        self.walk()
            .find(|&n| self.nodes[n].ctrl.class == class && self.text_of(n) == text)
    }
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }
    pub fn state(&mut self, id: NodeId) -> &mut NodeState {
        &mut self.nodes[id].state
    }
    pub fn text_of(&self, id: NodeId) -> String {
        let n = &self.nodes[id];
        n.state
            .text
            .clone()
            .or_else(|| n.ctrl.text.clone())
            .unwrap_or_default()
    }
    pub fn set_text(&mut self, id: NodeId, t: impl Into<String>) {
        let t = t.into();
        let n = &mut self.nodes[id];
        if matches!(n.state.value, Value::Text(_)) {
            n.state.cursor = t.chars().count();
            n.state.value = Value::Text(t.clone());
        }
        n.state.text = Some(t);
    }
    pub fn set_visible(&mut self, id: NodeId, v: bool) {
        self.nodes[id].state.visible = v;
    }
    pub fn set_active(&mut self, id: NodeId, v: bool) {
        self.nodes[id].state.active = v;
    }
    pub fn bool_value(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].state.value, Value::Bool(true))
    }
    pub fn set_bool(&mut self, id: NodeId, v: bool) {
        self.nodes[id].state.value = Value::Bool(v);
    }
    pub fn edit_text(&self, id: NodeId) -> String {
        match &self.nodes[id].state.value {
            Value::Text(t) => t.clone(),
            _ => self.text_of(id),
        }
    }
    pub fn selected(&self, id: NodeId) -> Option<i64> {
        match self.nodes[id].state.value {
            Value::Selected(s) => s,
            _ => None,
        }
    }
    pub fn select(&mut self, id: NodeId, sel: Option<i64>) {
        self.nodes[id].state.value = Value::Selected(sel);
    }
    pub fn selected_text(&self, id: NodeId) -> Option<String> {
        let s = self.selected(id)?;
        self.nodes[id]
            .state
            .items
            .iter()
            .find(|(_, i)| *i == s)
            .map(|(t, _)| t.clone())
    }
    pub fn num(&self, id: NodeId) -> f32 {
        match self.nodes[id].state.value {
            Value::Num(v) => v,
            _ => 0.0,
        }
    }
    pub fn set_num(&mut self, id: NodeId, v: f32) {
        self.nodes[id].state.value = Value::Num(v);
    }

    /// Visible = this node and all ancestors visible.
    pub fn is_shown(&self, mut id: NodeId) -> bool {
        loop {
            let n = &self.nodes[id];
            if !n.state.visible {
                return false;
            }
            match n.parent {
                Some(p) => id = p,
                None => return id == self.root,
            }
        }
    }

    /// Depth-first ids in tree (draw) order.
    pub fn walk(&self) -> impl Iterator<Item = NodeId> + '_ {
        let mut stack = vec![self.root];
        std::iter::from_fn(move || {
            let id = stack.pop()?;
            stack.extend(self.nodes[id].children.iter().rev());
            Some(id)
        })
    }

    // ---------------------------------------------------------------- layout

    /// Lay out for a logical canvas. The root always fills the canvas
    /// (GuiCanvas::maintainSizing); children follow GuiControl::parentResized
    /// from their authored extents.
    pub fn layout(&mut self, w: i32, h: i32) {
        self.canvas = (w, h);
        let root = self.root;
        let authored = authored_rect(&self.nodes[root].ctrl);
        self.nodes[root].rect = Rect::new(0, 0, w, h);
        self.layout_children(root, (authored.w, authored.h), Rect::new(0, 0, w, h));
    }

    /// Lay out again for the current canvas (after content changed size).
    pub fn relayout(&mut self) {
        let (w, h) = self.canvas;
        self.layout(w, h);
    }

    fn layout_children(&mut self, id: NodeId, old_parent: (i32, i32), parent_rect: Rect) {
        let is_scroll = self.nodes[id].ctrl.class == "GuiScrollCtrl";
        let scroll_y = self.nodes[id].state.scroll_y;
        let kids = self.nodes[id].children.clone();
        for k in kids {
            let c = &self.nodes[k].ctrl;
            let a = authored_rect(c);
            let r = resize(
                a,
                c.h_sizing,
                c.v_sizing,
                c.min_extent,
                old_parent,
                (parent_rect.w, parent_rect.h),
            );
            let mut abs = r.offset(parent_rect.x, parent_rect.y);
            let (moved, resized) = (self.nodes[k].state.moved, self.nodes[k].state.resized);
            if resized != (0, 0) {
                abs.w = (abs.w + resized.0).min(parent_rect.w);
                abs.h = (abs.h + resized.1).min(parent_rect.h);
            }
            if moved != (0, 0) || resized != (0, 0) {
                abs = keep_inside(abs.offset(moved.0, moved.1), parent_rect);
            }
            if self.nodes[k].state.maximized {
                abs = parent_rect;
            } else if let Some(title) = self.nodes[k].state.minimized {
                abs.h = title;
            }
            if is_scroll {
                abs = abs.offset(0, -scroll_y);
            }
            // Like Torque's GuiTextListCtrl::setSize, a list grows to hold every
            // row; its scroll parent clips and scrolls it. Otherwise rows past
            // the authored extent are clipped away and cannot be reached.
            if self.nodes[k].ctrl.class == "GuiTextListCtrl" {
                abs.h = abs.h.max(self.list_height(k));
            }
            // GuiConsole sizes itself to its log: one row per line, at least
            // as wide as the scroll area it sits in.
            if self.nodes[k].ctrl.class == "GuiConsole" {
                abs.h = self.list_height(k);
                abs.w = abs.w.max(parent_rect.w - a.x);
            }
            self.nodes[k].rect = abs;
            self.layout_children(k, (a.w, a.h), abs);
        }
    }

    // ------------------------------------------------------------- rendering

    pub fn draw(&self, pack: &Pack, dl: &mut DrawList) {
        self.draw_node(pack, dl, self.root);
        if let Some(p) = &self.popup {
            self.draw_popup_list(pack, dl, p);
        }
    }

    fn style<'a>(&self, pack: &'a Pack, id: NodeId) -> Option<&'a Style> {
        pack.data.styles.get(&self.nodes[id].ctrl.style)
    }

    /// Profile font, falling back to GuiDefaultProfile's and then Arial 14
    /// (Torque's GuiControlProfile defaults for profiles without fontType).
    fn font_id<'a>(pack: &'a Pack, style: &'a Style) -> Option<&'a str> {
        style
            .font
            .as_deref()
            .or_else(|| {
                pack.data
                    .styles
                    .get("GuiDefaultProfile")
                    .and_then(|d| d.font.as_deref())
            })
            .or_else(|| {
                pack.data
                    .fonts
                    .contains_key("arial_14")
                    .then_some("arial_14")
            })
    }

    fn profile_font<'a>(pack: &'a Pack, profile: &str) -> Option<Font<'a>> {
        let style = pack.data.styles.get(profile)?;
        Font::get(pack, Self::font_id(pack, style)?)
    }

    /// Height a `GuiMLTextCtrl` of this profile reflows `text` to at
    /// `width`, line by line as `draw_ml` lays it out.
    pub fn ml_height(pack: &Pack, profile: &str, text: &str, width: i32) -> i32 {
        let Some(font) = Self::profile_font(pack, profile) else {
            return 0;
        };
        text::layout_ml(pack, &font, text, width, Justify::Left)
            .iter()
            .map(|line| {
                text::ml_rich_runs(&line.text)
                    .iter()
                    .filter_map(|run| match run {
                        text::MlRun::Bitmap(id) => pack.image_size(id),
                        _ => None,
                    })
                    .map(|(_, h)| h as i32)
                    .fold(line.height, i32::max)
            })
            .sum()
    }

    /// `getPixelWidth`: the width of a control's text in its profile font.
    pub fn pixel_width(&self, pack: &Pack, id: NodeId) -> i32 {
        Self::profile_font(pack, &self.nodes[id].ctrl.style)
            .map_or(0, |font| font.width(&self.text_of(id)))
    }

    /// Line height of a control's profile font.
    pub fn line_height(&self, pack: &Pack, id: NodeId) -> i32 {
        Self::profile_font(pack, &self.nodes[id].ctrl.style).map_or(0, |font| font.line_height())
    }

    fn draw_node(&self, pack: &Pack, dl: &mut DrawList, id: NodeId) {
        let n = &self.nodes[id];
        if !n.state.visible {
            return;
        }
        if !dl.push_clip(self.bounds(id)) {
            return;
        }
        self.draw_self(pack, dl, id);
        let skip_children = matches!(n.ctrl.class.as_str(), "GuiShapeNameHud");
        if !skip_children {
            if n.ctrl.class == "GuiScrollCtrl" {
                let inner = self.scroll_content_rect(pack, id);
                if dl.push_clip(inner) {
                    for &k in &n.children {
                        self.draw_node(pack, dl, k);
                    }
                    dl.pop_clip();
                }
            } else {
                for &k in &n.children {
                    self.draw_node(pack, dl, k);
                }
            }
        }
        dl.pop_clip();
    }

    fn skin_piece(&self, pack: &Pack, image: &str, idx: usize) -> Option<[f32; 4]> {
        let s = pack.data.skins.get(image)?;
        let p = s.pieces.get(idx)?;
        Some([p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32])
    }

    /// Full-screen backgrounds fill the canvas by cropping, never by
    /// stretching, so screenshots keep their aspect on any window shape.
    fn blit_cover(&self, pack: &Pack, dl: &mut DrawList, image: &str, r: Rect, tint: Rgba) {
        if let Some((w, h)) = pack.image_size(image) {
            dl.image(
                TexKey::Image(image.to_string()),
                cover_src((w as f32, h as f32), (r.w as f32, r.h as f32)),
                [r.x as f32, r.y as f32, r.w as f32, r.h as f32],
                tint,
                Filter::Linear,
            );
        }
    }

    fn blit(&self, pack: &Pack, dl: &mut DrawList, image: &str, r: Rect, tint: Rgba) {
        if let Some((w, h)) = pack.image_size(image) {
            dl.image(
                TexKey::Image(image.to_string()),
                [0.0, 0.0, w as f32, h as f32],
                [r.x as f32, r.y as f32, r.w as f32, r.h as f32],
                tint,
                Filter::Linear,
            );
        }
    }

    fn blit_tiled(&self, pack: &Pack, dl: &mut DrawList, image: &str, r: Rect, tint: Rgba) {
        let Some((w, h)) = pack.image_size(image) else {
            return;
        };
        if !dl.push_clip(r) {
            return;
        }
        let (w, h) = (w as i32, h as i32);
        let mut y = r.y;
        while y < r.bottom() {
            let mut x = r.x;
            while x < r.right() {
                dl.image(
                    TexKey::Image(image.to_string()),
                    [0.0, 0.0, w as f32, h as f32],
                    [x as f32, y as f32, w as f32, h as f32],
                    tint,
                    Filter::Linear,
                );
                x += w;
            }
            y += h;
        }
        dl.pop_clip();
    }

    fn piece(&self, dl: &mut DrawList, image: &str, src: [f32; 4], dst: Rect) {
        dl.image(
            TexKey::Image(image.to_string()),
            src,
            [dst.x as f32, dst.y as f32, dst.w as f32, dst.h as f32],
            WHITE,
            Filter::Nearest,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_in(
        &self,
        pack: &Pack,
        dl: &mut DrawList,
        id: NodeId,
        r: Rect,
        text: &str,
        justify: Option<Justify>,
        color: Option<Rgba>,
    ) {
        self.draw_text_outlined(pack, dl, id, r, text, justify, color, true);
    }

    /// `draw_text_in`, with the profile's `doFontOutline` only when
    /// `outline` (Blockland outlines labels and chat, not list rows).
    #[allow(clippy::too_many_arguments)]
    fn draw_text_outlined(
        &self,
        pack: &Pack,
        dl: &mut DrawList,
        id: NodeId,
        r: Rect,
        text: &str,
        justify: Option<Justify>,
        color: Option<Rgba>,
        outline: bool,
    ) {
        let Some(style) = self.style(pack, id) else {
            return;
        };
        let Some(fid) = Self::font_id(pack, style) else {
            return;
        };
        let Some(font) = Font::get(pack, fid) else {
            return;
        };
        let just = justify.unwrap_or(style.justify);
        let color =
            color.unwrap_or_else(|| text::style_color(style, false, !self.nodes[id].state.active));
        let lines: Vec<&str> = text.split('\n').collect();
        let total = font.line_height() * lines.len() as i32;
        let mut y = r.y + (r.h - total) / 2;
        for line in lines {
            let w = font.width(line);
            let x = match just {
                Justify::Left => r.x,
                Justify::Center => r.x + (r.w - w) / 2,
                Justify::Right => r.right() - w,
            };
            font.draw_outlined(
                dl,
                x as f32,
                y as f32,
                line,
                color,
                style.font_outline.filter(|_| outline),
                &style.font_colors,
            );
            y += font.line_height();
        }
    }

    fn draw_ml(&self, pack: &Pack, dl: &mut DrawList, id: NodeId, r: Rect, text: &str) {
        let Some(style) = self.style(pack, id) else {
            return;
        };
        let Some(fid) = Self::font_id(pack, style) else {
            return;
        };
        let Some(font) = Font::get(pack, fid) else {
            return;
        };
        // A runtime tint stands in for a profile whose `fontColor` Torque
        // aliases to a later `fontColors[0]` (see `play::chat_base_color`).
        let color = self.nodes[id]
            .state
            .tint
            .or(style.font_color)
            .unwrap_or(geom::BLACK);
        let mut y = r.y;
        // Each line starts in the profile's font and colour; the line's own
        // markers (carried from the line before) switch them.
        for line in text::layout_ml(pack, &font, text, r.w, Justify::Left) {
            let runs = text::ml_rich_runs(&line.text);
            let bitmap_width: i32 = runs
                .iter()
                .filter_map(|run| match run {
                    text::MlRun::Bitmap(id) => pack.image_size(id),
                    _ => None,
                })
                .map(|(w, _)| w as i32)
                .sum();
            let width = line.width + bitmap_width;
            let left = r.x + line.indent;
            let mut x = match line.justify {
                Justify::Left => left,
                Justify::Center => left + (r.w - line.indent - width) / 2,
                Justify::Right => r.right() - width,
            };
            let mut height = line.height;
            let (mut run_font, mut run_color) = (font, color);
            for run in runs {
                match run {
                    text::MlRun::Bitmap(image) => {
                        if let Some((w, h)) = pack.image_size(image) {
                            dl.image(
                                TexKey::Image(image.to_string()),
                                [0.0, 0.0, w as f32, h as f32],
                                [x as f32, y as f32, w as f32, h as f32],
                                geom::WHITE,
                                crate::draw::Filter::Linear,
                            );
                            x += w as i32;
                            height = height.max(h as i32);
                        }
                    }
                    text::MlRun::Font(id) => {
                        if let Some(f) = text::font_named(pack, id) {
                            run_font = f;
                        }
                    }
                    text::MlRun::Rgb(rgb) => run_color = rgb,
                    text::MlRun::Text(run) => {
                        // Glyphs sit on the line's baseline.
                        let top = y + line.height - run_font.line_height();
                        run_font.draw_outlined(
                            dl,
                            x as f32,
                            top as f32,
                            run,
                            run_color,
                            style.font_outline,
                            &style.font_colors,
                        );
                        x += run_font.width(run);
                    }
                }
            }
            y += height;
        }
    }

    fn draw_self(&self, pack: &Pack, dl: &mut DrawList, id: NodeId) {
        let n = &self.nodes[id];
        let r = n.rect;
        let style = self.style(pack, id);
        let hovered = self.hover == Some(id);
        let down = matches!(self.pressed, Some((p, MouseButton::Left)) if p == id) && hovered;
        let text = self.text_of(id);
        match n.ctrl.class.as_str() {
            "GuiWindowCtrl" => self.draw_window(pack, dl, id),
            "GuiBitmapCtrl"
            | "GuiChunkedBitmapCtrl"
            | "GuiFadeinBitmapCtrl"
            | "GuiCrossHairHud" => {
                let bmp = n.state.bitmap.clone().or_else(|| n.ctrl.bitmap.clone());
                if let Some(id) = n.state.external_texture {
                    dl.image(
                        TexKey::External(id),
                        [0.0, 0.0, 1.0, 1.0],
                        [r.x as f32, r.y as f32, r.w as f32, r.h as f32],
                        n.state.tint.unwrap_or(WHITE),
                        Filter::Linear,
                    );
                } else if let Some(b) = bmp {
                    let tint = n.state.tint.unwrap_or(WHITE);
                    if n.ctrl.field("wrap") == Some("1") {
                        self.blit_tiled(pack, dl, &b, r, tint);
                    } else if r == Rect::new(0, 0, self.canvas.0, self.canvas.1) {
                        self.blit_cover(pack, dl, &b, r, tint);
                    } else {
                        self.blit(pack, dl, &b, r, tint);
                    }
                }
            }
            "GuiAnimatedBitmapCtrl" => {
                if let Some(b) = &n.ctrl.bitmap {
                    self.blit(pack, dl, &format!("{b}_{:02}", n.state.frame), r, WHITE);
                }
            }
            "GuiBitmapButtonCtrl" => {
                let base = n.state.bitmap.clone().or_else(|| n.ctrl.bitmap.clone());
                if let Some(b) = base {
                    let st = if !n.state.active {
                        "_i"
                    } else if down {
                        "_d"
                    } else if hovered {
                        "_h"
                    } else {
                        "_n"
                    };
                    let img = [format!("{b}{st}"), format!("{b}_n"), b.clone()]
                        .into_iter()
                        .find(|i| pack.has_image(i));
                    if let Some(img) = img {
                        let tint = n.state.tint.or(n.ctrl.color).unwrap_or(WHITE);
                        self.blit(pack, dl, &img, r, tint);
                    }
                }
                if !text.trim().is_empty()
                    && let Some(s) = style
                {
                    let color = text::style_color(s, hovered && n.state.active, !n.state.active);
                    self.draw_text_in(pack, dl, id, r, &text, Some(Justify::Center), Some(color));
                }
            }
            "GuiButtonCtrl" => {
                if let Some(s) = style {
                    let fill = if down { s.fill_color_hl } else { s.fill_color }
                        .unwrap_or([200, 200, 200, 255]);
                    dl.fill(r, fill);
                    dl.frame(r, s.border_color.unwrap_or(geom::BLACK));
                    self.draw_text_in(pack, dl, id, r, &text, Some(Justify::Center), None);
                }
            }
            "GuiSwatchCtrl" => {
                if let Some(c) = n.state.tint.or(n.ctrl.color) {
                    dl.fill(r, c);
                }
            }
            "GuiTextCtrl" => {
                if style.is_some() {
                    self.draw_text_in(pack, dl, id, r, &text, None, None);
                }
            }
            "GuiMLTextCtrl" => self.draw_ml(pack, dl, id, r, &text),
            "GuiTextEditCtrl" | "GuiMLTextEditCtrl" => {
                if let Some(s) = style {
                    if s.opaque {
                        dl.fill(r, s.fill_color.unwrap_or(WHITE));
                    }
                    if s.border != 0 {
                        dl.frame(r, s.border_color.unwrap_or(geom::BLACK));
                    }
                    let t = self.edit_text(id);
                    let shown = if n.ctrl.field("password") == Some("1") {
                        "*".repeat(t.chars().count())
                    } else {
                        t
                    };
                    let inner = Rect::new(r.x + s.text_offset[0] + 2, r.y, r.w - 4, r.h);
                    let font = Self::font_id(pack, s).and_then(|f| Font::get(pack, f));
                    let before: String = shown.chars().take(n.state.cursor).collect();
                    // Like GuiTextEditCtrl, text slides left once the caret
                    // passes the right edge, so long input stays editable.
                    let scroll = match &font {
                        Some(f) if n.ctrl.class == "GuiTextEditCtrl" => {
                            (f.width(&before) - (inner.w - 2)).max(0)
                        }
                        _ => 0,
                    };
                    if n.ctrl.class == "GuiMLTextEditCtrl" {
                        self.draw_ml(pack, dl, id, inner.offset(0, 2), &shown);
                    } else if scroll > 0 {
                        if dl.push_clip(inner) {
                            let moved =
                                Rect::new(inner.x - scroll, inner.y, inner.w + scroll, inner.h);
                            self.draw_text_in(
                                pack,
                                dl,
                                id,
                                moved,
                                &shown,
                                Some(Justify::Left),
                                None,
                            );
                            dl.pop_clip();
                        }
                    } else {
                        self.draw_text_in(pack, dl, id, inner, &shown, Some(Justify::Left), None);
                    }
                    if self.focus == Some(id)
                        && (self.time_ms / 500).is_multiple_of(2)
                        && let Some(f) = font
                    {
                        let cx = inner.x - scroll + f.width(&before);
                        let lh = f.line_height();
                        dl.fill(
                            Rect::new(cx, r.y + (r.h - lh) / 2, 1, lh),
                            s.font_color.unwrap_or(geom::BLACK),
                        );
                    }
                }
            }
            "GuiCheckBoxCtrl" | "GuiRadioCtrl" => {
                if let Some(s) = style
                    && let Some(bmp) = s.bitmap.as_deref()
                {
                    let on = matches!(n.state.value, Value::Bool(true));
                    let idx = match (n.state.active, on) {
                        (true, false) => 0,
                        (true, true) => 1,
                        (false, false) => 2,
                        (false, true) => 3,
                    };
                    if let Some(src) = self.skin_piece(pack, bmp, idx) {
                        let (pw, ph) = (src[2] as i32, src[3] as i32);
                        self.piece(dl, bmp, src, Rect::new(r.x, r.y + (r.h - ph) / 2, pw, ph));
                        let tr = Rect::new(r.x + pw + 3, r.y, r.w - pw - 3, r.h);
                        self.draw_text_in(pack, dl, id, tr, &text, Some(Justify::Left), None);
                    }
                }
            }
            "GuiPopUpMenuCtrl" => {
                if let Some(s) = style {
                    dl.fill(
                        r,
                        if hovered {
                            s.fill_color_hl
                        } else {
                            s.fill_color
                        }
                        .unwrap_or([149, 152, 166, 255]),
                    );
                    dl.frame(r, s.border_color.unwrap_or(geom::BLACK));
                    let label = self.selected_text(id).unwrap_or_else(|| text.clone());
                    let inner = Rect::new(r.x + 4, r.y, r.w - 20, r.h);
                    let color = s.font_color.unwrap_or(geom::BLACK);
                    let searching = self.popup.is_some_and(|p| p.node == id);
                    if searching && dl.push_clip(inner) {
                        // Type-to-filter: the typed text, the rest of the
                        // highlighted match as grey ghost text, and a caret.
                        let typed = &self.popup_query;
                        let ghost = self.popup_ghost().unwrap_or_default();
                        let font = Self::font_id(pack, s).and_then(|f| Font::get(pack, f));
                        let tw = font.as_ref().map_or(0, |f| f.width(typed));
                        let grey = s.font_color_na.unwrap_or([128, 128, 128, 255]);
                        if typed.is_empty() {
                            // The current choice, greyed like a placeholder:
                            // typing replaces it.
                            self.draw_text_in(
                                pack,
                                dl,
                                id,
                                inner,
                                &label,
                                Some(Justify::Left),
                                Some(grey),
                            );
                        } else {
                            self.draw_text_in(
                                pack,
                                dl,
                                id,
                                inner,
                                typed,
                                Some(Justify::Left),
                                Some(color),
                            );
                            let rest =
                                Rect::new(inner.x + tw, inner.y, (inner.w - tw).max(0), inner.h);
                            self.draw_text_in(
                                pack,
                                dl,
                                id,
                                rest,
                                &ghost,
                                Some(Justify::Left),
                                Some(grey),
                            );
                        }
                        if (self.time_ms / 500).is_multiple_of(2)
                            && let Some(f) = font
                        {
                            let lh = f.line_height();
                            dl.fill(Rect::new(inner.x + tw, r.y + (r.h - lh) / 2, 1, lh), color);
                        }
                        dl.pop_clip();
                    } else if dl.push_clip(inner) {
                        self.draw_text_in(
                            pack,
                            dl,
                            id,
                            inner,
                            &label,
                            Some(Justify::Left),
                            Some(color),
                        );
                        dl.pop_clip();
                    }
                    self.draw_scroll_arrow(
                        pack,
                        dl,
                        Rect::new(r.right() - 16, r.y + (r.h - 14) / 2, 14, 14),
                        scroll::DOWN,
                    );
                }
            }
            "GuiScrollCtrl" => self.draw_scroll(pack, dl, id),
            "GuiTextListCtrl" => self.draw_list(pack, dl, id),
            "GuiConsole" => self.draw_console(pack, dl, id),
            "GuiSliderCtrl" => {
                let (lo, hi) = self.range(id);
                let v = self.num(id);
                let t = if hi > lo {
                    ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let mid = r.y + r.h / 2;
                dl.fill(Rect::new(r.x + 4, mid - 1, r.w - 8, 2), geom::BLACK);
                if let Some(ticks) = n
                    .ctrl
                    .field("ticks")
                    .and_then(|t| t.parse::<i32>().ok())
                    .filter(|t| *t > 0)
                {
                    for i in 0..=ticks + 1 {
                        let x = r.x + 4 + (r.w - 8) * i / (ticks + 1);
                        dl.fill(Rect::new(x, mid + 3, 1, 3), geom::BLACK);
                    }
                }
                let tx = r.x + 4 + ((r.w - 8) as f32 * t) as i32;
                let thumb = Rect::new(tx - 4, mid - 8, 8, 16);
                dl.fill(thumb, [149, 152, 166, 255]);
                dl.frame(thumb, geom::BLACK);
            }
            // Torque GuiHealthBarHud: background fill, the value bar in
            // damageFillColor, then the frame.
            "GuiHealthBarHud" => {
                let color = |field: &str| -> Option<Rgba> {
                    let c: Vec<u8> = n
                        .ctrl
                        .field(field)?
                        .split_whitespace()
                        .filter_map(|x| x.parse::<f32>().ok())
                        .map(|x| (x.clamp(0.0, 1.0) * 255.0) as u8)
                        .collect();
                    c.try_into().ok()
                };
                let on = |field: &str| n.ctrl.field(field).is_some_and(|v| v == "1");
                if on("showFill")
                    && let Some(c) = color("fillColor")
                {
                    dl.fill(r, c);
                }
                let f = self.num(id).clamp(0.0, 1.0);
                let bar = if on("flipped") {
                    let w = (r.w as f32 * f) as i32;
                    Rect::new(r.x + r.w - w, r.y, w, r.h)
                } else {
                    Rect::new(r.x, r.y, (r.w as f32 * f) as i32, r.h)
                };
                if let Some(c) = color("damageFillColor") {
                    dl.fill(bar, c);
                }
                if on("showFrame")
                    && let Some(c) = color("frameColor")
                {
                    dl.frame(r, c);
                }
            }
            "GuiProgressCtrl" => {
                if let Some(s) = style {
                    let f = self.num(id).clamp(0.0, 1.0);
                    dl.fill(
                        Rect::new(r.x, r.y, (r.w as f32 * f) as i32, r.h),
                        s.fill_color.unwrap_or([0, 0, 128, 128]),
                    );
                    if s.border != 0 {
                        dl.frame(r, s.border_color.unwrap_or(geom::BLACK));
                    }
                }
            }
            _ => {
                // Containers (GuiControl, GameTSCtrl, GuiObjectView host views …)
                // draw only an opaque fill when their profile asks for one.
                if let Some(s) = style
                    && s.opaque
                    && n.ctrl.class == "GuiControl"
                    && n.parent.is_some()
                {
                    dl.fill(r, s.fill_color.unwrap_or([200, 200, 200, 255]));
                }
            }
        }
    }

    fn draw_window(&self, pack: &Pack, dl: &mut DrawList, id: NodeId) {
        let n = &self.nodes[id];
        let r = n.rect;
        let Some(style) = self.style(pack, id) else {
            return;
        };
        let Some(img) = style.bitmap.as_deref() else {
            dl.fill(r, style.fill_color.unwrap_or([200, 200, 200, 255]));
            return;
        };
        let Some(skin) = pack.data.skins.get(img) else {
            return;
        };
        if skin.pieces.len() < 23 {
            return;
        }
        let p = |i: usize| {
            let q = skin.pieces[i];
            (
                [q[0] as f32, q[1] as f32, q[2] as f32, q[3] as f32],
                q[2] as i32,
                q[3] as i32,
            )
        };
        let (tl, tlw, tlh) = p(win::TOP_LEFT);
        let (tr, trw, trh) = p(win::TOP_RIGHT);
        let (t, _, th) = p(win::TOP);
        let (l, lw, _) = p(win::LEFT);
        let (rt, rw, _) = p(win::RIGHT);
        let (bl, blw, blh) = p(win::BOTTOM_LEFT);
        let (b, _, bh) = p(win::BOTTOM);
        let (br, brw, brh) = p(win::BOTTOM_RIGHT);
        dl.fill(
            Rect::new(r.x + lw, r.y + th, r.w - lw - rw, r.h - th - bh),
            style.fill_color.unwrap_or([200, 200, 200, 255]),
        );
        self.piece(dl, img, tl, Rect::new(r.x, r.y, tlw, tlh));
        self.piece(dl, img, tr, Rect::new(r.right() - trw, r.y, trw, trh));
        self.piece(dl, img, t, Rect::new(r.x + tlw, r.y, r.w - tlw - trw, th));
        self.piece(dl, img, l, Rect::new(r.x, r.y + tlh, lw, r.h - tlh - blh));
        self.piece(
            dl,
            img,
            rt,
            Rect::new(r.right() - rw, r.y + trh, rw, r.h - trh - brh),
        );
        self.piece(dl, img, bl, Rect::new(r.x, r.bottom() - blh, blw, blh));
        self.piece(
            dl,
            img,
            br,
            Rect::new(r.right() - brw, r.bottom() - brh, brw, brh),
        );
        self.piece(
            dl,
            img,
            b,
            Rect::new(r.x + blw, r.bottom() - bh, r.w - blw - brw, bh),
        );
        if n.ctrl.field("canClose") != Some("0") {
            let state = if self.pressed.map(|p| p.0) == Some(id) && self.hover_close(id) {
                2
            } else if self.hover == Some(id) && self.hover_close(id) {
                1
            } else {
                0
            };
            let (c, cw, ch) = p(win::CLOSE + state);
            self.piece(dl, img, c, self.close_rect(id, cw, ch));
        }
        // Maximize and minimize, left of the close box; each shows Normal
        // (restore) while the window is in its state.
        for (field, slot, on, piece) in [
            ("canMaximize", 1, n.state.maximized, win::MAXIMIZE),
            ("canMinimize", 2, n.state.minimized.is_some(), win::MINIMIZE),
        ] {
            if n.ctrl.field(field) == Some("1") {
                let (c, cw, ch) = p(if on { win::NORMAL } else { piece });
                let r = self.title_button(id, slot);
                self.piece(dl, img, c, Rect::new(r.x, r.y, cw, ch));
            }
        }
        let title = self.text_of(id);
        if let Some(f) = style.font.as_deref().and_then(|f| Font::get(pack, f)) {
            let x = r.x + style.text_offset[0] + 4;
            let y = r.y + style.text_offset[1];
            f.draw_outlined(
                dl,
                x as f32,
                y as f32,
                &title,
                style.font_color.unwrap_or(WHITE),
                style.font_outline,
                &style.font_colors,
            );
        }
    }

    /// The window skin's title bar: its top edge piece.
    fn title_height(&self, pack: &Pack, id: NodeId) -> i32 {
        self.style(pack, id)
            .and_then(|s| s.bitmap.as_deref())
            .and_then(|img| pack.data.skins.get(img))
            .and_then(|skin| skin.pieces.get(win::TOP))
            .map_or(20, |q| q[3] as i32)
    }

    /// Move a window by (dx, dy), keeping it inside its parent (the screen).
    fn drag_window(&mut self, id: NodeId, dx: i32, dy: i32) {
        let r = self.nodes[id].rect;
        let parent = self.nodes[id]
            .parent
            .map_or(Rect::new(0, 0, self.canvas.0, self.canvas.1), |p| {
                self.nodes[p].rect
            });
        let to = keep_inside(r.offset(dx, dy), parent);
        let moved = &mut self.nodes[id].state.moved;
        moved.0 += to.x - r.x;
        moved.1 += to.y - r.y;
        self.relayout();
    }

    /// Grow or shrink a window by (dw, dh). It never shrinks below its
    /// authored size, so the dialog's own layout keeps its room, nor grows
    /// past its parent (the screen); its children follow their sizing flags.
    fn resize_window(&mut self, id: NodeId, dw: i32, dh: i32) {
        let r = self.nodes[id].rect;
        let parent = self.nodes[id]
            .parent
            .map_or(Rect::new(0, 0, self.canvas.0, self.canvas.1), |p| {
                self.nodes[p].rect
            });
        let resized = &mut self.nodes[id].state.resized;
        resized.0 = (resized.0 + dw).clamp(0, (parent.right() - r.x - r.w + resized.0).max(0));
        resized.1 = (resized.1 + dh).clamp(0, (parent.bottom() - r.y - r.h + resized.1).max(0));
        self.relayout();
    }

    /// Title bar box `slot` from the right: 0 close, 1 maximize, 2 minimize.
    fn title_button(&self, id: NodeId, slot: i32) -> Rect {
        let r = self.nodes[id].rect;
        Rect::new(r.right() - 20 - slot * 18, r.y + 3, 16, 16)
    }

    fn close_rect(&self, id: NodeId, cw: i32, ch: i32) -> Rect {
        let r = self.nodes[id].rect;
        Rect::new(r.right() - cw - 4, r.y + 3, cw, ch)
    }

    fn hover_close(&self, _id: NodeId) -> bool {
        self.close_hot
    }

    fn range(&self, id: NodeId) -> (f32, f32) {
        let r: Vec<f32> = self.nodes[id]
            .ctrl
            .field("range")
            .unwrap_or("0 1")
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect();
        if r.len() == 2 {
            (r[0], r[1])
        } else {
            (0.0, 1.0)
        }
    }

    fn scroll_bitmap<'a>(&self, pack: &'a Pack, id: NodeId) -> Option<&'a str> {
        let s = self.style(pack, id)?;
        s.bitmap.as_deref().filter(|b| {
            pack.data
                .skins
                .get(*b)
                .is_some_and(|k| k.pieces.len() >= 18)
        })
    }

    fn scroll_bar_width(&self, pack: &Pack, id: NodeId) -> i32 {
        let n = &self.nodes[id];
        let mode = n.ctrl.field("vScrollBar").unwrap_or("dynamic");
        if mode == "alwaysOff" {
            return 0;
        }
        if mode == "dynamic" && self.content_height(id) <= n.rect.h {
            return 0;
        }
        self.scroll_bitmap(pack, id)
            .and_then(|b| self.skin_piece(pack, b, scroll::UP * scroll::STATES))
            .map_or(12, |p| p[2] as i32)
    }

    /// Heights of the scroll bar's up and down arrows.
    fn scroll_arrows(&self, pack: &Pack, id: NodeId) -> (i32, i32) {
        let part = |p: usize| {
            self.scroll_bitmap(pack, id)
                .and_then(|b| self.skin_piece(pack, b, p * scroll::STATES))
                .map_or(12, |r| r[3] as i32)
        };
        (part(scroll::UP), part(scroll::DOWN))
    }

    fn scroll_step(&self, id: NodeId) -> i32 {
        self.nodes[id]
            .ctrl
            .field("rowHeight")
            .and_then(|r| r.parse::<i32>().ok())
            .filter(|r| *r > 0)
            .unwrap_or(32)
    }

    pub fn content_height(&self, id: NodeId) -> i32 {
        self.nodes[id]
            .children
            .iter()
            .map(|&k| {
                let c = &self.nodes[k];
                if matches!(c.ctrl.class.as_str(), "GuiTextListCtrl" | "GuiConsole") {
                    c.ctrl.position[1] + self.list_height(k)
                } else {
                    c.ctrl.position[1] + c.rect.h
                }
            })
            .max()
            .unwrap_or(0)
    }

    pub fn scroll_content_rect(&self, pack: &Pack, id: NodeId) -> Rect {
        let r = self.nodes[id].rect;
        Rect::new(r.x, r.y, r.w - self.scroll_bar_width(pack, id), r.h)
    }

    fn draw_scroll_arrow(&self, pack: &Pack, dl: &mut DrawList, r: Rect, part: usize) {
        if let Some(src) = self.skin_piece(pack, SCROLL_SKIN, part * scroll::STATES) {
            self.piece(dl, SCROLL_SKIN, src, r);
        }
    }

    fn draw_scroll(&self, pack: &Pack, dl: &mut DrawList, id: NodeId) {
        let n = &self.nodes[id];
        let r = n.rect;
        if let Some(s) = self.style(pack, id)
            && s.opaque
        {
            dl.fill(r, s.fill_color.unwrap_or(WHITE));
        }
        let bw = self.scroll_bar_width(pack, id);
        let Some(img) = self.scroll_bitmap(pack, id) else {
            return;
        };
        if bw == 0 {
            return;
        }
        self.draw_scrollbar(
            pack,
            dl,
            img,
            Rect::new(r.right() - bw, r.y, bw, r.h),
            self.content_height(id),
            r.h,
            n.state.scroll_y,
        );
    }

    /// Arrows, page track and a thumb sized for `visible` of `content`
    /// pixels scrolled by `offset`.
    #[allow(clippy::too_many_arguments)]
    fn draw_scrollbar(
        &self,
        pack: &Pack,
        dl: &mut DrawList,
        img: &str,
        bar: Rect,
        content: i32,
        visible: i32,
        offset: i32,
    ) {
        let get = |part: usize| self.skin_piece(pack, img, part * scroll::STATES);
        let (Some(up), Some(dn), Some(page)) =
            (get(scroll::UP), get(scroll::DOWN), get(scroll::PAGE))
        else {
            return;
        };
        let (x, bw) = (bar.x, bar.w);
        let uh = up[3] as i32;
        let dh = dn[3] as i32;
        self.piece(dl, img, page, Rect::new(x, bar.y + uh, bw, bar.h - uh - dh));
        self.piece(dl, img, up, Rect::new(x, bar.y, bw, uh));
        self.piece(dl, img, dn, Rect::new(x, bar.bottom() - dh, bw, dh));
        if let Some((ty, th)) = thumb(bar.y + uh, bar.h - uh - dh, content, visible, offset)
            && let (Some(t0), Some(t1), Some(t2)) = (
                get(scroll::THUMB_TOP),
                get(scroll::THUMB),
                get(scroll::THUMB_BOTTOM),
            )
        {
            let (h0, h2) = (t0[3] as i32, t2[3] as i32);
            self.piece(dl, img, t0, Rect::new(x, ty, bw, h0));
            self.piece(
                dl,
                img,
                t1,
                Rect::new(x, ty + h0, bw, (th - h0 - h2).max(0)),
            );
            self.piece(dl, img, t2, Rect::new(x, ty + th - h2, bw, h2));
        }
    }

    fn list_row_height(&self, pack: &Pack, id: NodeId) -> i32 {
        self.style(pack, id)
            .and_then(|s| Self::font_id(pack, s))
            .and_then(|f| pack.font(f))
            .map_or(16, |f| f.line_height as i32 + 2)
    }

    /// Cache every text list's row height from its profile font so layout,
    /// scrolling and hit tests agree with drawing.
    pub fn measure(&mut self, pack: &Pack) {
        for id in 0..self.nodes.len() {
            match self.nodes[id].ctrl.class.as_str() {
                "GuiTextListCtrl" => {
                    self.nodes[id].state.row_height = self.list_row_height(pack, id)
                }
                // GuiConsole cells are exactly one font line tall.
                "GuiConsole" => {
                    self.nodes[id].state.row_height = self.line_height(pack, id).max(1);
                }
                _ => {}
            }
        }
    }

    fn list_height(&self, id: NodeId) -> i32 {
        self.nodes[id].state.items.len() as i32 * self.nodes[id].state.row_height
    }

    /// A control's extent for drawing and hit tests. Text lists grow to hold
    /// every row (GuiTextListCtrl sizes itself to its cells); authored lists
    /// are often only a few pixels tall.
    fn bounds(&self, id: NodeId) -> Rect {
        let n = &self.nodes[id];
        let mut r = n.rect;
        if n.ctrl.class == "GuiTextListCtrl" {
            r.h = r.h.max(self.list_height(id));
        }
        r
    }

    fn draw_list(&self, pack: &Pack, dl: &mut DrawList, id: NodeId) {
        let n = &self.nodes[id];
        let r = n.rect;
        let rh = self.list_row_height(pack, id);
        let Some(style) = self.style(pack, id) else {
            return;
        };
        let cols: Vec<i32> = n
            .ctrl
            .field("columns")
            .unwrap_or("0")
            .split_whitespace()
            .filter_map(|c| c.parse().ok())
            .collect();
        let sel = self.selected(id);
        for (i, (text, item)) in n.state.items.iter().enumerate() {
            let row = Rect::new(r.x, r.y + i as i32 * rh, r.w, rh);
            if Some(*item) == sel {
                dl.fill(row, style.fill_color_hl.unwrap_or([128, 128, 255, 255]));
            }
            for (c, field) in text.split('\t').enumerate() {
                let x = cols.get(c).copied().unwrap_or(0);
                if x >= 9999 {
                    continue;
                }
                let next = cols.get(c + 1).copied().unwrap_or(r.w);
                let cell = Rect::new(r.x + x + 2, row.y, (next - x - 2).max(0), rh);
                // Join Server's ServerListProfile sets doFontOutline (black
                // on black), but v20's text lists drew rows without it.
                if dl.push_clip(cell) {
                    self.draw_text_outlined(
                        pack,
                        dl,
                        id,
                        cell,
                        field,
                        Some(Justify::Left),
                        None,
                        false,
                    );
                    dl.pop_clip();
                }
            }
        }
    }

    /// GuiConsole rows: item ids are log levels (0 normal, 1 warning,
    /// 2 error) drawn in the profile's normal, HL and NA font colours, 3px
    /// in. Only rows inside the scroll area are drawn.
    fn draw_console(&self, pack: &Pack, dl: &mut DrawList, id: NodeId) {
        let n = &self.nodes[id];
        let Some(style) = self.style(pack, id) else {
            return;
        };
        let r = n.rect;
        let rh = n.state.row_height.max(1);
        let visible = n.parent.map_or(r, |p| self.nodes[p].rect);
        let first = ((visible.y - r.y) / rh).max(0) as usize;
        let last = ((visible.bottom() - r.y) / rh + 1).max(0) as usize;
        let black = geom::BLACK;
        for (i, (text, level)) in n.state.items.iter().enumerate().take(last).skip(first) {
            let color = match level {
                1 => style.font_color_hl,
                2 => style.font_color_na,
                _ => style.font_color,
            };
            let row = Rect::new(r.x + 3, r.y + i as i32 * rh, r.w - 3, rh);
            self.draw_text_in(
                pack,
                dl,
                id,
                row,
                text,
                Some(Justify::Left),
                Some(color.unwrap_or(black)),
            );
        }
    }

    fn draw_popup_list(&self, pack: &Pack, dl: &mut DrawList, p: &Popup) {
        let n = &self.nodes[p.node];
        let Some(style) = self.style(pack, p.node) else {
            return;
        };
        let r = p.rect;
        dl.fill(r, WHITE);
        dl.frame(r, geom::BLACK);
        let shown = self.popup_shown.len();
        let bar = p.scroll_bar(shown);
        let text_w = r.w - 2 - bar.map_or(0, |b| b.w);
        let visible = self
            .popup_shown
            .iter()
            .enumerate()
            .skip(p.scroll)
            .take(p.rows);
        for (row, (i, &item)) in visible.enumerate() {
            let Some((text, _)) = n.state.items.get(item) else {
                continue;
            };
            let row = Rect::new(r.x + 1, r.y + 1 + (row as i32) * p.row_h, text_w, p.row_h);
            if Some(i) == p.hover {
                dl.fill(row, style.fill_color_hl.unwrap_or([171, 171, 171, 255]));
            }
            let t = Rect::new(row.x + 3, row.y, row.w - 6, p.row_h);
            if dl.push_clip(t) {
                self.draw_text_in(
                    pack,
                    dl,
                    p.node,
                    t,
                    text,
                    Some(Justify::Left),
                    Some(geom::BLACK),
                );
                dl.pop_clip();
            }
        }
        if let Some(bar) = bar {
            self.draw_scrollbar(
                pack,
                dl,
                SCROLL_SKIN,
                bar,
                shown as i32 * p.row_h,
                p.rows as i32 * p.row_h,
                p.scroll as i32 * p.row_h,
            );
        }
    }

    /// Lay out the list for `id` below the control (above it when the canvas
    /// has no room), at least as wide as its widest item, with the selected
    /// item highlighted and scrolled into view.
    fn open_popup(&mut self, pack: &Pack, id: NodeId) {
        let n = &self.nodes[id];
        let items = n.state.items.len();
        let font = self
            .style(pack, id)
            .and_then(|s| Self::font_id(pack, s))
            .and_then(|f| Font::get(pack, f));
        let row_h = self.list_row_height(pack, id).max(n.rect.h - 2);
        let max_rows = n
            .ctrl
            .field("maxPopupHeight")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(200)
            / row_h.max(1);
        let rows = items.min(max_rows.max(1) as usize).max(1);
        let bar = if rows < items { POPUP_BAR } else { 0 };
        let widest = font.map_or(0, |f| {
            n.state
                .items
                .iter()
                .map(|(t, _)| f.width(t))
                .max()
                .unwrap_or(0)
        });
        let w = n.rect.w.max(widest + 8 + bar).min(self.canvas.0);
        let h = rows as i32 * row_h + 2;
        let mut y = n.rect.bottom();
        if y + h > self.canvas.1 {
            y = (n.rect.y - h).max(0);
        }
        let x = n.rect.x.min(self.canvas.0 - w).max(0);
        let selected = self
            .selected(id)
            .and_then(|s| n.state.items.iter().position(|(_, i)| *i == s));
        let mut p = Popup {
            node: id,
            rect: Rect::new(x, y, w, h),
            row_h,
            rows,
            scroll: 0,
            hover: selected,
            dragging: true,
        };
        if let Some(i) = selected {
            p.scroll_to(i, items);
        }
        self.popup_query.clear();
        self.popup_keys = self.nodes[id]
            .state
            .items
            .iter()
            .map(|(t, _)| search_key(t))
            .collect();
        self.popup_shown.clear();
        self.popup_shown.extend(0..items);
        self.popup = Some(p);
    }

    /// The control whose list is open, if any.
    pub fn open_popup_node(&self) -> Option<NodeId> {
        self.popup.map(|p| p.node)
    }

    /// What the player has typed into the open dropdown to filter it.
    pub fn popup_query(&self) -> Option<&str> {
        self.popup.map(|_| self.popup_query.as_str())
    }

    /// Items (text, id) the open dropdown currently lists, in order.
    pub fn popup_rows(&self) -> Vec<(String, i64)> {
        let Some(p) = self.popup else {
            return Vec::new();
        };
        let items = &self.nodes[p.node].state.items;
        self.popup_shown
            .iter()
            .filter_map(|&i| items.get(i).cloned())
            .collect()
    }

    /// The highlighted item of the open dropdown, if any.
    pub fn popup_highlight(&self) -> Option<(String, i64)> {
        let p = self.popup?;
        let item = *self.popup_shown.get(p.hover?)?;
        self.nodes[p.node].state.items.get(item).cloned()
    }

    /// Rest of the highlighted item after the typed text, shown as grey
    /// autocomplete when the item starts with it.
    pub fn popup_ghost(&self) -> Option<String> {
        if self.popup_query.is_empty() {
            return None;
        }
        let (text, _) = self.popup_highlight()?;
        let text = text.trim();
        let n = self.popup_query.chars().count();
        let head: String = text.chars().take(n).collect();
        (head.to_lowercase() == self.popup_query.to_lowercase())
            .then(|| text.chars().skip(n).collect())
    }

    /// Re-filter the open dropdown after the query changed. The highlight
    /// goes to the best match (the current choice when the query is empty).
    fn refilter_popup(&mut self) {
        let Some(mut p) = self.popup else {
            return;
        };
        let q = self.popup_query.to_lowercase();
        filter_keys(&self.popup_keys, &q, &mut self.popup_shown);
        p.scroll = 0;
        p.hover = if q.is_empty() {
            self.selected(p.node).and_then(|s| {
                self.nodes[p.node]
                    .state
                    .items
                    .iter()
                    .position(|(_, i)| *i == s)
            })
        } else {
            self.popup_shown
                .iter()
                .position(|&i| !pinned_key(&self.popup_keys[i]))
        };
        if let Some(i) = p.hover {
            p.scroll_to(i, self.popup_shown.len());
        }
        self.popup = Some(p);
    }

    pub fn close_popup(&mut self) {
        self.popup = None;
    }

    /// Choose display row `row` of the open list.
    fn choose_popup_item(&mut self, p: Popup, row: usize, out: &mut Vec<ViewEvent>) {
        self.popup = None;
        let Some(&i) = self.popup_shown.get(row) else {
            return;
        };
        if let Some((_, item)) = self.nodes[p.node].state.items.get(i).cloned() {
            self.nodes[p.node].state.value = Value::Selected(Some(item));
            out.push(ViewEvent {
                node: p.node,
                kind: EventKind::Changed,
            });
        }
    }

    fn scroll_popup(&mut self, rows: i32) {
        if let Some(p) = &mut self.popup {
            let items = self.popup_shown.len();
            let max = items.saturating_sub(p.rows) as i32;
            p.scroll = (p.scroll as i32 + rows).clamp(0, max) as usize;
        }
    }

    /// Mouse press while a list is open: rows choose, the scroll bar
    /// scrolls, anything else closes the list without reaching the control
    /// underneath.
    fn popup_mouse_down(&mut self, mut p: Popup, x: i32, y: i32, out: &mut Vec<ViewEvent>) {
        let items = self.popup_shown.len();
        if let Some(i) = p.row_at(x, y, items) {
            self.choose_popup_item(p, i, out);
        } else if let Some(bar) = p.scroll_bar(items).filter(|b| b.contains(x, y)) {
            let step = if y < bar.y + POPUP_BAR {
                -1
            } else if y >= bar.bottom() - POPUP_BAR {
                1
            } else if y < bar.y + bar.h / 2 {
                -(p.rows as i32)
            } else {
                p.rows as i32
            };
            p.dragging = false;
            self.popup = Some(p);
            self.scroll_popup(step);
        } else {
            self.popup = None;
        }
    }

    /// Keyboard while a list is open. The list owns every key so nothing
    /// leaks to the dialog or gameplay underneath.
    /// Typing filters it (`char`): Backspace edits the filter, Escape clears
    /// it and then closes, Enter or Tab takes the highlighted row.
    fn popup_key(&mut self, mut p: Popup, key: Key, out: &mut Vec<ViewEvent>) {
        let items = self.popup_shown.len();
        let last = items.saturating_sub(1);
        let cur = p.hover;
        let next = match key {
            Key::Escape if !self.popup_query.is_empty() => {
                self.popup_query.clear();
                self.refilter_popup();
                return;
            }
            Key::Escape => {
                self.popup = None;
                return;
            }
            Key::Backspace => {
                if self.popup_query.pop().is_some() {
                    self.refilter_popup();
                }
                return;
            }
            Key::Return | Key::NumpadEnter | Key::Tab => {
                match cur {
                    Some(i) => self.choose_popup_item(p, i, out),
                    None => self.popup = None,
                }
                return;
            }
            Key::Up => cur.map_or(last, |i| i.saturating_sub(1)),
            Key::Down => cur.map_or(0, |i| (i + 1).min(last)),
            Key::PageUp => cur.map_or(0, |i| i.saturating_sub(p.rows)),
            Key::PageDown => cur.map_or(0, |i| (i + p.rows).min(last)),
            Key::Home => 0,
            Key::End => last,
            _ => return,
        };
        if items > 0 {
            p.hover = Some(next);
            p.scroll_to(next, items);
            self.popup = Some(p);
        }
    }

    // ------------------------------------------------------------------ input

    fn clickable(&self, id: NodeId) -> bool {
        matches!(
            self.nodes[id].ctrl.class.as_str(),
            "GuiBitmapButtonCtrl"
                | "GuiButtonCtrl"
                | "GuiCheckBoxCtrl"
                | "GuiRadioCtrl"
                | "GuiPopUpMenuCtrl"
                | "GuiTextEditCtrl"
                | "GuiMLTextEditCtrl"
                | "GuiTextListCtrl"
                | "GuiSliderCtrl"
                | "GuiScrollCtrl"
                | "GuiWindowCtrl"
        )
    }

    /// Deepest visible control containing the point (respecting parent clips).
    pub fn hit(&self, x: i32, y: i32) -> Option<NodeId> {
        self.hit_in(
            self.root,
            x,
            y,
            Rect::new(0, 0, self.canvas.0, self.canvas.1),
        )
    }

    fn hit_in(&self, id: NodeId, x: i32, y: i32, clip: Rect) -> Option<NodeId> {
        let n = &self.nodes[id];
        if !n.state.visible {
            return None;
        }
        let c = clip.intersect(&self.bounds(id))?;
        if !c.contains(x, y) {
            return None;
        }
        if n.ctrl.class != "GuiShapeNameHud" {
            for &k in n.children.iter().rev() {
                if let Some(h) = self.hit_in(k, x, y, c) {
                    return Some(h);
                }
            }
        }
        Some(id)
    }

    /// Nearest clickable ancestor-or-self of the hit control.
    fn target(&self, x: i32, y: i32) -> Option<NodeId> {
        let mut id = self.hit(x, y)?;
        loop {
            if self.clickable(id) && self.nodes[id].state.active {
                return Some(id);
            }
            id = self.nodes[id].parent?;
        }
    }

    /// Another modal view (or another application) owns the pointer. Reset
    /// hover so a later real entry produces exactly one new Hover event.
    pub fn mouse_leave(&mut self) {
        self.hover = None;
        self.scroll_drag = None;
        self.window_drag = None;
        self.window_resize = None;
        self.close_hot = false;
    }

    pub fn mouse_move(&mut self, x: i32, y: i32, out: &mut Vec<ViewEvent>) {
        self.mouse = (x, y);
        if let Some(p) = &mut self.popup {
            let items = self.popup_shown.len();
            if let Some(i) = p.row_at(x, y, items) {
                p.hover = Some(i);
            }
            return;
        }
        if let Some((id, grab, uh, dh)) = self.scroll_drag {
            self.drag_scroll(id, y - grab, uh, dh);
            return;
        }
        if let Some((id, lx, ly)) = self.window_drag {
            self.drag_window(id, x - lx, y - ly);
            self.window_drag = Some((id, x, y));
            return;
        }
        if let Some((id, lx, ly, w, h)) = self.window_resize {
            self.resize_window(id, if w { x - lx } else { 0 }, if h { y - ly } else { 0 });
            self.window_resize = Some((id, x, y, w, h));
            return;
        }
        if let Some((id, MouseButton::Left)) = self.pressed
            && self.nodes[id].ctrl.class == "GuiSliderCtrl"
        {
            self.slide_to(id, x);
            out.push(ViewEvent {
                node: id,
                kind: EventKind::Changed,
            });
        }
        let t = self.target(x, y);
        if t != self.hover {
            self.hover = t;
            if let Some(h) = t {
                out.push(ViewEvent {
                    node: h,
                    kind: EventKind::Hover,
                });
            }
        }
        self.close_hot = self.hover.is_some_and(|h| {
            self.nodes[h].ctrl.class == "GuiWindowCtrl" && self.close_rect(h, 16, 16).contains(x, y)
        });
    }

    fn slide_to(&mut self, id: NodeId, x: i32) {
        let r = self.nodes[id].rect;
        let (lo, hi) = self.range(id);
        let t = ((x - r.x - 4) as f32 / (r.w - 8).max(1) as f32).clamp(0.0, 1.0);
        let mut v = lo + t * (hi - lo);
        if let Some(ticks) = self.nodes[id]
            .ctrl
            .field("snap")
            .filter(|s| *s == "1")
            .and(self.nodes[id].ctrl.field("ticks"))
            .and_then(|t| t.parse::<f32>().ok())
        {
            let step = (hi - lo) / (ticks + 1.0);
            v = lo + ((v - lo) / step).round() * step;
        }
        self.nodes[id].state.value = Value::Num(v);
    }

    pub fn mouse_down(
        &mut self,
        b: MouseButton,
        x: i32,
        y: i32,
        pack: &Pack,
        out: &mut Vec<ViewEvent>,
    ) {
        self.mouse = (x, y);
        if let Some(p) = self.popup {
            self.popup_mouse_down(p, x, y, out);
            return;
        }
        let Some(t) = self.target(x, y) else {
            self.focus = None;
            return;
        };
        self.pressed = Some((t, b));
        let class = self.nodes[t].ctrl.class.clone();
        // GuiWindowCtrl::onMouseDown: a press on the title bar, off the close
        // box, drags the window (`canMove`).
        // GuiWindowCtrl::onMouseDown: its right and bottom edges resize it
        // (`resizeWidth`, `resizeHeight`); the title bar, off the close
        // box, drags it (`canMove`).
        let edges = (class == "GuiWindowCtrl" && b == MouseButton::Left).then(|| {
            let r = self.nodes[t].rect;
            let c = &self.nodes[t].ctrl;
            (
                c.field("resizeWidth") == Some("1") && x >= r.right() - RESIZE_EDGE,
                c.field("resizeHeight") == Some("1") && y >= r.bottom() - RESIZE_EDGE,
            )
        });
        if let Some((w, h)) = edges.filter(|(w, h)| *w || *h) {
            self.window_resize = Some((t, x, y, w, h));
        } else if class == "GuiWindowCtrl"
            && b == MouseButton::Left
            && self.nodes[t].ctrl.field("canMove") != Some("0")
            && y < self.nodes[t].rect.y + self.title_height(pack, t)
            && !(0..3).any(|slot| self.title_button(t, slot).contains(x, y))
        {
            self.window_drag = Some((t, x, y));
        }
        match class.as_str() {
            "GuiTextEditCtrl" | "GuiMLTextEditCtrl" => {
                self.focus = Some(t);
                let len = self.edit_text(t).chars().count();
                self.nodes[t].state.cursor = len;
            }
            "GuiSliderCtrl" if b == MouseButton::Left => {
                self.slide_to(t, x);
                out.push(ViewEvent {
                    node: t,
                    kind: EventKind::Changed,
                });
            }
            "GuiPopUpMenuCtrl"
                if b == MouseButton::Left && !self.nodes[t].state.items.is_empty() =>
            {
                self.open_popup(pack, t);
            }
            // GuiScrollCtrl: arrows step a row, the thumb drags and the
            // track pages by the visible height.
            "GuiScrollCtrl" if b == MouseButton::Left => {
                let r = self.nodes[t].rect;
                let bw = self.scroll_bar_width(pack, t);
                if bw > 0 && x >= r.right() - bw {
                    let (uh, dh) = self.scroll_arrows(pack, t);
                    let offset = self.nodes[t].state.scroll_y;
                    let content = self.content_height(t);
                    let track = (r.y + uh, r.h - uh - dh);
                    if y < r.y + uh {
                        self.scroll_by(t, -self.scroll_step(t));
                    } else if y >= r.bottom() - dh {
                        self.scroll_by(t, self.scroll_step(t));
                    } else if let Some((ty, th)) = thumb(track.0, track.1, content, r.h, offset) {
                        if y < ty {
                            self.scroll_by(t, -r.h);
                        } else if y >= ty + th {
                            self.scroll_by(t, r.h);
                        } else {
                            self.scroll_drag = Some((t, y - ty, uh, dh));
                        }
                    }
                }
            }
            "GuiTextListCtrl" if b == MouseButton::Left => {
                let r = self.nodes[t].rect;
                let rh = self.list_row_height(pack, t);
                let i = ((y - r.y) / rh.max(1)) as usize;
                if let Some((_, item)) = self.nodes[t].state.items.get(i).cloned() {
                    let double = self.last_click.is_some_and(|(n, when)| {
                        n == t
                            && self.time_ms.saturating_sub(when) < 400
                            && self.selected(t) == Some(item)
                    });
                    self.nodes[t].state.value = Value::Selected(Some(item));
                    out.push(ViewEvent {
                        node: t,
                        kind: EventKind::Changed,
                    });
                    if double {
                        out.push(ViewEvent {
                            node: t,
                            kind: EventKind::Submit,
                        });
                    }
                    self.last_click = Some((t, self.time_ms));
                }
            }
            _ => {}
        }
    }

    pub fn mouse_up(
        &mut self,
        b: MouseButton,
        x: i32,
        y: i32,
        _pack: &Pack,
        out: &mut Vec<ViewEvent>,
    ) {
        self.mouse = (x, y);
        self.scroll_drag = None;
        self.window_drag = None;
        self.window_resize = None;
        if let Some(mut open) = self.popup {
            // Press-drag-release over a row picks it, like Torque's list.
            let items = self.popup_shown.len();
            match open.row_at(x, y, items) {
                Some(i) if open.dragging => self.choose_popup_item(open, i, out),
                _ => {
                    open.dragging = false;
                    self.popup = Some(open);
                }
            }
            self.pressed = None;
            return;
        }
        let Some((p, pb)) = self.pressed.take() else {
            return;
        };
        if pb != b {
            return;
        }
        let over = self.target(x, y) == Some(p);
        if !over {
            return;
        }
        let class = self.nodes[p].ctrl.class.clone();
        match (class.as_str(), b) {
            ("GuiWindowCtrl", MouseButton::Left) if self.close_rect(p, 16, 16).contains(x, y) => {
                out.push(ViewEvent {
                    node: p,
                    kind: EventKind::Close,
                });
            }
            ("GuiWindowCtrl", MouseButton::Left)
                if self.nodes[p].ctrl.field("canMaximize") == Some("1")
                    && self.title_button(p, 1).contains(x, y) =>
            {
                let s = &mut self.nodes[p].state;
                s.maximized = !s.maximized;
                s.minimized = None;
                self.relayout();
            }
            ("GuiWindowCtrl", MouseButton::Left)
                if self.nodes[p].ctrl.field("canMinimize") == Some("1")
                    && self.title_button(p, 2).contains(x, y) =>
            {
                let title = self.title_height(_pack, p);
                let s = &mut self.nodes[p].state;
                s.minimized = if s.minimized.is_some() {
                    None
                } else {
                    Some(title + 4)
                };
                s.maximized = false;
                self.relayout();
            }
            ("GuiCheckBoxCtrl", MouseButton::Left) => {
                let v = !self.bool_value(p);
                self.set_bool(p, v);
                out.push(ViewEvent {
                    node: p,
                    kind: EventKind::Changed,
                });
                out.push(ViewEvent {
                    node: p,
                    kind: EventKind::Click,
                });
            }
            ("GuiRadioCtrl", MouseButton::Left) => {
                self.select_radio(p);
                out.push(ViewEvent {
                    node: p,
                    kind: EventKind::Changed,
                });
                out.push(ViewEvent {
                    node: p,
                    kind: EventKind::Click,
                });
            }
            ("GuiBitmapButtonCtrl" | "GuiButtonCtrl", MouseButton::Left) => {
                let double = self
                    .last_click
                    .is_some_and(|(n, when)| n == p && self.time_ms.saturating_sub(when) < 400);
                out.push(ViewEvent {
                    node: p,
                    kind: EventKind::Click,
                });
                if double {
                    out.push(ViewEvent {
                        node: p,
                        kind: EventKind::DoubleClick,
                    });
                }
                self.last_click = Some((p, self.time_ms));
            }
            ("GuiBitmapButtonCtrl" | "GuiButtonCtrl", MouseButton::Right) => {
                out.push(ViewEvent {
                    node: p,
                    kind: EventKind::RightClick,
                });
            }
            _ => {}
        }
    }

    /// Radio buttons: exclusive within the same parent and `groupNum`.
    pub fn select_radio(&mut self, id: NodeId) {
        let group = self.nodes[id].ctrl.group;
        if let Some(parent) = self.nodes[id].parent {
            for k in self.nodes[parent].children.clone() {
                if self.nodes[k].ctrl.class == "GuiRadioCtrl" && self.nodes[k].ctrl.group == group {
                    self.nodes[k].state.value = Value::Bool(k == id);
                }
            }
        }
    }

    pub fn scroll_by(&mut self, id: NodeId, dy: i32) {
        let to = self.nodes[id].state.scroll_y + dy;
        self.scroll_to(id, to);
    }

    /// Set a scroll control's offset and move its children with it.
    pub fn scroll_to(&mut self, id: NodeId, y: i32) {
        let max = (self.content_height(id) - self.nodes[id].rect.h).max(0);
        let y = y.clamp(0, max);
        if self.nodes[id].state.scroll_y == y {
            return;
        }
        self.nodes[id].state.scroll_y = y;
        let a = authored_rect(&self.nodes[id].ctrl);
        let r = self.nodes[id].rect;
        self.layout_children(id, (a.w, a.h), r);
    }

    /// Thumb dragged so its top is at `thumb_y`.
    fn drag_scroll(&mut self, id: NodeId, thumb_y: i32, uh: i32, dh: i32) {
        let r = self.nodes[id].rect;
        let content = self.content_height(id);
        let Some((_, th)) = thumb(r.y + uh, r.h - uh - dh, content, r.h, 0) else {
            return;
        };
        let free = (r.h - uh - dh - th).max(1);
        let t = (thumb_y - r.y - uh).clamp(0, free);
        let to = (t as i64 * (content - r.h) as i64 / free as i64) as i32;
        self.scroll_to(id, to);
    }

    /// Mouse wheel: scroll the innermost scroll control under the cursor.
    /// Returns true if consumed.
    pub fn wheel(&mut self, delta: i32) -> bool {
        if self.popup.is_some() {
            self.scroll_popup(-delta);
            return true;
        }
        let Some(mut id) = self.hit(self.mouse.0, self.mouse.1) else {
            return false;
        };
        loop {
            if self.nodes[id].ctrl.class == "GuiScrollCtrl" {
                self.scroll_by(id, -delta * self.scroll_step(id));
                return true;
            }
            match self.nodes[id].parent {
                Some(p) => id = p,
                None => return false,
            }
        }
    }

    /// Keyboard for the focused edit control. Returns true if consumed.
    pub fn key(&mut self, key: Key, mods: Modifiers, out: &mut Vec<ViewEvent>) -> bool {
        if let Some(p) = self.popup {
            self.popup_key(p, key, out);
            return true;
        }
        let Some(f) = self.focus else { return false };
        if !self.is_shown(f) {
            self.focus = None;
            return false;
        }
        let mut t: Vec<char> = self.edit_text(f).chars().collect();
        let cur = self.nodes[f].state.cursor.min(t.len());
        match key {
            Key::Backspace if cur > 0 => {
                t.remove(cur - 1);
                self.nodes[f].state.cursor = cur - 1;
            }
            Key::Delete if cur < t.len() => {
                t.remove(cur);
            }
            Key::Left => self.nodes[f].state.cursor = cur.saturating_sub(1),
            Key::Right => self.nodes[f].state.cursor = (cur + 1).min(t.len()),
            Key::Home => self.nodes[f].state.cursor = 0,
            Key::End => self.nodes[f].state.cursor = t.len(),
            Key::Return | Key::NumpadEnter => {
                out.push(ViewEvent {
                    node: f,
                    kind: EventKind::Submit,
                });
                return true;
            }
            Key::Tab => {
                self.focus_next(f, mods.shift);
                return true;
            }
            Key::Backspace | Key::Delete => {}
            _ => return false,
        }
        let s: String = t.into_iter().collect();
        self.nodes[f].state.value = Value::Text(s);
        out.push(ViewEvent {
            node: f,
            kind: EventKind::Changed,
        });
        true
    }

    fn focus_next(&mut self, from: NodeId, back: bool) {
        let edits: Vec<NodeId> = self
            .walk()
            .filter(|&n| {
                matches!(self.nodes[n].ctrl.class.as_str(), "GuiTextEditCtrl")
                    && self.is_shown(n)
                    && self.nodes[n].state.active
            })
            .collect();
        if let Some(i) = edits.iter().position(|&e| e == from) {
            let j = if back {
                (i + edits.len() - 1) % edits.len()
            } else {
                (i + 1) % edits.len()
            };
            self.focus = Some(edits[j]);
            let len = self.edit_text(edits[j]).chars().count();
            self.nodes[edits[j]].state.cursor = len;
        }
    }

    /// Typed character for the focused edit control.
    /// With a dropdown open, the character goes into its type-to-filter
    /// query instead.
    pub fn char(&mut self, c: char, out: &mut Vec<ViewEvent>) -> bool {
        if self.popup.is_some() {
            if !c.is_control() && text::to_cp1252(c).is_some() && self.popup_query.len() < 64 {
                self.popup_query.push(c);
                self.refilter_popup();
            }
            return true;
        }
        let Some(f) = self.focus else { return false };
        if c.is_control() || text::to_cp1252(c).is_none() {
            return self.focus.is_some();
        }
        let max = self.nodes[f]
            .ctrl
            .field("maxLength")
            .and_then(|m| m.parse::<usize>().ok())
            .unwrap_or(255);
        let mut t: Vec<char> = self.edit_text(f).chars().collect();
        if t.len() >= max {
            return true;
        }
        let cur = self.nodes[f].state.cursor.min(t.len());
        t.insert(cur, c);
        self.nodes[f].state.cursor = cur + 1;
        self.nodes[f].state.value = Value::Text(t.into_iter().collect());
        out.push(ViewEvent {
            node: f,
            kind: EventKind::Changed,
        });
        true
    }

    /// Accelerator lookup: first visible, active control (tree order) whose
    /// `accelerator` matches (GuiCanvas accelerator map semantics).
    pub fn accelerator(&self, key: Key, mods: Modifiers) -> Option<NodeId> {
        let pressed = Chord { mods, key };
        self.walk().find(|&n| {
            let c = &self.nodes[n].ctrl;
            c.accelerator
                .as_deref()
                .and_then(Chord::parse)
                .is_some_and(|a| {
                    a == pressed
                        || (a.key == Key::Return && key == Key::NumpadEnter && a.mods == mods)
                })
                && self.is_shown(n)
                && self.nodes[n].state.active
        })
    }

    pub fn tick(&mut self, dt_ms: u64) {
        self.time_ms += dt_ms;
    }
}

/// GuiControl::parentResized for one control (Torque3D guiControl.cpp:1348).
pub fn resize(
    a: Rect,
    h: HSizing,
    v: VSizing,
    min: [i32; 2],
    old: (i32, i32),
    new: (i32, i32),
) -> Rect {
    let (dx, dy) = (new.0 - old.0, new.1 - old.1);
    if matches!((h, v), (HSizing::Relative, VSizing::Relative)) && old.0 > 0 && old.1 > 0 {
        return relative_uniform(a, min, old, new);
    }
    let (mut x, mut y, mut w, mut hh) = (a.x, a.y, a.w, a.h);
    match h {
        HSizing::Center => x = (new.0 - a.w) >> 1,
        HSizing::Width => w = a.w + dx,
        HSizing::Left => x = a.x + dx,
        HSizing::Relative if old.0 != 0 => {
            x = (a.x as f32 / old.0 as f32 * new.0 as f32).round() as i32;
            w = (a.w as f32 / old.0 as f32 * new.0 as f32).round() as i32;
        }
        _ => {}
    }
    match v {
        VSizing::Center => y = (new.1 - a.h) >> 1,
        VSizing::Height => hh = a.h + dy,
        VSizing::Top => y = a.y + dy,
        VSizing::Relative if old.1 != 0 => {
            y = (a.y as f32 / old.1 as f32 * new.1 as f32).round() as i32;
            hh = (a.h as f32 / old.1 as f32 * new.1 as f32).round() as i32;
        }
        _ => {}
    }
    // GuiControl::resize clamps the extent to minExtent.
    Rect::new(x, y, w.max(min[0]), hh.max(min[1]))
}

/// Source rectangle (pixels) of an image that covers `dst` at its own aspect,
/// cropping the overflow evenly from both sides.
fn cover_src(img: (f32, f32), dst: (f32, f32)) -> [f32; 4] {
    if img.0 <= 0.0 || img.1 <= 0.0 || dst.0 <= 0.0 || dst.1 <= 0.0 {
        return [0.0, 0.0, img.0, img.1];
    }
    let k = (dst.0 / img.0).max(dst.1 / img.1);
    let (w, h) = (dst.0 / k, dst.1 / k);
    [(img.0 - w) / 2.0, (img.1 - h) / 2.0, w, h]
}

/// Torque scales a relative/relative control by the parent's change on each
/// axis separately, which squashes the main menu's text bitmaps on any aspect
/// other than 4:3. Keep v20's relative placement but scale the extent by one
/// factor so art keeps its aspect. Inside its relative cell the control hugs
/// the parent edge it was authored against, otherwise it stays centred.
fn relative_uniform(a: Rect, min: [i32; 2], old: (i32, i32), new: (i32, i32)) -> Rect {
    let sx = new.0 as f32 / old.0 as f32;
    let sy = new.1 as f32 / old.1 as f32;
    let k = sx.min(sy);
    let w = ((a.w as f32 * k).round() as i32).max(min[0]);
    let h = ((a.h as f32 * k).round() as i32).max(min[1]);
    let place = |pos: i32, len: i32, parent: i32, s: f32, fit: i32| {
        let (start, cell) = (pos as f32 * s, len as f32 * s);
        let slack = cell - fit as f32;
        (if pos <= 0 {
            start
        } else if pos + len >= parent {
            start + slack
        } else {
            start + slack / 2.0
        })
        .round() as i32
    };
    Rect::new(
        place(a.x, a.w, old.0, sx, w),
        place(a.y, a.h, old.1, sy, h),
        w,
        h,
    )
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn relative_controls_scale_uniformly_on_widescreen() {
        // v20's main menu Join button, left edge, on a 16:9 canvas.
        let join = Rect::new(0, 200, 224, 40);
        let r = resize(
            join,
            HSizing::Relative,
            VSizing::Relative,
            [8, 2],
            (640, 480),
            (960, 540),
        );
        assert_eq!(r, Rect::new(0, 225, 252, 45));
        // The About button, authored past the right edge, hugs it.
        let about = Rect::new(520, 390, 160, 30);
        let r = resize(
            about,
            HSizing::Relative,
            VSizing::Relative,
            [8, 2],
            (640, 480),
            (960, 540),
        );
        assert_eq!((r.w, r.h), (180, 34));
        assert_eq!(r.x + r.w, 1020);
        // 4:3 matches Torque exactly.
        let r = resize(
            about,
            HSizing::Relative,
            VSizing::Relative,
            [8, 2],
            (640, 480),
            (1280, 960),
        );
        assert_eq!(r, Rect::new(1040, 780, 320, 60));
    }

    #[test]
    fn backgrounds_crop_to_cover() {
        assert_eq!(
            cover_src((640.0, 480.0), (960.0, 540.0)),
            [0.0, 60.0, 640.0, 360.0]
        );
        assert_eq!(
            cover_src((640.0, 480.0), (640.0, 480.0)),
            [0.0, 0.0, 640.0, 480.0]
        );
    }
}
