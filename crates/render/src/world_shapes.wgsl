struct Camera {
    view_projection: mat4x4<f32>,
    eye: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};
@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) outside: vec4<f32>,
    @location(3) inside: vec4<f32>,
) -> VertexOut {
    var out: VertexOut;
    out.position = camera.view_projection * vec4<f32>(position, 1.0);
    // The same for the whole face: the eye is on one side of its plane.
    out.color = select(inside, outside, dot(normal, camera.eye.xyz - position) >= 0.0);
    return out;
}
@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    if (in.color.a <= 0.0) {
        discard;
    }
    return vec4<f32>(output_color(in.color.rgb), in.color.a);
}
