//! Walking and falling through linked bricks' openings: the picture the
//! view camera sees, traced frame by frame through the openings as the
//! renderer draws them, never jumps at the crossing (Valve's Portal rule:
//! the frame before going through and the frame after show the same).
use super::*;
use crate::controls::Controls;
use bri_content::{
    brick::{Brick as Mesh, Face, Frame, Link},
    collision::{CollisionBody, Part},
    passage::Passages,
};
use bri_sim::definitions::{Definition, Definitions};
use bri_world::{Brick, ContentRef, World};
use glam::{Affine3A, Vec3};
use rapier3d::prelude::{ColliderBuilder, Vector};

const DOOR: &str = "door";
const FLOOR: &str = "floor";
/// Frames per second the walks are drawn at: fast enough that the view's
/// own motion between frames is a hair, so any jump is the crossing's.
const FPS: f32 = 1000.0;
/// A pixel whose traced point moves further than this between frames,
/// and further than [`TURN`] of the way it was seen from, jumped.
const JUMP: f32 = 0.5;
/// The most the picture turns in a frame without jumping (radians): the
/// view turning at 50 radians a second, far quicker than the roll easing
/// upright (a cut to another view moves points by far more).
const TURN: f32 = 0.05;
/// Share of the picture that may jump between two frames (edges of the
/// openings and frames sliding across a pixel or two).
const MOST_JUMPED: f32 = 0.05;
/// Frames either side of the crossing a chase camera is checked over.
const CHASE_REACH: usize = 100;

