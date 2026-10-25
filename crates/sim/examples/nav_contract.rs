//! Bounded native navigation/motor agreement audit; never interactive gameplay.
//! nav_contract <map-root> <map-id> <points.json> <archetype.json> <output.json> [scale]
use anyhow::{Result, ensure};
use bri_motor::player::{MoveInput, Player, PlayerTuning};
use bri_sim::nav::{Body, Ground, Nav, Search};
use glam::Vec3;
use serde::Deserialize;
use serde_json::json;
use std::{path::Path, time::Instant};

#[derive(Deserialize)]
struct Point {
    id: String,
    feet: [f32; 3],
}
#[derive(Deserialize)]
struct Input {
    nodes: Vec<Point>,
    #[serde(default)]
    movers: std::collections::BTreeMap<String, u8>,
    #[serde(default)]
    links: Vec<[[f32; 3]; 2]>,
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        a.len() >= 5,
        "nav_contract <map-root> <map-id> <points> <archetype> <output> [scale]"
    );
    let input: Input = serde_json::from_slice(&std::fs::read(&a[2])?)?;
    ensure!(
        input.nodes.len() <= 512,
        "bounded audit: at most 512 points"
    );
    let scale: f32 = a.get(5).map(|v| v.parse()).transpose()?.unwrap_or(1.);
    let def: serde_json::Value = serde_json::from_slice(&std::fs::read(&a[3])?)?;
    let mut tuning: PlayerTuning = serde_json::from_value(def["movement"].clone())?;
    tuning = tuning.scaled(scale);
    // Observe physical settling, without an automatic relocation hiding it.
    let unstick = tuning.unstick.take();
    let body = Body::of(&tuning, 1.);
    let native = bri_sim::map::NativeMap::load(Path::new(&a[0]), &a[1])?;
    let anchors = native.spawn_anchors()?;
    let world = bri_world::World::new(
        "Navigation contract audit".into(),
        a[1].clone(),
        vec![[1.; 4]],
    );
    let mut sim = bri_sim::simulation::Simulation::new(
        world,
        bri_sim::testing::definitions(),
        native.colliders,
    )?;
    sim.attach_terrain(native.terrain, anchors)?;
    for mover in &native.movers {
        if let Some(key) = input.movers.get(&mover.def.id) {
            ensure!(
                usize::from(*key) < mover.def.keys.len(),
                "invalid diagnostic mover key"
            );
            let transform = mover.transform(&bri_content::mover::MoverState::at_rest(*key), 0);
            let (_, rotation, translation) = transform.to_scale_rotation_translation();
            sim.move_map_colliders(
                mover.colliders.clone(),
                rapier3d::prelude::Pose::from_parts(
                    rapier3d::prelude::Vector::from_array(translation.to_array()),
                    rapier3d::prelude::Rotation::from_array(rotation.to_array()),
                ),
            );
        }
    }
    // Hold the same closed/static mover poses for both queries. Door-opening
    // policy is a separate contract and must not masquerade as placement drift.
    sim.physics.step();
    let mut planned = Vec::new();
    let mut links = Vec::new();
    {
        let terrain = |o, d, r| sim.terrain_ray(o, d, r);
        let ground = Ground {
            physics: &sim.physics,
            terrain: &terrain,
            passages: sim.passages(),
            waters: &native.waters,
            bodies: &[],
            motions: &[],
            solid: &bri_sim::nav::all_solid,
        };
        let mut nav = Nav::default();
        nav.unmetered();
        for pair in &input.links {
            let from = Vec3::from(pair[0]);
            let to = Vec3::from(pair[1]);
            let mut search = Search::new(from, to, 40.);
            let mut found = None;
            for _ in 0..10000 {
                nav.unmetered();
                if let Some(result) = search.step(&mut nav, &ground, &body) {
                    found = Some(format!("{result:?}"));
                    break;
                }
            }
            let shape =
                rapier3d::prelude::SharedShape::cylinder(body.height * 0.5, body.width * 0.5);
            let pose =
                rapier3d::prelude::Pose::translation(from.x, from.y + body.height * 0.5, from.z);
            let query = sim.physics.query_pipeline_with_filter(
                rapier3d::prelude::QueryFilter::only_fixed().exclude_sensors(),
            );
            let contacts: Vec<_> = query.intersect_shape(pose, shape.as_ref()).filter_map(|(h,c)| {
                let contact = rapier3d::parry::query::contact(c.position(),c.shape(),&pose,shape.as_ref(),0.05).ok().flatten()?;
                let index = sim.map_collider_index(h);
                let mover = native.movers.iter().find(|m| index.is_some_and(|i| m.colliders.contains(&i))).map(|m| &m.def.id);
                Some(json!({"collider":index,"mover":mover,"gap":contact.dist,"normal":contact.normal1.to_array(),"point":contact.point1.to_array()}))
            }).collect();
            links.push(json!({"from":pair[0],"to":pair[1],"from_clear":Player::clear(&sim.physics, from, &tuning),"straight":ground.walkable(&body, from, to),"search":found,"contacts":contacts}));
        }
        for p in input.nodes {
            let at = nav
                .node_at(&ground, &body, Vec3::from(p.feet))
                .flatten()
                .map(|n| n.feet());
            planned.push((p, at));
        }
    }
    let began = Instant::now();
    // Independently execute each retained link from the actual raw pose. This
    // distinguishes a bad waypoint from a body unable to move out of contact.
    for (i, pair) in input.links.iter().enumerate() {
        let from = Vec3::from(pair[0]);
        let to = Vec3::from(pair[1]);
        let mut actor = Player::spawn_overlapping(&mut sim.physics, 998, from, tuning.clone())?;
        let mut state = actor.state().clone();
        state.scale = scale;
        actor.restore(&mut sim.physics, state, tuning.clone())?;
        let direction = to - from;
        let control = MoveInput {
            forward: 1.,
            yaw: direction.x.atan2(-direction.z),
            ..Default::default()
        };
        for _ in 0..240 {
            actor.step(&mut sim.physics, control)?;
            sim.physics.step();
        }
        links[i]["motor_after_two_seconds"] = json!(actor.state().feet);
        actor.despawn(&mut sim.physics);
    }
    let mut rows = Vec::new();
    for (i, (p, at)) in planned.into_iter().enumerate() {
        let Some(at) = at else {
            rows.push(json!({"id":p.id,"hint":p.feet,"classification":"no navigation pose"}));
            continue;
        };
        let start = [body.step, body.step * 0.5, 0.1, 0.02]
            .into_iter()
            .map(|rise| at + Vec3::Y * rise)
            .find(|feet| Player::clear(&sim.physics, *feet, &tuning));
        let Some(start) = start else {
            rows.push(json!({"id":p.id,"hint":p.feet,"nav":at.to_array(),"classification":"no clear fall start; inconclusive (closed door/headroom/placement)"}));
            continue;
        };
        let mut actor = Player::spawn_overlapping(&mut sim.physics, 999, start, tuning.clone())?;
        let mut state = actor.state().clone();
        state.scale = scale;
        actor.restore(&mut sim.physics, state, tuning.clone())?;
        for _ in 0..120 {
            actor.step(&mut sim.physics, MoveInput::default())?;
            sim.physics.step();
        }
        let state = actor.state().clone();
        let actual = Vec3::from(state.feet);
        rows.push(json!({"id":p.id,"hint":p.feet,"nav":at.to_array(),"motor":state.feet,"grounded":state.grounded,
            "delta":(actual-at).to_array(),"classification":"observed ordinary motor; unstick disabled"}));
        actor.despawn(&mut sim.physics);
        if i % 50 == 0 {
            eprintln!("AUDIT {i} points {:.1}s", began.elapsed().as_secs_f64());
        }
    }
    let result = json!({"scope":"placement/local-link agreement on identical static/explicit-mover collision; no autonomous door interaction, crowd or combat acceptance", "scale":scale, "movers":input.movers,"links":links,
        "unstick_disabled":unstick.is_some(),"seconds":began.elapsed().as_secs_f64(),"rows":rows});
    std::fs::write(&a[4], serde_json::to_vec_pretty(&result)?)?;
    eprintln!(
        "AUDIT complete {} points {:.1}s",
        result["rows"].as_array().unwrap().len(),
        began.elapsed().as_secs_f64()
    );
    Ok(())
}
