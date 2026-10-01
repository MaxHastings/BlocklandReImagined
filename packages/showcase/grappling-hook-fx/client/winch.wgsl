// Grappling Hook skin: the Printer the launcher is built from, rebuilt as
// an expedition winch gun: riveted gunmetal plates with brushed grain
// along the barrel, brass drum bands wound with steel cable, and a small
// amber gauge that glows while the grapnel is out.
//
// Drawn over the game's own Printer with its own model (`image_mesh`),
// puffed out a hair along its normals so it covers it exactly. Torque
// models point along +y, so the bands and coils go round y.
//   0: the gauge (1 while the grapnel is out), time, unused, seed
//   1: direction the sunlight travels xyz, unused
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

fn hash3(q: vec3<f32>) -> f32 {
    return fract(sin(dot(q, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let q = in.local;
    let n = normalize(in.normal + vec3<f32>(0.0, 0.00001, 0.0));
    let view = normalize(bri_frame.camera.xyz - in.world);
    let sun = normalize(p[1].xyz + vec3<f32>(0.0, -0.00001, 0.0));

    // Gunmetal plates: brushed streaks along the barrel, panel seams.
    let streak = hash3(vec3<f32>(floor(q.x * 90.0), floor(q.z * 90.0), 1.0));
    var colour = mix(vec3<f32>(0.2, 0.22, 0.24), vec3<f32>(0.3, 0.32, 0.34), streak);
    var gloss = 0.55;
    let panel = fract(q.y * 1.6 + 0.15);
    let seam = smoothstep(0.03, 0.0, abs(panel - 0.5));
    colour = colour * (1.0 - 0.6 * seam);
    // Rivets in rows each side of the seams.
    let around = atan2(q.z, q.x) / 6.2831853;
    let rivet_cell = vec2<f32>(fract(around * 8.0), fract(q.y * 1.6 + 0.15));
    let rivet = smoothstep(0.1, 0.05, length(vec2<f32>(rivet_cell.x - 0.5, (abs(rivet_cell.y - 0.5) - 0.08) * 3.0)));
    colour = mix(colour, vec3<f32>(0.45, 0.46, 0.47), rivet);
    gloss = mix(gloss, 0.9, rivet);

    // Brass drum bands, each wound with steel cable.
    let band_at = fract(q.y * 1.6 + 0.7);
    let band = smoothstep(0.0, 0.02, band_at) * smoothstep(0.24, 0.22, band_at);
    let coil = 0.5 + 0.5 * sin((band_at * 40.0 + around * 1.0) * 6.28318);
    let brass = mix(vec3<f32>(0.55, 0.4, 0.14), vec3<f32>(0.85, 0.66, 0.3), hash3(floor(q * 50.0)) * 0.3 + 0.5);
    let wound = mix(brass, mix(vec3<f32>(0.5, 0.52, 0.55), vec3<f32>(0.8, 0.82, 0.84), coil),
        smoothstep(0.04, 0.06, band_at) * smoothstep(0.2, 0.18, band_at));
    colour = mix(colour, wound, band);
    gloss = mix(gloss, 0.85, band);

    // The gauge: an amber dot on top, glowing while the grapnel is out.
    let gauge_cell = vec2<f32>(q.x * 8.0, fract(q.y * 0.8) - 0.5);
    let gauge = smoothstep(0.35, 0.25, length(gauge_cell)) * step(0.3, n.y);
    let glow = p[0].x * (0.7 + 0.3 * sin(p[0].y * 9.0));
    colour = mix(colour, vec3<f32>(0.9, 0.55, 0.1) * (0.4 + glow), gauge);

    let lit = colour * (p[3].rgb * 1.15 + p[2].rgb * max(dot(n, -sun), 0.0));
    let spec = pow(max(dot(reflect(sun, n), view), 0.0), 36.0) * gloss * p[2].rgb * 0.7;
    let emit = vec3<f32>(1.0, 0.6, 0.15) * gauge * glow * 0.8;
    return vec4<f32>(lit + spec + emit, 1.0);
}
