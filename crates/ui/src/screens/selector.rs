//! Native counterparts of BSD_* and PSD_* (v20 client scripts 9472–10500).
//! Authored windows retain their original art; catalogs build the dynamic grids.
use super::*;
use crate::api::{BrickInfo, GameAction, IconRef, PrintInfo, UiAction};
use crate::input::Chord;
use crate::models::hud::Outbox;
use crate::models::selector::{CART_SLOTS, CatalogLayout};
use crate::view::EventKind;

const BG: &str = "base/client/ui/brickicons/brickiconbg";
const ACTIVE: &str = "base/client/ui/brickicons/brickiconactive";
const TILE: &str = "base/client/ui/brickicons/brickiconbtn";
const UNKNOWN: &str = "base/client/ui/brickicons/unknown";
const SEARCH: &str = "BSD_Search";
/// The brick grids start below the search row (v20's started at 57).
const GRID: Rect = Rect::new(3, 78, 634, 342);

fn named(mut control: Control, name: impl Into<String>) -> Control {
    control.name = Some(name.into());
    control
}

fn index(command: &str, prefix: &str) -> Option<usize> {
    command
        .strip_prefix(prefix)?
        .strip_suffix(");")?
        .parse()
        .ok()
}

fn set_icon(view: &mut View, node: NodeId, icon: &IconRef, fallback: &str) {
    let state = view.state(node);
    state.external_texture = match icon {
        IconRef::External(id) => Some(*id),
        _ => None,
    };
    state.bitmap = match icon {
        IconRef::Pack(path) => Some(path.to_ascii_lowercase()),
        IconRef::External(_) => None,
        IconRef::None => Some(fallback.into()),
    };
    // External nodes must not fall back to their original placeholder bitmap.
    if state.external_texture.is_some() {
        view.nodes[node].ctrl.bitmap = None;
    }
}

fn scroller(view: &mut View, parent: NodeId, name: &str, rect: Rect) -> NodeId {
    let mut c = named(ctrl("GuiScrollCtrl", "ColorScrollProfile", rect), name);
    c.fields.insert("hScrollBar".into(), "alwaysOff".into());
    c.fields.insert("vScrollBar".into(), "alwaysOn".into());
    view.add(parent, c)
}

pub struct BrickSelector {
    view: View,
    catalog: Vec<BrickInfo>,
    tabs: Vec<(NodeId, NodeId)>,
    tiles: Vec<(usize, NodeId)>,
    /// The search results' scroller and body, and their tiles.
    results: Option<(NodeId, NodeId)>,
    result_tiles: Vec<(usize, NodeId)>,
    slots: Vec<(NodeId, NodeId)>,
    request: Option<RequestId>,
    instant: Option<BrickInfo>,
}

impl BrickSelector {
    pub fn new(core: &Core) -> Self {
        let mut screen = Self {
            view: layout_view(core, "BrickSelectorDlg"),
            catalog: core.bricks.clone(),
            tabs: Vec::new(),
            tiles: Vec::new(),
            results: None,
            result_tiles: Vec::new(),
            slots: Vec::new(),
            request: None,
            instant: None,
        };
        screen.build(core);
        screen
    }

