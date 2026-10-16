//! Invented Wasm callbacks through real ClientCode, in App render order.
//! Strict same-draw requirements are deliberately ignored/red until the engine
//! supplies current context before camera selection. No original package code.
use anyhow::{Result, ensure};
use bri_client::client_code::{ClientCode, Host};
use bri_client_sandbox::{
    Shot, View, World,
    host::LOOK_CONTROL,
    world::{Held, Player},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use glam::{Mat4, Vec3};
use std::sync::Arc;

const IMAGE: &str = "fixture:image/tool";
const EYE: Vec3 = Vec3::new(3., 4., 5.);

// A tiny binary encoder keeps this fixture source-free without a compiler,
// generated binary, new dependency or production test hook.
fn uint(mut n: u32, out: &mut Vec<u8>) {
    loop {
        let byte = (n & 127) as u8;
        n >>= 7;
        out.push(byte | if n == 0 { 0 } else { 128 });
        if n == 0 {
            break;
        }
    }
}
fn int(mut n: i32, out: &mut Vec<u8>) {
    loop {
        let byte = (n & 127) as u8;
        n >>= 7;
        let done = (n == 0 && byte & 64 == 0) || (n == -1 && byte & 64 != 0);
        out.push(byte | if done { 0 } else { 128 });
        if done {
            break;
        }
    }
}
fn text(s: &str, out: &mut Vec<u8>) {
    uint(s.len() as u32, out);
    out.extend(s.bytes());
}
#[derive(Default)]
struct Code(Vec<u8>);
impl Code {
    fn i(&mut self, n: i32) {
        self.0.push(0x41);
        int(n, &mut self.0);
    }
    fn call(&mut self, n: u8) {
        self.0.extend([0x10, n]);
    }
    fn op(&mut self, ops: &[u8]) {
        self.0.extend(ops);
    }
    fn load(&mut self, at: i32, float: bool) {
        self.i(at);
        self.op(&[if float { 0x2a } else { 0x28 }, 2, 0]);
    }
    fn store_i(&mut self, at: i32, value: i32) {
        self.i(at);
        self.i(value);
        self.op(&[0x36, 2, 0]);
    }
    fn zero_float(&mut self, at: i32) {
        self.store_i(at, 0);
    }
    fn done(mut self) -> Vec<u8> {
        self.0.push(0x0b);
        self.0
    }
}
fn module() -> Vec<u8> {
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    let mut section = |id: u8, data: Vec<u8>| {
        wasm.push(id);
        uint(data.len() as u32, &mut wasm);
        wasm.extend(data);
    };
    // Types: i->i, i->(), ii->i, ()->i, iii->i, ()->(), ff->().
    section(
        1,
        vec![
            7, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 1, 0x7f, 0, 0x60, 2, 0x7f, 0x7f, 1, 0x7f, 0x60, 0, 1,
            0x7f, 0x60, 3, 0x7f, 0x7f, 0x7f, 1, 0x7f, 0x60, 0, 0, 0x60, 2, 0x7d, 0x7d, 0,
        ],
    );
    let mut imports = vec![11];
    for (name, ty) in [
        ("input_claim", 0),
        ("view", 1),
        ("players", 2),
        ("local_player", 3),
        ("life", 0),
        ("held", 4),
        ("image_kind", 2),
        ("image_mesh", 0),
        ("look_delta", 1),
        ("camera", 1),
        ("camera_set", 0),
    ] {
        text("bri", &mut imports);
        text(name, &mut imports);
        imports.extend([0, ty]);
    }
    section(2, imports);
    section(3, vec![4, 5, 3, 6, 6]); // init11, scope12, frame13, camera14
    section(5, vec![1, 0, 1]);
    let mut exports = vec![4];
    for (name, kind, index) in [
        ("memory", 2, 0),
        ("init", 0, 11),
        ("frame", 0, 13),
        ("camera_frame", 0, 14),
    ] {
        text(name, &mut exports);
        exports.extend([kind, index]);
    }
    section(7, exports);
    let mut init = Code(vec![0]);
    init.i(300);
    init.i(512);
    init.i(IMAGE.len() as i32);
    init.call(6);
    init.op(&[0x36, 2, 0]);
    init.store_i(304, -1);
    init.store_i(320, -1);
    init.i(0);
    init.call(0);
    init.op(&[0x1a]);
    // Locals eligible, granted, life. Check actual view, local player, image,
    // held transform AND mesh availability in both callbacks.
    let mut scope = Code(vec![1, 3, 0x7f]);
    scope.i(0);
    scope.call(1);
    scope.i(64);
    scope.i(1);
    scope.call(2);
    scope.i(1);
    scope.op(&[0x46]);
    scope.load(20, true);
    scope.op(&[0xa8]);
    scope.i(5);
    scope.op(&[0x71]);
    scope.i(5);
    scope.op(&[0x46, 0x71]);
    scope.load(68, true);
    scope.op(&[0xa8]);
    scope.i(3);
    scope.op(&[0x71]);
    scope.i(3);
    scope.op(&[0x46, 0x71]);
    scope.load(124, true);
    scope.op(&[0xa8]);
    scope.load(300, false);
    scope.op(&[0x46, 0x71]);
    scope.call(3);
    scope.i(0);
    scope.i(128);
    scope.call(5);
    scope.i(1);
    scope.op(&[0x46, 0x71]);
    scope.load(300, false);
    scope.call(7);
    scope.i(0);
    scope.op(&[0x4e, 0x71, 0x21, 0]);
    scope.i(LOOK_CONTROL as i32);
    scope.i(0);
    scope.op(&[0x20, 0, 0x1b]);
    scope.call(0);
    scope.op(&[0x21, 1]);
    scope.call(3);
    scope.call(4);
    scope.op(&[0x21, 2]);
    scope.op(&[0x20, 0, 0x45, 0x20, 1, 0x45, 0x72, 0x20, 2]);
    scope.load(304, false);
    scope.op(&[0x47, 0x72, 0x04, 0x40]);
    scope.zero_float(308);
    scope.zero_float(312);
    scope.op(&[0x0b]);
    scope.i(304);
    scope.op(&[0x20, 2, 0x36, 2, 0, 0x20, 0]);
    let mut frame = Code(vec![0]);
    frame.call(12);
    frame.op(&[0x1a]);
    frame.i(316);
    frame.load(316, false);
    frame.i(1);
    frame.op(&[0x6a, 0x36, 2, 0]);
    let mut camera = Code(vec![0]);
    camera.call(12);
    camera.op(&[0x45, 0x04, 0x40, 0x0f, 0x0b]);
    camera.load(316, false);
    camera.load(320, false);
    camera.op(&[0x47, 0x04, 0x40]);
    camera.i(256);
    camera.call(8);
    for (at, delta) in [(308, 256), (312, 260)] {
        camera.i(at);
        camera.load(at, true);
        camera.load(delta, true);
        camera.op(&[0x92, 0x38, 2, 0]);
    }
    camera.i(320);
    camera.load(316, false);
    camera.op(&[0x36, 2, 0, 0x0b]);
    camera.i(224);
    camera.call(9);
    // Deliberately invented linear look law; host normalizes. No source law.
    for (at, offset) in [(236, 308), (240, 312)] {
        camera.i(at);
        camera.load(at, true);
        camera.load(offset, true);
        camera.op(&[0x92, 0x38, 2, 0]);
    }
    camera.i(224);
    camera.call(10);
    camera.op(&[0x1a]);
    let mut bodies = vec![4];
    for body in [init.done(), scope.done(), frame.done(), camera.done()] {
        uint(body.len() as u32, &mut bodies);
        bodies.extend(body);
    }
    section(10, bodies);
    let mut data = vec![1, 0];
    data.push(0x41);
    int(512, &mut data);
    data.push(0x0b);
    text(IMAGE, &mut data);
    section(11, data);
    wasm
}

struct Fixture {
    _root: tempfile::TempDir,
    state: tempfile::TempDir,
    code: ClientCode,
    now: f64,
}
impl Fixture {
    fn new(ids: &[&str]) -> Result<Self> {
        let root = tempfile::tempdir()?;
        let mut packages = Vec::new();
        for id in ids {
            let dir = root.path().join(id);
            std::fs::create_dir_all(&dir)?;
            std::fs::write(dir.join("main.wasm"), module())?;
            let manifest = serde_json::json!({"id":id,"version":"1.0.0","name":id,
                "client":{"module":"main.wasm","capabilities":["render.layer","world.read","input.capture","camera.control"]}});
            std::fs::write(dir.join("package.json"), serde_json::to_vec(&manifest)?)?;
            packages.push(PackageEntry {
                id: (*id).into(),
                version: "1.0.0".into(),
                side: Side::Shared,
                dir: (*id).into(),
                role: None,
            });
        }
        let mut code = ClientCode::load(
            root.path(),
            &PackageSet {
                schema_version: 1,
                packages,
            },
        );
        let state = tempfile::tempdir()?;
        code.start(Host::Local, state.path());
        ensure!(
            code.running().len() == ids.len(),
            "load/start failed: {:?}",
            code.take_messages()
        );
        Ok(Self {
            _root: root,
            state,
            code,
            now: 0.,
        })
    }
    /// Same production API order as App::render_frame: camera then frame.
    /// No shim refreshes host snapshots, and no callback is simulated in Rust.
    fn draw(&mut self, world: Arc<World>, view: View) -> Option<Shot> {
        self.now += 0.016;
        let shot = self.code.camera_shot(EYE, Vec3::NEG_Z);
        self.code.run_frame(self.now, EYE, Vec3::NEG_Z, world, view);
        shot
    }
    fn acquire(&mut self) -> Result<()> {
        for _ in 0..3 {
            self.draw(world(true, true, true, 7), View::default());
        }
        ensure!(self.code.captured_controls() == LOOK_CONTROL);
        ensure!(
            self.draw(world(true, true, true, 7), View::default())
                .is_some()
        );
        Ok(())
    }
}
fn world(alive: bool, equipped: bool, geometry: bool, life: u64) -> Arc<World> {
    let mut w = World {
        local: 1,
        ..Default::default()
    };
    w.players.push(Player {
        id: 1,
        alive,
        life,
        eye: EYE.to_array(),
        look: Vec3::NEG_Z.to_array(),
        image: if equipped {
            IMAGE.into()
        } else {
            String::new()
        },
        held: if equipped {
            vec![Held {
                hand: 0,
                transform: Mat4::from_translation(EYE).to_cols_array(),
                muzzle: None,
            }]
        } else {
            vec![]
        },
        ..Default::default()
    });
    if geometry {
        use bri_client_sandbox::host::{Mesh, Vertex};
        let vertices = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
            .map(|position| Vertex {
                position,
                normal: [0., 0., 1.],
                uv: [0.; 2],
            })
            .to_vec();
        w.image_meshes.insert(
            IMAGE.into(),
            Arc::new(Mesh {
                vertices,
                indices: vec![0, 1, 2],
            }),
        );
    }
    Arc::new(w)
}
fn direction(shot: Option<Shot>, expected: Vec3) -> Result<()> {
    let shot = shot.ok_or_else(|| anyhow::anyhow!("expected a positively acquired camera"))?;
    ensure!(shot.eye == EYE.to_array() && shot.fov.is_none());
    ensure!(
        Vec3::from(shot.forward).distance(expected.normalize()) < 1e-6,
        "camera {:?}, expected {:?}",
        shot.forward,
        expected.normalize()
    );
    Ok(())
}

#[test]
fn actual_unique_stock_and_two_owner_orders_keep_existing_arbitration() -> Result<()> {
    for id in ["fixture-a", "fixture-b"] {
        let mut f = Fixture::new(&[id])?;
        f.acquire()?;
    }
    for ids in [
        vec![],
        vec!["fixture-a", "fixture-b"],
        vec!["fixture-b", "fixture-a"],
    ] {
        let mut f = Fixture::new(&ids)?;
        for _ in 0..4 {
            ensure!(
                f.draw(world(true, true, true, 7), View::default())
                    .is_none()
            );
        }
        ensure!(f.code.captured_controls() == 0 && !f.code.capture_look(0.2, 0.1));
    }
    // Matching selectors and held metadata alone cannot acquire without mesh.
    let mut f = Fixture::new(&["fixture-a"])?;
    for _ in 0..4 {
        ensure!(
            f.draw(world(true, true, false, 7), View::default())
                .is_none()
        );
    }
    ensure!(f.code.captured_controls() == 0);
    Ok(())
}

#[test]
fn delivered_look_is_not_replayed_and_disconnect_device_loss_clear_immediately() -> Result<()> {
    let mut f = Fixture::new(&["fixture-a"])?;
    f.acquire()?;
    ensure!(f.code.capture_look(0.12, -0.06));
    f.draw(world(true, true, true, 7), View::default()); // current known delay
    let expected = Vec3::new(0.12, -0.06, -1.);
    for _ in 0..3 {
        direction(
            f.draw(world(true, true, true, 7), View::default()),
            expected,
        )?;
    }
    f.code.stop();
    ensure!(f.code.captured_controls() == 0 && !f.code.capture_look(1., 1.));
    ensure!(f.code.camera_shot(EYE, Vec3::NEG_Z).is_none());
    f.code.start(Host::Local, f.state.path());
    f.acquire()?;
    direction(
        f.draw(world(true, true, true, 8), View::default()),
        Vec3::NEG_Z,
    )?;
    f.code.device_lost();
    ensure!(f.code.captured_controls() == 0 && !f.code.capture_look(1., 1.));
    ensure!(f.code.camera_shot(EYE, Vec3::NEG_Z).is_none());
    Ok(())
}

#[test]
#[ignore = "strict red requirement: current draw camera still reads previous frame input"]
fn current_look_reaches_this_draw_without_replay() -> Result<()> {
    let mut f = Fixture::new(&["fixture-a"])?;
    f.acquire()?;
    ensure!(f.code.capture_look(0.12, -0.06));
    let expected = Vec3::new(0.12, -0.06, -1.);
    for _ in 0..3 {
        direction(
            f.draw(world(true, true, true, 7), View::default()),
            expected,
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "strict red requirement: current death/unequip/first-person exit retains old camera"]
fn current_lifecycle_selectors_suppress_camera_in_the_same_draw() -> Result<()> {
    let mut stale = Vec::new();
    for transition in ["death", "unequip", "first-person exit"] {
        let mut f = Fixture::new(&["fixture-a"])?;
        f.acquire()?;
        let view = View {
            first_person: transition != "first-person exit",
            alive: transition != "death",
            ..Default::default()
        };
        let shot = f.draw(
            world(transition != "death", transition != "unequip", true, 7),
            view,
        );
        if shot.is_some() {
            stale.push(transition);
        }
        ensure!(
            f.code.captured_controls() == 0,
            "fresh frame must release LOOK for {transition}"
        );
        ensure!(
            f.draw(
                world(transition != "death", transition != "unequip", true, 7),
                view
            )
            .is_none()
        );
    }
    ensure!(
        stale.is_empty(),
        "same draw retained stale camera for {stale:?}"
    );
    Ok(())
}
