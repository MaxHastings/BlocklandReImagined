//! v20 slide builds: a Blockhead rides a chain of 72 degree ramps down
//! hands-free. Acceptance runs on the v20 Slate save "Mr.Block's Slides". Its
//! slides are lanes of two 72 degree ramps facing each other (collision 74.5
//! degrees, steeper than the 70 degree run surface), stepping down 0.4 per
//! brick. The player box is wider than the lane floor, so it wedges between
//! the slopes, and Torque's crease rule turns each fall into speed along the
//! lane.
use bri_sim::{
    definitions::Definitions,
    player::{MoveInput, Player, PlayerTuning},
    simulation::Simulation,
};
use glam::{Quat, Vec3};
use rapier3d::prelude::*;
use std::path::Path;

const SLIDES: &str =
    "worlds-pass-006/0a885afb52ad3e873315d260a0a19e94630a1c61bde8c0e6a62d3bd4721aee5c.world.json";
/// Every `stride`th case, a spread over the whole tower that runs in a few
/// seconds; `BRI_SLIDES_FULL=1` runs every case (a minute in release).
fn sample(stride: usize) -> usize {
    if std::env::var_os("BRI_SLIDES_FULL").is_some() {
        1
    } else {
        stride
    }
}
const RAMPS: [&str; 2] = [
    "v20/brick/brick1x2x3rampdata",
    "v20/brick/brick2x2x3rampdata",
];

/// A ramp brick's position and quarter turns.
type Ramp = ([f32; 3], u8);

/// The save's simulation, and its 72 degree ramps (position, quarter turns).
fn slides() -> anyhow::Result<(Simulation, Vec<Ramp>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let world = bri_world::persistence::load(&root.join(SLIDES))?;
    let ramps = world
        .bricks
        .values()
        .filter(|b| {
            matches!(&b.definition, bri_world::ContentRef::Resolved(id) if RAMPS.contains(&id.as_str()))
        })
        .map(|b| (b.position, b.quarter_turns))
        .collect();
    let definitions =
        Definitions::load(&root.join("stock-catalog-004"), &root.join("maps-pass-008"))?;
    let sim = Simulation::new(
        world,
        definitions,
        vec![ColliderBuilder::cuboid(400.0, 0.5, 400.0).translation(Vector::new(0.0, -0.5, 0.0))],
    )?;
    Ok((sim, ramps))
}

/// The direction a ramp's slope faces (its local -z).
fn facing(quarter_turns: u8) -> Vec3 {
    Quat::from_rotation_y(-f32::from(quarter_turns) * std::f32::consts::FRAC_PI_2) * Vec3::NEG_Z
}

/// One segment of a slide lane: feet wedged between the two slopes (0.45
/// under the ramps' centres), and the lane's horizontal axis.
#[derive(Clone, Copy)]
struct Lane {
    feet: Vec3,
    axis: Vec3,
}

/// Two 72 degree ramps at the same height facing each other two studs apart.
fn lanes(ramps: &[Ramp]) -> Vec<Lane> {
    let mut out: Vec<Lane> = vec![];
    for (a, ta) in ramps {
        let facing = facing(*ta);
        let partner = Vec3::from(*a) + facing * 2.0;
        let paired = ramps
            .iter()
            .any(|(b, tb)| (Vec3::from(*b) - partner).length() < 0.01 && (*tb + 4 - *ta) % 4 == 2);
        let feet = Vec3::from(*a) + facing - Vec3::Y * 0.43;
        if paired && !out.iter().any(|l| (l.feet - feet).length() < 0.01) {
            out.push(Lane {
                feet,
                axis: Vec3::new(-facing.z, 0.0, facing.x),
            });
        }
    }
    out
}

/// The downhill direction along a lane: toward an adjacent lower segment.
fn downhill(lanes: &[Lane], lane: &Lane) -> Option<Vec3> {
    lanes.iter().find_map(|other| {
        let d = other.feet - lane.feet;
        let along = d.dot(lane.axis);
        let across = (d - lane.axis * along).with_y(0.0).length();
        (across < 0.05 && along.abs() <= 1.1 && along.abs() > 0.1 && d.y < -0.05)
            .then(|| lane.axis * along.signum())
    })
}

/// Feet of a player resting against the middle of a 72 degree ramp's slope.
/// Locally the slope rises from (z -0.5, y -0.9) to (z 0, y 0.9) and faces -z.
fn on_slope(position: [f32; 3], quarter_turns: u8) -> Vec3 {
    let local = Vec3::new(0.0, 0.0, -0.25 - 0.02 - 0.625);
    let turn = Quat::from_rotation_y(-f32::from(quarter_turns) * std::f32::consts::FRAC_PI_2);
    Vec3::from(position) + turn * local
}

