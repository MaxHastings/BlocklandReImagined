//! Control profiles: bindings and mouse settings that follow the player type.
//!
//! Blockland's own bindings are the default profile and stay in
//! `Core::binds`, saved as before. Every other profile (declared by an
//! Add-On, see `bri_content::controls`) keeps a [`BindMap`] of its own here.
//! The active profile follows the local player's body; a layer for what the
//! player rides sits on top of it and is gone again on exit. Inputs a profile
//! leaves unbound fall through to the default profile when it inherits.

use crate::api::{BindEntry, BindInput};
use crate::binds::BindMap;
use crate::input::{Key, Modifiers};
use crate::prefs::Prefs;
use crate::schema::RemapEntry;
use bri_content::controls::{DEFAULT_PROFILE, LayerDef, LayerWhen, ProfileDef};
use std::collections::BTreeMap;

/// What the local player is riding, for matching layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RideContext {
    pub driver: bool,
    /// Driving a vehicle the mouse steers.
    pub mouse_driver: bool,
}

impl RideContext {
    fn matches(self, when: LayerWhen) -> bool {
        match when {
            LayerWhen::Vehicle => true,
            LayerWhen::Driver => self.driver,
            LayerWhen::MouseDriver => self.mouse_driver,
            LayerWhen::Passenger => !self.driver,
        }
    }
}

/// An action bound in a non-default profile, with its default input.
#[derive(Debug, Clone)]
pub struct ProfileAction {
    pub entry: RemapEntry,
    pub default: Option<BindInput>,
}

/// The mouse settings in force.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseSettings {
    pub sensitivity: f32,
    /// Invert on foot.
    pub invert: bool,
    /// Invert while driving a mouse-steered vehicle.
    pub vehicle_invert: bool,
}

/// Blockland actions a profile's bodies have no use for, so its Controls
/// list leaves them out. Each follows a declaration the game already makes:
/// the `building` HUD claim, and the body's own `can_jet`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hides {
    /// Building, tools and inventory, and the free-camera drops.
    pub building: bool,
    /// Jet, and the Jump/Jet Combo option.
    pub jet: bool,
}

impl Hides {
    pub fn command(self, command: &str) -> bool {
        (self.building && building_command(&command.to_ascii_lowercase()))
            || (self.jet && command.eq_ignore_ascii_case("jet"))
    }
}

/// A stock command of the building interface (brick, paint and tool
/// picking, shifting the ghost brick, the selector) or of the free camera
/// builders drop around; `c` is lower-case.
pub fn building_command(c: &str) -> bool {
    matches!(
        c,
        "openbsd"
            | "usebricks"
            | "usetools"
            | "usespraycan"
            | "invup"
            | "invdown"
            | "invleft"
            | "invright"
            | "usefirstslot"
            | "usesecondslot"
            | "usethirdslot"
            | "usefourthslot"
            | "usefifthslot"
            | "usesixthslot"
            | "useseventhslot"
            | "useeighthslot"
            | "useninthslot"
            | "usetenthslot"
            | "dropcameraatplayer"
            | "dropplayeratcamera"
    ) || c.starts_with("shiftbrick")
        || c.starts_with("supershiftbrick")
}

#[derive(Debug, Clone)]
pub struct ProfileState {
    defs: Vec<ProfileDef>,
    /// What each profile hides of Blockland's actions.
    hides: BTreeMap<String, Hides>,
    /// Blockland's remappable actions, for the shared part of a list.
    stock: Vec<RemapEntry>,
    /// Bindings of every non-default profile, including ones no running
    /// Add-On declares now, so a player's choices wait for it to return.
    maps: BTreeMap<String, BindMap>,
    /// Each profile's declared default inputs.
    defaults: BTreeMap<String, Vec<(String, BindInput)>>,
    /// Each profile's own actions.
    own: BTreeMap<String, Vec<RemapEntry>>,
    /// Each profile's Controls list: its own actions, then the Blockland
    /// ones that still apply, with their commands and the own count.
    remaps: BTreeMap<String, (Vec<RemapEntry>, Vec<String>, usize)>,
    active: String,
    ride: Option<RideContext>,
    /// The tab Options → Controls shows.
    pub viewed: String,
}

