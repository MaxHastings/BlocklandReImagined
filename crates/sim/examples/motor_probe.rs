//! The motor against a map's collision, offline: walk a body (the archetype's own movement block) at a spot from many directions and print
//! where it ends, so "does it catch on this prop" is a number.
//!
//!   cargo run --profile playtest -p bri-sim --example motor_probe -- <shape.json> <archetype.json> "x,y,z" [radius] [seconds] [lateral]
//!
//! For `x,y,z` (a point on the floor, near a prop) the body starts `radius` (default 3) away in each of 16 compass directions, facing the
//! point, and holds forward for `seconds` (default 3), the way a player pushes into a prop. For each start it prints the distance
//! it covered, its speed over the last half second, and where it ended. A flat wall gives the baseline: a body pushing straight into
//! it stops (speed 0), one pushing at an angle slides along it at about the speed times the cosine of the angle off the wall.
use bri_motor::player::{MoveInput, Player, PlayerTuning};
use glam::Vec3;
use rapier3d::prelude::*;

fn num(v: &serde_json::Value) -> f32 {
    v.as_f64().expect("a number") as f32
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 3,
        "usage: motor_probe <shape.json> <archetype.json> x,y,z [radius] [seconds]"
    );
    let read = |p: &str| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(p).expect(p)).expect(p)
    };
    let (shape, archetype) = (read(&args[0]), read(&args[1]));
    let mut physics = bri_physics::new_world();
    for object in shape["objects"].as_array().unwrap() {
        if !object["name"].as_str().unwrap().starts_with("col_") {
            continue;
        }
        for mesh in object["meshes"].as_array().unwrap() {
            let mesh = &shape["meshes"][mesh.as_u64().unwrap() as usize];
            let positions: Vec<Vector> = mesh["positions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| Vector::new(num(&p[0]), num(&p[1]), num(&p[2])))
                .collect();
            let mut triangles: Vec<[u32; 3]> = Vec::new();
            for primitive in mesh["primitives"].as_array().unwrap() {
                for t in primitive["triangles"].as_array().unwrap() {
                    triangles.push([
                        t[0].as_u64().unwrap() as u32,
                        t[1].as_u64().unwrap() as u32,
                        t[2].as_u64().unwrap() as u32,
                    ]);
                }
            }
            physics.insert_collider(
                ColliderBuilder::trimesh(positions, triangles).expect("a mesh"),
                None,
            );
        }
    }
    bri_physics::detect_collisions(&mut physics);
    let tuning: PlayerTuning =
        serde_json::from_value(archetype["movement"].clone()).expect("the archetype's movement block");
    let point: Vec<f32> = args[2].split(',').map(|x| x.trim().parse().unwrap()).collect();
    let center = Vec3::new(point[0], point[1], point[2]);
    let radius: f32 = args.get(3).map_or(3.0, |s| s.parse().unwrap());
    let seconds: f32 = args.get(4).map_or(3.0, |s| s.parse().unwrap());
    let lateral: f32 = args.get(5).map_or(0.0, |s| s.parse().unwrap());
    let ticks = (seconds * 120.0) as usize;
    for k in 0..16 {
        // yaw in (-pi, pi]
        let angle = {
            let a = k as f32 * std::f32::consts::TAU / 16.0;
            if a > std::f32::consts::PI { a - std::f32::consts::TAU } else { a }
        };
        // heading (sin a, -cos a) in the motor's yaw: yaw 0 faces -z, yaw pi/2 faces +x. The body starts on the opposite side.
        let heading = Vec3::new(angle.sin(), 0.0, -angle.cos());
        // aim at the point `lateral` to the body's right of the prop's centre, so the body grazes the prop's side (0: straight at it)
        let right = Vec3::new(angle.cos(), 0.0, angle.sin());
        let aim = center + right * lateral;
        let start = aim - heading * radius;
        let world = &mut physics;
        let Ok(mut player) = Player::spawn(world, 1, Vec3::new(start.x, center.y + 0.05, start.z), tuning.clone()) else {
            println!("dir {k:2}: start {start:.2?} is inside geometry");
            continue;
        };
        let input = MoveInput {
            forward: 1.0,
            yaw: angle,
            ..Default::default()
        };
        // settle onto the floor first
        for _ in 0..60 {
            player.step(world, MoveInput { yaw: angle, ..Default::default() }).unwrap();
        }
        let from = Vec3::from(player.state().feet);
        let mut half = from;
        for t in 0..ticks {
            player.step(world, input).unwrap();
            if t + 60 == ticks {
                half = Vec3::from(player.state().feet);
            }
        }
        let end = Vec3::from(player.state().feet);
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
        println!(
            "dir {k:2} (heading {:5.1} deg): covered {:5.2} of {:5.2} through the point, last half second {:4.2}/s, ended {:.2?}",
            angle.to_degrees(),
            (flat(end) - flat(from)).length(),
            radius * 2.0,
            (flat(end) - flat(half)).length() * 2.0,
            end
        );
        player.despawn(world);
    }
}
