use bri_ue2_emitters::laser::*;
use bri_ue2_emitters::lens_flare::*;
use bri_ue2_emitters::props::Color;
use bri_ue2_emitters::rotation::Rotator;
use glam::Vec3;

fn props() -> SunProps {
    SunProps {
        lens_flares: vec![
            LensFlare {
                material: 1,
                color: Color::rgba(255, 255, 255, 255),
                offset: 1.0,
                radius_min: 1.0,
                radius_max: 2.0,
            },
            LensFlare {
                material: 2,
                color: Color::rgba(255, 255, 255, 80),
                offset: 0.0,
                radius_min: 0.0,
                radius_max: 0.0,
            },
        ],
        white_out: Color::rgba(102, 197, 254, 33),
        white_out_ramp: 2.0,
        angle_to_start_white_out: 90.0,
        angle_to_start_lens_flares: 90.0,
        intensity_scalar: Color::rgba(128, 128, 128, 255),
        draw_scale: 1.0,
        glow_skin: Some(9),
        lens_flares_in_editor: true,
    }
}

fn view(dot_angle_deg: f32, blocked: bool) -> SunView {
    let a = dot_angle_deg.to_radians();
    SunView {
        screen: [400.0, 100.0],
        width: 800.0,
        height: 600.0,
        sun_dir: Vec3::new(a.cos(), a.sin(), 0.0),
        view_dir: Vec3::X,
        blocked,
        using_glow: false,
    }
}

#[test]
fn flares_grow_with_alignment_and_sit_on_the_sun_to_centre_line() {
    // Looking straight at the sun: t = 1, so the radius is Max (2) and the flare with
    // offset 1 sits at the screen centre; the zero-radius flare is skipped.
    let tiles = apply_glow_effects(&props(), &view(0.0, false));
    let flares: Vec<_> = tiles
        .iter()
        .filter(|t| matches!(t.kind, TileKind::Flare(_)))
        .collect();
    assert_eq!(flares.len(), 1);
    let v = &flares[0].vertices;
    let centre = (v[0].position + v[2].position) / 2.0;
    assert!((centre - Vec3::new(400.0, 300.0, 0.0)).length() < 1e-3);
    // Half-width 0.5 * r * W; half-height 0.5 * r * H / 0.75.
    assert!(((v[2].position.x - v[0].position.x) / 2.0 - 800.0).abs() < 1e-2);
    assert!(((v[2].position.y - v[0].position.y) / 2.0 - 800.0).abs() < 1e-2);
    // Alpha = scalar.a/255 * flare.a * t = 255.
    assert_eq!(v[0].color >> 24, 255);
    // Colour is white times the 128 grey intensity.
    assert_eq!((v[0].color >> 16) & 255, 128);
}

#[test]
fn a_blocked_sun_draws_everything_black() {
    let tiles = apply_glow_effects(&props(), &view(0.0, true));
    assert!(!tiles.is_empty());
    for t in &tiles {
        assert_eq!(t.vertices[0].color & 0x00ff_ffff, 0, "{:?}", t.kind);
    }
}

#[test]
fn the_white_out_ramps_by_the_configured_power() {
    // 45 degrees off a 90 degree cone: ((cos45 - 0) / 1)^2 = 0.5 -> alpha 16.
    let tiles = apply_glow_effects(&props(), &view(45.0, false));
    let w = tiles
        .iter()
        .find(|t| t.kind == TileKind::WhiteOut)
        .expect("white out");
    assert_eq!(w.vertices[0].color >> 24, 16);
    assert_eq!(w.vertices[2].position, Vec3::new(800.0, 600.0, 0.0));
}

struct Wall {
    hit: Option<LaserHit>,
}
impl LaserWorld for Wall {
    fn trace(&mut self, _: Vec3, _: Vec3) -> Option<LaserHit> {
        self.hit
    }
}

fn pawn_input(viewer: Option<Vec3>) -> LaserInput {
    LaserInput {
        is_on: true,
        has_base: true,
        base: LaserBase::Pawn {
            location: Vec3::new(0.0, 0.0, 10.0),
            walk_bob: Vec3::ZERO,
            eye_height: 54.0,
            controller_rotation: Rotator::new(0, 0, 0),
        },
        viewer,
    }
}

#[test]
fn the_laser_dot_sits_four_units_off_the_hit_and_grows_with_distance() {
    let hit = LaserHit {
        location: Vec3::new(1000.0, 0.0, 64.0),
        pawn: None,
    };
    let out = tick(&pawn_input(Some(Vec3::ZERO)), &mut Wall { hit: Some(hit) });
    let p = out.placement.expect("placed");
    assert!((p.location - Vec3::new(996.0, 0.0, 64.0)).length() < 1e-3);
    let d = hit.location.length();
    assert!((p.draw_scale - 0.0175 * (1.0 + (d - 66.0) / 133.0)).abs() < 1e-6);
    // Near the viewer the scale stays at the base size.
    let near = LaserHit {
        location: Vec3::new(30.0, 0.0, 0.0),
        pawn: None,
    };
    let out = tick(&pawn_input(Some(Vec3::ZERO)), &mut Wall { hit: Some(near) });
    assert_eq!(out.placement.unwrap().draw_scale, 0.0175);
    // No hit, or switched off: detached.
    assert!(
        tick(&pawn_input(Some(Vec3::ZERO)), &mut Wall { hit: None })
            .placement
            .is_none()
    );
    let mut off = pawn_input(Some(Vec3::ZERO));
    off.is_on = false;
    assert!(tick(&off, &mut Wall { hit: Some(hit) }).placement.is_none());
}

#[test]
fn dazzle_needs_the_eye_close_and_the_victim_looking_into_the_beam() {
    let eye = Vec3::new(1000.0, 0.0, 64.0);
    let facing_beam = LaserHit {
        location: eye + Vec3::new(0.0, 0.2, 0.0),
        pawn: Some(HitPawn {
            id: 5,
            view_direction: Vec3::NEG_X,
            left_eye: eye,
        }),
    };
    let out = tick(
        &pawn_input(Some(Vec3::ZERO)),
        &mut Wall {
            hit: Some(facing_beam),
        },
    );
    let e = out.exposure.expect("exposure");
    // c = -1: ((0.5947 + 1) / 1.5947) * (1 - 0.04 / 0.6416) = 0.9376
    assert!((e.intensity - 0.9376).abs() < 1e-3, "{}", e.intensity);
    // Looking the same way as the beam travels: nothing.
    let mut same = facing_beam;
    same.pawn.as_mut().unwrap().view_direction = Vec3::X;
    assert!(
        tick(&pawn_input(Some(Vec3::ZERO)), &mut Wall { hit: Some(same) })
            .exposure
            .is_none()
    );
    // Eye too far from the hit.
    let mut far = facing_beam;
    far.location = eye + Vec3::new(0.0, 2.0, 0.0);
    assert!(
        tick(&pawn_input(Some(Vec3::ZERO)), &mut Wall { hit: Some(far) })
            .exposure
            .is_none()
    );
}

#[test]
fn exposure_rises_at_once_and_decays_half_a_unit_per_second() {
    let mut list = Vec::new();
    record_exposure(&mut list, 1, 0.4, 0.0);
    record_exposure(&mut list, 1, 0.9, 1.0);
    assert_eq!(list[0].intensity, 0.9);
    record_exposure(&mut list, 1, 0.1, 2.0);
    assert!((list[0].intensity - 0.4).abs() < 1e-6);
    record_exposure(&mut list, 1, 0.1, 10.0);
    assert!((list[0].intensity - 0.1).abs() < 1e-6);
}
