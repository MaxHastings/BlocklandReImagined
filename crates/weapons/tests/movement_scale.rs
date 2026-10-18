use bri_weapons::MovementScale;

#[test]
fn reloading_and_aiming_multiply_and_each_stays_in_range() {
    let m: MovementScale = serde_json::from_str(r#"{ "reloading": 0.75, "aiming": 0.5 }"#).unwrap();
    assert!(m.valid());
    assert_eq!(m.scale(false, false), 1.0);
    assert_eq!(m.scale(true, false), 0.75);
    assert_eq!(m.scale(false, true), 0.5);
    assert_eq!(m.scale(true, true), 0.375);
    // A field left out slows nothing.
    let only: MovementScale = serde_json::from_str(r#"{ "aiming": 0.75 }"#).unwrap();
    assert_eq!(only.scale(true, false), 1.0);
    assert!(!MovementScale { reloading: 0.0, aiming: 1.0 }.valid());
    assert!(!MovementScale { reloading: 1.0, aiming: 1.5 }.valid());
    assert!(serde_json::from_str::<MovementScale>(r#"{ "walking": 0.5 }"#).is_err());
}
