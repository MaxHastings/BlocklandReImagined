use super::testing::{ARMATURE_SCALE, BONES, character, rifle, room};
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

/// A clip's channel that holds a bone at rest still overrides the clips
/// below it: aiming holds the left hand still, so the run's swing of that
/// hand does not show through (Mario's weapon hold broke this way). Bones
/// the clip has no channel for still follow the run.
#[test]
fn a_channel_held_at_rest_still_overrides_the_layer_below() {
    use bri_content::animation::{Layer, sample, sample_layers};
    let c = convert_glb(&character(true).glb, &body_opts()).unwrap();
    let clip = |name: &str| c.shape.animations.iter().find(|a| a.name == name).unwrap();
    let (run, aim) = (clip("run"), clip("armreadyright"));
    assert!(
        aim.nodes
            .iter()
            .any(|t| t.node == "hand.L" && !t.rotations.is_empty()),
        "the still hand's channel is kept: {:?}",
        aim.nodes.iter().map(|t| &t.node).collect::<Vec<_>>()
    );
    let hand = c
        .shape
        .nodes
        .iter()
        .position(|n| n.name == "hand.L")
        .unwrap();
    let hips = c.shape.nodes.iter().position(|n| n.name == "hips").unwrap();
    let rest = sample(&c.shape, None, 0.0).unwrap();
    let running = sample(&c.shape, Some(run), 0.1).unwrap();
    let aiming = sample_layers(
        &c.shape,
        &[
            Layer {
                animation: run,
                time: 0.1,
                weight: 1.0,
            },
            Layer {
                animation: aim,
                time: aim.duration,
                weight: 1.0,
            },
        ],
    )
    .unwrap();
    let turn = |pose: &bri_content::animation::Pose, n: usize| {
        let (_, r, _) = pose.nodes[n].to_scale_rotation_translation();
        r
    };
    assert!(
        turn(&running, hand).dot(turn(&rest, hand)).abs() < 0.999,
        "the run swings the left hand"
    );
    assert!(
        turn(&aiming, hand).dot(turn(&rest, hand)).abs() > 0.9999,
        "aiming holds it at rest"
    );
    assert!(
        running.nodes[hips]
            .w_axis
            .distance(aiming.nodes[hips].w_axis)
            < 1e-4,
        "the hips still follow the run"
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
    // Its normal map comes along as a helper material the surface names.
    let surface = skin
        .surface
        .as_ref()
        .expect("the skin keeps its normal map");
    let normal = surface.normal.expect("a normal map");
    assert_eq!(c.shape.materials[normal].name, "skin_tone_normal");
    assert!(c.textures.contains_key("skin_tone_normal"));
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
    assert_eq!(report.model, "big_figure:body/big_figure");
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
        Some("big_figure:body/big_figure")
    );
    // Its collision box is its own size, not the Blockhead's: as tall as
    // the model, as wide for that height as a Blockhead, the eye at its
    // eye socket.
    let m = &archetype.movement;
    let get = |k: &str| m[k].as_f64().unwrap() as f32;
    assert!((get("stand_height") - report.height).abs() < 0.01, "{m:?}");
    assert!(
        (get("width") - report.height * 1.25 / 2.65).abs() < 0.01,
        "{m:?}"
    );
    assert!(
        get("stand_eye") > report.height * 0.7 && get("stand_eye") < report.height,
        "{m:?}"
    );
    assert!(get("crouch_height") < get("stand_height") && get("sit_eye") < get("stand_eye"));
    let check = bri_package_runtime::check::check(&dir);
    assert!(check.ok, "{check}");
    // Everyone draws it, so joiners download it: shared, not host-only.
    let info: bri_package::library::PackageInfo =
        serde_json::from_slice(&std::fs::read(dir.join("package.json")).unwrap()).unwrap();
    assert_eq!(info.side(), Some(bri_package::packages::Side::Shared));
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
fn a_vehicle_lists_its_seats_hubs_gun_and_collision_hulls() {
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
    g.node(
        json!({ "name": "pivot", "translation": [0.0, 1.5, -1.0] }),
        Some(root),
    );
    g.node(
        json!({ "name": "muzzlePoint", "translation": [0.0, 1.6, 0.0] }),
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
        [
            ("mount0", [-0.5, 1.0, -0.5]),
            ("hub0", [-1.0, 0.3, -1.5]),
            ("pivot", [0.0, 1.5, 1.0]),
            ("muzzlePoint", [0.0, 1.6, 0.0]),
        ]
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

/// The posed vertices of `detail`, in the idle clip's middle.
fn posed_detail(shape: &Shape, detail: usize) -> Vec<Vec3> {
    let clip = shape.animations.iter().find(|a| a.name == "root");
    let pose = animation::sample(shape, clip, 0.5).unwrap();
    animation::triangles(shape, &pose, detail, |_| true)
        .unwrap()
        .iter()
        .flat_map(|t| t.vertices.iter().map(|v| v.position).collect::<Vec<_>>())
        .collect()
}

#[test]
fn a_heavy_body_gets_a_distant_detail_that_keeps_its_skin_and_sockets() {
    let figure = super::testing::detailed_character(true, 12);
    let mut opts = body_opts();
    opts.lod = Some(8652 / 4);
    let c = convert_glb(&figure.glb, &opts).unwrap();
    let full: usize = c.report.triangles;
    assert!(full > 8000, "{full}");
    let lod = c.report.lod.as_ref().expect("a distant detail");
    assert!(!lod.hand_made);
    assert!(
        lod.triangles <= full / 4 + full / 20,
        "{} triangles of {full}",
        lod.triangles
    );
    let s = &c.shape;
    let visible: Vec<_> = s.details.iter().filter(|d| !d.collision).collect();
    assert_eq!(visible.len(), 2);
    assert_eq!(
        (visible[0].pixel_threshold, visible[0].mesh_offset),
        (LOD_PIXELS, 0)
    );
    assert_eq!(
        (visible[1].pixel_threshold, visible[1].mesh_offset),
        (0.0, 1)
    );
    for o in &s.objects {
        assert_eq!(o.meshes.len(), 2, "{} has both details", o.name);
    }
    let body = s.objects.iter().find(|o| o.name == "Body").unwrap();
    let low = s.meshes[body.meshes[1]].as_ref().unwrap();
    let skin = low.skin.as_ref().expect("the distant detail is skinned");
    let mut total = vec![0.0f32; low.frame_vertices];
    for i in &skin.influences {
        total[i.vertex] += i.weight;
    }
    assert!(
        total.iter().all(|w| (w - 1.0).abs() < 1e-4),
        "every vertex weighted"
    );
    for socket in ["Mount0", "Mount1", "Eye"] {
        node(s, socket);
    }
    // Posed, the distant detail covers the same figure: every one of its
    // vertices is a vertex of the full one, and it reaches the same extent.
    let (near_full, far) = (posed_detail(s, 0), posed_detail(s, 1));
    let extent = |v: &[Vec3]| {
        v.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        })
    };
    let (a, b) = (extent(&near_full), extent(&far));
    assert!(
        a.0.distance(b.0) < 0.02 && a.1.distance(b.1) < 0.02,
        "{a:?} {b:?}"
    );
    assert!(far.iter().all(|p| near_full.iter().any(|q| near(*p, *q))));
}

#[test]
fn a_distant_detail_is_made_only_when_asked_for() {
    let heavy = super::testing::detailed_character(true, 12);
    let mut opts = body_opts();
    for lod in [None, Some(0)] {
        opts.lod = lod;
        let c = convert_glb(&heavy.glb, &opts).unwrap();
        assert!(c.report.lod.is_none());
        assert_eq!(c.shape.details.iter().filter(|d| !d.collision).count(), 1);
    }
    opts.lod = Some(600);
    opts.lod_pixels = 150.0;
    let c = convert_glb(&heavy.glb, &opts).unwrap();
    let lod = c.report.lod.unwrap();
    assert!(lod.triangles <= 700, "{}", lod.triangles);
    assert_eq!(c.shape.details[0].name, "detail150");
}

#[test]
fn hand_made_lod1_objects_stand_in_at_a_distance() {
    let mut g = super::testing::Gltf::new();
    let flat = |v: &[Vec3]| v.iter().flat_map(|v| v.to_array()).collect::<Vec<f32>>();
    let mesh = |g: &mut super::testing::Gltf, name: &str, size: f32| {
        let (p, n, uv, i) = super::testing::cuboid(Vec3::ZERO, Vec3::splat(size));
        let (a, b, c, d) = (
            g.floats(&flat(&p), 3),
            g.floats(&flat(&n), 3),
            g.floats(&uv.concat(), 2),
            g.uints(&i, 1),
        );
        g.mesh(
            name,
            vec![json!({ "attributes": { "POSITION": a, "NORMAL": b, "TEXCOORD_0": c }, "indices": d })],
        )
    };
    let (body, low, hat) = (
        mesh(&mut g, "Body", 1.0),
        mesh(&mut g, "Body_LOD1", 0.8),
        mesh(&mut g, "Hat", 0.3),
    );
    g.node(
        json!({ "name": "Body", "mesh": body, "translation": [0.0, 0.5, 0.0] }),
        None,
    );
    g.node(
        json!({ "name": "Body_LOD1", "mesh": low, "translation": [0.0, 1.5, 0.0] }),
        None,
    );
    g.node(
        json!({ "name": "Hat", "mesh": hat, "translation": [0.0, 1.2, 0.0] }),
        None,
    );
    g.node(json!({ "name": "Shoe_LOD1", "mesh": hat }), None);
    let c = convert_glb(&g.glb(), &body_opts()).unwrap();
    let lod = c.report.lod.as_ref().unwrap();
    assert!(lod.hand_made);
    assert_eq!(
        c.report.triangles, 24,
        "the twins are not counted as full detail"
    );
    assert!(c.report.warnings.iter().any(|w| w.contains("Shoe_LOD1")));
    let s = &c.shape;
    let names: Vec<_> = s.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["Body", "Hat"]);
    let hat = &s.objects[1];
    assert_eq!(hat.meshes[0], hat.meshes[1], "no twin: draws as it is");
    let body = &s.objects[0];
    let far = s.meshes[body.meshes[1]].as_ref().unwrap();
    // The twin sits a unit higher than the body; its vertices move into
    // the body's frame so it draws where the twin was modelled.
    let top = far.positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    assert!((top - 1.4).abs() < 1e-4, "{top}");
}

