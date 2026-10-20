use bri_ue2_emitters::corona::*;
use bri_ue2_emitters::random::Rand;
use bri_ue2_emitters::rotation::Rotator;
use glam::Vec3;

struct Clear;
impl CoronaWorld for Clear {
    fn trace(&mut self, _: Vec3, _: Vec3, _: Option<u32>) -> Option<Option<u32>> {
        None
    }
}
struct Wall;
impl CoronaWorld for Wall {
    fn trace(&mut self, _: Vec3, _: Vec3, _: Option<u32>) -> Option<Option<u32>> {
        Some(Some(7))
    }
}

/// The MP_Pipeline lamp from the spec's worked example.
fn lamp(location: Vec3) -> CoronaActor {
    CoronaActor {
        id: 1,
        location,
        rotation: Rotator::default(),
        owner: None,
        hue: 28,
        saturation: 255,
        scale_glow: 1.0,
        draw_scale: 1.0,
        draw_scale3d: Vec3::new(0.65, 0.65, 0.10),
        clamp_distance: 1500.0,
        near_clamp_distance: 800.0,
        directional: false,
        night_vision_only: false,
        normal_vision_only: false,
        has_texture: true,
        deleted: false,
        alpha_texture: false,
        glow_descriptor: None,
        light: Some(LightCorona {
            min_size: 80.0,
            max_size: 100.0,
            ..Default::default()
        }),
    }
}

fn scene(time: f64) -> CoronaScene {
    CoronaScene {
        camera: Vec3::ZERO,
        right: Vec3::new(0.0, 1.0, 0.0),
        up: Vec3::new(0.0, 0.0, 1.0),
        time,
        behind_view: false,
        view_actor: None,
        night_vision: false,
        glow_filter: None,
    }
}

fn half_size(q: &CoronaQuad) -> f32 {
    (q.vertices[1].position - q.vertices[0].position).length() / 2.0
}

#[test]
fn size_follows_distance_and_the_light_clamp() {
    // d = 800: s = 0.64, X = 53 -> clamped up to 80. d = 1200: 99.84. Far: 100.
    let near = quad(&scene(0.0), &lamp(Vec3::new(800.0, 0.0, 0.0)), 1.0);
    assert!(
        (half_size(&near) - 80.0).abs() < 0.5,
        "{}",
        half_size(&near)
    );
    let mid = quad(&scene(0.0), &lamp(Vec3::new(1200.0, 0.0, 0.0)), 1.0);
    assert!((half_size(&mid) - 99.84).abs() < 0.5, "{}", half_size(&mid));
    let far = quad(&scene(0.0), &lamp(Vec3::new(5000.0, 0.0, 0.0)), 1.0);
    assert!((half_size(&far) - 100.0).abs() < 0.5);
}

#[test]
fn colour_is_the_hue_colour_times_fade_with_full_alpha() {
    let mut a = lamp(Vec3::new(1200.0, 0.0, 0.0));
    a.saturation = 0;
    let q = quad(&scene(0.0), &a, 0.5);
    let c = q.vertices[0].color;
    // Saturation 255 is white at any hue.
    let w = quad(&scene(0.0), &lamp(Vec3::new(1200.0, 0.0, 0.0)), 1.0).vertices[0].color;
    assert_eq!(w, 0xffff_ffff);
    assert_eq!(c >> 24, 255);
    // Hue 28 unsaturated is an orange: red 0.67 and green 0.33 of full, halved by the fade.
    let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
    assert!(
        (r as i32 - 85).abs() <= 2 && (g as i32 - 41).abs() <= 2 && b == 0,
        "{r} {g} {b}"
    );
}

#[test]
fn fade_rises_and_falls_at_three_times_scale_glow_per_second() {
    let mut m = CoronaManager::default();
    let mut rand = Rand::new(4);
    let a = lamp(Vec3::new(1000.0, 0.0, 0.0));
    let lookup = |id: u32| (id == 1).then(|| a.clone());
    m.add(1);
    // Ten frames of 1/30 s in the clear: +3 per second takes the fade to 1 in 1/3 s.
    let mut t = 0.0;
    for _ in 0..4 {
        t += 1.0 / 30.0;
        m.frame(&scene(t), &lookup, &mut Clear, &mut rand);
    }
    let f = m.fade(1).expect("entry");
    assert!(f > 0.5 && f < 1.0, "{f}");
    for _ in 0..10 {
        t += 1.0 / 30.0;
        m.frame(&scene(t), &lookup, &mut Clear, &mut rand);
    }
    assert_eq!(m.fade(1), Some(1.0));
    // Behind a wall the entry decays and is freed after about a third of a second.
    // The cached visibility is refreshed at random, so give it a few frames.
    for _ in 0..30 {
        t += 1.0 / 30.0;
        m.frame(&scene(t), &lookup, &mut Wall, &mut rand);
    }
    assert_eq!(m.fade(1), None);
}

#[test]
fn a_directional_corona_facing_away_is_hidden_without_a_trace() {
    let mut a = lamp(Vec3::new(1000.0, 0.0, 0.0));
    a.directional = true;
    // Facing +X, away from a camera at the origin looking at it.
    let mut m = CoronaManager::default();
    let mut rand = Rand::new(5);
    let lookup = |id: u32| (id == 1).then(|| a.clone());
    m.add(1);
    let q = m.frame(&scene(0.5), &lookup, &mut Clear, &mut rand);
    assert!(q.is_empty());
    assert_eq!(m.fade(1), None);
}

#[test]
fn cull_distance_scales_with_the_field_of_view() {
    // At FOV 90 the factor is tan(90 * pi / 360) = 0.91633.
    assert!(cull_distance_allows(0.0, 1e9, 90.0));
    assert!(cull_distance_allows(1000.0, 1090.0, 90.0));
    assert!(!cull_distance_allows(1000.0, 1100.0, 90.0));
    // A narrow FOV is clamped at 10 degrees: factor tan(5 deg) = 0.0875.
    assert!(cull_distance_allows(100.0, 1000.0, 5.0));
    assert!(!cull_distance_allows(100.0, 1200.0, 5.0));
}
