//! Wrench Events editing (`wrenchEventsDlg`, c:17787–18700).
//!
//! Menus list only the host-supported inputs/outputs of the catalog, sorted
//! by name like v20's `%menu.sort()`. Imported rows the host cannot run are
//! kept as read-only [`EventRow::Preserved`] rows and sent back unchanged.

use crate::api::{EventCatalog, EventLine, EventRow, ParamValue};
use crate::schema::ParamSpec;

pub const NAMED_BRICK: &str = "<NAMED BRICK>";
pub const MAX_DELAY_MS: u32 = 30_000;

#[derive(Debug, Clone, PartialEq)]
pub enum RowState {
    Editable(EditRow),
    Preserved {
        enabled: bool,
        text: String,
        token: String,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EditRow {
    pub enabled: bool,
    /// Delay field text (clamped to 0..=30000 when accepted).
    pub delay_text: String,
    /// Input event name (`None` = "-").
    pub input: Option<String>,
    pub target: Option<String>,
    pub named: Option<String>,
    pub output: Option<String>,
    pub params: Vec<ParamValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventsModel {
    pub brick: u64,
    pub rows: Vec<RowState>,
    pub named_targets: Vec<String>,
    pub allow_named: bool,
}

/// `mClamp(value, 0, 30000)` on the delay text (atoi semantics).
pub fn clamp_delay(text: &str) -> u32 {
    let t = text.trim();
    let digits: String = t
        .char_indices()
        .take_while(|(i, c)| c.is_ascii_digit() || (*i == 0 && *c == '-'))
        .map(|(_, c)| c)
        .collect();
    digits
        .parse::<i64>()
        .unwrap_or(0)
        .clamp(0, MAX_DELAY_MS as i64) as u32
}

pub fn default_param(spec: &ParamSpec) -> ParamValue {
    match spec {
        ParamSpec::Int { default, .. } => ParamValue::Int(*default),
        ParamSpec::IntList { .. } => ParamValue::Text(String::new()),
        ParamSpec::Float { default, .. } => ParamValue::Float(*default),
        ParamSpec::Bool => ParamValue::Bool(false),
        ParamSpec::String { .. } => ParamValue::Text(String::new()),
        ParamSpec::Datablock { .. } => ParamValue::Datablock(None),
        ParamSpec::Vector { .. } => ParamValue::Vector([0.0; 3]),
        ParamSpec::List { items } => ParamValue::List(items.first().map_or(0, |i| i.1)),
        ParamSpec::PaintColor { default } => ParamValue::PaintColor(*default as u32),
        ParamSpec::Unknown { .. } => ParamValue::Text(String::new()),
    }
}

/// Validate/clamp a parameter the way the dialog's widgets do.
pub fn clamp_param(spec: &ParamSpec, v: ParamValue) -> ParamValue {
    match (spec, v) {
        (ParamSpec::Int { min, max, .. }, ParamValue::Int(x)) => {
            ParamValue::Int(x.clamp(*min, *max))
        }
        (ParamSpec::Float { min, max, step, .. }, ParamValue::Float(x)) => {
            let s = if *step > 0.0 {
                ((x - min) / step).round() * step + min
            } else {
                x
            };
            ParamValue::Float(s.clamp(*min, *max))
        }
        (ParamSpec::String { max_length, .. }, ParamValue::Text(t)) => {
            ParamValue::Text(t.chars().take(*max_length as usize).collect())
        }
        (ParamSpec::Vector { max }, ParamValue::Vector(v)) if *max > 0.0 => {
            ParamValue::Vector(v.map(|c| c.clamp(-max, *max)))
        }
        (_, v) => v,
    }
}

impl EventsModel {
    /// Open with the brick's rows; a trailing blank row is always present.
    pub fn open(
        brick: u64,
        rows: Vec<EventRow>,
        named_targets: Vec<String>,
        allow_named: bool,
        catalog: &EventCatalog,
    ) -> Self {
        let mut m = EventsModel {
            brick,
            rows: Vec::new(),
            named_targets,
            allow_named,
        };
        for r in rows {
            m.rows.push(match r {
                EventRow::Editable(l) if m.line_supported(&l, catalog) => {
                    RowState::Editable(EditRow {
                        enabled: l.enabled,
                        delay_text: l.delay_ms.min(MAX_DELAY_MS).to_string(),
                        input: Some(l.input),
                        target: Some(l.target),
                        named: l.named_target,
                        output: Some(l.output),
                        params: l.params,
                    })
                }
                EventRow::Editable(l) => RowState::Preserved {
                    enabled: l.enabled,
                    text: describe(&l),
                    token: serde_json::to_string(&l).unwrap_or_default(),
                },
                EventRow::Preserved {
                    enabled,
                    text,
                    token,
                } => RowState::Preserved {
                    enabled,
                    text,
                    token,
                },
            });
        }
        m.rows.push(RowState::Editable(EditRow::blank()));
        m
    }

    fn line_supported(&self, l: &EventLine, c: &EventCatalog) -> bool {
        let input_ok = c.inputs.iter().any(|i| i.supported && i.name == l.input);
        let output_ok = c.outputs.iter().any(|o| o.supported && o.name == l.output);
        let named_ok = l.target != NAMED_BRICK || self.allow_named;
        input_ok && output_ok && named_ok
    }

    /// Sorted supported input names (first entry of the popup is "-").
    pub fn input_choices(c: &EventCatalog) -> Vec<String> {
        let mut v: Vec<String> = c
            .inputs
            .iter()
            .filter(|i| i.supported)
            .map(|i| i.name.clone())
            .collect();
        v.sort_by_key(|s| s.to_ascii_lowercase());
        v
    }

    /// Targets for an input in registration order, plus `<NAMED BRICK>`.
    pub fn target_choices(&self, c: &EventCatalog, input: &str) -> Vec<String> {
        let mut v: Vec<String> = c
            .inputs
            .iter()
            .find(|i| i.name == input)
            .map(|i| i.targets.iter().map(|t| t.0.clone()).collect())
            .unwrap_or_default();
        if self.allow_named {
            v.push(NAMED_BRICK.to_string());
        }
        v
    }

    /// Class of a target (`fxDTSBrick` for named bricks).
    pub fn target_class(c: &EventCatalog, input: &str, target: &str) -> Option<String> {
        if target == NAMED_BRICK {
            return Some("fxDTSBrick".into());
        }
        c.inputs
            .iter()
            .find(|i| i.name == input)
            .and_then(|i| i.targets.iter().find(|t| t.0 == target))
            .map(|t| t.1.clone())
    }

    /// Sorted supported outputs for a class.
    pub fn output_choices(c: &EventCatalog, class: &str) -> Vec<String> {
        let mut v: Vec<String> = c
            .outputs
            .iter()
            .filter(|o| o.supported && o.class.eq_ignore_ascii_case(class))
            .map(|o| o.name.clone())
            .collect();
        v.sort_by_key(|s| s.to_ascii_lowercase());
        v
    }

    pub fn named_choices(&self) -> Vec<String> {
        let mut v = self.named_targets.clone();
        v.sort_by_key(|s| s.to_ascii_lowercase());
        v
    }

    fn edit(&mut self, row: usize) -> Option<&mut EditRow> {
        match self.rows.get_mut(row) {
            Some(RowState::Editable(e)) => Some(e),
            _ => None,
        }
    }

    /// Choose an input (`createTargetList`). `None` = "-": deletes the row
    /// unless it is the last one. Choosing an input on the trailing row
    /// appends a new blank row.
    pub fn set_input(&mut self, row: usize, input: Option<String>) {
        let last = row + 1 == self.rows.len();
        match input {
            None => {
                if !last {
                    self.rows.remove(row);
                } else if let Some(e) = self.edit(row) {
                    *e = EditRow::blank();
                }
            }
            Some(i) => {
                if let Some(e) = self.edit(row) {
                    let was_blank = e.input.is_none();
                    e.input = Some(i);
                    e.target = None;
                    e.named = None;
                    e.output = None;
                    e.params.clear();
                    if was_blank && last {
                        self.rows.push(RowState::Editable(EditRow::blank()));
                    }
                }
            }
        }
    }

    /// Choose a target (`createOutputList`): a different target class
    /// clears the output and parameters; the same class keeps them.
    pub fn set_target(&mut self, row: usize, target: String, c: &EventCatalog) {
        let Some(e) = self.edit(row) else { return };
        let input = e.input.clone().unwrap_or_default();
        let old = e
            .target
            .as_deref()
            .and_then(|t| Self::target_class(c, &input, t));
        let new = Self::target_class(c, &input, &target);
        e.target = Some(target.clone());
        if target != NAMED_BRICK {
            e.named = None;
        }
        if old != new {
            e.output = None;
            e.params.clear();
        }
    }

    pub fn set_named(&mut self, row: usize, name: String) {
        if let Some(e) = self.edit(row) {
            e.named = Some(name);
        }
    }

    /// Choose an output (`createOutputParameters`): parameters reset to
    /// their defaults.
    pub fn set_output(&mut self, row: usize, output: String, c: &EventCatalog) {
        let Some(e) = self.edit(row) else { return };
        let input = e.input.clone().unwrap_or_default();
        let class = e
            .target
            .as_deref()
            .and_then(|t| Self::target_class(c, &input, t))
            .unwrap_or_default();
        let specs = c
            .outputs
            .iter()
            .find(|o| o.name == output && o.class.eq_ignore_ascii_case(&class))
            .map(|o| o.params.clone())
            .unwrap_or_default();
        e.output = Some(output);
        e.params = specs.iter().take(4).map(default_param).collect();
    }

    pub fn set_param(&mut self, row: usize, i: usize, v: ParamValue, c: &EventCatalog) {
        let spec = self.param_specs(row, c).get(i).cloned();
        if let (Some(e), Some(spec)) = (self.edit(row), spec)
            && i < e.params.len()
        {
            e.params[i] = clamp_param(&spec, v);
        }
    }

    pub fn set_enabled(&mut self, row: usize, on: bool) {
        match self.rows.get_mut(row) {
            Some(RowState::Editable(e)) => e.enabled = on,
            Some(RowState::Preserved { enabled, .. }) => *enabled = on,
            None => {}
        }
    }

    pub fn set_delay_text(&mut self, row: usize, text: String) {
        if let Some(e) = self.edit(row) {
            e.delay_text = text;
        }
    }

    /// Delay field accepted (Enter / focus loss): clamp like v20.
    pub fn accept_delay(&mut self, row: usize) {
        if let Some(e) = self.edit(row) {
            e.delay_text = clamp_delay(&e.delay_text).to_string();
        }
    }

    pub fn param_specs(&self, row: usize, c: &EventCatalog) -> Vec<ParamSpec> {
        let Some(RowState::Editable(e)) = self.rows.get(row) else {
            return Vec::new();
        };
        let (Some(input), Some(target), Some(output)) = (&e.input, &e.target, &e.output) else {
            return Vec::new();
        };
        let class = Self::target_class(c, input, target).unwrap_or_default();
        c.outputs
            .iter()
            .find(|o| &o.name == output && o.class.eq_ignore_ascii_case(&class))
            .map(|o| o.params.iter().take(4).cloned().collect())
            .unwrap_or_default()
    }

    /// Clear button: all rows removed, one blank row left.
    pub fn clear(&mut self) {
        self.rows = vec![RowState::Editable(EditRow::blank())];
    }

    /// Rows for `SendEvents`: complete editable rows plus preserved rows, in
    /// order. Incomplete rows (no target/output, or a named target without a
    /// name) are dropped like v20's send skips them.
    pub fn to_send(&self) -> Vec<EventRow> {
        self.rows
            .iter()
            .filter_map(|r| match r {
                RowState::Preserved {
                    enabled,
                    text,
                    token,
                } => Some(EventRow::Preserved {
                    enabled: *enabled,
                    text: text.clone(),
                    token: token.clone(),
                }),
                RowState::Editable(e) => {
                    let (input, target, output) =
                        (e.input.clone()?, e.target.clone()?, e.output.clone()?);
                    if target == NAMED_BRICK && e.named.is_none() {
                        return None;
                    }
                    Some(EventRow::Editable(EventLine {
                        enabled: e.enabled,
                        delay_ms: clamp_delay(&e.delay_text),
                        input,
                        target,
                        named_target: e.named.clone(),
                        output,
                        params: e.params.clone(),
                    }))
                }
            })
            .collect()
    }

    /// A removed named target invalidates the dialog (v20 closes it with
    /// "Named Target List Invalidated").
    pub fn uses_named(&self, name: &str) -> bool {
        self.rows
            .iter()
            .any(|r| matches!(r, RowState::Editable(e) if e.named.as_deref() == Some(name)))
    }
}

impl EditRow {
    pub fn blank() -> Self {
        EditRow {
            enabled: true,
            delay_text: "0".into(),
            ..Default::default()
        }
    }
}

/// Read-only text for a preserved row.
pub fn describe(l: &EventLine) -> String {
    let target = match (&l.named_target, l.target.as_str()) {
        (Some(n), NAMED_BRICK) => format!("\"{n}\""),
        _ => l.target.clone(),
    };
    let params: Vec<String> = l
        .params
        .iter()
        .map(|p| match p {
            ParamValue::Int(v) | ParamValue::List(v) => v.to_string(),
            ParamValue::Float(v) => format!("{v}"),
            ParamValue::Bool(b) => (*b as u8).to_string(),
            ParamValue::Text(t) => t.clone(),
            ParamValue::Datablock(d) => d.clone().unwrap_or_else(|| "NONE".into()),
            ParamValue::Vector(v) => format!("{} {} {}", v[0], v[1], v[2]),
            ParamValue::PaintColor(c) => c.to_string(),
        })
        .collect();
    format!(
        "{} {}ms {} -> {} {} {}",
        if l.enabled { "[x]" } else { "[ ]" },
        l.delay_ms,
        l.input,
        target,
        l.output,
        params.join(" ")
    )
    .trim_end()
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{CURRENT_BRICK_EVENT_INPUTS, CURRENT_BRICK_EVENT_OUTPUTS};
    use crate::schema::{EventTables, InputEventDef, OutputEventDef};

    pub fn catalog() -> EventCatalog {
        let t = EventTables {
            inputs: vec![
                InputEventDef {
                    class: "fxDTSBrick".into(),
                    name: "onPlayerTouch".into(),
                    targets: vec![
                        ("Self".into(), "fxDTSBrick".into()),
                        ("Player".into(), "Player".into()),
                    ],
                    source_line: 1,
                },
                InputEventDef {
                    class: "fxDTSBrick".into(),
                    name: "onActivate".into(),
                    targets: vec![
                        ("Self".into(), "fxDTSBrick".into()),
                        ("Player".into(), "Player".into()),
                    ],
                    source_line: 2,
                },
                InputEventDef {
                    class: "fxDTSBrick".into(),
                    name: "onRelay".into(),
                    targets: vec![],
                    source_line: 3,
                },
            ],
            outputs: vec![
                OutputEventDef {
                    class: "fxDTSBrick".into(),
                    name: "setColor".into(),
                    params: vec![ParamSpec::PaintColor { default: 0 }],
                    append_client: false,
                    source_line: 4,
                },
                OutputEventDef {
                    class: "fxDTSBrick".into(),
                    name: "setColliding".into(),
                    params: vec![ParamSpec::Bool],
                    append_client: false,
                    source_line: 5,
                },
                OutputEventDef {
                    class: "fxDTSBrick".into(),
                    name: "disappear".into(),
                    params: vec![ParamSpec::Int {
                        min: -1,
                        max: 300,
                        default: 5,
                    }],
                    append_client: true,
                    source_line: 6,
                },
                OutputEventDef {
                    class: "Player".into(),
                    name: "Kill".into(),
                    params: vec![],
                    append_client: true,
                    source_line: 7,
                },
            ],
        };
        EventCatalog::from_tables(&t, CURRENT_BRICK_EVENT_INPUTS, CURRENT_BRICK_EVENT_OUTPUTS)
    }

    #[test]
    fn cascade_and_trailing_row() {
        let c = catalog();
        let mut m = EventsModel::open(7, vec![], vec!["door".into(), "Alpha".into()], true, &c);
        assert_eq!(m.rows.len(), 1);
        assert_eq!(
            EventsModel::input_choices(&c),
            vec!["onActivate", "onPlayerTouch"]
        );
        m.set_input(0, Some("onActivate".into()));
        assert_eq!(m.rows.len(), 2, "choosing an input appends a blank row");
        assert_eq!(
            m.target_choices(&c, "onActivate"),
            vec!["Self", "Player", NAMED_BRICK]
        );
        m.set_target(0, "Self".into(), &c);
        assert_eq!(
            EventsModel::output_choices(&c, "fxDTSBrick"),
            vec!["setColliding", "setColor"]
        );
        assert!(
            EventsModel::output_choices(&c, "Player").is_empty(),
            "unsupported outputs hidden"
        );
        m.set_output(0, "setColor".into(), &c);
        assert_eq!(
            m.param_specs(0, &c),
            vec![ParamSpec::PaintColor { default: 0 }]
        );
        m.set_param(0, 0, ParamValue::PaintColor(5), &c);
        m.set_delay_text(0, "99999".into());
        m.accept_delay(0);
        // Changing to a target of the same class keeps output/params.
        m.set_target(0, NAMED_BRICK.into(), &c);
        assert_eq!(m.named_choices(), vec!["Alpha", "door"]);
        assert!(
            m.to_send().is_empty(),
            "named target without a name is incomplete"
        );
        m.set_named(0, "door".into());
        let sent = m.to_send();
        assert_eq!(sent.len(), 1);
        let EventRow::Editable(l) = &sent[0] else {
            panic!()
        };
        assert_eq!(l.delay_ms, 30_000);
        assert_eq!(l.params, vec![ParamValue::PaintColor(5)]);
        assert!(m.uses_named("door"));
        // Different class clears the output.
        m.set_target(0, "Player".into(), &c);
        assert!(matches!(&m.rows[0], RowState::Editable(e) if e.output.is_none()));
        // "-" on a non-last row deletes it.
        m.set_input(0, None);
        assert_eq!(m.rows.len(), 1);
        m.set_input(0, None);
        assert_eq!(m.rows.len(), 1, "the trailing row stays");
    }

    #[test]
    fn unsupported_rows_are_preserved_read_only() {
        let c = catalog();
        let relay = EventLine {
            enabled: true,
            delay_ms: 100,
            input: "onRelay".into(),
            target: "Self".into(),
            named_target: None,
            output: "disappear".into(),
            params: vec![ParamValue::Int(5)],
        };
        let m = EventsModel::open(1, vec![EventRow::Editable(relay)], vec![], true, &c);
        let RowState::Preserved { text, .. } = &m.rows[0] else {
            panic!("expected preserved")
        };
        assert_eq!(text, "[x] 100ms onRelay -> Self disappear 5");
        let sent = m.to_send();
        assert!(matches!(sent[0], EventRow::Preserved { .. }));
    }

    #[test]
    fn delay_and_param_clamps() {
        assert_eq!(clamp_delay("-5"), 0);
        assert_eq!(clamp_delay("250ms"), 250);
        assert_eq!(clamp_delay(""), 0);
        let f = ParamSpec::Float {
            min: 0.2,
            max: 2.0,
            step: 0.1,
            default: 1.0,
        };
        assert_eq!(
            clamp_param(&f, ParamValue::Float(5.0)),
            ParamValue::Float(2.0)
        );
        let s = ParamSpec::String {
            max_length: 3,
            width: 10,
        };
        assert_eq!(
            clamp_param(&s, ParamValue::Text("abcdef".into())),
            ParamValue::Text("abc".into())
        );
    }
}
