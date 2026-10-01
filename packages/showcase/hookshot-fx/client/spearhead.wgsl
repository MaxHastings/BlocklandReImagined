// HookShot: a temple spearhead, a flat leaf blade of old gold-bronze on a
// round socket wound with gold filigree and set with a teal stone, and two
// barbed side claws that fold flat in flight and spring out to bite.
//
// Drawn three times with the same tube grid (uv.x round, uv.y along): the
// socket and blade, then each claw. The vertex shader bends the tube into
// that part: the socket runs from the chain's ring to the blade, which
// swells into a flattened leaf and narrows to its point; each claw leaves
// the socket's neck and sweeps back and out like a fish-hook barb.
//   0: the point (the spearhead's tip) xyz, size (1 is about 0.7 long)
//   1: the direction it points xyz, part (0 the blade, 1 and 2 a claw)
//   2: direction the sunlight travels xyz times its strength, turn of the
//      blade's flat round its length (radians)
//   3: ambient rgb, how far the claws are open (0 folded, 1 open)

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Along the part (0..1), the part, round it (0..1).
    @location(2) along: vec3<f32>,
};

const TAU: f32 = 6.2831853;

fn bezier(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, s: f32) -> vec3<f32> {
    let r = 1.0 - s;
    return a * (r * r) + b * (2.0 * r * s) + c * (s * s);
}

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let p = bri_draw.params;
    let tip = p[0].xyz;
    let size = p[0].w;
    let d = normalize(p[1].xyz + vec3<f32>(0.0, 0.0, 0.0001));
    let part = p[1].w;
    let open = p[3].w;
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(d.y) > 0.95) {
        up = vec3<f32>(1.0, 0.0, 0.0);
    }
    let r1 = normalize(cross(d, up));
    let r2 = cross(d, r1);
    // The blade's flat lies along `flat`; the claws stick out of its edges.
    let spin = p[2].w;
    let flat = r1 * cos(spin) + r2 * sin(spin);
    let thin = cross(d, flat);
    let s = v.uv.y;
    let turn = v.uv.x * TAU;
    var world: vec3<f32>;
    var n: vec3<f32>;
    if (part < 0.5) {
        let back = tip - d * (0.7 * size);
        let centre = mix(back, tip, s);
        // The socket (round, a ring at its back and a collar at its neck),
        // then the leaf: widest a third of the way up, to a point.
        var wide: f32;
        var squash: f32;
        if (s < 0.38) {
            let ring = 0.018 * (1.0 - smoothstep(0.0, 0.06, s));
            let collar = 0.016 * smoothstep(0.3, 0.34, s) * (1.0 - smoothstep(0.36, 0.38, s));
            wide = 0.045 + ring + collar;
            squash = 1.0;
        } else {
            let leaf = (s - 0.38) / 0.62;
            wide = mix(0.035, 0.14, sin(min(leaf * 1.7, 1.0) * 1.5708)) * pow(1.0 - leaf, 0.7) + 0.003;
            squash = mix(0.6, 0.18, smoothstep(0.0, 0.25, leaf));
        }
        let ca = cos(turn);
        let sa = sin(turn);
        world = centre + (flat * ca * wide + thin * sa * wide * squash) * size;
        // The flattened ellipse's normal.
        n = normalize(flat * ca * squash + thin * sa);
    } else {
        let side = select(-1.0, 1.0, part > 1.5);
        let out = flat * side;
        // Folded, the claws lie back along the socket; open, they spread
        // out and hook back.
        let a = tip - d * (0.42 * size) + out * (0.03 * size);
        let b = tip - d * (0.5 * size) + out * (mix(0.06, 0.16, open) * size);
        let c = tip - d * (mix(0.66, 0.6, open) * size) + out * (mix(0.07, 0.26, open) * size);
        let centre = bezier(a, b, c, s);
        let tangent = normalize(bezier(a, b, c, min(s + 0.02, 1.0)) - bezier(a, b, c, max(s - 0.02, 0.0)));
        var ref_up = thin;
        if (abs(dot(tangent, ref_up)) > 0.95) {
            ref_up = d;
        }
        let e1 = normalize(cross(tangent, ref_up));
        let e2 = cross(tangent, e1);
        n = e1 * cos(turn) + e2 * sin(turn);
        let radius = size * mix(0.03, 0.004, s * s);
        world = centre + n * radius;
    }
    var o: Varyings;
    o.clip = bri_frame.view_proj * vec4<f32>(world, 1.0);
    o.world = world;
    o.normal = n;
    o.along = vec3<f32>(s, part, v.uv.x);
    return o;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let p = bri_draw.params;
    let n = normalize(in.normal + vec3<f32>(0.0, 0.00001, 0.0));
    let view = normalize(bri_frame.camera.xyz - in.world);
    let sun = p[2].xyz;
    let strength = length(sun);
    let toward_sun = -sun / max(strength, 0.0001);
    // Old gold-bronze, darkest in the hollows, honed bright at the edges.
    let bronze = vec3<f32>(0.5, 0.34, 0.14);
    let gold = vec3<f32>(0.95, 0.74, 0.32);
    var base = bronze;
    var gloss = 0.6;
    var glow = 0.0;
    let s = in.along.x;
    if (in.along.y < 0.5) {
        if (s < 0.38) {
            // Gold filigree wound round the socket in a double spiral.
            let spiral = abs(fract(in.along.z * 2.0 + s * 9.0) - 0.5);
            let counter = abs(fract(in.along.z * 2.0 - s * 9.0) - 0.5);
            let thread = smoothstep(0.08, 0.03, min(spiral, counter));
            base = mix(vec3<f32>(0.3, 0.2, 0.09), gold, thread);
            gloss = mix(0.4, 1.0, thread);
            // The teal stone set in each face of the collar.
            let stone = smoothstep(0.08, 0.04,
                length(vec2<f32>((s - 0.3) * 4.0, abs(fract(in.along.z * 2.0) - 0.5) * 0.6)));
            base = mix(base, vec3<f32>(0.1, 0.7, 0.62), stone);
            glow = stone;
        } else {
            // The blade: a raised midrib, honed edges, verdigris in the
            // grooves beside the rib.
            let across = abs(fract(in.along.z * 2.0) - 0.5) * 2.0;
            let edge = smoothstep(0.75, 0.95, abs(cos(in.along.z * TAU)));
            let groove = smoothstep(0.18, 0.1, abs(across - 0.25)) * (1.0 - smoothstep(0.85, 1.0, s));
            base = mix(bronze, gold, edge);
            base = mix(base, vec3<f32>(0.22, 0.5, 0.42), groove * 0.7);
            gloss = mix(0.6, 1.0, edge) * (1.0 - groove * 0.6);
        }
    } else {
        // Claws: dark bronze, honed bright to their points.
        let honed = smoothstep(0.5, 0.95, s);
        base = mix(vec3<f32>(0.3, 0.2, 0.09), gold, honed);
        gloss = mix(0.4, 1.0, honed);
    }
    let diffuse = max(dot(n, toward_sun), 0.0) * strength;
    let half_vec = normalize(toward_sun + view);
    let spec = pow(max(dot(n, half_vec), 0.0), 50.0) * gloss * strength;
    let rim = pow(1.0 - max(dot(n, view), 0.0), 3.0) * 0.3 * gloss;
    let lit = base * (p[3].rgb * 1.1 + vec3<f32>(1.0, 0.95, 0.85) * diffuse)
        + vec3<f32>(1.0, 0.88, 0.6) * spec + p[3].rgb * rim
        + vec3<f32>(0.1, 0.8, 0.7) * glow * 0.35;
    return vec4<f32>(lit, 1.0);
}
