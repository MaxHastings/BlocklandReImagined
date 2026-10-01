//! v20 slide builds: a Blockhead rides a chain of steep ramps down
//! hands-free. Slides are lanes of two ramps facing each other, steeper than
//! the 70 degree run surface, stepping down along the lane. The player box
//! is wider than the lane floor, so it wedges between the slopes, and
//! Torque's crease rule turns each fall into speed along the lane.
//!
//! Each test runs on a made-up tower of [`bri_sim::testing::STEEP_RAMP`]
//! lanes, and again, ignored, on the v20 Slate save "Mr.Block's Slides",
//! whose lanes are 72 degree ramps (collision 74.5 degrees) stepping down
//! 0.4 per brick.
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
/// Every `stride`th case of the save, a spread over the whole tower that
/// runs in a few seconds; `BRI_SLIDES_FULL=1` runs every case (a minute in
/// release).
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

/// A slide build and the shape of its ramps.
struct Slides {
    sim: Simulation,
    /// Its steep ramps (position, quarter turns).
    ramps: Vec<Ramp>,
    /// Feet of a rider wedged in a lane, this far under the ramps' centres.
    wedge: f32,
    /// The middle of a ramp's slope, along its local z (it faces -z).
    slope_middle: f32,
    /// Every `stride`th ramp or lane is tried.
    stride: usize,
}

impl Slides {
    /// The Slides save. Its ramps' slopes rise from (z -0.5, y -0.9) to
    /// (z 0, y 0.9) in their own frame; a rider wedges 0.43 under them.
    fn content() -> anyhow::Result<Self> {
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
        Ok(Self {
            sim: simulation(world, definitions)?,
            ramps,
            wedge: 0.43,
            slope_middle: -0.25,
            stride: sample(90),
        })
    }
    /// A made-up tower: two legs of [`bri_sim::testing::STEEP_RAMP`] lanes
    /// at right angles, each stepping down `STEP` per brick to the floor,
    /// and [`LONE_RAMPS`] ramps standing alone, one per quarter turn.
    fn synthetic() -> anyhow::Result<Self> {
        const STEP: f32 = 0.2;
        const SEGMENTS: usize = 16;
        let definitions = bri_sim::testing::definitions();
        let mesh = &definitions.entries[bri_sim::testing::STEEP_RAMP].mesh;
        let depth = mesh.footprint_studs[1] as f32 * 0.5;
        let height = mesh.height_plates as f32 * 0.2;
        let mut world = bri_world::World::new("Slides".into(), "test".into(), vec![[1.0; 4]]);
        let mut ramps = vec![];
        for (start, turns) in [
            (Vec3::new(0.0, 0.0, 0.0), 0u8),
            (Vec3::new(-6.0, 0.0, 8.0), 1),
        ] {
            let facing = facing(turns);
            let along = Vec3::new(-facing.z, 0.0, facing.x);
            for i in 0..SEGMENTS {
                let y = height * 0.5 + (SEGMENTS - i) as f32 * STEP;
                let a = start + along * i as f32 + Vec3::Y * y;
                for (position, turns) in [(a, turns), (a + facing * LANE, (turns + 2) % 4)] {
                    let mut brick = bri_world::Brick::new(
                        bri_world::ContentRef::Resolved(bri_sim::testing::STEEP_RAMP.into()),
                        position.to_array(),
                        0,
                    );
                    brick.quarter_turns = turns;
                    world.bricks.insert(world.next_brick_id, brick);
                    world.next_brick_id += 1;
                    ramps.push((position.to_array(), turns));
                }
            }
        }
        for turns in 0..LONE_RAMPS as u8 {
            let position = Vec3::new(20.0 + 4.0 * f32::from(turns), height * 0.5, -20.0);
            let mut brick = bri_world::Brick::new(
                bri_world::ContentRef::Resolved(bri_sim::testing::STEEP_RAMP.into()),
                position.to_array(),
                0,
            );
            brick.quarter_turns = turns;
            world.bricks.insert(world.next_brick_id, brick);
            world.next_brick_id += 1;
            ramps.push((position.to_array(), turns));
        }
        // The lane's floor between the slopes' feet is `LANE - depth` wide;
        // each slope leans in by half the depth over the full height, so a
        // player box wedges where the gap grows to its width, `wedged` from
        // the ramps' centres. A rider starts a little above where the box
        // clears the next higher segment's slopes too, and drops to wedge.
        let width = PlayerTuning::default().width;
        let wedged = -height * 0.5 + (width - (LANE - depth)) * 0.5 * height / (depth * 0.5);
        Ok(Self {
            sim: simulation(world, definitions)?,
            ramps,
            wedge: -(wedged + STEP) - 0.05,
            slope_middle: -depth * 0.25,
            stride: 1,
        })
    }
}

