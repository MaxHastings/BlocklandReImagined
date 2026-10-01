//! Authored wrench windows and host-capability-driven event rows. Commands are
//! identified as data; no recovered script is executed.
use super::*;
use crate::api::{Choice, EventCatalog, ParamValue, UiAction};
use crate::models::events::{self, EventsModel, NAMED_BRICK, RowState};
use crate::models::wrench::{WrenchField, clean_name, respawn_ms};
use crate::schema::ParamSpec;
use crate::view::EventKind;
use std::collections::BTreeMap;

fn named(mut control: Control, name: impl Into<String>) -> Control {
    control.name = Some(name.into());
    control
}

fn choices(core: &Core, class: &str, sorted: bool) -> Vec<Choice> {
    let mut list = core
        .datablocks
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(class))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    if sorted {
        list.sort_by_key(|c| c.name.to_ascii_lowercase());
    }
    list
}

/// Stable host strings stay outside widget numeric menu indices.
fn resource_menu(
    view: &mut View,
    node: NodeId,
    list: &[Choice],
    current: Option<&str>,
) -> Vec<Option<String>> {
    let mut ids = vec![None];
    let mut items = vec![(" NONE".into(), 0)];
    for c in list {
        items.push((c.name.clone(), ids.len() as i64));
        ids.push(Some(c.id.clone()));
    }
    let selected = ids
        .iter()
        .position(|id| id.as_deref() == current)
        .unwrap_or_else(|| {
            items.push((
                format!("Unavailable: {}", current.unwrap_or_default()),
                ids.len() as i64,
            ));
            ids.push(current.map(str::to_string));
            ids.len() - 1
        });
    view.state(node).items = items;
    view.select(node, Some(selected as i64));
    ids
}

fn suffix(field: WrenchField) -> &'static str {
    use WrenchField::*;
    match field {
        Name => "Name",
        Light => "Lights",
        Emitter => "Emitters",
        EmitterDir => "EmitterDir",
        Item => "Items",
        ItemPos => "ItemPos",
        ItemDir => "ItemDir",
        ItemRespawn => "ItemRespawnTime",
        RayCasting => "RayCasting",
        Colliding => "Collision",
        Rendering => "Rendering",
        Sound => "Sounds",
        Vehicle => "Vehicles",
        RecolorVehicle => "ReColorVehicle",
    }
}

pub struct Wrench {
    view: View,
    variant: WrenchVariant,
    brick: Option<u64>,
    prefix: &'static str,
    menus: BTreeMap<NodeId, Vec<Option<String>>>,
    request: Option<(RequestId, Operation)>,
    datablocks: crate::api::DatablockMenus,
}

#[derive(Clone, Copy, PartialEq)]
enum Operation {
    Send,
    Events,
    Respawn,
}

impl Wrench {
    pub fn new(core: &Core, variant: WrenchVariant) -> Self {
        let (layout, prefix) = match variant {
            WrenchVariant::Normal => ("wrenchDlg", "Wrench"),
            WrenchVariant::Sound => ("wrenchSoundDlg", "WrenchSound"),
            WrenchVariant::VehicleSpawn => ("wrenchVehicleSpawnDlg", "WrenchVehicleSpawn"),
        };
        let mut s = Self {
            view: layout_view(core, layout),
            variant,
            brick: core
                .wrench
                .open
                .as_ref()
                .filter(|o| o.variant == variant)
                .map(|o| o.brick),
            prefix,
            menus: BTreeMap::new(),
            request: None,
            datablocks: core.datablocks.clone(),
        };
        s.fill(core);
        s.view.layout(core.logical.0, core.logical.1);
        s
    }

    fn field_node(&self, field: WrenchField) -> Option<NodeId> {
        self.view.id(&format!("{}_{}", self.prefix, suffix(field)))
    }

    fn current(&self, core: &Core) -> bool {
        core.wrench
            .open
            .as_ref()
            .is_some_and(|o| Some(o.brick) == self.brick && o.variant == self.variant)
    }

    fn fill(&mut self, core: &Core) {
        let data = core.wrench.values(self.variant);
        self.menus.clear();
        for &field in WrenchField::for_variant(self.variant) {
            if let Some(n) = self
                .view
                .id(&format!("{}Lock_{}", self.prefix, suffix(field)))
            {
                self.view
                    .set_bool(n, core.wrench.ticked(self.variant, field));
            }
            if matches!(
                field,
                WrenchField::EmitterDir | WrenchField::ItemDir | WrenchField::ItemPos
            ) {
                let selected = match field {
                    WrenchField::EmitterDir => data.emitter_dir,
                    WrenchField::ItemDir => data.item_dir,
                    _ => data.item_pos,
                };
                for i in 0..6 {
                    if let Some(n) = self
                        .view
                        .id(&format!("{}_{}{i}", self.prefix, suffix(field)))
                    {
                        self.view.set_bool(n, i == selected);
                    }
                }
                continue;
            }
            let Some(n) = self.field_node(field) else {
                continue;
            };
            use WrenchField::*;
            match field {
                Name => self.view.set_text(n, &data.name),
                // Vanilla transmits floor(itemRespawnTime / 1000) to this
                // seconds field; the typed host boundary remains milliseconds.
                ItemRespawn => self
                    .view
                    .set_text(n, (data.item_respawn_ms / 1000).to_string()),
                RayCasting => self.view.set_bool(n, data.raycasting),
                Colliding => self.view.set_bool(n, data.colliding),
                Rendering => self.view.set_bool(n, data.rendering),
                RecolorVehicle => self.view.set_bool(n, data.recolor_vehicle),
                Light | Emitter | Item | Sound | Vehicle => {
                    let (class, value) = match field {
                        Light => ("FxLightData", data.light.as_deref()),
                        Emitter => ("ParticleEmitterData", data.emitter.as_deref()),
                        Item => ("ItemData", data.item.as_deref()),
                        Sound => (
                            if core.datablocks.contains_key("Music") {
                                "Music"
                            } else {
                                "AudioProfile"
                            },
                            data.sound.as_deref(),
                        ),
                        _ => ("Vehicle", data.vehicle.as_deref()),
                    };
                    let list = choices(core, class, field != Light);
                    let ids = resource_menu(&mut self.view, n, &list, value);
                    self.menus.insert(n, ids);
                }
                _ => {}
            }
        }
        if let Some(n) = self.view.id(&format!("{}_Window", self.prefix)) {
            let open = core.wrench.open.as_ref();
            let title = match open.and_then(|o| o.fill) {
                Some(1) => "Fill Wrench - 1 Brick".to_string(),
                Some(count) => format!("Fill Wrench - {count} Bricks"),
                None => format!("Wrench - {}", open.map(|o| o.owner.as_str()).unwrap_or_default()),
            };
            self.view.set_text(n, title);
        }
        self.refresh(core);
    }

    fn refresh(&mut self, core: &Core) {
        let ready = self.current(core) && self.request.is_none();
        let ids: Vec<_> = self.view.walk().collect();
        for n in ids {
            self.view.set_active(n, ready);
            let name = self.view.node(n).ctrl.name.clone().unwrap_or_default();
            if name.ends_with("Blocker") || name.ends_with("LoadingWindow") {
                self.view.set_visible(n, false);
            }
            let command = command_of(&self.view, n).to_ascii_lowercase();
            if command.contains("pushdialog(wrencheventsdlg)") {
                self.view.set_active(
                    n,
                    ready && core.wrench.open.as_ref().is_some_and(|o| o.events_allowed),
                );
            }
        }
        // Pending actions must finish/reject before an edit can be resubmitted.
    }

