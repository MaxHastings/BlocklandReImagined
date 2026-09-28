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

/// Pipeline constants telling `output_color` how `format` stores colour.
pub fn output_constants(format: wgpu::TextureFormat) -> [(&'static str, f64); 1] {
    [("OUTPUT_ENCODED", if format.is_srgb() { 0.0 } else { 1.0 })]
}
