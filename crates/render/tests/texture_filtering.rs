//! Use Sharp Filter changes only brickSIDE, as in v20: map, terrain and
//! model textures keep the player's smooth filtering.
use bri_render::scene::TextureFiltering;

#[test]
fn sharp_filter_changes_only_brick_sides() {
    let smooth = TextureFiltering::default();
    let sharp = TextureFiltering {
        sharp: true,
        ..smooth
    };
    let [tiled, clamped, side] = smooth.diffuse_samplers();
    let [sharp_tiled, sharp_clamped, sharp_side] = sharp.diffuse_samplers();
    assert_eq!(sharp_tiled, tiled);
    assert_eq!(sharp_clamped, clamped);
    assert_eq!(tiled.mag_filter, wgpu::FilterMode::Linear);
    assert_eq!(tiled.min_filter, wgpu::FilterMode::Linear);
    assert_eq!(tiled.anisotropy_clamp, 8);
    assert_eq!(side, clamped);
    assert_eq!(sharp_side.min_filter, wgpu::FilterMode::Nearest);
    assert_eq!(sharp_side.mag_filter, wgpu::FilterMode::Nearest);
    assert_eq!(sharp_side.lod_max_clamp, 0.0);
    assert_eq!(sharp_side.anisotropy_clamp, 1);
}
