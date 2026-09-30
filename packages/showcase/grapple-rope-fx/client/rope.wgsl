// Grapple Rope: a thick braided rope, three twisted strands of hemp with
// a green vine strand laid in, as if twisted by hand in a jungle.
//
// The mesh is a grid: uv.x runs round the rope, uv.y along it. The vertex
// shader lays it along the rope's path from the gun's muzzle to the hook:
// straight when the rope is taut, sagging in a curve when it is slack,
// with a shiver that dies away when it snaps tight. It is a real tube, so
// it lights and hides behind things like the world does.
//   0: muzzle xyz, radius
//   1: hook end xyz, sag (units the middle hangs below the straight line)
//   2: direction the sunlight travels xyz times its strength, shiver
//      (sideways, units, at the middle)
//   3: ambient rgb, how far the strands have run along (reeling)

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Round the rope (0..1) and along it, in units.
    @location(2) braid: vec2<f32>,
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
    // Sideways for the shiver: level and across the rope.
    var side = cross(axis, vec3<f32>(0.0, 1.0, 0.0));
    if (length(side) < 0.05) {
        side = vec3<f32>(1.0, 0.0, 0.0);
    }
    side = normalize(side);
    let s = v.uv.y;
    // A sagging rope hangs in a curve close to a parabola; it shivers in
    // its first mode, the middle moving most.
    let hang = 4.0 * s * (1.0 - s);
    let centre = start + span * s - vec3<f32>(0.0, sag * hang, 0.0) + side * (shiver * sin(3.14159 * s));
    let slope = span - vec3<f32>(0.0, sag * 4.0 * (1.0 - 2.0 * s), 0.0)
        + side * (shiver * 3.14159 * cos(3.14159 * s));
    let t = normalize(slope + axis * 0.0001);
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(t.y) > 0.95) {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let a = normalize(cross(t, up));
    let b = cross(t, a);
    let turn = v.uv.x * TAU;
    let n = a * cos(turn) + b * sin(turn);
    // Along the rope, in units, for the braid's pitch.
    let along = s * (reach + sag * 2.6) + p[3].w;
    // The strands stand proud of the grooves between them.
    let strands = fract(v.uv.x * 3.0 - along * 5.0);
    let bulge = 1.0 + 0.16 * sin(strands * 3.14159);
    let world = centre + n * p[0].w * bulge;
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = n;
    out.braid = vec2<f32>(v.uv.x, along);
    return out;
}

fn hash(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453);
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let n = normalize(in.normal + vec3<f32>(0.0, 0.00001, 0.0));
    let view = normalize(bri_frame.camera.xyz - in.world);
    // Three strands twisting one way; which strand this is, and where
    // across it.
    let twist = in.braid.x * 3.0 - in.braid.y * 5.0;
    let across = fract(twist);
    let strand = floor(twist) - 3.0 * floor(floor(twist) / 3.0);
    // Round across each strand: dark in the grooves, bright on the crown.
    let crown = sin(across * 3.14159);
    // Fibres run along each strand, twisted the other way inside it.
    let fibre_coord = across * 10.0 + in.braid.y * 24.0;
    let fibre = 0.7 + 0.3 * hash(floor(fibre_coord) + strand * 17.0)
        * (0.5 + 0.5 * sin(fract(fibre_coord) * 3.14159));
    // Hemp, weathered, with one vine strand laid in.
    let hemp = mix(vec3<f32>(0.46, 0.34, 0.18), vec3<f32>(0.68, 0.54, 0.32), hash(floor(in.braid.y * 1.3) + 3.0) * 0.35 + 0.4);
    var colour = hemp;
    if (strand < 0.5) {
        colour = mix(vec3<f32>(0.2, 0.3, 0.1), vec3<f32>(0.32, 0.44, 0.16), crown);
    }
    colour = colour * fibre * mix(0.3, 1.0, pow(crown, 0.6));
    let sun = p[2].xyz;
    let strength = length(sun);
    let toward_sun = -sun / max(strength, 0.0001);
    let diffuse = max(dot(n, toward_sun), 0.0) * strength;
    // A little sheen on the crowns, as waxed rope has.
    let spec = pow(max(dot(reflect(-toward_sun, n), view), 0.0), 18.0) * 0.18 * crown * strength;
    let lit = colour * (p[3].rgb * 1.1 + vec3<f32>(1.0, 0.95, 0.85) * diffuse) + vec3<f32>(spec);
    return vec4<f32>(lit, 1.0);
}
