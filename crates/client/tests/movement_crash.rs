//! Headless regression probe for the released client's reported Shift/Space crash.
//! Exercises the real converted Blockhead rig and the production avatar pose path.
use anyhow::Result;
use bri_client::avatar::{AvatarAnimationInput, AvatarAssets, AvatarMesh, HeldToolPose};
use bri_render::scene::SceneRenderer;
use bri_sim::player::PlayerState;
use bri_ui::gpu::Headless;
use std::path::Path;

fn player() -> PlayerState {
    PlayerState {
        owner: 1,
        feet: [0.0; 3],
        velocity: [0.0; 3],
        yaw: 0.0,
        pitch: 0.0,
        head_yaw: 0.0,
        grounded: true,
        crouched: false,
        jetting: false,
        jump: Default::default(),
        archetype: Default::default(),
        scale: 1.0,
        energy: 100.0,
        tick: Default::default(),
    }
}

fn sample(
    mesh: &mut AvatarMesh,
    assets: &AvatarAssets,
    renderer: &SceneRenderer,
    gpu: &Headless,
    state: &PlayerState,
    time: f64,
    animation: &AvatarAnimationInput,
) -> Result<()> {
    mesh.pose_with_animation(assets, state, time, animation)
        .and_then(|()| mesh.upload(renderer, &gpu.device, &gpu.queue))
}

#[test]
#[ignore = "requires original native avatar pack and offscreen GPU; no window"]
fn original_avatar_survives_held_crouch_jump_and_locomotion_transitions() -> Result<()> {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-001");
    let assets = AvatarAssets::load(&content)?;
    let mut mesh = assets.mesh(assets.package.defaults.clone())?;
    let gpu = Headless::new()?;
    let renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let mut state = player();
    let mut time = 0.0;

    let no_tool = AvatarAnimationInput::default();
    // Mimic holding Shift while idle and moving, then releasing it.
    for velocity in [[0.0, 0.0, 0.0], [0.0, 0.0, -3.0], [2.0, 0.0, 0.0]] {
        state.crouched = true;
        state.velocity = velocity;
        for _ in 0..120 {
            time += 1.0 / 60.0;
            sample(&mut mesh, &assets, &renderer, &gpu, &state, time, &no_tool)?;
        }
    }
    state.crouched = false;
    state.velocity = [0.0; 3];
    for _ in 0..4 {
        time += 1.0 / 60.0;
        sample(&mut mesh, &assets, &renderer, &gpu, &state, time, &no_tool)?;
    }

    // Basic ascent/descent and actual jet pose both succeed in the production
    // sampler, while checking vertex uploads through successive pose changes.
    state.grounded = false;
    state.velocity = [0.0, 12.0, 0.0];
    for frame in 0..240 {
        state.velocity[1] = if frame < 120 { 12.0 } else { -12.0 };
        time += 1.0 / 60.0;
        sample(&mut mesh, &assets, &renderer, &gpu, &state, time, &no_tool)?;
    }

    // These combinations previously failed because the additive jump clip was
    // ordered before the absolute crouch/held-arm overlays.
    for (crouched, held_tool) in [
        (true, HeldToolPose::None),
        (false, HeldToolPose::Right),
        (false, HeldToolPose::Both),
    ] {
        state.grounded = false;
        state.crouched = crouched;
        state.jetting = false;
        state.velocity = [0.0, 12.0, 0.0];
        let animation = AvatarAnimationInput {
            held_tool_pose: held_tool,
            action: None,
            ..Default::default()
        };
        time += 1.0 / 60.0;
        sample(&mut mesh, &assets, &renderer, &gpu, &state, time, &animation)?;
    }

    // A held jet changes jump to the absolute fall clip, so crouch and held
    // hands do not trigger that layer-order error on the jetting path.
    state.crouched = true;
    state.jetting = true;
    state.velocity = [0.0, 3.0, 0.0];
    let jet_animation = AvatarAnimationInput {
        held_tool_pose: HeldToolPose::Both,
        action: None,
        ..Default::default()
    };
    time += 1.0 / 60.0;
    sample(&mut mesh, &assets, &renderer, &gpu, &state, time, &jet_animation)?;

    // Include the landing/idle transition after jump release.
    state.grounded = true;
    state.crouched = false;
    state.jetting = false;
    state.velocity = [0.0; 3];
    state.jump = Default::default();
    time += 1.0 / 60.0;
    sample(&mut mesh, &assets, &renderer, &gpu, &state, time, &no_tool)?;
    Ok(())
}
