use anyhow::Result;
use bri_physics::preflight::*;

fn main() -> Result<()> {
    let destination = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or("artifacts/preflight".into()),
    );
    std::fs::create_dir_all(&destination)?;
    let started = std::time::Instant::now();
    let report = serde_json::json!({
        "status": "passed", "library": "rapier3d 0.36.0", "tick_hz": 120,
        "resting_box": drop_and_rest()?, "character_at_wall": character_wall()?,
        "driven_vehicle": vehicle_suspension_and_drive()?,
        "elapsed_ms": started.elapsed().as_millis(),
        "scope": "synthetic collision/controller integration; not Blockland movement or Jeep tuning"
    });
    std::fs::write(
        destination.join("physics-probe.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
