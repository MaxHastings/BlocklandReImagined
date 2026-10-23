//! Exercise the same device smoke test used by native startup, without a window.
#[test]
fn uploaded_picture_survives_shaders_depth_raster_and_readback() -> anyhow::Result<()> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: std::env::var_os("BRI_TEST_SOFTWARE_GPU").is_some(),
        ..Default::default()
    }))?;
    eprintln!("Pixel probe adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        bri_render::device_probe::check_surface_view(&device, &queue, format)?;
    }
    Ok(())
}
