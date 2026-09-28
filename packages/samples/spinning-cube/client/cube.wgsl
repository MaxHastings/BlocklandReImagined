// Spinning Cube's shader. The engine supplies bri_frame (camera and time),
// bri_draw (this draw's model matrix and material params) and BriVertex.

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

// Turn about the vertical axis, then tilt towards the camera.
fn spin(p: vec3<f32>, angle: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    let turned = vec3<f32>(c * p.x + s * p.z, p.y, -s * p.x + c * p.z);
    let ct = cos(0.35);
    let st = sin(0.35);
    return vec3<f32>(turned.x, ct * turned.y - st * turned.z, st * turned.y + ct * turned.z);
}

@vertex
fn vs_main(v: BriVertex) -> Varyings {
    let angle = bri_frame.time.x * 0.8;
    var out: Varyings;
    out.clip = bri_frame.view_proj * (bri_draw.model * vec4<f32>(spin(v.position, angle), 1.0));
    out.normal = normalize((bri_draw.model * vec4<f32>(spin(v.normal, angle), 0.0)).xyz);
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let t = bri_frame.time.x;
    let tint = bri_draw.params[0].rgb;
    // Diagonal bands flowing across each face.
    let band = 0.5 + 0.5 * sin((in.uv.x + in.uv.y) * 10.0 - t * 4.0);
    // Rings pulsing out from each face's centre. The loop is bounded by
    // the engine as well as by its own condition.
    var rings = 0.0;
    for (var i = 0; i < 3; i++) {
        let r = length(in.uv - vec2<f32>(0.5)) * 2.5 - f32(i) * 0.33 - t * 1.5;
        rings += 0.3 * smoothstep(0.15, 0.0, abs(fract(r) - 0.5));
    }
    let light = max(dot(in.normal, normalize(vec3<f32>(0.4, 0.8, 0.5))), 0.0) * 0.7 + 0.3;
    let colour = mix(tint * 0.2, tint, band) + vec3<f32>(rings);
    return vec4<f32>(colour * light, 1.0);
}
