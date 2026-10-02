//! First-open name prompt, built on v20's `regNameGui` window. v20 never
//! asked; players who missed the Avatar screen's Name box all joined as
//! "Blockhead". Shown once while the saved name is still the stock one.
use super::*;
use crate::view::EventKind;

/// Set once the prompt was answered or skipped.
pub const PROMPTED: &str = "$pref::Player::NamePrompted";
const LAN_NAME: &str = "$pref::Player::LANName";

pub fn should_prompt(core: &Core) -> bool {
    let name = core.settings.avatar.lan_name.trim();
    !core.prefs.bool_or(PROMPTED, false)
        && (name.is_empty() || name.eq_ignore_ascii_case("Blockhead"))
}

/// "Blockhead" plus four digits, so a player who just presses OK still
/// stands apart from everyone else who did.
fn suggestion() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!("Blockhead{}", 1000 + nanos % 9000)
}

pub struct ChooseName {
    view: View,
    field: Option<NodeId>,
}

impl ChooseName {
    pub fn new(core: &Core) -> Self {
        let mut view = layout_view(core, "regNameGui");
        if let Some(n) = view.id("regName_registerWindow") {
            view.set_visible(n, false);
        }
        if let Some(n) = view.id("regName_CurrName") {
            view.set_visible(n, false);
        }
        for n in view.walk().collect::<Vec<_>>() {
            let text = match view.text_of(n).trim() {
                "Register Name" => "Choose Your Name",
                "New Name:" => "Name:",
                "Register >>" => "OK >>",
                "<< Cancel" => "<< Skip",
                "Current Name:" => {
                    view.set_visible(n, false);
                    continue;
                }
                _ => continue,
            };
            view.set_text(n, text);
        }
        let field = view.id("regName_NewName");
        if let Some(n) = field {
            view.set_text(n, suggestion());
            view.state(n).name_text = true;
            view.focus = Some(n);
        }
        Self { view, field }
    }
    fn finish(&mut self, accept: bool, core: &mut Core) {
        if accept && let Some(n) = self.field {
            let name = self.view.edit_text(n).trim().to_string();
            if !name.is_empty() {
                core.settings.avatar.lan_name = name.clone();
                core.prefs.set(LAN_NAME, &name);
            }
        }
        core.prefs.set_bool(PROMPTED, true);
        core.save_settings();
        core.pop(ScreenId::ChooseName);
    }
}

impl Screen for ChooseName {
    fn id(&self) -> ScreenId {
        ScreenId::ChooseName
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        match key {
            Key::Escape => self.finish(false, core),
            Key::Return | Key::NumpadEnter => self.finish(true, core),
            _ => return false,
        }
        true
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        match ev.kind {
            EventKind::Submit => self.finish(true, core),
            EventKind::Close => self.finish(false, core),
            EventKind::Click => match command_of(&self.view, ev.node).as_str() {
                "regNameGui::register();" => self.finish(true, core),
                "canvas.popDialog(regNameGui);" => self.finish(false, core),
                _ => {}
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Settings, UiAction};
    use crate::binds::Platform;
    use crate::schema::UiPack;
    use crate::ui::{Ui, UiConfig};
    use std::rc::Rc;

    fn fixture() -> Ui {
        let mut pack = UiPack::default();
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut window = ctrl(
            "GuiWindowCtrl",
            "GuiDefaultProfile",
            Rect::new(166, 164, 307, 152),
        );
        window.text = Some("Register Name".into());
        window.children.push(text(
            "GuiTextProfile",
            Rect::new(13, 35, 69, 18),
            "Current Name:",
        ));
        window.children.push(text(
            "GuiTextProfile",
            Rect::new(26, 65, 56, 18),
            "New Name:",
        ));
        let mut field = ctrl(
            "GuiTextEditCtrl",
            "GuiDefaultProfile",
            Rect::new(92, 65, 191, 18),
        );
        field.name = Some("regName_NewName".into());
        window.children.push(field);
        let register = "regNameGui::register();";
        window.children.push(button(
            "GuiDefaultProfile",
            Rect::new(178, 98, 91, 38),
            "",
            "Register >>",
            register,
        ));
        let cancel = "canvas.popDialog(regNameGui);";
        window.children.push(button(
            "GuiDefaultProfile",
            Rect::new(38, 98, 91, 38),
            "",
            "<< Cancel",
            cancel,
        ));
        root.children.push(window);
        pack.layouts.insert("regNameGui".into(), root);
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
        ui.core.cmds.clear();
        ui.drain_actions();
        ui
    }

    #[test]
    fn prompt_saves_typed_name_once_and_skip_is_remembered() {
        let mut ui = fixture();
        ui.core.settings.avatar.lan_name = "Blockhead".into();
        assert!(should_prompt(&ui.core));
        let mut s = ChooseName::new(&ui.core);
        let labels: Vec<String> = s
            .view
            .walk()
            .filter(|&n| s.view.is_shown(n))
            .map(|n| s.view.text_of(n))
            .collect();
        for label in ["Choose Your Name", "Name:", "OK >>", "<< Skip"] {
            assert!(labels.iter().any(|l| l == label), "{labels:?}");
        }
        assert!(!labels.iter().any(|l| l == "Current Name:"), "{labels:?}");
        let n = s.field.unwrap();
        assert!(s.view.edit_text(n).starts_with("Blockhead"));
        assert_eq!(s.view.focus, Some(n));
        s.view.set_text(n, "  Max  ");
        s.on_key(Key::Return, Modifiers::NONE, &mut ui.core);
        assert_eq!(ui.core.settings.avatar.lan_name, "Max");
        assert_eq!(ui.core.prefs.get(LAN_NAME), Some("Max"));
        assert!(!should_prompt(&ui.core));
        let saved = ui.drain_actions().into_iter().find_map(|(_, a)| match a {
            UiAction::SaveSettings(s) => Some(s),
            _ => None,
        });
        let saved: Box<Settings> = saved.expect("prompt saves settings");
        assert_eq!(saved.avatar.lan_name, "Max");

        let mut ui = fixture();
        ui.core.settings.avatar.lan_name = "Blockhead".into();
        let mut s = ChooseName::new(&ui.core);
        s.on_key(Key::Escape, Modifiers::NONE, &mut ui.core);
        assert_eq!(ui.core.settings.avatar.lan_name, "Blockhead");
        assert!(!should_prompt(&ui.core));
    }
}
