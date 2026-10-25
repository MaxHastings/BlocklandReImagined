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
// The display ramp of the games that set one through the video driver's gamma
// table (Unreal Engine 2's Brightness, Contrast and Gamma): per channel,
// clamp(x * (C + 0.5) + (B - C) / 2, 0, 1) with x = c ^ (1 / (G - 0.2)).
// That is the table D3DDrv's UpdateGamma builds, 16-bit steps aside. The
// neutral values (B 0.5, C 0.5, G 1.2) are the identity.
override RAMP_BRIGHTNESS: f32 = 0.5;
override RAMP_CONTRAST: f32 = 0.5;
override RAMP_GAMMA: f32 = 1.2;
fn display_ramp(c:vec3<f32>)->vec3<f32> {
    // The identity is returned untouched, not through pow.
    if RAMP_BRIGHTNESS == 0.5 && RAMP_CONTRAST == 0.5 && RAMP_GAMMA == 1.2 { return c; }
    let x = pow(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / (RAMP_GAMMA - 0.2)));
    return clamp(x * (RAMP_CONTRAST + 0.5) + vec3<f32>((RAMP_BRIGHTNESS - RAMP_CONTRAST) * 0.5), vec3<f32>(0.0), vec3<f32>(1.0));
}
fn output_color(display:vec3<f32>)->vec3<f32> {
    let seen = display_ramp(daltonize(display));
    if OUTPUT_ENCODED == 1u { return seen; }
    return linear_color(seen);
}
