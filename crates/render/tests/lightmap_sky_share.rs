//! A map's `lightmap_sky_share` reaches the camera uniform and survives the
//! live environment, so lightmaps can follow the live sun and ambient.
use bri_content::atmosphere::Live;
use bri_render::scene::{Camera, SceneData};

fn live(direct: f32) -> Live {
    Live {
        sun_direction: [0.0, -1.0, 0.0],
        direct_light: [direct; 3],
        ambient_light: [0.1; 3],
        shadow_color: None,
        fog_start: 0.0,
        fog_end: 0.0,
        fog_color: [0.0; 3],
        fog_height: None,
        fog_max: None,
        sky_tint: [1.0; 3],
        flare: ([0.0; 4], 1.0),
        vignette: None,
    }
}

fn camera_with(share: f32) -> Camera {
    let scene = SceneData {
        lightmap_sky_share: share,
        ..SceneData::default()
    };
    let mut camera = Camera::default();
    camera.apply_environment(&scene);
    camera.apply_atmosphere(&live(0.0));
    camera
}

#[test]
fn the_share_reaches_the_camera_and_a_relit_environment_keeps_it() {
    let camera = camera_with(0.75);
    assert_eq!(camera.baked_sun_direction[3], 1.0, "the dark sun relights");
    assert_eq!(camera.baked_ambient[3], 0.75);
    assert_eq!(camera_with(2.0).baked_ambient[3], 1.0, "clamped");
}

#[test]
fn a_map_that_does_not_say_keeps_its_lightmaps_as_baked() {
    assert_eq!(camera_with(0.0).baked_ambient[3], 0.0);
}
