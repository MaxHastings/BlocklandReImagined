// Every pass that writes the frame works in display-encoded colour, the way
// the original art and palette were authored, and ends with output_color. A
// non-sRGB target stores display values as they are; an sRGB target expects
// linear ones and re-encodes them itself.
override OUTPUT_ENCODED: u32 = 0u;
fn output_color(display:vec3<f32>)->vec3<f32> {
    if OUTPUT_ENCODED == 1u { return display; }
    return linear_color(display);
}
