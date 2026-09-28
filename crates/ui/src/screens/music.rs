//! v20's Music Files (Start Game): which music loops a hosted game offers
//! its music bricks. Each track is `$Music__<Name>` in the prefs, 1 on and
//! -1 off, as `MusicFilesGui::onSleep` exported them; every stock track
//! starts on (`defaultMusicList.cs`).
use super::*;
use crate::prefs::Prefs;
use crate::view::EventKind;

/// The pref that turns a music track (by its menu name) on or off.
pub fn music_pref(name: &str) -> String {
    format!("$Music__{}", name.trim().replace(' ', "_"))
}

/// Whether a hosted game offers this track.
pub fn music_enabled(prefs: &Prefs, name: &str) -> bool {
    prefs.i64_or(&music_pref(name), 1) != -1
}

const ROW: i32 = 18;

pub struct MusicFiles {
    view: View,
    rows: Vec<(NodeId, String)>,
}

impl MusicFiles {
    pub fn new(core: &Core) -> Self {
        let mut view = layout_view(core, "MusicFilesGui");
        let mut rows = vec![];
        if let Some(scroll) = view.id("MFG_Scroll") {
            let w = view.node(scroll).ctrl.extent[0] - 20;
            let mut tracks = core.music_tracks.clone();
            tracks.sort_by_key(|t| t.to_lowercase());
            let n = i32::try_from(tracks.len()).unwrap_or(0);
            let mut bx = swatch(Rect::new(0, 0, w, n * ROW), [0, 0, 0, 0]);
            bx.name = Some("MFG_Box".into());
            let bx = view.add(scroll, bx);
            for (i, name) in tracks.into_iter().enumerate() {
                let mut c = ctrl(
                    "GuiCheckBoxCtrl",
                    "GuiCheckBoxProfile",
                    Rect::new(5, i as i32 * ROW, w - 5, ROW),
                );
                c.text = Some(name.clone());
                let node = view.add(bx, c);
                view.set_bool(node, music_enabled(&core.prefs, &name));
                rows.push((node, name));
            }
        }
        view.measure(&core.pack);
        Self { view, rows }
    }
    fn check_all(&mut self, on: bool) {
        for &(n, _) in &self.rows {
            self.view.set_bool(n, on);
        }
    }
    /// `MusicFilesGui::onSleep`: every track's choice is saved on close.
    fn save(&mut self, core: &mut Core) {
        for (n, name) in &self.rows {
            let on = self.view.bool_value(*n);
            core.prefs
                .set(&music_pref(name), if on { "1" } else { "-1" });
        }
        core.save_settings();
        core.pop(ScreenId::MusicFiles);
    }
}

impl Screen for MusicFiles {
    fn id(&self) -> ScreenId {
        ScreenId::MusicFiles
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            self.save(core);
            return true;
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if ev.kind == EventKind::Close {
            self.save(core);
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        let command = command_of(&self.view, ev.node).to_ascii_lowercase();
        match command.as_str() {
            "canvas.popdialog(musicfilesgui);" => self.save(core),
            "musicfilesgui.clicknone();" => self.check_all(false),
            // defaultMusicList.cs turns every stock track on.
            "musicfilesgui.clickdefaults();" => self.check_all(true),
            _ => {}
        }
    }
}
