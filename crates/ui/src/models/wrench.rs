//! Wrench dialogs state (`clientCmdSetWrenchData`, `wrenchDlg::send`,
//! c:15530–15900). Dialog values persist between openings and "Copy" locks
//! keep a field's value when the next brick's data arrives.

use crate::api::{EventCatalog, EventRow, WrenchData, WrenchVariant};
use crate::models::events::EventsModel;
use bri_console::Clamp;
use std::collections::{BTreeMap, BTreeSet};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum WrenchField {
    Name,
    Light,
    Emitter,
    EmitterDir,
    Item,
    ItemPos,
    ItemDir,
    ItemRespawn,
    RayCasting,
    Colliding,
    Rendering,
    Sound,
    Vehicle,
    RecolorVehicle,
}

impl WrenchField {
    /// Fields shown by each variant, in dialog order.
    pub fn for_variant(v: WrenchVariant) -> &'static [WrenchField] {
        use WrenchField::*;
        match v {
            WrenchVariant::Normal => &[
                Name,
                Light,
                Emitter,
                EmitterDir,
                Item,
                ItemPos,
                ItemDir,
                ItemRespawn,
                RayCasting,
                Colliding,
                Rendering,
            ],
            WrenchVariant::Sound => &[Name, Sound],
            WrenchVariant::VehicleSpawn => &[
                Name,
                Vehicle,
                RecolorVehicle,
                RayCasting,
                Colliding,
                Rendering,
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenWrench {
    pub brick: u64,
    pub variant: WrenchVariant,
    /// Title heading ("Wrench - <owner>").
    pub owner: String,
    pub admin_override: bool,
    pub events_allowed: bool,
    /// A fill wrench (a duplicator's) on this many bricks: the ticked
    /// settings go on every one of them.
    pub fill: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WrenchState {
    /// Dialog values per variant (they persist like the Torque controls).
    pub values: BTreeMap<u8, WrenchData>,
    pub locks: BTreeSet<(u8, WrenchField)>,
    pub open: Option<OpenWrench>,
    pub events: Option<EventsModel>,
    /// The builder (owner id) of the brick whose events are open: its rows
    /// run in their mini-game, so its teams are the ones rows name.
    pub events_builder: Option<u64>,
    /// That builder's brick group name, when it is not the local player.
    pub events_builder_name: Option<String>,
    /// Copy checkbox survives dialog closure. Only editable rows may cross bricks.
    pub events_copy: Option<EventsModel>,
    /// The fill wrench's ticked settings: its Copy boxes say which settings
    /// to put on every brick.
    pub fill_ticks: BTreeSet<WrenchField>,
}

fn vkey(v: WrenchVariant) -> u8 {
    match v {
        WrenchVariant::Normal => 0,
        WrenchVariant::Sound => 1,
        WrenchVariant::VehicleSpawn => 2,
    }
}

impl WrenchState {
    pub fn values(&self, v: WrenchVariant) -> WrenchData {
        self.values
            .get(&vkey(v))
            .cloned()
            .unwrap_or_else(|| WrenchData {
                emitter_dir: 0,
                item_pos: 0,
                item_dir: 2,
                raycasting: true,
                colliding: true,
                rendering: true,
                ..Default::default()
            })
    }
    pub fn set_values(&mut self, v: WrenchVariant, d: WrenchData) {
        self.values.insert(vkey(v), d);
    }
    pub fn locked(&self, v: WrenchVariant, f: WrenchField) -> bool {
        self.locks.contains(&(vkey(v), f))
    }
    pub fn set_lock(&mut self, v: WrenchVariant, f: WrenchField, on: bool) {
        if on {
            self.locks.insert((vkey(v), f));
        } else {
            self.locks.remove(&(vkey(v), f));
        }
    }

    /// Server opened the wrench on a brick: fill unlocked fields.
    pub fn open(
        &mut self,
        brick: u64,
        variant: WrenchVariant,
        owner: String,
        data: WrenchData,
        admin_override: bool,
        events_allowed: bool,
    ) {
        let mut cur = self.values(variant);
        // Region size belongs to the inspected brick, never a remembered Copy lock.
        cur.rule_region = data.rule_region;
        cur.rule_region_default = data.rule_region_default;
        cur.region_inputs = data.region_inputs;
        let l = |f| self.locked(variant, f);
        use WrenchField::*;
        if !l(Name) {
            cur.name = data.name.trim().to_string();
        }
        if !l(Light) {
            cur.light = data.light;
        }
        if !l(Emitter) {
            cur.emitter = data.emitter;
        }
        if !l(EmitterDir) {
            cur.emitter_dir = data.emitter_dir.min(5);
        }
        if !l(Item) {
            cur.item = data.item;
        }
        if !l(ItemPos) {
            cur.item_pos = data.item_pos.min(5);
        }
        if !l(ItemDir) {
            cur.item_dir = data.item_dir.clamp(2, 5);
        }
        if !l(ItemRespawn) {
            cur.item_respawn_ms = data.item_respawn_ms;
        }
        if !l(RayCasting) {
            cur.raycasting = data.raycasting;
        }
        if !l(Colliding) {
            cur.colliding = data.colliding;
        }
        if !l(Rendering) {
            cur.rendering = data.rendering;
        }
        if !l(Sound) {
            cur.sound = data.sound;
        }
        if !l(Vehicle) {
            cur.vehicle = data.vehicle;
        }
        if !l(RecolorVehicle) {
            cur.recolor_vehicle = data.recolor_vehicle;
        }
        self.set_values(variant, cur);
        self.open = Some(OpenWrench {
            brick,
            variant,
            owner,
            admin_override,
            events_allowed,
            fill: None,
        });
    }

    /// A duplicator opened the fill wrench on `bricks` bricks: the dialog
    /// keeps the values last set, nothing ticked.
    pub fn open_fill(&mut self, bricks: u32) {
        self.fill_ticks.clear();
        self.open = Some(OpenWrench {
            brick: 0,
            variant: WrenchVariant::Normal,
            owner: String::new(),
            admin_override: false,
            events_allowed: false,
            fill: Some(bricks),
        });
    }

    /// The fill wrench is open.
    pub fn filling(&self) -> bool {
        self.open.as_ref().is_some_and(|o| o.fill.is_some())
    }

    /// Whether `field`'s Copy box is ticked: on a fill wrench, whether the
    /// setting goes on every brick.
    pub fn ticked(&self, v: WrenchVariant, f: WrenchField) -> bool {
        if self.filling() {
            self.fill_ticks.contains(&f)
        } else {
            self.locked(v, f)
        }
    }
    pub fn set_ticked(&mut self, v: WrenchVariant, f: WrenchField, on: bool) {
        if !self.filling() {
            self.set_lock(v, f, on);
        } else if on {
            self.fill_ticks.insert(f);
        } else {
            self.fill_ticks.remove(&f);
        }
    }

    pub fn close(&mut self) {
        self.open = None;
        self.events = None;
    }

    pub fn open_events(
        &mut self,
        brick: u64,
        rows: Vec<EventRow>,
        named_targets: Vec<String>,
        allow_named: bool,
        catalog: &EventCatalog,
    ) {
        self.events_builder = None;
        self.events_builder_name = None;
        let mut incoming = EventsModel::open(brick, rows, named_targets, allow_named, catalog);
        if let Some(copy) = &self.events_copy {
            use crate::models::events::{EditRow, RowState};
            // Only the destination host owns its opaque preservation tokens.
            incoming
                .rows
                .retain(|r| matches!(r, RowState::Preserved { .. }));
            incoming.rows.extend(copy.rows.iter().filter_map(|row| {
                let RowState::Editable(draft) = row else {
                    return None;
                };
                draft.input.as_ref()?;
                let mut draft = draft.clone();
                draft.copied_draft = true;
                Some(RowState::Editable(draft))
            }));
            incoming.rows.push(RowState::Editable(EditRow::blank()));
        }
        self.events = Some(incoming);
    }
}

/// `SetWrenchData` name rule: max 32 characters, trimmed.
pub fn clean_name(s: &str) -> String {
    s.trim().chars().take(32).collect()
}

/// Original wrench text is seconds: mFloor(value), then server *1000 and
/// clamp to the stock 1000..300000 ms range. The typed API stores milliseconds.
pub fn respawn_ms(text: &str) -> u32 {
    let seconds = text.trim().parse::<f64>().unwrap_or(0.0);
    let seconds = if seconds.is_nan() { 0.0 } else { seconds };
    (seconds.floor().clamped(1.0, 300.0) as u32) * 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrench_respawn_seconds_match_original_floor_and_server_bounds() {
        for (text, expected) in [
            ("4", 4000),
            ("5.9", 5000),
            (" 12 ", 12000),
            ("0", 1000),
            ("-3", 1000),
            ("301", 300000),
            ("bad", 1000),
            ("NaN", 1000),
            ("inf", 300000),
        ] {
            assert_eq!(respawn_ms(text), expected, "{text}");
        }
    }

    #[test]
    fn event_copy_survives_close_without_copying_opaque_tokens() {
        use crate::api::{EventInputInfo, EventLine, EventOutputInfo};
        let catalog = EventCatalog {
            target_notes: Default::default(),
            inputs: vec![EventInputInfo {
                name: "onActivate".into(),
                targets: vec![("Self".into(), "fxDTSBrick".into())],
                supported: true,
            }],
            outputs: vec![EventOutputInfo {
                provider: "Blockland".into(),
                class: "fxDTSBrick".into(),
                name: "setRendering".into(),
                params: vec![],
                supported: true,
            }],
        };
        let line = EventRow::Editable(EventLine {
            conditions: vec![],
            enabled: true,
            delay_ms: 10,
            input: "onActivate".into(),
            target: "Self".into(),
            named_target: None,
            output: "setRendering".into(),
            params: vec![],
        });
        let opaque = |token: &str| EventRow::Preserved {
            enabled: false,
            text: "legacy".into(),
            token: token.into(),
        };
        let mut state = WrenchState::default();
        state.open_events(
            1,
            vec![line.clone(), opaque("source:1")],
            vec![],
            true,
            &catalog,
        );
        state.events_copy = state.events.clone();
        state.close();
        assert!(state.events.is_none());
        assert!(state.events_copy.is_some());
        state.open_events(2, vec![opaque("destination:2")], vec![], true, &catalog);
        let rows = state.events.as_ref().unwrap().to_send();
        assert_eq!(rows, vec![opaque("destination:2"), line]);
        assert_eq!(state.events.as_ref().unwrap().brick, 2);
    }

    #[test]
    fn copy_locks_keep_values() {
        let mut w = WrenchState::default();
        let a = WrenchData {
            name: "door".into(),
            rule_region: Some([8.0, 5.0, 8.0]),
            rule_region_default: Some([2.0, 4.0, 2.0]),
            region_inputs: true,
            light: Some("red".into()),
            rendering: true,
            ..Default::default()
        };
        w.open(1, WrenchVariant::Normal, "Blockhead".into(), a, false, true);
        w.set_lock(WrenchVariant::Normal, WrenchField::Light, true);
        let b = WrenchData {
            name: "other".into(),
            light: None,
            rendering: false,
            ..Default::default()
        };
        w.open(2, WrenchVariant::Normal, "Blockhead".into(), b, false, true);
        let v = w.values(WrenchVariant::Normal);
        assert_eq!(v.name, "other");
        assert_eq!(
            v.light.as_deref(),
            Some("red"),
            "locked field carried to the next brick"
        );
        assert!(!v.rendering);
        assert_eq!(
            v.rule_region, None,
            "region dimensions belong to this brick"
        );
        assert_eq!(v.rule_region_default, None);
        assert!(!v.region_inputs);
        assert_eq!(w.open.as_ref().unwrap().brick, 2);
        assert_eq!(respawn_ms("5"), 5000);
        assert_eq!(clean_name(&"x".repeat(40)).len(), 32);
    }
}
