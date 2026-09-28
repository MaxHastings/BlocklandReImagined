//! v20's `LoadBricksColorGui`: a save whose colours differ from the world's
//! asks how to load them (`LoadBricks_ColorCheck`). Nearest Match paints
//! each brick the closest colour already in the set; Add More Colors
//! appends the save's, and is hidden when they would not fit.
//!
//! Replace Current Color Set is hidden too. v20 offered it because its set
//! held 64 colours; ours holds 256, so a v20 save's colours always append,
//! and replacing would repaint every brick already built.
use super::*;
use crate::api::{ColorLoad, UiAction};
use crate::view::EventKind;

pub struct ColorWarning {
    view: View,
}

impl ColorWarning {
    pub fn new(core: &Core) -> Self {
        let mut view = layout_view(core, "LoadBricksColorGui");
        let replace = view.by_command("ColorWarning_ClickReplace();");
        let append = view.by_command("ColorWarning_ClickAppend();");
        if let Some(replace) = replace {
            let slot = view.nodes[replace].ctrl.position;
            view.set_visible(replace, false);
            // Add More Colors moves up into Replace's place, so the two
            // remaining choices sit together.
            if let Some(append) = append {
                view.nodes[append].ctrl.position = slot;
            }
        }
        if let Some(append) = append {
            view.set_visible(append, core.color_append_fits);
        }
        view.measure(&core.pack);
        Self { view }
    }
    fn choose(&mut self, core: &mut Core, choice: ColorLoad) {
        core.request(UiAction::LoadBricksColors(choice));
        core.pop(ScreenId::LoadBricksColor);
    }
}

impl Screen for ColorWarning {
    fn id(&self) -> ScreenId {
        ScreenId::LoadBricksColor
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            self.choose(core, ColorLoad::Cancel);
            return true;
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if ev.kind == EventKind::Close {
            self.choose(core, ColorLoad::Cancel);
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        let command = command_of(&self.view, ev.node).to_ascii_lowercase();
        match command.as_str() {
            "colorwarning_clickmatch();" => self.choose(core, ColorLoad::Match),
            "colorwarning_clickappend();" => self.choose(core, ColorLoad::Append),
            "colorwarning_clickcancel();" => self.choose(core, ColorLoad::Cancel),
            _ => {}
        }
    }
}
