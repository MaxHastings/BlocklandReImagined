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
// beam grips, and at the muzzle), 4 a ring thrown out round the direction
// (age 0..1), 5 motes rising from under a held object.
// Sparks are little turning squares, as blocky as the bricks; only the
// orb is round.

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) fade: f32,
    @location(2) square: f32,
};

const TAU: f32 = 6.2831853;

fn unit(seed: vec3<f32>) -> vec3<f32> {
    let z = seed.x * 2.0 - 1.0;
    let a = seed.y * TAU;
    let r = sqrt(max(1.0 - z * z, 0.0));
    return vec3<f32>(r * cos(a), z, r * sin(a));
}

// Two directions square to `axis`.
fn across(axis: vec3<f32>) -> mat2x3<f32> {
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(axis.y) > 0.9) {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let a = normalize(cross(axis, up));
    return mat2x3<f32>(a, cross(axis, a));
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
    var square = 1.0;
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
    } else if (mode < 3.5) {
        // One orb: only the first particle, pulsing.
        size = select(0.0, p[1].w * (1.0 + 0.15 * sin(t * 30.0)), index < 0.001);
        fade = 1.0;
        square = 0.0;
    } else if (mode < 4.5) {
        // A ring: the batch spaced round a circle square to the direction,
        // thrown outward and thinning as it goes.
        let ring = across(normalize(p[2].xyz + vec3<f32>(0.0, 0.0001, 0.0)));
        let a = index / max(p[1].z, 0.01) * TAU + seed.x * 0.08;
        let grow = 1.0 - (1.0 - age) * (1.0 - age);
        at += (ring[0] * cos(a) + ring[1] * sin(a)) * spread * (0.25 + grow * (0.9 + 0.2 * seed.y));
        fade = clamp(1.0 - age, 0.0, 1.0);
        size *= 1.0 - 0.5 * age;
    } else {
        // Rising: motes from a disc under the held thing float up through
        // it and fade, looping, as if it weighed nothing.
        let life = fract(t * (0.35 + 0.3 * seed.z) + seed.x);
        let a = seed.y * TAU;
        let r = spread * sqrt(seed.z) * 0.9;
        at += vec3<f32>(cos(a) * r, (life * 2.0 - 1.1) * spread, sin(a) * r);
        fade = sin(life * 3.14159);
    }
    // Only a share of the batch shows, so small effects cost little.
    if (index >= p[1].z) {
        size = 0.0;
    }
    // Each square turns on its own.
    let spin = seed.y * TAU + t * (seed.z - 0.5) * 6.0 * square;
    let c = vec2<f32>(
        v.uv.x * cos(spin) - v.uv.y * sin(spin),
        v.uv.x * sin(spin) + v.uv.y * cos(spin)
    );
    let view = normalize(bri_frame.camera.xyz - at);
    let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), view) + vec3<f32>(0.0001, 0.0, 0.0));
    let up = cross(view, right);
    let world = at + (right * c.x + up * c.y) * size;
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    out.corner = v.uv;
    out.fade = fade;
    out.square = square;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let disc = length(in.corner);
    let boxy = max(abs(in.corner.x), abs(in.corner.y));
    var glow = pow(clamp(1.0 - disc, 0.0, 1.0), 2.0);
    var hot = pow(clamp(1.0 - disc * 2.2, 0.0, 1.0), 2.0);
    if (in.square > 0.5) {
        // A crisp square with a white-hot middle, in a faint halo.
        glow = smoothstep(0.62, 0.5, boxy) * 0.85 + pow(clamp(1.0 - disc, 0.0, 1.0), 3.0) * 0.35;
        hot = smoothstep(0.32, 0.18, boxy);
    }
    let colour = mix(p[3].rgb, vec3<f32>(1.0, 0.97, 0.9), hot);
    return vec4<f32>(colour, clamp((glow + hot) * in.fade * p[3].a, 0.0, 1.0));
}
