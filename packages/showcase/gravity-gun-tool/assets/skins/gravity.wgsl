// Gravity Gun skin: the energy in the gun's own model
// (models/gravity-gun.shape.json, made by tools/make_gravity_gun_model.py).
// The model is whole without it: a dark shell cut with glowing seams, a
// glowing core and a yellow grip. This makes the light alive. Energy runs
// along the seams toward the claw, the core breathes, and while the beam
// is on (the image in its Grab state) everything flares and runs faster.
//
// Only the glowing parts are drawn: the shell, the grip and the socket
// keep the game's own lighting, as the model draws them. The model says
// which part a face is in its texture coordinates: u is the material's
// slot (slot k at (k + 0.5) / 8) and v how far along the gun it is (0 at
// the back, 1 at the claw's tips).
//   0: glow colour rgb, energy (0 at rest, 1 while the trigger grabs)
//   1: direction the sunlight travels xyz, seed
//   2: sun colour rgb, unused
//   3: ambient rgb, unused

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) along: f32,
    @location(1) @interpolate(flat) slot: u32,
    @location(2) world: vec3<f32>,
    @location(3) normal: vec3<f32>,
};

const SEAM: u32 = 1u;
const CORE: u32 = 2u;
const ACCENT: u32 = 4u;
const HOT: u32 = 6u;

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let m = bri_draw.model;
    let slot = u32(clamp(floor(v.uv.x * 8.0), 0.0, 7.0));
    var out: Varyings;
    out.slot = slot;
    out.along = v.uv.y;
    // A hair proud of the model, so it draws over its own faces.
    let world = (m * vec4<f32>(v.position + v.normal * 0.004, 1.0)).xyz;
    out.world = world;
    out.normal = (m * vec4<f32>(v.normal, 0.0)).xyz;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    // Faces that do not glow are left to the model: every corner of such
    // a triangle goes to one point, so it covers nothing.
    if (slot != SEAM && slot != CORE && slot != ACCENT && slot != HOT) {
        out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let t = bri_frame.time.x + p[1].w * 7.0;
    let energy = p[0].w;
    let glow = p[0].rgb;
    // Pulses running toward the claw: slow at rest, racing while it grabs.
    let speed = mix(1.2, 5.5, energy);
    let wave = fract(in.along * mix(3.0, 5.0, energy) - t * speed * 0.35);
    let pulse = smoothstep(0.0, 0.12, wave) * smoothstep(0.55, 0.12, wave);
    let level = mix(0.72, 1.0, energy);
    var colour = vec3<f32>(0.0);
    if (in.slot == SEAM) {
        colour = glow * (level + mix(0.25, 0.55, energy) * pulse);
    } else if (in.slot == CORE) {
        // The core breathes, and burns whiter while it holds something.
        let breath = 0.5 + 0.5 * sin(t * mix(2.2, 9.0, energy));
        let hot = mix(0.15, 0.55, energy) * breath;
        colour = mix(glow * 1.15, vec3<f32>(0.9, 1.0, 1.0), hot);
    } else if (in.slot == ACCENT) {
        let blink = 0.75 + 0.25 * sin(t * 1.7 + in.along * 9.0);
        colour = vec3<f32>(0.51, 0.06, 1.0) * mix(blink, 1.2, energy);
    } else {
        colour = vec3<f32>(0.9, 1.0, 1.0) * mix(0.9, 1.15, energy);
    }
    // Faces turned away a little brighter at the edge, as light in glass.
    let view = normalize(bri_frame.camera.xyz - in.world);
    let n = normalize(in.normal + vec3<f32>(0.0, 0.00001, 0.0));
    let edge = 1.0 - abs(dot(n, view));
    colour = colour * (1.0 + 0.2 * edge * edge);
    return vec4<f32>(min(colour, vec3<f32>(1.0)), 1.0);
}
