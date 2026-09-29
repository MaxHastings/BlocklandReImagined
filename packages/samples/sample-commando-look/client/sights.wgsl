// Commando Look's shader: one shader, two uses, picked by params[1].x.
//
//   0: a lit box, params[0] its colour. The rifle held in first person is
//      a few of these, drawn in view space (the camera at the origin,
//      looking down -z), so light comes from a fixed direction beside the
//      eye.
//   1: the scope, a quad over the whole screen drawn in screen space.
//      params[1].y is the screen's aspect (width / height). Inside the
//      lens it draws thin crosshairs and lets the world show through;
//      outside it is black.

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) local: vec2<f32>,
};

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    var out: Varyings;
    out.clip = bri_frame.view_proj * (bri_draw.model * vec4<f32>(v.position, 1.0));
    out.normal = normalize((bri_draw.model * vec4<f32>(v.normal, 0.0)).xyz);
    out.local = v.position.xy;
    return out;
}

fn lit_box(in: Varyings) -> vec4<f32> {
    let light = max(dot(in.normal, normalize(vec3<f32>(0.35, 0.8, 0.45))), 0.0) * 0.65 + 0.35;
    return vec4<f32>(bri_draw.params[0].rgb * light, 1.0);
}

fn scope(in: Varyings) -> vec4<f32> {
    // The quad's own -1 to 1 is stretched across the screen's width, so
    // scale x back by the aspect to keep the lens round.
    let p = vec2<f32>(in.local.x * bri_draw.params[1].y, in.local.y);
    let r = length(p);
    let lens = 0.92;
    if (r < lens) {
        let line = 0.0035;
        let gap = 0.05;
        let cross = (abs(p.x) < line && abs(p.y) > gap) || (abs(p.y) < line && abs(p.x) > gap);
        if (!cross) {
            discard;
        }
        return vec4<f32>(0.02, 0.02, 0.02, 1.0);
    }
    // A soft dark rim, then black.
    let rim = smoothstep(lens, lens + 0.04, r);
    return vec4<f32>(0.0, 0.0, 0.0, mix(0.85, 1.0, rim));
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    if (bri_draw.params[1].x > 0.5) {
        return scope(in);
    }
    return lit_box(in);
}