struct Ride {
    start: Vec3,
    end: Vec3,
    /// Came to rest without standing on a run surface.
    wedged: bool,
    max_speed: f32,
}

fn ride(sim: &mut Simulation, feet: Vec3, push: Vec3, ticks: usize) -> Option<Ride> {
    let physics = &mut sim.physics;
    let mut player = Player::spawn(physics, 1, feet, PlayerTuning::default()).ok()?;
    player.set_motion(push, false);
    let (mut still, mut wedged, mut max_speed) = (0, false, 0.0_f32);
    for _ in 0..ticks {
        // Bricks are fixed and the motor reads the query pipeline directly,
        // so the rider needs no physics step.
        player.step(physics, MoveInput::default()).unwrap();
        let v = Vec3::from(player.state().velocity);
        max_speed = max_speed.max(v.length());
        still = if v.length() < 0.5 { still + 1 } else { 0 };
        if still > 120 {
            wedged = !player.state().grounded;
            break;
        }
    }
    let end = Vec3::from(player.state().feet);
    player.despawn(physics);
    Some(Ride {
        start: feet,
        end,
        wedged,
        max_speed,
    })
}

/// Dropped onto the face of any 72 degree ramp in the save, a player slides
/// off it: steeper than the run surface is never ground.
#[test]
#[ignore = "requires the converted v20 worlds and stock catalog"]
fn no_72_degree_ramp_face_holds_a_player() -> anyhow::Result<()> {
    let (mut sim, ramps) = slides()?;
    let (mut rides, mut held) = (0, vec![]);
    for (position, turns) in ramps.iter().step_by(sample(90)) {
        let feet = on_slope(*position, *turns);
        if let Some(r) = ride(&mut sim, feet, Vec3::ZERO, 240) {
            rides += 1;
            // At rest off any run surface, still near where it started: the
            // face held it instead of sliding it off.
            if r.wedged && (r.end - r.start).length() < 0.2 {
                held.push(r.start);
            }
        }
    }
    eprintln!("rides {rides}, held {}", held.len());
    for start in held.iter().take(10) {
        eprintln!("  held at {start}");
    }
    assert!(rides * sample(90) > 3000);
    assert!(held.is_empty());
    Ok(())
}

/// Pushed gently downhill into any slide lane, a player rides it hands-free
/// to the end of its leg: no seam between ramps catches it on the way.
#[test]
#[ignore = "requires the converted v20 worlds and stock catalog"]
fn slide_lanes_carry_a_player_down_hands_free() -> anyhow::Result<()> {
    let (mut sim, ramps) = slides()?;
    let mut lanes = lanes(&ramps);
    // Highest first, so the sample always includes a ride from the top.
    lanes.sort_by(|a, b| b.feet.y.total_cmp(&a.feet.y));
    let (mut rides, mut caught, mut descents, mut speeds) = (0, vec![], vec![], vec![]);
    let downhill_lanes: Vec<_> = lanes
        .iter()
        .filter_map(|lane| downhill(&lanes, lane).map(|down| (lane, down)))
        .collect();
    for (lane, down) in downhill_lanes.iter().step_by(sample(90)) {
        let (lane, down) = (*lane, *down);
        let Some(r) = ride(&mut sim, lane.feet, down * 2.0, 120 * 10) else {
            continue;
        };
        rides += 1;
        descents.push(r.start.y - r.end.y);
        speeds.push(r.max_speed);
        // Wedged at rest with the lane stepping down just ahead: a seam caught
        // it. (Legs end in level run-outs, where a rider may coast to a stop.)
        let travel = (r.end - r.start).with_y(0.0).normalize_or_zero();
        let more = lanes.iter().any(|l| {
            let d = l.feet - r.end;
            d.y < -0.2 && d.with_y(0.0).length() < 1.2 && d.with_y(0.0).dot(travel) > 0.3
        });
        if r.wedged && more {
            caught.push((r.start, r.end));
        }
    }
    descents.sort_by(f32::total_cmp);
    speeds.sort_by(f32::total_cmp);
    eprintln!(
        "{} lane segments, {rides} downhill rides, {} caught; median descent {:.1}, longest {:.1}; median top speed {:.1}",
        lanes.len(),
        caught.len(),
        descents[descents.len() / 2],
        descents[descents.len() - 1],
        speeds[speeds.len() / 2],
    );
    for (start, end) in caught.iter().take(20) {
        eprintln!("  caught: from {start} to {end}");
    }
    // One spot remains: riders who drop down a shaft at over 20 u/s land a
    // hair inside the next lane's first ramp and stop at its end face.
    assert!(rides * sample(90) > 800);
    assert!(caught.len() * 25 <= rides, "{} caught", caught.len());
    assert!(descents[descents.len() - 1] > 20.0);
    Ok(())
}
