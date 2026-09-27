//! Player appearance and avatar palette editor, native bindings for v20's
//! AvatarGui/ColorSetGui. Preview requests never commit player preferences.
use super::*;
use crate::api::{AvatarPrefs, IconRef, UiAction};
use crate::schema::AvatarData;
use crate::view::EventKind;

const PARTS: &[&str] = &[
    "Face",
    "Hat",
    "Accent",
    "Decal",
    "Pack",
    "SecondPack",
    "Chest",
    "RArm",
    "LArm",
    "RHand",
    "LHand",
    "Hip",
    "RLeg",
    "LLeg",
];
const COLORS: &[(&str, &str)] = &[
    ("Head", "Head"),
    ("Torso", "Torso"),
    ("Hat", "Hat"),
    ("Accent", "Accent"),
    ("Pack", "Pack"),
    ("SecondPack", "SecondPack"),
    ("Hip", "Hip"),
    ("RightArm", "RArm"),
    ("LeftArm", "LArm"),
    ("RightHand", "RHand"),
    ("LeftHand", "LHand"),
    ("RightLeg", "RLeg"),
    ("LeftLeg", "LLeg"),
];

fn named(mut c: Control, name: &str) -> Control {
    c.name = Some(name.into());
    c
}
fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
fn paired(part: &str) -> Option<&'static str> {
    match part {
        "RArm" => Some("LArm"),
        "LArm" => Some("RArm"),
        "RHand" => Some("LHand"),
        "LHand" => Some("RHand"),
        "RLeg" => Some("LLeg"),
        "LLeg" => Some("RLeg"),
        _ => None,
    }
}
fn tint_key(part: &str) -> String {
    format!(
        "{}Color",
        match part {
            "Face" => "Head",
            "Decal" | "Chest" => "Torso",
            _ => part,
        }
    )
}
fn options(data: &AvatarData, draft: &AvatarPrefs, part: &str) -> Vec<String> {
    match part {
        "Face" => data.faces.clone(),
        "Decal" => data.decals.clone(),
        "Accent" => {
            let hat = data
                .parts
                .get("hat")
                .and_then(|v| v.get(draft.index("Hat")))
                .map(|v| v.to_ascii_lowercase())
                .unwrap_or_default();
            data.accents_allowed
                .get(&hat)
                .cloned()
                .unwrap_or_else(|| vec!["none".into()])
        }
        _ => data
            .parts
            .get(&part.to_ascii_lowercase())
            .cloned()
            .unwrap_or_default(),
    }
}
fn selected(draft: &AvatarPrefs, part: &str, choices: &[String]) -> usize {
    if matches!(part, "Face" | "Decal") {
        let name = draft.get(&format!("{part}Name")).unwrap_or_default();
        choices
            .iter()
            .position(|p| base_name(p).eq_ignore_ascii_case(base_name(name)))
            .unwrap_or(0)
    } else {
        draft.index(part).min(choices.len().saturating_sub(1))
    }
}
fn icon(pack: &Pack, part: &str, choice: &str) -> String {
    let path = if matches!(part, "Face" | "Decal") {
        let (dir, file) = choice.rsplit_once('/').unwrap_or(("", choice));
        let thumb = format!("{dir}/thumbs/{file}").to_ascii_lowercase();
        if part == "Face" && pack.has_image(&thumb) {
            thumb
        } else {
            choice.to_ascii_lowercase()
        }
    } else {
        format!(
            "base/client/ui/avataricons/{}/{}",
            part.to_ascii_lowercase(),
            choice.to_ascii_lowercase()
        )
    };
    if pack.has_image(&path) {
        path
    } else {
        "base/client/ui/avataricons/none".into()
    }
}

pub struct Avatar {
    view: View,
    draft: AvatarPrefs,
    data: AvatarData,
    palette: Vec<[f32; 4]>,
    picker: Option<(String, bool)>, // part, color picker
    editor: Option<PaletteEditor>,
    setting_favs: bool,
    request: Option<RequestId>,
    random: u64,
    rotation: [f32; 3],
    last_mouse: Option<(i32, i32)>,
}

impl Avatar {
    pub fn new(core: &Core) -> Self {
        let mut s = Self {
            view: layout_view(core, "AvatarGui"),
            draft: core.settings.avatar.clone(),
            data: core.pack.data.data.avatar.clone(),
            palette: if core.settings.avatar_colors.is_empty() {
                core.pack.data.data.avatar_colors.clone()
            } else {
                core.settings.avatar_colors.clone()
            },
            picker: None,
            editor: None,
            setting_favs: false,
            request: None,
            random: core.time_ms.wrapping_add(0x9e3779b97f4a7c15),
            rotation: [0.3, 0.6, 2.52],
            last_mouse: None,
        };
        s.resolve_material_indices();
        s.build(core);
        s
    }

