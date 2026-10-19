//! How long the world's pipelines take to build on this machine's adapter (a number for the log, not a pass/fail check).
//! `cargo test -p bri-render --test pipeline_time -- --ignored --nocapture`; `WGPU_BACKEND` and `WGPU_DX12_SHADER_COMPILER` choose.
use anyhow::Result;
use bri_render::scene::SceneRenderer;
use std::time::Instant;

#[test]
#[ignore = "timing"]
fn world_pipelines_build_time() -> Result<()> {
    let (device, info) = pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await?;
        let info = adapter.get_info();
        anyhow::Ok((adapter.request_device(&Default::default()).await?.0, info))
    })?;
    println!("adapter: {} ({:?})", info.name, info.backend);
    let started = Instant::now();
    let mut renderer = SceneRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
    println!("classic world pipelines: {:?}", started.elapsed());
    let started = Instant::now();
    renderer.prepare_lighting_pipelines(true, true);
    println!("modern lighting pipelines: {:?}", started.elapsed());
    Ok(())
}
