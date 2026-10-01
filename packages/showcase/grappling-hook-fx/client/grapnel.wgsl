// Grappling Hook: a forged four-claw grapnel, blackened steel with its
// points ground bright, a red band on the shank and an eye for the cable.
//
// Drawn five times with the same tube grid (uv.x round, uv.y along): the
// shank, then each of the four claws. The vertex shader bends the tube
// into that part: the shank runs straight from the eye where the cable
// ties on to the crown; each claw leaves the crown, sweeps out wide and
// hooks back hard toward the cable, thinning to a barbed point.
//   0: the crown (the grapnel's leading tip) xyz, size (1 is about half a
//      unit long)
//   1: the direction it points (from the cable to the crown) xyz, part (0
//      the shank, 1 to 4 a claw)
//   2: direction the sunlight travels xyz times its strength, turn of the
//      claws round the shank (radians)
//   3: ambient rgb, how far the claws are open (0 folded, 1 open)

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Along the part (0..1), and the part.
    @location(2) along: vec2<f32>,
};

const TAU: f32 = 6.2831853;

fn bezier(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, s: f32) -> vec3<f32> {
    let r = 1.0 - s;
    return a * (r * r) + b * (2.0 * r * s) + c * (s * s);
}

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let p = bri_draw.params;
    let crown = p[0].xyz;
    let size = p[0].w;
    let d = normalize(p[1].xyz + vec3<f32>(0.0, 0.0, 0.0001));
    let part = p[1].w;
    let open = p[3].w;
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(d.y) > 0.95) {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let e1 = normalize(cross(d, up));
    let e2 = cross(d, e1);
    let s = v.uv.y;
    var centre: vec3<f32>;
    var tangent: vec3<f32>;
    var radius: f32;
    if (part < 0.5) {
        let back = crown - d * (0.6 * size);
        centre = mix(back, crown, s);
        tangent = d;
        // The eye at the back, a boss at the crown.
        radius = size * (0.045 + 0.03 * smoothstep(0.86, 1.0, s) + 0.035 * (1.0 - smoothstep(0.0, 0.1, s)));
    } else {
        let angle = p[2].w + (part - 1.0) * (TAU / 4.0);
        let out = e1 * cos(angle) + e2 * sin(angle);
        // Folded, the claws lie back along the shank; open, they spread.
        let wide = mix(0.08, 0.34, open);
        let a = crown - d * (0.03 * size);
        let b = crown + d * (0.06 * size) + out * (wide * size);
        let c = crown - d * (mix(0.4, 0.3, open) * size) + out * (mix(0.1, 0.28, open) * size);
        centre = bezier(a, b, c, s);
        tangent = normalize(bezier(a, b, c, min(s + 0.02, 1.0)) - bezier(a, b, c, max(s - 0.02, 0.0)));
        // Square-ish and heavy at the crown, a barbed point at the end.
        let barb = smoothstep(0.78, 0.86, s) * (1.0 - smoothstep(0.86, 0.9, s)) * 0.018;
        radius = size * (mix(0.05, 0.005, s * s) + barb);
    }
    var ref_up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(tangent.y) > 0.95) {
        ref_up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let a = normalize(cross(tangent, ref_up));
    let b = cross(tangent, a);
    let turn = v.uv.x * TAU;
    let n = a * cos(turn) + b * sin(turn);
    var out: Varyings;
    let world = centre + n * radius;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = n;
    out.along = vec2<f32>(s, part);
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let n = normalize(in.normal + vec3<f32>(0.0, 0.00001, 0.0));
    let view = normalize(bri_frame.camera.xyz - in.world);
    let sun = p[2].xyz;
    let strength = length(sun);
    let toward_sun = -sun / max(strength, 0.0001);
    // Blackened forged steel, ground bright toward the claws' points.
    let black = vec3<f32>(0.16, 0.17, 0.18);
    let bright = vec3<f32>(0.85, 0.87, 0.9);
    var base = black;
    var gloss = 0.5;
    if (in.along.y > 0.5) {
        let ground = smoothstep(0.55, 0.9, in.along.x);
        base = mix(black, bright, ground);
        gloss = mix(0.5, 1.0, ground);
    } else if (in.along.x > 0.3 && in.along.x < 0.45) {
        // A red band painted round the shank, chipped.
        base = vec3<f32>(0.62, 0.08, 0.05);
        gloss = 0.3;
    }
    let diffuse = max(dot(n, toward_sun), 0.0) * strength;
    let half_vec = normalize(toward_sun + view);
    let spec = pow(max(dot(n, half_vec), 0.0), 48.0) * gloss * strength;
    let rim = pow(1.0 - max(dot(n, view), 0.0), 3.0) * 0.3 * gloss;
    let lit = base * (p[3].rgb * 1.1 + vec3<f32>(1.0, 0.97, 0.9) * diffuse)
        + vec3<f32>(spec) * mix(vec3<f32>(1.0), base, 0.3) + p[3].rgb * rim;
    return vec4<f32>(lit, 1.0);
}
