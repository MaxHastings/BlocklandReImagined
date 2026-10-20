use bri_ue2_emitters::projector::*;
use bri_ue2_emitters::rotation::Rotator;
use glam::{Vec3, Vec4};

fn props(fov: i32) -> ProjectorProps {
    ProjectorProps {
        location: Vec3::new(100.0, 200.0, 50.0),
        rotation: Rotator::new(0, 0, 0),
        draw_scale: 1.0,
        texture_size: (64, 32),
        fov,
        max_trace_distance: 500,
        rotating: false,
        gradient_height: None,
    }
}

fn project(m: &glam::Mat4, p: Vec3) -> Vec4 {
    *m * p.extend(1.0)
}

fn inside(v: &ProjectorVolume, p: Vec3) -> bool {
    v.planes.iter().all(|pl| pl.truncate().dot(p) - pl.w >= 0.0)
}

#[test]
fn orthographic_maps_the_picture_rectangle_to_unit_coordinates() {
    let v = calc_matrix(&props(0)).unwrap();
    let loc = Vec3::new(100.0, 200.0, 50.0);
    // Facing +X: right is +Y, up is +Z.
    assert!((v.forward - Vec3::X).length() < 1e-5);
    let c = project(&v.matrix, loc);
    assert!((c.x - 0.5).abs() < 1e-5 && (c.y - 0.5).abs() < 1e-5 && c.z.abs() < 1e-4);
    let right = project(&v.matrix, loc + Vec3::new(0.0, 32.0, 0.0));
    assert!((right.x - 1.0).abs() < 1e-5);
    let up = project(&v.matrix, loc + Vec3::new(0.0, 0.0, 16.0));
    assert!(up.y.abs() < 1e-5);
    // Depth along the axis.
    let d = project(&v.matrix, loc + Vec3::new(120.0, 0.0, 0.0));
    assert!((d.z - 120.0).abs() < 1e-3);
    // The volume: in front up to the trace distance, not behind or beyond.
    assert!(inside(&v, loc + Vec3::new(10.0, 0.0, 0.0)));
    assert!(!inside(&v, loc - Vec3::new(1.0, 0.0, 0.0)));
    assert!(!inside(&v, loc + Vec3::new(501.0, 0.0, 0.0)));
}

#[test]
fn perspective_uses_the_same_scale_on_both_axes() {
    let v = calc_matrix(&props(90)).unwrap();
    assert!(v.perspective);
    let loc = Vec3::new(100.0, 200.0, 50.0);
    // tan(45) = 1; the eye sits Wd/2 behind Location. A point on the axis maps to the middle.
    let on_axis = project(&v.matrix, loc + Vec3::new(40.0, 0.0, 0.0));
    let (s, t) = (on_axis.x / on_axis.z, on_axis.y / on_axis.z);
    assert!((s - 0.5).abs() < 1e-4 && (t - 0.5).abs() < 1e-4);
    // At the picture plane the half-width Wd/2 = 32 reaches the edge, and so does the
    // same distance vertically (not Ht/2 = 16).
    let edge = project(&v.matrix, loc + Vec3::new(0.0, 32.0, -32.0));
    assert!((edge.x / edge.z - 1.0).abs() < 1e-4, "{}", edge.x / edge.z);
    assert!((edge.y / edge.z - 1.0).abs() < 1e-4, "{}", edge.y / edge.z);
    assert!(inside(&v, loc + Vec3::new(50.0, 0.0, 0.0)));
    assert!(!inside(&v, loc + Vec3::new(50.0, 500.0, 0.0)));
}

#[test]
fn the_gradient_spans_the_trace_distance_from_one_texel_in() {
    let mut p = props(0);
    p.gradient_height = Some(64);
    let v = calc_matrix(&p).unwrap();
    let g = v.gradient.unwrap();
    let loc = p.location;
    // q = 500 / 64 = 7, s = 62 / (64 * 500). At Location the v coordinate is q * s.
    let at = g * loc.extend(1.0);
    assert!((at.y - 7.0 * 62.0 / 32000.0).abs() < 1e-6, "{}", at.y);
    let far = g * (loc + Vec3::new(500.0, 0.0, 0.0)).extend(1.0);
    assert!((far.y - (500.0 + 7.0) * 62.0 / 32000.0).abs() < 1e-5);
}

