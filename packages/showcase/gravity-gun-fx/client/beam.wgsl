// Gravity Gun beam: a ribbon of energy from the gun's muzzle to what it
// holds, bending like a whip when the held thing lags behind your aim.
//
// The mesh is a flat grid: uv.x runs across the beam, uv.y along it. The
// vertex shader lays it along a curve that leaves the muzzle the way you
// aim and swings round into the grip point, lets it ripple a little, and
// turns it to face the camera, so it reads as a solid glowing beam from
// any side. Drawn twice with additive blending: a wide soft glow and a
// thin white-hot core.
//   0: muzzle xyz, width
//   1: grip xyz, core (1) or glow (0)
//   2: bend xyz (the curve's pull, on your aim), seed
//   3: colour rgb, brightness

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// The quadratic curve from the muzzle through the bend's pull to the grip.
fn curve(start: vec3<f32>, bend: vec3<f32>, end: vec3<f32>, s: f32) -> vec3<f32> {
    let r = 1.0 - s;
    return start * (r * r) + bend * (2.0 * r * s) + end * (s * s);
}

// A point on the beam: the curve, with a small ripple running along it.
fn path(start: vec3<f32>, bend: vec3<f32>, end: vec3<f32>, a: vec3<f32>, b: vec3<f32>, s: f32, t: f32, seed: f32) -> vec3<f32> {
    let reach = length(end - start);
    let middle = sin(s * 3.14159);
    let ripple = middle * min(reach * 0.012, 0.12);
    let wave = vec2<f32>(
        sin(s * 11.0 - t * 14.0 + seed),
        cos(s * 9.0 - t * 12.0 + seed * 1.7)
    ) * ripple;
    return curve(start, bend, end, s) + a * wave.x + b * wave.y;
}

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let p = bri_draw.params;
    let start = p[0].xyz;
    let end = p[1].xyz;
    let bend = p[2].xyz;
    let axis = normalize(end - start + vec3<f32>(0.0, 0.0, 0.0001));
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(axis.y) > 0.95) {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let a = normalize(cross(axis, up));
    let b = cross(axis, a);
    let t = bri_frame.time.x;
    let seed = p[2].w;
    let s = v.uv.y;
    let centre = path(start, bend, end, a, b, s, t, seed);
    let ahead = path(start, bend, end, a, b, min(s + 0.02, 1.0), t, seed)
        - path(start, bend, end, a, b, max(s - 0.02, 0.0), t, seed);
    let toward = normalize(bri_frame.camera.xyz - centre);
    let side = normalize(cross(ahead, toward) + a * 0.0001);
    // Thin at the muzzle, fuller where it takes hold, throbbing as the
    // energy flows.
    let pinch = mix(0.35, 1.0, smoothstep(0.0, 0.25, s)) * (0.85 + 0.3 * smoothstep(0.7, 1.0, s));
    let pulse = 1.0 + 0.12 * sin(s * 30.0 - t * 22.0 + seed);
    let width = p[0].w * pinch * pulse;
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(centre + side * (v.uv.x * 2.0 - 1.0) * width, 1.0);
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let t = bri_frame.time.x;
    let core = p[1].w;
    let across = in.uv.x * 2.0 - 1.0;
    let body = exp(-across * across * mix(3.0, 10.0, core));
    // Pulses of energy running out from the gun, over a steady glow.
    let run = fract(in.uv.y * 3.0 - t * 2.6 + p[2].w * 0.1);
    let pulses = 0.65 + 0.35 * smoothstep(0.0, 0.2, run) * smoothstep(1.0, 0.45, run);
    let shimmer = 1.0 - mix(0.05, 0.18, core) * (0.5 + 0.5 * sin(in.uv.y * 140.0 - t * 50.0 + across * 4.0));
    let ends = smoothstep(0.0, 0.03, in.uv.y) * smoothstep(1.0, 0.94, in.uv.y);
    let hot = mix(p[3].rgb, vec3<f32>(0.9, 1.0, 1.0), core * 0.8);
    let alpha = body * pulses * shimmer * ends * p[3].a;
    return vec4<f32>(hot, clamp(alpha, 0.0, 1.0));
}
