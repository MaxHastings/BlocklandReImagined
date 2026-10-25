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

/// Prepend the shared colour and fog functions to a shader's own source.
pub fn shader_source(own: &str) -> String {
    use bri_content::environment::{FOG_DEPTH, FOG_HEIGHT};
    let fog = include_str!("fog.wgsl");
    format!(
        "{WGSL}const FOG_DEPTH:f32={FOG_DEPTH:?};\nconst FOG_HEIGHT:f32={FOG_HEIGHT:?};\n{fog}\n{own}"
    )
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

/// The display ramp renderers are built with: Brightness, Contrast and
/// Gamma as `f32` bits. The defaults are the identity ramp.
pub const RAMP_NEUTRAL: [f32; 3] = [0.5, 0.5, 1.2];
static RAMP: [std::sync::atomic::AtomicU32; 3] = [
    std::sync::atomic::AtomicU32::new(RAMP_NEUTRAL[0].to_bits()),
    std::sync::atomic::AtomicU32::new(RAMP_NEUTRAL[1].to_bits()),
    std::sync::atomic::AtomicU32::new(RAMP_NEUTRAL[2].to_bits()),
];
/// Choose the display ramp (Brightness, Contrast, Gamma) for pipelines built
/// from now on. Non-finite values leave the identity in place.
pub fn set_display_ramp(ramp: [f32; 3]) {
    for (slot, (value, neutral)) in RAMP.iter().zip(ramp.into_iter().zip(RAMP_NEUTRAL)) {
        let value = if value.is_finite() { value } else { neutral };
        slot.store(value.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
}
pub fn display_ramp() -> [f32; 3] {
    std::array::from_fn(|i| f32::from_bits(RAMP[i].load(std::sync::atomic::Ordering::Relaxed)))
}

/// Pipeline constants telling `output_color` how `format` stores colour,
/// which colour-vision assistance to apply and the display ramp.
pub fn output_constants(format: wgpu::TextureFormat) -> [(&'static str, f64); 5] {
    let [brightness, contrast, gamma] = display_ramp();
    [
        ("OUTPUT_ENCODED", if format.is_srgb() { 0.0 } else { 1.0 }),
        ("COLOR_VISION", f64::from(color_vision())),
        ("RAMP_BRIGHTNESS", f64::from(brightness)),
        ("RAMP_CONTRAST", f64::from(contrast)),
        ("RAMP_GAMMA", f64::from(gamma)),
    ]
}
