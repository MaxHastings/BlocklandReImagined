// Every pass that writes the frame works in display-encoded colour, the way
// the original art and palette were authored, and ends with output_color. A
// non-sRGB target stores display values as they are; an sRGB target expects
// linear ones and re-encodes them itself.
override OUTPUT_ENCODED: u32 = 0u;
// Colour-vision assistance (not in v20): 0 off, 1 protanopia,
// 2 deuteranopia, 3 tritanopia. Daltonization after Fidaner, Lin and Ozguven:
// the colour differences the player cannot see are shifted into ones they can.
override COLOR_VISION: u32 = 0u;
fn daltonize(c:vec3<f32>)->vec3<f32> {
    if COLOR_VISION == 0u { return c; }
    let l = dot(c, vec3<f32>(17.8824, 43.5161, 4.11935));
    let m = dot(c, vec3<f32>(3.45565, 27.1554, 3.86714));
    let s = dot(c, vec3<f32>(0.0299566, 0.184309, 1.46709));
    var sim = vec3<f32>(l, m, s);
    if COLOR_VISION == 1u {
        sim.x = 2.02344 * m - 2.52581 * s;
    } else if COLOR_VISION == 2u {
        sim.y = 0.494207 * l + 1.24827 * s;
    } else {
        sim.z = -0.395913 * l + 0.801109 * m;
    }
    let seen = vec3<f32>(
        dot(sim, vec3<f32>(0.0809444479, -0.130504409, 0.116721066)),
        dot(sim, vec3<f32>(-0.0102485335, 0.0540193266, -0.113614708)),
        dot(sim, vec3<f32>(-0.000365296938, -0.00412161469, 0.693511405)),
    );
    let err = c - seen;
    let shift = vec3<f32>(0.0, 0.7 * err.x + err.y, 0.7 * err.x + err.z);
    return clamp(c + shift, vec3<f32>(0.0), vec3<f32>(1.0));
}
fn output_color(display:vec3<f32>)->vec3<f32> {
    let seen = daltonize(display);
    if OUTPUT_ENCODED == 1u { return seen; }
    return linear_color(seen);
}
