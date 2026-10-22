//! Control profiles through the whole UI: the profile follows the player
//! type, a vehicle layer stacks on it and is removed on exit, the player's
//! per-profile choices persist, and settings saved before profiles load.
use bri_content::controls::{LayerDef, LayerWhen, ProfileDef};
use bri_ui::{
    api::*,
    binds::Platform,
    geom::Rect,
    input::*,
    pack::Pack,
    profiles::{self, RideContext},
    schema::{DefaultBind, Device, RemapEntry, UiPack},
    screens::ctrl,
    ui::{Ui, UiConfig},
};
use std::{path::PathBuf, rc::Rc};

fn pack() -> Rc<Pack> {
    let mut p = UiPack::default();
    for name in ["MainMenuGui", "PlayGui", "LoadingGui", "defaultControlsGui"] {
        p.layouts.insert(
            name.into(),
            ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480)),
        );
    }
    p.data.remap = vec![RemapEntry {
        division: None,
        name: "Crouch".into(),
        command: "crouch".into(),
    }];
    p.data.default_binds = vec![DefaultBind {
        device: Device::Keyboard,
        key: "lshift".into(),
        command: "crouch".into(),
        when: vec![],
        source_line: 1,
    }];
    Rc::new(Pack::from_parts(p, PathBuf::new()))
}

fn profile() -> ProfileDef {
    ProfileDef {
        id: "aa".into(),
        name: "Arena Assault".into(),
        inherit: true,
        player_types: vec!["aa-soldier".into()],
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

fn bind(name: &str, command: &str, key: &str) -> PackageBind {
    PackageBind {
        division: "Arena Assault".into(),
        name: name.into(),
        package: "aa".into(),
        command: command.into(),
        key: Some(key.into()),
        mouse: None,
        profile: "aa".into(),
        stock: None,
        hold: true,
        alt_key: None,
        screen: None,
        layer: String::new(),
    }
}

fn ui(settings: Settings) -> Ui {
    let mut u = Ui::new(
        pack(),
        UiConfig {
            size: (1280, 960),
            scale: Some(2.0),
            platform: Platform::Windows,
        },
        settings,
    );
    u.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: true,
        single_player: true,
        admin: true,
    }));
    u.drain_actions();
    u
}

fn with_profile(settings: Settings) -> Ui {
    let mut u = ui(settings);
    u.set_profile_defs(vec![profile()]);
    u.set_package_binds(vec![
        bind("Sprint", "sprint", "lshift"),
        bind("Fire Mode", "firemode", "v"),
    ]);
    u
}

fn press(u: &mut Ui, key: Key) -> Vec<UiAction> {
    u.handle_input(InputEvent::KeyDown {
        key,
        mods: Modifiers::NONE,
        repeat: false,
    });
    u.handle_input(InputEvent::KeyUp {
        key,
        mods: Modifiers::NONE,
    });
    u.drain_actions().into_iter().map(|(_, a)| a).collect()
}

fn package(command: &str, pressed: Option<bool>) -> UiAction {
    UiAction::Game(GameAction::Package {
        package: "aa".into(),
        command: command.into(),
        pressed,
    })
}

/// The pitch the mouse sends for a move of 100 counts down.
fn look(u: &mut Ui) -> (f32, f32) {
    u.handle_input(InputEvent::MouseDelta {
        dx: 100.0,
        dy: 100.0,
    });
    u.drain_actions()
        .into_iter()
        .find_map(|(_, a)| match a {
            UiAction::Game(GameAction::Look { yaw, pitch }) => Some((yaw, pitch)),
            _ => None,
        })
        .expect("the mouse looked")
}