    fn build(&mut self, core: &Core) {
        self.view = layout_view(core, "BrickSelectorDlg");
        self.tabs.clear();
        self.tiles.clear();
        self.result_tiles.clear();
        self.slots.clear();
        let parent = self.view.id("BSD_Window").unwrap_or(self.view.root);
        let layout = CatalogLayout::build(&self.catalog);
        self.view.add(
            parent,
            text("GuiTextProfile", Rect::new(8, 58, 48, 18), "Search:"),
        );
        let search = self.view.add(
            parent,
            named(
                ctrl(
                    "GuiTextEditCtrl",
                    "BlockTextEditProfile",
                    Rect::new(58, 58, 200, 18),
                ),
                SEARCH,
            ),
        );
        self.view.set_text(search, core.selector.search.clone());
        for (i, tab) in layout.tabs.iter().enumerate() {
            let b = self.view.add(
                parent,
                named(
                    button(
                        "BlockButtonProfile",
                        Rect::new(3 + i as i32 * 80, 30, 80, 25),
                        "base/client/ui/tab1",
                        &tab.name,
                        &format!("BSD_ShowTab({i});"),
                    ),
                    format!("BSD_Tab{i}"),
                ),
            );
            let scroll = scroller(&mut self.view, parent, &format!("BSD_Scroll{i}"), GRID);
            let body = self.view.add(
                scroll,
                ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 617, 2)),
            );
            let tiles = self.fill(body, &tab.sections, "BSD_Brick");
            self.tiles.extend(tiles);
            self.tabs.push((b, scroll));
        }
        let scroll = scroller(&mut self.view, parent, "BSD_SearchResults", GRID);
        let body = self.view.add(
            scroll,
            ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 617, 2)),
        );
        self.results = Some((scroll, body));
        self.fill_results(&core.selector.search);
        self.build_cart(parent, core);
    }

    /// The search results for `query` in their own scroller.
    fn fill_results(&mut self, query: &str) {
        let Some((_, body)) = self.results else {
            return;
        };
        self.view.clear_children(body);
        let sections = CatalogLayout::search(&self.catalog, query);
        self.result_tiles = self.fill(body, &sections, "BSD_Result");
        if sections.is_empty() && !query.trim().is_empty() {
            self.view.add(
                body,
                text(
                    "BlockButtonProfile",
                    Rect::new(18, 0, 581, 18),
                    "No bricks match.",
                ),
            );
        }
    }

    /// One tab's grid: a heading per section and six tiles per row.
    fn fill(
        &mut self,
        body: NodeId,
        sections: &[crate::models::selector::Section],
        prefix: &str,
    ) -> Vec<(usize, NodeId)> {
        let mut tiles = Vec::new();
        let mut y = 0;
        {
            for section in sections {
                self.view.add(
                    body,
                    text(
                        "BlockButtonProfile",
                        Rect::new(18, y - 2, 581, 18),
                        &section.name,
                    ),
                );
                for (j, &brick) in section.bricks.iter().enumerate() {
                    let rect = Rect::new(
                        18 + (j % 6) as i32 * 97,
                        y + 18 + (j / 6) as i32 * 97,
                        96,
                        96,
                    );
                    self.view.add(body, bitmap("BlockDefaultProfile", rect, BG));
                    let icon = self
                        .view
                        .add(body, bitmap("BlockDefaultProfile", rect, UNKNOWN));
                    set_icon(&mut self.view, icon, &self.catalog[brick].icon, UNKNOWN);
                    let active = self
                        .view
                        .add(body, bitmap("BlockDefaultProfile", rect, ACTIVE));
                    tiles.push((brick, active));
                    let mut c = named(
                        button(
                            "BlockButtonProfile",
                            rect,
                            TILE,
                            " ",
                            &format!("BSD_ClickIcon({brick});"),
                        ),
                        format!("{prefix}{brick}"),
                    );
                    c.alt_command = Some(format!("BSD_RightClickIcon({brick});"));
                    self.view.add(body, c);
                    self.view.add(
                        body,
                        text(
                            "HUDBSDNameProfile",
                            Rect::new(rect.x, rect.y + 78, 96, 18),
                            &self.catalog[brick].ui_name,
                        ),
                    );
                }
                // Preserve the authored section pitch (96), including its 1px row overlap.
                y += 23 + section.bricks.len().div_ceil(6) as i32 * 96;
            }
            self.view.nodes[body].ctrl.extent[1] = y.max(2);
        }
        tiles
    }

    fn build_cart(&mut self, parent: NodeId, core: &Core) {
        let cart = self.view.add(
            parent,
            named(
                swatch(Rect::new(3, 421, 559, 55), [51, 128, 255, 255]),
                "BSD_InvBox",
            ),
        );
        for i in 0..CART_SLOTS {
            let rect = Rect::new(i as i32 * 56, 0, 55, 55);
            self.view.add(cart, bitmap("GuiDefaultProfile", rect, BG));
            let icon = self.view.add(cart, bitmap("HUDBitmapProfile", rect, ""));
            let active = self
                .view
                .add(cart, bitmap("GuiDefaultProfile", rect, ACTIVE));
            self.view.add(
                cart,
                named(
                    button(
                        "BlockButtonProfile",
                        rect,
                        TILE,
                        " ",
                        &format!("BSD_ClickInv({i});"),
                    ),
                    format!("BSD_Slot{i}"),
                ),
            );
            self.slots.push((icon, active));
        }
        for name in ["BSD_ClearBtn", "BSD_FavsHelper"] {
            if let Some(n) = self.view.id(name) {
                self.view.push_to_back(n);
            }
        }
        self.refresh(core);
        self.view.layout(core.logical.0, core.logical.1);
    }

    fn refresh(&mut self, core: &Core) {
        let model = &core.selector;
        let pending = self.request.is_some() || core.is_pending(&Pending::Buy);
        let searching = !model.search.trim().is_empty();
        if let Some((scroll, _)) = self.results {
            self.view.set_visible(scroll, searching);
        }
        for (i, &(button, scroll)) in self.tabs.iter().enumerate() {
            self.view.state(button).bitmap = Some(
                if model.tab == i && !searching {
                    "base/client/ui/tab1use"
                } else {
                    "base/client/ui/tab1"
                }
                .into(),
            );
            self.view.set_visible(scroll, model.tab == i && !searching);
        }
        for &(brick, node) in self.tiles.iter().chain(&self.result_tiles) {
            self.view
                .set_visible(node, model.clicked_brick == Some(brick));
        }
        for (i, &(icon, active)) in self.slots.iter().enumerate() {
            let brick = model.cart[i].and_then(|b| self.catalog.get(b));
            set_icon(
                &mut self.view,
                icon,
                &brick.map_or(IconRef::None, |b| b.icon.clone()),
                if brick.is_some() { UNKNOWN } else { "" },
            );
            self.view.set_visible(active, model.clicked_slot == Some(i));
        }
        if let Some(n) = self.view.id("BSD_FavsHelper") {
            self.view.set_visible(n, model.setting_favs);
        }
        if let Some(n) = self.view.id("BSD_SetFavsButton") {
            self.view.set_text(n, model.set_favs_label());
        }
        if let Some(n) = self.view.id("BSD_DoneButton") {
            self.view
                .set_text(n, if pending { "Buying..." } else { "DONE" });
        }
        for i in 0..10 {
            if let Some(n) = self.view.id(&format!("BSD_FavButton{i}")) {
                self.view.state(n).tint = Some([
                    255,
                    255,
                    255,
                    if model.favorite_filled(i) { 255 } else { 128 },
                ]);
            }
        }
        for n in self.view.walk().collect::<Vec<_>>() {
            if self.view.node(n).ctrl.class.contains("Button") {
                self.view.set_active(n, !pending);
            }
        }
    }

    fn buy(&mut self, core: &mut Core, instant: Option<usize>) {
        if self.request.is_some() || core.is_pending(&Pending::Buy) {
            return;
        }
        if core.hud.building_disabled {
            core.message_ok("Building Disabled", "Building is disabled on this server.");
            return;
        }
        let action = if let Some(index) = instant {
            let Some(brick) = core.bricks.get(index).cloned() else {
                return;
            };
            self.instant = Some(brick.clone());
            UiAction::InstantUseBrick { brick: brick.id }
        } else {
            UiAction::BuyBricks {
                slots: core.selector.purchase(&core.bricks),
            }
        };
        self.request = Some(core.request_pending(action, Pending::Buy));
        self.refresh(core);
    }

    fn clear_search(&mut self, core: &mut Core) {
        core.selector.search.clear();
        core.selector.clicked_brick = None;
        if let Some(n) = self.view.id(SEARCH) {
            self.view.set_text(n, "");
        }
        self.view.focus = None;
        self.fill_results("");
        self.view.layout(core.logical.0, core.logical.1);
        self.refresh(core);
    }

    fn favorite(&mut self, number: u8, core: &mut Core) {
        if core.selector.click_fav(number, &core.bricks) {
            core.save_settings();
        }
        self.refresh(core);
    }
}

