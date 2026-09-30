// Grapple Rope: the three-pronged hook, cast brass gone dark with use,
// its prongs worn bright at the points, its shank lashed with cord.
//
// Drawn four times with the same tube grid (uv.x round, uv.y along): the
// shank, then each of the three prongs. The vertex shader bends the tube
// into that part: the shank runs straight from where the rope ties on to
// the crown; each prong leaves the crown, sweeps out and curls back
// toward the rope, thinning to a point.
//   0: the crown (the hook's leading tip) xyz, size (1 is about half a
//      unit long)
//   1: the direction the hook points (from the rope to the crown) xyz,
//      part (0 the shank, 1 to 3 a prong)
//   2: direction the sunlight travels xyz times its strength, turn of the
//      prongs round the shank (radians)
//   3: ambient rgb, unused

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
        // The shank: from the tie (a little behind) to the crown.
        let back = crown - d * (0.55 * size);
        centre = mix(back, crown, s);
        tangent = d;
        // A knob at the crown, the eye at the tie.
        radius = size * (0.05 + 0.025 * smoothstep(0.85, 1.0, s) + 0.02 * (1.0 - smoothstep(0.0, 0.12, s)));
    } else {
        let angle = p[2].w + (part - 1.0) * (TAU / 3.0);
        let out = e1 * cos(angle) + e2 * sin(angle);
        let a = crown - d * (0.04 * size);
        let b = crown + d * (0.1 * size) + out * (0.28 * size);
        let c = crown - d * (0.24 * size) + out * (0.3 * size);
        centre = bezier(a, b, c, s);
        tangent = normalize(bezier(a, b, c, min(s + 0.02, 1.0)) - bezier(a, b, c, max(s - 0.02, 0.0)));
        // Thick where it leaves the crown, a point at the barb.
        radius = size * mix(0.045, 0.006, s * s);
    }
    var ref_up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(tangent.y) > 0.95) {
        ref_up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let a = normalize(cross(tangent, ref_up));
    let b = cross(tangent, a);
    let turn = v.uv.x * TAU;
    let n = a * cos(turn) + b * sin(turn);
    let world = centre + n * radius;
    var out: Varyings;
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
    // Dark aged brass, worn bright toward the prongs' points.
    let aged = vec3<f32>(0.42, 0.3, 0.12);
    let worn = vec3<f32>(0.95, 0.74, 0.36);
    var base = aged;
    var gloss = 0.7;
    if (in.along.y > 0.5) {
        base = mix(aged, worn, smoothstep(0.35, 0.95, in.along.x));
    } else if (in.along.x < 0.42) {
        // Cord lashed round the shank above the tie: tight dark turns.
        let turns = 0.5 + 0.5 * sin(in.along.x * 110.0);
        base = mix(vec3<f32>(0.22, 0.16, 0.08), vec3<f32>(0.5, 0.38, 0.2), turns);
        gloss = 0.1;
    }
    let diffuse = max(dot(n, toward_sun), 0.0) * strength;
    let half_vec = normalize(toward_sun + view);
    let spec = pow(max(dot(n, half_vec), 0.0), 40.0) * gloss * strength;
    // Metal takes its highlight in its own colour; a rim of sky light.
    let rim = pow(1.0 - max(dot(n, view), 0.0), 3.0) * 0.25 * gloss;
    let lit = base * (p[3].rgb * 1.1 + vec3<f32>(1.0, 0.95, 0.85) * diffuse)
        + base * spec * 1.4 + p[3].rgb * rim;
    return vec4<f32>(lit, 1.0);
}