#[test]
fn the_profile_switches_with_the_player_type() {
    let mut u = with_profile(Settings {
        binds: Some(vec![]),
        ..Default::default()
    });
    // A Blockhead: the profile's keys do nothing, Blockland's still work.
    u.set_control_context("blockhead", None);
    assert!(press(&mut u, Key::Letter('v')).is_empty());
    // The soldier body switches the profile on: V is Fire Mode, and Shift
    // is Sprint, not the Blockland crouch bound to it.
    u.set_control_context("aa-soldier", None);
    assert_eq!(u.core.profiles.active(), "aa");
    assert_eq!(
        press(&mut u, Key::Letter('v')),
        vec![
            package("firemode", Some(true)),
            package("firemode", Some(false))
        ]
    );
    let shift = press(&mut u, Key::LShift);
    assert_eq!(
        shift,
        vec![
            package("sprint", Some(true)),
            package("sprint", Some(false))
        ]
    );
    // Back in a Blockhead's body, Shift is crouch again.
    u.set_control_context("blockhead", None);
    assert_eq!(u.core.profiles.active(), "blockland");
    assert!(
        !press(&mut u, Key::LShift)
            .iter()
            .any(|a| matches!(a, UiAction::Game(GameAction::Package { .. })))
    );
}

#[test]
fn a_vehicle_layer_changes_the_mouse_and_leaves_with_the_rider() {
    let mut u = with_profile(Settings {
        binds: Some(vec![]),
        ..Default::default()
    });
    let stock = look(&mut u);
    u.set_control_context("aa-soldier", None);
    let on_foot = look(&mut u);
    // The profile's sensitivity 1.0 against the stock 0.75.
    assert!((on_foot.0 / stock.0 - 1.0 / 0.75).abs() < 1e-3);
    u.set_control_context(
        "aa-soldier",
        Some(RideContext {
            driver: true,
            mouse_driver: true,
        }),
    );
    let flying = look(&mut u);
    assert!(
        (flying.0 / on_foot.0 - 0.5).abs() < 1e-3,
        "layer sensitivity"
    );
    assert!(u.core.mouse_settings().vehicle_invert);
    u.set_control_context("aa-soldier", None);
    assert_eq!(look(&mut u), on_foot, "the layer is gone on exit");
}

#[test]
fn per_profile_choices_persist_and_the_blockland_ones_are_untouched() {
    let mut u = with_profile(Settings {
        binds: Some(vec![]),
        ..Default::default()
    });
    let blockland = u.core.binds.clone();
    u.core.profiles.viewed = "aa".into();
    // A game's list is its own actions (the Remap All walk), then the Blockland actions that still apply to it.
    assert_eq!(u.core.remap_all_limit(), 2);
    assert_eq!(u.core.edit_remap().len(), 3);
    u.core.edit_binds_mut().force_remap(
        "package:aa:sprint",
        BindInput::Key(Chord::plain(Key::Letter('o'))),
    );
    u.core
        .prefs
        .set(&profiles::sensitivity_pref("aa"), "0.3".to_string());
    u.core.profiles.viewed = "blockland".into();
    assert_eq!(u.core.binds, blockland, "Blockland's bindings did not move");

    // Saved, then loaded by a later run.
    u.core.save_settings();
    let saved = u.settings();
    assert_eq!(saved.binds, Some(blockland.entries.clone()));
    let json = serde_json::to_string(&saved).unwrap();
    let loaded: Settings = serde_json::from_str(&json).unwrap();
    let mut later = with_profile(loaded);
    later.set_control_context("aa-soldier", None);
    assert!(
        press(&mut later, Key::Letter('o')).contains(&package("sprint", Some(true))),
        "the player's own key for the profile"
    );
    assert!((look(&mut later).0 - 100.0 * 0.3 * 0.005).abs() < 1e-5);
}

#[test]
fn a_profile_does_not_conflict_with_blockland_or_other_profiles() {
    let mut u = with_profile(Settings {
        binds: Some(vec![]),
        ..Default::default()
    });
    // Shift is crouch in Blockland and Sprint in the profile.
    u.core
        .binds
        .bind(BindInput::Key(Chord::plain(Key::LShift)), "crouch");
    assert_eq!(
        u.core.binds.binding_of("crouch"),
        Some(BindInput::Key(Chord::plain(Key::LShift)))
    );
    u.core.profiles.viewed = "aa".into();
    assert_eq!(
        u.core.edit_binds().binding_of("package:aa:sprint"),
        Some(BindInput::Key(Chord::plain(Key::LShift)))
    );
}