    fn store(&mut self, core: &mut Core) {
        let mut data = core.wrench.values(self.variant);
        for &field in WrenchField::for_variant(self.variant) {
            if let Some(n) = self
                .view
                .id(&format!("{}Lock_{}", self.prefix, suffix(field)))
            {
                core.wrench
                    .set_ticked(self.variant, field, self.view.bool_value(n));
            }
            use WrenchField::*;
            if matches!(field, EmitterDir | ItemDir | ItemPos) {
                for i in 0..6 {
                    if self
                        .view
                        .id(&format!("{}_{}{i}", self.prefix, suffix(field)))
                        .is_some_and(|n| self.view.bool_value(n))
                    {
                        match field {
                            EmitterDir => data.emitter_dir = i,
                            ItemDir => data.item_dir = i,
                            _ => data.item_pos = i,
                        }
                    }
                }
                continue;
            }
            let Some(n) = self.field_node(field) else {
                continue;
            };
            let resource = || {
                self.view
                    .selected(n)
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| self.menus.get(&n)?.get(i))
                    .cloned()
                    .flatten()
            };
            match field {
                Name => data.name = clean_name(&self.view.edit_text(n)),
                ItemRespawn => data.item_respawn_ms = respawn_ms(&self.view.edit_text(n)),
                Light => data.light = resource(),
                Emitter => data.emitter = resource(),
                Item => data.item = resource(),
                Sound => data.sound = resource(),
                Vehicle => data.vehicle = resource(),
                RayCasting => data.raycasting = self.view.bool_value(n),
                Colliding => data.colliding = self.view.bool_value(n),
                Rendering => data.rendering = self.view.bool_value(n),
                RecolorVehicle => data.recolor_vehicle = self.view.bool_value(n),
                _ => {}
            }
        }
        core.wrench.set_values(self.variant, data);
    }

    fn cancel(&mut self, core: &mut Core) {
        if !self.current(core) {
            return;
        }
        // Backing out always works: an edit still on its way may land, but
        // the dialog stops waiting for it.
        if let Some((id, _)) = self.request.take() {
            core.abandon(id);
        }
        self.store(core);
        if let Some(brick) = self.brick {
            core.request(UiAction::CancelWrench { brick });
        }
        core.wrench.close();
        core.pop(self.id());
    }
}

impl Screen for Wrench {
    fn id(&self) -> ScreenId {
        ScreenId::Wrench(self.variant)
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
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if !self.current(core) {
            return;
        }
        if ev.kind == EventKind::Close {
            self.cancel(core);
            return;
        }
        if self.request.is_some() {
            return;
        }
        if matches!(ev.kind, EventKind::Changed | EventKind::Submit) {
            self.store(core);
        }
        if ev.kind != EventKind::Click {
            return;
        }
        let command = command_of(&self.view, ev.node).to_ascii_lowercase();
        if command.contains("popdialog(") {
            self.cancel(core);
            return;
        }
        let Some(brick) = self.brick else { return };
        self.store(core);
        let op = if command.ends_with(".send();") {
            Some(Operation::Send)
        } else if command.ends_with(".respawn();") {
            Some(Operation::Respawn)
        } else if command.contains("pushdialog(wrencheventsdlg)")
            && core.wrench.open.as_ref().is_some_and(|o| o.events_allowed)
        {
            Some(Operation::Events)
        } else {
            None
        };
        if let Some(op) = op {
            let data = core.wrench.values(self.variant);
            let action = match op {
                Operation::Send if core.wrench.filling() => UiAction::SendFillWrench {
                    data,
                    fields: core.wrench.fill_ticks.iter().copied().collect(),
                },
                Operation::Send => UiAction::SendWrench {
                    brick,
                    variant: self.variant,
                    data,
                },
                Operation::Events => {
                    core.wrench.events = None;
                    UiAction::RequestEvents { brick }
                }
                Operation::Respawn => UiAction::RespawnVehicle {
                    brick,
                    vehicle: data.vehicle,
                },
            };
            let id = core.request_pending(action, Pending::Wrench);
            self.request = Some((id, op));
            self.refresh(core);
        }
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            self.cancel(core);
            true
        } else {
            false
        }
    }
    fn on_update(&mut self, core: &mut Core) {
        if let Some((id, Operation::Events)) = self.request
            && core
                .wrench
                .events
                .as_ref()
                .is_some_and(|e| Some(e.brick) == self.brick)
        {
            core.pending.remove(&id);
            self.request = None;
        }
        // Rebuild menus after host catalog changes without replacing the draft.
        if self.current(core) && self.request.is_none() && self.datablocks != core.datablocks {
            self.datablocks = core.datablocks.clone();
            self.fill(core);
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
        let Some((waiting, op)) = self.request else {
            return false;
        };
        if waiting != id {
            return false;
        }
        if !self.current(core) {
            self.request = None;
            return true;
        }
        if result.is_ok() && op == Operation::Events {
            return true;
        }
        self.request = None;
        match result {
            Ok(()) => {
                core.wrench.close();
                core.pop(self.id());
            }
            Err(reason) => core.message_ok("Wrench Rejected", reason),
        }
        self.refresh(core);
        true
    }
}

#[derive(Clone)]
enum Binding {
    Enabled(usize),
    Delay(usize),
    Input(usize),
    Target(usize),
    Named(usize),
    Output(usize),
    Parameter(usize, usize, ParamSpec),
}

pub struct WrenchEvents {
    view: View,
    model: Option<EventsModel>,
    catalog: EventCatalog,
    bindings: BTreeMap<NodeId, Binding>,
    resources: BTreeMap<NodeId, Vec<Option<String>>>,
    request: Option<RequestId>,
    error: Option<String>,
    datablocks: crate::api::DatablockMenus,
    paint: Vec<crate::api::PaintDivision>,
}

fn editable_catalog(source: &EventCatalog) -> EventCatalog {
    let mut catalog = source.clone();
    for output in &mut catalog.outputs {
        // Unknown specifications cannot be made safely editable by a text box.
        output.supported &= output.params.len() <= 4
            && output.params.iter().all(|p| match p {
                ParamSpec::Unknown { .. } => false,
                ParamSpec::Int { min, max, .. } => min <= max,
                ParamSpec::Float {
                    min,
                    max,
                    step,
                    default,
                } => min <= max && [min, max, step, default].iter().all(|v| v.is_finite()),
                ParamSpec::Vector { max } => max.is_finite(),
                _ => true,
            });
    }
    catalog
}

fn parameter_type_matches(spec: &ParamSpec, value: &ParamValue) -> bool {
    match (spec, value) {
        (ParamSpec::Int { .. }, ParamValue::Int(_))
        | (ParamSpec::IntList { .. } | ParamSpec::String { .. }, ParamValue::Text(_))
        | (ParamSpec::Bool, ParamValue::Bool(_))
        | (ParamSpec::Datablock { .. }, ParamValue::Datablock(_))
        | (ParamSpec::List { .. }, ParamValue::List(_))
        | (ParamSpec::PaintColor { .. }, ParamValue::PaintColor(_)) => true,
        (ParamSpec::Float { .. }, ParamValue::Float(v)) => v.is_finite(),
        (ParamSpec::Vector { .. }, ParamValue::Vector(v)) => v.iter().all(|x| x.is_finite()),
        _ => false,
    }
}

impl WrenchEvents {
    pub fn new(core: &Core) -> Self {
        let catalog = editable_catalog(&core.events);
        let mut s = Self {
            view: layout_view(core, "wrenchEventsDlg"),
            model: core.wrench.events.clone(),
            catalog,
            bindings: BTreeMap::new(),
            resources: BTreeMap::new(),
            request: None,
            error: None,
            datablocks: core.datablocks.clone(),
            paint: core.hud.paint.clone(),
        };
        s.preserve_unsupported();
        s.build(core);
        s
    }