impl Default for ProfileState {
    fn default() -> Self {
        Self {
            defs: Vec::new(),
            hides: BTreeMap::new(),
            stock: Vec::new(),
            maps: BTreeMap::new(),
            defaults: BTreeMap::new(),
            own: BTreeMap::new(),
            remaps: BTreeMap::new(),
            active: DEFAULT_PROFILE.into(),
            ride: None,
            viewed: DEFAULT_PROFILE.into(),
        }
    }
}

pub fn pref_key(profile: &str, layer: Option<&str>, what: &str) -> String {
    match layer {
        Some(l) => format!("$Pref::Profile::{profile}::Layer::{l}::{what}"),
        None => format!("$Pref::Profile::{profile}::{what}"),
    }
}
/// Where a profile's player-chosen mouse sensitivity is kept.
pub fn sensitivity_pref(profile: &str) -> String {
    pref_key(profile, None, "MouseSensitivity")
}
pub fn invert_pref(profile: &str) -> String {
    pref_key(profile, None, "MouseInvert")
}
pub fn vehicle_invert_pref(profile: &str) -> String {
    pref_key(profile, None, "VehicleMouseInvert")
}

impl ProfileState {
    /// Load the player's saved bindings for every profile.
    pub fn load(&mut self, saved: &BTreeMap<String, Vec<BindEntry>>) {
        for (id, entries) in saved {
            if id != DEFAULT_PROFILE {
                self.maps.insert(
                    id.clone(),
                    BindMap {
                        entries: entries.clone(),
                    },
                );
            }
        }
    }
    /// Every profile's bindings, for saving.
    pub fn entries(&self) -> BTreeMap<String, Vec<BindEntry>> {
        self.maps
            .iter()
            .map(|(k, m)| (k.clone(), m.entries.clone()))
            .collect()
    }
    pub fn maps(&self) -> &BTreeMap<String, BindMap> {
        &self.maps
    }
    pub fn restore(&mut self, maps: BTreeMap<String, BindMap>) {
        self.maps = maps;
    }

