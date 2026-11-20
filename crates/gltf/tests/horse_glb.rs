//! The horse's shipped GLB must lower into a shape the client can paint: its
//! drawn objects carry exactly one `body` and one `head`, every other drawn
//! object being a hidden ski. The glTF loader uniques repeated node names, so
//! each may arrive suffixed (`body_2`) — the client matches by base name.
//! Skipped where the content is absent.

#[test]
fn the_horse_glb_draws_paintable_objects() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/default-pack/vehicles-pack-012/models/horse.glb");
    if !path.is_file() {
        eprintln!("skipping: no horse.glb at {}", path.display());
        return;
    }
    let bytes = std::fs::read(&path).unwrap();
    // Lowered exactly as `AvatarAssets::load_horse` does.
    let converted = bri_gltf::lower(&bytes, "v20.vehicle.horsearmor").unwrap();
    let shape = &converted.shape;
    shape.validate().unwrap();

    /// A glTF loader's unique name back to the node it was named after: a
    /// trailing `_N` is dropped.
    fn base(name: &str) -> &str {
        match name.rsplit_once('_') {
            Some((b, s)) if !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()) => b,
            _ => name,
        }
    }

    // The detail `load_horse` draws: the first non-collision one.
    let index = shape
        .details
        .iter()
        .position(|d| !d.collision)
        .expect("the horse has no visible detail");
    let d = &shape.details[index];
    let objects = &shape.objects[d.object_start..d.object_start + d.object_count];
    assert!(!objects.is_empty());

    let mut bodies = 0;
    let mut heads = 0;
    for object in objects {
        let name = object.name.to_ascii_lowercase();
        // Every drawn object must carry geometry, or it draws nothing.
        assert!(
            object.meshes.iter().any(|m| shape.meshes[*m].is_some()),
            "drawn object {:?} has no mesh",
            name,
        );
        match base(&name) {
            "body" => bodies += 1,
            "head" => heads += 1,
            "rski" | "lski" => {} // the hidden ski nodes
            other => panic!(
                "drawn object {:?} is not a body, head or ski (the client would leave it unpainted)",
                other
            ),
        }
    }
    assert_eq!(bodies, 1, "exactly one body object");
    assert_eq!(heads, 1, "exactly one head object");

    // `vehicles.json` points the horse's `death1` alias at a clip of that
    // name; if a re-export dropped or renamed it, dead horses would silently
    // play nothing again.
    let death = shape
        .animations
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("death1"))
        .expect("the horse.glb carries its authored death clip");
    assert!(
        !death.nodes.is_empty(),
        "the death clip must keep its tracks (an empty one poses nothing)"
    );
}
