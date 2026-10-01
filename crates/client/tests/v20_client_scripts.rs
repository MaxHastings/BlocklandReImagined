//! Client behaviour pinned to v20's client scripts and engine: name tags
//! (`GuiShapeNameHud`, blocklandv20.exe 0x5278f0 and 0x527630, and the stock
//! scripts that colour names) and `handleYourSpawn`.
use bri_client::app::name_opacity;
use bri_client::minigame_ui::color_rgb;
use bri_ui::api::{NameTag, name_outline};

#[test]
fn names_stay_solid_until_the_fog_then_fade_out_by_the_visible_distance() {
    // Blockland ignores PlayGui_ShapeNameHud's `distanceFade = 0.1`: the
    // default name distance of 8192 leaves the fog distance as the fade start.
    assert_eq!(name_opacity(10.0, 8192.0, 300.0, 500.0), Some(1.0));
    assert_eq!(name_opacity(299.0, 8192.0, 300.0, 500.0), Some(1.0));
    assert_eq!(name_opacity(400.0, 8192.0, 300.0, 500.0), Some(0.5));
    assert_eq!(name_opacity(500.0, 8192.0, 300.0, 500.0), Some(0.0));
    assert_eq!(name_opacity(500.5, 8192.0, 300.0, 500.0), None);
    assert_eq!(name_opacity(0.0, 8192.0, 300.0, 500.0), None);
    // A mini-game's shorter name distance (Slayer's Name Distance 140):
    // full to 135, fading out at 140.
    assert_eq!(name_opacity(100.0, 140.0, 300.0, 500.0), Some(1.0));
    assert_eq!(name_opacity(137.5, 140.0, 300.0, 500.0), Some(0.5));
    assert_eq!(name_opacity(141.0, 140.0, 300.0, 500.0), None);
}

#[test]
fn names_are_white_with_a_black_outline_and_members_take_the_minigame_colour() {
    // `setShapeNameColor("1 1 1")` outside a mini-game; drawName ignores the
    // control's `textColor = "1 1 0.909"`.
    let tag: NameTag = serde_json::from_str(r#"{"x":0,"y":0,"text":"Max","opacity":1}"#).unwrap();
    assert_eq!(tag.color, [255, 255, 255]);
    assert_eq!(name_outline(tag.color), [0, 0, 0]);
    // $MiniGameColorI: Red, Blue and Black.
    assert_eq!(color_rgb(0), Some([255, 0, 0]));
    assert_eq!(color_rgb(7), Some([0, 128, 255]));
    assert_eq!(color_rgb(9), Some([0, 0, 0]));
    assert_eq!(color_rgb(10), None);
    // White outline only when red and green are both under 0.3.
    assert_eq!(name_outline([0, 0, 0]), [255, 255, 255]);
    assert_eq!(name_outline([0, 76, 255]), [255, 255, 255]);
    assert_eq!(name_outline([0, 77, 255]), [0, 0, 0]);
    assert_eq!(name_outline([0, 128, 255]), [0, 0, 0]);
    assert_eq!(name_outline([255, 0, 0]), [0, 0, 0]);
}

/// `handleYourSpawn`: with `$pref::Input::AutoLight` (on by default) a spawn
/// under a sun whose red, green and blue are all below 0.4 sends /light.
#[test]
fn spawning_under_a_dark_sun_turns_the_light_on() {
    use bri_client::app::dark_sun;
    assert!(dark_sun([0.1, 0.1, 0.2]));
    assert!(dark_sun([0.39, 0.39, 0.39]));
    assert!(!dark_sun([0.4, 0.1, 0.1]));
    assert!(!dark_sun([0.1, 0.1, 0.4]));
    assert!(!dark_sun([0.6, 0.6, 0.6]));
}