#[test]
fn settings_saved_before_profiles_still_load() {
    // A file as written before `profile_binds` existed: the same settings
    // with that field removed.
    let shift = BindEntry {
        command: "crouch".into(),
        input: BindInput::Key(Chord::plain(Key::LShift)),
    };
    let mut prefs = std::collections::BTreeMap::new();
    prefs.insert(
        "$pref::Input::MouseSensitivity".to_string(),
        "0.4".to_string(),
    );
    let now = Settings {
        prefs,
        binds: Some(vec![shift.clone()]),
        ..Default::default()
    };
    let mut json: serde_json::Value = serde_json::to_value(&now).unwrap();
    assert!(
        json.as_object_mut()
            .unwrap()
            .remove("profile_binds")
            .is_some()
    );
    let settings: Settings = serde_json::from_value(json).expect("old settings parse");
    assert!(settings.profile_binds.is_empty());
    let mut u = with_profile(settings);
    assert_eq!(u.core.binds.binding_of("crouch"), Some(shift.input));
    let stock = look(&mut u);
    assert!(
        (stock.0 - 100.0 * 0.4 * 0.005).abs() < 1e-5,
        "saved sensitivity"
    );
}

#[test]
fn a_profile_can_move_a_stock_action_off_a_key_it_takes() {
    let mut u = ui(Settings {
        binds: Some(vec![]),
        ..Default::default()
    });
    u.core
        .binds
        .bind(BindInput::Key(Chord::plain(Key::LShift)), "crouch");
    u.set_profile_defs(vec![profile()]);
    let mut crouch = bind("Crouch", "", "lcontrol");
    crouch.stock = Some("crouch".into());
    crouch.hold = false;
    u.set_package_binds(vec![bind("Sprint", "sprint", "lshift"), crouch]);
    u.set_control_context("aa-soldier", None);
    let held = |down| {
        UiAction::Game(GameAction::Held {
            control: HeldControl::Crouch,
            down,
        })
    };
    assert_eq!(press(&mut u, Key::LControl), vec![held(true), held(false)]);
    assert_eq!(
        press(&mut u, Key::LShift),
        vec![
            package("sprint", Some(true)),
            package("sprint", Some(false))
        ]
    );
    // As a Blockhead, Shift crouches again and Ctrl does not.
    u.set_control_context("blockhead", None);
    assert_eq!(press(&mut u, Key::LShift), vec![held(true), held(false)]);
}

#[test]
fn a_rule_layer_binds_over_the_profile_only_while_its_flag_is_on() {
    let mut u = ui(Settings {
        binds: Some(vec![]),
        ..Default::default()
    });
    let mut def = profile();
    def.layers.push(LayerDef {
        id: "radio".into(),
        name: None,
        when: LayerWhen::Rule("aa:player/radio".into()),
        invert_mouse: None,
        mouse_sensitivity: None,
    });
    u.set_profile_defs(vec![def]);
    let mut radio = bind("Radio One", "radio1", "v");
    radio.layer = "radio".into();
    radio.hold = false;
    u.set_package_binds(vec![bind("Fire Mode", "firemode", "v"), radio]);
    u.set_control_context("aa-soldier", None);
    assert_eq!(u.rule_flags_wanted(), ["aa:player/radio"]);
    // With the menu shut, V is the profile's Fire Mode.
    assert_eq!(
        press(&mut u, Key::Letter('v')),
        vec![
            package("firemode", Some(true)),
            package("firemode", Some(false))
        ]
    );
    // With it open, V sends the radio line and nothing else.
    u.set_rule_flags(["aa:player/radio".to_string()].into());
    assert_eq!(
        press(&mut u, Key::Letter('v')),
        vec![package("radio1", None)]
    );
    // Shut again: back to Fire Mode.
    u.set_rule_flags(Default::default());
    assert_eq!(
        press(&mut u, Key::Letter('v')),
        vec![
            package("firemode", Some(true)),
            package("firemode", Some(false))
        ]
    );
}