/// Facing ramps of a lane stand this far apart, centre to centre.
const LANE: f32 = 2.0;
/// Ramps of the made-up build that stand alone, faces open.
const LONE_RAMPS: usize = 4;

fn simulation(world: bri_world::World, definitions: Definitions) -> anyhow::Result<Simulation> {
    Simulation::new(
        world,
        definitions,
        vec![ColliderBuilder::cuboid(400.0, 0.5, 400.0).translation(Vector::new(0.0, -0.5, 0.0))],
    )
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

/// Two steep ramps at the same height facing each other [`LANE`] apart.
fn lanes(slides: &Slides) -> Vec<Lane> {
    let ramps = &slides.ramps;
    let mut out: Vec<Lane> = vec![];
    for (a, ta) in ramps {
        let facing = facing(*ta);
        let partner = Vec3::from(*a) + facing * LANE;
        let paired = ramps
            .iter()
            .any(|(b, tb)| (Vec3::from(*b) - partner).length() < 0.01 && (*tb + 4 - *ta) % 4 == 2);
        let feet = Vec3::from(*a) + facing * (LANE * 0.5) - Vec3::Y * slides.wedge;
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

/// Feet of a player resting against the middle of a steep ramp's slope,
/// which faces local -z.
fn on_slope(slope_middle: f32, position: [f32; 3], quarter_turns: u8) -> Vec3 {
    let half_width = PlayerTuning::default().width * 0.5;
    let local = Vec3::new(0.0, 0.0, slope_middle - 0.02 - half_width);
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

/// Dropped onto the face of any steep ramp, a player slides off it:
/// steeper than the run surface is never ground. How many rides ran.
fn ramp_faces_hold_no_player(slides: Slides) -> anyhow::Result<usize> {
    let Slides {
        mut sim,
        ramps,
        stride,
        slope_middle,
        ..
    } = slides;
    let (mut rides, mut held) = (0, vec![]);
    for (position, turns) in ramps.iter().step_by(stride) {
        let feet = on_slope(slope_middle, *position, *turns);
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
    assert!(rides > 0);
    assert!(held.is_empty());
    Ok(rides)
}

mod no_steep_ramp_face_holds_a_player {
    use super::*;

    #[test]
    fn synthetic() -> anyhow::Result<()> {
        // Every lone ramp's face is open to drop a player on.
        assert!(ramp_faces_hold_no_player(Slides::synthetic()?)? >= LONE_RAMPS);
        Ok(())
    }

    #[test]
    #[ignore = "requires generated v20 content"]
    fn content() -> anyhow::Result<()> {
        let rides = ramp_faces_hold_no_player(Slides::content()?)?;
        assert!(rides * sample(90) > 3000);
        Ok(())
    }
}

/// What riding every downhill lane found.
struct LaneRides {
    rides: usize,
    /// Descents, shortest first.
    descents: Vec<f32>,
}

/// Pushed gently downhill into any slide lane, a player rides it hands-free
/// to the end of its leg: no seam between ramps catches it on the way.
fn lanes_carry_riders_down(slides: Slides) -> anyhow::Result<LaneRides> {
    let mut lanes = lanes(&slides);
    let (mut sim, stride) = (slides.sim, slides.stride);
    // Highest first, so the sample always includes a ride from the top.
    lanes.sort_by(|a, b| b.feet.y.total_cmp(&a.feet.y));
    let (mut rides, mut caught, mut descents, mut speeds) = (0, vec![], vec![], vec![]);
    let downhill_lanes: Vec<_> = lanes
        .iter()
        .filter_map(|lane| downhill(&lanes, lane).map(|down| (lane, down)))
        .collect();
    for (lane, down) in downhill_lanes.iter().step_by(stride) {
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
    assert!(rides > 0);
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
    // One spot remains on the save: riders who drop down a shaft at over
    // 20 u/s land a hair inside the next lane's first ramp and stop at its
    // end face.
    assert!(caught.len() * 25 <= rides, "{} caught", caught.len());
    Ok(LaneRides { rides, descents })
}

mod slide_lanes_carry_a_player_down_hands_free {
    use super::*;

    #[test]
    fn synthetic() -> anyhow::Result<()> {
        let slides = Slides::synthetic()?;
        // Every lane but each leg's last steps down to another.
        let legs = 2;
        let downhill = (slides.ramps.len() - LONE_RAMPS) / 2 - legs;
        let top = lanes(&slides).iter().map(|l| l.feet.y).fold(0.0, f32::max);
        let r = lanes_carry_riders_down(slides)?;
        assert_eq!(r.rides, downhill);
        // From the top, the rider leaves the leg's foot for the floor.
        assert!(r.descents[r.descents.len() - 1] > top - 0.1);
        Ok(())
    }

    #[test]
    #[ignore = "requires generated v20 content"]
    fn content() -> anyhow::Result<()> {
        let r = lanes_carry_riders_down(Slides::content()?)?;
        assert!(r.rides * sample(90) > 800);
        assert!(r.descents[r.descents.len() - 1] > 20.0);
        Ok(())
    }
}

/// The ride from the top of the tower runs on v20's 32 ms ticks: stepping
/// the server at 120 Hz lands on exactly the positions of 32 ms Torque ticks.
/// How far down 313 ticks (ten seconds) carry the rider, and from where.
fn the_tower_ride(slides: Slides) -> anyhow::Result<(f32, Vec3)> {
    let mut lanes = lanes(&slides);
    let mut sim = slides.sim;
    lanes.sort_by(|a, b| b.feet.y.total_cmp(&a.feet.y));
    let (lane, down) = lanes
        .iter()
        .find_map(|lane| downhill(&lanes, lane).map(|down| (*lane, down)))
        .unwrap();
    let physics = &mut sim.physics;
    let ride = |physics: &mut PhysicsWorld, stepped: bool| {
        let mut p = Player::spawn(physics, 1, lane.feet, PlayerTuning::default()).unwrap();
        p.set_motion(down * 2.0, false);
        let mut ticks = vec![];
        while ticks.len() < 313 {
            if stepped {
                if p.step(physics, MoveInput::default()).unwrap().ticked {
                    ticks.push((Vec3::from(p.state().feet), Vec3::from(p.state().velocity)));
                }
            } else {
                p.torque_tick(physics, MoveInput::default(), &[], 0.032)
                    .unwrap();
                ticks.push((Vec3::from(p.state().feet), Vec3::from(p.state().velocity)));
            }
        }
        p.despawn(physics);
        ticks
    };
    let stepped = ride(physics, true);
    let ticked = ride(physics, false);
    assert_eq!(stepped, ticked);
    let end = stepped.last().unwrap().0;
    let top = stepped.iter().map(|(_, v)| v.length()).fold(0.0, f32::max);
    eprintln!(
        "from {} after 10 s: {end}, {:.1} down, top speed {top:.1}",
        lane.feet,
        lane.feet.y - end.y
    );
    Ok((lane.feet.y - end.y, lane.feet))
}

mod the_tower_ride_runs_on_v20_ticks {
    use super::*;

    #[test]
    fn synthetic() -> anyhow::Result<()> {
        // The made-up tower is short: the rider reaches the floor.
        let (down, from) = the_tower_ride(Slides::synthetic()?)?;
        assert!(down > from.y - 0.1, "{down} down from {from}");
        Ok(())
    }

    /// It carries the rider hundreds of units down the save's tower. Run at
    /// 120 Hz, the same rules stalled it 39 units down in a lane.
    #[test]
    #[ignore = "requires generated v20 content"]
    fn content() -> anyhow::Result<()> {
        let (down, _) = the_tower_ride(Slides::content()?)?;
        assert!(down > 300.0);
        Ok(())
    }
}
