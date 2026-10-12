use super::testing::{ARMATURE_SCALE, BONES, character, rifle};
use super::*;
use bri_content::animation;

fn convert_glb(glb: &[u8], opts: &Options) -> Result<Converted> {
    let doc = Document::from_bytes(glb, None)?;
    convert(&doc, None, opts, "figure", "")
}
fn body_opts() -> Options {
    Options::new("figure.glb", "out", Kind::Body)
}
fn node(shape: &Shape, name: &str) -> usize {
    shape
        .nodes
        .iter()
        .position(|n| n.name == name)
        .unwrap_or_else(|| panic!("no node {name}"))
}
fn posed(shape: &Shape, clip: Option<&str>, time: f32) -> Vec<Vec3> {
    let clip = clip.map(|c| shape.animations.iter().find(|a| a.name == c).unwrap());
    let pose = animation::sample(shape, clip, time).unwrap();
    let detail = shape.details.iter().position(|d| !d.collision).unwrap();
    animation::triangles(shape, &pose, detail, |_| true)
        .unwrap()
        .iter()
        .flat_map(|t| t.vertices.iter().map(|v| v.position).collect::<Vec<_>>())
        .collect()
}
fn near(a: Vec3, b: Vec3) -> bool {
    a.distance(b) < 1e-3
}

/// Where the figure's vertex `p` (model space, bind pose, skinned to
/// `bone` alone) is with the bones' local rotations `pose`, the way a glTF
/// viewer draws it, in the native frame: the reference the import must
/// reproduce.
fn gltf_posed(p: Vec3, bone: usize, pose: &[(usize, Quat, Vec3)]) -> Vec3 {
    let mut world: Vec<Mat4> = Vec::new();
    for (i, (_, parent, t)) in BONES.iter().enumerate() {
        let (r, t) = pose
            .iter()
            .find(|(b, ..)| *b == i)
            .map_or((Quat::IDENTITY, Vec3::from(*t)), |(_, r, t)| (*r, *t));
        let parent = parent.map_or(Mat4::from_scale(Vec3::splat(ARMATURE_SCALE)), |p| world[p]);
        world.push(parent * Mat4::from_rotation_translation(r, t));
    }
    let mut bind: Vec<Mat4> = Vec::new();
    for (_, parent, t) in BONES {
        let parent = parent.map_or(Mat4::from_scale(Vec3::splat(ARMATURE_SCALE)), |p| bind[p]);
        bind.push(parent * Mat4::from_translation(Vec3::from(t)));
    }
    point((world[bone] * bind[bone].inverse()).transform_point3(p))
}

#[test]
fn a_rigged_figure_faces_the_game_way_and_keeps_its_bind_pose() {
    let figure = character(true);
    let c = convert_glb(&figure.glb, &body_opts()).unwrap();
    let shape = &c.shape;
    shape.validate().unwrap();
    // Its nose, at +Z in glTF, is at -Z: it faces the way bodies face.
    let points = posed(shape, None, 0.0);
    assert!(points.iter().any(|p| near(*p, point(figure.nose))));
    // The armature's 0.5 scale is in the bones' positions, not lost.
    assert!(near(
        world(shape, node(shape, "head")).w_axis.truncate(),
        Vec3::new(0.0, 2.0, 0.0)
    ));
    assert!((c.report.height - 2.25).abs() < 1e-3, "{}", c.report.height);
    assert_eq!(c.report.bones, 5);
    // The crown is rigid on the head, squashed by its own scale: 0.1 tall.
    let crown = &shape.objects.iter().find(|o| o.name == "Crown").unwrap();
    let mesh = shape.meshes[crown.meshes[0]].as_ref().unwrap();
    let ys: Vec<f32> = mesh.positions.iter().map(|p| p[1]).collect();
    let span =
        ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min);
    assert!((span - 0.1).abs() < 1e-4, "{span}");
    assert!(mesh.skin.is_none() && crown.node == Some(node(shape, "Crown")));
}

