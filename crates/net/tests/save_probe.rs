//! `cargo test --release -p bri-net --test save_probe -- --ignored --nocapture`
use bri_net::codec;
use bri_world::{Brick, ContentRef, World, build::SavedBuild};
fn big_world(count: u64) -> World {
    let mut world = World::new("Big".into(), "test".into(), vec![[1.0; 4], [0.5; 4]]);
    for id in 1..=count {
        let (x, z) = ((id % 180) as f32, (id / 180 % 360) as f32 * 0.5);
        let layer = (id / (180 * 360)) as f32;
        let mut brick = Brick::new(
            ContentRef::Resolved("plate".into()),
            [-90.0 + x, 0.1 + 0.2 * layer, 5.25 + z],
            1 + id % 8,
        );
        brick.color = (id % 2) as u8;
        world.bricks.insert(id, brick);
    }
    world.next_brick_id = count + 1;
    world
}
#[test]
#[ignore = "timing probe"]
fn save_probe() {
    let world = big_world(200_000);
    let t = std::time::Instant::now();
    let w2 = world.clone();
    let clone = t.elapsed();
    let t = std::time::Instant::now();
    let build = SavedBuild::capture(&w2, true, true).unwrap();
    let capture_keep = t.elapsed();
    let t = std::time::Instant::now();
    let stripped = SavedBuild::capture(&w2, false, false).unwrap();
    let capture_strip = t.elapsed();
    let t = std::time::Instant::now();
    build.validate().unwrap();
    let validate = t.elapsed();
    let t = std::time::Instant::now();
    let bytes = codec::encode(&bri_net::protocol::Message::Reply {
        sequence: 1,
        result: Ok(bri_sim::session::Reply::Saved(Box::new(build))),
    })
    .unwrap();
    let encode = t.elapsed();
    println!(
        "200k bricks: clone {:?}, capture(keep all) {:?}, capture(strip) {:?}, validate {:?}, encode reply {:?} ({} bytes); stripped {} bricks",
        clone,
        capture_keep,
        capture_strip,
        validate,
        encode,
        bytes.len(),
        stripped.world.bricks.len()
    );
}