    fn resolve_material_indices(&mut self) {
        // IFL frame numbers belong to this content catalog. Favorites persist
        // material names, so a reordered pack must resolve its frame anew.
        for part in ["Face", "Decal"] {
            let choices = options(&self.data, &self.draft, part);
            let name = self.draft.get(&format!("{part}Name")).unwrap_or_default();
            if let Some(index) = choices
                .iter()
                .position(|choice| base_name(choice).eq_ignore_ascii_case(base_name(name)))
            {
                self.draft.set(&format!("{part}Color"), index.to_string());
            }
        }
    }
    fn build(&mut self, core: &Core) {
        self.view = layout_view(core, "AvatarGui");
        // The exported GUI contains stale runtime-generated menus. Replace their
        // contents from the checked native catalog, never their script object IDs.
        for n in self.view.walk().collect::<Vec<_>>() {
            if self.view.node(n).ctrl.class == "GuiScrollCtrl" {
                self.view.clear_children(n);
                self.view.set_visible(n, false);
            }
        }
        for (name, value) in [
            ("Avatar_Prefix", &self.draft.clan_prefix),
            ("Avatar_Suffix", &self.draft.clan_suffix),
            ("Avatar_Name", &self.draft.lan_name),
        ] {
            if let Some(n) = self.view.id(name) {
                self.view.set_text(n, value);
            }
        }
        if let Some(n) = self.view.id("Avatar_Preview") {
            self.view.nodes[n].ctrl.class = "GuiBitmapCtrl".into();
            let r = self.view.nodes[n].ctrl.clone();
            let parent = self.view.nodes[n].parent.unwrap_or(self.view.root);
            let mut c = ctrl(
                "GuiBitmapButtonCtrl",
                "BlockButtonProfile",
                Rect::new(r.position[0], r.position[1], r.extent[0], r.extent[1]),
            );
            c.command = Some("Avatar_Orbit();".into());
            let orbit = self.view.add(parent, named(c, "Avatar_Orbit"));
            let status = self.view.add(
                parent,
                named(
                    text(
                        "GuiTextProfile",
                        Rect::new(r.position[0] + 25, r.position[1] + 175, 210, 38),
                        "Avatar preview unavailable",
                    ),
                    "Avatar_PreviewStatus",
                ),
            );
            // Preserve the authored overlap: the rightmost color buttons sit
            // above the left edge of the preview's rectangle.
            let siblings = &mut self.view.nodes[parent].children;
            siblings.retain(|id| *id != orbit && *id != status);
            let position = siblings.iter().position(|id| *id == n).unwrap_or(0) + 1;
            siblings.insert(position, orbit);
            siblings.insert(position + 1, status);
        }
        self.refresh(core);
        if let Some((part, color)) = self.picker.clone() {
            self.build_picker(&part, color, core);
        }
        self.view.layout(core.logical.0, core.logical.1);
    }
    fn read_fields(&mut self) {
        if let Some(n) = self.view.id("Avatar_Prefix") {
            self.draft.clan_prefix = self.view.text_of(n);
        }
        if let Some(n) = self.view.id("Avatar_Suffix") {
            self.draft.clan_suffix = self.view.text_of(n);
        }
        if let Some(n) = self.view.id("Avatar_Name") {
            self.draft.lan_name = self.view.text_of(n);
        }
    }
    fn refresh(&mut self, core: &Core) {
        for &part in PARTS {
            let choices = options(&self.data, &self.draft, part);
            let choice = choices
                .get(selected(&self.draft, part, &choices))
                .map_or("none", String::as_str);
            if let Some(n) = self.view.id(&format!("Avatar_{part}Preview")) {
                self.view.state(n).bitmap = Some(icon(&core.pack, part, choice));
                if !matches!(part, "Face" | "Decal") {
                    self.view.state(n).tint = Some(rgba(self.draft.color(&tint_key(part))));
                }
            }
        }
        for &(control, field) in COLORS {
            if let Some(n) = self.view.id(&format!("Avatar_{control}Color")) {
                self.view.state(n).tint = Some(rgba(self.draft.color(&format!("{field}Color"))));
            }
        }
        for (name, field) in [
            ("Avatar_HeadBG", "HeadColor"),
            ("Avatar_DecalBG", "TorsoColor"),
        ] {
            if let Some(n) = self.view.id(name) {
                self.view.state(n).tint = Some(rgba(self.draft.color(field)));
            }
        }
        if let Some(n) = self.view.id("Avatar_SymmetryCheckbox") {
            self.view.set_bool(n, self.draft.symmetry);
        }
        if let Some(n) = self.view.id("AV_FavsHelper") {
            self.view.set_visible(n, self.setting_favs);
            self.view.push_to_back(n);
        }
        for i in 0..10 {
            if let Some(n) = self.view.id(&format!("Avatar_FavButton{i}")) {
                self.view.state(n).tint = Some([
                    255,
                    255,
                    255,
                    if core.settings.avatar_favorites.contains_key(&i) {
                        255
                    } else {
                        128
                    },
                ]);
            }
        }
        if let Some(n) = self.view.id("Avatar_Preview") {
            let state = self.view.state(n);
            state.external_texture = match core.avatar_preview {
                IconRef::External(id) => Some(id),
                _ => None,
            };
            state.bitmap = match &core.avatar_preview {
                IconRef::Pack(p) => Some(p.clone()),
                _ => None,
            };
        }
        if let Some(n) = self.view.id("Avatar_PreviewStatus") {
            self.view
                .set_visible(n, core.avatar_preview == IconRef::None);
        }
        for n in self.view.walk().collect::<Vec<_>>() {
            if self.view.node(n).ctrl.class.contains("Button")
                || self.view.node(n).ctrl.class == "GuiTextEditCtrl"
            {
                self.view.set_active(n, self.request.is_none());
            }
        }
        if let Some(n) = self.view.by_command("Avatar_Done();") {
            self.view.set_text(
                n,
                if self.request.is_some() {
                    "Applying..."
                } else {
                    "Done"
                },
            );
        }
    }
    fn preview(&mut self, core: &mut Core) {
        core.request(UiAction::PreviewAvatar {
            avatar: self.draft.clone(),
            camera_rotation: self.rotation,
            orbit_distance: 4.34,
        });
        self.refresh(core);
    }
    fn build_picker(&mut self, part: &str, color: bool, core: &Core) {
        let parent = self.view.id("Avatar_Window").unwrap_or(self.view.root);
        let anchor = if color {
            let name = COLORS
                .iter()
                .find(|(_, field)| *field == part)
                .map(|(name, _)| *name)
                .unwrap_or(part);
            self.view.id(&format!("Avatar_{name}Color"))
        } else {
            self.view.id(&format!("Avatar_{part}Preview"))
        };
        let Some(anchor) = anchor else {
            return;
        };
        let a = &self.view.node(anchor).ctrl;
        let choices = options(&self.data, &self.draft, part);
        let allowed: Vec<usize> = self
            .palette
            .iter()
            .enumerate()
            .filter(|(_, c)| part == "Accent" || c[3] >= 1.0)
            .map(|(i, _)| i)
            .collect();
        let count = if color { allowed.len() } else { choices.len() };
        let (columns, size) = if color { (6, 32) } else { (4, 64) };
        let width = count.clamp(1, columns) as i32 * size + 12;
        let height = count.div_ceil(columns).max(1) as i32 * size;
        let x = (a.position[0] + a.extent[0]).min(547 - width - 3);
        let y = if !color && matches!(part, "Decal" | "Pack" | "SecondPack") {
            a.position[1] - 64
        } else {
            a.position[1]
        };
        let mut scroll = named(
            ctrl(
                "GuiScrollCtrl",
                "ColorScrollProfile",
                Rect::new(x, y, width, height.min((480 - y) / size * size)),
            ),
            "Avatar_NativePicker",
        );
        scroll.fields.insert("vScrollBar".into(), "alwaysOn".into());
        scroll
            .fields
            .insert("hScrollBar".into(), "alwaysOff".into());
        let menu = self.view.add(parent, scroll);
        let body = self.view.add(
            menu,
            ctrl(
                "GuiControl",
                "GuiDefaultProfile",
                Rect::new(0, 0, width - 12, height),
            ),
        );
        for i in 0..count {
            let r = Rect::new(
                (i % columns) as i32 * size,
                (i / columns) as i32 * size,
                size,
                size,
            );
            let command;
            if color {
                self.view
                    .add(body, swatch(r, rgba(self.palette[allowed[i]])));
                command = format!("NativeAvatarColor({});", allowed[i]);
            } else {
                self.view.add(
                    body,
                    bitmap("GuiDefaultProfile", r, "base/client/ui/btndecalbg"),
                );
                let node = self.view.add(
                    body,
                    bitmap("GuiDefaultProfile", r, &icon(&core.pack, part, &choices[i])),
                );
                if !matches!(part, "Face" | "Decal") {
                    self.view.state(node).tint = Some(rgba(self.draft.color(&tint_key(part))));
                }
                command = format!("NativeAvatarPart({i});");
            }
            self.view.add(
                body,
                named(
                    button(
                        "BlockButtonProfile",
                        r,
                        if color {
                            "base/client/ui/btncolor"
                        } else {
                            "base/client/ui/btndecal"
                        },
                        " ",
                        &command,
                    ),
                    &format!("Avatar_Choice{i}"),
                ),
            );
        }
    }
    fn toggle_picker(&mut self, part: &str, color: bool, core: &Core) {
        self.read_fields();
        let next = Some((part.to_string(), color));
        self.picker = if self.picker == next { None } else { next };
        self.build(core);
    }
    fn set_part(&mut self, part: &str, index: usize) {
        let choices = options(&self.data, &self.draft, part);
        let Some(choice) = choices.get(index) else {
            return;
        };
        if matches!(part, "Face" | "Decal") {
            self.draft.set(&format!("{part}Name"), base_name(choice));
            self.draft.set(&format!("{part}Color"), index.to_string());
        } else {
            self.draft.set(part, index.to_string());
            if self.draft.symmetry && matches!(part, "LArm" | "RArm") {
                self.draft.set(paired(part).unwrap(), index.to_string());
            }
            if part == "Hat" {
                let count = options(&self.data, &self.draft, "Accent").len();
                if self.draft.index("Accent") >= count {
                    self.draft.set("Accent", "0");
                }
            }
        }
    }
    fn set_color(&mut self, part: &str, index: usize) {
        let Some(&color) = self.palette.get(index) else {
            return;
        };
        if part != "Accent" && color[3] < 1.0 {
            return;
        }
        self.draft.set_color(&format!("{part}Color"), color);
        if self.draft.symmetry
            && let Some(other) = paired(part)
        {
            self.draft.set_color(&format!("{other}Color"), color);
        }
    }
    fn favorite(&mut self, index: u8, core: &mut Core) {
        self.read_fields();
        if self.setting_favs {
            core.settings
                .avatar_favorites
                .insert(index, self.draft.clone());
            self.setting_favs = false;
            core.save_settings();
        } else if let Some(saved) = core.settings.avatar_favorites.get(&index) {
            // Original favorites export Avatar::* only: identity and symmetry stay local.
            self.draft.values = saved.values.clone();
            self.resolve_material_indices();
            self.preview(core);
        }
        self.picker = None;
        self.build(core);
    }
    fn roll(&mut self, count: usize) -> usize {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        if count == 0 {
            0
        } else {
            (self.random as usize) % count
        }
    }
    fn randomize(&mut self, core: &mut Core) {
        self.read_fields();
        for part in [
            "Face",
            "Decal",
            "Hat",
            "Pack",
            "SecondPack",
            "LArm",
            "Chest",
        ] {
            let n = options(&self.data, &self.draft, part).len();
            let i = self.roll(n);
            self.set_part(part, i);
        }
        self.draft.set("RArm", self.draft.index("LArm").to_string());
        let normal_hands = self.roll(101) < 70;
        let normal_legs = self.roll(101) < 80;
        let normal_hip = self.roll(101) < 70;
        for (part, normal) in [
            ("LHand", normal_hands),
            ("RHand", normal_hands),
            ("LLeg", normal_legs),
            ("RLeg", normal_legs),
            ("Hip", normal_hip),
        ] {
            let count = options(&self.data, &self.draft, part).len();
            let index = if normal { 0 } else { self.roll(count) };
            self.set_part(part, index);
        }
        let face = self
            .draft
            .get("FaceName")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if face.contains("female") {
            self.set_part("Chest", 1);
        } else if face != "smiley" && face != "smileycreepy" {
            self.set_part("Chest", 0);
        }
        let count = options(&self.data, &self.draft, "Accent").len();
        let accent = if count > 1 && self.roll(6) != 0 {
            1 + self.roll(count - 1)
        } else {
            0
        };
        self.set_part("Accent", accent);
        for part in [
            "Torso",
            "Pack",
            "SecondPack",
            "Hat",
            "Accent",
            "Hip",
            "LLeg",
            "LArm",
            "LHand",
            "RLeg",
            "RArm",
            "RHand",
        ] {
            if self.draft.symmetry && part.starts_with('R') {
                continue;
            }
            let choices: Vec<usize> = self
                .palette
                .iter()
                .take(9)
                .enumerate()
                .filter(|(_, c)| part == "Accent" || c[3] >= 1.0)
                .map(|(i, _)| i)
                .collect();
            if !choices.is_empty() {
                let i = choices[self.roll(choices.len())];
                self.set_color(part, i);
            }
        }
        if normal_hands && self.roll(2) == 1 {
            let color = self.draft.color("HeadColor");
            self.draft.set_color("LHandColor", color);
            if self.draft.symmetry {
                self.draft.set_color("RHandColor", color);
            }
        }
        self.picker = None;
        self.build(core);
        self.preview(core);
    }
    fn done(&mut self, core: &mut Core) {
        if self.request.is_some() {
            return;
        }
        self.read_fields();
        self.request =
            Some(core.request_pending(UiAction::SetAvatar(self.draft.clone()), Pending::Avatar));
        self.picker = None;
        self.build(core);
    }
}

