// HookShot: a chain of oval links, cast bronze gone dark with age, worn
// bright where the links rub and green with verdigris in their crooks, as
// if it had hung in a jungle temple for a thousand years.
//
// The mesh is a run of 32 links: uv.x goes round the wire, uv.y along the
// run, 14 rows a link. The first and last row of each link sit on its
// neighbours and are cut away, so each link is its own closed loop; the
// twelve rows between go once round the oval. The vertex shader strings
// the links along the chain's path from the launcher (or the hands) to the
// spearhead, each turned a quarter from the last so they interlock. A long
// chain is drawn as several runs, each starting `first` links in.
//   0: start xyz, pitch (units from one link to the next)
//   1: spearhead end xyz, sag (units the middle hangs below the line)
//   2: direction the sunlight travels xyz times its strength, the first
//      link of this run
//   3: ambient rgb, how far the links have run along (reeling)

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Round the wire (cos), inside or outside the loop (-1..1), and
    // whether this is the cut-away joint between links (1) or kept (0).
    @location(2) wear: vec3<f32>,
};

const TAU: f32 = 6.2831853;
const ROWS: f32 = 14.0;
const LINKS: f32 = 32.0;

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let p = bri_draw.params;
    let start = p[0].xyz;
    let pitch = p[0].w;
    let end = p[1].xyz;
    let sag = p[1].w;
    let span = end - start;
    let reach = max(length(span), 0.001);
    let axis = span / reach;
    let row = round(v.uv.y * (LINKS * ROWS - 1.0));
    let k = floor(row / ROWS);
    let r = row - k * ROWS;
    // Which link along the whole chain, and where it is: links run along
    // as the chain reels, so whole links come and go at the ends.
    let run = p[3].w;
    let shift = run / pitch;
    let link = p[2].w + k - floor(shift);
    let along = (p[2].w + k - 0.5 + fract(shift)) * pitch;
    let s = clamp(along / reach, 0.0, 1.0);
    let hang = 4.0 * s * (1.0 - s);
    let centre = start + span * s - vec3<f32>(0.0, sag * hang, 0.0);
    let slope = span - vec3<f32>(0.0, sag * 4.0 * (1.0 - 2.0 * s), 0.0);
    let t = normalize(slope + axis * 0.0001);
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(t.y) > 0.95) {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let e1 = normalize(cross(t, up));
    let e2 = cross(t, e1);
    // Every other link is turned a quarter, so they hang through each
    // other.
    let odd = fract(link * 0.5) > 0.25;
    var w = e1;
    var plane = e2;
    if (odd) {
        w = e2;
        plane = -e1;
    }
    // Round the oval once over rows 1..12; rows 0 and 13 sit on rows 1
    // and 12 (the cut-away joints).
    let around = clamp(r - 1.0, 0.0, ROWS - 3.0) / (ROWS - 3.0);
    let theta = around * TAU;
    let wire = pitch * 0.17;
    let half_long = pitch * 0.5 + wire;
    let half_wide = pitch * 0.3 + wire;
    let loop_point = t * (cos(theta) * half_long) + w * (sin(theta) * half_wide);
    let outward = normalize(t * (cos(theta) * half_wide) + w * (sin(theta) * half_long));
    let turn = v.uv.x * TAU;
    let n = outward * cos(turn) + plane * sin(turn);
    // Links past either end are not there (yet).
    var keep = 1.0;
    if (along < 0.0 || along > reach) {
        keep = 0.0;
    }
    let world = centre + (loop_point + n * wire) * keep;
    var out: Varyings;
    out.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = n;
    var joint = 0.0;
    if (r < 0.5 || r > ROWS - 1.5 || keep < 0.5) {
        joint = 1.0;
    }
    out.wear = vec3<f32>(cos(turn), cos(theta), joint);
    return out;
}

fn hash3(q: vec3<f32>) -> f32 {
    return fract(sin(dot(q, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    if (in.wear.z > 0.01) {
        discard;
    }
    let p = bri_draw.params;
    let n = normalize(in.normal + vec3<f32>(0.0, 0.00001, 0.0));
    let view = normalize(bri_frame.camera.xyz - in.world);
    // Dark old bronze; rubbed bright where the links bear on each other
    // (the ends of the oval, inside); verdigris in the crooks.
    let grain = hash3(floor(in.world * 60.0));
    let dark = mix(vec3<f32>(0.24, 0.16, 0.08), vec3<f32>(0.32, 0.22, 0.11), grain);
    let bright = vec3<f32>(0.86, 0.62, 0.3);
    let rub = smoothstep(0.75, 0.95, abs(in.wear.y)) * smoothstep(0.2, -0.6, in.wear.x);
    let crook = smoothstep(0.2, 0.9, -in.wear.x) * (1.0 - smoothstep(0.6, 0.9, abs(in.wear.y)));
    let patina = vec3<f32>(0.22, 0.5, 0.42) * (0.8 + 0.4 * grain);
    var colour = mix(dark, bright, rub);
    colour = mix(colour, patina, crook * 0.85);
    let gloss = mix(0.35, 1.0, rub) * (1.0 - crook * 0.8);
    let sun = p[2].xyz;
    let strength = length(sun);
    let toward_sun = -sun / max(strength, 0.0001);
    let diffuse = max(dot(n, toward_sun), 0.0) * strength;
    let half_vec = normalize(toward_sun + view);
    let spec = pow(max(dot(n, half_vec), 0.0), 40.0) * gloss * strength;
    let lit = colour * (p[3].rgb * 1.1 + vec3<f32>(1.0, 0.95, 0.85) * diffuse)
        + vec3<f32>(1.0, 0.85, 0.6) * spec * 0.8;
    return vec4<f32>(lit, 1.0);
}
