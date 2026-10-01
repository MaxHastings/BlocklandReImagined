// Grappling Hook: a steel wire cable, six bright strands laid round a
// core, as on an expedition winch.
//
// The mesh is a grid: uv.x runs round the cable, uv.y along it. The vertex
// shader lays it along the cable's path from the launcher (or the hands)
// to the grapnel: nearly straight, a winch cable being taut, with a
// shiver that dies away when it bites and takes the load. It is a real
// tube, so it lights and hides behind things like the world does.
//   0: start xyz, radius
//   1: grapnel end xyz, sag (units the middle hangs below the line)
//   2: direction the sunlight travels xyz times its strength, shiver
//      (sideways, units, at the middle)
//   3: ambient rgb, how far the strands have run along (winching)

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Round the cable (0..1) and along it, in units.
    @location(2) lay: vec2<f32>,
    @location(3) tangent: vec3<f32>,
};

const TAU: f32 = 6.2831853;

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let p = bri_draw.params;
    let start = p[0].xyz;
    let end = p[1].xyz;
    let sag = p[1].w;
    let shiver = p[2].w;
    let span = end - start;
    let reach = max(length(span), 0.001);
    let axis = span / reach;
    var side = cross(axis, vec3<f32>(0.0, 1.0, 0.0));
    if (length(side) < 0.05) {
        side = vec3<f32>(1.0, 0.0, 0.0);
    }
    side = normalize(side);
    let s = v.uv.y;
    let hang = 4.0 * s * (1.0 - s);
    // A taut cable shivers in its second mode as well as its first: the
    // quick buzz of wire, not the slow sway of rope.
    let buzz = sin(3.14159 * s) + 0.4 * sin(6.28318 * s);
    let centre = start + span * s - vec3<f32>(0.0, sag * hang, 0.0) + side * (shiver * buzz);
    let slope = span - vec3<f32>(0.0, sag * 4.0 * (1.0 - 2.0 * s), 0.0)
        + side * (shiver * (3.14159 * cos(3.14159 * s) + 2.51327 * cos(6.28318 * s)));
    let t = normalize(slope + axis * 0.0001);
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(t.y) > 0.95) {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let a = normalize(cross(t, up));
    let b = cross(t, a);
    let turn = v.uv.x * TAU;
    let n = a * cos(turn) + b * sin(turn);
    let along = s * reach + p[3].w;
    // Six strands stand proud of the lay.
    let strands = fract(v.uv.x * 6.0 - along * 7.0);
    let bulge = 1.0 + 0.1 * sin(strands * 3.14159);
    let world = centre + n * p[0].w * bulge;
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = n;
    out.lay = vec2<f32>(v.uv.x, along);
    out.tangent = t;
    return out;
}

fn hash(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453);
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let view = normalize(bri_frame.camera.xyz - in.world);
    let lay = in.lay.x * 6.0 - in.lay.y * 7.0;
    let across = fract(lay);
    let strand = floor(lay);
    let crown = sin(across * 3.14159);
    // Each strand is itself wound of fine wires, the other way.
    let wire = 0.8 + 0.2 * sin((across * 7.0 + in.lay.y * 40.0) * 6.28318);
    // Tilt the normal across the strand, so each catches the light as a
    // wound wire does.
    let t = normalize(in.tangent);
    let n0 = normalize(in.normal + vec3<f32>(0.0, 0.00001, 0.0));
    let n = normalize(n0 + t * (across - 0.5) * 0.9);
    // Bright galvanised steel, darker with grease in the lay.
    let steel = mix(vec3<f32>(0.5, 0.52, 0.55), vec3<f32>(0.78, 0.8, 0.82), hash(strand + floor(in.lay.y * 0.7) * 3.0) * 0.3 + 0.6);
    let colour = steel * wire * mix(0.25, 1.0, pow(crown, 0.5));
    let sun = p[2].xyz;
    let strength = length(sun);
    let toward_sun = -sun / max(strength, 0.0001);
    let diffuse = max(dot(n, toward_sun), 0.0) * strength;
    let half_vec = normalize(toward_sun + view);
    let spec = pow(max(dot(n, half_vec), 0.0), 60.0) * 0.9 * crown * strength;
    let lit = colour * (p[3].rgb * 1.1 + vec3<f32>(1.0, 0.97, 0.9) * diffuse * 0.8) + vec3<f32>(spec);
    return vec4<f32>(lit, 1.0);
}
