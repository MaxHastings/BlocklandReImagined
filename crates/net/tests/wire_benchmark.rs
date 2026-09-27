//! Wire cost of a real converted world: the MessagePack codec against the
//! JSON encoding it replaced, and the cost of copying a replica world.
//! Run: BRI_BENCH_WORLD=<path to .world.json> cargo test --release -p bri-net
//!      --test wire_benchmark -- --ignored --nocapture
use anyhow::{Context, Result};
use bri_net::protocol::{PublicWorld, public_brick};
use std::io::Read;
use std::time::Instant;

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "needs a converted world named by BRI_BENCH_WORLD"]
fn world_checkpoint_wire_cost() -> Result<()> {
    let path = std::env::var("BRI_BENCH_WORLD").context("Set BRI_BENCH_WORLD")?;
    let world = bri_world::persistence::load(std::path::Path::new(&path))?;
    let public = PublicWorld {
        name: world.name.clone(),
        map_id: world.map_id.clone(),
        palette: world.palette.clone(),
        bricks: world
            .bricks
            .iter()
            .map(|(id, b)| (*id, public_brick(b)))
            .collect(),
    };
    let start = Instant::now();
    let json = serde_json::to_vec(&public)?;
    let json_packed = zstd::stream::encode_all(json.as_slice(), 3)?;
    let json_encode = ms(start);
    let start = Instant::now();
    let mut raw = Vec::new();
    zstd::stream::read::Decoder::new(json_packed.as_slice())?.read_to_end(&mut raw)?;
    let _: PublicWorld = serde_json::from_slice(&raw)?;
    let json_decode = ms(start);

    let start = Instant::now();
    let frame = bri_net::codec::encode(&public)?;
    let wire_encode = ms(start);
    let start = Instant::now();
    let decoded: PublicWorld = bri_net::codec::decode(&frame)?;
    let wire_decode = ms(start);
    assert_eq!(decoded, public);
    let raw_wire = bri_net::codec::encode_request(&public, bri_net::codec::MAX_DECODED)?;

    let start = Instant::now();
    let _ = serde_json::to_vec(&public)?;
    let json_raw = ms(start);
    let start = Instant::now();
    let _ = bri_net::codec::encode_request(&public, bri_net::codec::MAX_DECODED)?;
    let wire_raw = ms(start);
    eprintln!("  Serialization only: JSON {json_raw:.1} ms, MessagePack {wire_raw:.1} ms");
    let start = Instant::now();
    let copy = public.clone();
    let clone = ms(start);
    drop(copy);
    eprintln!(
        "{} bricks\n  JSON: {} raw / {} zstd bytes, encode {json_encode:.1} ms, decode {json_decode:.1} ms\n  MessagePack: {} raw / {} zstd bytes, encode {wire_encode:.1} ms, decode {wire_decode:.1} ms\n  Replica world clone: {clone:.1} ms",
        public.bricks.len(),
        json.len(),
        json_packed.len(),
        raw_wire.len(),
        frame.len(),
    );
    Ok(())
}