#[test]
fn bsp_surfaces_must_face_the_projection_within_45_degrees() {
    let dir = Vec3::X;
    assert!(bsp_accepts(Vec3::NEG_X, dir, false));
    assert!(!bsp_accepts(Vec3::X, dir, false));
    // 37 degrees off (N.F = -0.8): fine. 60 degrees off (-0.5) and 80 degrees off (-0.17):
    // only when parallel surfaces are allowed.
    assert!(bsp_accepts(Vec3::new(-0.8, 0.6, 0.0), dir, false));
    assert!(!bsp_accepts(Vec3::new(-0.5, 0.866, 0.0), dir, false));
    assert!(bsp_accepts(Vec3::new(-0.5, 0.866, 0.0), dir, true));
    let grazing = Vec3::new(-0.17, 0.985, 0.0);
    assert!(!bsp_accepts(grazing, dir, false));
    assert!(bsp_accepts(grazing, dir, true));
}

#[test]
fn decals_expire_only_once_unseen_past_their_time_and_budget() {
    let budget = DecalBudget {
        session: 3,
        youngest: 100,
        keep: 10,
    };
    let life = |expiry: f64, stamp: i32, uid: i32, seen: f64| DecalLife {
        expiry,
        session_stamp: stamp,
        uid,
        last_seen: seen,
    };
    // Never expires.
    assert!(!is_expired(&life(0.0, 3, 5, 1.0), &budget, 99.0));
    // A different session or a forced detach removes it at once.
    assert!(is_expired(&life(50.0, 2, 95, 1.0), &budget, 1.0));
    assert!(is_expired(&life(50.0, -1, 95, 1.0), &budget, 1.0));
    // Within the budget (uid >= youngest - keep) it stays.
    assert!(!is_expired(&life(50.0, 3, 95, 1.0), &budget, 99.0));
    // Past the budget but not yet past its expiry, or recently seen: stays.
    assert!(!is_expired(&life(50.0, 3, 5, 40.0), &budget, 40.0));
    assert!(!is_expired(&life(50.0, 3, 5, 60.0), &budget, 60.5));
    // Past expiry, unseen for more than a second: gone.
    assert!(is_expired(&life(50.0, 3, 5, 58.0), &budget, 60.0));
}

#[test]
fn frame_buffer_ops_map_to_blends() {
    assert_eq!(
        frame_buffer_blend(ProjectorBlending::from_byte(1)),
        FrameBufferBlend::Modulate2X
    );
    assert_eq!(
        frame_buffer_blend(ProjectorBlending::from_byte(0)),
        FrameBufferBlend::Modulate2X
    );
    assert_eq!(
        frame_buffer_blend(ProjectorBlending::from_byte(2)),
        FrameBufferBlend::Alpha
    );
    assert_eq!(
        frame_buffer_blend(ProjectorBlending::from_byte(3)),
        FrameBufferBlend::Add
    );
}

#[test]
fn particle_projectors_are_selected_by_flag_tag_and_budget() {
    use bri_ue2_emitters::particle_projector::*;
    let cand = |tag: Option<u32>, particles: bool, op: u8| ProjectorCandidate {
        project_particles: particles,
        project_tag: tag,
        bitmap: Some(1),
        matrix: glam::Mat4::IDENTITY,
        perspective: false,
        blend_op: op,
    };
    let all = [
        cand(None, true, 3),
        cand(Some(7), true, 2),
        cand(Some(8), true, 1),
        cand(None, false, 1),
        cand(None, true, 0),
        cand(None, true, 1),
        cand(None, true, 1),
    ];
    // The emitter's tag is 7: untagged and tag-7 projectors that project particles, in
    // order, and only four of them.
    let picked = select(true, Some(7), &all);
    assert_eq!(picked.len(), 4);
    assert_eq!(picked[0].mode(), ParticleProjectorMode::Modulate2X);
    assert_eq!(picked[1].mode(), ParticleProjectorMode::ByPictureAlpha);
    assert_eq!(picked[2].mode(), ParticleProjectorMode::Modulate);
    assert!(select(false, Some(7), &all).is_empty());
    // A tagged projector for another tag is left out.
    assert_eq!(select(true, Some(9), &all[..3]).len(), 1);
}