    /// The profiles the running Add-Ons declare and each one's actions. A
    /// declared default input binds while the action and the input are free
    /// in that profile alone.
    pub fn declare(
        &mut self,
        defs: Vec<ProfileDef>,
        actions: BTreeMap<String, Vec<ProfileAction>>,
        stock: Vec<RemapEntry>,
    ) {
        for d in &defs {
            self.maps.entry(d.id.clone()).or_default();
        }
        for (id, list) in &actions {
            let map = self.maps.entry(id.clone()).or_default();
            for a in list {
                if let Some(input) = a.default
                    && map.binding_of(&a.entry.command).is_none()
                    && map.command_for(&input).is_none()
                {
                    map.bind(input, &a.entry.command);
                }
            }
        }
        self.defaults = actions
            .iter()
            .map(|(id, list)| {
                (
                    id.clone(),
                    list.iter()
                        .filter_map(|a| Some((a.entry.command.clone(), a.default?)))
                        .collect(),
                )
            })
            .collect();
        self.own = actions
            .iter()
            .map(|(id, list)| (id.clone(), list.iter().map(|a| a.entry.clone()).collect()))
            .collect();
        self.stock = stock;
        self.defs = defs;
        self.rebuild();
        if !self.is_declared(&self.active) {
            self.active = DEFAULT_PROFILE.into();
        }
        if !self.is_declared(&self.viewed) {
            self.viewed = DEFAULT_PROFILE.into();
        }
    }
    /// Put a profile's bindings back to its Add-On's suggested keys,
    /// dropping every change the player made (overrides of Blockland's
    /// actions included).
    pub fn reset(&mut self, profile: &str) {
        let Some(map) = self.maps.get_mut(profile) else {
            return;
        };
        map.entries.clear();
        for (command, input) in self.defaults.get(profile).into_iter().flatten() {
            if map.command_for(input).is_none() {
                map.bind(*input, command);
            }
        }
    }
    /// What each profile leaves out of Blockland's list.
    pub fn set_hides(&mut self, hides: BTreeMap<String, Hides>) {
        if self.hides != hides {
            self.hides = hides;
            self.rebuild();
        }
    }
    pub fn hides(&self, profile: &str) -> Hides {
        self.hides.get(profile).copied().unwrap_or_default()
    }
    /// Each profile's list: its own actions, then Blockland's that its
    /// bodies can still use (when it inherits), headed "Blockland".
    fn rebuild(&mut self) {
        let mut remaps = BTreeMap::new();
        for d in &self.defs {
            let own = self.own.get(&d.id).cloned().unwrap_or_default();
            let n = own.len();
            let mut entries = own;
            if d.inherit {
                let hides = self.hides(&d.id);
                let mut division: Option<&str> = None;
                let mut shown_division: Option<String> = None;
                for e in &self.stock {
                    if let Some(div) = &e.division {
                        division = Some(div);
                    }
                    if hides.command(&e.command)
                        || entries
                            .iter()
                            .any(|o| o.command.eq_ignore_ascii_case(&e.command))
                    {
                        continue;
                    }
                    let div = division.map(|d| format!("Blockland: {d}"));
                    entries.push(RemapEntry {
                        division: if div != shown_division {
                            div.clone()
                        } else {
                            None
                        },
                        ..e.clone()
                    });
                    shown_division = div;
                }
            }
            let commands = entries.iter().map(|e| e.command.clone()).collect();
            remaps.insert(d.id.clone(), (entries, commands, n));
        }
        self.remaps = remaps;
    }
    fn is_declared(&self, id: &str) -> bool {
        id == DEFAULT_PROFILE || self.defs.iter().any(|d| d.id == id)
    }
    pub fn defs(&self) -> &[ProfileDef] {
        &self.defs
    }
    pub fn def(&self, id: &str) -> Option<&ProfileDef> {
        self.defs.iter().find(|d| d.id == id)
    }
    /// Tabs for Options → Controls: the default profile, then the declared.
    pub fn tabs(&self) -> Vec<(String, String)> {
        std::iter::once((DEFAULT_PROFILE.to_string(), "Blockland".to_string()))
            .chain(self.defs.iter().map(|d| (d.id.clone(), d.name.clone())))
            .collect()
    }

    /// The active profile follows the local player's body.
    pub fn select_for(&mut self, player_type: &str) {
        self.active = self
            .defs
            .iter()
            .find(|d| d.player_types.iter().any(|t| t == player_type))
            .map_or_else(|| DEFAULT_PROFILE.to_string(), |d| d.id.clone());
    }
    pub fn set_ride(&mut self, ride: Option<RideContext>) {
        self.ride = ride;
    }
    pub fn active(&self) -> &str {
        &self.active
    }
    /// The layer on top of the active profile, if the rider matches one.
    pub fn layer(&self) -> Option<&LayerDef> {
        let ride = self.ride?;
        self.def(&self.active)?
            .layers
            .iter()
            .find(|l| ride.matches(l.when))
    }

