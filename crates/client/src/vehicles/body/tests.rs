use super::*;
use bri_content::shape::{Node, NodeTrack, Object};

fn frame() -> VehicleFrame {
    VehicleFrame {
        position: Vec3::new(2., 3., 4.),
        rotation: Quat::from_rotation_z(0.3),
        velocity: Vec3::ZERO,
        steering: -1.,
        wheel_suspension: vec![],
        wheel_rotation: vec![],
        wheel_contact: vec![],
        turret_aim: [0.; 2],
        driver_jump_held: false,
        driver_reversing: false,
        driver_braking: false,
    }
}

fn clip(
    name: &str,
    node: &str,
    rotations: Vec<[f32; 4]>,
    translations: Vec<[f32; 3]>,
) -> Animation {
    Animation {
        name: name.into(),
        frames: 2,
        duration: 1.,
        looping: true,
        additive: false,
        parent_translations: false,
        priority: 0,
        rotation_interpolation: Default::default(),
        nodes: vec![NodeTrack {
            node: node.into(),
            rotations,
            translations,
            scales: vec![],
            scale_rotations: vec![],
        }],
        objects: vec![],
        ground_translations: vec![],
        ground_rotations: vec![],
        triggers: vec![],
    }
}

#[test]
fn composed_body_parts_and_grips_follow_the_same_pose_and_shared_models_keep_their_parts()
-> Result<()> {
    let dir = crate::testing::ScratchDir::new("composed-vehicle")?;
    crate::testing::vehicles::write_pack(dir.path())?;
    let path = dir.path().join("vehicles.json");
    let mut pack: Pack = serde_json::from_slice(&std::fs::read(&path)?)?;
    let car = pack
        .definitions
        .iter()
        .find(|d| d.id == bri_vehicles::testing::CAR)
        .unwrap()
        .clone();
    let model = dir.path().join(&car.model);
    let mut shape: Shape = serde_json::from_slice(&std::fs::read(&model)?)?;
    let wheel = shape.nodes.len();
    shape.nodes.push(Node {
        name: "wheel".into(),
        parent: Some(0),
        translation: [0.; 3],
        rotation: Quat::IDENTITY.to_array(),
    });
    shape.nodes.push(Node {
        name: "grip".into(),
        parent: Some(wheel),
        translation: [0.2, 0., 0.],
        rotation: Quat::IDENTITY.to_array(),
    });
    shape.objects.push(Object {
        name: "lens".into(),
        node: Some(wheel),
        meshes: shape.objects[0].meshes.clone(),
        visibility: 1.,
        frame: 0,
        material_frame: 0,
        live: None,
    });
    shape.details[0].object_count = shape.objects.len();
    shape.animations.push(clip(
        "steer",
        "wheel",
        vec![
            Quat::IDENTITY.to_array(),
            Quat::from_rotation_z(0.7).to_array(),
        ],
        vec![],
    ));
    shape.animations.push(clip(
        "drive",
        &shape.nodes[0].name,
        vec![],
        vec![[0.; 3], [0., 0.4, 0.]],
    ));
    shape.animations.last_mut().unwrap().looping = false;
    shape.validate()?;
    let bytes = serde_json::to_vec(&shape)?;
    std::fs::write(model, &bytes)?;
    for asset in pack.assets.iter_mut().filter(|a| a.path == car.model) {
        asset.sha256 = crate::testing::sha256(&bytes);
    }
    let d = pack
        .definitions
        .iter_mut()
        .find(|d| d.id == car.id)
        .unwrap();
    d.steering_sequence = Some("steer".into());
    d.threads = vec![bri_vehicles::schema::AnimationThread {
        slot: 0,
        sequence: "drive".into(),
        rate: 1.,
        min_speed: None,
        max_speed: None,
    }];
    d.seats[0].hands.right_node = Some("grip".into());
    d.lights = vec![bri_vehicles::schema::Lamp {
        activation: Default::default(),
        effect: None,
        node: "wheel".into(),
        offset: [0.; 3],
        color: [1.; 3],
        radius: 5.,
        cone: None,
        ramp: None,
    }];
    d.visuals = serde_json::from_value(serde_json::json!([
        {"objects":["lens"],"when":{"all":["switch"],"none":["destroyed"]},"off":{"color":[0,0,0,255]}}
    ]))?;
    let mut sibling = car.clone();
    sibling.id = "test:vehicle/shared-body".into();
    pack.definitions.push(sibling.clone());
    pack.validate()?;
    std::fs::write(path, serde_json::to_vec(&pack)?)?;
    let mut assets = VehicleAssets::load(dir.path())?;
    let d = assets.definition(&car.id).unwrap().clone();
    let mut client = ClientVehicles {
        clock: 60.,
        ..Default::default()
    };
    let f = frame();
    client.frames.insert(1, f.clone());
    let mut info = VehicleInfo {
        id: 1,
        definition: car.id.clone(),
        color: None,
        occupants: vec![Some(7)],
        destroyed: false,
        turret_broken: false,
        scale: 1.7,
        lights: false,
    };
    let rig = &assets.bodies[&car.model];
    let posed = rig.sample(&d, &f, 0.5, None)?;
    assert!(
        (posed.nodes[0].w_axis.y - 0.2).abs() < 1e-5,
        "thread translation survives steering"
    );
    let body =
        Mat4::from_scale_rotation_translation(Vec3::splat(info.scale), f.rotation, f.position);
    let grip = client.model_node(&assets, &info, "grip").unwrap();
    assert!(grip.abs_diff_eq(body * posed.nodes[wheel + 1], 1e-5));
    let held = client.hand_holds(&assets, &info, 0)[0].unwrap();
    assert!(
        held.position
            .unwrap()
            .abs_diff_eq(grip.w_axis.truncate(), 1e-5)
    );
    let lens = rig
        .parts
        .iter()
        .find(|p| p.object_state.as_deref() == Some("lens"))
        .unwrap();
    let (on, off, inverse) = (
        lens.variants[&car.id][1].clone().unwrap(),
        lens.variants[&car.id][0].clone().unwrap(),
        lens.inverse,
    );
    client.prepare(&mut assets, &BTreeMap::from([(1, info.clone())]));
    let drawn = assets.models[&off].transforms[0].transform;
    assert!(drawn.abs_diff_eq(body * posed.nodes[wheel] * inverse, 1e-5));
    assert!(assets.models[&on].transforms.is_empty());
    // A sibling with no steering or lamps keeps its complete model at rest.
    info.definition = sibling.id.clone();
    client.prepare(&mut assets, &BTreeMap::from([(1, info.clone())]));
    assert_eq!(assets.models[&on].transforms.len(), 0);
    let default_key = assets.bodies[&car.model]
        .parts
        .iter()
        .find(|p| p.object_state.as_deref() == Some("lens"))
        .unwrap()
        .on
        .clone();
    assert_eq!(assets.models[&default_key].transforms.len(), 1);
    assert!(assets.models[&off].transforms.is_empty());
    assert!(
        assets.models[&default_key].transforms[0]
            .transform
            .abs_diff_eq(body, 1e-5)
    );
    // A look channel and steering compose instead of silently disabling one.
    let mut rig = assets.bodies.remove(&car.model).unwrap();
    rig.look = Some(clip(
        "look",
        &shape.nodes[0].name,
        vec![
            Quat::IDENTITY.to_array(),
            Quat::from_rotation_y(0.4).to_array(),
        ],
        vec![],
    ));
    rig.look.as_mut().unwrap().looping = false;
    let mut armed = d.clone();
    armed.weapon = pack.definitions.iter().find_map(|d| d.weapon.clone());
    armed.attachment_model = None;
    armed.weapon.as_mut().unwrap().look_animation_pitch = Some(armed.look_pitch);
    let mut aimed = f.clone();
    aimed.turret_aim[1] = armed.look_pitch[0];
    let combined = rig.sample(&armed, &aimed, 0.5, None)?;
    assert!((combined.nodes[0].w_axis.y - 0.2).abs() < 1e-5);
    assert!(!combined.nodes[0].abs_diff_eq(posed.nodes[0], 1e-5));
    assert!(
        (combined.nodes[wheel]
            .to_scale_rotation_translation()
            .1
            .dot(Quat::from_rotation_y(0.4) * Quat::from_rotation_z(0.7))
            .abs()
            - 1.)
            .abs()
            < 1e-5
    );
    Ok(())
}

