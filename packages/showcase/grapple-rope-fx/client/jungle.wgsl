// Grapple Rope skin: the Printer the launcher is built from, carved out
// of dark jungle hardwood. Its grain runs along the barrel, it is bound
// with bamboo bands, a vine winds round it with leaves on the vine, moss
// grows on whatever faces up, and brass pins hold it together.
//
// Drawn over the game's own Printer with its own model (`image_mesh`),
// puffed out a hair along its normals so it covers it exactly. Torque
// models point along +y, so the bands and the vine go round y.
//   0: brass shine (1 while the hook is out, so it glints as it fires),
//      unused, unused, seed
//   1: direction the sunlight travels xyz, unused
//   2: sun colour rgb, unused
//   3: ambient rgb, unused

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) local: vec3<f32>,
    @location(3) local_normal: vec3<f32>,
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
    out.local_normal = v.normal;
    return out;
}

fn hash3(q: vec3<f32>) -> f32 {
    return fract(sin(dot(q, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

// Smooth value noise, for moss and the wood's figure.
fn noise(q: vec3<f32>) -> f32 {
    let i = floor(q);
    let f = fract(q);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash3(i), hash3(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let b = mix(hash3(i + vec3<f32>(0.0, 1.0, 0.0)), hash3(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let c = mix(hash3(i + vec3<f32>(0.0, 0.0, 1.0)), hash3(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let d = mix(hash3(i + vec3<f32>(0.0, 1.0, 1.0)), hash3(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let q = in.local;
    let n = normalize(in.normal + vec3<f32>(0.0, 0.00001, 0.0));
    let view = normalize(bri_frame.camera.xyz - in.world);
    let sun = normalize(p[1].xyz + vec3<f32>(0.0, -0.00001, 0.0));

    // Hardwood: rings of grain along the barrel, warped by the figure.
    let figure = noise(q * vec3<f32>(6.0, 1.5, 6.0));
    let rings = fract((length(q.xz) * 26.0) + figure * 2.5 + q.y * 1.5);
    let grain = smoothstep(0.0, 0.25, rings) * smoothstep(1.0, 0.55, rings);
    var colour = mix(vec3<f32>(0.16, 0.08, 0.035), vec3<f32>(0.36, 0.2, 0.09), grain * 0.7 + figure * 0.3);
    var gloss = 0.25;

    // Bamboo bands round the barrel, each with its node ridge.
    let band_at = fract(q.y * 2.4 + 0.3);
    let band = smoothstep(0.0, 0.03, band_at) * smoothstep(0.2, 0.17, band_at);
    let node = smoothstep(0.02, 0.0, abs(band_at - 0.1));
    let bamboo = mix(vec3<f32>(0.62, 0.55, 0.26), vec3<f32>(0.8, 0.72, 0.4), noise(q * 40.0));
    colour = mix(colour, bamboo * (1.0 - 0.45 * node), band);
    gloss = mix(gloss, 0.5, band);

    // A vine winding round it, with a leaf every so often along it.
    let around = atan2(q.z, q.x) / 6.2831853;
    let coil = fract(around + q.y * 1.7);
    let vine = smoothstep(0.05, 0.02, abs(coil - 0.5));
    let leaf_at = fract(q.y * 5.0);
    let leaf = smoothstep(0.16, 0.08, length(vec2<f32>((coil - 0.56) * 2.2, (leaf_at - 0.5) * 0.9)))
        * step(0.5, hash3(floor(vec3<f32>(q.y * 5.0, 1.0, 2.0))));
    let green = mix(vec3<f32>(0.08, 0.22, 0.05), vec3<f32>(0.2, 0.46, 0.1), noise(q * 30.0));
    colour = mix(colour, green, max(vine, leaf) * (1.0 - band * 0.6));

    // Moss on whatever faces up, in patches.
    let upward = max(n.y, 0.0);
    let moss = smoothstep(0.55, 0.75, noise(q * 9.0 + vec3<f32>(p[0].w)) * upward);
    colour = mix(colour, mix(vec3<f32>(0.14, 0.26, 0.06), vec3<f32>(0.26, 0.38, 0.1), noise(q * 60.0)), moss);
    gloss = mix(gloss, 0.05, moss);

    // Brass pins in rows along the barrel.
    let pin_cell = vec2<f32>(fract(around * 6.0), fract(q.y * 4.8));
    let pin = smoothstep(0.1, 0.06, length(pin_cell - vec2<f32>(0.5, 0.5)) * 1.0) * (1.0 - band) * (1.0 - vine);
    colour = mix(colour, vec3<f32>(0.85, 0.62, 0.25), pin);
    gloss = mix(gloss, 0.9 + 0.1 * p[0].x, pin);

    let lit = colour * (p[3].rgb * 1.15 + p[2].rgb * max(dot(n, -sun), 0.0));
    let spec = pow(max(dot(reflect(sun, n), view), 0.0), mix(12.0, 50.0, pin)) * gloss * p[2].rgb * 0.6;
    return vec4<f32>(lit + spec, 1.0);
}
