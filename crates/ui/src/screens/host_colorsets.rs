//! Local host palette selection; browsing does not change the saved choice.
use super::*;
use crate::api::{HOST_COLORSET_PREF, UiAction};
use crate::view::EventKind;

pub(super) fn label(core: &Core, id: &str) -> String {
    core.host_colorsets
        .iter()
        .find(|choice| choice.id == id)
        .map(|choice| choice.name.clone())
        .unwrap_or_else(|| {
            let name = id
                .strip_prefix("user:")
                .or_else(|| id.strip_prefix("addon:"))
                .unwrap_or(id);
            format!("Unavailable: {}", name.replace('_', " "))
        })
}

fn named(mut control: Control, name: &str) -> Control {
    control.name = Some(name.into());
    control
}

fn scroll(name: &str, rect: Rect) -> Control {
    let mut control = named(ctrl("GuiScrollCtrl", "BlockScrollProfile", rect), name);
    control
        .fields
        .insert("hScrollBar".into(), "alwaysOff".into());
    control.fields.insert("vScrollBar".into(), "dynamic".into());
    control
}

pub struct HostColorsets {
    view: View,
    draft: String,
    choices: Vec<(NodeId, String)>,
    request: Option<RequestId>,
    catalog: Vec<crate::api::HostColorset>,
}

impl HostColorsets {
    pub fn new(core: &Core) -> Self {
        let mut screen = Self {
            view: View::new(&ctrl(
                "GuiControl",
                "GuiDefaultProfile",
                Rect::new(0, 0, 640, 480),
            )),
            draft: core.prefs.str_or(HOST_COLORSET_PREF, "").into(),
            choices: vec![],
            request: None,
            catalog: vec![],
        };
        screen.build(core);
        screen
    }

    fn available(&self, core: &Core) -> bool {
        core.host_colorsets
            .iter()
            .any(|choice| choice.id == self.draft)
    }

    fn build(&mut self, core: &Core) {
        let previous_scroll = self
            .view
            .id("HC_List")
            .map(|node| self.view.node(node).state.scroll_y)
            .unwrap_or(0);
        let mut ids: Vec<_> = core
            .host_colorsets
            .iter()
            .map(|choice| choice.id.clone())
            .collect();
        if !self.available(core) {
            ids.push(self.draft.clone());
        }
        // Size for the whole catalog so browsing a larger palette does not
        // move the action row. Large catalogs and previews scroll locally.
        let preview_rows = core
            .host_colorsets
            .iter()
            .map(|choice| {
                choice
                    .divisions
                    .iter()
                    .map(|division| division.colors.len())
                    .sum::<usize>()
                    .min(256)
                    .div_ceil(9)
            })
            .max()
            .unwrap_or(0);
        let content_height = (ids.len().saturating_mul(24).saturating_add(4))
            .max(preview_rows * 12 + 4)
            .clamp(96, 206) as i32;
        let height = content_height + 94;
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut window = ctrl(
            "GuiWindowCtrl",
            "BlockWindowProfile",
            Rect::new(130, (480 - height) / 2, 380, height),
        );
        window.name = Some("HC_Window".into());
        window.text = Some("Colorsets".into());
        window.h_sizing = HSizing::Center;
        window.v_sizing = VSizing::Center;
        let mut list = scroll("HC_List", Rect::new(12, 34, 210, content_height));
        for (i, id) in ids.iter().enumerate() {
            let mut radio = named(
                ctrl(
                    "GuiRadioCtrl",
                    "GuiRadioProfile",
                    Rect::new(4, i as i32 * 24, 186, 22),
                ),
                &format!("HC_Choice{i}"),
            );
            radio.group = Some(1);
            radio.text = Some(label(core, id));
            list.children.push(radio);
        }
        window.children.push(list);
        let mut preview = scroll("HC_Preview", Rect::new(236, 34, 132, content_height));
        if let Some(choice) = core
            .host_colorsets
            .iter()
            .find(|choice| choice.id == self.draft)
        {
            for (i, color) in choice
                .divisions
                .iter()
                .flat_map(|division| &division.colors)
                .enumerate()
                .take(256)
            {
                let mut swatch = ctrl(
                    "GuiSwatchCtrl",
                    "GuiDefaultProfile",
                    Rect::new(2 + (i % 9) as i32 * 12, (i / 9) as i32 * 12, 11, 11),
                );
                swatch.color =
                    Some(color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8));
                preview.children.push(swatch);
            }
        }
        window.children.push(preview);
        for (name, title, x) in [
            ("HC_Folder", "Folder...", 12),
            ("HC_Cancel", "Cancel", 114),
            ("HC_Use", "Use", 277),
        ] {
            window.children.push(named(
                button(
                    "BlockButtonProfile",
                    Rect::new(x, height - 50, 91, 38),
                    "base/client/ui/button2",
                    title,
                    name,
                ),
                name,
            ));
        }
        root.children.push(window);
        self.view = View::new(&root);
        self.view.measure(&core.pack);
        self.view.layout(core.logical.0, core.logical.1);
        self.choices = ids
            .into_iter()
            .enumerate()
            .map(|(i, id)| (self.view.id(&format!("HC_Choice{i}")).unwrap(), id))
            .collect();
        for (node, id) in &self.choices {
            self.view.set_bool(*node, id == &self.draft);
            self.view.set_active(
                *node,
                self.request.is_none() && core.host_colorsets.iter().any(|choice| &choice.id == id),
            );
        }
        self.view.set_active(
            self.view.id("HC_Use").unwrap(),
            self.request.is_none() && self.available(core),
        );
        self.view
            .set_active(self.view.id("HC_Folder").unwrap(), self.request.is_none());
        self.view
            .scroll_to(self.view.id("HC_List").unwrap(), previous_scroll);
        self.catalog.clone_from(&core.host_colorsets);
    }

    fn use_choice(&mut self, core: &mut Core) {
        if self.request.is_some() || !self.available(core) {
            return;
        }
        core.prefs.set(HOST_COLORSET_PREF, self.draft.clone());
        core.save_settings();
        core.pop(self.id());
    }
}