fn definition(id: &str, footprint: [u32; 2], plates: u32, link: Link) -> (String, Definition) {
    let mesh = Mesh {
        schema_version: 1,
        id: id.into(),
        footprint_studs: footprint,
        height_plates: plates,
        attachment_rows: vec!["b".repeat(footprint[0] as usize); footprint[1] as usize],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: id.into(),
        parts: link
            .frame_boxes(&mesh)
            .into_iter()
            .map(|b| Part::Box {
                center: b.center,
                size: b.size,
            })
            .collect(),
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    (
        id.into(),
        Definition {
            mesh,
            collision,
            shape,
            indestructible: false,
            special: Default::default(),
            reflection: None,
            link: Some(link),
            glass: [0.0; 4],
            bot: None,
        },
    )
}

/// The stock doorway (a 1x4x5 window opening both ways through its
/// middle) and a 4x4 plate opening upward, as the Portal Add-On's bricks.
fn definitions() -> Definitions {
    let link = |faces: Vec<Face>, depth: f32| Link {
        faces,
        depth,
        inset: 0.0,
        tint: [1.0; 3],
        idle: [0.5; 3],
        pass: true,
        frame: Frame {
            sides: 0.05,
            top: 0.05,
            bottom: 0.2,
        },
        name: "Portal".into(),
    };
    Definitions {
        entries: [
            definition(DOOR, [4, 1], 15, link(vec![Face::North, Face::South], 0.5)),
            definition(FLOOR, [4, 4], 3, link(vec![Face::Top], 0.0)),
        ]
        .into(),
    }
}

/// A world of these bricks (definition, position, quarter turns), all of
/// one pair's name, on a floor in a closed room.
struct Scene {
    bricks: World,
    /// Everything solid as world boxes (min, max).
    boxes: Vec<(Vec3, Vec3)>,
    passages: Passages,
}
impl Scene {
    fn new(bricks: &[(&str, [f32; 3], u8)]) -> Self {
        let mut world = World::new("Portals".into(), "test".into(), vec![[1.0; 4]]);
        for (i, (definition, position, turns)) in bricks.iter().enumerate() {
            let mut brick = Brick::new(ContentRef::Resolved((*definition).into()), *position, 1);
            brick.quarter_turns = *turns;
            brick.name = Some("Portal_a".into());
            world.bricks.insert(i as u64 + 1, brick);
            world.next_brick_id = i as u64 + 2;
        }
        let definitions = definitions();
        let mut boxes = vec![
            // Floor, walls and ceiling of a 60 unit room.
            (Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 30.0)),
            (Vec3::new(-31.0, 0.0, -30.0), Vec3::new(-30.0, 30.0, 30.0)),
            (Vec3::new(30.0, 0.0, -30.0), Vec3::new(31.0, 30.0, 30.0)),
            (Vec3::new(-30.0, 0.0, -31.0), Vec3::new(30.0, 30.0, -30.0)),
            (Vec3::new(-30.0, 0.0, 30.0), Vec3::new(30.0, 30.0, 31.0)),
            (Vec3::new(-30.0, 30.0, -30.0), Vec3::new(30.0, 31.0, 30.0)),
        ];
        for brick in world.bricks.values() {
            let ContentRef::Resolved(name) = &brick.definition else {
                continue;
            };
            let d = &definitions.entries[name];
            let frame = Affine3A::from_mat4(brick.transform());
            for b in d.link.as_ref().unwrap().frame_boxes(&d.mesh) {
                let half = Vec3::from(b.size) * 0.5;
                let (a, c) = (
                    frame.transform_point3(Vec3::from(b.center) - half),
                    frame.transform_point3(Vec3::from(b.center) + half),
                );
                boxes.push((a.min(c), a.max(c)));
            }
        }
        let mut mirror = CollisionMirror::new(definitions, vec![], vec![]);
        mirror.sync(&world.bricks).unwrap();
        let passages = mirror.links().passages().clone();
        assert_eq!(passages.list.len(), bricks.len() * bricks_faces(bricks));
        Scene {
            bricks: world,
            boxes,
            passages,
        }
    }
    /// A collision mirror of the scene, for prediction.
    fn mirror(&self) -> CollisionMirror {
        let mut mirror = CollisionMirror::new(
            definitions(),
            vec![ColliderBuilder::cuboid(30.0, 0.5, 30.0).translation(Vector::new(0.0, -0.5, 0.0))],
            vec![],
        );
        mirror.sync(&self.bricks.bricks).unwrap();
        mirror
    }
    /// Where a ray from `eye` along `direction` meets something, going on
    /// through every opening it goes in through, as the renderer draws them.
    fn trace(&self, eye: Vec3, direction: Vec3) -> Option<(Vec3, f32)> {
        const FAR: f32 = 200.0;
        let (mut eye, mut direction) = (eye, direction.normalize());
        let mut length = 0.0;
        for _ in 0..=bri_content::passage::MAX_CARRIES {
            let hit = self
                .boxes
                .iter()
                .filter_map(|(low, high)| slab(eye, direction, *low, *high))
                .fold(FAR, f32::min);
            match self.passages.first(eye, eye + direction * FAR) {
                Some((passage, t)) if t * FAR < hit => {
                    let carry = passage.carry;
                    let through = carry.transform_point3(eye + direction * t * FAR);
                    length += t * FAR;
                    direction = carry.transform_vector3(direction);
                    // On, a hair past the far side's plane (back to back
                    // with another opening, as a doorway's two sides are).
                    eye = through + direction * 1e-4;
                }
                _ => return (hit < FAR).then(|| (eye + direction * hit, length + hit)),
            }
        }
        None
    }
    /// The traced point under each pixel of a 90 degree view, and how far
    /// along the ray it is.
    fn picture(&self, eye: Vec3, look: (f32, f32, f32)) -> Vec<Option<(Vec3, f32)>> {
        const SIZE: (usize, usize) = (48, 32);
        let rotation = crate::portal_view::view_rotation(look);
        let mut out = Vec::with_capacity(SIZE.0 * SIZE.1);
        for y in 0..SIZE.1 {
            for x in 0..SIZE.0 {
                let sx = (x as f32 + 0.5) / SIZE.0 as f32 * 2.0 - 1.0;
                let sy = 1.0 - (y as f32 + 0.5) / SIZE.1 as f32 * 2.0;
                let ray = Vec3::new(sx, sy * SIZE.1 as f32 / SIZE.0 as f32, -1.0);
                out.push(self.trace(eye, rotation * ray));
            }
        }
        out
    }
}
fn bricks_faces(bricks: &[(&str, [f32; 3], u8)]) -> usize {
    match bricks[0].0 {
        DOOR => 2,
        _ => 1,
    }
}