#[test]
fn clips_play_as_a_gltf_viewer_shows_them() {
    let figure = character(true);
    let c = convert_glb(&figure.glb, &body_opts()).unwrap();
    let shape = &c.shape;
    let points = posed(shape, Some("run"), 0.5);
    // At 0.5 s the right arm is swung back (-0.6 about X) and the hips have
    // moved a quarter unit forward (glTF +Z, native -Z).
    let hand = Vec3::new(-0.4, 1.15, 0.1);
    let expected = gltf_posed(
        hand,
        3,
        &[
            (3, Quat::from_rotation_x(-0.6), Vec3::from(BONES[3].2)),
            (0, Quat::IDENTITY, Vec3::new(0.0, 2.0, 0.25)),
        ],
    );
    assert!(
        points.iter().any(|p| near(*p, expected)),
        "the swung hand is at {expected}"
    );
    // A step clip holds its first key until the second.
    let back = shape.animations.iter().find(|a| a.name == "back").unwrap();
    assert!(back.looping && back.frames == 15, "{} frames", back.frames);
    // Cubic splines hold their value at the keys.
    let side = shape.animations.iter().find(|a| a.name == "side").unwrap();
    let hips = side.nodes.iter().find(|t| t.node == "hips").unwrap();
    assert!(
        Quat::from_array(hips.rotations[0])
            .dot(Quat::IDENTITY)
            .abs()
            > 0.9999
    );
}

#[test]
fn roles_take_the_blockheads_clip_names_and_play_the_way_the_body_needs() {
    let c = convert_glb(&character(true).glb, &body_opts()).unwrap();
    let clip = |name: &str| {
        c.shape
            .animations
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("no clip {name}"))
    };
    let idle = clip("root");
    assert!(
        idle.looping && !idle.additive && idle.frames == 30 && (idle.duration - 1.0).abs() < 1e-6
    );
    let aim = clip("armreadyright");
    assert!(
        !aim.looping && aim.priority > idle.priority,
        "held at its last frame, over locomotion"
    );
    assert_eq!(aim.frames, 7, "0.2 s at 30 per second, both ends");
    // Look is an overlay relative to its middle (level) frame: the head's
    // first frame turns it up (+X in the native frame, the Blockhead's
    // `look`) and the still spine is left out.
    let look = clip("look");
    assert!(look.additive && !look.looping);
    assert_eq!(look.nodes.len(), 1, "only the head moves");
    let head = &look.nodes[0];
    assert_eq!(head.node, "head");
    let middle = Quat::from_array(head.rotations[look.frames / 2]);
    assert!(middle.dot(Quat::IDENTITY).abs() > 0.9999);
    let up = Quat::from_array(head.rotations[0]) * Vec3::NEG_Z;
    assert!(up.y > 0.6, "frame 0 looks up: {up}");
    assert!(clip("headside").additive);
    // Unrecognised clips keep their names and play once.
    let stomp = clip("stomp");
    assert!(!stomp.looping && !stomp.additive);
    let roles: Vec<_> = c
        .report
        .clips
        .iter()
        .filter_map(|c| c.role.clone())
        .collect();
    assert_eq!(
        roles,
        [
            "idle",
            "run",
            "back",
            "side",
            "crouch",
            "look",
            "head_side",
            "aim_right",
            "sit"
        ]
    );
    assert!(
        c.report
            .missing_roles
            .iter()
            .any(|m| m == "no `death` clip: it falls in its `idle`")
    );
    // `Run` drifts half a unit (a quarter after the armature's scale).
    assert!(
        c.report
            .warnings
            .iter()
            .any(|w| w.contains("`Run` moves the model 0.25 units")),
        "{:?}",
        c.report.warnings
    );
}

#[test]
fn sockets_follow_their_bones_and_hold_items_level() {
    let figure = character(true);
    let c = convert_glb(&figure.glb, &body_opts()).unwrap();
    let shape = &c.shape;
    let mount = node(shape, "Mount0");
    assert_eq!(
        shape.nodes[mount].parent,
        Some(node(shape, "socket_right_hand"))
    );
    let (_, rotation, at) = world(shape, mount).to_scale_rotation_translation();
    assert!(
        rotation.dot(Quat::IDENTITY).abs() > 0.9999,
        "level in the rest pose, however the empty turned"
    );
    // The socket's place: the arm (-0.8, 0.8) and the empty (0, -0.6, 0.1)
    // under the spine, at half scale, half a turn about Y.
    let expected = point(
        Vec3::new(0.0, 1.0, 0.0)
            + Vec3::new(0.0, 1.0, 0.0) * 0.5
            + Vec3::new(-0.8, 0.8, 0.0) * 0.5
            + Vec3::new(0.0, -0.6, 0.1) * 0.5,
    );
    assert!(near(at, expected), "{at} vs {expected}");
    assert_eq!(c.report.sockets["right_hand"], "socket_right_hand");
    assert_eq!(
        c.report.sockets["left_hand"], "hand.L",
        "no empty: the arm bone stands in"
    );
    assert!(
        c.report
            .warnings
            .iter()
            .any(|w| w.contains("follows the `hand.L` bone's joint"))
    );
    assert_eq!(c.report.sockets["eye"], "socket_eye");
    assert_eq!(
        shape.nodes[node(shape, "Eye")].parent,
        Some(node(shape, "socket_eye"))
    );
    // Following the arm as it swings.
    let swung = animation::sample(
        shape,
        shape.animations.iter().find(|a| a.name == "armreadyright"),
        1.0,
    )
    .unwrap();
    assert!(swung.nodes[mount].w_axis.truncate().distance(at) > 0.1);

    // No eye empty: an Eye node 92% up, and a warning.
    let c = convert_glb(&character(false).glb, &body_opts()).unwrap();
    let eye = &c.shape.nodes[node(&c.shape, "Eye")];
    assert!(eye.parent.is_none() && (eye.translation[1] - 0.92 * 2.25).abs() < 1e-3);
    assert!(
        c.report
            .warnings
            .iter()
            .any(|w| w.contains("no `socket_eye`"))
    );

    // A flag names any node instead.
    let mut opts = body_opts();
    opts.right_hand = Some("ARM.r".into());
    let c = convert_glb(&figure.glb, &opts).unwrap();
    assert_eq!(c.report.sockets["right_hand"], "arm.R");
    opts.right_hand = Some("wing".into());
    let err = convert_glb(&figure.glb, &opts).err().unwrap().to_string();
    assert!(err.contains("--right-hand names `wing`"), "{err}");
}