impl Screen for Avatar {
    fn id(&self) -> ScreenId {
        ScreenId::Avatar
    }
    fn view(&self) -> &View {
        self.editor.as_ref().map_or(&self.view, |e| &e.view)
    }
    fn view_mut(&mut self) -> &mut View {
        self.editor.as_mut().map_or(&mut self.view, |e| &mut e.view)
    }
    fn on_wake(&mut self, core: &mut Core) {
        self.preview(core);
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if let Some(editor) = self.editor.as_mut() {
            match editor.event(ev, core) {
                Some(true) => {
                    self.palette = editor.colors.clone();
                    core.settings.avatar_colors = self.palette.clone();
                    core.save_settings();
                    self.editor = None;
                }
                Some(false) => self.editor = None,
                None => {}
            }
            return;
        }
        if self.request.is_some() {
            return;
        }
        if ev.kind == EventKind::Close {
            core.pop(self.id());
            return;
        }
        let command = command_of(&self.view, ev.node);
        if ev.kind == EventKind::Changed {
            if self.view.node(ev.node).ctrl.name.as_deref() == Some("Avatar_SymmetryCheckbox") {
                self.draft.symmetry = self.view.bool_value(ev.node);
                self.preview(core);
            } else {
                self.read_fields();
            }
            return;
        }
        if ev.kind != EventKind::Click {
            return;
        }
        if let Some(part) = command
            .strip_prefix("Avatar_TogglePartMenu(Avatar_")
            .and_then(|s| s.strip_suffix("Menu);"))
        {
            self.toggle_picker(part, false, core);
            return;
        }
        if let Some(control) = command
            .strip_prefix("Avatar_Click")
            .and_then(|s| s.strip_suffix("Color();"))
        {
            if let Some((_, part)) = COLORS.iter().find(|(name, _)| *name == control) {
                self.toggle_picker(part, true, core);
            }
            return;
        }
        if let Some(i) = command
            .strip_prefix("NativeAvatarPart(")
            .and_then(|s| s.strip_suffix(");"))
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Some((part, false)) = self.picker.clone() {
                self.read_fields();
                self.set_part(&part, i);
                self.picker = None;
                self.build(core);
                self.preview(core);
            }
            return;
        }
        if let Some(i) = command
            .strip_prefix("NativeAvatarColor(")
            .and_then(|s| s.strip_suffix(");"))
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Some((part, true)) = self.picker.clone() {
                self.read_fields();
                self.set_color(&part, i);
                self.picker = None;
                self.build(core);
                self.preview(core);
            }
            return;
        }
        let lower = command.to_ascii_lowercase();
        if let Some(i) = lower
            .strip_prefix("avatargui.clickfav(")
            .and_then(|s| s.strip_suffix(");"))
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|i| *i < 10)
        {
            self.favorite(i, core);
            return;
        }
        match lower.as_str() {
            "avatargui.clickx();" => core.pop(self.id()),
            "avatar_done();" => self.done(core),
            "avatar_randomize();" => self.randomize(core),
            "avatargui.clicksetfavs();" => {
                self.setting_favs = !self.setting_favs;
                self.refresh(core);
            }
            "canvas.pushdialog(colorsetgui);" => {
                self.read_fields();
                self.editor = Some(PaletteEditor::new(core, self.palette.clone()));
            }
            _ => {}
        }
    }
    fn on_key(&mut self, key: Key, mods: Modifiers, core: &mut Core) -> bool {
        if self.request.is_some() {
            return true;
        }
        if key == Key::Escape {
            if self.editor.is_some() {
                self.editor = None;
            } else {
                core.pop(self.id());
            }
            return true;
        }
        if self.editor.is_none() && self.view.focus.is_none() && mods.is_empty() {
            if let Key::Digit(i) = key
                && i < 10
            {
                self.favorite(i, core);
                return true;
            }
            if matches!(key, Key::Return | Key::NumpadEnter) {
                self.done(core);
                return true;
            }
        }
        false
    }
    fn on_update(&mut self, core: &mut Core) {
        self.refresh(core);
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
                core.settings.avatar = self.draft.clone();
                for (key, value) in &self.draft.values {
                    core.prefs.set(&format!("$pref::Avatar::{key}"), value);
                }
                core.prefs
                    .set_bool("$pref::Player::Symmetry", self.draft.symmetry);
                core.prefs
                    .set("$pref::Player::LANName", &self.draft.lan_name);
                core.prefs
                    .set("$pref::Player::ClanPrefix", &self.draft.clan_prefix);
                core.prefs
                    .set("$pref::Player::ClanSuffix", &self.draft.clan_suffix);
                core.save_settings();
                core.pop(self.id());
                core.pop(ScreenId::Options);
            }
            Err(reason) => core.message_ok("Appearance Rejected", reason),
        }
        self.refresh(core);
        true
    }
    fn tick(&mut self, _dt: u64, core: &mut Core) {
        let pressed = self.view.pressed.map(|(n, _)| n) == self.view.id("Avatar_Orbit")
            && self.view.pressed.is_some();
        if pressed && self.editor.is_none() && self.request.is_none() {
            let mouse = self.view.mouse;
            if let Some(previous) = self.last_mouse
                && mouse != previous
            {
                self.rotation[2] += (mouse.0 - previous.0) as f32 * 0.01;
                self.rotation[0] =
                    (self.rotation[0] + (mouse.1 - previous.1) as f32 * 0.01).clamp(-1.4, 1.4);
                self.preview(core);
            }
            self.last_mouse = Some(mouse);
        } else {
            self.last_mouse = None;
        }
    }
    fn layout(&mut self, w: i32, h: i32, _core: &mut Core) {
        self.view.layout(w, h);
        if let Some(e) = &mut self.editor {
            e.view.layout(w, h);
        }
    }
    fn draw(&self, pack: &Pack, dl: &mut DrawList, _core: &Core) {
        self.view.draw(pack, dl);
        if let Some(e) = &self.editor {
            e.view.draw(pack, dl);
        }
    }
}