impl Screen for HostColorsets {
    fn id(&self) -> ScreenId {
        ScreenId::HostColorsets
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_wake(&mut self, core: &mut Core) {
        core.request(UiAction::RefreshHostColorsets);
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if let Some(id) = self.request.take() {
            core.pending.remove(&id);
        }
    }
    fn on_update(&mut self, core: &mut Core) {
        // Frame/network updates also reach this screen. Replacing the view
        // between mouse down and up discards its captured press.
        if self.catalog != core.host_colorsets {
            self.build(core);
        }
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        match key {
            Key::Escape => core.pop(self.id()),
            Key::Return | Key::NumpadEnter => self.use_choice(core),
            _ => return false,
        }
        true
    }
    fn on_event(&mut self, event: &ViewEvent, core: &mut Core) {
        if event.kind == EventKind::Close {
            core.pop(self.id());
            return;
        }
        if matches!(event.kind, EventKind::Changed | EventKind::Click)
            && let Some((_, id)) = self.choices.iter().find(|(node, _)| *node == event.node)
        {
            if self.request.is_none() && core.host_colorsets.iter().any(|choice| &choice.id == id) {
                self.draft = id.clone();
                self.build(core);
            }
            return;
        }
        if event.kind != EventKind::Click {
            return;
        }
        match self.view.node(event.node).ctrl.name.as_deref() {
            Some("HC_Cancel") => core.pop(self.id()),
            Some("HC_Use") => self.use_choice(core),
            Some("HC_Folder") if self.request.is_none() => {
                self.request =
                    Some(core.request_pending(UiAction::ColorsetsFolder, Pending::Other));
                self.build(core);
            }
            _ => {}
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
        if let Err(error) = result {
            core.message_ok("Could not open colorsets", error);
        }
        self.build(core);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{HostColorset, PaintDivision, Settings};
    use crate::ui::{Ui, UiConfig};

    fn fixture() -> Ui {
        let mut ui = Ui::new(
            crate::testing::screens_pack(),
            UiConfig {
                size: (1024, 768),
                scale: Some(1.0),
                platform: crate::binds::Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        ui.core
            .host_colorsets
            .extend(["first", "second"].map(|id| HostColorset {
                id: id.into(),
                name: "Same label".into(),
                divisions: vec![PaintDivision {
                    name: "Colors".into(),
                    colors: vec![[1.0, 0.0, 0.0, 1.0]],
                }],
            }));
        ui
    }

    fn choose(screen: &mut HostColorsets, core: &mut Core, id: &str) {
        let node = screen
            .choices
            .iter()
            .find(|(_, choice)| choice == id)
            .unwrap()
            .0;
        screen.on_event(
            &ViewEvent {
                node,
                kind: EventKind::Changed,
            },
            core,
        );
    }

    fn click(screen: &mut HostColorsets, core: &mut Core, name: &str) {
        screen.on_event(
            &ViewEvent {
                node: screen.view.id(name).unwrap(),
                kind: EventKind::Click,
            },
            core,
        );
    }

    #[test]
    fn browsing_cancel_and_escape_do_not_save_the_draft() {
        let mut ui = fixture();
        ui.core.prefs.set(HOST_COLORSET_PREF, "first");
        let mut screen = HostColorsets::new(&ui.core);
        screen.on_wake(&mut ui.core);
        assert!(
            ui.drain_actions()
                .iter()
                .any(|(_, a)| *a == UiAction::RefreshHostColorsets)
        );
        choose(&mut screen, &mut ui.core, "second");
        assert_eq!(screen.draft, "second");
        assert_eq!(ui.core.prefs.get(HOST_COLORSET_PREF), Some("first"));
        click(&mut screen, &mut ui.core, "HC_Cancel");
        assert!(ui.drain_actions().is_empty());
        let mut screen = HostColorsets::new(&ui.core);
        choose(&mut screen, &mut ui.core, "");
        screen.on_key(Key::Escape, Modifiers::default(), &mut ui.core);
        assert_eq!(ui.core.prefs.get(HOST_COLORSET_PREF), Some("first"));
        assert!(ui.drain_actions().is_empty());
    }

    #[test]
    fn draft_identity_survives_reorder_and_missing_choice_blocks_use() {
        let mut ui = fixture();
        let mut screen = HostColorsets::new(&ui.core);
        choose(&mut screen, &mut ui.core, "second");
        ui.core.host_colorsets.swap(1, 2); // Equal labels do not define identity.
        screen.on_update(&mut ui.core);
        assert_eq!(screen.draft, "second");
        let selected = screen
            .choices
            .iter()
            .find(|(_, id)| id == "second")
            .unwrap()
            .0;
        assert!(screen.view.bool_value(selected));
        assert_eq!(ui.core.prefs.str_or(HOST_COLORSET_PREF, ""), "");
        let second = ui.core.host_colorsets.remove(1);
        screen.on_update(&mut ui.core);
        assert!(
            !screen
                .view
                .node(screen.view.id("HC_Use").unwrap())
                .state
                .active
        );
        let selected = screen
            .choices
            .iter()
            .find(|(_, id)| id == "second")
            .unwrap()
            .0;
        assert_eq!(
            screen.view.node(selected).ctrl.text.as_deref(),
            Some("Unavailable: second")
        );
        click(&mut screen, &mut ui.core, "HC_Use");
        assert!(ui.drain_actions().is_empty());
        assert_eq!(ui.core.prefs.str_or(HOST_COLORSET_PREF, ""), "");
        ui.core.host_colorsets.push(second);
        screen.on_update(&mut ui.core);
        click(&mut screen, &mut ui.core, "HC_Use");
        assert_eq!(ui.core.prefs.get(HOST_COLORSET_PREF), Some("second"));
        assert!(
            ui.drain_actions()
                .iter()
                .any(|(_, a)| matches!(a, UiAction::SaveSettings(_)))
        );
    }

    #[test]
    fn folder_refresh_retains_draft_and_long_catalog_scroll() {
        let mut ui = fixture();
        ui.core.host_colorsets.extend((0..40).map(|i| HostColorset {
            id: format!("user:{i}.txt"),
            name: format!("Custom {i}"),
            divisions: vec![],
        }));
        let mut screen = HostColorsets::new(&ui.core);
        choose(&mut screen, &mut ui.core, "user:39.txt");
        let list = screen.view.id("HC_List").unwrap();
        screen.view.scroll_to(list, 800);
        assert!(screen.view.node(list).state.scroll_y > 0);
        click(&mut screen, &mut ui.core, "HC_Folder");
        assert_eq!(screen.draft, "user:39.txt");
        assert!(
            screen
                .view
                .node(screen.view.id("HC_List").unwrap())
                .state
                .scroll_y
                > 0
        );
        let request = ui
            .drain_actions()
            .into_iter()
            .find(|(_, a)| *a == UiAction::ColorsetsFolder)
            .unwrap()
            .0;
        assert!(
            !screen
                .view
                .node(screen.view.id("HC_Use").unwrap())
                .state
                .active
        );
        screen.on_result(request, Some(&Pending::Other), &Ok(()), &mut ui.core);
        assert!(
            screen
                .view
                .node(screen.view.id("HC_Use").unwrap())
                .state
                .active
        );
        assert_eq!(screen.draft, "user:39.txt");
        assert_eq!(ui.core.prefs.str_or(HOST_COLORSET_PREF, ""), "");
        assert_eq!(
            label(&ui.core, "addon:ColorSet_Trueno"),
            "Unavailable: ColorSet Trueno"
        );
    }

    #[test]
    fn compact_catalog_fits_sixty_four_swatches_and_large_catalog_stays_bounded() {
        let mut ui = fixture();
        ui.core.host_colorsets[1].divisions[0].colors = vec![[1.0; 4]; 64];
        ui.core.prefs.set(HOST_COLORSET_PREF, "first");
        let screen = HostColorsets::new(&ui.core);
        let window = screen.view.node(screen.view.id("HC_Window").unwrap()).rect;
        assert_eq!(window.h, 194);
        let preview = screen.view.node(screen.view.id("HC_Preview").unwrap());
        assert_eq!(preview.children.len(), 64);
        for &node in &preview.children {
            let swatch = screen.view.node(node).rect;
            assert_eq!(preview.rect.intersect(&swatch), Some(swatch));
        }
        for name in ["HC_Folder", "HC_Cancel", "HC_Use"] {
            let rect = screen.view.node(screen.view.id(name).unwrap()).rect;
            assert_eq!((rect.w, rect.h), (91, 38));
            assert_eq!(window.intersect(&rect), Some(rect));
        }
        ui.core
            .host_colorsets
            .extend((0..120).map(|i| HostColorset {
                id: i.to_string(),
                name: i.to_string(),
                divisions: vec![],
            }));
        let screen = HostColorsets::new(&ui.core);
        assert_eq!(
            screen
                .view
                .node(screen.view.id("HC_Window").unwrap())
                .rect
                .h,
            300
        );
    }

    #[test]
    fn pointer_clicks_survive_frame_and_unchanged_catalog_updates() {
        use crate::api::UiUpdate;
        use crate::input::{InputEvent, MouseButton};

        fn click_across_updates(ui: &mut Ui, control: &str) {
            let (x, y) = ui.control_center(ScreenId::HostColorsets, control).unwrap();
            ui.handle_input(InputEvent::MouseDown {
                button: MouseButton::Left,
                x,
                y,
            });
            ui.apply(UiUpdate::PerfFrame(Default::default()));
            ui.apply(UiUpdate::HostColorsets(ui.core.host_colorsets.clone()));
            ui.update(16);
            ui.handle_input(InputEvent::MouseUp {
                button: MouseButton::Left,
                x,
                y,
            });
        }

        for size in [(400, 300), (1024, 768)] {
            let mut ui = fixture();
            ui.resize(size, Some(1.0));
            ui.core.prefs.set(HOST_COLORSET_PREF, "first");
            ui.core.push(ScreenId::HostColorsets);
            ui.update(0);
            ui.drain_actions();

            click_across_updates(&mut ui, "HC_Choice2");
            let view = ui.screen(ScreenId::HostColorsets).unwrap().view();
            assert!(view.bool_value(view.id("HC_Choice2").unwrap()));
            assert!(!view.bool_value(view.id("HC_Choice1").unwrap()));
            assert_eq!(ui.core.prefs.get(HOST_COLORSET_PREF), Some("first"));
            click_across_updates(&mut ui, "HC_Cancel");
            assert_ne!(ui.top_id(), ScreenId::HostColorsets);
            assert_eq!(ui.core.prefs.get(HOST_COLORSET_PREF), Some("first"));
            assert!(ui.drain_actions().is_empty());

            ui.core.push(ScreenId::HostColorsets);
            ui.update(0);
            ui.drain_actions();
            click_across_updates(&mut ui, "HC_Choice2");
            click_across_updates(&mut ui, "HC_Use");
            assert_ne!(ui.top_id(), ScreenId::HostColorsets);
            assert_eq!(ui.core.prefs.get(HOST_COLORSET_PREF), Some("second"));
            assert!(
                ui.drain_actions()
                    .iter()
                    .any(|(_, action)| { matches!(action, UiAction::SaveSettings(_)) })
            );

            ui.core.push(ScreenId::HostColorsets);
            ui.update(0);
            ui.drain_actions();
            click_across_updates(&mut ui, "HC_Folder");
            assert!(
                ui.drain_actions()
                    .iter()
                    .any(|(_, action)| { *action == UiAction::ColorsetsFolder })
            );
        }
    }
}
