// Steel Ball: a polished steel sphere that mirrors the sky it rolls under.
//
// The mesh is a flat grid (uv 0..1); the vertex shader wraps it into a
// sphere and turns it with the ball's rotation, so the ball's roll carries
// its seam round with it. Per draw (params):
//   0: centre xyz, radius
//   1: rotation quaternion xyzw
//   2: direction the sunlight travels xyz, sun strength
//   3: sky (fog) colour rgb, unused

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) local: vec3<f32>,
};

const PI: f32 = 3.14159265;

fn rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let theta = v.uv.y * PI;
    let phi = v.uv.x * 2.0 * PI;
    let local = vec3<f32>(sin(theta) * cos(phi), cos(theta), sin(theta) * sin(phi));
    let p = bri_draw.params;
    let q = normalize(p[1]);
    let n = rotate(q, local);
    let world = p[0].xyz + n * p[0].w;
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = n;
    out.local = local;
    return out;
}

// What a mirror sees looking along `r`: the sky's gradient above the
// horizon, darker ground below, the sun as a hard disc with a halo.
fn environment(r: vec3<f32>, sky: vec3<f32>, sun: vec3<f32>, sun_strength: f32) -> vec3<f32> {
    let horizon = sky * 1.08 + vec3<f32>(0.06);
    let zenith = mix(sky, vec3<f32>(0.18, 0.32, 0.62), 0.55);
    let ground = mix(sky * 0.35, vec3<f32>(0.16, 0.15, 0.14), 0.6);
    var colour: vec3<f32>;
    if (r.y >= 0.0) {
        colour = mix(horizon, zenith, pow(r.y, 0.45));
    } else {
        colour = mix(horizon * 0.7, ground, pow(-r.y, 0.35));
    }
    let toward = max(dot(r, -sun), 0.0);
    colour += vec3<f32>(1.0, 0.96, 0.88) * sun_strength * (pow(toward, 900.0) * 9.0 + pow(toward, 48.0) * 0.35);
    return colour;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let n = normalize(in.normal);
    let v = normalize(bri_frame.camera.xyz - in.world);
    let r = reflect(-v, n);
    let sun = normalize(p[2].xyz);
    let reflected = environment(r, p[3].rgb, sun, p[2].w);
    // Steel reflects a lot, a touch more at grazing angles (Schlick).
    let f0 = vec3<f32>(0.62, 0.63, 0.65);
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - max(dot(n, v), 0.0), 5.0);
    var colour = reflected * fresnel;
    // The underside picks up the ground's shade, so the ball sits down.
    colour *= mix(0.55, 1.0, smoothstep(-0.9, 0.2, n.y));
    // A fine machined groove round its equator, turning as it rolls.
    let groove = smoothstep(0.012, 0.004, abs(in.local.y));
    colour = mix(colour, colour * 0.35, groove * 0.8);
    return vec4<f32>(colour, 1.0);
}