#[test]
fn height_scales_the_whole_figure_and_its_clips() {
    let figure = character(true);
    let mut opts = body_opts();
    opts.height = Some(1.6);
    let c = convert_glb(&figure.glb, &opts).unwrap();
    assert!((c.report.height - 1.6).abs() < 1e-3);
    let k = 1.6 / 2.25;
    assert!((c.report.scale - k).abs() < 1e-4);
    let points = posed(&c.shape, Some("run"), 0.5);
    let expected = gltf_posed(
        Vec3::new(-0.4, 1.15, 0.1),
        3,
        &[
            (3, Quat::from_rotation_x(-0.6), Vec3::from(BONES[3].2)),
            (0, Quat::IDENTITY, Vec3::new(0.0, 2.0, 0.25)),
        ],
    ) * k;
    assert!(points.iter().any(|p| near(*p, expected)));
}

#[test]
fn materials_become_pngs_with_their_colour_baked_in() {
    let c = convert_glb(&character(true).glb, &body_opts()).unwrap();
    assert_eq!(
        c.report.materials,
        ["boots", "skin_tone"],
        "in the order the nodes use them"
    );
    let skin = &c.shape.materials[1];
    assert!(!skin.wrap_u && skin.wrap_v && skin.blend == "opaque");
    let png = image::load_from_memory(&c.textures["skin_tone"])
        .unwrap()
        .to_rgba8();
    assert_eq!((png.width(), png.height()), (2, 2));
    assert_eq!(png.get_pixel(0, 0).0, [255, 0, 0, 255]);
    // The boots are a plain colour: its linear factor as sRGB.
    let boots = image::load_from_memory(&c.textures["boots"])
        .unwrap()
        .to_rgba8();
    assert_eq!(boots.get_pixel(0, 0).0, [124, 89, 63, 255]);
    assert!(
        c.report
            .warnings
            .iter()
            .any(|w| w.contains("`Skin Tone`: its normal map is ignored"))
    );
}

#[test]
fn an_item_keeps_its_grip_and_muzzle_and_points_forward() {
    let doc = Document::from_bytes(&rifle(), None).unwrap();
    let opts = Options::new("ma5.glb", "out", Kind::Item);
    let c = convert(&doc, None, &opts, "ma5", "ma5_").unwrap();
    let shape = &c.shape;
    let grip = node(shape, "mountPoint");
    let muzzle = node(shape, "muzzlePoint");
    assert!(
        world(shape, grip)
            .to_scale_rotation_translation()
            .1
            .dot(Quat::IDENTITY)
            .abs()
            > 0.9999
    );
    let tip = world(shape, muzzle).w_axis.truncate();
    assert!(
        near(tip, Vec3::new(0.0, 0.05, -1.2)),
        "barrel along -Z: {tip}"
    );
    assert_eq!(
        c.report
            .points
            .iter()
            .map(|p| p.node.as_str())
            .collect::<Vec<_>>(),
        ["mountPoint", "muzzlePoint"]
    );
    assert_eq!(c.report.materials, ["ma5_gunmetal"]);
    assert_eq!(shape.materials[0].blend, "alpha");
}

