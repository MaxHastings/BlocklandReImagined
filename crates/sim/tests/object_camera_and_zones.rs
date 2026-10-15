//! Two seams games that are not about a body need (a golf ball, a pushed
//! crate, a race car crossing a line): a rule's camera that follows an
//! object (`orbit_object`), and zones that notice objects as well as
//! players (`"objects": true`, `on_zone_object`).
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{Catalog, ops::ObjectRef};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{Command, ControlObject, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

const CUP: &str = "v20/brick/cup";
const BALL: &str = "steel-ball-kit:vehicle/steelball";

const SCRIPT: &str = r#"
fn cmd_watch(p) { orbit_object(p, objects()[0].ref, 6.0); }
fn cmd_stray(p) { orbit_object(p, "vehicle:999", 6.0); }
fn cmd_hit(p) { push(objects()[0].ref, 30.0, 6.0, 0.0); }
fn on_zone(p, b, e) {}
fn on_zone_object(o, b, e) {
    let heard = get("heard");
    heard.push(`${o} ${e}`);
    set("heard", heard);
}
"#;

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn add_on() -> (Root, Arc<Catalog>) {
    let root = Root(std::env::temp_dir().join(format!("bri-object-zones-{}", std::process::id())));
    let dir = root.0.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
        "name": "probe", "license": "CC0-1.0",
        "capabilities": ["player", "physics"],
        "provides": [
            { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "probe:script/main", "file": "main.rhai" }
        ]
    });
    let behaviour = json!({
        "schema_version": 1,
        "script": "main.rhai",
        "commands": [ { "name": "watch" }, { "name": "stray" }, { "name": "hit" } ],
        "state": { "global": {
            "heard": { "default": [], "visible": "everyone", "persist": false }
        } },
        "zones": [ { "bricks": [CUP], "objects": true, "period_ms": 10 } ]
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
    std::fs::write(dir.join("main.rhai"), SCRIPT).unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "probe".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "probe".into(),
            role: None,
        }],
    };
    let catalog = Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));
    (root, Arc::new(catalog))
}

/// A 2x2 brick a unit wide and 0.6 tall: the cup.
fn definitions() -> Definitions {
    let collision = CollisionBody {
        id: CUP.into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.6, 1.0],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [(
            CUP.into(),
            Definition {
                mesh: Mesh {
                    schema_version: 1,
                    id: CUP.into(),
                    footprint_studs: [2, 2],
                    height_plates: 3,
                    attachment_rows: vec!["bb".into(); 6],
                    collision_boxes: vec![],
                    needs_external_collision: false,
                    coverage: None,
                    quads: vec![],
                },
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
                bot: None,
            },
        )]
        .into(),
    }
}

fn session() -> (Root, Session) {
    let mut world = World::new("Golf".into(), "golf".into(), vec![[1.0; 4]]);
    world.bricks.insert(
        1,
        bri_world::Brick::new(
            bri_world::ContentRef::Resolved(CUP.into()),
            [8.0, 0.3, 0.0],
            0,
        ),
    );
    world.next_brick_id = 2;
    let mut s = Session::new(
        Simulation::new(
            world,
            definitions(),
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 6.0)]).unwrap();
    s.set_vehicle_pack(
        bri_vehicles::Pack::load(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../packages/showcase/steel-ball-kit/assets/vehicles.json"),
        )
        .unwrap(),
        Vec::new(),
    )
    .unwrap();
    let (root, catalog) = add_on();
    s.install_packages(catalog, None).unwrap();
    (root, s)
}

fn steps(s: &mut Session, p: OwnerId, n: u64) {
    for _ in 0..n {
        let seq = 1_000_000 + s.simulation().state().tick;
        s.movement(p, seq, MoveInput::default()).unwrap();
        s.step().unwrap();
    }
}

fn run(s: &mut Session, p: OwnerId, seq: u64, name: &str) -> anyhow::Result<()> {
    s.command(
        p,
        seq,
        Command::Package(PackageCommand {
            package: "probe".into(),
            command: name.into(),
            args: Vec::<PackageArg>::new(),
        }),
    )
    .map(|_| ())
}

fn heard(s: &Session) -> Vec<String> {
    s.package_state().packages["probe"].global["heard"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

/// A ball dropped on the cup enters its zone, a camera follows it as it
/// falls and flies, and it leaves the zone when struck away.
#[test]
fn a_ball_is_followed_and_noticed_by_its_cup() {
    let (_root, mut s) = session();
    let p = s
        .join("Golfer".into(), Vec3::new(0.0, 0.05, 6.0), false)
        .unwrap();
    let ball = s
        .spawn_vehicle_at(0, BALL, Vec3::new(8.0, 4.0, 0.0), 0.0, Vec3::ZERO)
        .unwrap();
    let reference = ObjectRef::Vehicle(ball).to_string();

    // An object that is not there gets no camera.
    let _ = run(&mut s, p, 1, "stray");
    steps(&mut s, p, 1);
    assert_eq!(s.control(p), Some(ControlObject::Player));
    run(&mut s, p, 2, "watch").unwrap();
    steps(&mut s, p, 1);
    assert_eq!(s.control(p), Some(ControlObject::Point));
    let first = s.vitals()[&p].camera_point.expect("the point replicates");
    assert_eq!(first.object, Some(ObjectRef::Vehicle(ball)));
    assert_eq!(first.distance, 6.0);

    // It falls onto the cup: the camera goes down with it, and the zone
    // hears it come in once.
    steps(&mut s, p, 240);
    let landed = s.vitals()[&p].camera_point.unwrap();
    assert!(
        landed.at[1] < first.at[1] - 1.0,
        "{first:?} then {landed:?}"
    );
    assert!((landed.at[0] - 8.0).abs() < 1.0, "{landed:?}");
    assert_eq!(heard(&s), vec![format!("{reference} enter")]);

    // Struck away, it leaves, and the camera goes with it.
    run(&mut s, p, 3, "hit").unwrap();
    steps(&mut s, p, 60);
    let flown = s.vitals()[&p].camera_point.unwrap();
    assert!(
        flown.at[0] > landed.at[0] + 3.0,
        "{landed:?} then {flown:?}"
    );
    assert_eq!(
        heard(&s),
        vec![format!("{reference} enter"), format!("{reference} leave")]
    );
}