/// Entry distance of a ray into a box, past a hair in front of the eye.
fn slab(eye: Vec3, direction: Vec3, low: Vec3, high: Vec3) -> Option<f32> {
    let (mut near, mut far) = (1e-4f32, f32::MAX);
    for a in 0..3 {
        if direction[a].abs() < 1e-9 {
            if eye[a] < low[a] || eye[a] > high[a] {
                return None;
            }
            continue;
        }
        let (t0, t1) = (
            (low[a] - eye[a]) / direction[a],
            (high[a] - eye[a]) / direction[a],
        );
        near = near.max(t0.min(t1));
        far = far.min(t0.max(t1));
    }
    (near <= far).then_some(near)
}

/// Share of the pixels whose traced point jumped between two frames.
fn jumped(a: &[Option<(Vec3, f32)>], b: &[Option<(Vec3, f32)>]) -> f32 {
    let count = a
        .iter()
        .zip(b)
        .filter(|(a, b)| match (a, b) {
            (Some((a, far)), Some((b, _))) => {
                let moved = a.distance(*b);
                moved > JUMP && moved > TURN * far
            }
            (None, None) => false,
            _ => true,
        })
        .count();
    count as f32 / a.len() as f32
}

#[derive(Clone, Copy, PartialEq)]
enum Camera {
    FirstPerson,
    Chase,
}

/// What a walk saw: the share of the picture that jumped at each frame
/// (from the one before), and the frame the view went through.
struct Walk {
    jumps: Vec<f32>,
    crossed: Option<usize>,
}

/// Run the local player's prediction from `feet`, moving `forward` while
/// looking (yaw, pitch), for `seconds` at [`FPS`], with the game's own
/// order each frame: advance, turn the look by any opening passed, present,
/// place the camera. Traces the picture every frame.
fn walk(
    scene: &Scene,
    feet: Vec3,
    look: (f32, f32),
    forward: f32,
    seconds: f32,
    camera: Camera,
) -> Walk {
    let archetypes = bri_sim::archetype::Archetypes::default();
    let mut motion = Motion::default();
    motion.install(scene.mirror());
    let mut player = PlayerState {
        owner: 2,
        feet: feet.to_array(),
        velocity: [0.0; 3],
        yaw: look.0,
        pitch: 0.0,
        head_yaw: 0.0,
        grounded: false,
        crouched: false,
        jetting: false,
        jump: Default::default(),
        archetype: Default::default(),
        scale: 1.0,
        speed_scale: 1.0,
        energy: 100.0,
        tick: Default::default(),
        tether: None,
    };
    player.tick.feet = player.feet;
    player.tick.from = player.feet;
    let pose = bri_net::protocol::Pose {
        tick: 0,
        acknowledged_input: 0,
        spawn_tick: 0,
        player,
    };
    motion.observe_local(&pose, &archetypes).unwrap();
    let mut controls = Controls::default();
    (controls.yaw, controls.pitch) = look;
    let passages = scene.passages.clone();
    let start = feet;
    let mut last: Option<Vec<Option<(Vec3, f32)>>> = None;
    let mut jumps = Vec::new();
    let mut crossed = None;
    let frames = (seconds * FPS) as usize;
    for _ in 0..frames {
        let input = MoveInput {
            forward,
            yaw: controls.yaw,
            pitch: controls.pitch,
            ..Default::default()
        };
        motion.advance(1.0 / FPS, input, 4).unwrap();
        if let Some(carry) = motion.take_passed() {
            controls.carry_look(&carry);
            crossed = crossed.or(Some(jumps.len()));
        }
        controls.ease_roll(1.0 / FPS);
        assert!(motion.present_local(2, controls.yaw, controls.pitch, 0.0));
        let local = motion.presented[&2].clone();
        let feet = Vec3::from(local.feet);
        let middle = feet + Vec3::Y * bri_sim::player::nominal_middle(local.scale);
        let (yaw, pitch) = (controls.yaw, controls.pitch);
        let roll = controls.portal_roll();
        // As `App::view_camera` places the camera.
        let tilt = controls.portal_tilt();
        let (eye, look) = match camera {
            Camera::FirstPerson => crate::portal_view::through(
                middle + tilt * (motion.local_eye().unwrap() - middle),
                (yaw, pitch, roll),
                None,
                middle,
                &passages,
            ),
            Camera::Chase => {
                let height = archetypes.tuning(local.archetype, local.scale).stand_height;
                let (distance, pivot, lean) = crate::app::pivot_camera(
                    height,
                    local.scale,
                    crate::app::PLAYER_CAMERA,
                    feet,
                    1.0,
                );
                let pivot = middle + tilt * (pivot - middle);
                let (yaw, pitch, roll) = crate::portal_view::leaned((yaw, pitch, roll), lean);
                let forward = Vec3::new(
                    yaw.sin() * pitch.cos(),
                    pitch.sin(),
                    -yaw.cos() * pitch.cos(),
                );
                let (eye, boom) = crate::portal_view::boom(
                    middle,
                    pivot,
                    forward,
                    distance,
                    &passages,
                    |e, f, d| Ok(e - f.normalize() * d),
                )
                .unwrap();
                crate::portal_view::through(eye, (yaw, pitch, roll), boom, middle, &passages)
            }
        };
        let picture = scene.picture(eye, look);
        if let Some(last) = &last {
            jumps.push(jumped(last, &picture));
        }
        last = Some(picture);
    }
    assert!(Vec3::from(motion.presented[&2].feet).distance(start) > 1.0);
    Walk { jumps, crossed }
}

