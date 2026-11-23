//! The scene shader parses and validates without a GPU, so binding and
//! sampler changes are caught on machines with no graphics adapter.
use wgpu::naga;

#[test]
fn scene_shader_validates() {
    for source in [
        include_str!("../src/scene_original.wgsl"),
        include_str!("../src/scene.wgsl"),
    ] {
        validate_source(&bri_render::scene::scene_shader_source(source));
    }
}

#[test]
fn vignette_shader_validates() {
    validate(include_str!("../src/vignette.wgsl"));
}

#[test]
fn ambient_occlusion_shader_validates() {
    let own = include_str!("../src/ambient_occlusion.wgsl");
    for texture in ["texture_depth_2d", "texture_depth_multisampled_2d"] {
        validate(&own.replace("DEPTH_TEXTURE", texture).replace(
            "MASK_TEXTURE",
            if texture.contains("multisampled") {
                "texture_multisampled_2d<f32>"
            } else {
                "texture_2d<f32>"
            },
        ));
    }
}

fn validate(own: &str) {
    validate_source(&bri_render::color::shader_source(own));
}

fn validate_source(src: &str) {
    let module =
        naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{e:?}"));
}
