use anyhow::{Context, Result, ensure};
use bri_vehicles::*;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 3,
        "usage: vehicle_probe VEHICLES_JSON REPORT_JSON"
    );
    let pack = Pack::load(&args[1])?;
    pack.verify_assets(std::path::Path::new(&args[1]).parent().unwrap())?;
    let definitions = pack.definitions.clone();
    let mut world = PhysicsWorld::new();
    world.integration_parameters.dt = FIXED_DT;
    world.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(1000., 0.5, 1000.),
    );
    let mut vehicles = VehiclesWorld::new(pack)?;
    for (i, d) in definitions.iter().enumerate() {
        let id = VehicleId(i as u64 + 1);
        vehicles.spawn(
            &mut world,
            Spawn {
                scale: 1.,
                id,
                owner: OwnerId(1),
                definition: d.id.clone(),
                transform: Transform {
                    position: [i as f32 * 30., 3., 0.],
                    ..Default::default()
                },
                spawn_id: None,
                respawn_ticks: None,
            },
        )?;
        let snapshot = vehicles.snapshot(&world);
        if let Some(s) = snapshot
            .vehicles
            .iter()
            .find(|v| v.id == id)
            .and_then(|v| v.seats.first())
        {
            vehicles.mount(
                &world,
                id,
                0,
                Occupant {
                    id: OccupantId(i as u64 + 1),
                    owner: OwnerId(1),
                    body: [1.25, 2.65],
                },
                s.transform.position,
            )?;
            if d.family != Family::Tumble {
                vehicles.set_controls(
                    OwnerId(1),
                    OccupantId(i as u64 + 1),
                    Controls {
                        throttle: 0.5,
                        ..Default::default()
                    },
                )?;
            }
        }
    }
    world.detect_collisions(&(), &());
    // A pool 4 deep between x 170 and 210 for the boats.
    let waters = [bri_content::water::Water::volume(
        [170., -1000., -1e4],
        [210., 4., 1e4],
    )];
    let start = std::time::Instant::now();
    let mut fires = 0;
    let mut intent_count = 0;
    for _ in 0..1200 {
        vehicles.pre_step(&mut world, &waters)?;
        world.step();
        vehicles.post_step(&mut world)?;
        let intents = vehicles.drain_intents();
        intent_count += intents.len();
        fires += intents
            .iter()
            .filter(|i| matches!(i, Intent::Fire(_)))
            .count();
    }
    let elapsed = start.elapsed();
    let snapshot = vehicles.snapshot(&world);
    ensure!(
        snapshot.vehicles.iter().all(|v| v
            .transform
            .position
            .iter()
            .chain(v.velocity.iter())
            .all(|x| x.is_finite())),
        "nonfinite native state"
    );
    let report = json!({"schema_version":1,"pack":args[1],"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"rapier":"0.36.0 enhanced-determinism","fixed_hz":120,"ticks":1200,"simulated_seconds":10,"wall_milliseconds":elapsed.as_secs_f64()*1000.,"mean_tick_microseconds":elapsed.as_secs_f64()*1e6/1200.,"vehicle_count":snapshot.vehicles.len(),"intents":intent_count,"projectile_intents":fires,"snapshot":snapshot,"limits":["Headless fixed floor and one water sample region; not map traversal or render evidence","Native Rapier adaptation, not subjective vanilla physics parity","No visible window, gameplay input or audio playback"]});
    let path = std::path::Path::new(&args[2]);
    std::fs::create_dir_all(path.parent().context("report directory")?)?;
    std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "11 vehicle definitions / 1200 shared ticks: {:.2} ms; {:.2} us/tick",
        elapsed.as_secs_f64() * 1000.,
        elapsed.as_secs_f64() * 1e6 / 1200.
    );
    Ok(())
}