#[test]
fn a_bodys_box_can_be_set() {
    let mut opts = body_opts();
    opts.height = Some(1.6);
    opts.box_width = Some(0.74);
    opts.box_height = Some(1.5);
    let c = convert_glb(&character(true).glb, &opts).unwrap();
    let m = c.report.movement.unwrap();
    assert_eq!((m.width, m.stand_height), (0.74, 1.5));
    assert!(c.report.markdown().contains("0.74 wide, 1.5 tall"));
    opts.box_width = Some(0.0);
    assert!(convert_glb(&character(true).glb, &opts).is_err());
}

/// `--kind map`: an Add-On the map list, the collision loader and the
/// renderer all read, with its spawn and lights where Blender put them.
fn import_room(col_floor: bool) -> (tempfile::TempDir, Report) {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("Test Room.glb");
    std::fs::write(&input, room(col_floor)).unwrap();
    let opts = Options::new(&input, root.path().join("test_room"), Kind::Map);
    let report = import(&opts).unwrap();
    (root, report)
}

#[test]
fn a_map_imports_as_an_add_on_the_game_loads() {
    let (root, report) = import_room(false);
    let dir = root.path().join("test_room");
    assert_eq!(report.id, "test_room");
    assert_eq!(report.model, "test_room:map/test_room");
    for f in [
        "package.json",
        "map/bundle.json",
        "map/test_room.scene.json",
        "map/test_room.shape.json",
        "map/test_room_wood.png",
        "IMPORT-REPORT.md",
    ] {
        assert!(dir.join(f).is_file(), "{f}: {:?}", report.files);
    }
    let check = bri_package_runtime::check::check(&dir);
    assert!(check.ok, "{check}");
    let info: bri_package::library::PackageInfo =
        serde_json::from_slice(&std::fs::read(dir.join("package.json")).unwrap()).unwrap();
    assert_eq!(info.side(), Some(bri_package::packages::Side::Shared));

    // The map list finds it beside the base game's maps.
    let base = root.path().join("base");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(
        base.join("bundle.json"),
        r#"{"schema_version":1,"maps":[]}"#,
    )
    .unwrap();
    let catalog = bri_sim::map_catalog::MapCatalog::new(
        &base,
        &[bri_sim::map_catalog::MapProvider {
            package: "test_room".into(),
            id: report.model.clone(),
            bundle: dir.join("map/bundle.json"),
        }],
    )
    .unwrap();
    let map_root = catalog.root(&report.model).unwrap();
    assert_eq!(catalog.get(&report.model).unwrap().name, "Test Room");

    // Players stand on the floor (no `col_` meshes: solid where drawn) and
    // start at the spawn Empty, half a turn about Y like every model.
    let native = bri_sim::map::NativeMap::load(map_root, &report.model).unwrap();
    // The level blocks bricks like an interior, not like a prop.
    assert!(!native.colliders.is_empty());
    for c in &native.colliders {
        assert_eq!(
            bri_sim::map::MapSurface::from_tag(c.user_data),
            Some(bri_sim::map::MapSurface::Interior)
        );
    }
    let scene = &native.scene;
    let spawn = scene
        .nodes
        .iter()
        .find(|n| matches!(n.kind, bri_content::scene::Kind::Spawn))
        .unwrap();
    assert!(near(
        Vec3::new(
            spawn.transform[12],
            spawn.transform[13],
            spawn.transform[14]
        ),
        Vec3::new(-2.0, 0.0, -3.0)
    ));
    // The teleporter pair, turned with the model.
    let teleporters = scene.teleporters().unwrap();
    assert_eq!(teleporters.len(), 2, "{teleporters:?}");
    let (entrance, exit) = (&teleporters[0], &teleporters[1]);
    assert!(entrance.entrance && !exit.entrance);
    assert_eq!(
        (entrance.channel.as_str(), exit.channel.as_str()),
        ("a", "a")
    );
    assert!((entrance.radius - 1.5).abs() < 1e-5 && (exit.radius - 1.0).abs() < 1e-5);
    assert!(near(
        Vec3::from(entrance.position),
        Vec3::new(5.0, 0.0, 5.0)
    ));
    assert!(near(Vec3::from(exit.position), Vec3::new(-5.0, 0.0, -5.0)));
    // glTF +X is native -X after the half turn: a yaw of minus a quarter.
    assert!(
        (exit.yaw + std::f32::consts::FRAC_PI_2).abs() < 1e-4,
        "{}",
        exit.yaw
    );
    assert_eq!(native.teleporters.len(), 2);
    let lights = scene.lights().unwrap();
    assert_eq!(lights.len(), 2, "{lights:?}");
    let lamp = lights
        .iter()
        .find(|l| scene.nodes[l.node].name == "Lamp")
        .unwrap();
    assert_eq!(lamp.kind, "strobe");
    assert!(near(Vec3::from(lamp.position), Vec3::new(0.0, 3.0, 0.0)));
    assert!((lamp.outer - 8.0).abs() < 1e-4);
    assert!(
        (lamp.color[0] - 1.5).abs() < 1e-4,
        "brightness scales colour"
    );
    let lamp_node = scene.nodes.iter().find(|n| n.name == "Lamp").unwrap();
    assert_eq!(lamp_node.properties["speed"], "2");
    let spot = lights
        .iter()
        .find(|l| scene.nodes[l.node].name == "Spot")
        .unwrap();
    assert_eq!(spot.kind, "spot");
    assert!((spot.outer - 6.0).abs() < 1e-4, "glTF range is the radius");
    let spot_node = scene.nodes.iter().find(|n| n.name == "Spot").unwrap();
    let cone: f32 = spot_node.properties["outer_cone"].parse().unwrap();
    assert!((cone - 0.6f32.to_degrees()).abs() < 1e-3);

    use rapier3d::prelude::{Ray, Vector};
    let floor = native
        .colliders
        .iter()
        .filter_map(|c| {
            let c = c.build();
            c.shape().cast_ray(
                c.position(),
                &Ray::new(Vector::new(2.0, 3.0, 2.0), -Vector::Y),
                10.0,
                true,
            )
        })
        .fold(f32::MAX, f32::min);
    assert!(
        (floor - 3.0).abs() < 0.01,
        "players stand on the floor: {floor}"
    );

    // Each material's `bri_surface` is what its triangles are made of.
    let kind_at = |from: Vector, to: Vector| {
        native
            .colliders
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let c = c.build();
                let hit = c.shape().cast_ray_and_get_normal(
                    c.position(),
                    &Ray::new(from, to),
                    20.0,
                    true,
                )?;
                Some((hit.time_of_impact, native.surfaces.kind(i, hit.subshape)))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .and_then(|(_, kind)| kind)
    };
    assert_eq!(
        kind_at(Vector::new(2.0, 3.0, 2.0), -Vector::Y),
        Some("dirt")
    );
    assert_eq!(
        kind_at(Vector::new(2.0, 2.0, 2.0), Vector::X),
        Some("metal")
    );

    // The renderer draws it with its lights in Dynamic.
    let drawn = bri_render::scene_loader::load_map_bundle_dynamic(map_root, &report.model).unwrap();
    assert!(!drawn.scene.vertices.is_empty());
    let modern = drawn.modern_lights.unwrap();
    assert_eq!(modern.len(), 2);
    assert!(
        modern
            .iter()
            .any(|l| near(Vec3::from(l.position), Vec3::new(0.0, 3.0, 0.0)))
    );

    // The strobe blinks: a light animation for the lamp's light.
    assert_eq!(drawn.light_animations.len(), 1);
    let (lamp, strobe) = drawn.light_animations[0];
    assert!(near(
        Vec3::from(modern[lamp].position),
        Vec3::new(0.0, 3.0, 0.0)
    ));
    assert_ne!(strobe.tint_at(0.1), strobe.tint_at(0.6));

    let md = std::fs::read_to_string(dir.join("IMPORT-REPORT.md")).unwrap();
    assert!(md.contains("(map)"), "{md}");
    // The spot light shines down its cone.
    assert_eq!(drawn.light_spots.len(), 1);
}