#[test]
fn imports_write_a_package_the_runtime_reads() {
    let root = std::env::temp_dir().join(format!("bri-gltf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("Big Figure.glb");
    std::fs::write(&input, character(true).glb).unwrap();
    let mut opts = Options::new(&input, root.join("addons/big_figure"), Kind::Body);
    opts.name = Some("Big Figure".into());
    let report = import(&opts).unwrap();
    assert_eq!(report.id, "big_figure");
    assert_eq!(report.model, "big_figure:asset/big_figure.glb");
    let dir = root.join("addons/big_figure");
    for f in [
        "package.json",
        "assets/content.json",
        "assets/big_figure.shape.json",
        "assets/skin_tone.png",
        "assets/boots.png",
        "assets/archetypes/big_figure.json",
        "IMPORT-REPORT.md",
    ] {
        assert!(dir.join(f).is_file(), "{f}");
    }
    let archetype: bri_package_runtime::content::ArchetypeDef = serde_json::from_slice(
        &std::fs::read(dir.join("assets/archetypes/big_figure.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        archetype.model.as_deref(),
        Some("big_figure:asset/big_figure.glb")
    );
    let check = bri_package_runtime::check::check(&dir);
    assert!(check.ok, "{check}");
    let md = std::fs::read_to_string(dir.join("IMPORT-REPORT.md")).unwrap();
    assert!(
        md.contains("`Look` → `look` (role `look`)") && md.contains("overlay"),
        "{md}"
    );
    // A second import into the same folder is refused rather than mixed in.
    assert!(
        import(&opts)
            .err()
            .unwrap()
            .to_string()
            .contains("not empty")
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn what_a_model_lacks_is_named() {
    let mut g = super::testing::Gltf::new();
    g.node(json!({ "name": "Empty" }), None);
    let err = convert_glb(&g.glb(), &body_opts())
        .err()
        .unwrap()
        .to_string();
    assert_eq!(err, "the model has no visible mesh");
    let err = Document::from_bytes(br#"{"asset":{"version":"1.0"}}"#, None)
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("export glTF 2.0"), "{err}");
}

#[test]
fn a_vehicle_lists_its_seats_hubs_and_collision_hulls() {
    let mut g = super::testing::Gltf::new();
    let (p, n, uv, i) = super::testing::cuboid(Vec3::new(0.0, 0.5, 0.0), Vec3::new(2.0, 1.0, 4.0));
    let flat = |v: &[Vec3]| v.iter().flat_map(|v| v.to_array()).collect::<Vec<f32>>();
    let (a, b, c, d) = (
        g.floats(&flat(&p), 3),
        g.floats(&flat(&n), 3),
        g.floats(&uv.concat(), 2),
        g.uints(&i, 1),
    );
    let attributes = json!({ "POSITION": a, "NORMAL": b, "TEXCOORD_0": c });
    let body = g.mesh(
        "Chassis",
        vec![json!({ "attributes": attributes, "indices": d })],
    );
    let root = g.node(json!({ "name": "Chassis", "mesh": body }), None);
    g.node(
        json!({ "name": "col_body", "mesh": body, "scale": [0.5, 0.5, 0.5] }),
        Some(root),
    );
    g.node(
        json!({ "name": "mount0", "translation": [0.5, 1.0, 0.5] }),
        Some(root),
    );
    g.node(
        json!({ "name": "hub0", "translation": [1.0, 0.3, 1.5] }),
        Some(root),
    );
    g.node(json!({ "name": "headlight" }), Some(root));
    let doc = Document::from_bytes(&g.glb(), None).unwrap();
    let c = convert(
        &doc,
        None,
        &Options::new("hog.glb", "out", Kind::Prop),
        "hog",
        "hog_",
    )
    .unwrap();
    let points: Vec<_> = c
        .report
        .points
        .iter()
        .map(|p| (p.node.as_str(), p.position))
        .collect();
    assert_eq!(
        points,
        [("mount0", [-0.5, 1.0, -0.5]), ("hub0", [-1.0, 0.3, -1.5])]
    );
    assert_eq!(c.report.hulls.len(), 1);
    assert_eq!(c.report.hulls[0].object, "col_body");
    assert_eq!(c.report.hulls[0].points.len(), 8, "a box's corners");
    assert!(
        c.report.hulls[0]
            .points
            .iter()
            .all(|p| p[0].abs() == 0.5 && p[2].abs() == 1.0)
    );
    let collision = c.shape.details.iter().find(|d| d.collision).unwrap();
    assert_eq!(
        c.shape.objects[collision.object_start].name, "col_body",
        "not drawn"
    );
    assert_eq!(c.shape.materials[0].name, "hog_default");
}