    fn preserve_unsupported(&mut self) {
        let Some(model) = &mut self.model else { return };
        for row in &mut model.rows {
            let RowState::Editable(e) = row else { continue };
            let (Some(input), Some(target), Some(output)) = (&e.input, &e.target, &e.output) else {
                continue;
            };
            let class = EventsModel::target_class(&self.catalog, input, target);
            let valid = self
                .catalog
                .inputs
                .iter()
                .any(|i| i.supported && &i.name == input)
                && self.catalog.outputs.iter().any(|o| {
                    o.supported
                        && &o.name == output
                        && class
                            .as_ref()
                            .is_some_and(|c| o.class.eq_ignore_ascii_case(c))
                        && o.params.len() == e.params.len()
                        && o.params
                            .iter()
                            .zip(&e.params)
                            .all(|(spec, value)| parameter_type_matches(spec, value))
                });
            if !valid {
                let line = crate::api::EventLine {
                    enabled: e.enabled,
                    delay_ms: events::clamp_delay(&e.delay_text),
                    input: input.clone(),
                    target: target.clone(),
                    named_target: e.named.clone(),
                    output: output.clone(),
                    params: e.params.clone(),
                };
                *row = RowState::Preserved {
                    enabled: line.enabled,
                    text: events::describe(&line),
                    token: serde_json::to_string(&line).unwrap_or_default(),
                };
            }
        }
    }

    fn current(&self, core: &Core) -> bool {
        self.model.as_ref().is_some_and(|m| {
            core.wrench
                .events
                .as_ref()
                .is_some_and(|e| e.brick == m.brick)
        })
    }

    fn save_draft(&self, core: &mut Core) {
        if self.current(core) {
            core.wrench.events = self.model.clone();
            if core.wrench.events_copy.is_some() {
                core.wrench.events_copy = self.model.clone();
            }
        }
    }

    fn widget(
        &mut self,
        parent: NodeId,
        class: &str,
        rect: Rect,
        row: usize,
        field: &str,
        binding: Binding,
    ) -> NodeId {
        let profile = match class {
            "GuiCheckBoxCtrl" => "GuiCheckBoxProfile",
            "GuiPopUpMenuCtrl" => "GuiPopUpMenuProfile",
            _ => "GuiTextEditProfile",
        };
        let n = self.view.add(
            parent,
            named(
                ctrl(class, profile, rect),
                format!("WrenchEvent_{row}_{field}"),
            ),
        );
        self.bindings.insert(n, binding);
        n
    }

    fn menu(&mut self, node: NodeId, options: Vec<String>, selected: Option<&str>, blank: bool) {
        let mut items = Vec::new();
        if blank {
            items.push(("-".into(), -1));
        }
        items.extend(options.into_iter().enumerate().map(|(i, s)| (s, i as i64)));
        let value = items
            .iter()
            .find(|(s, _)| Some(s.as_str()) == selected)
            .map(|(_, i)| *i)
            .or(if blank { Some(-1) } else { None });
        self.view.state(node).items = items;
        self.view.select(node, value);
    }

