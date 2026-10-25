//! Isolate portable topology loading/querying from physics and gameplay.
#[path = "../src/navigation/detour.rs"]
mod detour;
use anyhow::{Result, ensure};
use bri_content::navigation::WalkingMap;
use glam::Vec3;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 3,
        "nav_query <walking-map.json> <probes.txt> <output.json>"
    );
    let map: WalkingMap = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    map.validate()?;
    eprintln!("validated {} polygons", map.mesh.polygons.len());
    let mut mesh = detour::Mesh::new(&map.mesh, &map.profile)?;
    eprintln!("created query");
    let mut results = Vec::new();
    for line in std::fs::read_to_string(&args[1])?.lines() {
        let values: Vec<_> = line.split_whitespace().collect();
        ensure!(values.len() == 7, "probe needs name and two points");
        let numbers: Vec<f32> = values[1..]
            .iter()
            .map(|s| s.parse())
            .collect::<std::result::Result<_, _>>()?;
        let from = Vec3::from_slice(&numbers[..3]);
        let to = Vec3::from_slice(&numbers[3..]);
        let start = std::time::Instant::now();
        let attachments = [
            mesh.attachment(from, map.profile.step).map(|(_, p)| p),
            mesh.attachment(to, map.profile.step).map(|(_, p)| p),
        ];
        let route = mesh.route(from, to, map.profile.step);
        eprintln!(
            "{} connected={} points={}",
            values[0],
            route.is_some(),
            route.as_ref().map_or(0, |r| r.len())
        );
        results.push(serde_json::json!({"name": values[0], "attachments":attachments, "connected": route.is_some(), "query_ms": start.elapsed().as_secs_f64() * 1000., "points": route.map(|r| r.into_iter().map(|p| p.to_array()).collect::<Vec<_>>())}));
    }
    std::fs::write(&args[2], serde_json::to_vec_pretty(&results)?)?;
    Ok(())
}