/// No frame of the walk jumps; a chase camera's only around the crossing
/// (`reach` frames either side): afterwards its boom slides off the edge of
/// the opening it looks back through, as it would round a wall.
fn assert_seamless(name: &str, walk: &Walk, reach: Option<usize>) {
    let crossed = walk
        .crossed
        .unwrap_or_else(|| panic!("{name}: never went through"));
    let frames = match reach {
        Some(reach) => crossed.saturating_sub(reach)..(crossed + reach).min(walk.jumps.len()),
        None => 0..walk.jumps.len(),
    };
    let (frame, worst) = frames
        .clone()
        .map(|i| (i, walk.jumps[i]))
        .fold((0, 0.0f32), |a, (i, j)| if j > a.1 { (i, j) } else { a });
    let bad = frames.filter(|i| walk.jumps[*i] > MOST_JUMPED).count();
    assert!(
        bad == 0,
        "{name}: {bad} frames jumped; worst {:.0}% of the picture at frame {frame}, \
         going through at frame {crossed}",
        worst * 100.0
    );
}

/// In through the south side of a doorway walking north, out of the north
/// side of its partner turned a quarter: the picture never jumps, in first
/// person or from the chase camera.
#[test]
fn walking_through_a_doorway_never_changes_the_picture() {
    let scene = Scene::new(&[(DOOR, [0.0, 1.5, -4.25], 0), (DOOR, [10.25, 1.5, -4.0], 1)]);
    for camera in [Camera::FirstPerson, Camera::Chase] {
        let walk = walk(
            &scene,
            Vec3::new(0.0, 0.05, 0.0),
            (0.0, -0.1),
            1.0,
            1.4,
            camera,
        );
        match camera {
            Camera::FirstPerson => assert_seamless("first person", &walk, None),
            Camera::Chase => assert_seamless("chase camera", &walk, Some(CHASE_REACH)),
        }
    }
}

/// Falling into a portal in the floor while looking ahead and down comes
/// out of its partner's top flying up, the view turned over with the body:
/// the picture is the same, then the roll eases back upright.
#[test]
fn falling_through_a_floor_portal_never_changes_the_picture() {
    let scene = Scene::new(&[(FLOOR, [0.0, 0.3, 0.0], 0), (FLOOR, [12.0, 0.3, 4.0], 1)]);
    for camera in [Camera::FirstPerson, Camera::Chase] {
        let walk = walk(
            &scene,
            Vec3::new(0.0, 3.0, 0.0),
            (0.4, -0.7),
            0.0,
            0.9,
            camera,
        );
        match camera {
            Camera::FirstPerson => assert_seamless("first person, floor", &walk, None),
            Camera::Chase => assert_seamless("chase camera, floor", &walk, Some(CHASE_REACH)),
        }
    }
}