    fn build(&mut self, core: &Core) {
        let scroll_offset = self
            .view
            .id("wrenchEvents_Scroll")
            .map(|n| self.view.node(n).state.scroll_y)
            .unwrap_or_default();
        self.view = layout_view(core, "wrenchEventsDlg");
        self.bindings.clear();
        self.resources.clear();
        let win = self
            .view
            .id("WrenchEvents_Window")
            .unwrap_or(self.view.root);
        let width = core.logical.0.clamp(400, 800);
        let height = core.logical.1.clamp(300, 600);
        // v20 shrinks this 800x600 window to the canvas, keeping all controls usable.
        self.view.nodes[win].ctrl.position =
            [(core.logical.0 - width) / 2, (core.logical.1 - height) / 2];
        self.view.nodes[win].ctrl.extent = [width, height];
        let scroll = self.view.id("wrenchEvents_Scroll").unwrap_or_else(|| {
            self.view.add(
                win,
                named(
                    ctrl(
                        "GuiScrollCtrl",
                        "ColorScrollProfile",
                        Rect::new(11, 51, width - 20, height - 115),
                    ),
                    "wrenchEvents_Scroll",
                ),
            )
        });
        self.view.nodes[scroll].ctrl.position = [11, 51];
        self.view.nodes[scroll].ctrl.extent = [width - 20, height - 115];
        self.view.clear_children(scroll);
        let body = self.view.add(
            scroll,
            named(
                ctrl(
                    "GuiSwatchCtrl",
                    "GuiDefaultProfile",
                    Rect::new(0, 0, width - 35, 1),
                ),
                "WrenchEvents_Box",
            ),
        );
        let nodes: Vec<_> = self.view.walk().collect();
        for n in nodes {
            let command = command_of(&self.view, n).to_ascii_lowercase();
            if command == "wrencheventsdlg.send();" {
                self.view.nodes[n].ctrl.position = [width - 100, height - 51];
            }
            if command == "canvas.popdialog(wrencheventsdlg);" {
                self.view.nodes[n].ctrl.position = [11, height - 51];
            }
            if command == "wrencheventsdlg.clear();" {
                self.view.nodes[n].ctrl.position = [width - 117, 31];
            }
            if self.view.node(n).ctrl.text.as_deref() == Some("Output Parameters") {
                self.view.set_visible(n, false);
            }
            if self.view.node(n).ctrl.name.as_deref() == Some("WrenchLock_Events") {
                self.view.nodes[n].ctrl.position = [width - 50, 29];
                self.view.set_bool(n, core.wrench.events_copy.is_some());
            }
            if self.view.node(n).ctrl.class == "GuiSwatchCtrl" && n != body {
                self.view.set_visible(n, false);
            }
        }
        let rows = self
            .model
            .as_ref()
            .map(|m| m.rows.clone())
            .unwrap_or_default();
        let mut y = 0;
        for (row, state) in rows.into_iter().enumerate() {
            match state {
                RowState::Preserved {
                    text: preserved, ..
                } => {
                    let mut label = named(
                        text(
                            "GuiDefaultProfile",
                            Rect::new(4, y, width - 50, 38),
                            &format!("Preserved (read only): {preserved}"),
                        ),
                        format!("WrenchEvent_{row}_preserved"),
                    );
                    label.class = "GuiMLTextCtrl".into();
                    self.view.add(body, label);
                    y += 42;
                }
                RowState::Editable(e) => {
                    let enabled = self.widget(
                        body,
                        "GuiCheckBoxCtrl",
                        Rect::new(1, y, 25, 22),
                        row,
                        "enabled",
                        Binding::Enabled(row),
                    );
                    self.view.set_bool(enabled, e.enabled);
                    self.view.set_text(enabled, row.to_string());
                    let delay = self.widget(
                        body,
                        "GuiTextEditCtrl",
                        Rect::new(30, y, 42, 22),
                        row,
                        "delay",
                        Binding::Delay(row),
                    );
                    self.view.set_text(delay, e.delay_text);
                    let input = self.widget(
                        body,
                        "GuiPopUpMenuCtrl",
                        Rect::new(78, y, 100, 22),
                        row,
                        "input",
                        Binding::Input(row),
                    );
                    self.menu(
                        input,
                        EventsModel::input_choices(&self.catalog),
                        e.input.as_deref(),
                        true,
                    );
                    if let Some(input_name) = &e.input {
                        let target = self.widget(
                            body,
                            "GuiPopUpMenuCtrl",
                            Rect::new(182, y, 100, 22),
                            row,
                            "target",
                            Binding::Target(row),
                        );
                        let targets = self
                            .model
                            .as_ref()
                            .unwrap()
                            .target_choices(&self.catalog, input_name);
                        self.menu(target, targets, e.target.as_deref(), false);
                    }
                    if let (Some(input), Some(target)) = (&e.input, &e.target) {
                        let class = EventsModel::target_class(&self.catalog, input, target)
                            .unwrap_or_default();
                        let output = self.widget(
                            body,
                            "GuiPopUpMenuCtrl",
                            Rect::new(286, y, 125, 22),
                            row,
                            "output",
                            Binding::Output(row),
                        );
                        self.menu(
                            output,
                            EventsModel::output_choices(&self.catalog, &class),
                            e.output.as_deref(),
                            false,
                        );
                    }
                    let mut parameter_y = y + 25;
                    if e.target.as_deref() == Some(NAMED_BRICK) {
                        self.view.add(
                            body,
                            text(
                                "GuiDefaultProfile",
                                Rect::new(4, parameter_y, 70, 22),
                                "Named:",
                            ),
                        );
                        let node = self.widget(
                            body,
                            "GuiPopUpMenuCtrl",
                            Rect::new(78, parameter_y, 333, 22),
                            row,
                            "named",
                            Binding::Named(row),
                        );
                        self.menu(
                            node,
                            self.model.as_ref().unwrap().named_choices(),
                            e.named.as_deref(),
                            false,
                        );
                        parameter_y += 25;
                    }
                    let specs = self.model.as_ref().unwrap().param_specs(row, &self.catalog);
                    if !specs.is_empty() {
                        self.view.add(
                            body,
                            text(
                                "GuiDefaultProfile",
                                Rect::new(4, parameter_y, 70, 22),
                                "Params:",
                            ),
                        );
                    }
                    let parameter_width = ((width - 100) / specs.len().max(1) as i32).min(165);
                    for (i, spec) in specs.iter().enumerate() {
                        let rect = Rect::new(
                            78 + i as i32 * parameter_width,
                            parameter_y,
                            parameter_width - 5,
                            22,
                        );
                        let value = e
                            .params
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| events::default_param(spec));
                        self.parameter(body, (row, i), spec, value, rect, core);
                    }
                    y = if specs.is_empty() {
                        parameter_y
                    } else {
                        parameter_y + 26
                    };
                    y += 5;
                }
            }
        }
        self.view.nodes[body].ctrl.extent[1] = y.max(1);
        self.view.state(scroll).scroll_y = scroll_offset;
        let status = self
            .error
            .as_deref()
            .unwrap_or("Host-supported events only; preserved rows remain unchanged.");
        self.view.add(
            win,
            named(
                text(
                    "GuiDefaultProfile",
                    Rect::new(108, height - 45, width - 220, 34),
                    status,
                ),
                "WrenchEvents_Status",
            ),
        );
        self.refresh(core);
        self.view.layout(core.logical.0, core.logical.1);
    }

    fn parameter(
        &mut self,
        parent: NodeId,
        address: (usize, usize),
        spec: &ParamSpec,
        value: ParamValue,
        rect: Rect,
        core: &Core,
    ) {
        let (row, index) = address;
        let class = match spec {
            ParamSpec::Bool => "GuiCheckBoxCtrl",
            ParamSpec::Datablock { .. } | ParamSpec::List { .. } | ParamSpec::PaintColor { .. } => {
                "GuiPopUpMenuCtrl"
            }
            _ => "GuiTextEditCtrl",
        };
        let node = self.widget(
            parent,
            class,
            rect,
            row,
            &format!("param{index}"),
            Binding::Parameter(row, index, spec.clone()),
        );
        match (spec, value) {
            (ParamSpec::Datablock { class }, ParamValue::Datablock(current)) => {
                let ids = resource_menu(
                    &mut self.view,
                    node,
                    &choices(core, class, true),
                    current.as_deref(),
                );
                self.resources.insert(node, ids);
            }
            (ParamSpec::List { items }, ParamValue::List(current)) => {
                self.view.state(node).items = items.clone();
                self.view.select(node, Some(current));
            }
            (ParamSpec::PaintColor { .. }, ParamValue::PaintColor(current)) => {
                self.view.state(node).items = core
                    .hud
                    .paint
                    .iter()
                    .flat_map(|d| d.colors.iter())
                    .enumerate()
                    .map(|(i, c)| {
                        let c = rgba(*c);
                        (
                            format!("{i}: #{:02X}{:02X}{:02X}", c[0], c[1], c[2]),
                            i as i64,
                        )
                    })
                    .collect();
                self.view.select(node, Some(i64::from(current)));
                self.view.nodes[node].ctrl.extent[0] = (rect.w - 25).max(30);
                let color = core.hud.color(current).map(rgba).unwrap_or([0, 0, 0, 0]);
                self.view.add(
                    parent,
                    named(
                        swatch(Rect::new(rect.x + rect.w - 21, rect.y + 1, 20, 20), color),
                        format!("WrenchEvent_{row}_param{index}_swatch"),
                    ),
                );
            }
            (_, ParamValue::Bool(v)) => self.view.set_bool(node, v),
            (_, ParamValue::Int(v) | ParamValue::List(v)) => {
                self.view.set_text(node, v.to_string())
            }
            (_, ParamValue::Float(v)) => self.view.set_text(node, v.to_string()),
            (_, ParamValue::Text(v)) => self.view.set_text(node, v),
            (_, ParamValue::Vector(v)) => self
                .view
                .set_text(node, format!("{} {} {}", v[0], v[1], v[2])),
            _ => self.view.set_text(node, ""),
        }
    }

    fn refresh(&mut self, core: &Core) {
        let ready = self.current(core) && self.request.is_none();
        let nodes: Vec<_> = self.view.walk().collect();
        for n in nodes {
            self.view.set_active(n, ready);
            if self.view.node(n).ctrl.name.as_deref() == Some("WrenchEvents_LoadingWindow") {
                self.view.set_visible(n, self.model.is_none());
            }
        }
    }

    fn read_parameter(&self, node: NodeId, spec: &ParamSpec) -> Result<ParamValue, String> {
        let text = self.view.edit_text(node);
        let bad = || "Enter a valid parameter value before sending.".to_string();
        Ok(match spec {
            ParamSpec::Int { .. } => ParamValue::Int(text.trim().parse().map_err(|_| bad())?),
            ParamSpec::Float { .. } => {
                let v: f32 = text.trim().parse().map_err(|_| bad())?;
                if !v.is_finite() {
                    return Err(bad());
                }
                ParamValue::Float(v)
            }
            ParamSpec::Bool => ParamValue::Bool(self.view.bool_value(node)),
            ParamSpec::String { .. } => ParamValue::Text(text),
            ParamSpec::IntList { .. } => {
                if text.split_whitespace().any(|v| v.parse::<i64>().is_err()) {
                    return Err(bad());
                }
                ParamValue::Text(text)
            }
            ParamSpec::Vector { .. } => {
                let values: Vec<f32> = text
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad())?;
                if values.len() != 3 || values.iter().any(|v| !v.is_finite()) {
                    return Err(bad());
                }
                ParamValue::Vector([values[0], values[1], values[2]])
            }
            ParamSpec::Datablock { .. } => {
                let i = usize::try_from(self.view.selected(node).ok_or_else(bad)?)
                    .map_err(|_| bad())?;
                ParamValue::Datablock(
                    self.resources
                        .get(&node)
                        .and_then(|v| v.get(i))
                        .cloned()
                        .ok_or_else(bad)?,
                )
            }
            ParamSpec::List { items } => {
                let id = self.view.selected(node).ok_or_else(bad)?;
                if !items.iter().any(|(_, i)| *i == id) {
                    return Err(bad());
                }
                ParamValue::List(id)
            }
            ParamSpec::PaintColor { .. } => {
                let id = self.view.selected(node).ok_or_else(bad)?;
                if !self
                    .view
                    .node(node)
                    .state
                    .items
                    .iter()
                    .any(|(_, i)| *i == id)
                {
                    return Err(bad());
                }
                ParamValue::PaintColor(u32::try_from(id).map_err(|_| bad())?)
            }
            ParamSpec::Unknown { .. } => {
                return Err("Unsupported parameter specification is read only.".into());
            }
        })
    }

    fn accept_parameters(&mut self) -> Result<(), String> {
        let bindings = self.bindings.clone();
        for (node, binding) in bindings {
            match binding {
                Binding::Parameter(row, index, spec) => {
                    let value = self.read_parameter(node, &spec)?;
                    self.model
                        .as_mut()
                        .unwrap()
                        .set_param(row, index, value, &self.catalog);
                }
                Binding::Delay(row) => {
                    let m = self.model.as_mut().unwrap();
                    m.set_delay_text(row, self.view.edit_text(node));
                    m.accept_delay(row);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn cancel(&mut self, core: &mut Core) {
        if let Some(id) = self.request.take() {
            core.abandon(id);
        }
        self.save_draft(core);
        core.pop(ScreenId::WrenchEvents);
    }
}

impl Screen for WrenchEvents {
    fn id(&self) -> ScreenId {
        ScreenId::WrenchEvents
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
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if !self.current(core) {
            return;
        }
        if ev.kind == EventKind::Close {
            self.cancel(core);
            return;
        }
        if self.request.is_some() {
            return;
        }
        let command = command_of(&self.view, ev.node).to_ascii_lowercase();
        if ev.kind == EventKind::Click {
            if command == "canvas.popdialog(wrencheventsdlg);" {
                self.cancel(core);
                return;
            }
            if command == "wrencheventsdlg.clear();" {
                // Preserve read-only host tokens even when clearing runnable edits.
                if let Some(m) = &mut self.model {
                    m.rows.retain(|r| matches!(r, RowState::Preserved { .. }));
                    m.rows.push(RowState::Editable(events::EditRow::blank()));
                }
                self.error = None;
                self.save_draft(core);
                self.build(core);
                return;
            }
            if command == "wrencheventsdlg.send();" {
                match self.accept_parameters() {
                    Ok(()) => {
                        let m = self.model.as_ref().unwrap();
                        self.request = Some(core.request_pending(
                            UiAction::SendEvents {
                                brick: m.brick,
                                rows: m.to_send(),
                            },
                            Pending::Events,
                        ));
                        self.save_draft(core);
                        self.refresh(core);
                    }
                    Err(error) => {
                        self.error = Some(error);
                        core.message_ok("Event Parameter", self.error.as_deref().unwrap());
                    }
                }
                return;
            }
        }
        if !matches!(ev.kind, EventKind::Changed | EventKind::Submit) {
            return;
        }
        if self.view.node(ev.node).ctrl.name.as_deref() == Some("WrenchLock_Events") {
            core.wrench.events_copy = if self.view.bool_value(ev.node) {
                self.model.clone()
            } else {
                None
            };
            return;
        }
        let Some(binding) = self.bindings.get(&ev.node).cloned() else {
            return;
        };
        let selected = self.view.selected_text(ev.node);
        let mut rebuild = true;
        match binding {
            Binding::Enabled(row) => {
                self.model
                    .as_mut()
                    .unwrap()
                    .set_enabled(row, self.view.bool_value(ev.node));
                rebuild = false;
            }
            Binding::Delay(row) => {
                let m = self.model.as_mut().unwrap();
                m.set_delay_text(row, self.view.edit_text(ev.node));
                if ev.kind == EventKind::Submit {
                    m.accept_delay(row);
                } else {
                    rebuild = false;
                }
            }
            Binding::Input(row) => {
                let input = selected.filter(|s| s != "-");
                if input
                    .as_ref()
                    .is_some_and(|s| !EventsModel::input_choices(&self.catalog).contains(s))
                {
                    return;
                }
                self.model.as_mut().unwrap().set_input(row, input);
            }
            Binding::Target(row) => {
                if let Some(target) = selected {
                    self.model
                        .as_mut()
                        .unwrap()
                        .set_target(row, target, &self.catalog);
                }
            }
            Binding::Named(row) => {
                if let Some(name) = selected {
                    self.model.as_mut().unwrap().set_named(row, name);
                }
            }
            Binding::Output(row) => {
                if let Some(output) = selected {
                    self.model
                        .as_mut()
                        .unwrap()
                        .set_output(row, output, &self.catalog);
                }
            }
            Binding::Parameter(row, index, spec) => {
                match self.read_parameter(ev.node, &spec) {
                    Ok(value) => {
                        if let ParamValue::PaintColor(color) = &value
                            && let Some(n) = self
                                .view
                                .id(&format!("WrenchEvent_{row}_param{index}_swatch"))
                        {
                            self.view.state(n).tint = core.hud.color(*color).map(rgba);
                        }
                        self.model
                            .as_mut()
                            .unwrap()
                            .set_param(row, index, value, &self.catalog);
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
                rebuild = false;
            }
        }
        self.save_draft(core);
        if rebuild {
            self.build(core);
        }
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            self.cancel(core);
            true
        } else {
            false
        }
    }
    fn on_update(&mut self, core: &mut Core) {
        let catalog = editable_catalog(&core.events);
        if catalog != self.catalog
            || self.datablocks != core.datablocks
            || self.paint != core.hud.paint
        {
            self.catalog = catalog;
            self.datablocks = core.datablocks.clone();
            self.paint = core.hud.paint.clone();
            self.preserve_unsupported();
            self.save_draft(core);
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
        if self.request != Some(id) {
            return false;
        }
        self.request = None;
        if !self.current(core) {
            return true;
        }
        match result {
            Ok(()) => {
                self.save_draft(core);
                core.pop(self.id());
            }
            Err(reason) => core.message_ok("Events Rejected", reason),
        }
        self.refresh(core);
        true
    }
    fn layout(&mut self, w: i32, h: i32, core: &mut Core) {
        self.build(core);
        self.view.layout(w, h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{EventInputInfo, EventOutputInfo, EventRow, Settings, UiUpdate, WrenchData};
    use crate::binds::Platform;
    use crate::input::{InputEvent, MouseButton};
    use crate::schema::UiPack;
    use crate::ui::{StackCmd, Ui, UiConfig};
    use std::rc::Rc;

    fn fixture() -> Ui {
        let mut data = UiPack::default();
        for (layout, prefix, variant) in [
            ("wrenchDlg", "Wrench", WrenchVariant::Normal),
            ("wrenchSoundDlg", "WrenchSound", WrenchVariant::Sound),
            (
                "wrenchVehicleSpawnDlg",
                "WrenchVehicleSpawn",
                WrenchVariant::VehicleSpawn,
            ),
        ] {
            let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
            let mut win = named(
                ctrl(
                    "GuiWindowCtrl",
                    "GuiDefaultProfile",
                    Rect::new(0, 0, 400, 470),
                ),
                format!("{prefix}_Window"),
            );
            for (i, &field) in WrenchField::for_variant(variant).iter().enumerate() {
                let class = match field {
                    WrenchField::Name | WrenchField::ItemRespawn => "GuiTextEditCtrl",
                    WrenchField::Light
                    | WrenchField::Emitter
                    | WrenchField::Item
                    | WrenchField::Sound
                    | WrenchField::Vehicle => "GuiPopUpMenuCtrl",
                    _ => "GuiCheckBoxCtrl",
                };
                win.children.push(named(
                    ctrl(
                        class,
                        "GuiDefaultProfile",
                        Rect::new(15, 25 + i as i32 * 25, 190, 22),
                    ),
                    format!("{prefix}_{}", suffix(field)),
                ));
                win.children.push(named(
                    ctrl(
                        "GuiCheckBoxCtrl",
                        "GuiDefaultProfile",
                        Rect::new(220, 25 + i as i32 * 25, 50, 22),
                    ),
                    format!("{prefix}Lock_{}", suffix(field)),
                ));
            }
            for (i, (name, command)) in [
                ("Send", format!("{layout}.send();")),
                ("Cancel", format!("canvas.popDialog({layout});")),
                ("Events", "canvas.pushDialog(WrenchEventsDlg);".into()),
                ("Respawn", format!("{layout}.respawn();")),
            ]
            .into_iter()
            .enumerate()
            {
                win.children.push(named(
                    button(
                        "GuiDefaultProfile",
                        Rect::new(5 + i as i32 * 95, 400, 90, 25),
                        "",
                        name,
                        &command,
                    ),
                    format!("{prefix}_{name}"),
                ));
            }
            root.children.push(win);
            data.layouts.insert(layout.into(), root);
        }
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut win = named(
            ctrl(
                "GuiWindowCtrl",
                "GuiDefaultProfile",
                Rect::new(0, 0, 640, 480),
            ),
            "WrenchEvents_Window",
        );
        for (label, command) in [
            ("Send", "wrenchEventsDlg.send();"),
            ("Cancel", "canvas.popDialog(wrenchEventsDlg);"),
            ("Clear", "wrenchEventsDlg.clear();"),
        ] {
            win.children.push(named(
                button(
                    "GuiDefaultProfile",
                    Rect::new(0, 0, 91, 30),
                    "",
                    label,
                    command,
                ),
                format!("Events_{label}"),
            ));
        }
        win.children.push(named(
            ctrl(
                "GuiCheckBoxCtrl",
                "GuiDefaultProfile",
                Rect::new(0, 0, 42, 22),
            ),
            "WrenchLock_Events",
        ));
        root.children.push(win);
        data.layouts.insert("wrenchEventsDlg".into(), root);
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(data, Default::default())),
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
        ui.core.events = EventCatalog {
            inputs: vec![
                EventInputInfo {
                    name: "onActivate".into(),
                    targets: vec![
                        ("Self".into(), "fxDTSBrick".into()),
                        ("Player".into(), "Player".into()),
                    ],
                    supported: true,
                },
                EventInputInfo {
                    name: "onUnsupported".into(),
                    targets: vec![],
                    supported: false,
                },
            ],
            outputs: vec![
                EventOutputInfo {
                    class: "fxDTSBrick".into(),
                    name: "setLight".into(),
                    params: vec![ParamSpec::Datablock {
                        class: "FxLightData".into(),
                    }],
                    supported: true,
                },
                EventOutputInfo {
                    class: "Player".into(),
                    name: "kill".into(),
                    params: vec![],
                    supported: false,
                },
                EventOutputInfo {
                    class: "fxDTSBrick".into(),
                    name: "setVector".into(),
                    params: vec![ParamSpec::Vector { max: 10.0 }],
                    supported: true,
                },
            ],
        };
        for class in ["FxLightData", "ItemData", "Music", "Vehicle"] {
            ui.core.datablocks.insert(
                class.into(),
                vec![
                    Choice {
                        id: format!("{class}:beta"),
                        name: "Beta".into(),
                    },
                    Choice {
                        id: format!("{class}:alpha"),
                        name: "Alpha".into(),
                    },
                ],
            );
        }
        ui.core.wrench.open(
            10,
            WrenchVariant::Normal,
            "Owner".into(),
            WrenchData::default(),
            false,
            true,
        );
        ui.core
            .wrench
            .open_events(10, vec![], vec!["door".into()], true, &ui.core.events);
        ui.core.cmds.clear();
        ui.drain_actions();
        ui
    }

    fn click(s: &mut dyn Screen, name: &str, core: &mut Core) {
        let n = s.view().id(name).unwrap();
        let r = s.view().node(n).rect;
        let mut events = vec![];
        s.view_mut().mouse_down(
            MouseButton::Left,
            r.x + r.w / 2,
            r.y + r.h / 2,
            &core.pack,
            &mut events,
        );
        s.view_mut().mouse_up(
            MouseButton::Left,
            r.x + r.w / 2,
            r.y + r.h / 2,
            &core.pack,
            &mut events,
        );
        for event in events {
            s.on_event(&event, core);
        }
    }

    fn choose(s: &mut dyn Screen, name: &str, label: &str, core: &mut Core) {
        let node = s.view().id(name).unwrap();
        let id = s
            .view()
            .node(node)
            .state
            .items
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing {label} in {name}"))
            .1;
        s.view_mut().select(node, Some(id));
        s.on_event(
            &ViewEvent {
                node,
                kind: EventKind::Changed,
            },
            core,
        );
    }

    fn edit(s: &mut dyn Screen, name: &str, value: &str, core: &mut Core) {
        let node = s.view().id(name).unwrap();
        s.view_mut().set_text(node, value);
        s.on_event(
            &ViewEvent {
                node,
                kind: EventKind::Changed,
            },
            core,
        );
    }

    #[test]
    fn properties_copy_pending_reject_retry_and_cancel() {
        let mut ui = fixture();
        let mut s = Wrench::new(&ui.core, WrenchVariant::Normal);
        choose(&mut s, "Wrench_Lights", "Beta", &mut ui.core);
        click(&mut s, "WrenchLock_Lights", &mut ui.core);
        edit(&mut s, "Wrench_Name", "  door  ", &mut ui.core);
        edit(&mut s, "Wrench_ItemRespawnTime", "5", &mut ui.core);
        click(&mut s, "Wrench_Send", &mut ui.core);
        click(&mut s, "Wrench_Send", &mut ui.core);
        let actions = ui.drain_actions();
        assert_eq!(actions.len(), 1);
        let (id, UiAction::SendWrench { brick, data, .. }) = &actions[0] else {
            panic!()
        };
        assert_eq!(*brick, 10);
        assert_eq!(data.name, "door");
        assert_eq!(data.item_respawn_ms, 5000);
        assert_eq!(data.light.as_deref(), Some("FxLightData:beta"));
        s.on_result(
            *id,
            Some(&Pending::Wrench),
            &Err("permission denied".into()),
            &mut ui.core,
        );
        assert!(!ui.core.cmds.iter().any(|c| matches!(c, StackCmd::Pop(_))));
        click(&mut s, "Wrench_Send", &mut ui.core);
        let retry = ui.drain_actions()[0].0;
        ui.core.wrench.open(
            11,
            WrenchVariant::Normal,
            "New".into(),
            WrenchData::default(),
            false,
            true,
        );
        assert_eq!(
            ui.core
                .wrench
                .values(WrenchVariant::Normal)
                .light
                .as_deref(),
            Some("FxLightData:beta")
        );
        s.on_result(retry, Some(&Pending::Wrench), &Ok(()), &mut ui.core);
        assert_eq!(ui.core.wrench.open.as_ref().unwrap().brick, 11);
        let mut next = Wrench::new(&ui.core, WrenchVariant::Normal);
        next.on_key(Key::Escape, Modifiers::NONE, &mut ui.core);
        assert!(
            ui.drain_actions()
                .iter()
                .any(|(_, a)| matches!(a, UiAction::CancelWrench { brick: 11 }))
        );
    }

    #[test]
    fn typing_reaches_an_open_wrench_dropdown_search() {
        let mut ui = fixture();
        ui.apply(UiUpdate::OpenWrench {
            brick: 10,
            variant: WrenchVariant::Normal,
            owner: "Owner".into(),
            data: WrenchData::default(),
            admin_override: false,
            events_allowed: true,
        });
        let id = ScreenId::Wrench(WrenchVariant::Normal);
        assert_eq!(ui.top_id(), id);
        assert!(!ui.takes_text(), "nothing to type into yet");
        let (x, y) = ui.control_center(id, "Wrench_Lights").unwrap();
        for ev in [
            InputEvent::MouseMove { x, y },
            InputEvent::MouseDown {
                button: MouseButton::Left,
                x,
                y,
            },
            InputEvent::MouseUp {
                button: MouseButton::Left,
                x,
                y,
            },
        ] {
            ui.handle_input(ev);
        }
        let view = ui.screen(id).unwrap().view();
        assert_eq!(view.open_popup_node(), view.id("Wrench_Lights"));
        // The platform delivers typed characters only while this holds.
        assert!(ui.takes_text(), "an open dropdown's search takes typing");
        for ch in "alp".chars() {
            ui.handle_input(InputEvent::KeyDown {
                key: Key::Letter(ch),
                mods: Modifiers::NONE,
                repeat: false,
            });
            ui.handle_input(InputEvent::Char(ch));
            ui.handle_input(InputEvent::KeyUp {
                key: Key::Letter(ch),
                mods: Modifiers::NONE,
            });
        }
        let view = ui.screen(id).unwrap().view();
        assert_eq!(view.popup_query(), Some("alp"));
        assert_eq!(
            view.popup_highlight().map(|(t, _)| t).as_deref(),
            Some("Alpha")
        );
        ui.handle_input(InputEvent::KeyDown {
            key: Key::Return,
            mods: Modifiers::NONE,
            repeat: false,
        });
        let view = ui.screen(id).unwrap().view();
        assert_eq!(view.open_popup_node(), None);
        assert_eq!(
            view.selected_text(view.id("Wrench_Lights").unwrap())
                .as_deref(),
            Some("Alpha")
        );
        assert!(!ui.takes_text());
    }

    #[test]
    fn item_respawn_field_shows_source_seconds_and_sends_native_milliseconds() {
        let mut ui = fixture();
        ui.core.wrench.open(
            10,
            WrenchVariant::Normal,
            "Owner".into(),
            WrenchData {
                item_respawn_ms: 4000,
                ..Default::default()
            },
            false,
            true,
        );
        let mut s = Wrench::new(&ui.core, WrenchVariant::Normal);
        let n = s.view.id("Wrench_ItemRespawnTime").unwrap();
        assert_eq!(s.view.edit_text(n), "4");
        edit(&mut s, "Wrench_ItemRespawnTime", "2.9", &mut ui.core);
        click(&mut s, "Wrench_Send", &mut ui.core);
        assert!(ui.drain_actions().iter().any(
            |(_, a)| matches!(a,UiAction::SendWrench {data,..} if data.item_respawn_ms==2000)
        ));
    }

    #[test]
    fn sound_vehicle_and_event_request_use_typed_host_ids() {
        let mut ui = fixture();
        ui.core.wrench.open(
            15,
            WrenchVariant::Sound,
            "Owner".into(),
            WrenchData::default(),
            false,
            true,
        );
        let mut sound = Wrench::new(&ui.core, WrenchVariant::Sound);
        choose(&mut sound, "WrenchSound_Sounds", "Alpha", &mut ui.core);
        click(&mut sound, "WrenchSound_Send", &mut ui.core);
        assert!(ui.drain_actions().iter().any(|(_, a)| matches!(a, UiAction::SendWrench { variant: WrenchVariant::Sound, data, .. } if data.sound.as_deref() == Some("Music:alpha"))));
        ui.core.wrench.open(
            16,
            WrenchVariant::VehicleSpawn,
            "Owner".into(),
            WrenchData::default(),
            false,
            true,
        );
        let mut vehicle = Wrench::new(&ui.core, WrenchVariant::VehicleSpawn);
        choose(
            &mut vehicle,
            "WrenchVehicleSpawn_Vehicles",
            "Beta",
            &mut ui.core,
        );
        click(&mut vehicle, "WrenchVehicleSpawn_Respawn", &mut ui.core);
        assert!(ui.drain_actions().iter().any(|(_, a)| matches!(a, UiAction::RespawnVehicle { brick:16, vehicle: Some(v) } if v == "Vehicle:beta")));
        ui.core.wrench.open(
            17,
            WrenchVariant::Normal,
            "Owner".into(),
            WrenchData::default(),
            false,
            true,
        );
        let mut wrench = Wrench::new(&ui.core, WrenchVariant::Normal);
        click(&mut wrench, "Wrench_Events", &mut ui.core);
        assert!(
            ui.drain_actions()
                .iter()
                .any(|(_, a)| matches!(a, UiAction::RequestEvents { brick: 17 }))
        );
        ui.core
            .wrench
            .open_events(17, vec![], vec![], true, &ui.core.events);
        wrench.on_update(&mut ui.core);
        assert!(wrench.request.is_none());
    }

    #[test]
    fn escape_backs_out_of_a_wrench_that_is_still_sending() {
        let mut ui = fixture();
        ui.core.wrench.open(
            18,
            WrenchVariant::Normal,
            "Owner".into(),
            WrenchData::default(),
            false,
            true,
        );
        let mut wrench = Wrench::new(&ui.core, WrenchVariant::Normal);
        click(&mut wrench, "Wrench_Events", &mut ui.core);
        let id = wrench.request.expect("waiting for the events").0;
        // The events never come (the host lost the brick): Escape still works.
        assert!(wrench.on_key(Key::Escape, Modifiers::default(), &mut ui.core));
        assert!(wrench.request.is_none());
        assert!(!ui.core.pending.contains_key(&id));
        assert!(
            ui.drain_actions()
                .iter()
                .any(|(_, a)| matches!(a, UiAction::CancelWrench { brick: 18 }))
        );
    }

    #[test]
    fn event_cascade_delay_parameters_preservation_and_pending() {
        let mut ui = fixture();
        let preserved = EventRow::Preserved {
            enabled: false,
            text: "legacy event".into(),
            token: "brick10:row0".into(),
        };
        ui.core.wrench.open_events(
            10,
            vec![preserved.clone()],
            vec!["door".into()],
            true,
            &ui.core.events,
        );
        let mut s = WrenchEvents::new(&ui.core);
        assert!(s.view.id("WrenchEvent_0_preserved").is_some());
        assert!(s.view.id("WrenchEvent_0_enabled").is_none());
        let input = s.view.id("WrenchEvent_1_input").unwrap();
        assert!(
            !s.view
                .node(input)
                .state
                .items
                .iter()
                .any(|(s, _)| s == "onUnsupported")
        );
        choose(&mut s, "WrenchEvent_1_input", "onActivate", &mut ui.core);
        choose(&mut s, "WrenchEvent_1_target", NAMED_BRICK, &mut ui.core);
        choose(&mut s, "WrenchEvent_1_named", "door", &mut ui.core);
        choose(&mut s, "WrenchEvent_1_output", "setLight", &mut ui.core);
        choose(&mut s, "WrenchEvent_1_param0", "Alpha", &mut ui.core);
        edit(&mut s, "WrenchEvent_1_delay", "99999", &mut ui.core);
        click(&mut s, "Events_Send", &mut ui.core);
        click(&mut s, "Events_Send", &mut ui.core);
        let actions = ui.drain_actions();
        assert_eq!(actions.len(), 1);
        let (id, UiAction::SendEvents { rows, brick: 10 }) = &actions[0] else {
            panic!("{actions:?}")
        };
        assert_eq!(rows[0], preserved);
        let EventRow::Editable(line) = &rows[1] else {
            panic!()
        };
        assert_eq!(line.delay_ms, 30000);
        assert_eq!(line.named_target.as_deref(), Some("door"));
        assert_eq!(
            line.params,
            vec![ParamValue::Datablock(Some("FxLightData:alpha".into()))]
        );
        s.on_result(
            *id,
            Some(&Pending::Events),
            &Err("not allowed".into()),
            &mut ui.core,
        );
        choose(&mut s, "WrenchEvent_1_target", "Player", &mut ui.core);
        let output = s.view.id("WrenchEvent_1_output").unwrap();
        assert!(
            s.view.node(output).state.items.is_empty(),
            "unsupported Player output never advertised"
        );
        click(&mut s, "Events_Clear", &mut ui.core);
        assert_eq!(s.model.as_ref().unwrap().to_send(), vec![preserved]);
    }

    #[test]
    fn invalid_vector_does_not_send_and_cancel_does_not_apply() {
        let mut ui = fixture();
        let mut s = WrenchEvents::new(&ui.core);
        choose(&mut s, "WrenchEvent_0_input", "onActivate", &mut ui.core);
        choose(&mut s, "WrenchEvent_0_target", "Self", &mut ui.core);
        choose(&mut s, "WrenchEvent_0_output", "setVector", &mut ui.core);
        edit(&mut s, "WrenchEvent_0_param0", "NaN 2 3", &mut ui.core);
        click(&mut s, "Events_Send", &mut ui.core);
        assert!(ui.drain_actions().is_empty());
        edit(&mut s, "WrenchEvent_0_param0", "20 2 -30", &mut ui.core);
        click(&mut s, "WrenchLock_Events", &mut ui.core);
        assert!(ui.core.wrench.events_copy.is_some());
        s.on_key(Key::Escape, Modifiers::NONE, &mut ui.core);
        assert!(ui.drain_actions().is_empty());
        assert!(
            ui.core
                .cmds
                .contains(&StackCmd::Pop(ScreenId::WrenchEvents))
        );
        let sent = ui.core.wrench.events_copy.as_ref().unwrap().to_send();
        let EventRow::Editable(line) = &sent[0] else {
            panic!()
        };
        assert_eq!(line.params, vec![ParamValue::Vector([10.0, 2.0, -10.0])]);
    }

    #[test]
    fn typed_parameters_validate_clamp_and_follow_catalog_capabilities() {
        let mut ui = fixture();
        ui.core.events.outputs.push(EventOutputInfo {
            class: "fxDTSBrick".into(),
            name: "typed".into(),
            supported: true,
            params: vec![
                ParamSpec::Int {
                    min: -2,
                    max: 5,
                    default: 0,
                },
                ParamSpec::Float {
                    min: 0.0,
                    max: 1.0,
                    step: 0.25,
                    default: 0.0,
                },
                ParamSpec::String {
                    max_length: 3,
                    width: 100,
                },
                ParamSpec::List {
                    items: vec![("Up".into(), 7), ("Down".into(), 9)],
                },
            ],
        });
        let mut s = WrenchEvents::new(&ui.core);
        choose(&mut s, "WrenchEvent_0_input", "onActivate", &mut ui.core);
        choose(&mut s, "WrenchEvent_0_target", "Self", &mut ui.core);
        choose(&mut s, "WrenchEvent_0_output", "typed", &mut ui.core);
        edit(&mut s, "WrenchEvent_0_param0", "99", &mut ui.core);
        edit(&mut s, "WrenchEvent_0_param1", "0.63", &mut ui.core);
        edit(&mut s, "WrenchEvent_0_param2", "abcdef", &mut ui.core);
        choose(&mut s, "WrenchEvent_0_param3", "Down", &mut ui.core);
        click(&mut s, "Events_Send", &mut ui.core);
        let actions = ui.drain_actions();
        let (id, UiAction::SendEvents { rows, .. }) = &actions[0] else {
            panic!()
        };
        let EventRow::Editable(line) = &rows[0] else {
            panic!()
        };
        assert_eq!(
            line.params,
            vec![
                ParamValue::Int(5),
                ParamValue::Float(0.75),
                ParamValue::Text("abc".into()),
                ParamValue::List(9)
            ]
        );
        s.on_result(
            *id,
            Some(&Pending::Events),
            &Err("retry".into()),
            &mut ui.core,
        );
        ui.core.events.outputs.last_mut().unwrap().supported = false;
        s.on_update(&mut ui.core);
        assert!(s.view.id("WrenchEvent_0_preserved").is_some());
        assert!(s.view.id("WrenchEvent_0_param0").is_none());
    }

    #[test]
    fn wrong_target_class_and_unknown_parameter_rows_are_read_only() {
        let mut ui = fixture();
        ui.core.events.outputs.push(EventOutputInfo {
            class: "fxDTSBrick".into(),
            name: "unknown".into(),
            supported: true,
            params: vec![ParamSpec::Unknown {
                text: "newtype".into(),
            }],
        });
        let make = |target: &str, output: &str, param| {
            EventRow::Editable(crate::api::EventLine {
                enabled: true,
                delay_ms: 0,
                input: "onActivate".into(),
                target: target.into(),
                named_target: None,
                output: output.into(),
                params: vec![param],
            })
        };
        ui.core.wrench.open_events(
            10,
            vec![
                make("Player", "setLight", ParamValue::Datablock(None)),
                make("Self", "unknown", ParamValue::Text("opaque".into())),
                make("Self", "setLight", ParamValue::Int(7)),
            ],
            vec![],
            true,
            &ui.core.events,
        );
        let s = WrenchEvents::new(&ui.core);
        assert!(
            s.model.as_ref().unwrap().rows[..3]
                .iter()
                .all(|r| matches!(r, RowState::Preserved { .. }))
        );
    }

    #[test]
    #[cfg(feature = "gpu")]
    #[ignore = "bounded offscreen rendering requires converted original content and GPU"]
    fn authored_wrench_offscreen() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = Rc::new(Pack::load(&root.join("content/ui-pack-004")).unwrap());
        let mut ui = fixture();
        ui.core.pack = pack.clone();
        let output = root.join("artifacts/ui-native-wrench");
        std::fs::create_dir_all(&output).unwrap();
        let gpu = crate::gpu::Headless::new().unwrap();
        let mut renderer = crate::gpu::UiRenderer::new(&gpu.device, &gpu.queue);
        let mut render = |name: &str, screen: &mut dyn Screen, core: &mut Core| {
            screen.layout(640, 480, core);
            let mut list = DrawList::new(Rect::new(0, 0, 640, 480));
            screen.draw(&pack, &mut list, core);
            assert!(list.glyph_count() > 20, "{name}");
            let rgba = gpu
                .render_rgba(
                    &mut renderer,
                    &pack,
                    &list,
                    (640, 480),
                    1.0,
                    [0.15, 0.15, 0.18, 1.0],
                )
                .unwrap();
            assert!(
                renderer.missing_textures().next().is_none(),
                "missing texture in {name}"
            );
            image::save_buffer(
                output.join(format!("{name}.png")),
                &rgba,
                640,
                480,
                image::ColorType::Rgba8,
            )
            .unwrap();
        };
        for variant in [
            WrenchVariant::Normal,
            WrenchVariant::Sound,
            WrenchVariant::VehicleSpawn,
        ] {
            ui.core.wrench.open(
                10,
                variant,
                "Maxwell".into(),
                WrenchData {
                    name: "test_brick".into(),
                    light: Some("FxLightData:beta".into()),
                    raycasting: true,
                    colliding: true,
                    rendering: true,
                    ..Default::default()
                },
                false,
                true,
            );
            render(
                &format!("{variant:?}"),
                &mut Wrench::new(&ui.core, variant),
                &mut ui.core,
            );
        }
        ui.core.wrench.open_events(
            10,
            vec![EventRow::Preserved {
                enabled: false,
                text: "onRelay -> Player legacyEffect (unavailable on host)".into(),
                token: "host-row:10:0".into(),
            }],
            vec!["door".into()],
            true,
            &ui.core.events,
        );
        let mut events = WrenchEvents::new(&ui.core);
        choose(
            &mut events,
            "WrenchEvent_1_input",
            "onActivate",
            &mut ui.core,
        );
        choose(
            &mut events,
            "WrenchEvent_1_target",
            NAMED_BRICK,
            &mut ui.core,
        );
        choose(&mut events, "WrenchEvent_1_named", "door", &mut ui.core);
        choose(
            &mut events,
            "WrenchEvent_1_output",
            "setLight",
            &mut ui.core,
        );
        choose(&mut events, "WrenchEvent_1_param0", "Alpha", &mut ui.core);
        render("Events", &mut events, &mut ui.core);
        std::fs::write(output.join("verification.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version":1, "pack":"content/ui-pack-004", "viewport":[640,480],
            "screens":["Normal","Sound","VehicleSpawn","Events"], "no_missing_textures":true,
            "scope":"bounded offscreen dialog check; host actions and interactive play remain separate"
        })).unwrap()).unwrap();
    }
}