#[test]
fn a_map_without_a_spawn_or_with_col_meshes_says_what_it_did() {
    let (root, report) = import_room(true);
    let shape: Shape = serde_json::from_slice(
        &std::fs::read(root.path().join("test_room/map/test_room.shape.json")).unwrap(),
    )
    .unwrap();
    let collision = shape.details.iter().find(|d| d.collision).unwrap();
    assert_eq!(collision.object_count, 1, "only col_floor collides");
    assert!(
        report
            .warnings
            .iter()
            .all(|w| !w.contains("no Empty named"))
    );

    let mut g = super::testing::Gltf::new();
    let (p, _, _, i) = super::testing::cuboid(Vec3::ZERO, Vec3::new(4.0, 1.0, 4.0));
    let flat: Vec<f32> = p.iter().flat_map(|v| v.to_array()).collect();
    let (a, b) = (g.floats(&flat, 3), g.uints(&i, 1));
    let mesh = g.mesh(
        "Slab",
        vec![json!({ "attributes": { "POSITION": a }, "indices": b })],
    );
    g.node(json!({ "name": "Slab", "mesh": mesh }), None);
    let input = root.path().join("slab.glb");
    std::fs::write(&input, g.glb()).unwrap();
    let report = import(&Options::new(&input, root.path().join("slab"), Kind::Map)).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("no Empty named `spawn`")),
        "{:?}",
        report.warnings
    );
}

