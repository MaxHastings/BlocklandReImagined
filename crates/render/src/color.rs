//! The one colour convention every pass that writes the frame shares.
//!
//! Textures are sampled as sRGB (decoded to linear), converted back to
//! display encoding with `display_color`, tinted and lit in display space,
//! and written through `output_color`, which adapts to the target format.

/// `display_color`, `linear_color` and `output_color`, to prepend to a
/// shader's own source.
pub const WGSL: &str = concat!(
    include_str!("color.wgsl"),
    "\n",
    include_str!("output.wgsl"),
    "\n"
);

/// Prepend the shared colour functions to a shader's own source.
pub fn shader_source(own: &str) -> String {
    format!("{WGSL}{own}")
}

/// The colour-vision assistance renderers are built with (`COLOR_VISION`):
/// 0 off, 1 protanopia, 2 deuteranopia, 3 tritanopia.
static COLOR_VISION: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Choose colour-vision assistance for pipelines built from now on.
pub fn set_color_vision(mode: u32) {
    COLOR_VISION.store(mode.min(3), std::sync::atomic::Ordering::Relaxed);
}
pub fn color_vision() -> u32 {
    COLOR_VISION.load(std::sync::atomic::Ordering::Relaxed)
}

/// Pipeline constants telling `output_color` how `format` stores colour
/// and which colour-vision assistance to apply.
pub fn output_constants(format: wgpu::TextureFormat) -> [(&'static str, f64); 2] {
    [
        ("OUTPUT_ENCODED", if format.is_srgb() { 0.0 } else { 1.0 }),
        ("COLOR_VISION", f64::from(color_vision())),
    ]
}
