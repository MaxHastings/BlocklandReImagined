//! Original administration layouts with native typed dispatch, never script eval.
use super::*;
use crate::{models::admin::*, view::EventKind};

pub struct AdminScreen {
    id: ScreenId,
    view: View,
    ids: Vec<u64>,
    map_ids: Vec<String>,
    /// Saved-rank keys in list order.
    keys: Vec<String>,
    sort: usize,
    descending: bool,
    options: Option<AdminOptions>,
}
/// Native rank buttons under adminGui's player list: name, label, the rank
/// given, and where.
const RANK_BUTTONS: [(&str, &str, AdminRole, Rect); 3] = [
    (
        "NativeMakeAdmin",
        "Admin",
        AdminRole::Admin,
        Rect::new(14, 425, 92, 19),
    ),
    (
        "NativeMakeSuperAdmin",
        "Super Admin",
        AdminRole::SuperAdmin,
        Rect::new(108, 425, 92, 19),
    ),
    (
        "NativeDeAdmin",
        "De-Admin",
        AdminRole::Player,
        Rect::new(14, 446, 92, 19),
    ),
];
/// Beside De-Admin: the saved rank list.
const SAVED_RANKS_BUTTON: Rect = Rect::new(108, 446, 92, 19);
/// adminGui's right column, where native buttons go: x, width, the first
/// row (under the status text), button height and the gap between them.
const COLUMN: (i32, i32, i32, i32, i32) = (205, 98, 210, 28, 8);
/// Native buttons flow down the right column, each in the first spot clear
/// of every button already there, so v20's own (Destructo Wand, Change Map,
/// Clear Bricks) keep their place; one that would pass `bottom` starts a new
/// column to the right.
fn flow_buttons(taken: &[Rect], count: usize, bottom: i32) -> Vec<Rect> {
    let (mut x, w, top, h, gap) = COLUMN;
    let mut placed = Vec::with_capacity(count);
    let mut y = top;
    while placed.len() < count {
        if y + h > bottom {
            x += w + gap;
            y = top;
        }
        let r = Rect::new(x, y, w, h);
        match taken.iter().find(|o| o.intersect(&r).is_some()) {
            // Just under the button in the way, as v20 packs its own.
            Some(o) => y = o.bottom() + 2,
            None => {
                placed.push(r);
                y += h + gap;
            }
        }
    }
    placed
}
fn role_label(role: AdminRole) -> &'static str {
    match role {
        AdminRole::SuperAdmin => "Super Admin",
        AdminRole::Admin => "Admin",
        AdminRole::Player => "-",
    }
}
/// How much the player list gives up for them.
const RANK_ROWS_HEIGHT: i32 = 46;
fn set(v: &mut View, name: &str, value: impl Into<String>) {
    if let Some(n) = v.id(name) {
        v.set_text(n, value.into());
    }
}
fn edit(v: &View, name: &str) -> String {
    v.id(name).map(|n| v.edit_text(n)).unwrap_or_default()
}
fn check(v: &View, name: &str) -> bool {
    v.id(name).is_some_and(|n| v.bool_value(n))
}
fn add_named(v: &mut View, parent: NodeId, mut c: Control, name: &str) {
    c.name = Some(name.into());
    v.add(parent, c);
}
fn native_dialog(title: &str) -> View {
    let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
    let mut win = ctrl(
        "GuiWindowCtrl",
        "GuiWindowProfile",
        Rect::new(110, 55, 420, 370),
    );
    win.text = Some(title.into());
    win.h_sizing = HSizing::Center;
    win.v_sizing = VSizing::Center;
    root.children.push(win);
    View::new(&root)
}
/// Stable names for serverConfigGui's unnamed `$Pref::Server::*` fields.
/// v20's "UNKNOWN MAP" picture, which changeMapGui authors in its preview
/// until a map is picked.
const DEFAULT_MAP_PREVIEW: &str = "base/data/missions/default";
/// Native name of changeMapGui's preview: the bitmap authored with
/// [`DEFAULT_MAP_PREVIEW`].
const MAP_PREVIEW: &str = "NativeMapPreview";
fn name_map_preview(view: &mut View) {
    let preview = view.walk().find(|&n| {
        let c = &view.nodes[n].ctrl;
        c.class.eq_ignore_ascii_case("GuiBitmapCtrl")
            && c.bitmap
                .as_deref()
                .is_some_and(|b| b.eq_ignore_ascii_case(DEFAULT_MAP_PREVIEW))
    });
    if let Some(n) = preview {
        view.names.insert(MAP_PREVIEW.into(), n);
    }
}
/// serverConfigGui's own preference for a field this game adds.
fn has_pref(view: &View, var: &str) -> Option<NodeId> {
    view.walk().find(|&n| {
        view.nodes[n]
            .ctrl
            .variable
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case(var))
    })
}
/// Max bots, which v20 did not have: a row under Max Player Vehicles,
/// copied from its label and box, with the controls below moved down to
/// make room.
fn add_max_bots_field(view: &mut View) {
    if has_pref(view, "$Pref::Server::MaxBots").is_some() {
        return;
    }
    let Some(edit) = has_pref(view, "$Pref::Server::MaxPlayerVehicles_Total") else {
        return;
    };
    let Some(parent) = view.nodes[edit].parent else {
        return;
    };
    let field = view.nodes[edit].ctrl.clone();
    let (y, h) = (field.position[1], field.extent[1]);
    // Its label: the text left of it on the same row.
    let label = view.nodes[parent]
        .children
        .iter()
        .copied()
        .filter(|&c| {
            let c = &view.nodes[c].ctrl;
            c.class.eq_ignore_ascii_case("GuiTextCtrl")
                && c.position[0] < field.position[0]
                && (c.position[1] - y).abs() < h
        })
        .max_by_key(|&c| view.nodes[c].ctrl.position[0])
        .map(|c| view.nodes[c].ctrl.clone());
    let step = h + 4;
    // Everything below the row moves down a row, out to the window.
    let (mut node, mut below) = (parent, y + h);
    for c in view.nodes[node].children.clone() {
        if view.nodes[c].ctrl.position[1] >= below {
            view.nodes[c].ctrl.position[1] += step;
        }
    }
    while !view.nodes[node]
        .ctrl
        .class
        .eq_ignore_ascii_case("GuiWindowCtrl")
    {
        let Some(up) = view.nodes[node].parent else {
            break;
        };
        below = view.nodes[node].ctrl.position[1] + view.nodes[node].ctrl.extent[1];
        view.nodes[node].ctrl.extent[1] += step;
        for c in view.nodes[up].children.clone() {
            if c != node && view.nodes[c].ctrl.position[1] >= below {
                view.nodes[c].ctrl.position[1] += step;
            }
        }
        node = up;
    }
    view.nodes[node].ctrl.extent[1] += step;
    if let Some(mut label) = label {
        label.position[1] += step;
        label.text = Some("Max Bots:".into());
        label.name = None;
        view.add(parent, label);
    }
    let mut field = field;
    field.position[1] += step;
    field.variable = Some("$Pref::Server::MaxBots".into());
    field.name = None;
    view.add(parent, field);
}
fn name_option_fields(view: &mut View) {
    add_max_bots_field(view);
    for n in view.walk().collect::<Vec<_>>() {
        if let Some(var) = view.nodes[n].ctrl.variable.clone()
            && let Some(suffix) = var.to_ascii_lowercase().strip_prefix("$pref::server::")
        {
            let name = format!("AdminOption_{suffix}");
            view.names.insert(name.clone(), n);
            view.nodes[n].ctrl.name = Some(name);
        }
    }
}
/// Show `o` in serverConfigGui's fields.
fn fill_options(view: &mut View, o: &AdminOptions) {
    for (k, value) in option_pairs(o) {
        if let Some(n) = view.id(&format!("AdminOption_{}", k.to_ascii_lowercase())) {
            if view.node(n).ctrl.class == "GuiCheckBoxCtrl" {
                view.set_bool(n, value == "1");
            } else {
                view.set_text(n, value);
            }
        }
    }
}
/// serverConfigGui's fields over `base` (which keeps what the form lacks).
fn collect_options(view: &View, base: AdminOptions) -> Result<AdminOptions, String> {
    let mut o = base.clone();
    for (k, _) in option_pairs(&base) {
        let name = format!("AdminOption_{}", k.to_ascii_lowercase());
        let Some(n) = view.id(&name) else { continue };
        let value = if view.node(n).ctrl.class == "GuiCheckBoxCtrl" {
            if view.bool_value(n) {
                "1".into()
            } else {
                "0".into()
            }
        } else {
            view.edit_text(n)
        };
        set_option(&mut o, k, &value)?;
    }
    Ok(o)
}
/// Sends the confirmed request. `changeMapButton::click` also pops
/// changeMapGui and adminGui, so the loading GUI takes over the screen.
fn accept_confirmation(core: &mut Core) {
    if let Some(c) = core.admin.confirmation.take() {
        if matches!(c.action, AdminAction::ChangeMap { .. }) {
            core.pop(ScreenId::AdminMaps);
            core.pop(ScreenId::Admin);
        }
        core.admin_request(c.action);
    }
}
impl AdminScreen {
    pub fn new(id: ScreenId, core: &Core) -> Self {
        let layout = match id {
            ScreenId::Admin => "adminGui",
            ScreenId::AdminLogin => "AdminLoginGui",
            ScreenId::AdminBan => "addBanGui",
            ScreenId::AdminUnban => "unBanGui",
            ScreenId::AdminBricks => "BrickManGui",
            ScreenId::AdminMaps => "changeMapGui",
            ScreenId::AdminOptions => "serverConfigGui",
            ScreenId::AdminConfirm => "MessageBoxYesNoDlg",
            _ => "",
        };
        let mut view = if id == ScreenId::AdminCredentials {
            native_dialog("Server Passwords")
        } else if id == ScreenId::AdminRanks {
            native_dialog("Saved Ranks")
        } else {
            layout_view(core, layout)
        };
        let parent = window(&view).unwrap_or(view.root);
        // Stable native names identify unnamed source buttons for internal tests.
        name_option_fields(&mut view);
        if id == ScreenId::AdminMaps {
            name_map_preview(&mut view);
        }
        if id == ScreenId::Admin {
            let natives = [
                ("NativeEnvironment", "Environment >>"),
                ("NativeHostOptions", "Host Options"),
                ("NativeAdminCredentials", "Passwords"),
                ("NativeAddOnSettings", "Add-On Settings"),
            ];
            let taken: Vec<Rect> = view.nodes[parent]
                .children
                .iter()
                .map(|&c| &view.nodes[c].ctrl)
                .filter(|c| c.class.to_ascii_lowercase().contains("button"))
                .map(|c| Rect::new(c.position[0], c.position[1], c.extent[0], c.extent[1]))
                .collect();
            let extent = view.nodes[parent].ctrl.extent;
            let rects = flow_buttons(&taken, natives.len(), extent[1] - 8);
            for ((name, label), r) in natives.into_iter().zip(rects) {
                let w = &mut view.nodes[parent].ctrl.extent[0];
                *w = (*w).max(r.right() + 9);
                let b = button(
                    "BlockButtonProfile",
                    r,
                    "base/client/ui/button1",
                    label,
                    name,
                );
                add_named(&mut view, parent, b, name);
            }
            // The list (and the swatch behind it) ends above the rank rows.
            for c in view.nodes[parent].children.clone() {
                let ctrl = &mut view.nodes[c].ctrl;
                if ctrl.position == [14, 52]
                    && (ctrl.class.eq_ignore_ascii_case("GuiScrollCtrl")
                        || ctrl.class.eq_ignore_ascii_case("GuiSwatchCtrl"))
                {
                    ctrl.extent[1] -= RANK_ROWS_HEIGHT;
                }
            }
            for (name, label, _, rect) in RANK_BUTTONS {
                let b = button(
                    "BlockButtonProfile",
                    rect,
                    "base/client/ui/button1",
                    label,
                    name,
                );
                add_named(&mut view, parent, b, name);
            }
            let b = button(
                "BlockButtonProfile",
                SAVED_RANKS_BUTTON,
                "base/client/ui/button1",
                "Saved Ranks >>",
                "NativeSavedRanks",
            );
            add_named(&mut view, parent, b, "NativeSavedRanks");
            for n in view.walk().collect::<Vec<_>>() {
                if view.nodes[n].ctrl.text.as_deref() == Some("BL_ID") {
                    view.set_text(n, "Identity");
                }
            }
        }
        if id == ScreenId::AdminRanks {
            view.add(
                parent,
                text(
                    "GuiTextProfile",
                    Rect::new(15, 30, 390, 22),
                    "Players get these ranks back when they rejoin.",
                ),
            );
            let mut scroll = ctrl(
                "GuiScrollCtrl",
                "BlockScrollProfile",
                Rect::new(15, 55, 390, 250),
            );
            scroll
                .fields
                .insert("hScrollBar".into(), "alwaysOff".into());
            scroll.fields.insert("vScrollBar".into(), "dynamic".into());
            let mut list = ctrl(
                "GuiTextListCtrl",
                "GuiTextListProfile",
                Rect::new(0, 0, 374, 16),
            );
            list.name = Some("AdminRanks_list".into());
            list.fields.insert("columns".into(), "0 170 270".into());
            scroll.children.push(list);
            view.add(parent, scroll);
            for (name, label, x) in [
                ("AdminForgetRank", "Remove", 15),
                ("AdminCloseRanks", "Close", 307),
            ] {
                add_named(
                    &mut view,
                    parent,
                    button(
                        "BlockButtonProfile",
                        Rect::new(x, 315, 98, 28),
                        "base/client/ui/button1",
                        label,
                        name,
                    ),
                    name,
                );
            }
        }
        if id == ScreenId::AdminCredentials {
            for (name, label, y, password) in [
                ("AdminServerName", "Server name", 45, false),
                ("AdminMaxPlayers", "Max players", 83, false),
                ("AdminNewPassword", "New password", 190, true),
            ] {
                view.add(
                    parent,
                    text("GuiTextProfile", Rect::new(15, y, 125, 22), label),
                );
                let mut c = ctrl(
                    "GuiTextEditCtrl",
                    "GuiTextEditProfile",
                    Rect::new(145, y, 250, 22),
                );
                if password {
                    c.fields.insert("password".into(), "1".into());
                }
                add_named(&mut view, parent, c, name);
            }
            view.add(
                parent,
                text(
                    "GuiTextProfile",
                    Rect::new(15, 152, 125, 22),
                    "Password type",
                ),
            );
            add_named(
                &mut view,
                parent,
                ctrl(
                    "GuiPopUpMenuCtrl",
                    "GuiPopUpMenuProfile",
                    Rect::new(145, 152, 250, 22),
                ),
                "AdminPasswordSlot",
            );
            for (name, label, x, y) in [
                ("AdminApplyIdentity", "Apply name / capacity", 145, 115),
                ("AdminApplyPassword", "Set password", 145, 225),
                ("AdminClearPassword", "Clear password", 275, 225),
                ("AdminCloseCredentials", "Close", 295, 300),
            ] {
                add_named(
                    &mut view,
                    parent,
                    button(
                        "BlockButtonProfile",
                        Rect::new(x, y, 125, 28),
                        "base/client/ui/button1",
                        label,
                        name,
                    ),
                    name,
                );
            }
            view.add(
                parent,
                text(
                    "GuiTextProfile",
                    Rect::new(15, 266, 390, 22),
                    "Changes take effect once the server accepts them.",
                ),
            );
        }
        if id == ScreenId::AdminOptions {
            if let Some(n) = view.by_command("canvas.popDialog(ServerConfigGui);") {
                view.set_text(n, "Apply");
            }
            // Defaults require a verified host reset adapter; no hidden local-pref write.
            if let Some(n) = view.by_command("ServerConfigGui.clickDefaults();") {
                view.set_active(n, false);
                view.set_text(n, "Host defaults");
            }
        }
        if id != ScreenId::AdminConfirm {
            let r = if id == ScreenId::Admin {
                // Above the Environment button.
                Rect::new(205, 158, 98, 48)
            } else {
                let h = view.nodes[parent].ctrl.extent[1];
                // The status row grows the window, but never past v20's
                // 640x480 canvas, the smallest the automatic UI scale
                // leaves (2560x1440 is 853x480): a full-height window takes
                // the row from its tallest scroll box instead.
                let over = (h + 29 - 480).max(0);
                let mut span = (12, view.nodes[parent].ctrl.extent[0] - 24);
                if over > 0 {
                    let children = view.nodes[parent].children.clone();
                    let scroll = children
                        .iter()
                        .copied()
                        .filter(|&c| {
                            view.nodes[c]
                                .ctrl
                                .class
                                .eq_ignore_ascii_case("GuiScrollCtrl")
                        })
                        .max_by_key(|&c| view.nodes[c].ctrl.extent[1]);
                    if let Some(scroll) = scroll {
                        let bottom =
                            view.nodes[scroll].ctrl.position[1] + view.nodes[scroll].ctrl.extent[1];
                        view.nodes[scroll].ctrl.extent[1] -= over;
                        // Under the box it came from, clear of the buttons.
                        span = (
                            view.nodes[scroll].ctrl.position[0],
                            view.nodes[scroll].ctrl.extent[0],
                        );
                        for c in children {
                            if view.nodes[c].ctrl.position[1] >= bottom {
                                view.nodes[c].ctrl.position[1] -= over;
                            }
                        }
                    }
                }
                // Controls v20 parked below the window (AdminLoginGui's
                // escape `closer`) stay clipped out of sight.
                for c in view.nodes[parent].children.clone() {
                    if view.nodes[c].ctrl.position[1] >= h {
                        view.nodes[c].ctrl.position[1] += 29 - over;
                    }
                }
                view.nodes[parent].ctrl.extent[1] = h + 29 - over;
                Rect::new(span.0, h - over, span.1, 27)
            };
            let mut c = text("GuiMLTextProfile", r, "");
            c.class = "GuiMLTextCtrl".into();
            add_named(&mut view, parent, c, "NativeAdminStatus");
        }
        let options = core.admin.snapshot.as_ref().and_then(|s| s.options.clone());
        let mut screen = Self {
            id,
            view,
            ids: Vec::new(),
            map_ids: Vec::new(),
            keys: Vec::new(),
            sort: 0,
            descending: false,
            options,
        };
        if id == ScreenId::AdminBan {
            for (name, max, label) in [
                ("AddBan_Days", 15, "Days"),
                ("AddBan_Hours", 23, "Hours"),
                ("AddBan_Minutes", 59, "Minutes"),
            ] {
                if let Some(n) = screen.view.id(name) {
                    screen.view.state(n).items =
                        (0..=max).map(|v| (format!("{v} {label}"), v)).collect();
                    screen.view.select(n, Some(0));
                }
            }
            if let Some(n) = screen.view.id("AddBan_Forever") {
                screen.view.set_bool(n, false);
            }
        }
        screen.populate_options();
        screen.refresh(core);
        screen
    }
    fn populate_options(&mut self) {
        if let Some(o) = &self.options {
            fill_options(&mut self.view, o);
            set(&mut self.view, "AdminServerName", o.name.clone());
            set(&mut self.view, "AdminMaxPlayers", o.max_players.to_string());
        }
    }
    fn status(&mut self, core: &Core) {
        let message = if !core.admin.status.is_empty() {
            core.admin.status.clone()
        } else if core.admin.busy() {
            "Waiting for host...".into()
        } else if core.admin.snapshot.is_none() {
            "Administration information is unavailable.".into()
        } else {
            "Permissions and actions are checked by the host.".into()
        };
        set(
            &mut self.view,
            "NativeAdminStatus",
            message.replace(['<', '>'], ""),
        );
    }
    fn refresh(&mut self, core: &Core) {
        let busy = core.admin.busy();
        let m = &core.admin;
        if self.id == ScreenId::Admin {
            let mut rows = m
                .snapshot
                .as_ref()
                .map(|s| s.players.clone())
                .unwrap_or_default();
            rows.sort_by(|a, b| {
                if self.sort == 1 {
                    a.identity_label
                        .cmp(&b.identity_label)
                        .then(a.connection.cmp(&b.connection))
                } else {
                    a.name
                        .to_lowercase()
                        .cmp(&b.name.to_lowercase())
                        .then(a.connection.cmp(&b.connection))
                }
            });
            if self.descending {
                rows.reverse();
            }
            self.ids = rows.iter().map(|p| p.connection).collect();
            if let Some(n) = self.view.id("lstAdminPlayerList") {
                self.view.state(n).items = rows
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (format!("{}\t{}", p.name, p.identity_label), i as i64))
                    .collect();
                self.view.select(
                    n,
                    m.selected_player
                        .and_then(|id| self.ids.iter().position(|x| *x == id))
                        .map(|i| i as i64),
                );
            }
            if let Some(n) = self.view.id("adminGui_banBlocker") {
                self.view
                    .set_visible(n, m.snapshot.as_ref().is_some_and(|s| s.legacy_lan));
            }
            let t = m.selected_player.unwrap_or(0);
            for (cmd, a) in [
                ("AdminGui_KickPlayer();", AdminAction::Kick { target: t }),
                (
                    "AdminGui_BanPlayer();",
                    AdminAction::Ban {
                        target: t,
                        minutes: None,
                        reason: String::new(),
                    },
                ),
                ("adminGui::spy();", AdminAction::Spy { target: t }),
                ("AdminGui_Wand();", AdminAction::Wand),
                ("canvas.pushDialog(unBanGui);", AdminAction::RequestBans),
                ("canvas.pushdialog(changeMapGui);", AdminAction::RequestMaps),
                (
                    "AdminGui.ClickClearBricks();",
                    AdminAction::RequestBrickGroups,
                ),
            ] {
                if let Some(n) = self.view.by_command(cmd) {
                    self.view.set_active(n, !busy && m.allowed(&a));
                }
            }
            let ranks = m.available(AdminFeature::Ranks);
            if let Some(n) = self.view.id("NativeSavedRanks") {
                self.view.set_visible(n, ranks);
                self.view.set_active(n, !busy && ranks);
            }
            for (name, _, role, _) in RANK_BUTTONS {
                if let Some(n) = self.view.id(name) {
                    self.view.set_visible(n, ranks);
                    self.view.set_active(
                        n,
                        !busy && m.allowed(&AdminAction::SetRole { target: t, role }),
                    );
                }
            }
            if let Some(n) = self.view.id("NativeEnvironment") {
                let environment = m.available(AdminFeature::Environment);
                self.view.set_visible(n, environment);
                self.view.set_active(n, !busy && environment);
            }
            if let Some(n) = self.view.id("NativeHostOptions") {
                self.view
                    .set_visible(n, m.snapshot.as_ref().is_some_and(|s| s.local_host));
                self.view.set_active(
                    n,
                    !busy
                        && m.available(AdminFeature::HostOptions)
                        && m.snapshot.as_ref().is_some_and(|s| s.options.is_some()),
                );
            }
            if let Some(n) = self.view.id("NativeAddOnSettings") {
                // The running Add-Ons' server-wide settings, which only the
                // host changes, as Host Options.
                let any = core.minigames.addon_settings.iter().any(|s| s.server);
                self.view
                    .set_visible(n, any && m.snapshot.as_ref().is_some_and(|s| s.local_host));
                self.view.set_active(
                    n,
                    !busy
                        && m.available(AdminFeature::HostOptions)
                        && m.snapshot.as_ref().is_some_and(|s| s.options.is_some()),
                );
            }
            if let Some(n) = self.view.id("NativeAdminCredentials") {
                self.view.set_visible(
                    n,
                    m.snapshot
                        .as_ref()
                        .is_some_and(|s| s.local_host || s.role == AdminRole::SuperAdmin),
                );
                self.view.set_active(
                    n,
                    !busy
                        && (m.available(AdminFeature::AdminPassword)
                            || m.available(AdminFeature::HostOptions)),
                );
            }
        } else if self.id == ScreenId::AdminLogin {
            if let Some(n) = self.view.by_command("SAD(txtAdminPass.getValue());") {
                self.view
                    .set_active(n, !busy && m.available(AdminFeature::Login));
            }
            if let Some(n) = self.view.id("txtAdminPass") {
                self.view
                    .set_active(n, !busy && m.available(AdminFeature::Login));
            }
        } else if self.id == ScreenId::AdminBan {
            let t = m.selected_player.unwrap_or(0);
            if let Some(p) = m.player(t) {
                set(
                    &mut self.view,
                    "addBan_Window",
                    format!("BAN {} ({})", p.name, p.identity_label),
                );
            }
            let forever = check(&self.view, "AddBan_Forever");
            if let Some(n) = self.view.id("AddBan_TimeBlocker") {
                self.view.set_visible(n, forever);
            }
            for name in ["AddBan_Days", "AddBan_Hours", "AddBan_Minutes"] {
                if let Some(n) = self.view.id(name) {
                    self.view.set_active(n, !busy && !forever);
                }
            }
            if let Some(n) = self.view.by_command("addBanGui.ban();") {
                self.view.set_active(
                    n,
                    !busy
                        && m.allowed(&AdminAction::Ban {
                            target: t,
                            minutes: None,
                            reason: String::new(),
                        }),
                );
            }
        } else if self.id == ScreenId::AdminUnban {
            let mut rows = m.bans.clone();
            rows.sort_by(|a, b| {
                match self.sort {
                    0 => a.administrator.cmp(&b.administrator),
                    2 => a.identity_label.cmp(&b.identity_label),
                    3 => a.address.cmp(&b.address),
                    4 => a.reason.cmp(&b.reason),
                    6 => a
                        .remaining_minutes
                        .unwrap_or(u64::MAX)
                        .cmp(&b.remaining_minutes.unwrap_or(u64::MAX)),
                    _ => a.name.cmp(&b.name),
                }
                .then(a.id.cmp(&b.id))
            });
            if self.descending {
                rows.reverse();
            }
            self.ids = rows.iter().map(|r| r.id).collect();
            if let Some(n) = self.view.id("unBan_list") {
                self.view.state(n).items = rows
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        (
                            format!(
                                "{}\t{}\t{}\t{}\t{}\t{}",
                                r.administrator,
                                r.name,
                                r.identity_label,
                                r.address.as_deref().unwrap_or("Unavailable"),
                                r.reason,
                                r.remaining_minutes.map_or("Forever".into(), |v| format!(
                                    "{}d {:02}:{:02}",
                                    v / 1440,
                                    v % 1440 / 60,
                                    v % 60
                                ))
                            ),
                            i as i64,
                        )
                    })
                    .collect();
                self.view.select(
                    n,
                    m.selected_ban
                        .and_then(|id| self.ids.iter().position(|x| *x == id))
                        .map(|i| i as i64),
                );
            }
            if let Some(n) = self.view.by_command("unBanGui.clickUnBan();") {
                self.view.set_active(
                    n,
                    !busy
                        && m.allowed(&AdminAction::Unban {
                            ban: m.selected_ban.unwrap_or(0),
                        }),
                );
            }
        } else if self.id == ScreenId::AdminBricks {
            let mut rows = m.groups.clone();
            rows.sort_by(|a, b| {
                match self.sort {
                    0 => a.identity_label.cmp(&b.identity_label),
                    2 => a.bricks.cmp(&b.bricks),
                    _ => a.name.cmp(&b.name),
                }
                .then(a.id.cmp(&b.id))
            });
            if self.descending {
                rows.reverse();
            }
            self.ids = rows.iter().map(|r| r.id).collect();
            if let Some(n) = self.view.id("BrickMan_list") {
                self.view.state(n).items = rows
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        (
                            format!("{}\t{}\t{}", r.identity_label, r.name, r.bricks),
                            i as i64,
                        )
                    })
                    .collect();
                self.view.select(
                    n,
                    m.selected_group
                        .and_then(|id| self.ids.iter().position(|x| *x == id))
                        .map(|i| i as i64),
                );
            }
            for (cmd, a) in [
                (
                    "BrickManGui.clickClear();",
                    AdminAction::ClearBrickGroup {
                        group: m.selected_group.unwrap_or(0),
                    },
                ),
                (
                    "BrickManGui.clickHilight();",
                    AdminAction::HighlightBrickGroup {
                        group: m.selected_group.unwrap_or(0),
                    },
                ),
                ("BrickManGui.clickClearAll();", AdminAction::ClearAllBricks),
            ] {
                if let Some(n) = self.view.by_command(cmd) {
                    let selected =
                        matches!(a, AdminAction::ClearAllBricks) || m.selected_group.is_some();
                    self.view.set_active(n, !busy && selected && m.allowed(&a));
                }
            }
            if let Some(n) = self.view.by_command("BrickManGui.clickBan();") {
                self.view.set_active(n, false);
            }
        } else if self.id == ScreenId::AdminMaps {
            self.map_ids = m.maps.iter().map(|r| r.id.clone()).collect();
            if let Some(n) = self.view.id("changeMapList") {
                self.view.state(n).items = m
                    .maps
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (r.name.clone(), i as i64))
                    .collect();
                self.view.select(
                    n,
                    m.selected_map
                        .as_ref()
                        .and_then(|id| self.map_ids.iter().position(|x| x == id))
                        .map(|i| i as i64),
                );
            }
            if let Some(n) = self.view.id("changeMapButton") {
                self.view.set_active(
                    n,
                    !busy
                        && m.allowed(&AdminAction::ChangeMap {
                            map: m.selected_map.clone().unwrap_or_default(),
                        }),
                );
            }
            set(
                &mut self.view,
                "changeMapName",
                m.maps
                    .iter()
                    .find(|r| Some(&r.id) == m.selected_map.as_ref())
                    .map(|r| r.name.clone())
                    .unwrap_or_default(),
            );
            // The picked map's own picture, from this client's content (the
            // host lists maps by the same ids); else the authored placeholder.
            if let Some(n) = self.view.id(MAP_PREVIEW) {
                let preview = m
                    .selected_map
                    .as_ref()
                    .and_then(|id| core.maps.iter().find(|map| map.id == *id))
                    .map_or(crate::api::IconRef::None, |map| map.preview.clone());
                self.view.set_icon(n, &preview);
            }
            set(
                &mut self.view,
                "changeMapDescription",
                "Only host-supplied native maps are available.",
            );
        } else if self.id == ScreenId::AdminOptions {
            for n in self.view.walk().collect::<Vec<_>>() {
                if self.view.nodes[n].ctrl.variable.is_some() {
                    self.view.set_active(
                        n,
                        !busy && m.available(AdminFeature::HostOptions) && self.options.is_some(),
                    );
                }
            }
            if let Some(n) = self.view.by_command("canvas.popDialog(ServerConfigGui);") {
                self.view.set_active(
                    n,
                    !busy && m.available(AdminFeature::HostOptions) && self.options.is_some(),
                );
            }
        } else if self.id == ScreenId::AdminRanks {
            let mut rows = m.saved_ranks.clone();
            rows.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then(a.key.cmp(&b.key))
            });
            self.keys = rows.iter().map(|r| r.key.clone()).collect();
            if let Some(n) = self.view.id("AdminRanks_list") {
                self.view.state(n).items = rows
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        (
                            format!("{}\t{}\t{}", r.name, role_label(r.role), &r.key[..8]),
                            i as i64,
                        )
                    })
                    .collect();
                self.view.select(
                    n,
                    m.selected_rank
                        .as_ref()
                        .and_then(|key| self.keys.iter().position(|k| k == key))
                        .map(|i| i as i64),
                );
            }
            if let Some(n) = self.view.id("AdminForgetRank") {
                let key = m.selected_rank.clone().unwrap_or_default();
                self.view
                    .set_active(n, !busy && m.allowed(&AdminAction::ForgetRank { key }));
            }
        } else if self.id == ScreenId::AdminCredentials {
            let host = m.available(AdminFeature::HostOptions);
            for name in ["AdminServerName", "AdminMaxPlayers", "AdminApplyIdentity"] {
                if let Some(n) = self.view.id(name) {
                    self.view.set_visible(n, host);
                    self.view
                        .set_active(n, !busy && host && self.options.is_some());
                }
            }
            if let Some(n) = self.view.id("AdminPasswordSlot") {
                let selected = self.view.selected(n);
                // No server checks a join password yet, so there is no
                // Join slot to set.
                self.view.state(n).items = if host {
                    vec![("Admin".into(), 1), ("Super Admin".into(), 2)]
                } else {
                    vec![("Admin".into(), 1)]
                };
                self.view
                    .select(n, selected.filter(|x| host || *x == 1).or(Some(1)));
            }
            for name in [
                "AdminApplyPassword",
                "AdminClearPassword",
                "AdminNewPassword",
            ] {
                if let Some(n) = self.view.id(name) {
                    self.view.set_active(
                        n,
                        !busy && (host || m.available(AdminFeature::AdminPassword)),
                    );
                }
            }
        } else if self.id == ScreenId::AdminConfirm {
            if let Some(c) = &m.confirmation {
                set(&mut self.view, "MBYesNoFrame", &c.title);
                set(
                    &mut self.view,
                    "MBYesNoText",
                    c.text.replace(['<', '>'], ""),
                );
            } else {
                set(
                    &mut self.view,
                    "MBYesNoText",
                    "The target or permission changed. Close this confirmation.",
                );
            }
        }
        self.status(core);
    }
    fn confirm(&mut self, core: &mut Core, title: &str, message: String, action: AdminAction) {
        if !core.admin.allowed(&action) || core.admin.busy() {
            return;
        }
        core.admin.confirmation = Some(AdminConfirmation {
            title: title.into(),
            text: message,
            action,
        });
        core.push(ScreenId::AdminConfirm);
    }
    fn submit_ban(&mut self, core: &mut Core) {
        let mut minutes = 0u32;
        for (name, mult, max) in [
            ("AddBan_Days", 1440, 15),
            ("AddBan_Hours", 60, 23),
            ("AddBan_Minutes", 1, 59),
        ] {
            let n = self
                .view
                .id(name)
                .and_then(|n| self.view.selected(n))
                .unwrap_or(-1);
            if !(0..=max).contains(&n) {
                core.admin.status = "Select a valid ban duration.".into();
                return;
            }
            minutes += n as u32 * mult;
        }
        let forever = check(&self.view, "AddBan_Forever");
        if !forever && minutes == 0 {
            core.admin.status = "Choose a duration greater than zero or Forever.".into();
            return;
        }
        let reason = edit(&self.view, "addBan_reason");
        if reason.len() > 512 || reason.chars().any(char::is_control) {
            core.admin.status =
                "Reason must be at most 512 bytes without control characters.".into();
            return;
        }
        let Some(target) = core.admin.selected_player else {
            return;
        };
        core.admin_request(AdminAction::Ban {
            target,
            minutes: if forever { None } else { Some(minutes) },
            reason,
        });
    }
    fn collect_options(&self) -> Result<AdminOptions, String> {
        collect_options(
            &self.view,
            self.options.clone().ok_or("Host settings unavailable")?,
        )
    }
}
impl Screen for AdminScreen {
    fn id(&self) -> ScreenId {
        self.id
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_wake(&mut self, core: &mut Core) {
        if self.id == ScreenId::AdminLogin {
            set(&mut self.view, "txtAdminPass", "");
            self.view.focus = self.view.id("txtAdminPass");
        }
        let query = match self.id {
            ScreenId::AdminUnban => Some(AdminAction::RequestBans),
            ScreenId::AdminBricks => Some(AdminAction::RequestBrickGroups),
            ScreenId::AdminMaps => Some(AdminAction::RequestMaps),
            ScreenId::AdminRanks => Some(AdminAction::RequestRanks),
            _ => None,
        };
        if let Some(a) = query {
            core.admin_request(a);
        }
        self.refresh(core);
    }
    fn on_sleep(&mut self, _core: &mut Core) {
        set(&mut self.view, "txtAdminPass", "");
        set(&mut self.view, "AdminNewPassword", "");
    }
    fn on_update(&mut self, core: &mut Core) {
        if self.id == ScreenId::AdminLogin && core.admin.is_admin() {
            core.pop(self.id);
            core.push(ScreenId::Admin);
        }
        if self.id == ScreenId::Admin && core.admin.snapshot.is_some() && !core.admin.is_admin() {
            for id in [
                ScreenId::Admin,
                ScreenId::AdminBan,
                ScreenId::AdminUnban,
                ScreenId::AdminBricks,
                ScreenId::AdminMaps,
                ScreenId::AdminOptions,
                ScreenId::AdminCredentials,
                ScreenId::AdminRanks,
                ScreenId::AdminEnvironment,
                ScreenId::AdminColorPicker,
                ScreenId::AdminConfirm,
            ] {
                core.pop(id);
            }
            core.push(ScreenId::AdminLogin);
        }
        self.refresh(core);
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            if self.id == ScreenId::AdminConfirm {
                core.admin.confirmation = None;
            }
            core.pop(self.id);
            return true;
        }
        if self.id == ScreenId::AdminConfirm && matches!(key, Key::Return | Key::NumpadEnter) {
            accept_confirmation(core);
            core.pop(self.id);
            return true;
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if !self.view.node(ev.node).state.active {
            return;
        }
        if ev.kind == EventKind::Close {
            core.pop(self.id);
            return;
        }
        if ev.kind == EventKind::Changed {
            let selected = self
                .view
                .selected(ev.node)
                .and_then(|i| usize::try_from(i).ok());
            match self.view.node(ev.node).ctrl.name.as_deref() {
                Some("lstAdminPlayerList") => {
                    core.admin.selected_player = selected.and_then(|i| self.ids.get(i).copied())
                }
                Some("unBan_list") => {
                    core.admin.selected_ban = selected.and_then(|i| self.ids.get(i).copied())
                }
                Some("BrickMan_list") => {
                    core.admin.selected_group = selected.and_then(|i| self.ids.get(i).copied())
                }
                Some("AdminRanks_list") => {
                    core.admin.selected_rank = selected.and_then(|i| self.keys.get(i).cloned())
                }
                Some("changeMapList") => {
                    core.admin.selected_map = selected.and_then(|i| self.map_ids.get(i).cloned())
                }
                _ => {}
            }
            self.refresh(core);
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        let cmd = event_command(&self.view, ev).to_ascii_lowercase();
        if self.id == ScreenId::AdminConfirm {
            if crate::view::message_answer(&cmd) != Some(false) {
                accept_confirmation(core);
            } else {
                core.admin.confirmation = None;
            }
            core.pop(self.id);
            return;
        }
        if cmd.contains("sortlist(") || cmd.contains("sortnumlist(") {
            if let Some(c) = cmd
                .split('(')
                .nth(1)
                .and_then(|s| s.split(')').next())
                .and_then(|s| s.parse::<usize>().ok())
            {
                self.descending = if c == self.sort {
                    !self.descending
                } else {
                    false
                };
                self.sort = c;
                self.refresh(core);
            }
            return;
        }
        let target = core.admin.selected_player.unwrap_or(0);
        match cmd.as_str() {
            "sad(txtadminpass.getvalue());" => {
                let password = edit(&self.view, "txtAdminPass");
                set(&mut self.view, "txtAdminPass", "");
                if !password.is_empty() && password.len() <= 256 {
                    core.admin_request(AdminAction::Login {
                        password: AdminSecret(password),
                    });
                }
            }
            "admingui_kickplayer();" => {
                if let Some(p) = core.admin.player(target) {
                    self.confirm(
                        core,
                        "Kick Player?",
                        format!("Kick {} ({})?", p.name, p.identity_label),
                        AdminAction::Kick { target },
                    );
                }
            }
            "admingui_banplayer();" => core.push(ScreenId::AdminBan),
            "admingui::spy();" => {
                core.admin_request(AdminAction::Spy { target });
            }
            // `AdminGui_Wand` pops adminGui and escapeMenu too, so the
            // wand is in hand the moment it is picked.
            "admingui_wand();" => {
                core.admin_request(AdminAction::Wand);
                core.pop(ScreenId::Admin);
                core.pop(ScreenId::EscapeMenu);
            }
            "canvas.pushdialog(unbangui);" => core.push(ScreenId::AdminUnban),
            "canvas.pushdialog(changemapgui);" => core.push(ScreenId::AdminMaps),
            "admingui.clickclearbricks();" => {
                if core.admin.snapshot.as_ref().is_some_and(|s| s.legacy_lan) {
                    self.confirm(
                        core,
                        "Clear Bricks?",
                        "Delete all bricks?".into(),
                        AdminAction::ClearAllBricks,
                    );
                } else {
                    core.push(ScreenId::AdminBricks);
                }
            }
            "addbangui.ban();" => self.submit_ban(core),
            "unbangui.clickunban();" => {
                if let Some(b) = core
                    .admin
                    .bans
                    .iter()
                    .find(|b| Some(b.id) == core.admin.selected_ban)
                {
                    self.confirm(
                        core,
                        "Un-Ban Player?",
                        format!("Un-ban {} ({})?", b.name, b.identity_label),
                        AdminAction::Unban { ban: b.id },
                    );
                }
            }
            "brickmangui.clickclear();" => {
                if let Some(g) = core
                    .admin
                    .groups
                    .iter()
                    .find(|g| Some(g.id) == core.admin.selected_group)
                {
                    self.confirm(
                        core,
                        "Clear Bricks?",
                        format!(
                            "Destroy all {} bricks belonging to {} ({})?",
                            g.bricks, g.name, g.identity_label
                        ),
                        AdminAction::ClearBrickGroup { group: g.id },
                    );
                }
            }
            "brickmangui.clickclearall();" => self.confirm(
                core,
                "Clear ALL Bricks?",
                "Destroy ALL bricks?".into(),
                AdminAction::ClearAllBricks,
            ),
            "brickmangui.clickhilight();" => {
                if let Some(group) = core.admin.selected_group {
                    core.admin_request(AdminAction::HighlightBrickGroup { group });
                }
            }
            "changemapbutton.click();" => {
                if let Some(m) = core
                    .admin
                    .maps
                    .iter()
                    .find(|m| Some(&m.id) == core.admin.selected_map.as_ref())
                {
                    self.confirm(
                        core,
                        "Change Map?",
                        format!("Change to {}? The current mission will be cleared.", m.name),
                        AdminAction::ChangeMap { map: m.id.clone() },
                    );
                }
            }
            "nativemakeadmin" | "nativemakesuperadmin" | "nativedeadmin" => {
                let role = match cmd.as_str() {
                    "nativemakeadmin" => AdminRole::Admin,
                    "nativemakesuperadmin" => AdminRole::SuperAdmin,
                    _ => AdminRole::Player,
                };
                if let Some(p) = core.admin.player(target) {
                    let (title, text) = match role {
                        AdminRole::Admin => ("Make Admin?", format!("Make {} an Admin?", p.name)),
                        AdminRole::SuperAdmin => (
                            "Make Super Admin?",
                            format!("Make {} a Super Admin?", p.name),
                        ),
                        AdminRole::Player => {
                            ("De-Admin?", format!("Take {}'s admin away?", p.name))
                        }
                    };
                    self.confirm(core, title, text, AdminAction::SetRole { target, role });
                }
            }
            "nativesavedranks" => core.push(ScreenId::AdminRanks),
            "admincloseranks" => core.pop(ScreenId::AdminRanks),
            "adminforgetrank" => {
                if let Some(r) = core
                    .admin
                    .saved_ranks
                    .iter()
                    .find(|r| Some(&r.key) == core.admin.selected_rank.as_ref())
                    .cloned()
                {
                    self.confirm(
                        core,
                        "Remove Saved Rank?",
                        format!(
                            "Stop giving {} {} when they join?",
                            r.name,
                            role_label(r.role)
                        ),
                        AdminAction::ForgetRank { key: r.key },
                    );
                }
            }
            "nativeenvironment" => core.push(ScreenId::AdminEnvironment),
            "nativehostoptions" => core.push(ScreenId::AdminOptions),
            "nativeaddonsettings" => {
                core.minigame_addons = None;
                core.server_addon_settings = true;
                core.push(ScreenId::MiniGameAddOns);
            }
            "nativeadmincredentials" => core.push(ScreenId::AdminCredentials),
            "canvas.popdialog(serverconfiggui);" => match self.collect_options() {
                Ok(mut options) => {
                    // Add-On settings change in their own window: keep the
                    // host's latest.
                    if let Some(current) = core
                        .admin
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.options.as_ref())
                    {
                        options.addon_settings = current.addon_settings.clone();
                    }
                    // Only the local host changes these; they are its saved
                    // `$Pref::Server::*`, as in v20.
                    options_to_prefs(&options, &mut core.prefs);
                    core.save_settings();
                    core.admin_request(AdminAction::ConfigureHost {
                        options: Box::new(options),
                    });
                }
                Err(e) => core.admin.status = e,
            },
            "adminapplyidentity" => {
                if let Some(mut options) = self.options.clone() {
                    options.name = edit(&self.view, "AdminServerName");
                    if let Ok(n) = edit(&self.view, "AdminMaxPlayers").parse::<u16>() {
                        if !options.name.is_empty()
                            && options.name.len() <= 128
                            && n > 0
                            && n <= 1024
                        {
                            options.max_players = n;
                            core.admin_request(AdminAction::ConfigureHost {
                                options: Box::new(options),
                            });
                        } else {
                            core.admin.status = "Use a server name and capacity of 1–1024.".into();
                        }
                    } else {
                        core.admin.status = "Invalid player capacity.".into();
                    }
                }
            }
            "adminapplypassword" | "adminclearpassword" => {
                let password = if cmd == "adminclearpassword" {
                    String::new()
                } else {
                    edit(&self.view, "AdminNewPassword")
                };
                set(&mut self.view, "AdminNewPassword", "");
                if password.len() <= 256 {
                    let slot = match self
                        .view
                        .id("AdminPasswordSlot")
                        .and_then(|n| self.view.selected(n))
                    {
                        Some(0) => AdminPasswordSlot::Join,
                        Some(2) => AdminPasswordSlot::SuperAdmin,
                        _ => AdminPasswordSlot::Admin,
                    };
                    core.admin_request(AdminAction::SetPassword {
                        slot,
                        password: AdminSecret(password),
                    });
                } else {
                    core.admin.status = "Password must be at most 256 bytes.".into();
                }
            }
            "adminclosecredentials" => core.pop(self.id),
            _ if cmd.starts_with("canvas.popdialog(") => core.pop(self.id),
            _ => {}
        }
        self.refresh(core);
    }
}

/// Start Game's Advanced Config: v20's serverConfigGui over the saved
/// `$Pref::Server::*`, which the next hosted game starts with. Done saves
/// them; Defaults puts v20's values back in the form.
pub struct ServerConfig {
    view: View,
}
impl ServerConfig {
    pub fn new(core: &Core) -> Self {
        let mut view = layout_view(core, "serverConfigGui");
        name_option_fields(&mut view);
        fill_options(&mut view, &options_from_prefs(&core.prefs));
        Self { view }
    }
    fn done(&mut self, core: &mut Core) {
        match collect_options(&self.view, options_from_prefs(&core.prefs)) {
            Ok(options) => {
                options_to_prefs(&options, &mut core.prefs);
                core.save_settings();
                core.pop(ScreenId::ServerConfig);
            }
            Err(e) => core.message_ok("Advanced Config", &e),
        }
    }
}
impl Screen for ServerConfig {
    fn id(&self) -> ScreenId {
        ScreenId::ServerConfig
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.pop(ScreenId::ServerConfig);
            return true;
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if ev.kind == EventKind::Close {
            core.pop(ScreenId::ServerConfig);
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        let command = command_of(&self.view, ev.node).to_ascii_lowercase();
        match command.as_str() {
            "canvas.popdialog(serverconfiggui);" => self.done(core),
            "serverconfiggui.clickdefaults();" => {
                fill_options(&mut self.view, &AdminOptions::default())
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v20's right column as the release shows it: Kick, Ban, Un-Ban, Spy,
    /// the Destructo Wand under Passwords' row, Change Map and Clear Bricks.
    const STOCK: [Rect; 7] = [
        Rect::new(205, 30, 98, 18),
        Rect::new(205, 52, 98, 18),
        Rect::new(205, 74, 98, 18),
        Rect::new(209, 96, 90, 38),
        Rect::new(203, 312, 102, 38),
        Rect::new(205, 382, 98, 36),
        Rect::new(205, 422, 98, 36),
    ];

    #[test]
    fn native_admin_buttons_never_cover_another_button() {
        for count in [4, 6, 12] {
            let placed = flow_buttons(&STOCK, count, 467);
            assert_eq!(placed.len(), count);
            let all: Vec<Rect> = STOCK.iter().chain(&placed).copied().collect();
            for (i, a) in all.iter().enumerate() {
                assert!(a.bottom() <= 467, "{a:?} passes the window foot");
                for b in &all[i + 1..] {
                    assert!(a.intersect(b).is_none(), "{a:?} covers {b:?}");
                }
            }
        }
        // The first three keep their old rows above the wand; Add-On
        // Settings goes under it rather than on top of it.
        let placed = flow_buttons(&STOCK, 4, 467);
        assert_eq!(
            placed.iter().map(|r| r.y).collect::<Vec<_>>(),
            [210, 246, 282, 352]
        );
    }
}
