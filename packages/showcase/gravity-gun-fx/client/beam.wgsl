// Gravity Gun beam: a crackling ribbon of energy from the gun to what it
// holds.
//
// The mesh is a flat grid: uv.x runs across the beam, uv.y along it. The
// vertex shader bends it between the two ends, lets it sag and writhe in
// the middle, pinches it at the gun, and turns it to face the camera, so
// it reads as a solid glowing beam from any side. Drawn twice with
// additive blending: a wide soft glow and a thin white-hot core.
//   0: start xyz, width
//   1: end xyz, charge 0..1
//   2: seed, brightness, unused, core (1) or glow (0)
//   3: colour rgb, alpha

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Where along the beam's bent path `along` (0 at the gun, 1 at the
// target) lies.
fn path(start: vec3<f32>, span: vec3<f32>, a: vec3<f32>, b: vec3<f32>, along: f32, t: f32, seed: f32) -> vec3<f32> {
    let reach = length(span);
    let middle = sin(along * 3.14159);
    let sway = middle * reach * 0.02;
    let wobble = vec2<f32>(
        sin(along * 7.0 - t * 9.0 + seed) + 0.5 * sin(along * 19.0 + t * 13.0),
        cos(along * 6.0 + t * 8.0 + seed * 1.7) + 0.5 * cos(along * 23.0 - t * 11.0)
    ) * sway;
    let sag = vec3<f32>(0.0, -middle * reach * 0.04, 0.0);
    return start + span * along + a * wobble.x + b * wobble.y + sag;
}

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let p = bri_draw.params;
    let start = p[0].xyz;
    let span = p[1].xyz - start;
    let axis = normalize(span + vec3<f32>(0.0, 0.0, 0.0001));
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(axis.y) > 0.95) {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let a = normalize(cross(axis, up));
    let b = cross(axis, a);
    let t = bri_frame.time.x;
    let seed = p[2].x;
    let along = v.uv.y;
    let centre = path(start, span, a, b, along, t, seed);
    let ahead = path(start, span, a, b, min(along + 0.02, 1.0), t, seed)
        - path(start, span, a, b, max(along - 0.02, 0.0), t, seed);
    let toward = normalize(bri_frame.camera.xyz - centre);
    let side = normalize(cross(ahead, toward) + a * 0.0001);
    // Pinched at the gun, swelling a little where it takes hold, and
    // throbbing as the energy flows.
    let pinch = smoothstep(0.0, 0.1, along) * (0.75 + 0.35 * smoothstep(0.6, 1.0, along));
    let pulse = 1.0 + 0.1 * sin(along * 40.0 - t * 28.0 + seed);
    let width = p[0].w * pinch * pulse * (1.0 + 0.5 * p[1].w);
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(centre + side * (v.uv.x * 2.0 - 1.0) * width, 1.0);
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let t = bri_frame.time.x;
    let core = p[2].w;
    let across = in.uv.x * 2.0 - 1.0;
    // A soft glow falling off from the middle; the core is sharper.
    let body = exp(-across * across * mix(3.5, 9.0, core));
    // Energy streaming from the gun to the target: long bright streaks
    // sliding along a steady glow, and a fine crackle.
    let streak = sin(in.uv.y * 9.0 - t * 17.0 + p[2].x + across * 1.5);
    let flow = 0.7 + 0.3 * streak * streak;
    let crackle = 1.0 - mix(0.04, 0.2, core) * (0.5 + 0.5 * sin(in.uv.y * 173.0 - t * 61.0 + across * 5.0));
    let ends = smoothstep(0.0, 0.04, in.uv.y) * smoothstep(1.0, 0.92, in.uv.y);
    let hot = mix(p[3].rgb, vec3<f32>(1.0, 0.95, 0.8), core);
    let alpha = body * flow * crackle * ends * p[2].y * p[3].a;
    return vec4<f32>(hot, clamp(alpha, 0.0, 1.0));
}
