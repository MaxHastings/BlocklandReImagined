//! A converted map goes to Blender and comes back: `export-map` writes it
//! as a `.glb` and `gltf --kind map` imports that as a map Add-On which
//! stands, spawns and lights where the original did.
use bri_addon_import::gltf::{self, Kind, Options};
use bri_content::scene::Kind as NodeKind;
use bri_content::testing::map_bundle::{rooms_for, write_bundle};
use glam::Vec3;
use sha2::Digest;

const MAP: &str = "fixture/maps/hall.mis";

fn spawn(scene: &bri_content::scene::Scene) -> Vec3 {
    let n = scene
        .nodes
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Spawn))
        .unwrap();
    Vec3::new(n.transform[12], n.transform[13], n.transform[14])
}

#[test]
fn a_converted_map_round_trips_through_blender() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("bundle");
    let rooms = rooms_for(&[MAP]);
    write_bundle(&bundle, &rooms).unwrap();
    // The lamps recovered from its lightmaps, one brighter than glTF holds.
    let lamp = Vec3::from(rooms[0].lamp.position);
    let sha = format!(
        "{:x}",
        sha2::Sha256::digest(std::fs::read(bundle.join("bundle.json")).unwrap())
    );
    let params = serde_json::json!({ "schema_version": 1, "bundle_sha256": sha, "maps": { MAP: [
        { "position": lamp.to_array(), "color": [2.0, 1.0, 0.5], "inner": 3.0, "outer": 15.0 },
    ] } });
    std::fs::write(
        bundle.join("lighting-parameters.json"),
        serde_json::to_vec(&params).unwrap(),
    )
    .unwrap();

    let (glb, report) = gltf::export_map(&bundle, &rooms[0].name).unwrap();
    assert_eq!(report.map, MAP);
    assert!(report.meshes >= 2, "room and prop: {report:?}");
    assert!(report.collision_meshes >= 1, "{report:?}");
    assert_eq!(report.lights, 1);
    assert_eq!(report.spawns, 1 + rooms[0].extra_spawns.len());
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);

    let input = dir.path().join("hall-extended.glb");
    std::fs::write(&input, glb).unwrap();
    let out = dir.path().join("hall-extended");
    let imported = gltf::import(&Options::new(&input, &out, Kind::Map)).unwrap();
    let check = bri_package_runtime::check::check(&out);
    assert!(check.ok, "{check}");

    let original = bri_sim::map::NativeMap::load(&bundle, MAP).unwrap();
    let again = bri_sim::map::NativeMap::load(&out.join("map"), &imported.model).unwrap();
    assert!(
        spawn(&original.scene).distance(spawn(&again.scene)) < 1e-3,
        "{} vs {}",
        spawn(&original.scene),
        spawn(&again.scene)
    );
    assert!(!again.colliders.is_empty());
    // The floor is where it was: a ray down from the spawn meets it at the
    // same height in both.
    use rapier3d::prelude::{Ray, Vector};
    let floor = |map: &bri_sim::map::NativeMap| {
        let s = spawn(&map.scene);
        map.colliders
            .iter()
            .filter_map(|c| {
                let c = c.build();
                c.shape().cast_ray(
                    c.position(),
                    &Ray::new(Vector::new(s.x, s.y + 1.0, s.z), -Vector::Y),
                    50.0,
                    true,
                )
            })
            .fold(f32::MAX, f32::min)
    };
    assert!(
        (floor(&original) - floor(&again)).abs() < 1e-3,
        "{} vs {}",
        floor(&original),
        floor(&again)
    );

    let lights = again.scene.lights().unwrap();
    assert_eq!(lights.len(), 1);
    assert!(Vec3::from(lights[0].position).distance(lamp) < 1e-3);
    assert!((lights[0].outer - 15.0).abs() < 1e-4 && (lights[0].inner - 3.0).abs() < 1e-4);
    assert!(
        Vec3::from(lights[0].color).distance(Vec3::new(2.0, 1.0, 0.5)) < 1e-4,
        "brightness carries colour above 1: {:?}",
        lights[0].color
    );
}

#[test]
fn a_map_is_found_by_its_id_name_or_short_name() {
    let dir = tempfile::tempdir().unwrap();
    write_bundle(dir.path(), &rooms_for(&[MAP])).unwrap();
    for wanted in [MAP, "hall", "HALL"] {
        assert_eq!(
            gltf::find_map(dir.path(), wanted).unwrap().0,
            MAP,
            "{wanted}"
        );
    }
    let error = gltf::find_map(dir.path(), "kitchen").unwrap_err();
    assert!(format!("{error:#}").contains("no map `kitchen`"));
    // Without recovered lamps it still exports, and says so.
    let (_, report) = gltf::export_map(dir.path(), "hall").unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("lighting-parameters.json")),
        "{:?}",
        report.warnings
    );
}
