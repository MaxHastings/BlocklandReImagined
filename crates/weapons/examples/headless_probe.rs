use anyhow::{Context, Result};
use bri_weapons::*;
use glam::Vec3;
struct Empty;
impl Query for Empty {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        vec![]
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(
        args.len() == 2,
        "Usage: headless_probe PACK_JSON REPORT_JSON"
    );
    let pack = Pack::from_json(&std::fs::read(&args[0])?)?;
    let mut world = WeaponsWorld::new(pack)?;
    for actor in 0..8 {
        world.add_actor(ActorId(actor), 5)?;
    }
    for p in 0..1024 {
        world.spawn(
            &native_id("projectile", "pongProjectile"),
            ActorId(p % 8),
            Vec3::new(p as f32, 100.0, 0.0),
            Vec3::NEG_Z * 65.0,
            1.0,
        )?;
    }
    let mut durations = Vec::new();
    let mut events = 0;
    let mut queries = Empty;
    for _ in 0..1200 {
        let start = std::time::Instant::now();
        events += world.step(&mut queries).len();
        durations.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    durations.sort_by(f64::total_cmp);
    let mean = durations.iter().sum::<f64>() / durations.len() as f64;
    let report = serde_json::json!({"schema_version":1,"scenario":"8 actor identities / 1024 native Pong projectiles / 1200 fixed ticks / empty query scene","limitations":"No GPU, native map, player, network, damage or audio workload; isolated subsystem envelope only","tick_hz":120,"actors":8,"projectiles":world.projectiles().count(),"ticks":1200,"events":events,"mean_ms":mean,"p95_ms":durations[1140],"max_ms":durations.last().context("No ticks")?,"pack":world.pack.id,"os":std::env::consts::OS,"arch":std::env::consts::ARCH});
    std::fs::write(&args[1], serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
