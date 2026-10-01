// HookShot skin: the Printer the launcher is built from, recast as a
// temple relic: weathered bronze gone green with verdigris in every
// hollow, carved glyph rings round the barrel, bands of gold filigree,
// and a teal eye-stone on top that wakes and glows while the chain is out.
//
// Drawn over the game's own Printer with its own model (`image_mesh`),
// puffed out a hair along its normals so it covers it exactly. Torque
// models point along +y, so the rings and bands go round y.
//   0: the stone (1 while the chain is out), time, unused, seed
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

// Smooth value noise, for the patina's blotches.
fn blot(q: vec3<f32>) -> f32 {
    let i = floor(q);
    let f = fract(q);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(hash3(i), hash3(i + vec3<f32>(1.0, 0.0, 0.0)), u.x),
                mix(hash3(i + vec3<f32>(0.0, 1.0, 0.0)), hash3(i + vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(hash3(i + vec3<f32>(0.0, 0.0, 1.0)), hash3(i + vec3<f32>(1.0, 0.0, 1.0)), u.x),
                mix(hash3(i + vec3<f32>(0.0, 1.0, 1.0)), hash3(i + vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let q = in.local;
    let n = normalize(in.normal + vec3<f32>(0.0, 0.00001, 0.0));
    let view = normalize(bri_frame.camera.xyz - in.world);
    let sun = normalize(p[1].xyz + vec3<f32>(0.0, -0.00001, 0.0));
    let around = atan2(q.z, q.x) / 6.2831853;

    // Cast bronze, mottled where the green has eaten in.
    let mottle = blot(q * 9.0) * 0.6 + blot(q * 23.0) * 0.4;
    var colour = mix(vec3<f32>(0.34, 0.22, 0.09), vec3<f32>(0.5, 0.34, 0.15), hash3(floor(q * 70.0)) * 0.4 + 0.3);
    var gloss = 0.55;
    let patina = smoothstep(0.5, 0.75, mottle);
    colour = mix(colour, vec3<f32>(0.25, 0.52, 0.43), patina);
    gloss = mix(gloss, 0.1, patina);

    // Carved glyph rings: a groove each side, and a row of blocky glyphs
    // between, green in their cuts.
    let ring_at = fract(q.y * 1.4 + 0.2);
    let ring = smoothstep(0.0, 0.02, ring_at) * smoothstep(0.2, 0.18, ring_at);
    let grooves = smoothstep(0.025, 0.0, abs(ring_at - 0.02)) + smoothstep(0.025, 0.0, abs(ring_at - 0.18));
    let cell = vec2<f32>(fract(around * 12.0), (ring_at - 0.04) / 0.12);
    let glyph_id = hash3(vec3<f32>(floor(around * 12.0), floor(q.y * 1.4 + 0.2), 3.0));
    let bar_h = step(abs(cell.y - 0.5), 0.08) * step(abs(cell.x - 0.5), 0.3);
    let bar_v = step(abs(cell.x - mix(0.3, 0.7, step(0.5, glyph_id))), 0.07) * step(abs(cell.y - 0.5), 0.35);
    let dot_g = step(length(cell - vec2<f32>(0.5, mix(0.2, 0.8, fract(glyph_id * 7.0)))), 0.1);
    let glyph = clamp(bar_h + bar_v + dot_g, 0.0, 1.0) * ring;
    colour = mix(colour, vec3<f32>(0.12, 0.32, 0.27), clamp(glyph + grooves, 0.0, 1.0) * 0.85);

    // Gold filigree bands, a wave wound round between the rings.
    let band_at = fract(q.y * 1.4 + 0.7);
    let band = smoothstep(0.0, 0.015, band_at) * smoothstep(0.12, 0.105, band_at);
    let wave = abs(band_at - 0.06 - 0.035 * sin(around * 6.2831853 * 10.0));
    let thread = smoothstep(0.012, 0.004, wave) * band;
    let edges = smoothstep(0.012, 0.0, abs(band_at - 0.006)) + smoothstep(0.012, 0.0, abs(band_at - 0.114));
    let gilt = clamp(thread + edges * band, 0.0, 1.0);
    colour = mix(colour, vec3<f32>(0.95, 0.75, 0.32), gilt);
    gloss = mix(gloss, 1.0, gilt);

    // The eye-stone on top, teal, waking while the chain is out.
    let stone_cell = vec2<f32>(q.x * 7.0, fract(q.y * 0.8) - 0.5);
    let stone = smoothstep(0.38, 0.28, length(stone_cell)) * step(0.3, n.y);
    let setting = smoothstep(0.48, 0.38, length(stone_cell)) * step(0.3, n.y) * (1.0 - stone);
    let glow = p[0].x * (0.65 + 0.35 * sin(p[0].y * 4.0));
    colour = mix(colour, vec3<f32>(0.95, 0.75, 0.32), setting);
    colour = mix(colour, vec3<f32>(0.08, 0.55, 0.5) * (0.5 + glow), stone);
    gloss = mix(gloss, 1.0, stone + setting);

    let lit = colour * (p[3].rgb * 1.15 + p[2].rgb * max(dot(n, -sun), 0.0));
    let spec = pow(max(dot(reflect(sun, n), view), 0.0), 36.0) * gloss * p[2].rgb * 0.7;
    let emit = vec3<f32>(0.2, 1.0, 0.85) * stone * glow * 0.8;
    return vec4<f32>(lit + spec + emit, 1.0);
}
