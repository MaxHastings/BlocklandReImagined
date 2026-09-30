// Gravity Gun sparks: a whole particle system in one draw. The mesh is a
// batch of quads; each quad's corner is in uv (-1..1) and its own random
// numbers are in its normal (and position.y). The vertex shader moves
// each particle through its life and turns it to face the camera, so the
// game only ever sends four numbers per effect.
//   0: centre xyz, spread
//   1: mode, age or charge, share of particles shown 0..1, size
//   2: direction xyz, speed
//   3: colour rgb, alpha
// Modes: 0 sparks orbiting a held object, 1 sparks drawn into a point,
// 2 a burst thrown out along the direction, 3 one glowing orb (where the
// beam grips, and at the muzzle).

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) fade: f32,
};

const TAU: f32 = 6.2831853;

fn unit(seed: vec3<f32>) -> vec3<f32> {
    let z = seed.x * 2.0 - 1.0;
    let a = seed.y * TAU;
    let r = sqrt(max(1.0 - z * z, 0.0));
    return vec3<f32>(r * cos(a), z, r * sin(a));
}

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let p = bri_draw.params;
    let t = bri_frame.time.x;
    let seed = v.normal;
    let index = v.position.x;
    let mode = p[1].x;
    let age = p[1].y;
    let spread = p[0].w;
    var at = p[0].xyz;
    var size = p[1].w * (0.6 + 0.8 * seed.z);
    var fade = 1.0;
    if (mode < 0.5) {
        // Orbiting: each spark circles on its own tilted ring.
        let axis = unit(seed);
        let side = normalize(cross(axis, vec3<f32>(0.3, 1.0, 0.2)));
        let other = cross(axis, side);
        let a = t * (2.0 + 3.0 * seed.y) + seed.x * TAU;
        at += (side * cos(a) + other * sin(a)) * spread * (0.9 + 0.3 * seed.z);
        fade = 0.6 + 0.4 * sin(t * 9.0 + seed.y * TAU);
    } else if (mode < 1.5) {
        // Drawn in: from a shell round the muzzle to its centre, looping.
        let life = fract(t * (1.2 + seed.z) + seed.x);
        at += unit(seed) * spread * (1.0 - life) * (1.0 - life);
        fade = life * age;
        size *= 0.5 + age;
    } else if (mode < 2.5) {
        // Burst: thrown out in a cone along the direction, falling, fading.
        let dir = normalize(p[2].xyz + unit(seed) * 0.9);
        let speed = p[2].w * (0.35 + 0.9 * seed.z);
        at += dir * speed * age + vec3<f32>(0.0, -4.0 * age * age, 0.0) + unit(seed.zyx) * spread * 0.3;
        fade = clamp(1.0 - age / (0.35 + 0.4 * seed.x), 0.0, 1.0);
    } else {
        // One orb: only the first particle, pulsing.
        size = select(0.0, p[1].w * (1.0 + 0.15 * sin(t * 30.0)), index < 0.001);
        fade = 1.0;
    }
    // Only a share of the batch shows, so small effects cost little.
    if (index >= p[1].z) {
        size = 0.0;
    }
    let view = normalize(bri_frame.camera.xyz - at);
    let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), view) + vec3<f32>(0.0001, 0.0, 0.0));
    let up = cross(view, right);
    let world = at + (right * v.uv.x + up * v.uv.y) * size;
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    out.corner = v.uv;
    out.fade = fade;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let d = length(in.corner);
    let glow = pow(clamp(1.0 - d, 0.0, 1.0), 2.0);
    let hot = pow(clamp(1.0 - d * 2.2, 0.0, 1.0), 2.0);
    let colour = mix(p[3].rgb, vec3<f32>(1.0, 0.97, 0.9), hot);
    return vec4<f32>(colour, clamp((glow + hot) * in.fade * p[3].a, 0.0, 1.0));
}
