@group(0) @binding(0) var<uniform> view_projection: mat4x4<f32>;
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
};
@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) color: vec3<f32>) -> VertexOut {
    var out: VertexOut;
    out.position = view_projection * vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}
@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(output_color(in.color), 1.0);
}
