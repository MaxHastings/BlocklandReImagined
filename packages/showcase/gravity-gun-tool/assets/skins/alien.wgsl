// Gravity Gun skin: the Printer the gun is built from, turned into
// something not made by people. A dark oily shell with a colour-shifting
// sheen, and glowing veins whose light runs toward the muzzle: a slow
// pulse at rest, a bright flare while the beam is on.
//
// The gun's look (looks.json): the game draws it over every copy of the
// Printer the gun is built from, in a hand, dropped, on a spawn brick and
// in a mirror, puffed out a hair along its normals so it covers it exactly.
//   0: vein colour rgb, energy (0 at rest, 1 while the trigger grabs)
//   1: direction the sunlight travels xyz, seed
//   2: sun colour rgb, unused
//   3: ambient rgb, unused

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) local: vec3<f32>,
};

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let m = bri_draw.model;
    let local = v.position + v.normal * 0.012;
    let world = (m * vec4<f32>(local, 1.0)).xyz;
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = (m * vec4<f32>(v.normal, 0.0)).xyz;
    out.local = v.position;
    return out;
}

// Where the veins run: the zero lines of a few warped waves.
fn veins(q: vec3<f32>) -> f32 {
    let w = sin(q.x * 7.0 + 2.0 * sin(q.y * 5.0 + q.z * 3.0))
        + sin(q.y * 6.0 + 2.0 * sin(q.z * 4.0 + q.x * 5.0))
        + 0.7 * sin(q.z * 8.0 + 1.5 * sin(q.x * 4.0 + q.y * 2.0));
    return 1.0 - smoothstep(0.0, 0.16, abs(w));
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let t = bri_frame.time.x;
    let n = normalize(in.normal + vec3<f32>(0.0, 0.00001, 0.0));
    let view = normalize(bri_frame.camera.xyz - in.world);
    let sun = normalize(p[1].xyz + vec3<f32>(0.0, -0.00001, 0.0));
    // The shell: nearly black, lit only a little.
    let shell = vec3<f32>(0.035, 0.025, 0.05);
    let lit = shell * (p[3].rgb * 1.2 + p[2].rgb * max(dot(n, -sun), 0.0));
    // An oil-slick sheen toward the edges, its colours slowly drifting.
    let edge = 1.0 - abs(dot(n, view));
    // Thin-film colours, kept to the cold end: teal through violet.
    let hue = edge * 1.3 + dot(in.local, vec3<f32>(0.6, 0.9, 0.4)) + t * 0.04;
    let film = 0.5 + 0.5 * cos(6.2831853 * (vec3<f32>(hue) + vec3<f32>(0.0, 0.33, 0.67)));
    let cold = mix(vec3<f32>(0.25, 0.1, 0.55), vec3<f32>(0.1, 0.75, 0.8), film.g) * (0.6 + 0.4 * film.b);
    let sheen = cold * pow(edge, 3.0) * 0.4;
    // A hard highlight: it is wet-looking, not matte.
    let spec = pow(max(dot(reflect(sun, n), view), 0.0), 48.0) * p[2].rgb * 0.8;
    // The veins, their light running along the gun (+y) toward the muzzle.
    let q = in.local * 3.2;
    let flow = 0.5 + 0.5 * sin(in.local.y * 9.0 - t * mix(2.5, 9.0, p[0].w) + p[1].w);
    let energy = mix(0.35, 1.6, p[0].w);
    let glow = p[0].rgb * veins(q) * energy * (0.45 + 0.55 * flow);
    return vec4<f32>(lit + sheen + spec + glow, 1.0);
}