/// A propeller whose `slow` and `fast` sequences turn it a quarter turn
/// per frame over one second and a quarter second.
fn propeller() -> BodyRig {
    let quarter =
        |i: usize| Quat::from_rotation_z(i as f32 * std::f32::consts::FRAC_PI_2).to_array();
    let clip = |name: &str, duration: f32| Animation {
        name: name.into(),
        frames: 4,
        duration,
        looping: true,
        additive: false,
        parent_translations: false,
        priority: 0,
        nodes: vec![bri_content::shape::NodeTrack {
            node: "prop".into(),
            rotations: (0..4).map(quarter).collect(),
            translations: vec![],
            scales: vec![],
            scale_rotations: vec![],
        }],
        objects: vec![],
        rotation_interpolation: Default::default(),
        ground_translations: vec![],
        ground_rotations: vec![],
        triggers: vec![],
    };
    let node = |name: &str, parent| bri_content::shape::Node {
        name: name.into(),
        parent,
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    BodyRig {
        shape: Shape {
            schema_version: 1,
            id: "plane".into(),
            nodes: vec![node("root", None), node("prop", Some(0))],
            objects: vec![],
            details: vec![],
            meshes: vec![],
            materials: vec![],
            animations: [("slow", 1.0), ("fast", 0.25)]
                .into_iter()
                .map(|(n, d)| clip(n, d))
                .collect(),
        },
        steering: BTreeMap::new(),
        look: None,
        parts: vec![],
    }
}
#[test]
fn threads_pick_their_sequence_by_speed_and_play_at_their_rate() {
    use bri_vehicles::schema::AnimationThread;
    let rig = propeller();
    let thread = |sequence: &str, min: Option<f32>, max: Option<f32>, rate: f32| AnimationThread {
        slot: 0,
        sequence: sequence.into(),
        rate,
        min_speed: min,
        max_speed: max,
    };
    let blade = |threads: &[AnimationThread], speed: f32, seconds: f64| {
        let mut d = bri_vehicles::testing::pack().definitions[0].clone();
        d.threads = threads.to_vec();
        let mut input = frame();
        input.velocity = Vec3::X * speed;
        let pose = rig.sample(&d, &input, seconds, None).unwrap();
        let tip = pose.nodes[1].transform_vector3(Vec3::X);
        tip.y.atan2(tip.x).to_degrees().round()
    };
    let switch = [
        thread("slow", None, Some(5.0), 1.0),
        thread("fast", Some(5.0), None, 1.0),
    ];
    // Slow below speed 5: a quarter turn a quarter second in.
    assert_eq!(blade(&switch, 0.0, 0.25), 90.0);
    // Fast from speed 5: a whole turn by then, a quarter an eighth later.
    assert_eq!(blade(&switch, 5.0, 0.25), 0.0);
    assert_eq!(blade(&switch, 30.0, 0.3125), 90.0);
    // Twice the rate, and a negative rate turning it backwards.
    assert_eq!(blade(&[thread("slow", None, None, 2.0)], 0.0, 0.125), 90.0);
    assert_eq!(blade(&[thread("slow", None, None, -1.0)], 0.0, 0.25), -90.0);
    // Hours in, the phase is still exact.
    assert_eq!(blade(&switch, 0.0, 36_000.25), 90.0);
    // No thread matches: the blade rests.
    assert_eq!(
        blade(&[thread("fast", Some(5.0), None, 1.0)], 0.0, 0.25),
        0.0
    );
}
