//! `bri-import-addon gltf --kind brick`: a made-up glTF table becomes a
//! brick Add-On the engine loads like any imported brick.
use bri_addon_import::gltf::{self, Kind, Options, testing};
use glam::Vec3;
use serde_json::json;

#[test]
fn a_gltf_model_becomes_a_brick_the_engine_loads() {
    let dir = std::env::temp_dir().join(format!("bri-gltf-brick-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // A 1 x 1 x 1 unit table: a top and one leg.
    let mut g = testing::Gltf::new();
    let flat = |v: &[Vec3]| v.iter().flat_map(|v| v.to_array()).collect::<Vec<f32>>();
    for (name, center, size) in [
        ("Top", Vec3::new(0.0, 0.9, 0.0), Vec3::new(1.0, 0.2, 1.0)),
        ("Leg", Vec3::new(0.0, 0.4, 0.0), Vec3::new(0.2, 0.8, 0.2)),
    ] {
        let (p, n, uv, i) = testing::cuboid(center, size);
        let (a, b, c, d) = (
            g.floats(&flat(&p), 3),
            g.floats(&flat(&n), 3),
            g.floats(&uv.concat(), 2),
            g.uints(&i, 1),
        );
        let mesh = g.mesh(name, vec![json!({ "attributes": { "POSITION": a, "NORMAL": b, "TEXCOORD_0": c }, "indices": d })]);
        g.node(json!({ "name": name, "mesh": mesh }), None);
    }
    let glb = dir.join("table.glb");
    std::fs::write(&glb, g.glb()).unwrap();

    let out = dir.join("table");
    gltf::import(&Options::new(&glb, &out, Kind::Brick)).unwrap();
    let catalog = out.join("assets/brick-catalog");
    let definitions = bri_sim::definitions::Definitions::load(&catalog, &catalog).unwrap();
    let table = definitions.by_id("table:brick/table").unwrap();
    // 1 unit is 2 studs wide and deep and 5 plates tall.
    assert_eq!(table.mesh.footprint_studs, [2, 2]);
    assert_eq!(table.mesh.height_plates, 5);
    assert_eq!(table.mesh.quads.len(), 24, "two cuboids' triangles");
    // No `col_` mesh: it collides as its grid box.
    assert_eq!(table.mesh.collision_boxes.len(), 1);
    assert_eq!(table.mesh.collision_boxes[0].size, [1.0, 1.0, 1.0]);
    let manifest = std::fs::read_to_string(out.join("package.json")).unwrap();
    assert!(manifest.contains("\"table:bricks/main\""), "{manifest}");
    std::fs::remove_dir_all(dir).unwrap();
}
