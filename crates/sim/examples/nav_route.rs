//! Routes of a map's collision for a body, as the bots' planner finds them, without the bots' per-tick budgets:
//!
//!   cargo run --release -p bri-sim --example nav_route -- <shape.json> <scene.json> <archetype.json> "x,y,z->x,y,z" ...
//!
//! The collision is the shape's objects whose names start with `col_`; the scene's `climb` nodes are ladders. Every
//! mover (door) is left out, as a route search walks through the doors the rules open. Prints one JSON line per
//! route: `{"from", "to", "found": "path"|"partial", "legs": [{"feet", "mode"}]}`, so a map's authored waypoints are
//! made from what the planner can walk, not guessed.
use bri_content::passage::Passages;
use bri_motor::{climb::Climb, player::Fall, player::PlayerTuning};
use bri_sim::nav::{Body, Found, Ground, Mode, Nav, Search, all_solid};
use glam::Vec3;
use rapier3d::prelude::*;

fn num(v: &serde_json::Value) -> f32 {
    v.as_f64().expect("a number") as f32
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 4,
        "usage: nav_route <shape.json> <scene.json> <archetype.json> <from->to>..."
    );
    let read = |p: &str| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(p).expect(p)).expect(p)
    };
    let (shape, scene, archetype) = (read(&args[0]), read(&args[1]), read(&args[2]));
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
    for node in scene["nodes"].as_array().unwrap() {
        if node["kind"] != "climb" {
            continue;
        }
        let t: Vec<f32> = node["transform"]
            .as_array()
            .unwrap()
            .iter()
            .map(num)
            .collect();
        let half: Vec<f32> = node["properties"]["half"]
            .as_str()
            .unwrap()
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        let speed: f32 = node["properties"]["speed"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let facing = -Vec3::new(t[8], t[9], t[10]);
        physics.insert_collider(
            bri_motor::climb::collider(
                Vec3::new(t[12], t[13], t[14]),
                Vec3::new(half[0], half[1], half[2]),
                facing.x.atan2(-facing.z),
                speed,
                false,
            ),
            None,
        );
    }
    bri_physics::detect_collisions(&mut physics);
    // The body: the archetype's movement block over the motor's defaults.
    let movement = &archetype["movement"];
    let mut tuning = PlayerTuning::default();
    let set = |name: &str, field: &mut f32| {
        if let Some(v) = movement.get(name) {
            *field = num(v);
        }
    };
    set("width", &mut tuning.width);
    set("stand_height", &mut tuning.stand_height);
    set("crouch_height", &mut tuning.crouch_height);
    set("step_height", &mut tuning.step_height);
    set("jump_speed", &mut tuning.jump_speed);
    set("gravity", &mut tuning.gravity);
    if let Some(fall) = movement.get("fall") {
        tuning.fall = Some(Fall {
            threshold: num(&fall["threshold"]),
            per_speed: num(&fall["per_speed"]),
            base: 0.0,
            max: None,
        });
    }
    if let Some(climb) = movement.get("climb") {
        tuning.climb = Some(Climb {
            wall_pull: climb.get("wall_pull").map_or(0.0, num),
            ..Climb::default()
        });
    }
    let body = Body::of(&tuning, 1.0);
    let passages = Passages {
        list: Vec::new(),
        closed: Vec::new(),
    };
    let no_terrain = |_: Vec3, _: Vec3, _: f32| None;
    let ground = Ground {
        physics: &physics,
        terrain: &no_terrain,
        passages: &passages,
        waters: &[],
        bodies: &[],
        motions: &[],
        solid: &all_solid,
    };
    for pair in &args[3..] {
        let (a, b) = pair.split_once("->").expect("from->to");
        let point = |s: &str| {
            let n: Vec<f32> = s.split(',').map(|x| x.trim().parse().unwrap()).collect();
            Vec3::new(n[0], n[1], n[2])
        };
        let (from, to) = (point(a), point(b));
        let mut nav = Nav::default();
        let mut search = Search::new(from, to, 1000.0).unlimited();
        let found = loop {
            nav.begin_tick();
            nav.unmetered();
            if let Some(found) = search.step(&mut nav, &ground, &body) {
                break found;
            }
        };
        let (kind, legs) = match found {
            Found::Path(p) => ("path", p),
            Found::Partial(p) => ("partial", p),
            Found::Nowhere => ("nowhere", Vec::new()),
        };
        let legs: Vec<serde_json::Value> = legs
            .iter()
            .map(|w| {
                let mode = match w.mode {
                    Mode::Walk => "walk",
                    Mode::Swim => "swim",
                    Mode::Jet { .. } => "jet",
                    Mode::Leap { .. } => "leap",
                    Mode::Climb { .. } => "climb",
                };
                serde_json::json!({ "feet": [w.feet.x, w.feet.y, w.feet.z], "mode": mode, "jump": w.jump })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "from": from.to_array(), "to": to.to_array(), "found": kind, "legs": legs })
        );
    }
}
