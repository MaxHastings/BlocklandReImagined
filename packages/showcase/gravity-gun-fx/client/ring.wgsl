// Gravity Gun shockwave: the ring of force a throw or punt leaves behind,
// racing outward across the throw's direction and fading. The grid mesh
// becomes a flat band: uv.x round the ring, uv.y across its width.
//   0: centre xyz, final radius
//   1: facing xyz (the throw's direction), age 0..1
//   2: unused
//   3: colour rgb, alpha

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) across: f32,
    @location(1) fade: f32,
};

const TAU: f32 = 6.2831853;

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let p = bri_draw.params;
    let age = clamp(p[1].w, 0.0, 1.0);
    let facing = normalize(p[1].xyz + vec3<f32>(0.0, 0.0001, 0.0));
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(facing.y) > 0.95) {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let a = normalize(cross(facing, up));
    let b = cross(facing, a);
    // Fast out, easing off.
    let grow = 1.0 - (1.0 - age) * (1.0 - age) * (1.0 - age);
    let radius = p[0].w * (0.15 + 0.85 * grow);
    let width = radius * 0.22 * (1.0 - age * 0.6);
    let r = radius + (v.uv.y - 0.5) * width;
    let angle = v.uv.x * TAU;
    // It travels a little along the throw as it spreads.
    let world = p[0].xyz + facing * grow * p[0].w * 0.4 + (a * cos(angle) + b * sin(angle)) * r;
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    out.across = v.uv.y;
    out.fade = (1.0 - age) * (1.0 - age);
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let band = 1.0 - abs(in.across - 0.5) * 2.0;
    let edge = pow(band, 1.8);
    let colour = mix(p[3].rgb, vec3<f32>(1.0, 0.95, 0.85), pow(band, 6.0));
    return vec4<f32>(colour, clamp(edge * in.fade * p[3].a, 0.0, 1.0));
}