/// A map brings its own sun, sky and water: a Blender Sun light, the
/// scene's `bri_sky_color` and `bri_fog`, a `--sky` folder, and a mesh
/// named `water` that becomes a body of water instead of a wall.
#[test]
fn a_map_brings_its_own_sun_sky_and_water() {
    let root = tempfile::tempdir().unwrap();
    let mut g = super::testing::Gltf::new();
    let flat = |v: &[Vec3]| v.iter().flat_map(|v| v.to_array()).collect::<Vec<f32>>();
    let part = |g: &mut super::testing::Gltf,
                name: &str,
                center: Vec3,
                size: Vec3,
                extras: Value| {
        let (p, n, uv, i) = super::testing::cuboid(center, size);
        let (a, b, c, d) = (
            g.floats(&flat(&p), 3),
            g.floats(&flat(&n), 3),
            g.floats(&uv.concat(), 2),
            g.uints(&i, 1),
        );
        let lava = g.material("Lava", [0.9, 0.3, 0.1, 0.5], None, json!({}));
        let mesh = g.mesh(name, vec![json!({ "attributes": { "POSITION": a, "NORMAL": b, "TEXCOORD_0": c }, "indices": d, "material": lava })]);
        g.node(
            json!({ "name": name, "mesh": mesh, "extras": extras }),
            None,
        );
    };
    part(
        &mut g,
        "Floor",
        Vec3::new(0.0, -0.25, 0.0),
        Vec3::new(20.0, 0.5, 20.0),
        json!({}),
    );
    part(
        &mut g,
        "Water.001",
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(10.0, 0.0, 10.0),
        json!({ "bri_liquid": "lava" }),
    );
    g.node(
        json!({ "name": "spawn", "translation": [8.0, 0.0, 8.0] }),
        None,
    );
    g.json["extensionsUsed"] = json!(["KHR_lights_punctual"]);
    g.json["extensions"] = json!({ "KHR_lights_punctual": { "lights": [
        { "name": "Sun", "type": "directional", "color": [1.0, 0.9, 0.8] },
    ] } });
    // Shining down at 45 degrees towards glTF -Z (Blender +Y): from the
    // Torque north, azimuth 0.
    g.node(json!({ "name": "Sun", "rotation": Quat::from_rotation_x(-std::f32::consts::FRAC_PI_4).to_array(),
        "extensions": { "KHR_lights_punctual": { "light": 0 } }, "extras": { "bri_ambient": 0.3 } }), None);
    g.json["scenes"][0]["extras"] = json!({ "bri_sky_color": [0.2, 0.3, 0.4], "bri_fog": [100.0, 400.0],
                "bri_fog_density_end": 200.0, "bri_fog_max": 0.6, "bri_fog_height": 20.0 });
    let input = root.path().join("pond.glb");
    std::fs::write(&input, g.glb()).unwrap();
    let sky = root.path().join("sky");
    std::fs::create_dir_all(&sky).unwrap();
    for (k, face) in super::map_environment::SKY_FACES.iter().enumerate() {
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([k as u8 * 40, 100, 200, 255]));
        img.save(sky.join(format!("{face}.png"))).unwrap();
    }
    let mut opts = Options::new(&input, root.path().join("pond"), Kind::Map);
    opts.sky = Some(sky);
    let report = import(&opts).unwrap();
    let dir = root.path().join("pond");
    let check = bri_package_runtime::check::check(&dir);
    assert!(check.ok, "{check}");

    let native = bri_sim::map::NativeMap::load(&dir.join("map"), &report.model).unwrap();
    let sun = native
        .scene
        .nodes
        .iter()
        .find(|n| matches!(n.kind, bri_content::scene::Kind::Sun))
        .unwrap();
    let number = |k: &str| sun.properties[k].parse::<f32>().unwrap();
    assert!(
        (number("elevation") - 45.0).abs() < 0.01,
        "{:?}",
        sun.properties
    );
    assert!(
        number("azimuth") < 0.01 || number("azimuth") > 358.99,
        "{:?}",
        sun.properties
    );
    assert_eq!(sun.properties["color"], "1 0.9 0.8");
    assert_eq!(sun.properties["ambient"], "0.3 0.3 0.3");

    // The water is a lava pool from its surface to the floor, not a wall.
    assert_eq!(native.waters.len(), 1);
    let water = &native.waters[0];
    assert_eq!(water.liquid_type, "Lava");
    assert!(
        (water.max[1] - 1.0).abs() < 0.01 && water.min[1] <= -0.4,
        "{water:?}"
    );
    assert!((water.opacity - 0.5).abs() < 0.01);
    use rapier3d::prelude::{Ray, Vector};
    let ground = native
        .colliders
        .iter()
        .filter_map(|c| {
            let c = c.build();
            c.shape().cast_ray(
                c.position(),
                &Ray::new(Vector::new(0.0, 5.0, 0.0), -Vector::Y),
                10.0,
                true,
            )
        })
        .fold(f32::MAX, f32::min);
    assert!(
        (ground - 5.0).abs() < 0.01,
        "water does not collide: {ground}"
    );

    // The sky: its six pictures, colour and fog.
    let bundle: Value =
        serde_json::from_slice(&std::fs::read(dir.join("map/bundle.json")).unwrap()).unwrap();
    let environment: bri_content::environment::Environment =
        serde_json::from_value(bundle["environments"][&report.model].clone()).unwrap();
    environment.validate().unwrap();
    assert_eq!(environment.faces.len(), 6);
    assert!(
        environment
            .faces
            .iter()
            .all(|f| dir.join("map").join(&f.file).is_file())
    );
    assert_eq!(environment.solid_color, [0.2, 0.3, 0.4]);
    assert_eq!((environment.fog.start, environment.fog.end), (100.0, 400.0));
    assert_eq!(environment.fog.density_end, Some(200.0));
    assert_eq!((environment.fog.max, environment.fog.height), (0.6, 20.0));

    // The renderer draws the map with its water and sky.
    let drawn =
        bri_render::scene_loader::load_map_bundle_dynamic(&dir.join("map"), &report.model).unwrap();
    assert!(!drawn.scene.vertices.is_empty());
    assert!(
        drawn
            .scene
            .materials
            .iter()
            .any(|m| m.kind == bri_render::scene::MaterialKind::Water)
    );
}