    /// Command for an input in gameplay: the active profile's own bindings
    /// first, then the default profile's when the profile inherits.
    pub fn command_for<'a>(&'a self, base: &'a BindMap, input: &BindInput) -> Option<&'a str> {
        self.own(|m| m.command_for(input))
            .or_else(|| self.inherits().then(|| base.command_for(input)).flatten())
    }
    pub fn command_for_key<'a>(
        &'a self,
        base: &'a BindMap,
        key: Key,
        mods: Modifiers,
    ) -> Option<&'a str> {
        self.own(|m| m.command_for_key(key, mods)).or_else(|| {
            self.inherits()
                .then(|| base.command_for_key(key, mods))
                .flatten()
        })
    }
    fn inherits(&self) -> bool {
        self.def(&self.active).is_none_or(|d| d.inherit)
    }
    fn own<'a, T>(&'a self, f: impl FnOnce(&'a BindMap) -> Option<T>) -> Option<T> {
        if self.active == DEFAULT_PROFILE {
            return None;
        }
        self.maps.get(&self.active).and_then(f)
    }

    /// The map the Controls tab being viewed edits (none for the default
    /// profile, which edits `Core::binds`).
    pub fn viewed_map(&self) -> Option<&BindMap> {
        self.maps.get(&self.viewed)
    }
    pub fn viewed_map_mut(&mut self) -> Option<&mut BindMap> {
        self.maps.get_mut(&self.viewed)
    }
    /// The Controls list of the tab being viewed, when it is a profile's.
    pub fn viewed_remap(&self) -> Option<(&[RemapEntry], &[String])> {
        self.remaps
            .get(&self.viewed)
            .map(|(e, c, _)| (e.as_slice(), c.as_slice()))
    }
    /// How many of the viewed list's first rows are the profile's own; the
    /// rest are Blockland's, changed here only as this profile's override.
    pub fn viewed_own_count(&self) -> Option<usize> {
        self.remaps.get(&self.viewed).map(|r| r.2)
    }

    /// Mouse settings: the layer's, else the profile's, else the player's
    /// global Blockland settings. A player's own choice for a layer or
    /// profile beats the Add-On's declared default.
    pub fn mouse(&self, prefs: &Prefs, global: MouseSettings) -> MouseSettings {
        let Some(def) = self
            .def(&self.active)
            .filter(|_| self.active != DEFAULT_PROFILE)
        else {
            return global;
        };
        let id = &def.id;
        let user_f = |key: String| prefs.get(&key).and_then(|v| v.trim().parse::<f32>().ok());
        let user_b = |key: String| prefs.get(&key).map(|_| prefs.bool_or(&key, false));
        let mut out = MouseSettings {
            sensitivity: user_f(sensitivity_pref(id))
                .or(def.mouse_sensitivity)
                .unwrap_or(global.sensitivity),
            invert: user_b(invert_pref(id))
                .or(def.invert_mouse)
                .unwrap_or(global.invert),
            vehicle_invert: user_b(vehicle_invert_pref(id))
                .or(def.vehicle_invert_mouse)
                .unwrap_or(global.vehicle_invert),
        };
        if let Some(layer) = self.layer() {
            out.sensitivity = user_f(pref_key(id, Some(&layer.id), "MouseSensitivity"))
                .or(layer.mouse_sensitivity)
                .unwrap_or(out.sensitivity);
            out.vehicle_invert = user_b(pref_key(id, Some(&layer.id), "MouseInvert"))
                .or(layer.invert_mouse)
                .unwrap_or(out.vehicle_invert);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binds::RemapOutcome;
    use crate::input::Chord;

    fn key(k: Key) -> BindInput {
        BindInput::Key(Chord::plain(k))
    }
    fn def() -> ProfileDef {
        ProfileDef {
            id: "aa".into(),
            name: "Arena Assault".into(),
            inherit: true,
            player_types: vec!["soldier".into()],
            layers: vec![LayerDef {
                id: "aircraft".into(),
                name: None,
                when: LayerWhen::MouseDriver,
                invert_mouse: Some(true),
                mouse_sensitivity: Some(0.5),
            }],
            invert_mouse: Some(false),
            vehicle_invert_mouse: None,
            mouse_sensitivity: Some(1.0),
        }
    }
    fn action(command: &str, default: Option<BindInput>) -> ProfileAction {
        ProfileAction {
            entry: RemapEntry {
                division: None,
                name: command.into(),
                command: command.into(),
            },
            default,
        }
    }
    /// Blockland's list as the UI pack gives it, in part.
    fn stock_list() -> Vec<RemapEntry> {
        let e = |division: Option<&str>, command: &str| RemapEntry {
            division: division.map(String::from),
            name: command.into(),
            command: command.into(),
        };
        vec![
            e(Some("Movement"), "moveforward"),
            e(None, "jet"),
            e(Some("Tools / Inventory"), "useBricks"),
            e(None, "invUp"),
            e(Some("Building"), "openBSD"),
            e(Some("Communication"), "GlobalChat"),
        ]
    }
    fn state() -> ProfileState {
        let mut s = ProfileState::default();
        s.declare(
            vec![def()],
            BTreeMap::from([(
                "aa".to_string(),
                vec![
                    action("sprint", Some(key(Key::LShift))),
                    action("firemode", Some(key(Key::Letter('v')))),
                ],
            )]),
            stock_list(),
        );
        s
    }
    fn global() -> MouseSettings {
        MouseSettings {
            sensitivity: 0.75,
            invert: true,
            vehicle_invert: true,
        }
    }

    #[test]
    fn the_profile_follows_the_player_type() {
        let mut s = state();
        assert_eq!(s.active(), DEFAULT_PROFILE);
        s.select_for("soldier");
        assert_eq!(s.active(), "aa");
        s.select_for("blockhead");
        assert_eq!(s.active(), DEFAULT_PROFILE);
    }

    #[test]
    fn a_profile_binds_win_and_the_rest_fall_through() {
        let mut s = state();
        let mut base = BindMap::default();
        base.bind(key(Key::LShift), "crouch");
        base.bind(key(Key::Letter('w')), "moveforward");
        let at = |s: &ProfileState, k| {
            s.command_for_key(&base, k, Modifiers::NONE)
                .map(str::to_string)
        };
        assert_eq!(at(&s, Key::LShift).as_deref(), Some("crouch"));
        s.select_for("soldier");
        assert_eq!(at(&s, Key::LShift).as_deref(), Some("sprint"));
        assert_eq!(at(&s, Key::Letter('v')).as_deref(), Some("firemode"));
        assert_eq!(at(&s, Key::Letter('w')).as_deref(), Some("moveforward"));
        s.select_for("blockhead");
        assert_eq!(at(&s, Key::Letter('v')), None);
    }

    #[test]
    fn a_profile_that_does_not_inherit_hides_the_defaults() {
        let mut s = ProfileState::default();
        let mut d = def();
        d.inherit = false;
        s.declare(vec![d], BTreeMap::new(), Vec::new());
        s.select_for("soldier");
        let mut base = BindMap::default();
        base.bind(key(Key::Letter('w')), "moveforward");
        assert_eq!(
            s.command_for_key(&base, Key::Letter('w'), Modifiers::NONE),
            None
        );
    }

    #[test]
    fn conflicts_are_only_within_a_profile() {
        let mut s = state();
        assert!(
            s.viewed_map().is_none(),
            "the default tab edits Core::binds"
        );
        s.viewed = "aa".into();
        let remappable = vec!["sprint".to_string(), "firemode".to_string()];
        let map = s.viewed_map_mut().unwrap();
        // Shift is Sprint in this profile; the default profile's crouch on
        // Shift is no conflict, and a free key binds.
        assert!(matches!(
            map.remap("firemode", key(Key::LShift), &remappable),
            RemapOutcome::Conflict { .. }
        ));
        assert!(matches!(
            map.remap("firemode", key(Key::Letter('w')), &remappable),
            RemapOutcome::Bound
        ));
    }

    #[test]
    fn a_vehicle_layer_stacks_and_is_removed_on_exit() {
        let mut s = state();
        let prefs = Prefs::default();
        s.select_for("soldier");
        assert_eq!(s.mouse(&prefs, global()).sensitivity, 1.0);
        assert!(!s.mouse(&prefs, global()).invert);
        s.set_ride(Some(RideContext {
            driver: true,
            mouse_driver: true,
        }));
        let m = s.mouse(&prefs, global());
        assert_eq!((m.sensitivity, m.vehicle_invert), (0.5, true));
        assert_eq!(s.layer().map(|l| l.id.as_str()), Some("aircraft"));
        // A ground vehicle does not match a mouse-driver layer.
        s.set_ride(Some(RideContext {
            driver: true,
            mouse_driver: false,
        }));
        assert!(s.layer().is_none());
        s.set_ride(None);
        assert!(s.layer().is_none());
        assert_eq!(s.mouse(&prefs, global()).sensitivity, 1.0);
    }

    #[test]
    fn the_players_own_mouse_choice_beats_the_declared_default() {
        let mut s = state();
        s.select_for("soldier");
        let mut prefs = Prefs::default();
        prefs.set(&sensitivity_pref("aa"), "0.3");
        prefs.set(&invert_pref("aa"), "1");
        let m = s.mouse(&prefs, global());
        assert_eq!((m.sensitivity, m.invert), (0.3, true));
        // The default profile ignores them: the player's global settings.
        s.select_for("blockhead");
        assert_eq!(s.mouse(&prefs, global()), global());
    }

    #[test]
    fn overrides_persist_per_profile_and_survive_a_missing_addon() {
        let mut s = state();
        s.viewed = "aa".into();
        s.viewed_map_mut()
            .unwrap()
            .force_remap("sprint", key(Key::Letter('o')));
        let saved = s.entries();
        // A later run: the Add-On is not loaded, then returns.
        let mut later = ProfileState::default();
        later.load(&saved);
        assert_eq!(later.entries(), saved, "kept while undeclared");
        later.declare(
            vec![def()],
            BTreeMap::from([(
                "aa".to_string(),
                vec![action("sprint", Some(key(Key::LShift)))],
            )]),
            Vec::new(),
        );
        later.viewed = "aa".into();
        let map = later.viewed_map().unwrap();
        assert_eq!(map.binding_of("sprint"), Some(key(Key::Letter('o'))));
    }

    #[test]
    fn a_list_is_its_own_actions_then_blockland_ones_still_in_use() {
        let mut s = state();
        s.viewed = "aa".into();
        let names = |s: &ProfileState| -> Vec<String> {
            s.viewed_remap()
                .unwrap()
                .1
                .iter()
                .map(|c| c.to_string())
                .collect()
        };
        assert_eq!(s.viewed_own_count(), Some(2));
        assert_eq!(
            names(&s),
            [
                "sprint",
                "firemode",
                "moveforward",
                "jet",
                "useBricks",
                "invUp",
                "openBSD",
                "GlobalChat"
            ]
        );
        // The first shared row of each Blockland group heads it.
        let entries = s.viewed_remap().unwrap().0;
        assert_eq!(entries[2].division.as_deref(), Some("Blockland: Movement"));
        assert_eq!(entries[3].division, None);
        // A game with no building and no jet leaves those out.
        s.set_hides(BTreeMap::from([(
            "aa".to_string(),
            Hides {
                building: true,
                jet: true,
            },
        )]));
        assert_eq!(
            names(&s),
            ["sprint", "firemode", "moveforward", "GlobalChat"]
        );
        let entries = s.viewed_remap().unwrap().0;
        assert_eq!(
            entries[3].division.as_deref(),
            Some("Blockland: Communication")
        );
    }

    #[test]
    fn reset_restores_the_suggested_keys_and_drops_overrides() {
        let mut s = state();
        s.viewed = "aa".into();
        let map = s.viewed_map_mut().unwrap();
        map.unbind_command("sprint");
        map.bind(key(Key::Letter('q')), "firemode");
        map.bind(key(Key::Letter('o')), "moveforward");
        s.reset("aa");
        let map = s.viewed_map().unwrap();
        assert_eq!(map.binding_of("sprint"), Some(key(Key::LShift)));
        assert_eq!(map.binding_of("firemode"), Some(key(Key::Letter('v'))));
        assert_eq!(map.binding_of("moveforward"), None, "override dropped");
    }
}