impl Screen for BrickSelector {
    fn id(&self) -> ScreenId {
        ScreenId::BrickSelector
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_wake(&mut self, core: &mut Core) {
        // `BrickSelectorDlg::onWake` sends `serverCmdBSD`, whose "Bricks"
        // emote rises over the player's head for everyone to see.
        core.game(GameAction::Emote { name: "bsd".into() });
        core.selector.open();
        core.selector.tab = core.selector.tab.min(self.tabs.len().saturating_sub(1));
        core.hud.boxes_visible = false;
        self.refresh(core);
    }
    fn on_sleep(&mut self, core: &mut Core) {
        core.hud.boxes_visible = true;
        core.selector.clicked_brick = None;
        core.selector.clicked_slot = None;
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if ev.kind == EventKind::Close {
            core.pop(self.id());
            return;
        }
        if self.view.node(ev.node).ctrl.name.as_deref() == Some(SEARCH) {
            if ev.kind == EventKind::Changed {
                core.selector.search = self.view.edit_text(ev.node);
                let query = core.selector.search.clone();
                // Highlight the brick Enter would pick.
                core.selector.clicked_brick = CatalogLayout::best_match(&self.catalog, &query);
                core.selector.clicked_slot = None;
                self.fill_results(&query);
                self.view.layout(core.logical.0, core.logical.1);
                self.refresh(core);
            }
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::RightClick) {
            return;
        }
        let command = command_of(&self.view, ev.node);
        if command.to_ascii_lowercase().contains("popdialog") {
            core.pop(self.id());
            return;
        }
        if self.request.is_some() || core.is_pending(&Pending::Buy) {
            return;
        }
        if let Some(i) = index(&command, "BSD_ClickIcon(") {
            if i < core.bricks.len() {
                if ev.kind == EventKind::RightClick {
                    self.buy(core, Some(i));
                } else {
                    core.selector.click_brick(i);
                }
            }
        } else if ev.kind == EventKind::Click {
            if let Some(i) = index(&command, "BSD_ClickInv(") {
                if i < CART_SLOTS {
                    core.selector.click_slot(i);
                }
            } else if let Some(i) = index(&command, "BSD_ShowTab(") {
                if i < self.tabs.len() {
                    core.selector.tab = i;
                    // A tab ends the search.
                    core.selector.search.clear();
                    if let Some(n) = self.view.id(SEARCH) {
                        self.view.set_text(n, "");
                    }
                }
            } else if let Some(i) = index(&command, "BSD_ClickFav(") {
                if i < 10 {
                    self.favorite(i as u8, core);
                }
            } else {
                match command.as_str() {
                    "BSD_BuyBricks();" => self.buy(core, None),
                    "BSD_ClickClear();" => core.selector.clear_cart(),
                    "BSD_SetFavs();" => core.selector.toggle_set_favs(),
                    "BSD_NextTab();" => core.selector.next_tab(self.tabs.len()),
                    _ => {}
                }
            }
        }
        self.refresh(core);
    }
    fn on_key(&mut self, key: Key, mods: Modifiers, core: &mut Core) -> bool {
        let search = self.view.id(SEARCH);
        let in_search = self.view.focus.is_some() && self.view.focus == search;
        if key == Key::Escape {
            // Escape clears a search first, then closes.
            if !core.selector.search.is_empty() {
                self.clear_search(core);
            } else {
                core.pop(self.id());
            }
            return true;
        }
        if in_search {
            // Enter takes the best match straight into the hand to build
            // with, like right-clicking its tile.
            if matches!(key, Key::Return | Key::NumpadEnter) {
                if let Some(b) = CatalogLayout::best_match(&self.catalog, &core.selector.search) {
                    self.buy(core, Some(b));
                }
                return true;
            }
            // Other keys typed into the box are text, not shortcuts.
            return false;
        }
        if self.request.is_some() || core.is_pending(&Pending::Buy) {
            return true;
        }
        let buy_binding = core
            .binds
            .command_for_key(key, mods)
            .is_some_and(|c| c.eq_ignore_ascii_case("openBSD"));
        if buy_binding || matches!(key, Key::Return | Key::NumpadEnter) {
            self.buy(core, None);
            return true;
        }
        // Typing a letter anywhere in the selector starts a search; the
        // character itself follows as text into the focused box.
        if let (Key::Letter(_), Some(n)) = (key, search)
            && !(mods.ctrl || mods.alt || mods.cmd)
        {
            self.view.focus = Some(n);
            self.view.state(n).cursor = self.view.edit_text(n).chars().count();
            return false;
        }
        if mods.is_empty() {
            match key {
                Key::Digit(i) if i < 10 => self.favorite(i, core),
                Key::Tab => {
                    core.selector.next_tab(self.tabs.len());
                    self.refresh(core);
                }
                _ => return false,
            }
            return true;
        }
        false
    }
    fn on_update(&mut self, core: &mut Core) {
        core.selector.queue_brick_buying =
            core.prefs.bool_or("$pref::Input::QueueBrickBuying", true);
        if self.catalog != core.bricks {
            // Core remaps cart indices by authored ID before notifying screens.
            self.catalog = core.bricks.clone();
            core.selector.tab = core.selector.tab.min(
                CatalogLayout::build(&self.catalog)
                    .tabs
                    .len()
                    .saturating_sub(1),
            );
            self.build(core);
        } else {
            self.refresh(core);
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
        match result {
            Ok(()) => {
                if let Some(brick) = self.instant.take() {
                    // The request was already sent; apply only the accepted local HUD transition.
                    let mut out = Outbox::default();
                    core.hud.instant_use(brick, &mut out);
                    for (text, seconds) in out.center_prints {
                        core.center_print(&text, seconds);
                    }
                }
                core.pop(self.id());
            }
            Err(reason) => {
                self.instant = None;
                core.message_ok("Brick Selection Rejected", reason);
            }
        }
        self.refresh(core);
        true
    }
}

pub struct PrintSelector {
    view: View,
    aspect: String,
    prints: Vec<PrintInfo>,
    letters: Vec<PrintInfo>,
    showing_letters: bool,
    request: Option<(RequestId, String)>,
    closed: bool,
}

impl PrintSelector {
    pub fn new(core: &Core) -> Self {
        let mut s = Self {
            view: layout_view(core, "PrintSelectorDlg"),
            aspect: core.print_aspect.clone().unwrap_or_default(),
            prints: vec![],
            letters: vec![],
            showing_letters: false,
            request: None,
            closed: false,
        };
        s.read_catalogs(core);
        s.showing_letters = core.print_letters_visible || s.prints.is_empty();
        s.build(core);
        s
    }
    fn read_catalogs(&mut self, core: &Core) {
        self.prints = core.prints.get(&self.aspect).cloned().unwrap_or_default();
        self.letters = core
            .prints
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("Letters"))
            .map(|(_, p)| p.clone())
            .unwrap_or_default();
    }
    fn build(&mut self, core: &Core) {
        self.view = layout_view(core, "PrintSelectorDlg");
        let parent = self.view.id("PSD_Window").unwrap_or(self.view.root);
        for (letters, prints) in [(false, &self.prints), (true, &self.letters)] {
            let suffix = if letters { "Letters" } else { "Prints" };
            let scroll = scroller(
                &mut self.view,
                parent,
                &format!("PSD_Scroll{suffix}"),
                Rect::new(6, 42, 205, 392),
            );
            let body = self.view.add(
                scroll,
                ctrl(
                    "GuiControl",
                    "ColorScrollProfile",
                    Rect::new(0, 0, 195, (prints.len().div_ceil(3) as i32 * 65).max(2)),
                ),
            );
            for (i, print) in prints.iter().enumerate() {
                let rect = Rect::new((i % 3) as i32 * 65, (i / 3) as i32 * 65, 64, 64);
                let icon = self.view.add(body, bitmap("GuiDefaultProfile", rect, ""));
                set_icon(&mut self.view, icon, &print.icon, "");
                let label = if print.icon == IconRef::None {
                    print.name.as_str()
                } else {
                    " "
                };
                self.view.add(
                    body,
                    named(
                        button(
                            "BlockButtonProfile",
                            rect,
                            "base/client/ui/btnprint",
                            label,
                            &format!("PSD_{suffix}({i});"),
                        ),
                        format!("PSD_{suffix}{i}"),
                    ),
                );
            }
        }
        self.refresh();
        self.view.layout(core.logical.0, core.logical.1);
    }
    fn refresh(&mut self) {
        for (letters, suffix, command) in [
            (false, "Prints", "PSD_PrintsTab();"),
            (true, "Letters", "PSD_LettersTab();"),
        ] {
            if let Some(n) = self.view.id(&format!("PSD_Scroll{suffix}")) {
                self.view.set_visible(n, letters == self.showing_letters);
            }
            if let Some(n) = self.view.by_command(command) {
                self.view.state(n).bitmap = Some(
                    if letters == self.showing_letters {
                        "base/client/ui/tab1use"
                    } else {
                        "base/client/ui/tab1"
                    }
                    .into(),
                );
            }
        }
        for n in self.view.walk().collect::<Vec<_>>() {
            if self.view.node(n).ctrl.class.contains("Button") {
                self.view.set_active(n, self.request.is_none());
            }
        }
        if let Some(n) = self.view.id("PSD_Window") {
            self.view.set_text(
                n,
                if self.request.is_some() {
                    "Applying Print..."
                } else {
                    "Print Selector"
                },
            );
        }
    }
    fn select(&mut self, print: String, core: &mut Core) {
        if self.request.is_some() || core.is_pending(&Pending::Print) {
            return;
        }
        let id = core.request_pending(
            UiAction::SetPrint {
                print: print.clone(),
            },
            Pending::Print,
        );
        self.request = Some((id, print));
        self.refresh();
    }
    fn close(&mut self, core: &mut Core) {
        if let Some((id, _)) = self.request.take() {
            core.abandon(id);
        }
        if !self.closed {
            core.request(UiAction::ClosePrintSelector);
            self.closed = true;
        }
        core.pop(self.id());
    }
}

impl Screen for PrintSelector {
    fn id(&self) -> ScreenId {
        ScreenId::PrintSelector
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn no_shift(&self) -> bool {
        true
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if !self.prints.is_empty() {
            core.print_letters_visible = self.showing_letters;
        }
        if let Some((id, _)) = self.request.take() {
            core.abandon(id);
        }
        if !self.closed {
            core.request(UiAction::ClosePrintSelector);
            self.closed = true;
        }
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if ev.kind == EventKind::Close {
            self.close(core);
            return;
        }
        if ev.kind != EventKind::Click {
            return;
        }
        let command = command_of(&self.view, ev.node);
        if command.to_ascii_lowercase().contains("popdialog") {
            self.close(core);
            return;
        }
        if self.request.is_some() {
            return;
        }
        match command.as_str() {
            "PSD_LettersTab();" => self.showing_letters = true,
            "PSD_PrintsTab();" => self.showing_letters = false,
            _ => {
                let print = index(&command, "PSD_Prints(")
                    .and_then(|i| self.prints.get(i))
                    .or_else(|| index(&command, "PSD_Letters(").and_then(|i| self.letters.get(i)))
                    .map(|p| p.id.clone());
                if let Some(print) = print {
                    self.select(print, core);
                }
            }
        }
        self.refresh();
    }
    fn on_key(&mut self, key: Key, mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            self.close(core);
            return true;
        }
        if self.request.is_some() {
            return true;
        }
        // v20 registers every print button's accelerator when the dialog is
        // pushed, hidden scrollers included, so letters type on either tab.
        let chord = Chord { key, mods };
        if let Some(print) = self
            .prints
            .iter()
            .chain(&self.letters)
            .find(|p| print_shortcut(&p.name) == Some(chord))
        {
            self.select(print.id.clone(), core);
            return true;
        }
        false
    }
    fn on_update(&mut self, core: &mut Core) {
        let previous = (
            self.aspect.clone(),
            self.prints.clone(),
            self.letters.clone(),
        );
        self.aspect = core.print_aspect.clone().unwrap_or_default();
        self.read_catalogs(core);
        if previous
            != (
                self.aspect.clone(),
                self.prints.clone(),
                self.letters.clone(),
            )
        {
            if self.prints.is_empty() {
                self.showing_letters = true;
            }
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
        if self.request.as_ref().map(|p| p.0) != Some(id) {
            return false;
        }
        let (_, print) = self.request.take().expect("matching pending print");
        match result {
            Ok(()) => {
                core.last_print.insert(self.aspect.clone(), print);
                self.closed = true;
                core.pop(self.id());
            }
            Err(reason) => core.message_ok("Print Rejected", reason),
        }
        self.refresh();
        true
    }
}

/// v20's literal filename-to-accelerator mapping, including shift punctuation.
fn print_shortcut(name: &str) -> Option<Chord> {
    let lower = name.to_ascii_lowercase();
    let binding = match lower.as_str() {
        "-bang" => "shift 1",
        "-at" => "shift 2",
        "-pound" => "shift 3",
        "-dollar" => "shift 4",
        "-percent" => "shift 5",
        "-caret" => "shift 6",
        "-and" => "shift 7",
        "-asterisk" => "shift 8",
        "-minus" => "-",
        "-equals" => "=",
        "-plus" => "shift =",
        "-period" => ".",
        "-less than" => "shift ,",
        "-greater than" => "shift .",
        "-qmark" => "shift /",
        "-apostrophe" => "'",
        "-space" => "space",
        _ if lower.chars().count() == 1 => &lower,
        _ => return None,
    };
    Chord::parse(binding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Settings, UiUpdate};
    use crate::binds::Platform;
    use crate::input::MouseButton;
    use crate::schema::UiPack;
    use crate::ui::{StackCmd, Ui, UiConfig};
    use std::rc::Rc;

    fn fixture() -> Ui {
        let mut pack = UiPack::default();
        for (layout, window) in [
            ("BrickSelectorDlg", "BSD_Window"),
            ("PrintSelectorDlg", "PSD_Window"),
        ] {
            let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
            root.children.push(named(
                ctrl(
                    "GuiWindowCtrl",
                    "GuiDefaultProfile",
                    Rect::new(0, 0, 640, 480),
                ),
                window,
            ));
            pack.layouts.insert(layout.into(), root);
        }
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(pack, Default::default())),
            UiConfig {
                size: (640, 480),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        ui.core.bricks = ["First", "Second", "Third"]
            .iter()
            .enumerate()
            .map(|(i, name)| BrickInfo {
                id: format!("brick:{i}"),
                ui_name: (*name).into(),
                category: if i == 2 { "Plates" } else { "Bricks" }.into(),
                subcategory: "1x".into(),
                icon: if i == 1 {
                    IconRef::External(73)
                } else {
                    IconRef::None
                },
            })
            .collect();
        ui.core.cmds.clear();
        ui.drain_actions();
        ui
    }

    fn click(screen: &mut dyn Screen, name: &str, button: MouseButton, core: &mut Core) {
        // Exercise the retained hit testing and pointer event path without any desktop input.
        let node = screen.view().id(name).expect("control exists");
        let rect = screen.view().node(node).rect;
        let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
        let mut events = vec![];
        screen
            .view_mut()
            .mouse_down(button, x, y, &core.pack, &mut events);
        screen
            .view_mut()
            .mouse_up(button, x, y, &core.pack, &mut events);
        for event in events {
            screen.on_event(&event, core);
        }
    }

    #[test]
    fn search_shows_results_that_add_to_the_cart_and_a_tab_ends_it() {
        let mut ui = fixture();
        let mut s = BrickSelector::new(&ui.core);
        s.on_wake(&mut ui.core);
        let search = s.view.id(SEARCH).unwrap();
        s.view.focus = Some(search);
        // Keys typed into the box are text, not favorites or Buy.
        assert!(!s.on_key(Key::Digit(3), Modifiers::NONE, &mut ui.core));
        s.view.set_text(search, "second");
        s.on_event(
            &ViewEvent {
                node: search,
                kind: EventKind::Changed,
            },
            &mut ui.core,
        );
        let results = s.view.id("BSD_SearchResults").unwrap();
        assert!(s.view.is_shown(results));
        assert!(!s.view.is_shown(s.view.id("BSD_Brick0").unwrap()));
        let hit = s.view.id("BSD_Result1").unwrap();
        assert!(s.view.is_shown(hit));
        assert!(s.view.id("BSD_Result0").is_none());
        click(&mut s, "BSD_Result1", MouseButton::Left, &mut ui.core);
        click(&mut s, "BSD_Result1", MouseButton::Left, &mut ui.core);
        assert_eq!(ui.core.selector.cart[0], Some(1));
        click(&mut s, "BSD_Tab0", MouseButton::Left, &mut ui.core);
        assert!(ui.core.selector.search.is_empty());
        assert!(!s.view.is_shown(results));
        assert!(s.view.is_shown(s.view.id("BSD_Brick0").unwrap()));
    }

    /// Type `text` as characters into the selector, feeding its events back.
    fn type_text(s: &mut BrickSelector, text: &str, core: &mut Core) {
        for ch in text.chars() {
            let key = Key::Letter(ch.to_ascii_lowercase());
            if ch.is_ascii_alphabetic() && s.on_key(key, Modifiers::NONE, core) {
                continue;
            }
            let mut out = Vec::new();
            s.view.char(ch, &mut out);
            for ev in out {
                s.on_event(&ev, core);
            }
        }
    }

    #[test]
    fn typing_starts_a_search_escape_clears_it_and_enter_takes_the_best_match() {
        let mut ui = fixture();
        let mut s = BrickSelector::new(&ui.core);
        s.on_wake(&mut ui.core);
        ui.drain_actions();
        let search = s.view.id(SEARCH).unwrap();
        assert_eq!(s.view.focus, None);
        type_text(&mut s, "th", &mut ui.core);
        assert_eq!(
            s.view.focus,
            Some(search),
            "a letter focuses the search box"
        );
        assert_eq!(ui.core.selector.search, "th");
        assert_eq!(
            ui.core.selector.clicked_brick,
            Some(2),
            "Third is highlighted"
        );
        assert!(s.on_key(Key::Escape, Modifiers::NONE, &mut ui.core));
        assert!(
            ui.core.selector.search.is_empty(),
            "Escape clears the search"
        );
        assert!(
            !ui.core
                .cmds
                .contains(&StackCmd::Pop(ScreenId::BrickSelector))
        );
        assert_eq!(s.view.focus, None);
        type_text(&mut s, "Econd", &mut ui.core);
        assert_eq!(ui.core.selector.clicked_brick, Some(1));
        assert!(s.on_key(Key::Return, Modifiers::NONE, &mut ui.core));
        let actions = ui.drain_actions();
        assert!(
            actions.iter().any(
                |(_, a)| matches!(a, UiAction::InstantUseBrick { brick } if brick == "brick:1")
            ),
            "Enter puts the best match in hand"
        );
        let mut ui = fixture();
        let mut s = BrickSelector::new(&ui.core);
        s.on_key(Key::Escape, Modifiers::NONE, &mut ui.core);
        assert!(
            ui.core
                .cmds
                .contains(&StackCmd::Pop(ScreenId::BrickSelector)),
            "Escape with no search closes"
        );
    }

    #[test]
    fn cart_pointer_favorites_and_tabs() {
        let mut ui = fixture();
        let mut s = BrickSelector::new(&ui.core);
        s.on_wake(&mut ui.core);
        assert!(!ui.core.hud.boxes_visible);
        // `BrickSelectorDlg::onWake`: commandToServer('BSD').
        assert!(ui.drain_actions().iter().any(|(_, a)| matches!(
            a,
            UiAction::Game(GameAction::Emote { name }) if name == "bsd"
        )));
        click(&mut s, "BSD_Brick0", MouseButton::Left, &mut ui.core);
        click(&mut s, "BSD_Brick0", MouseButton::Left, &mut ui.core);
        assert_eq!(ui.core.selector.cart[0], Some(0)); // double-click event does not add twice
        assert_eq!(ui.core.selector.cart[1], None);
        click(&mut s, "BSD_Brick1", MouseButton::Left, &mut ui.core);
        click(&mut s, "BSD_Slot4", MouseButton::Left, &mut ui.core);
        click(&mut s, "BSD_Slot0", MouseButton::Left, &mut ui.core);
        click(&mut s, "BSD_Slot4", MouseButton::Left, &mut ui.core);
        assert_eq!(
            (ui.core.selector.cart[0], ui.core.selector.cart[4]),
            (Some(1), Some(0))
        );
        ui.core.selector.toggle_set_favs();
        s.on_key(Key::Digit(3), Modifiers::NONE, &mut ui.core);
        assert!(ui.drain_actions().iter().any(|(_, a)| matches!(a, UiAction::SaveSettings(settings) if settings.brick_favorites[&3][0] == "Second")));
        ui.core.selector.clear_cart();
        s.on_key(Key::Digit(3), Modifiers::NONE, &mut ui.core);
        assert_eq!(ui.core.selector.cart[4], Some(0));
        s.on_key(Key::Tab, Modifiers::NONE, &mut ui.core);
        assert_eq!(ui.core.selector.tab, 1);
        assert!(!s.view.is_shown(s.view.id("BSD_Brick0").unwrap()));
        assert!(s.view.is_shown(s.view.id("BSD_Brick2").unwrap()));
        s.on_key(Key::Escape, Modifiers::NONE, &mut ui.core);
        assert!(
            !ui.drain_actions()
                .iter()
                .any(|(_, a)| matches!(a, UiAction::BuyBricks { .. }))
        );
        s.on_sleep(&mut ui.core);
        assert!(ui.core.hud.boxes_visible);
    }

    #[test]
    fn buy_pending_rejection_retry_and_authoritative_inventory() {
        let mut ui = fixture();
        ui.core.selector.cart[2] = Some(1);
        let mut s = BrickSelector::new(&ui.core);
        s.on_key(Key::Return, Modifiers::NONE, &mut ui.core);
        s.on_key(Key::Return, Modifiers::NONE, &mut ui.core);
        let actions = ui.drain_actions();
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0].1, UiAction::BuyBricks { slots } if slots.len() == 10 && slots[2].as_deref() == Some("brick:1"))
        );
        assert!(
            !ui.core
                .cmds
                .contains(&StackCmd::Pop(ScreenId::BrickSelector))
        );
        let id = actions[0].0;
        ui.core.pending.remove(&id);
        assert!(!s.on_result(id + 1, Some(&Pending::Buy), &Ok(()), &mut ui.core));
        assert!(s.on_result(
            id,
            Some(&Pending::Buy),
            &Err("Trust denied".into()),
            &mut ui.core
        ));
        assert_eq!(ui.core.selector.cart[2], Some(1));
        assert!(
            ui.core
                .cmds
                .iter()
                .any(|cmd| matches!(cmd, StackCmd::Message(m) if m.text == "Trust denied"))
        );
        ui.core.cmds.clear();
        s.on_key(Key::Return, Modifiers::NONE, &mut ui.core);
        let id = ui.drain_actions()[0].0;
        ui.apply(UiUpdate::BrickInventory(vec![Some("brick:1".into())]));
        ui.core.pending.remove(&id);
        s.on_result(id, Some(&Pending::Buy), &Ok(()), &mut ui.core);
        assert!(
            ui.core
                .cmds
                .contains(&StackCmd::Pop(ScreenId::BrickSelector))
        );
        assert_eq!(ui.core.hud.bricks[0].as_ref().unwrap().id, "brick:1");
    }

    #[test]
    fn instant_use_and_catalog_reorder_preserve_cart_identity() {
        let mut ui = fixture();
        ui.core.selector.cart[0] = Some(0);
        let mut s = BrickSelector::new(&ui.core);
        click(&mut s, "BSD_Brick1", MouseButton::Right, &mut ui.core);
        let actions = ui.drain_actions();
        assert!(matches!(&actions[0].1, UiAction::InstantUseBrick { brick } if brick == "brick:1"));
        assert_eq!(ui.core.selector.cart[0], Some(0));
        ui.core.pending.remove(&actions[0].0);
        s.on_result(actions[0].0, Some(&Pending::Buy), &Ok(()), &mut ui.core);
        assert!(ui.drain_actions().is_empty());
        assert_eq!(ui.core.hud.last_instant_use.as_ref().unwrap().id, "brick:1");
        let mut reordered = ui.core.bricks.clone();
        reordered.swap(0, 2);
        ui.apply(UiUpdate::Bricks(reordered));
        s.on_update(&mut ui.core);
        assert_eq!(ui.core.selector.cart[0], Some(2));
        assert!(
            s.view
                .nodes
                .iter()
                .any(|n| n.state.external_texture == Some(73))
        );
        let mut removed = ui.core.bricks.clone();
        removed.remove(2);
        ui.apply(UiUpdate::Bricks(removed));
        s.on_update(&mut ui.core);
        assert_eq!(ui.core.selector.cart[0], None);
    }

    fn prints(ui: &mut Ui) {
        ui.core.print_aspect = Some("2x2f".into());
        ui.core.prints.insert(
            "2x2f".into(),
            vec![PrintInfo {
                id: "print:tile".into(),
                name: "Tile".into(),
                icon: IconRef::None,
            }],
        );
        ui.core.prints.insert(
            "Letters".into(),
            ["A", "-bang", "-qmark", "-space"]
                .iter()
                .map(|n| PrintInfo {
                    id: format!("letters:{n}"),
                    name: (*n).into(),
                    icon: IconRef::None,
                })
                .collect(),
        );
        ui.core.prints.insert(
            "incompatible".into(),
            vec![PrintInfo {
                id: "wrong".into(),
                name: "Bad".into(),
                icon: IconRef::None,
            }],
        );
    }

    #[test]
    fn letter_shortcuts_work_on_the_prints_tab() {
        let mut ui = fixture();
        prints(&mut ui);
        let mut s = PrintSelector::new(&ui.core);
        assert!(!s.showing_letters);
        assert!(s.on_key(Key::Letter('a'), Modifiers::NONE, &mut ui.core));
        let actions = ui.drain_actions();
        assert_eq!(
            actions[0].1,
            UiAction::SetPrint {
                print: "letters:A".into()
            }
        );
    }

    #[test]
    fn print_aspect_shortcuts_pending_rejection_and_cancel() {
        let mut ui = fixture();
        prints(&mut ui);
        let mut s = PrintSelector::new(&ui.core);
        assert_eq!(s.prints.len(), 1);
        assert!(!s.showing_letters);
        assert!(!s.on_key(Key::Letter('b'), Modifiers::NONE, &mut ui.core));
        s.showing_letters = true;
        s.refresh();
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        assert!(s.on_key(Key::Slash, shift, &mut ui.core));
        assert!(s.on_key(Key::Digit(1), shift, &mut ui.core)); // pending consumes, doesn't send
        let actions = ui.drain_actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].1,
            UiAction::SetPrint {
                print: "letters:-qmark".into()
            }
        );
        ui.core.pending.remove(&actions[0].0);
        s.on_result(
            actions[0].0,
            Some(&Pending::Print),
            &Err("Brick removed".into()),
            &mut ui.core,
        );
        assert!(!ui.core.last_print.contains_key("2x2f"));
        s.on_key(Key::Letter('a'), Modifiers::NONE, &mut ui.core);
        let id = ui.drain_actions()[0].0;
        ui.core.pending.remove(&id);
        s.on_result(id, Some(&Pending::Print), &Ok(()), &mut ui.core);
        assert_eq!(ui.core.last_print["2x2f"], "letters:A");
        s.on_sleep(&mut ui.core);
        assert!(ui.core.print_letters_visible);
        assert!(ui.drain_actions().is_empty());
        let mut reopened = PrintSelector::new(&ui.core);
        assert!(reopened.showing_letters);
        reopened.on_key(Key::Escape, Modifiers::NONE, &mut ui.core);
        reopened.on_sleep(&mut ui.core);
        assert_eq!(
            ui.drain_actions()
                .iter()
                .filter(|(_, a)| *a == UiAction::ClosePrintSelector)
                .count(),
            1
        );
    }

    #[test]
    fn every_stock_symbol_shortcut_is_mapped() {
        for (name, key, shift) in [
            ("-bang", Key::Digit(1), true),
            ("-at", Key::Digit(2), true),
            ("-pound", Key::Digit(3), true),
            ("-dollar", Key::Digit(4), true),
            ("-percent", Key::Digit(5), true),
            ("-caret", Key::Digit(6), true),
            ("-and", Key::Digit(7), true),
            ("-asterisk", Key::Digit(8), true),
            ("-minus", Key::Minus, false),
            ("-equals", Key::Equals, false),
            ("-plus", Key::Equals, true),
            ("-period", Key::Period, false),
            ("-less than", Key::Comma, true),
            ("-greater than", Key::Period, true),
            ("-qmark", Key::Slash, true),
            ("-apostrophe", Key::Apostrophe, false),
            ("-space", Key::Space, false),
            ("A", Key::Letter('a'), false),
        ] {
            assert_eq!(
                print_shortcut(name),
                Some(Chord {
                    key,
                    mods: Modifiers {
                        shift,
                        ..Modifiers::NONE
                    }
                }),
                "{name}"
            );
        }
        assert_eq!(print_shortcut("unmapped tile"), None);
    }

    /// Draws every brick tab and the print selector's letters on `pack`
    /// at 1024x768 and renders them offscreen. `letters` is the image id
    /// prefix of the letter prints. Returns the window and letter count.
    fn selector_pack_draw_check(
        pack: Rc<Pack>,
        bricks: Vec<BrickInfo>,
        letters: &str,
    ) -> (Rect, usize) {
        let mut ui = fixture();
        ui.core.pack = pack.clone();
        ui.core.logical = (1024, 768);
        ui.core.bricks = bricks;
        assert!(!ui.core.bricks.is_empty());
        let mut bricks = BrickSelector::new(&ui.core);
        bricks.on_wake(&mut ui.core);
        let window = bricks.view.node(bricks.view.id("BSD_Window").unwrap()).rect;
        assert_eq!(
            (window.x, window.y),
            ((1024 - window.w) / 2, (768 - window.h) / 2),
            "the selector window is centred"
        );
        let tile = bricks.view.node(bricks.view.id("BSD_Brick0").unwrap()).rect;
        assert_eq!((tile.w, tile.h), (96, 96));
        let mut dl = DrawList::new(Rect::new(0, 0, 1024, 768));
        for tab in 0..bricks.tabs.len() {
            ui.core.selector.tab = tab;
            bricks.refresh(&ui.core);
            bricks.draw(&pack, &mut dl, &ui.core);
        }
        assert!(dl.glyph_count() > 100);
        ui.core.print_aspect = Some("2x2f".into());
        ui.core.prints.insert(
            "Letters".into(),
            pack.data
                .images
                .keys()
                .filter(|p| p.starts_with(letters))
                .map(|p| PrintInfo {
                    id: p.clone(),
                    name: p.rsplit('/').next().unwrap().into(),
                    icon: IconRef::Pack(p.clone()),
                })
                .collect(),
        );
        let prints = PrintSelector::new(&ui.core);
        assert!(!prints.letters.is_empty());
        let letter_count = prints.letters.len();
        prints.draw(&pack, &mut dl, &ui.core);
        for cmd in &dl.cmds {
            if let crate::draw::DrawCmd::Image {
                tex: crate::pack::TexKey::Image(id),
                ..
            } = cmd
            {
                assert!(pack.has_image(id), "missing selector artwork {id}");
            }
        }
        #[cfg(feature = "gpu")]
        {
            let gpu = crate::gpu::Headless::new().unwrap();
            let mut renderer = crate::gpu::UiRenderer::new(&gpu.device, &gpu.queue);
            let rgba = gpu
                .render_rgba(
                    &mut renderer,
                    &pack,
                    &dl,
                    (1024, 768),
                    1.0,
                    [0.0, 0.0, 0.0, 1.0],
                )
                .unwrap();
            assert_eq!(rgba.len(), 1024 * 768 * 4);
            assert!(renderer.missing_textures().next().is_none());
        }
        (window, letter_count)
    }

    #[test]
    fn selector_pack_draw_check_synthetic() {
        let mut data = fixture().core.pack.data.clone();
        if let Some(window) = data
            .layouts
            .get_mut("BrickSelectorDlg")
            .and_then(|l| l.children.first_mut())
        {
            window.h_sizing = crate::schema::HSizing::Center;
            window.v_sizing = crate::schema::VSizing::Center;
        }
        let mut bricks = vec![];
        for (i, (category, subcategory)) in [
            ("Bricks", "1x"),
            ("Bricks", "2x"),
            ("Plates", "1x"),
            ("Special", "Doors"),
        ]
        .into_iter()
        .cycle()
        .take(14)
        .enumerate()
        {
            let icon = format!("fixture/bricks/icon{i}");
            crate::testing::add_image(&mut data, &icon, 96, 96);
            bricks.push(BrickInfo {
                id: format!("fixture.brick.{i}"),
                ui_name: format!("Brick {i}"),
                category: category.into(),
                subcategory: subcategory.into(),
                icon: IconRef::Pack(icon),
            });
        }
        for c in 'a'..='z' {
            crate::testing::add_image(&mut data, &format!("fixture/prints/letters/{c}"), 64, 64);
        }
        selector_pack_draw_check(
            crate::testing::pack(data),
            bricks,
            "fixture/prints/letters/",
        );
    }

    #[test]
    #[ignore = "requires generated v20 content"]
    fn authored_selector_pack_draw_check() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = Rc::new(Pack::load(&root.join("content/ui-pack-004")).unwrap());
        let catalog: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("content/stock-catalog-004/stock-catalog.json")).unwrap(),
        )
        .unwrap();
        let bricks: Vec<BrickInfo> = catalog["bricks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["display_name"].as_str().is_some_and(|n| !n.is_empty()))
            .map(|b| BrickInfo {
                id: b["id"].as_str().unwrap().into(),
                ui_name: b["display_name"].as_str().unwrap().into(),
                category: b["category"].as_str().unwrap().into(),
                subcategory: b["subcategory"].as_str().unwrap().into(),
                icon: b["icon_source"]
                    .as_str()
                    .filter(|p| pack.has_image(&p.to_ascii_lowercase()))
                    .map_or(IconRef::None, |p| IconRef::Pack(p.to_ascii_lowercase())),
            })
            .collect();
        assert!(bricks.len() >= 166);
        let (window, letters) =
            selector_pack_draw_check(pack, bricks, "add-ons/print_letters_default/icons/");
        assert_eq!(window, Rect::new(192, 144, 640, 480));
        assert!(letters >= 26);
    }
}
