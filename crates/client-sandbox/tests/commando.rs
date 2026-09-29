//! The Commando sample's client code (`packages/samples/sample-commando-look`):
//! the rifle drawn in view space in first person, the scope drawn in
//! screen space while aiming, and nothing when the player holds something
//! else. With a GPU (`--ignored`), it renders offscreen to PNGs.
use bri_client_sandbox::{
    AddOn, AddOnCode, Budgets, Capability, FrameInput, Sandbox, Space, TrustLevel, View, World,
    world::Player,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const RIFLE: &str = "sample-commando-rifle:image/rifle";

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples/sample-commando-look")
}

fn start() -> (AddOnCode, AddOn) {
    let code = match AddOnCode::load(&dir()) {
        Ok(Some(code)) => code,
        Ok(None) => panic!("no client code"),
        Err(problems) => panic!("{problems:#?}"),
    };
    let addon = Sandbox::new()
        .unwrap()
        .start(&code, Budgets::default(), TrustLevel::Sandboxed)
        .unwrap_or_else(|e| panic!("stopped: {e}"));
    (code, addon)
}

fn world(image: &str) -> Arc<World> {
    Arc::new(World {
        local: 1,
        players: vec![
            Player {
                id: 2,
                alive: true,
                image: RIFLE.into(),
                ..Default::default()
            },
            Player {
                id: 1,
                alive: true,
                image: image.into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    })
}

fn view(aiming: bool, clip: u32) -> View {
    View {
        fov: if aiming { 20.0 } else { 90.0 },
        aiming,
        ammo: Some([clip, 24, 8]),
        ..Default::default()
    }
}

fn frame(t: f32, world: &Arc<World>, view: View) -> FrameInput {
    FrameInput {
        time: t,
        dt: 1.0 / 60.0,
        world: world.clone(),
        view,
        ..Default::default()
    }
}

#[test]
fn the_sights_module_is_built_from_its_source() {
    let built = wat::parse_file(dir().join("client/main.wat")).unwrap();
    let path = dir().join("client/main.wasm");
    if std::env::var_os("BRI_BLESS").is_some() {
        std::fs::write(&path, &built).unwrap();
    }
    assert_eq!(
        std::fs::read(&path).unwrap_or_default(),
        built,
        "client/main.wasm is stale; rerun with BRI_BLESS=1"
    );
}

#[test]
fn the_rifle_is_drawn_in_view_space_and_the_scope_on_the_screen() {
    let (code, mut addon) = start();
    assert_eq!(code.name, "Commando Look");
    assert!(code.capabilities.contains(&Capability::WorldRead));
    let holding = world(RIFLE);
    let drawn = addon.frame(frame(0.0, &holding, view(false, 8))).unwrap().clone();
    assert_eq!(drawn.draws.len(), 6, "six boxes of rifle and hand");
    let layer = addon.layer();
    for d in &drawn.draws {
        assert_eq!(layer.materials[d.material].space, Space::View);
        assert!(d.model[14] < 0.0, "in front of the eye: {}", d.model[14]);
    }
    let rest = drawn.draws[0].model[14];
    // A round leaves the clip: the rifle kicks back towards the eye, then
    // settles.
    let kicked = addon.frame(frame(0.02, &holding, view(false, 7))).unwrap().clone();
    assert!(kicked.draws[0].model[14] > rest + 0.05);
    for i in 0..30 {
        addon
            .frame(frame(0.04 + i as f32 / 60.0, &holding, view(false, 7)))
            .unwrap();
    }
    let settled = addon.frame(frame(1.0, &holding, view(false, 7))).unwrap().clone();
    assert_eq!(settled.draws[0].model[14], rest);
    // Aiming: one full-screen scope quad in screen space, told the aspect.
    let aimed = addon.frame(frame(1.1, &holding, view(true, 7))).unwrap().clone();
    assert_eq!(aimed.draws.len(), 1);
    let scope = &aimed.draws[0];
    assert_eq!(addon.layer().materials[scope.material].space, Space::Screen);
    let params = scope.params.unwrap();
    assert_eq!(params[1][0], 1.0, "scope mode");
    assert!((params[1][1] - 1280.0 / 720.0).abs() < 1e-4, "aspect");
    // Third person: no rifle in front of the eye.
    let third = View {
        first_person: false,
        ..view(false, 7)
    };
    assert!(addon.frame(frame(1.2, &holding, third)).unwrap().draws.is_empty());
    // Another item in hand, or dead: nothing.
    let other = world("v20.image.hammerimage");
    assert!(addon.frame(frame(1.3, &other, view(false, 7))).unwrap().draws.is_empty());
    let dead = View {
        alive: false,
        ..view(false, 7)
    };
    assert!(addon.frame(frame(1.4, &holding, dead)).unwrap().draws.is_empty());
}

/// Needs a GPU: the rifle in first person, then the scope, to PNGs.
#[test]
#[ignore = "needs a GPU adapter"]
fn the_commando_sights_render_offscreen() {
    let (_, mut addon) = start();
    let (adapter, images) = bri_client_sandbox::gpu::render_offscreen_views(
        &mut addon,
        640,
        360,
        &[0.0, 1.0],
        glam::Vec3::new(0.0, 2.0, 6.0),
        glam::Vec3::new(0.0, 2.0, 0.0),
        |t| (world(RIFLE), view(t > 0.5, 8)),
    )
    .unwrap();
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("commando-preview");
    std::fs::create_dir_all(&out).unwrap();
    for (i, image) in images.iter().enumerate() {
        bri_client_sandbox::gpu::write_png(image, &out.join(format!("sights-{i:02}.png"))).unwrap();
    }
    let at = |image: &bri_client_sandbox::gpu::Image, x: u32, y: u32| {
        let i = ((y * image.width + x) * 4) as usize;
        image.pixels[i..i + 4].to_vec()
    };
    let background = at(&images[0], 5, 5);
    assert_ne!(at(&images[0], 560, 300), background, "the rifle, low right, on {adapter}");
    // The scope blacks out the corners and leaves the lens centre clear.
    assert_eq!(&at(&images[1], 5, 5)[..3], &[0, 0, 0], "black outside the lens");
    assert_eq!(at(&images[1], 330, 150), background, "clear inside the lens");
}
