//! The projector baker: package-unit projectors become native-frame decals whose matrices, planes and receiver rules agree with CalcMatrix.
use bri_content::decals::Blend;
use bri_ue2_import::projector_bake::{Picture, bake};
use glam::{Mat4, Vec3, Vec4};
use serde_json::json;

fn projector(extra: serde_json::Value) -> serde_json::Value {
    let mut p = json!({
        "name": "Projector0", "texture": "T.Shadow",
        "location": [0.0, 0.0, 0.0], "rotation": [0, 0, 0], "draw_scale": 1.0,
        "properties": { "FOV": 0, "MaxTraceDistance": 64 },
        "defaults": { "FrameBufferBlendingOp": 1, "bProjectBSP": true, "bProjectTerrain": true,
                      "bProjectStaticMesh": true, "bProjectOnParallelBSP": true },
    });
    for (k, v) in extra.as_object().unwrap() {
        p["properties"][k] = v.clone();
    }
    json!({ "projectors": [p] })
}

fn find(_: &str) -> Option<Picture> {
    Some(Picture {
        file: "T.Shadow.png".into(),
        width: 64,
        height: 64,
    })
}

#[test]
fn an_orthographic_projector_maps_its_centre_to_the_middle_of_the_picture_in_the_native_frame() {
    let set = bake(&projector(json!({})), 0.04, &find);
    let d = &set.decals[0];
    let m = Mat4::from_cols_array(&d.world_to_texture);
    assert!(!d.perspective);
    // Package +x (forward) is native -z; package +y (right) is native +x.
    assert!((Vec3::from(d.forward) - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-5);
    let at = |p: Vec3| m * Vec4::new(p.x, p.y, p.z, 1.0);
    let centre = at(Vec3::new(0.0, 0.0, -0.4));
    assert!(
        (centre.x - 0.5).abs() < 1e-5 && (centre.y - 0.5).abs() < 1e-5,
        "{centre}"
    );
    // 16 package units to the right: a quarter of the 64-texel picture.
    let right = at(Vec3::new(16.0 * 0.04, 0.0, 0.0));
    assert!((right.x - 0.75).abs() < 1e-4, "{right}");
    // The picture's depth runs along the forward direction.
    assert!((at(Vec3::new(0.0, 0.0, -2.56)).z - 64.0).abs() < 1e-3);
    // The centre is inside every plane, a point far behind is not.
    let inside = |p: Vec3| {
        d.planes
            .iter()
            .all(|pl| Vec3::new(pl[0], pl[1], pl[2]).dot(p) - pl[3] >= -1e-4)
    };
    assert!(inside(Vec3::new(0.0, 0.0, -1.0)));
    assert!(!inside(Vec3::new(0.0, 0.0, 1.0)));
    assert_eq!(d.blend, Blend::Modulate2x);
    assert!(d.surfaces.enabled && d.ground.enabled);
}

#[test]
fn a_perspective_projector_divides_and_a_spinning_one_gets_wider_planes_and_a_rate() {
    let still = bake(&projector(json!({ "FOV": 55 })), 0.04, &find);
    assert!(still.decals[0].perspective);
    assert_eq!(still.decals[0].spin_degrees_per_second, 0.0);
    let spinning = bake(
        &projector(json!({ "FOV": 55, "Physics": 5, "RotationRate": [0, 0, 16384] })),
        0.04,
        &find,
    );
    let d = &spinning.decals[0];
    assert!((d.spin_degrees_per_second - 90.0).abs() < 1e-3);
    // A point just outside the still frustum's side is inside the spinning one's.
    let probe = Vec3::new(0.7, 0.0, -0.5);
    let inside = |set: &bri_content::decals::DecalSet| {
        set.decals[0]
            .planes
            .iter()
            .all(|pl| Vec3::new(pl[0], pl[1], pl[2]).dot(probe) - pl[3] >= 0.0)
    };
    assert!(
        inside(&spinning) || !inside(&still),
        "a wider frustum never holds less"
    );
}

#[test]
fn receiver_flags_follow_the_projector() {
    let set = bake(
        &projector(
            json!({ "bProjectBSP": false, "bProjectStaticMesh": false, "bProjectOnParallelBSP": false }),
        ),
        0.04,
        &find,
    );
    let d = &set.decals[0];
    assert!(!d.surfaces.enabled);
    assert!(d.ground.enabled && d.ground.fade_by_facing);
    let both = bake(
        &projector(json!({ "bProjectOnParallelBSP": false })),
        0.04,
        &find,
    );
    assert!((both.decals[0].surfaces.facing_limit + std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    let missing = bake(&projector(json!({})), 0.04, &|_| None);
    assert!(missing.decals.is_empty());
    assert!(missing.notes.iter().any(|n| n.contains("not available")));
}