/// A stated metallic factor makes bare metal at the material's roughness,
/// a strong glow draws self-lit, a faint one glows on a lit surface, and a
/// plain surface stays as it was.
#[test]
fn metal_and_glow_come_from_the_material() {
    let doc = super::read::Document::from_bytes(
        serde_json::to_string(&json!({
            "asset": { "version": "2.0" },
            "materials": [
                { "name": "Steel", "pbrMetallicRoughness": { "metallicFactor": 1.0, "roughnessFactor": 0.3 } },
                { "name": "Paint", "pbrMetallicRoughness": { "metallicFactor": 0.0 } },
                { "name": "Lamp", "emissiveFactor": [1.0, 0.9, 0.5] },
                { "name": "Ember", "emissiveFactor": [0.2, 0.0, 0.0] },
                {"name":"Panel","extras":{"bri_uv_motion":{"center":[0.5,0.5],"rotation_rate_degrees":180.0}}},
                {"name":"Invalid motion","extras":{"bri_uv_motion":{"rotation_rate_degrees":1e9}}},
            ]
        }))
        .unwrap()
        .as_bytes(),
        None,
    )
    .unwrap();
    let mut report = Report::default();
    let mut taken = BTreeSet::new();
    let mut get = |i| {
        super::make_material(&doc, None, Some(i), "", &mut taken, &mut report)
            .unwrap()
            .0
    };
    let (steel, paint, lamp, ember) = (get(0), get(1), get(2), get(3));
    let panel = get(4);
    let motion = panel
        .uv_motion
        .expect("authored texture motion survives import");
    assert_eq!(motion.center, [0.5; 2]);
    assert_eq!(motion.rotation_rate_degrees, 180.);
    assert!(
        super::make_material(&doc, None, Some(5), "", &mut taken, &mut report).is_err(),
        "invalid motion must be reported, never silently ignored"
    );
    let metal = steel.metal.expect("steel is metal");
    assert_eq!((metal.color, metal.roughness), ([1.0; 3], 0.3));
    assert!(paint.metal.is_none() && !paint.unlit);
    assert!(lamp.unlit && lamp.metal.is_none());
    // A faint glow lights the surface from within instead.
    assert!(!ember.unlit);
    let glow = ember.surface.expect("ember keeps its glow");
    assert_eq!(glow.emissive, [0.2, 0.0, 0.0]);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

/// A `--terrain` folder becomes the native heightmap terrain: shaped as its
/// picture is from above in Blender, sized and placed by the scene's
/// properties, with its ground textures where their masks say, and lit.
#[test]
fn a_map_brings_its_own_heightmap_terrain() {
    let root = tempfile::tempdir().unwrap();
    let mut g = super::testing::Gltf::new();
    let (p, n, uv, i) = super::testing::cuboid(Vec3::new(0.0, 6.0, 0.0), Vec3::splat(2.0));
    let flat = |v: &[Vec3]| v.iter().flat_map(|v| v.to_array()).collect::<Vec<f32>>();
    let (a, b, c, d) = (
        g.floats(&flat(&p), 3),
        g.floats(&flat(&n), 3),
        g.floats(&uv.concat(), 2),
        g.uints(&i, 1),
    );
    let stone = g.material("Stone", [0.5, 0.5, 0.5, 1.0], None, json!({}));
    let mesh = g.mesh("Rock", vec![json!({ "attributes": { "POSITION": a, "NORMAL": b, "TEXCOORD_0": c }, "indices": d, "material": stone })]);
    g.node(json!({ "name": "Rock", "mesh": mesh }), None);
    g.json["scenes"][0]["extras"] = json!({
        "bri_terrain_size": 64.0, "bri_terrain_height": 10.0, "bri_terrain_center": [0.0, 0.0, -5.0],
    });
    let input = root.path().join("hills.glb");
    std::fs::write(&input, g.glb()).unwrap();
    // Rising towards Blender's +X (the picture's right); grass everywhere,
    // sand over the picture's top half (Blender's +Y).
    let dir = root.path().join("terrain");
    std::fs::create_dir_all(&dir).unwrap();
    image::GrayImage::from_fn(32, 32, |x, _| image::Luma([(x * 255 / 31) as u8]))
        .save(dir.join("height.png"))
        .unwrap();
    image::RgbImage::from_pixel(8, 8, image::Rgb([40, 160, 40]))
        .save(dir.join("layer0.png"))
        .unwrap();
    image::RgbImage::from_pixel(8, 8, image::Rgb([220, 200, 120]))
        .save(dir.join("layer1.jpg"))
        .unwrap();
    image::GrayImage::from_fn(32, 32, |_, y| image::Luma([if y < 16 { 255 } else { 0 }]))
        .save(dir.join("weight1.png"))
        .unwrap();
    let mut opts = Options::new(&input, root.path().join("hills"), Kind::Map);
    opts.terrain = Some(dir);
    let report = import(&opts).unwrap();
    let out = root.path().join("hills");
    let check = bri_package_runtime::check::check(&out);
    assert!(check.ok, "{check}");

    let native = bri_sim::map::NativeMap::load(&out.join("map"), &report.model).unwrap();
    assert_eq!(native.terrain.len(), 1);
    let field = &native.terrain[0];
    // Blender (x, y) is native (-x, -y) on the ground.
    let at = |x: f32, y: f32| field.height(-x, -y);
    let (east, west) = (at(30.0, 0.0).unwrap(), at(-30.0, 0.0).unwrap());
    assert!(
        (east - 4.7).abs() < 0.3 && (west + 4.7).abs() < 0.3,
        "{east} {west}"
    );
    assert!(at(40.0, 0.0).is_none(), "nothing past the picture's edge");
    let north = |x: usize, y: usize| field.terrain.layers[1].weights[y * 32 + x];
    assert_eq!((north(3, 5), north(3, 25)), (255, 0));

    // It draws in both lighting modes, with its baked sunlight.
    let bundle: Value =
        serde_json::from_slice(&std::fs::read(out.join("map/bundle.json")).unwrap()).unwrap();
    let light = &bundle["lighting"][&report.model]["terrain"][0];
    assert!(
        out.join("map")
            .join(light["file"].as_str().unwrap())
            .is_file()
    );
    for drawn in [
        bri_render::scene_loader::load_map_bundle(&out.join("map"), &report.model).unwrap(),
        bri_render::scene_loader::load_map_bundle_dynamic(&out.join("map"), &report.model).unwrap(),
    ] {
        assert_eq!(drawn.terrain.len(), 1);
    }
}