struct PaletteEditor {
    view: View,
    colors: Vec<[f32; 4]>,
    selected: usize,
    hsv: bool,
}
impl PaletteEditor {
    fn new(core: &Core, colors: Vec<[f32; 4]>) -> Self {
        let mut s = Self {
            view: layout_view(core, "ColorSetGui"),
            colors,
            selected: 0,
            hsv: false,
        };
        s.rebuild(core);
        s
    }
    fn rebuild(&mut self, core: &Core) {
        if let Some(n) = self.view.id("ColorSet_Box") {
            self.view.clear_children(n);
            self.view.nodes[n].ctrl.extent =
                [192, (self.colors.len().div_ceil(6).max(1) * 32) as i32];
            for (i, &color) in self.colors.iter().enumerate() {
                let r = Rect::new((i % 6) as i32 * 32, (i / 6) as i32 * 32, 32, 32);
                self.view.add(
                    n,
                    named(swatch(r, rgba(color)), &format!("ColorSetSwatch{i}")),
                );
                self.view.add(
                    n,
                    button(
                        "BlockButtonProfile",
                        r,
                        "base/client/ui/btncolor",
                        " ",
                        &format!("colorSetGui.selectColor({i});"),
                    ),
                );
            }
        }
        self.selected = self.selected.min(self.colors.len().saturating_sub(1));
        self.sync();
        self.view.layout(core.logical.0, core.logical.1);
    }
    fn sync(&mut self) {
        let color = self
            .colors
            .get(self.selected)
            .copied()
            .unwrap_or([0.5, 0.5, 0.5, 1.0]);
        let values = if self.hsv { rgb_to_hsv(color) } else { color };
        for i in 0..4 {
            if let Some(n) = self.view.id(&format!("ColorSetGui_Slider{i}")) {
                self.view.set_num(n, values[i]);
            }
            if i < 3
                && let Some(n) = self.view.id(&format!("colorSetGui_Label{i}"))
            {
                self.view.set_text(
                    n,
                    if self.hsv {
                        ["H", "S", "V"][i]
                    } else {
                        ["R", "G", "B"][i]
                    },
                );
            }
        }
        if let Some(n) = self.view.id("colorSet_Result") {
            self.view.state(n).tint = Some(rgba(color));
        }
        if let Some(n) = self
            .view
            .id(&format!("colorSetGui_option{}", usize::from(self.hsv)))
        {
            self.view.select_radio(n);
        }
    }
    fn event(&mut self, ev: &ViewEvent, core: &Core) -> Option<bool> {
        if ev.kind == EventKind::Close {
            return Some(false);
        }
        let command = command_of(&self.view, ev.node).to_ascii_lowercase();
        if ev.kind == EventKind::Changed && self.view.node(ev.node).ctrl.class == "GuiSliderCtrl" {
            let mut values = [0.0; 4];
            for (i, value) in values.iter_mut().enumerate() {
                if let Some(n) = self.view.id(&format!("ColorSetGui_Slider{i}")) {
                    *value = self.view.num(n);
                }
            }
            if let Some(color) = self.colors.get_mut(self.selected) {
                *color = if self.hsv { hsv_to_rgb(values) } else { values };
            }
            if let Some(n) = self.view.id(&format!("ColorSetSwatch{}", self.selected)) {
                self.view.state(n).tint = self.colors.get(self.selected).copied().map(rgba);
            }
            if let Some(n) = self.view.id("colorSet_Result") {
                self.view.state(n).tint = self.colors.get(self.selected).copied().map(rgba);
            }
            return None;
        }
        if ev.kind != EventKind::Click {
            return None;
        }
        if let Some(index) = command
            .strip_prefix("colorsetgui.selectcolor(")
            .and_then(|s| s.strip_suffix(");"))
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|i| *i < self.colors.len())
        {
            self.selected = index;
            self.sync();
            return None;
        }
        match command.as_str() {
            "colorsetgui.save();canvas.popdialog(colorsetgui);" => return Some(true),
            "colorsetgui.load();canvas.popdialog(colorsetgui);" => return Some(false),
            "colorsetgui.addcolor();" => {
                self.colors.push(
                    self.colors
                        .get(self.selected)
                        .copied()
                        .unwrap_or([0.5, 0.5, 0.5, 1.0]),
                );
                self.selected = self.colors.len() - 1;
                self.rebuild(core);
            }
            "colorsetgui.deletecolor();" => {
                if self.selected < self.colors.len() {
                    self.colors.remove(self.selected);
                }
                self.rebuild(core);
            }
            "colorsetgui.defaults();" => {
                self.colors = core.pack.data.data.avatar_colors.clone();
                self.rebuild(core);
            }
            "colorsetgui.setmode(0);" => {
                self.hsv = false;
                self.sync();
            }
            "colorsetgui.setmode(1);" => {
                self.hsv = true;
                self.sync();
            }
            _ => {}
        }
        None
    }
}
fn rgb_to_hsv([r, g, b, a]: [f32; 4]) -> [f32; 4] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    [
        hue,
        if max <= f32::EPSILON {
            0.0
        } else {
            delta / max
        },
        max,
        a,
    ]
}
fn hsv_to_rgb([h, s, v, a]: [f32; 4]) -> [f32; 4] {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let m = v - c;
    let [r, g, b] = match h as u32 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    [r + m, g + m, b + m, a]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Settings;
    use crate::binds::Platform;
    use crate::input::MouseButton;
    use crate::schema::UiPack;
    use crate::ui::{StackCmd, Ui, UiConfig};
    use std::rc::Rc;

    fn fixture() -> Ui {
        let mut pack = UiPack::default();
        let data = &mut pack.data.avatar;
        for part in PARTS
            .iter()
            .filter(|p| !matches!(**p, "Face" | "Decal" | "Accent"))
        {
            data.parts.insert(
                part.to_ascii_lowercase(),
                vec![format!("{part}0"), format!("{part}1")],
            );
        }
        data.parts.insert(
            "hat".into(),
            vec!["none".into(), "helmet".into(), "scouthat".into()],
        );
        data.parts.insert(
            "accent".into(),
            vec!["none".into(), "plume".into(), "visor".into()],
        );
        data.accents_allowed
            .insert("helmet".into(), vec!["none".into(), "visor".into()]);
        data.accents_allowed
            .insert("scouthat".into(), vec!["none".into(), "plume".into()]);
        data.faces = vec!["faces/smiley".into(), "faces/smileyfemale1".into()];
        data.decals = vec!["decals/none".into(), "decals/tunic".into()];
        pack.data.avatar_colors = vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 0.5],
            [0.0, 0.0, 1.0, 1.0],
        ];
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut window = named(
            ctrl(
                "GuiWindowCtrl",
                "GuiDefaultProfile",
                Rect::new(46, 0, 547, 480),
            ),
            "Avatar_Window",
        );
        for (i, part) in PARTS.iter().enumerate() {
            let rect = Rect::new(20 + (i % 3) as i32 * 98, 41 + (i / 3) as i32 * 69, 64, 64);
            window.children.push(named(
                bitmap("GuiDefaultProfile", rect, ""),
                &format!("Avatar_{part}Preview"),
            ));
            window.children.push(button(
                "BlockButtonProfile",
                rect,
                "",
                " ",
                &format!("Avatar_TogglePartMenu(Avatar_{part}Menu);"),
            ));
        }
        for (i, &(name, _)) in COLORS.iter().enumerate() {
            window.children.push(named(
                swatch(
                    Rect::new(84 + (i % 3) as i32 * 98, 41 + (i / 3) as i32 * 69, 32, 32),
                    [255; 4],
                ),
                &format!("Avatar_{name}Color"),
            ));
        }
        window.children.push(named(
            ctrl(
                "GuiObjectView",
                "GuiDefaultProfile",
                Rect::new(320, 36, 210, 350),
            ),
            "Avatar_Preview",
        ));
        for (i, name) in ["Avatar_Name", "Avatar_Prefix", "Avatar_Suffix"]
            .iter()
            .enumerate()
        {
            window.children.push(named(
                ctrl(
                    "GuiTextEditCtrl",
                    "GuiDefaultProfile",
                    Rect::new(20 + i as i32 * 150, 410, 120, 20),
                ),
                name,
            ));
        }
        window.children.push(named(
            ctrl(
                "GuiCheckBoxCtrl",
                "GuiDefaultProfile",
                Rect::new(20, 390, 100, 20),
            ),
            "Avatar_SymmetryCheckbox",
        ));
        root.children.push(window);
        pack.layouts.insert("AvatarGui".into(), root);
        let mut palette = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        palette.children.push(named(
            ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 192, 192)),
            "ColorSet_Box",
        ));
        for i in 0..4 {
            palette.children.push(named(
                ctrl(
                    "GuiSliderCtrl",
                    "GuiDefaultProfile",
                    Rect::new(220, 50 + i * 30, 200, 20),
                ),
                &format!("ColorSetGui_Slider{i}"),
            ));
        }
        pack.layouts.insert("ColorSetGui".into(), palette);
        let mut settings = Settings {
            binds: Some(vec![]),
            ..Default::default()
        };
        settings.avatar.symmetry = true;
        settings.avatar.lan_name = "Original".into();
        settings.avatar.set("FaceName", "smiley");
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(pack, Default::default())),
            UiConfig {
                size: (640, 480),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            settings,
        );
        ui.core.cmds.clear();
        ui.drain_actions();
        ui
    }
    fn event(screen: &mut Avatar, command: &str, core: &mut Core) {
        let n = if let Some(n) = screen.view().by_command(command) {
            n
        } else {
            let root = screen.view().root;
            screen.view_mut().add(
                root,
                button("BlockButtonProfile", Rect::new(0, 0, 1, 1), "", "", command),
            )
        };
        screen.on_event(
            &ViewEvent {
                node: n,
                kind: EventKind::Click,
            },
            core,
        );
    }
    fn click_choice(screen: &mut Avatar, index: usize, core: &mut Core) {
        let n = screen.view.id(&format!("Avatar_Choice{index}")).unwrap();
        let r = screen.view.node(n).rect;
        let (x, y) = (r.x + r.w / 2, r.y + r.h / 2);
        let mut events = vec![];
        screen
            .view
            .mouse_down(MouseButton::Left, x, y, &core.pack, &mut events);
        screen
            .view
            .mouse_up(MouseButton::Left, x, y, &core.pack, &mut events);
        for e in events {
            screen.on_event(&e, core);
        }
    }

    #[test]
    fn native_picker_hat_dependencies_and_limb_symmetry() {
        let mut ui = fixture();
        let mut s = Avatar::new(&ui.core);
        s.on_wake(&mut ui.core);
        event(
            &mut s,
            "Avatar_TogglePartMenu(Avatar_HatMenu);",
            &mut ui.core,
        );
        click_choice(&mut s, 1, &mut ui.core);
        assert_eq!(s.draft.index("Hat"), 1);
        event(
            &mut s,
            "Avatar_TogglePartMenu(Avatar_AccentMenu);",
            &mut ui.core,
        );
        click_choice(&mut s, 1, &mut ui.core);
        assert_eq!(
            options(&s.data, &s.draft, "Accent")[s.draft.index("Accent")],
            "visor"
        );
        s.set_part("Hat", 2);
        assert_eq!(
            options(&s.data, &s.draft, "Accent")[s.draft.index("Accent")],
            "plume"
        );
        s.set_part("Hat", 0);
        assert_eq!(s.draft.index("Accent"), 0);
        s.set_part("LArm", 1);
        assert_eq!(s.draft.index("RArm"), 1);
        s.set_part("LHand", 1);
        assert_eq!(s.draft.index("RHand"), 0);
        s.set_part("LLeg", 1);
        assert_eq!(s.draft.index("RLeg"), 0);
        s.set_color("LHand", 2);
        assert_eq!(s.draft.color("RHandColor"), [0.0, 0.0, 1.0, 1.0]);
        s.set_color("LHand", 1);
        assert_eq!(s.draft.color("LHandColor")[3], 1.0);
        s.set_color("Accent", 1);
        assert_eq!(s.draft.color("AccentColor")[3], 0.5);
        s.draft.symmetry = false;
        s.set_color("RHand", 0);
        assert_ne!(s.draft.color("RHandColor"), s.draft.color("LHandColor"));
        assert!(
            ui.drain_actions()
                .iter()
                .all(|(_, action)| matches!(action, UiAction::PreviewAvatar { .. }))
        );
    }

    #[test]
    fn preview_cancel_and_acknowledged_commit_are_separate() {
        let mut ui = fixture();
        let original = ui.core.settings.avatar.clone();
        let mut s = Avatar::new(&ui.core);
        s.on_wake(&mut ui.core);
        s.randomize(&mut ui.core);
        assert_eq!(ui.core.settings.avatar, original);
        s.on_key(Key::Escape, Modifiers::NONE, &mut ui.core);
        assert!(ui.core.cmds.contains(&StackCmd::Pop(ScreenId::Avatar)));
        assert!(
            ui.drain_actions()
                .iter()
                .all(|(_, a)| matches!(a, UiAction::PreviewAvatar { .. }))
        );
        ui.core.cmds.clear();
        let mut s = Avatar::new(&ui.core);
        s.set_part("Hat", 1);
        let n = s.view.id("Avatar_Name").unwrap();
        s.view.set_text(n, "Renamed");
        s.done(&mut ui.core);
        s.done(&mut ui.core);
        let actions = ui.drain_actions();
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0].1,UiAction::SetAvatar(a) if a.lan_name=="Renamed" && a.index("Hat")==1)
        );
        s.on_key(Key::Escape, Modifiers::NONE, &mut ui.core);
        assert!(ui.core.cmds.is_empty());
        ui.core.pending.remove(&actions[0].0);
        s.on_result(
            actions[0].0,
            Some(&Pending::Avatar),
            &Err("Denied".into()),
            &mut ui.core,
        );
        assert_eq!(ui.core.settings.avatar, original);
        assert_eq!(s.draft.index("Hat"), 1);
        ui.core.cmds.clear();
        s.done(&mut ui.core);
        let id = ui.drain_actions()[0].0;
        ui.core.pending.remove(&id);
        s.on_result(id, Some(&Pending::Avatar), &Ok(()), &mut ui.core);
        assert_eq!(ui.core.settings.avatar.lan_name, "Renamed");
        assert_eq!(ui.core.prefs.get("$pref::Avatar::Hat"), Some("1"));
        assert_eq!(
            ui.drain_actions()
                .iter()
                .filter(|(_, a)| matches!(a, UiAction::SaveSettings(_)))
                .count(),
            1
        );
    }

    #[test]
    fn favorites_preserve_identity_and_saved_avatar_until_done() {
        let mut ui = fixture();
        let original = ui.core.settings.avatar.clone();
        let mut s = Avatar::new(&ui.core);
        s.set_part("Hat", 1);
        s.setting_favs = true;
        s.favorite(8, &mut ui.core);
        assert_eq!(ui.core.settings.avatar, original);
        assert_eq!(ui.core.settings.avatar_favorites[&8].index("Hat"), 1);
        let favorite = ui.core.settings.avatar_favorites.get_mut(&8).unwrap();
        favorite.set("FaceName", "smileyfemale1");
        favorite.set("FaceColor", "91"); // stale IFL index from another catalog
        s.set_part("Hat", 0);
        s.view
            .set_text(s.view.id("Avatar_Name").unwrap(), "Local name");
        s.favorite(8, &mut ui.core);
        assert_eq!(s.draft.index("Hat"), 1);
        assert_eq!(s.draft.lan_name, "Local name");
        assert_eq!(s.draft.index("FaceColor"), 1);
        assert_eq!(ui.core.settings.avatar, original);
    }

    #[test]
    fn palette_hsv_add_delete_cancel_and_independent_save() {
        let mut ui = fixture();
        let mut s = Avatar::new(&ui.core);
        let original = s.palette.clone();
        event(&mut s, "canvas.pushDialog(colorSetGui);", &mut ui.core);
        event(&mut s, "colorSetGui.addColor();", &mut ui.core);
        assert_eq!(s.editor.as_ref().unwrap().colors.len(), 4);
        event(&mut s, "ColorSetGui.setMode(1);", &mut ui.core);
        let e = s.editor.as_mut().unwrap();
        let slider = e.view.id("ColorSetGui_Slider0").unwrap();
        e.view.set_num(slider, 1.0 / 3.0);
        s.on_event(
            &ViewEvent {
                node: slider,
                kind: EventKind::Changed,
            },
            &mut ui.core,
        );
        let green = s.editor.as_ref().unwrap().colors[3];
        assert!(green[1] > 0.99 && green[0] < 0.01);
        event(
            &mut s,
            "colorSetGui.load();canvas.popDialog(colorSetGui);",
            &mut ui.core,
        );
        assert_eq!(s.palette, original);
        assert!(ui.drain_actions().is_empty());
        event(&mut s, "canvas.pushDialog(colorSetGui);", &mut ui.core);
        event(&mut s, "colorSetGui.deleteColor();", &mut ui.core);
        event(
            &mut s,
            "colorSetGui.save();canvas.popDialog(colorSetGui);",
            &mut ui.core,
        );
        assert_eq!(ui.core.settings.avatar_colors.len(), 2);
        assert_eq!(s.palette.len(), 2);
        assert!(matches!(
            &ui.drain_actions()[0].1,
            UiAction::SaveSettings(_)
        ));
        for color in [
            [0.2, 0.7, 0.9, 0.5],
            [0.0, 0.0, 0.0, 1.0],
            [0.9, 0.9, 0.9, 1.0],
            [1.0, 0.0, 0.7, 1.0],
        ] {
            let roundtrip = hsv_to_rgb(rgb_to_hsv(color));
            for i in 0..4 {
                assert!((roundtrip[i] - color[i]).abs() < 0.0001);
            }
        }
    }

    #[test]
    fn external_preview_and_orbit_do_not_commit_preferences() {
        let mut ui = fixture();
        ui.core.avatar_preview = IconRef::External(82);
        let mut s = Avatar::new(&ui.core);
        let n = s.view.id("Avatar_Preview").unwrap();
        assert_eq!(s.view.node(n).state.external_texture, Some(82));
        assert!(
            !s.view
                .node(s.view.id("Avatar_PreviewStatus").unwrap())
                .state
                .visible
        );
        let orbit = s.view.id("Avatar_Orbit").unwrap();
        s.view.pressed = Some((orbit, MouseButton::Left));
        s.view.mouse = (400, 150);
        s.tick(16, &mut ui.core);
        s.view.mouse = (420, 155);
        s.tick(16, &mut ui.core);
        let a = ui.drain_actions();
        assert_eq!(a.len(), 1);
        assert!(
            matches!(&a[0].1,UiAction::PreviewAvatar {camera_rotation,..} if camera_rotation[2]>2.7)
        );
    }

    #[test]
    #[ignore = "requires locally converted original UI content; run after content setup"]
    fn authored_avatar_pack_and_palette_render_check() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = Rc::new(Pack::load(&root.join("content/ui-pack-001")).unwrap());
        let mut ui = fixture();
        ui.core.pack = pack.clone();
        ui.core.logical = (1024, 768);
        ui.core.settings.avatar = AvatarPrefs::from_prefs(
            &crate::prefs::Prefs::new(&pack.data.data.prefs, &Default::default()),
            &pack.data.data.prefs,
        );
        let mut s = Avatar::new(&ui.core);
        assert!(s.data.faces.len() >= 27 && s.data.decals.len() >= 28);
        let mut dl = DrawList::new(Rect::new(0, 0, 1024, 768));
        s.draw(&pack, &mut dl, &ui.core);
        for part in PARTS {
            s.toggle_picker(part, false, &ui.core);
            s.draw(&pack, &mut dl, &ui.core);
        }
        s.editor = Some(PaletteEditor::new(&ui.core, s.palette.clone()));
        s.draw(&pack, &mut dl, &ui.core);
        assert!(dl.glyph_count() > 100);
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
    }
}
