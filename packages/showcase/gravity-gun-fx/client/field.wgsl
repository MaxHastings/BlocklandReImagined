// Gravity Gun field: the shimmering force bubble round whatever the gun
// holds. The grid mesh wraps into a sphere whose surface ripples; it is
// clear face-on and glows at the rim, with bands of energy sliding over it
// a faint glow filling it, two rings of light turning round it, and a
// bright flicker where the beam grips.
//   0: centre xyz, radius
//   1: where the beam arrives xyz, charge 0..1
//   2: seed, brightness, unused, unused
//   3: colour rgb, alpha

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

const PI: f32 = 3.14159265;

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let p = bri_draw.params;
    let t = bri_frame.time.x;
    let theta = v.uv.y * PI;
    let phi = v.uv.x * 2.0 * PI;
    let n = vec3<f32>(sin(theta) * cos(phi), cos(theta), sin(theta) * sin(phi));
    let ripple = 1.0
        + 0.025 * sin(n.x * 7.0 + t * 5.0 + p[2].x)
        + 0.02 * sin(n.y * 9.0 - t * 6.5)
        + 0.015 * sin(n.z * 11.0 + t * 7.5)
        + 0.06 * p[1].w;
    let world = p[0].xyz + n * p[0].w * ripple;
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    out.normal = n;
    out.world = world;
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let t = bri_frame.time.x;
    let n = normalize(in.normal);
    let view = normalize(bri_frame.camera.xyz - in.world);
    // Clear face-on, glowing only toward the rim: a heat shimmer more
    // than a shell.
    let rim = pow(1.0 - abs(dot(n, view)), 3.0);
    let bands = 0.5 + 0.5 * sin(in.uv.y * 46.0 + t * 7.0 + sin(in.uv.x * 12.566 + t * 2.0) * 1.5);
    let grip_dir = normalize(p[1].xyz - p[0].xyz + vec3<f32>(0.0, 0.0001, 0.0));
    let grip = pow(max(dot(n, grip_dir), 0.0), 24.0) * (0.75 + 0.25 * sin(t * 40.0));
    // Two rings sweeping round it on tilted axes, so it reads as a cage of
    // force even face-on.
    let r1 = abs(dot(n, normalize(vec3<f32>(sin(t * 0.9), 0.35, cos(t * 0.9)))));
    let r2 = abs(dot(n, normalize(vec3<f32>(cos(t * 0.7 + 1.3), 0.8, sin(t * 0.7 + 1.3)))));
    let rings = (smoothstep(0.06, 0.0, r1) + smoothstep(0.05, 0.0, r2)) * 0.35;
    let fill = 0.05;
    let alpha = (rim * (0.35 + 0.65 * bands) * 0.7 + grip * 0.45 + rings + fill) * p[2].y * p[3].a;
    let colour = mix(p[3].rgb, vec3<f32>(1.0, 0.92, 0.75), grip);
    return vec4<f32>(colour, clamp(alpha, 0.0, 1.0));
}
