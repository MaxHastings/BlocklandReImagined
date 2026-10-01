//! The scene shader parses and validates without a GPU, so binding and
//! sampler changes are caught on machines with no graphics adapter.
use wgpu::naga;

#[test]
fn scene_shader_validates() {
    validate(include_str!("../src/scene.wgsl"));
}

#[test]
fn vignette_shader_validates() {
    validate(include_str!("../src/vignette.wgsl"));
}

fn validate(own: &str) {
    let src = bri_render::color::shader_source(own);
    let module =
        naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{e:?}"));
}
